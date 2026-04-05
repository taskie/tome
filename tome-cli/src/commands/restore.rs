use anyhow::{Context, Result};
use clap::Args;
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_db::ops;
use tome_store::factory;

use crate::snapshot_ref::{self, SnapshotRef};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct RestoreArgs {
    /// Snapshot reference (ID, @latest, @latest~N, @YYYY-MM-DD)
    #[arg(long)]
    pub snapshot: String,
    /// Repository name (required for @-references) [default: "default"]
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Restore only from this store (default: use any available store)
    #[arg(long)]
    pub store: Option<String>,
    /// Path prefix filter (restore only files whose path starts with this)
    #[arg(long, default_value = "")]
    pub prefix: String,
    /// Destination directory (files are written as <dest>/<path>)
    pub dest: std::path::PathBuf,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: RestoreArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let snap_ref: SnapshotRef = args.snapshot.parse()?;
    let snapshot_id = snapshot_ref::resolve(db, repo.id, &snap_ref).await?;

    // Resolve optional store filter.
    let store_filter: Option<i64> = if let Some(ref store_name) = args.store {
        let s = ops::find_store_by_name(db, store_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("store {:?} not found", store_name))?;
        Some(s.id)
    } else {
        None
    };

    // Fetch all present entries in the snapshot (with blob info).
    let entries = ops::entries_with_digest(db, snapshot_id, &args.prefix).await?;
    let present: Vec<_> = entries.into_iter().filter(|(e, _)| e.status == 1 && e.object_id.is_some()).collect();

    if present.is_empty() {
        println!("no files to restore in snapshot {}", snapshot_id);
        return Ok(());
    }

    println!("restoring {} file(s) to {:?} ...", present.len(), args.dest);

    let tmp_dir = tempfile::tempdir()?;
    let mut restored = 0u64;
    let mut skipped = 0u64;
    let mut errors = 0u64;

    for (entry, _blob) in &present {
        let object_id = entry.object_id.context("present entry has no object_id")?;

        // Find usable replicas for this object.
        let replicas = ops::replicas_for_object(db, object_id).await?;
        let candidates: Vec<_> =
            replicas.iter().filter(|(r, s)| !r.encrypted && store_filter.is_none_or(|sid| s.id == sid)).collect();

        if candidates.is_empty() {
            warn!("no replica found for object {} (path: {})", object_id, entry.path);
            skipped += 1;
            continue;
        }

        // Try each candidate until one succeeds.
        let mut downloaded = false;
        let tmp_file = tmp_dir.path().join(object_id.to_string());

        for (replica, store) in &candidates {
            let storage = match factory::open_storage(&store.url).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("failed to open store {:?}: {}", store.name, e);
                    continue;
                }
            };
            match storage.download(&replica.path, &tmp_file).await {
                Ok(()) => {
                    downloaded = true;
                    break;
                }
                Err(e) => {
                    warn!("download failed from store {:?}: {}", store.name, e);
                }
            }
        }

        if !downloaded {
            warn!("could not download blob for: {}", entry.path);
            errors += 1;
            continue;
        }

        // Write to destination.
        let dest_path = args.dest.join(&entry.path);
        if let Some(parent) = dest_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("failed to create directory {:?}: {}", parent, e);
                errors += 1;
                continue;
            }
        }

        match std::fs::copy(&tmp_file, &dest_path) {
            Ok(_) => {
                info!("restored: {}", entry.path);
                restored += 1;
            }
            Err(e) => {
                warn!("failed to write {:?}: {}", dest_path, e);
                errors += 1;
            }
        }

        // Remove temp file to avoid stale content on next iteration.
        let _ = std::fs::remove_file(&tmp_file);
    }

    println!("done: {} restored, {} skipped (no replica), {} errors", restored, skipped, errors);
    Ok(())
}
