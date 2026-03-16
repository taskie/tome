use std::{
    collections::{BTreeMap, HashMap, HashSet},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Args;
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use tracing::{debug, info, warn};

use tome_core::{
    hash::{self, DigestAlgorithm, FastHashAlgorithm},
    models::EntryStatus,
};
use tome_db::ops;

#[derive(Args)]
pub struct ScanArgs {
    /// Repository name (default: "default")
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,

    /// Do not respect .gitignore / .ignore files
    #[arg(long)]
    pub no_ignore: bool,

    /// Optional message to attach to this snapshot
    #[arg(long, short = 'm', default_value = "")]
    pub message: String,

    /// Digest algorithm for new repositories: sha256 (default) or blake3
    /// Existing repositories use their stored algorithm; this arg is ignored.
    #[arg(long, default_value = "sha256")]
    pub digest_algorithm: DigestAlgorithm,

    /// Fast-digest algorithm for new repositories: xxhash64 (default) or xxh3-64
    /// Existing repositories use their stored algorithm; this arg is ignored.
    #[arg(long, default_value = "xxhash64")]
    pub fast_hash_algorithm: FastHashAlgorithm,

    /// Number of files per DB transaction (default: 1000; -1 = one big commit at the end)
    #[arg(long, default_value = "1000")]
    pub batch_size: i64,

    /// Keep the snapshot even if no files were added, modified, or deleted
    #[arg(long)]
    pub allow_empty: bool,

    /// Directory to scan (default: current directory)
    pub path: Option<PathBuf>,
}

#[derive(Default, Debug)]
struct ScanStats {
    scanned: u64,
    added: u64,
    modified: u64,
    unchanged: u64,
    deleted: u64,
    errors: u64,
}

pub async fn run(db: &DatabaseConnection, args: ScanArgs) -> Result<()> {
    // 1. Get or create repository (needed to resolve scan_root from config).
    let repo = ops::get_or_create_repository(db, &args.repo).await?;

    // 2. Resolve scan root: explicit arg > saved config > current directory.
    let scan_root = match args.path {
        Some(ref p) => p.clone(),
        None => ops::get_repository_scan_root(&repo).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")),
    };
    let scan_root = scan_root.canonicalize().with_context(|| format!("cannot access scan root {:?}", scan_root))?;

    info!("scanning {:?} for repo {:?}", scan_root, args.repo);

    // 2. Find the previous snapshot (for parent chain).
    let parent = ops::latest_snapshot(db, repo.id).await?;
    let parent_id = parent.as_ref().map(|s| s.id);

    // 3. Resolve digest algorithm (set in repo.config on first use).
    let algo = ops::get_or_init_repository_digest_algorithm(db, &repo, args.digest_algorithm).await?;

    // 3b. Resolve fast-hash algorithm (set in repo.config on first use).
    let fast_algo = ops::get_or_init_repository_fast_hash_algorithm(db, &repo, args.fast_hash_algorithm).await?;

    // 4. Create a new snapshot.
    let snapshot = ops::create_snapshot(db, repo.id, parent_id, &args.message).await?;

    // 5. Load entry cache (previous state).
    let mut cache = ops::load_entry_cache(db, repo.id).await?;

    let mut stats = ScanStats::default();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 6. Collect directory entries (errors counted separately to avoid borrow conflict).
    let dir_entries: Vec<ignore::DirEntry> = {
        let mut walk_errors = 0u64;
        let use_ignore = !args.no_ignore;
        // Always exclude .git/ regardless of hidden() setting.
        let overrides = {
            let mut ob = ignore::overrides::OverrideBuilder::new(&scan_root);
            ob.add("!.git").map_err(|e| anyhow::anyhow!("{}", e))?;
            ob.build().map_err(|e| anyhow::anyhow!("{}", e))?
        };
        let entries: Vec<_> = ignore::WalkBuilder::new(&scan_root)
            .hidden(false)
            .git_ignore(use_ignore)
            .git_global(use_ignore)
            .git_exclude(use_ignore)
            .overrides(overrides)
            .sort_by_file_name(|a, b| a.cmp(b))
            .build()
            .filter_map(|e| match e {
                Ok(e) => Some(e),
                Err(err) => {
                    warn!("walk error: {}", err);
                    walk_errors += 1;
                    None
                }
            })
            .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            .collect();
        stats.errors += walk_errors;
        entries
    };

    // 7. Process each file entry.
    // batch_size <= 0 means "one big commit at the end" (usize::MAX never triggers mid-loop commit).
    let effective_batch: usize = if args.batch_size <= 0 { usize::MAX } else { args.batch_size as usize };

    let mut ctx = ScanContext {
        snapshot_id: snapshot.id,
        repository_id: repo.id,
        algo,
        fast_algo,
        cache: &mut cache,
        stats: &mut stats,
    };
    let mut batch_count = 0usize;
    let mut txn = db.begin().await?;

    for dir_entry in dir_entries {
        let abs_path = dir_entry.path();
        let rel_path = match abs_path.strip_prefix(&scan_root) {
            Ok(p) => p.to_string_lossy().into_owned(),
            Err(_) => {
                warn!("could not relativize {:?}", abs_path);
                ctx.stats.errors += 1;
                continue;
            }
        };

        ctx.stats.scanned += 1;
        seen_paths.insert(rel_path.clone());

        match process_file(&txn, &mut ctx, abs_path, &rel_path).await {
            Ok(()) => {}
            Err(e) => {
                warn!("error processing {:?}: {}", rel_path, e);
                ctx.stats.errors += 1;
            }
        }

        batch_count += 1;
        if batch_count >= effective_batch {
            txn.commit().await?;
            txn = db.begin().await?;
            batch_count = 0;
        }
    }

    // 8. Detect deleted files: paths in cache that were not seen in the walk.
    let deleted_paths: Vec<String> = cache
        .keys()
        .filter(|p| {
            let cached = &cache[*p];
            cached.status != EntryStatus::Deleted.as_i16() && !seen_paths.contains(*p)
        })
        .cloned()
        .collect();

    for path in deleted_paths {
        match ops::insert_entry_deleted(&txn, snapshot.id, &path).await {
            Ok(e) => {
                ops::upsert_cache_deleted(&txn, repo.id, &path, snapshot.id, e.id).await?;
                stats.deleted += 1;
                info!("deleted    {}", path);
            }
            Err(e) => {
                warn!("error recording deletion of {:?}: {}", path, e);
                stats.errors += 1;
            }
        }

        batch_count += 1;
        if batch_count >= effective_batch {
            txn.commit().await?;
            txn = db.begin().await?;
            batch_count = 0;
        }
    }

    txn.commit().await?;

    // 9. Compute tree hashes bottom-up and insert directory entries.
    let root_object_id = compute_tree_objects(db, snapshot.id, algo).await?;
    if let Some(root_id) = root_object_id {
        ops::update_snapshot_root_object(db, snapshot.id, root_id).await?;
    }

    // 10. Persist the scan root in the repository config.
    ops::set_repository_scan_root(db, &repo, &scan_root.to_string_lossy()).await?;

    // 11. Discard the snapshot if nothing changed and --allow-empty is not set.
    if stats.added == 0 && stats.modified == 0 && stats.deleted == 0 && !args.allow_empty {
        ops::delete_snapshot_records(db, &[snapshot.id]).await?;
        println!(
            "scan complete: {} scanned, no changes detected (snapshot discarded; use --allow-empty to keep)",
            stats.scanned
        );
        return Ok(());
    }

    // 12. Update snapshot metadata with scan statistics.
    let metadata = tome_core::metadata::ScanMetadata {
        scan_root: scan_root.to_string_lossy().into_owned(),
        scanned: stats.scanned,
        added: stats.added,
        modified: stats.modified,
        unchanged: stats.unchanged,
        deleted: stats.deleted,
        errors: stats.errors,
    };
    ops::update_snapshot_metadata(db, snapshot.id, serde_json::to_value(metadata)?).await?;

    println!(
        "scan complete: {} scanned, {} added, {} modified, {} unchanged, {} deleted, {} errors",
        stats.scanned, stats.added, stats.modified, stats.unchanged, stats.deleted, stats.errors
    );

    Ok(())
}

struct ScanContext<'a> {
    snapshot_id: i64,
    repository_id: i64,
    algo: DigestAlgorithm,
    fast_algo: FastHashAlgorithm,
    cache: &'a mut std::collections::HashMap<String, tome_db::entities::entry_cache::Model>,
    stats: &'a mut ScanStats,
}

async fn process_file<C: ConnectionTrait>(
    conn: &C,
    ctx: &mut ScanContext<'_>,
    abs_path: &Path,
    rel_path: &str,
) -> Result<()> {
    let meta = std::fs::metadata(abs_path)?;
    let mtime_secs = meta.mtime();
    let mtime_nanos = meta.mtime_nsec() as u32;
    let size = meta.len();

    // Stage 1: compare mtime + size against cache.
    if let Some(cached) = ctx.cache.get(rel_path) {
        if cached.status == EntryStatus::Present.as_i16() {
            if let (Some(cached_size), Some(cached_mtime)) = (cached.size, cached.mtime) {
                let cached_mtime_secs = cached_mtime.timestamp();
                let cached_mtime_nanos = cached_mtime.timestamp_subsec_nanos();
                if cached_size == size as i64 && cached_mtime_secs == mtime_secs && cached_mtime_nanos == mtime_nanos {
                    // mtime + size unchanged — skip hashing.
                    ctx.stats.unchanged += 1;
                    debug!("unchanged  size={:<10}  {}", size, rel_path);
                    return Ok(());
                }
            }

            // Stage 2: compute xxHash64 and compare.
            let file_hash = hash::hash_file(abs_path, ctx.algo, ctx.fast_algo)?;

            if let Some(cached_fast) = cached.fast_digest {
                if cached_fast == file_hash.fast_digest {
                    // Content unchanged — update mtime in cache only.
                    debug!(
                        "unchanged  size={:<10}  sha256={}  {}",
                        file_hash.size,
                        tome_core::hash::hex_encode(&file_hash.digest)[..12].to_owned(),
                        rel_path,
                    );
                    let mtime_dt = make_mtime(mtime_secs, mtime_nanos);
                    ops::upsert_cache_present(
                        conn,
                        ops::UpsertCachePresentParams {
                            repository_id: ctx.repository_id,
                            path: rel_path.to_owned(),
                            snapshot_id: cached.snapshot_id,
                            entry_id: cached.entry_id,
                            object_id: cached.object_id.context("cache entry has no object_id")?,
                            mtime: Some(mtime_dt),
                            digest: cached.digest.clone(),
                            size: cached.size,
                            fast_digest: cached.fast_digest,
                        },
                    )
                    .await?;
                    ctx.stats.unchanged += 1;
                    return Ok(());
                }
            }

            // Content changed — create blob + entry.
            record_present_file(conn, ctx, rel_path, &meta, &file_hash, true).await?;
            return Ok(());
        }
    }

    // No cache entry or previously deleted — full hash.
    let file_hash = hash::hash_file(abs_path, ctx.algo, ctx.fast_algo)?;
    record_present_file(conn, ctx, rel_path, &meta, &file_hash, false).await?;
    Ok(())
}

async fn record_present_file<C: ConnectionTrait>(
    conn: &C,
    ctx: &mut ScanContext<'_>,
    rel_path: &str,
    meta: &std::fs::Metadata,
    file_hash: &hash::FileHash,
    modified: bool,
) -> Result<()> {
    let mtime_secs = meta.mtime();
    let mtime_nanos = meta.mtime_nsec() as u32;
    let mode = meta.mode() as i32;

    let blob = ops::get_or_create_blob(conn, file_hash).await?;
    let mtime_dt = make_mtime(mtime_secs, mtime_nanos);

    let entry = ops::insert_entry_present(conn, ctx.snapshot_id, rel_path, blob.id, Some(mode), Some(mtime_dt)).await?;

    ops::upsert_cache_present(
        conn,
        ops::UpsertCachePresentParams {
            repository_id: ctx.repository_id,
            path: rel_path.to_owned(),
            snapshot_id: ctx.snapshot_id,
            entry_id: entry.id,
            object_id: blob.id,
            mtime: Some(mtime_dt),
            digest: Some(file_hash.digest.to_vec()),
            size: Some(file_hash.size as i64),
            fast_digest: Some(file_hash.fast_digest),
        },
    )
    .await?;

    // Remove from cache map so it won't appear as "deleted" in the second pass.
    ctx.cache.remove(rel_path);

    let sha256_short = tome_core::hash::hex_encode(&file_hash.digest)[..12].to_owned();
    if modified {
        ctx.stats.modified += 1;
        info!("modified   size={:<10}  sha256={}  {}", file_hash.size, sha256_short, rel_path);
    } else {
        ctx.stats.added += 1;
        info!("added      size={:<10}  sha256={}  {}", file_hash.size, sha256_short, rel_path);
    }

    Ok(())
}

fn make_mtime(secs: i64, nanos: u32) -> chrono::DateTime<chrono::FixedOffset> {
    use chrono::TimeZone;
    chrono::Utc.timestamp_opt(secs, nanos).single().unwrap_or_else(chrono::Utc::now).fixed_offset()
}

/// Directory entry mode constant (matching POSIX S_IFDIR | 0o755).
const DIR_MODE: i32 = 0o040000;

/// Compute tree hash objects bottom-up for all directories in the snapshot.
///
/// Returns the root tree object ID, or `None` if the snapshot has no entries.
async fn compute_tree_objects(db: &DatabaseConnection, snapshot_id: i64, algo: DigestAlgorithm) -> Result<Option<i64>> {
    let entries = ops::entries_with_digest(db, snapshot_id, "").await?;
    if entries.is_empty() {
        return Ok(None);
    }

    // Group file entries by parent directory.
    // Key: directory path (empty string = root), Value: list of (name, digest)
    let mut dir_files: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    let mut all_dirs: HashSet<String> = HashSet::new();

    for (entry, obj) in &entries {
        if entry.status != EntryStatus::Present.as_i16() {
            continue;
        }
        let obj = match obj {
            Some(o) => o,
            None => continue,
        };

        let path = &entry.path;
        let (parent, name) = match path.rfind('/') {
            Some(pos) => (path[..pos].to_owned(), &path[pos + 1..]),
            None => (String::new(), path.as_str()),
        };

        dir_files.entry(parent.clone()).or_default().push((name.to_owned(), obj.digest.clone()));

        // Register all ancestor directories.
        let mut dir = parent.as_str();
        while !dir.is_empty() {
            if !all_dirs.insert(dir.to_owned()) {
                break;
            }
            dir = match dir.rfind('/') {
                Some(pos) => &dir[..pos],
                None => "",
            };
        }
        all_dirs.insert(String::new()); // root
    }

    // Ensure leaf directories with no files but with subdirectories are included.
    for dir in &all_dirs {
        dir_files.entry(dir.clone()).or_default();
    }

    let hash_algo = algo.to_hash_algorithm();

    // Sort directories by depth (deepest first) for bottom-up processing.
    let mut dirs: Vec<String> = dir_files.keys().cloned().collect();
    dirs.sort_by(|a, b| {
        let depth_a = if a.is_empty() { 0 } else { a.matches('/').count() + 1 };
        let depth_b = if b.is_empty() { 0 } else { b.matches('/').count() + 1 };
        depth_b.cmp(&depth_a).then_with(|| a.cmp(b))
    });

    // tree_digest[dir_path] = (object_id, digest_bytes)
    let mut tree_digest: HashMap<String, (i64, Vec<u8>)> = HashMap::new();

    let txn = db.begin().await?;

    for dir in &dirs {
        // Build children list: files (b'F') + subdirectories (b'D')
        let mut children: Vec<(u8, String, Vec<u8>)> = Vec::new();

        // File children
        if let Some(files) = dir_files.get(dir) {
            for (name, digest) in files {
                children.push((b'F', name.clone(), digest.clone()));
            }
        }

        // Directory children (already computed due to bottom-up order)
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        for (child_dir, (_, child_digest)) in &tree_digest {
            let suffix = child_dir.strip_prefix(prefix.as_str()).unwrap_or(child_dir);
            // Direct child: no more '/' separators after stripping prefix
            if !prefix.is_empty() {
                if let Some(s) = child_dir.strip_prefix(prefix.as_str()) {
                    if !s.contains('/') {
                        children.push((b'D', s.to_owned(), child_digest.clone()));
                    }
                }
            } else if !child_dir.is_empty() && !suffix.contains('/') {
                children.push((b'D', child_dir.clone(), child_digest.clone()));
            }
        }

        // Compute tree hash
        let children_refs: Vec<(u8, &str, &[u8])> =
            children.iter().map(|(k, n, d)| (*k, n.as_str(), d.as_slice())).collect();
        let digest = treblo::native::tree::compute_tree_hash(&children_refs, hash_algo);

        let tree_obj = ops::get_or_create_tree(&txn, &digest).await?;

        // Insert directory entry (skip root — root is referenced via snapshot.root_object_id)
        if !dir.is_empty() {
            ops::insert_entry_present(&txn, snapshot_id, dir, tree_obj.id, Some(DIR_MODE), None).await?;
        }

        debug!(
            "tree       {}  {}",
            tome_core::hash::hex_encode(&digest)[..12].to_owned(),
            if dir.is_empty() { "(root)" } else { dir },
        );

        tree_digest.insert(dir.clone(), (tree_obj.id, digest));
    }

    txn.commit().await?;

    // Return root tree object ID
    Ok(tree_digest.get("").map(|(id, _)| *id))
}
