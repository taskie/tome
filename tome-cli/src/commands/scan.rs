use std::{
    collections::HashSet,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;
use tracing::{debug, info, warn};

use tome_core::{
    hash::{self, DigestAlgorithm},
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
    let scan_root = args.path.unwrap_or_else(|| PathBuf::from("."));
    let scan_root = scan_root.canonicalize()?;

    info!("scanning {:?} for repo {:?}", scan_root, args.repo);

    // 1. Get or create repository.
    let repo = ops::get_or_create_repository(db, &args.repo).await?;

    // 2. Find the previous snapshot (for parent chain).
    let parent = ops::latest_snapshot(db, repo.id).await?;
    let parent_id = parent.as_ref().map(|s| s.id);

    // 3. Resolve digest algorithm (set in repo.config on first use).
    let algo = ops::get_or_init_repository_digest_algorithm(db, &repo, args.digest_algorithm).await?;

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
    let mut ctx = ScanContext {
        db,
        snapshot_id: snapshot.id,
        repository_id: repo.id,
        algo,
        cache: &mut cache,
        stats: &mut stats,
    };
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

        match process_file(&mut ctx, abs_path, &rel_path).await {
            Ok(()) => {}
            Err(e) => {
                warn!("error processing {:?}: {}", rel_path, e);
                ctx.stats.errors += 1;
            }
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
        match ops::insert_entry_deleted(db, snapshot.id, &path).await {
            Ok(e) => {
                ops::upsert_cache_deleted(db, repo.id, &path, snapshot.id, e.id).await?;
                stats.deleted += 1;
                info!("deleted    {}", path);
            }
            Err(e) => {
                warn!("error recording deletion of {:?}: {}", path, e);
                stats.errors += 1;
            }
        }
    }

    // 9. Update snapshot metadata with scan statistics.
    let metadata = serde_json::json!({
        "scan_root": scan_root.to_string_lossy(),
        "scanned": stats.scanned,
        "added": stats.added,
        "modified": stats.modified,
        "unchanged": stats.unchanged,
        "deleted": stats.deleted,
        "errors": stats.errors,
    });
    ops::update_snapshot_metadata(db, snapshot.id, metadata).await?;

    println!(
        "scan complete: {} scanned, {} added, {} modified, {} unchanged, {} deleted, {} errors",
        stats.scanned, stats.added, stats.modified, stats.unchanged, stats.deleted, stats.errors
    );

    Ok(())
}

struct ScanContext<'a> {
    db: &'a DatabaseConnection,
    snapshot_id: i64,
    repository_id: i64,
    algo: DigestAlgorithm,
    cache: &'a mut std::collections::HashMap<String, tome_db::entities::entry_cache::Model>,
    stats: &'a mut ScanStats,
}

async fn process_file(ctx: &mut ScanContext<'_>, abs_path: &Path, rel_path: &str) -> Result<()> {
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
            let file_hash = hash::hash_file(abs_path, ctx.algo)?;

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
                        ctx.db,
                        ops::UpsertCachePresentParams {
                            repository_id: ctx.repository_id,
                            path: rel_path.to_owned(),
                            snapshot_id: cached.snapshot_id,
                            entry_id: cached.entry_id,
                            blob_id: cached.blob_id.unwrap(),
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
            record_present_file(ctx, rel_path, &meta, &file_hash, true).await?;
            return Ok(());
        }
    }

    // No cache entry or previously deleted — full hash.
    let file_hash = hash::hash_file(abs_path, ctx.algo)?;
    record_present_file(ctx, rel_path, &meta, &file_hash, false).await?;
    Ok(())
}

async fn record_present_file(
    ctx: &mut ScanContext<'_>,
    rel_path: &str,
    meta: &std::fs::Metadata,
    file_hash: &hash::FileHash,
    modified: bool,
) -> Result<()> {
    let mtime_secs = meta.mtime();
    let mtime_nanos = meta.mtime_nsec() as u32;
    let mode = meta.mode() as i32;

    let blob = ops::get_or_create_blob(ctx.db, file_hash).await?;
    let mtime_dt = make_mtime(mtime_secs, mtime_nanos);

    let entry =
        ops::insert_entry_present(ctx.db, ctx.snapshot_id, rel_path, blob.id, Some(mode), Some(mtime_dt)).await?;

    ops::upsert_cache_present(
        ctx.db,
        ops::UpsertCachePresentParams {
            repository_id: ctx.repository_id,
            path: rel_path.to_owned(),
            snapshot_id: ctx.snapshot_id,
            entry_id: entry.id,
            blob_id: blob.id,
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
