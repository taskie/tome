use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_db::{connection::open as open_db, entities::blob, ops};
use tome_server::routes::sync::{PullResponse, PushRequest, SyncEntry, SyncReplica};

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
    /// Push changes to a sync peer
    Push(SyncPushArgs),
}

#[derive(Args)]
pub struct SyncAddArgs {
    /// Peer name
    pub name: String,
    /// Peer URL: sqlite:///path, postgres://... or https://tome.example.com
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

#[derive(Args)]
pub struct SyncPushArgs {
    /// Peer name
    pub name: String,
    /// Local repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
    /// Local machine_id to record as source (defaults to current Sonyflake machine_id)
    #[arg(long)]
    pub machine_id: Option<i16>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: SyncArgs) -> Result<()> {
    match args.command {
        SyncCommands::Add(a) => sync_add(db, a).await,
        SyncCommands::List(a) => sync_list(db, a).await,
        SyncCommands::Pull(a) => sync_pull(db, a).await,
        SyncCommands::Push(a) => sync_push(db, a).await,
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
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn is_http_peer(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Apply a received snapshot (entries + replicas) to the local database.
/// Used by both the DB-mode pull and the HTTP-mode pull.
async fn apply_pulled_snapshot(
    local_db: &DatabaseConnection,
    local_repo_id: i64,
    peer_name: &str,
    remote_snap_id_str: &str,
    entries: &[SyncEntry],
    replicas: &[SyncReplica],
) -> Result<(u64, u64)> {
    let local_parent = ops::latest_snapshot(local_db, local_repo_id).await?.map(|s| s.id);
    let local_snap = ops::create_snapshot(local_db, local_repo_id, local_parent, "").await?;

    let mut blobs_created = 0u64;
    let mut entries_synced = 0u64;

    // Build replica lookup: blob_digest → Vec<SyncReplica>
    let mut replica_by_digest: std::collections::HashMap<String, Vec<&SyncReplica>> = std::collections::HashMap::new();
    for r in replicas {
        replica_by_digest.entry(r.blob_digest.clone()).or_default().push(r);
    }

    for e in entries {
        if e.status == 1 {
            if let (Some(hex), Some(size), Some(fast)) = (&e.blob_digest, e.blob_size, e.blob_fast_digest) {
                let digest_bytes = hex::decode(hex)?;
                let digest_arr: [u8; 32] =
                    digest_bytes.as_slice().try_into().context("invalid digest length in sync entry")?;

                let b: blob::Model = if let Some(b) = ops::find_blob_by_digest(local_db, &digest_bytes).await? {
                    b
                } else {
                    let fh = tome_core::hash::FileHash { size: size as u64, fast_digest: fast, digest: digest_arr };
                    let b = ops::get_or_create_blob(local_db, &fh).await?;
                    blobs_created += 1;
                    b
                };

                // Upsert replicas for this blob.
                if let Some(reps) = replica_by_digest.get(hex.as_str()) {
                    for r in reps {
                        let store =
                            ops::get_or_create_store(local_db, &r.store_name, &r.store_url, serde_json::json!({}))
                                .await?;
                        if !ops::replica_exists(local_db, b.id, store.id).await? {
                            ops::insert_replica(local_db, b.id, store.id, &r.path, r.encrypted).await?;
                        }
                    }
                }

                let mtime =
                    e.mtime.as_deref().map(|s| s.parse::<chrono::DateTime<chrono::FixedOffset>>()).transpose()?;
                let entry = ops::insert_entry_present(local_db, local_snap.id, &e.path, b.id, e.mode, mtime).await?;

                ops::upsert_cache_present(
                    local_db,
                    ops::UpsertCachePresentParams {
                        repository_id: local_repo_id,
                        path: e.path.clone(),
                        snapshot_id: local_snap.id,
                        entry_id: entry.id,
                        blob_id: b.id,
                        mtime,
                        digest: Some(b.digest.clone()),
                        size: Some(b.size),
                        fast_digest: Some(b.fast_digest),
                    },
                )
                .await?;
            }
        } else {
            let entry = ops::insert_entry_deleted(local_db, local_snap.id, &e.path).await?;
            ops::upsert_cache_deleted(local_db, local_repo_id, &e.path, local_snap.id, entry.id).await?;
        }
        entries_synced += 1;
    }

    let meta = serde_json::json!({
        "synced_from": peer_name,
        "remote_snapshot_id": remote_snap_id_str,
        "entries": entries.len(),
    });
    ops::update_snapshot_metadata(local_db, local_snap.id, meta).await?;

    Ok((blobs_created, entries_synced))
}

// ──────────────────────────────────────────────────────────────────────────────
// sync pull
// ──────────────────────────────────────────────────────────────────────────────

async fn sync_pull(local_db: &DatabaseConnection, args: SyncPullArgs) -> Result<()> {
    let local_repo = ops::get_or_create_repository(local_db, &args.repo).await?;
    let peer = ops::find_sync_peer(local_db, &args.name, local_repo.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync peer {:?} not found", args.name))?;

    let peer_repo_name = peer.config.get("peer_repo").and_then(|v| v.as_str()).unwrap_or(&args.repo).to_owned();

    if is_http_peer(&peer.url) {
        sync_pull_http(local_db, &local_repo, &peer, &peer_repo_name, &args.name).await
    } else {
        sync_pull_db(local_db, &local_repo, &peer, &peer_repo_name, &args.name).await
    }
}

async fn sync_pull_http(
    local_db: &DatabaseConnection,
    local_repo: &tome_db::entities::repository::Model,
    peer: &tome_db::entities::sync_peer::Model,
    peer_repo_name: &str,
    peer_display_name: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let after_param = peer.last_snapshot_id.map(|id| id.to_string());

    let mut url = format!("{}/sync/pull?repo={}", peer.url.trim_end_matches('/'), peer_repo_name);
    if let Some(after) = &after_param {
        url.push_str(&format!("&after={after}"));
    }

    let resp = client.get(&url).send().await?.error_for_status()?;
    let data: PullResponse = resp.json().await?;

    if data.snapshots.is_empty() {
        println!("already up to date (no new snapshots from {:?})", peer_display_name);
        return Ok(());
    }

    println!("pulling {} snapshot(s) from {:?} (HTTP) ...", data.snapshots.len(), peer_display_name);

    let mut total_blobs = 0u64;
    let mut total_entries = 0u64;
    let mut last_remote_id: Option<i64> = peer.last_snapshot_id;

    for snap in &data.snapshots {
        let (b, e) =
            apply_pulled_snapshot(local_db, local_repo.id, peer_display_name, &snap.id, &snap.entries, &snap.replicas)
                .await?;
        total_blobs += b;
        total_entries += e;
        last_remote_id = Some(snap.id.parse()?);
        info!("synced snapshot {} ({} entries)", snap.id, snap.entries.len());
    }

    if let Some(last_id) = last_remote_id {
        ops::update_sync_peer_progress(local_db, peer.id, last_id).await?;
    }

    println!(
        "sync complete: {} snapshot(s), {} entries, {} blobs created",
        data.snapshots.len(),
        total_entries,
        total_blobs
    );
    Ok(())
}

async fn sync_pull_db(
    local_db: &DatabaseConnection,
    local_repo: &tome_db::entities::repository::Model,
    peer: &tome_db::entities::sync_peer::Model,
    peer_repo_name: &str,
    peer_display_name: &str,
) -> Result<()> {
    let peer_db = open_db(&peer.url).await?;

    let remote_repo = match ops::get_or_create_repository(&peer_db, peer_repo_name).await {
        Ok(r) => r,
        Err(e) => bail!("failed to access peer repo {:?}: {}", peer_repo_name, e),
    };

    let new_snapshots = ops::snapshots_after(&peer_db, remote_repo.id, peer.last_snapshot_id).await?;

    if new_snapshots.is_empty() {
        println!("already up to date (no new snapshots from {:?})", peer_display_name);
        return Ok(());
    }

    println!("pulling {} snapshot(s) from {:?} ...", new_snapshots.len(), peer_display_name);

    let mut blobs_created = 0u64;
    let mut entries_synced = 0u64;
    let mut last_remote_snapshot_id = peer.last_snapshot_id;

    for remote_snap in &new_snapshots {
        let local_parent = ops::latest_snapshot(local_db, local_repo.id).await?.map(|s| s.id);
        let local_snap = ops::create_snapshot(local_db, local_repo.id, local_parent, "").await?;

        let remote_entries = ops::entries_in_snapshot(&peer_db, remote_snap.id).await?;

        for remote_entry in &remote_entries {
            let local_blob: Option<blob::Model> = if let Some(ref remote_blob_id) = remote_entry.blob_id {
                let remote_blob = match ops::find_blob_by_id(&peer_db, *remote_blob_id).await? {
                    Some(b) => b,
                    None => {
                        warn!("blob {} not found in peer", remote_blob_id);
                        continue;
                    }
                };

                let b = if let Some(b) = ops::find_blob_by_digest(local_db, &remote_blob.digest).await? {
                    b
                } else {
                    let fh = tome_core::hash::FileHash {
                        size: remote_blob.size as u64,
                        fast_digest: remote_blob.fast_digest,
                        digest: remote_blob
                            .digest
                            .as_slice()
                            .try_into()
                            .context("invalid digest length in remote blob")?,
                    };
                    let b = ops::get_or_create_blob(local_db, &fh).await?;
                    blobs_created += 1;
                    b
                };

                let remote_replicas = ops::replicas_for_blob(&peer_db, *remote_blob_id).await?;
                for (rr, remote_store) in &remote_replicas {
                    let local_store = ops::get_or_create_store(
                        local_db,
                        &remote_store.name,
                        &remote_store.url,
                        remote_store.config.clone(),
                    )
                    .await?;
                    if !ops::replica_exists(local_db, b.id, local_store.id).await? {
                        ops::insert_replica(local_db, b.id, local_store.id, &rr.path, rr.encrypted).await?;
                    }
                }

                Some(b)
            } else {
                None
            };

            if remote_entry.status == 1 {
                if let Some(ref b) = local_blob {
                    let local_entry = ops::insert_entry_present(
                        local_db,
                        local_snap.id,
                        &remote_entry.path,
                        b.id,
                        remote_entry.mode,
                        remote_entry.mtime,
                    )
                    .await?;

                    ops::upsert_cache_present(
                        local_db,
                        ops::UpsertCachePresentParams {
                            repository_id: local_repo.id,
                            path: remote_entry.path.clone(),
                            snapshot_id: local_snap.id,
                            entry_id: local_entry.id,
                            blob_id: b.id,
                            mtime: remote_entry.mtime,
                            digest: Some(b.digest.clone()),
                            size: Some(b.size),
                            fast_digest: Some(b.fast_digest),
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

        let meta = serde_json::json!({
            "synced_from": peer_display_name,
            "remote_snapshot_id": remote_snap.id,
            "entries": remote_entries.len(),
        });
        ops::update_snapshot_metadata(local_db, local_snap.id, meta).await?;

        last_remote_snapshot_id = Some(remote_snap.id);
        info!("synced snapshot {} ({} entries)", remote_snap.id, remote_entries.len());
    }

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

// ──────────────────────────────────────────────────────────────────────────────
// sync push
// ──────────────────────────────────────────────────────────────────────────────

async fn sync_push(local_db: &DatabaseConnection, args: SyncPushArgs) -> Result<()> {
    let local_repo = ops::get_or_create_repository(local_db, &args.repo).await?;
    let peer = ops::find_sync_peer(local_db, &args.name, local_repo.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync peer {:?} not found", args.name))?;

    let peer_repo_name = peer.config.get("peer_repo").and_then(|v| v.as_str()).unwrap_or(&args.repo).to_owned();

    let source_machine_id = args.machine_id.unwrap_or(0);
    if source_machine_id == 0 {
        warn!("machine_id is 0 (local-only default). Set --machine-id or configure machine_id in tome.toml.");
    }

    if is_http_peer(&peer.url) {
        sync_push_http(local_db, &local_repo, &peer, &peer_repo_name, source_machine_id, &args.name).await
    } else {
        sync_push_db(local_db, &local_repo, &peer, &peer_repo_name, source_machine_id, &args.name).await
    }
}

async fn sync_push_http(
    local_db: &DatabaseConnection,
    local_repo: &tome_db::entities::repository::Model,
    peer: &tome_db::entities::sync_peer::Model,
    peer_repo_name: &str,
    source_machine_id: i16,
    peer_display_name: &str,
) -> Result<()> {
    let new_snapshots = ops::snapshots_after(local_db, local_repo.id, peer.last_snapshot_id).await?;

    if new_snapshots.is_empty() {
        println!("already up to date (no new snapshots to push to {:?})", peer_display_name);
        return Ok(());
    }

    println!("pushing {} snapshot(s) to {:?} (HTTP) ...", new_snapshots.len(), peer_display_name);

    let client = reqwest::Client::new();
    let push_url = format!("{}/sync/push?repo={}", peer.url.trim_end_matches('/'), peer_repo_name);

    let mut entries_synced = 0u64;
    let mut replicas_synced = 0u64;
    let mut last_local_snapshot_id = peer.last_snapshot_id;

    for local_snap in &new_snapshots {
        // Collect entries with blob info (LEFT JOIN).
        let pairs = ops::entries_with_digest(local_db, local_snap.id, "").await?;

        // Collect all blob IDs to fetch replicas in batch.
        let blob_ids: Vec<i64> = pairs.iter().filter_map(|(_, b)| b.as_ref().map(|b| b.id)).collect();
        let all_replicas = ops::replicas_for_blobs(local_db, &blob_ids).await?;

        // Build blob_id → digest map for replica records.
        let blob_digest_map: std::collections::HashMap<i64, String> = pairs
            .iter()
            .filter_map(|(_, b)| b.as_ref().map(|b| (b.id, tome_core::hash::hex_encode(&b.digest))))
            .collect();

        let mut sync_entries: Vec<SyncEntry> = Vec::with_capacity(pairs.len());
        let mut sync_replicas: Vec<SyncReplica> = Vec::new();

        for (entry, blob) in &pairs {
            sync_entries.push(SyncEntry {
                path: entry.path.clone(),
                status: entry.status,
                blob_digest: blob.as_ref().map(|b| tome_core::hash::hex_encode(&b.digest)),
                blob_size: blob.as_ref().map(|b| b.size),
                blob_fast_digest: blob.as_ref().map(|b| b.fast_digest),
                mode: entry.mode,
                mtime: entry.mtime.map(|t| t.to_rfc3339()),
            });
        }

        for (replica, store) in &all_replicas {
            if let Some(digest) = blob_digest_map.get(&replica.blob_id) {
                sync_replicas.push(SyncReplica {
                    blob_digest: digest.clone(),
                    store_name: store.name.clone(),
                    store_url: store.url.clone(),
                    path: replica.path.clone(),
                    encrypted: replica.encrypted,
                });
                replicas_synced += 1;
            }
        }

        let req = PushRequest {
            source_machine_id: Some(source_machine_id),
            source_snapshot_id: Some(local_snap.id.to_string()),
            message: local_snap.message.clone(),
            metadata: local_snap.metadata.clone(),
            entries: sync_entries,
            replicas: sync_replicas,
        };

        let resp = client.post(&push_url).json(&req).send().await?.error_for_status()?;
        let result: serde_json::Value = resp.json().await?;
        let remote_id = result["snapshot_id"].as_str().unwrap_or("?");

        entries_synced += pairs.len() as u64;
        last_local_snapshot_id = Some(local_snap.id);
        info!("pushed snapshot {} -> {} ({} entries)", local_snap.id, remote_id, pairs.len());
    }

    if let Some(last_id) = last_local_snapshot_id {
        ops::update_sync_peer_progress(local_db, peer.id, last_id).await?;
    }

    println!(
        "push complete: {} snapshot(s), {} entries, {} replicas synced",
        new_snapshots.len(),
        entries_synced,
        replicas_synced
    );
    Ok(())
}

async fn sync_push_db(
    local_db: &DatabaseConnection,
    local_repo: &tome_db::entities::repository::Model,
    peer: &tome_db::entities::sync_peer::Model,
    peer_repo_name: &str,
    source_machine_id: i16,
    peer_display_name: &str,
) -> Result<()> {
    let peer_db = open_db(&peer.url).await?;
    let remote_repo = ops::get_or_create_repository(&peer_db, peer_repo_name).await?;

    let new_snapshots = ops::snapshots_after(local_db, local_repo.id, peer.last_snapshot_id).await?;

    if new_snapshots.is_empty() {
        println!("already up to date (no new snapshots to push to {:?})", peer_display_name);
        return Ok(());
    }

    println!("pushing {} snapshot(s) to {:?} ...", new_snapshots.len(), peer_display_name);

    let mut blobs_created = 0u64;
    let mut entries_synced = 0u64;
    let mut replicas_synced = 0u64;
    let mut last_local_snapshot_id = peer.last_snapshot_id;

    for local_snap in &new_snapshots {
        let remote_parent = ops::latest_snapshot(&peer_db, remote_repo.id).await?.map(|s| s.id);
        let remote_snap = ops::create_snapshot_with_source(
            &peer_db,
            remote_repo.id,
            remote_parent,
            &local_snap.message,
            source_machine_id,
            local_snap.id,
        )
        .await?;

        let local_entries = ops::entries_in_snapshot(local_db, local_snap.id).await?;

        for local_entry in &local_entries {
            let remote_blob_id = if let Some(local_blob_id) = local_entry.blob_id {
                let local_blob = match ops::find_blob_by_id(local_db, local_blob_id).await? {
                    Some(b) => b,
                    None => {
                        warn!("blob {} not found locally", local_blob_id);
                        continue;
                    }
                };

                let remote_blob = if let Some(b) = ops::find_blob_by_digest(&peer_db, &local_blob.digest).await? {
                    b
                } else {
                    let fh = tome_core::hash::FileHash {
                        size: local_blob.size as u64,
                        fast_digest: local_blob.fast_digest,
                        digest: local_blob
                            .digest
                            .as_slice()
                            .try_into()
                            .context("invalid digest length in local blob")?,
                    };
                    let b = ops::get_or_create_blob(&peer_db, &fh).await?;
                    blobs_created += 1;
                    b
                };

                let local_replicas = ops::replicas_for_blob(local_db, local_blob_id).await?;
                for (lr, local_store) in &local_replicas {
                    let peer_store = ops::get_or_create_store(
                        &peer_db,
                        &local_store.name,
                        &local_store.url,
                        local_store.config.clone(),
                    )
                    .await?;
                    if !ops::replica_exists(&peer_db, remote_blob.id, peer_store.id).await? {
                        ops::insert_replica(&peer_db, remote_blob.id, peer_store.id, &lr.path, lr.encrypted).await?;
                        replicas_synced += 1;
                    }
                }

                Some(remote_blob.id)
            } else {
                None
            };

            if local_entry.status == 1 {
                if let Some(blob_id) = remote_blob_id {
                    ops::insert_entry_present(
                        &peer_db,
                        remote_snap.id,
                        &local_entry.path,
                        blob_id,
                        local_entry.mode,
                        local_entry.mtime,
                    )
                    .await?;
                }
            } else {
                ops::insert_entry_deleted(&peer_db, remote_snap.id, &local_entry.path).await?;
            }
            entries_synced += 1;
        }

        let meta = serde_json::json!({
            "pushed_from_machine_id": source_machine_id,
            "source_snapshot_id": local_snap.id,
            "entries": local_entries.len(),
        });
        ops::update_snapshot_metadata(&peer_db, remote_snap.id, meta).await?;

        last_local_snapshot_id = Some(local_snap.id);
        info!("pushed snapshot {} -> {} ({} entries)", local_snap.id, remote_snap.id, local_entries.len());
    }

    if let Some(last_id) = last_local_snapshot_id {
        ops::update_sync_peer_progress(local_db, peer.id, last_id).await?;
    }

    println!(
        "push complete: {} snapshot(s), {} entries, {} blobs created, {} replicas synced",
        new_snapshots.len(),
        entries_synced,
        blobs_created,
        replicas_synced
    );
    Ok(())
}
