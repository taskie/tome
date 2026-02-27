use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_db::{connection::open as open_db, ops};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: SyncCommands,
}

#[derive(Subcommand)]
pub enum SyncCommands {
    /// Register a sync peer
    Add(SyncAddArgs),
    /// List sync peers
    List(SyncListArgs),
    /// Pull changes from a sync peer
    Pull(SyncPullArgs),
}

#[derive(Args)]
pub struct SyncAddArgs {
    /// Peer name
    pub name: String,
    /// Peer database URL (sqlite:///path or postgres://...)
    pub peer_url: String,
    /// Local repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
    /// Remote repository name on the peer [default: same as --repo]
    #[arg(long)]
    pub peer_repo: Option<String>,
}

#[derive(Args)]
pub struct SyncListArgs {
    /// Repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
}

#[derive(Args)]
pub struct SyncPullArgs {
    /// Peer name
    pub name: String,
    /// Local repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: SyncArgs) -> Result<()> {
    match args.command {
        SyncCommands::Add(a) => sync_add(db, a).await,
        SyncCommands::List(a) => sync_list(db, a).await,
        SyncCommands::Pull(a) => sync_pull(db, a).await,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// sync add
// ──────────────────────────────────────────────────────────────────────────────

async fn sync_add(db: &DatabaseConnection, args: SyncAddArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let peer_repo = args.peer_repo.unwrap_or_else(|| args.repo.clone());

    let config = serde_json::json!({ "peer_repo": peer_repo });
    let peer = ops::insert_sync_peer(db, &args.name, &args.peer_url, repo.id, config).await?;

    println!("sync peer registered: {} (id={}, url={}, peer_repo={})", peer.name, peer.id, peer.url, peer_repo);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// sync list
// ──────────────────────────────────────────────────────────────────────────────

async fn sync_list(db: &DatabaseConnection, args: SyncListArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let peers = ops::list_sync_peers(db, repo.id).await?;

    if peers.is_empty() {
        println!("no sync peers for repo {:?}", args.repo);
        return Ok(());
    }

    println!("{:<20} {:<20} url", "name", "last_snapshot_id");
    println!("{}", "-".repeat(70));
    for p in peers {
        let last = p.last_snapshot_id.map(|id| id.to_string()).unwrap_or_else(|| "-".to_owned());
        println!("{:<20} {:<20} {}", p.name, last, p.url);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// sync pull
// ──────────────────────────────────────────────────────────────────────────────

async fn sync_pull(local_db: &DatabaseConnection, args: SyncPullArgs) -> Result<()> {
    // Resolve local repo and peer record.
    let local_repo = ops::get_or_create_repository(local_db, &args.repo).await?;
    let peer = ops::find_sync_peer(local_db, &args.name, local_repo.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync peer {:?} not found", args.name))?;

    let peer_repo_name = peer.config.get("peer_repo").and_then(|v| v.as_str()).unwrap_or(&args.repo).to_owned();

    // Open peer database connection.
    let peer_db = open_db(&peer.url).await?;

    // Find the remote repository.
    let remote_repo = match ops::get_or_create_repository(&peer_db, &peer_repo_name).await {
        Ok(r) => r,
        Err(e) => bail!("failed to access peer repo {:?}: {}", peer_repo_name, e),
    };

    // Get new snapshots from peer since last sync.
    let new_snapshots = ops::snapshots_after(&peer_db, remote_repo.id, peer.last_snapshot_id).await?;

    if new_snapshots.is_empty() {
        println!("already up to date (no new snapshots from {:?})", args.name);
        return Ok(());
    }

    println!("pulling {} snapshot(s) from {:?} ...", new_snapshots.len(), args.name);

    let mut blobs_created = 0u64;
    let mut entries_synced = 0u64;
    let mut last_remote_snapshot_id = peer.last_snapshot_id;

    for remote_snap in &new_snapshots {
        // Create a corresponding local snapshot.
        let local_parent = ops::latest_snapshot(local_db, local_repo.id).await?.map(|s| s.id);
        let local_snap = ops::create_snapshot(local_db, local_repo.id, local_parent).await?;

        // Pull entries from remote snapshot.
        let remote_entries = ops::entries_in_snapshot(&peer_db, remote_snap.id).await?;

        for remote_entry in &remote_entries {
            // If the entry has a blob, ensure it exists locally.
            let local_blob_id = if let Some(ref remote_blob_id) = remote_entry.blob_id {
                // Fetch the remote blob record.
                let remote_blob = match ops::find_blob_by_id(&peer_db, *remote_blob_id).await? {
                    Some(b) => b,
                    None => {
                        warn!("blob {} not found in peer", remote_blob_id);
                        continue;
                    }
                };

                // Check if blob already exists locally by digest.
                let local_blob = ops::find_blob_by_digest(local_db, &remote_blob.digest).await?;

                let local_blob = if let Some(b) = local_blob {
                    b
                } else {
                    // Create blob locally (metadata only — no actual file transfer here).
                    let fh = tome_core::hash::FileHash {
                        size: remote_blob.size as u64,
                        fast_digest: remote_blob.fast_digest,
                        digest: remote_blob.digest.as_slice().try_into().unwrap_or([0u8; 32]),
                    };
                    let b = ops::get_or_create_blob(local_db, &fh).await?;
                    blobs_created += 1;
                    b
                };
                Some(local_blob.id)
            } else {
                None
            };

            // Create local entry.
            if remote_entry.status == 1 {
                if let Some(blob_id) = local_blob_id {
                    let local_entry = ops::insert_entry_present(
                        local_db,
                        local_snap.id,
                        &remote_entry.path,
                        blob_id,
                        remote_entry.mode,
                        remote_entry.mtime,
                    )
                    .await?;

                    // Update entry_cache.
                    ops::upsert_cache_present(
                        local_db,
                        ops::UpsertCachePresentParams {
                            repository_id: local_repo.id,
                            path: remote_entry.path.clone(),
                            snapshot_id: local_snap.id,
                            entry_id: local_entry.id,
                            blob_id,
                            mtime: remote_entry.mtime,
                            digest: None, // filled on next scan
                            size: None,
                            fast_digest: None,
                        },
                    )
                    .await?;
                }
            } else {
                let local_entry = ops::insert_entry_deleted(local_db, local_snap.id, &remote_entry.path).await?;
                ops::upsert_cache_deleted(local_db, local_repo.id, &remote_entry.path, local_snap.id, local_entry.id)
                    .await?;
            }
            entries_synced += 1;
        }

        // Update snapshot metadata.
        let meta = serde_json::json!({
            "synced_from": peer.name,
            "remote_snapshot_id": remote_snap.id,
            "entries": remote_entries.len(),
        });
        ops::update_snapshot_metadata(local_db, local_snap.id, meta).await?;

        last_remote_snapshot_id = Some(remote_snap.id);
        info!("synced snapshot {} ({} entries)", remote_snap.id, remote_entries.len());
    }

    // Update sync peer progress.
    if let Some(last_id) = last_remote_snapshot_id {
        ops::update_sync_peer_progress(local_db, peer.id, last_id).await?;
    }

    println!(
        "sync complete: {} snapshot(s), {} entries, {} blobs created",
        new_snapshots.len(),
        entries_synced,
        blobs_created
    );
    Ok(())
}
