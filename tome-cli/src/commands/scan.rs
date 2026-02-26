use std::{
    collections::HashSet,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_core::{hash, models::EntryStatus};
use tome_db::ops;

#[derive(Args)]
pub struct ScanArgs {
    /// Repository name (default: "default")
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,

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

    // 3. Create a new snapshot.
    let snapshot = ops::create_snapshot(db, repo.id, parent_id).await?;

    // 4. Load entry cache (previous state).
    let mut cache = ops::load_entry_cache(db, repo.id).await?;

    let mut stats = ScanStats::default();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 5. Collect directory entries (errors counted separately to avoid borrow conflict).
    let dir_entries: Vec<walkdir::DirEntry> = {
        let mut walk_errors = 0u64;
        let entries: Vec<_> = walkdir::WalkDir::new(&scan_root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| match e {
                Ok(e) => Some(e),
                Err(err) => {
                    warn!("walkdir error: {}", err);
                    walk_errors += 1;
                    None
                }
            })
            .filter(|e| e.file_type().is_file())
            .collect();
        stats.errors += walk_errors;
        entries
    };

    // 6. Process each file entry.
    for dir_entry in dir_entries {
        let abs_path = dir_entry.path();
        let rel_path = match abs_path.strip_prefix(&scan_root) {
            Ok(p) => p.to_string_lossy().into_owned(),
            Err(_) => {
                warn!("could not relativize {:?}", abs_path);
                stats.errors += 1;
                continue;
            }
        };

        stats.scanned += 1;
        seen_paths.insert(rel_path.clone());

        match process_file(db, abs_path, &rel_path, snapshot.id, repo.id, &mut cache, &mut stats)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                warn!("error processing {:?}: {}", rel_path, e);
                stats.errors += 1;
            }
        }
    }

    // 7. Detect deleted files: paths in cache that were not seen in the walk.
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
                info!("deleted: {}", path);
            }
            Err(e) => {
                warn!("error recording deletion of {:?}: {}", path, e);
                stats.errors += 1;
            }
        }
    }

    // 8. Update snapshot metadata with scan statistics.
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

async fn process_file(
    db: &DatabaseConnection,
    abs_path: &Path,
    rel_path: &str,
    snapshot_id: i64,
    repository_id: i64,
    cache: &mut std::collections::HashMap<String, tome_db::entities::entry_cache::Model>,
    stats: &mut ScanStats,
) -> Result<()> {
    let meta = std::fs::metadata(abs_path)?;
    let mtime_secs = meta.mtime();
    let mtime_nanos = meta.mtime_nsec() as u32;
    let size = meta.len();

    // Stage 1: compare mtime + size against cache.
    if let Some(cached) = cache.get(rel_path) {
        if cached.status == EntryStatus::Present.as_i16() {
            if let (Some(cached_size), Some(cached_mtime)) = (cached.size, cached.mtime) {
                let cached_mtime_secs = cached_mtime.timestamp();
                let cached_mtime_nanos = cached_mtime.timestamp_subsec_nanos();
                if cached_size == size as i64
                    && cached_mtime_secs == mtime_secs
                    && cached_mtime_nanos == mtime_nanos
                {
                    // mtime + size unchanged — skip hashing.
                    stats.unchanged += 1;
                    return Ok(());
                }
            }

            // Stage 2: compute xxHash64 and compare.
            let file_hash = hash::hash_file(abs_path)?;

            if let Some(cached_fast) = cached.fast_digest {
                if cached_fast == file_hash.fast_digest {
                    // Content unchanged — update mtime in cache only.
                    let mtime_dt = make_mtime(mtime_secs, mtime_nanos);
                    ops::upsert_cache_present(
                        db,
                        repository_id,
                        rel_path,
                        cached.snapshot_id,
                        cached.entry_id,
                        cached.blob_id.unwrap(),
                        Some(mtime_dt),
                        cached.digest.clone(),
                        cached.size,
                        cached.fast_digest,
                    )
                    .await?;
                    stats.unchanged += 1;
                    return Ok(());
                }
            }

            // Content changed — create blob + entry.
            record_present_file(db, abs_path, rel_path, snapshot_id, repository_id, &meta, &file_hash, cache, stats, true).await?;
            return Ok(());
        }
    }

    // No cache entry or previously deleted — full hash.
    let file_hash = hash::hash_file(abs_path)?;
    record_present_file(db, abs_path, rel_path, snapshot_id, repository_id, &meta, &file_hash, cache, stats, false).await?;
    Ok(())
}

async fn record_present_file(
    db: &DatabaseConnection,
    _abs_path: &Path,
    rel_path: &str,
    snapshot_id: i64,
    repository_id: i64,
    meta: &std::fs::Metadata,
    file_hash: &hash::FileHash,
    cache: &mut std::collections::HashMap<String, tome_db::entities::entry_cache::Model>,
    stats: &mut ScanStats,
    modified: bool,
) -> Result<()> {
    let mtime_secs = meta.mtime();
    let mtime_nanos = meta.mtime_nsec() as u32;
    let mode = meta.mode() as i32;

    let blob = ops::get_or_create_blob(db, file_hash).await?;
    let mtime_dt = make_mtime(mtime_secs, mtime_nanos);

    let entry =
        ops::insert_entry_present(db, snapshot_id, rel_path, blob.id, Some(mode), Some(mtime_dt))
            .await?;

    ops::upsert_cache_present(
        db,
        repository_id,
        rel_path,
        snapshot_id,
        entry.id,
        blob.id,
        Some(mtime_dt),
        Some(file_hash.digest.to_vec()),
        Some(file_hash.size as i64),
        Some(file_hash.fast_digest),
    )
    .await?;

    // Remove from cache map so it won't appear as "deleted" in the second pass.
    cache.remove(rel_path);

    if modified {
        stats.modified += 1;
        info!("modified: {}", rel_path);
    } else {
        stats.added += 1;
        info!("added: {}", rel_path);
    }

    Ok(())
}

fn make_mtime(secs: i64, nanos: u32) -> chrono::DateTime<chrono::FixedOffset> {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_opt(secs, nanos)
        .single()
        .unwrap_or_else(chrono::Utc::now)
        .fixed_offset()
}
