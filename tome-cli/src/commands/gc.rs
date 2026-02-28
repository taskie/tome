use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;
use std::collections::{HashMap, HashSet};
use tracing::warn;

use tome_db::ops;
use tome_store::factory;

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct GcArgs {
    /// Only report what would be deleted — make no changes
    #[arg(long)]
    pub dry_run: bool,

    /// Keep at least N most-recent snapshots per repository (0 = no pruning)
    #[arg(long, default_value_t = 0)]
    pub keep: usize,

    /// Keep snapshots newer than D days; 0 = no age cutoff
    #[arg(long, default_value_t = 0)]
    pub keep_days: u64,

    /// Restrict snapshot pruning to one repository (default: all)
    #[arg(long, short = 'r')]
    pub repo: Option<String>,

    /// Restrict replica file/record deletion to one store.
    /// Blob records are only removed when all replicas are handled.
    #[arg(long, short = 's')]
    pub store: Option<String>,

    /// Skip deleting files from stores; only clean up DB records
    #[arg(long)]
    pub no_store_delete: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: GcArgs) -> Result<()> {
    // Resolve optional store filter (applies to file + replica record deletion).
    let store_filter_id: Option<i64> = if let Some(ref store_name) = args.store {
        let s = ops::find_store_by_name(db, store_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("store {:?} not found", store_name))?;
        Some(s.id)
    } else {
        None
    };

    // Collect repositories in scope for snapshot pruning.
    let repos = if let Some(ref name) = args.repo {
        let r = ops::get_or_create_repository(db, name).await?;
        vec![r]
    } else {
        ops::list_repositories(db).await?
    };

    // ── Phase 1: determine which snapshots to prune ──────────────────────────
    //
    // Snapshots are listed newest-first per repo.  We retain the first `keep`
    // entries and any that are newer than the age cutoff.  Everything else is
    // a candidate for pruning.

    let will_prune = args.keep > 0 || args.keep_days > 0;
    let mut prune_snap_ids: HashSet<i64> = HashSet::new();

    if will_prune {
        let cutoff: Option<chrono::DateTime<chrono::Utc>> = if args.keep_days > 0 {
            Some(chrono::Utc::now() - chrono::Duration::days(args.keep_days as i64))
        } else {
            None
        };

        for repo in &repos {
            let snapshots = ops::list_snapshots_for_repo(db, repo.id).await?;
            let keep_count = if args.keep > 0 { args.keep } else { snapshots.len() };

            let to_prune: Vec<i64> = snapshots
                .iter()
                .enumerate()
                .filter(|(i, s)| {
                    // Index i is 0-based newest-first.  A snapshot is prunable
                    // when it falls beyond the keep window AND (no age cutoff OR
                    // it is old enough).
                    *i >= keep_count && cutoff.is_none_or(|c| s.created_at.with_timezone(&chrono::Utc) < c)
                })
                .map(|(_, s)| s.id)
                .collect();

            if !to_prune.is_empty() {
                println!(
                    "repo {:?}: {} of {} snapshot(s) marked for pruning",
                    repo.name,
                    to_prune.len(),
                    snapshots.len()
                );
                prune_snap_ids.extend(to_prune);
            }
        }
    }

    let prune_ids_vec: Vec<i64> = prune_snap_ids.iter().copied().collect();

    // ── Phase 2: find orphaned blobs ─────────────────────────────────────────
    //
    // Live mode  : execute Phase 1 first, then call unreferenced_blobs().
    //              After deleting entries/snapshots the query naturally picks
    //              up blobs that were freed by the pruning.
    //
    // Dry-run mode: nothing has been deleted yet, so we pre-compute:
    //   orphaned = currently_unreferenced ∪ exclusively_in_pruned_snapshots

    let orphaned_blob_ids: Vec<i64>;

    if args.dry_run {
        // Pre-compute without touching the DB.
        let current_orphans: HashSet<i64> = ops::unreferenced_blobs(db).await?.into_iter().map(|b| b.id).collect();

        let exclusive_from_prune = if prune_ids_vec.is_empty() {
            HashSet::new()
        } else {
            let pruned_blobs = ops::blob_ids_in_snapshots(db, &prune_ids_vec).await?;
            // Retained = all snapshot IDs NOT in the prune set.
            let all_ids = ops::all_snapshot_ids(db).await?;
            let retained_ids: Vec<i64> = all_ids.into_iter().filter(|id| !prune_snap_ids.contains(id)).collect();
            let retained_blobs = ops::blob_ids_in_snapshots(db, &retained_ids).await?;
            pruned_blobs.difference(&retained_blobs).copied().collect()
        };

        orphaned_blob_ids = current_orphans.union(&exclusive_from_prune).copied().collect();
    } else {
        // Execute Phase 1.
        if !prune_ids_vec.is_empty() {
            // Clear entry_cache rows anchored to pruned snapshots FIRST.  Unchanged
            // files keep their cache row pointing at the entry from the snapshot where
            // they were last recorded, so we must remove those rows before deleting
            // entries / snapshots (FK constraints would otherwise reject the deletion).
            ops::delete_entry_cache_for_snapshots(db, &prune_ids_vec).await?;
            let entries_del = ops::delete_entries_in_snapshots(db, &prune_ids_vec).await?;
            let snaps_del = ops::delete_snapshot_records(db, &prune_ids_vec).await?;
            println!("pruned {} snapshot(s), removed {} entry record(s)", snaps_del, entries_del);
        } else {
            println!("no snapshots to prune");
        }

        // Now collect all blobs with no remaining entries.
        orphaned_blob_ids = ops::unreferenced_blobs(db).await?.into_iter().map(|b| b.id).collect();
    }

    if orphaned_blob_ids.is_empty() {
        println!("no orphaned blobs — nothing to collect");
        return Ok(());
    }

    println!("found {} orphaned blob(s)", orphaned_blob_ids.len());

    // ── Phase 3: collect replica files + DB records ──────────────────────────
    //
    // For each orphaned blob:
    //   - Iterate its replicas; skip those outside --store scope.
    //   - Delete the file from the store (unless --no-store-delete or dry-run).
    //     On failure: warn and mark the blob as not fully cleaned.
    //   - Delete the replica record from DB (or report it in dry-run).
    //   - If ALL replicas for this blob were handled, delete the blob record.
    //
    // Rationale for "all replicas must be handled" invariant: the blobs table
    // must not be deleted while replica records still point at it (FK).  Also,
    // keeping the blob record while some replicas remain lets the user resume GC
    // (e.g., for a different store) without losing track of the blob.

    let replicas = ops::replicas_for_blobs(db, &orphaned_blob_ids).await?;

    // Group replicas by blob_id for fast lookup.
    let mut replica_map: HashMap<i64, Vec<_>> = HashMap::new();
    for (r, s) in replicas {
        replica_map.entry(r.blob_id).or_default().push((r, s));
    }

    let mut files_deleted = 0u64;
    let mut file_errors = 0u64;
    let mut replica_records_deleted = 0u64;
    let mut blob_records_deleted = 0u64;

    for blob_id in &orphaned_blob_ids {
        let blob_replicas = replica_map.get(blob_id).map(Vec::as_slice).unwrap_or(&[]);
        // blob_fully_cleaned: true iff every replica for this blob was
        // successfully handled (file deleted + record deleted, or no-store-delete).
        // Starts true; set to false when a replica is out-of-scope or errors.
        let mut blob_fully_cleaned = true;

        for (replica, store) in blob_replicas {
            // Scope check: if --store was given, skip replicas in other stores.
            let in_scope = store_filter_id.is_none_or(|sid| store.id == sid);
            if !in_scope {
                blob_fully_cleaned = false;
                continue;
            }

            if args.dry_run {
                // Just count.
                if !args.no_store_delete {
                    files_deleted += 1;
                }
                replica_records_deleted += 1;
                continue;
            }

            // Delete the file from the store (unless --no-store-delete).
            let file_ok = if args.no_store_delete {
                true
            } else {
                match factory::open_storage(&store.url).await {
                    Err(e) => {
                        warn!("cannot open store {:?}: {}", store.name, e);
                        file_errors += 1;
                        blob_fully_cleaned = false;
                        false
                    }
                    Ok(storage) => match storage.delete(&replica.path).await {
                        Ok(()) => {
                            files_deleted += 1;
                            true
                        }
                        Err(e) => {
                            warn!("failed to delete {:?} from store {:?}: {}", replica.path, store.name, e);
                            file_errors += 1;
                            blob_fully_cleaned = false;
                            false
                        }
                    },
                }
            };

            // Only remove the replica record when the file is gone (or --no-store-delete).
            if file_ok {
                ops::delete_replica_records(db, &[replica.id]).await?;
                replica_records_deleted += 1;
            }
        }

        // Remove the blob record only when every replica has been cleaned up.
        if !args.dry_run && blob_fully_cleaned {
            ops::delete_blob_records(db, &[*blob_id]).await?;
            blob_records_deleted += 1;
        } else if args.dry_run {
            blob_records_deleted += 1;
        }
    }

    // ── Summary ──────────────────────────────────────────────────────────────
    let prefix = if args.dry_run { "[dry-run] would " } else { "" };
    println!("---");
    if prune_ids_vec.is_empty() {
        println!("no snapshots pruned");
    } else if args.dry_run {
        println!("{}prune {} snapshot(s)", prefix, prune_ids_vec.len());
    }
    if !args.no_store_delete {
        println!(
            "{}delete {} replica file(s) from store(s){err}",
            prefix,
            files_deleted,
            err = if file_errors > 0 { format!(" ({} error(s))", file_errors) } else { String::new() }
        );
    }
    println!("{}remove {} replica record(s), {} blob record(s)", prefix, replica_records_deleted, blob_records_deleted);

    if file_errors > 0 {
        anyhow::bail!("{} file deletion(s) failed; re-run to retry", file_errors);
    }

    Ok(())
}
