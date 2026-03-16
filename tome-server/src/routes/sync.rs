use std::collections::HashMap;

use axum::{Json, extract::Query};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use tome_core::hash::{FileHash, hex_encode};

use super::Db;
use crate::error::{AppError, AppResult};

// ── Shared protocol types ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SyncEntry {
    pub path: String,
    /// 0 = deleted, 1 = present
    pub status: i16,
    pub blob_digest: Option<String>,
    pub blob_size: Option<i64>,
    pub blob_fast_digest: Option<i64>,
    pub mode: Option<i32>,
    /// RFC 3339
    pub mtime: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SyncReplica {
    pub blob_digest: String,
    pub store_name: String,
    pub store_url: String,
    /// Blob path within the store (e.g. objects/de/ad/deadbeef…)
    pub path: String,
    pub encrypted: bool,
}

// ── Pull ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
pub struct PullQuery {
    /// Repository name.
    pub repo: String,
    /// Decimal snapshot ID; return only snapshots created after this one.
    pub after: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SyncSnapshot {
    pub id: String,
    pub parent_id: Option<String>,
    pub message: String,
    pub metadata: serde_json::Value,
    pub source_machine_id: Option<i16>,
    pub source_snapshot_id: Option<String>,
    pub created_at: String,
    pub entries: Vec<SyncEntry>,
    pub replicas: Vec<SyncReplica>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct PullResponse {
    pub snapshots: Vec<SyncSnapshot>,
}

#[utoipa::path(
    get,
    path = "/sync/pull",
    params(PullQuery),
    responses(
        (status = 200, description = "Incremental snapshots since `after`", body = PullResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse),
    ),
    tag = "sync"
)]
pub async fn pull(db: Db, Query(q): Query<PullQuery>) -> AppResult<Json<PullResponse>> {
    let repo = find_repo_or_404(&**db, &q.repo).await?;

    let after_id: Option<i64> = q.after.as_deref().map(|s| s.parse()).transpose()?;
    let snapshots = db.snapshots_after(repo.id, after_id).await?;

    let mut result = Vec::with_capacity(snapshots.len());

    for snap in snapshots {
        // entries with blob (LEFT JOIN)
        let pairs = db.entries_with_digest(snap.id, "").await?;

        // batch-fetch replicas for all blobs in this snapshot
        let blob_ids: Vec<i64> = pairs.iter().filter_map(|(_, b)| b.as_ref().map(|b| b.id)).collect();
        let all_replicas = db.replicas_for_objects(&blob_ids).await?;

        // build a digest → [SyncReplica] map
        let blob_digest_map: HashMap<i64, String> =
            pairs.iter().filter_map(|(_, b)| b.as_ref().map(|b| (b.id, hex_encode(&b.digest)))).collect();

        let mut replica_map: HashMap<i64, Vec<SyncReplica>> = HashMap::new();
        for (replica, store) in all_replicas {
            if let Some(digest) = blob_digest_map.get(&replica.object_id) {
                replica_map.entry(replica.object_id).or_default().push(SyncReplica {
                    blob_digest: digest.clone(),
                    store_name: store.name,
                    store_url: store.url,
                    path: replica.path,
                    encrypted: replica.encrypted,
                });
            }
        }

        let mut entries = Vec::with_capacity(pairs.len());
        let mut replicas: Vec<SyncReplica> = Vec::new();

        for (entry, blob) in pairs {
            entries.push(SyncEntry {
                path: entry.path,
                status: entry.status,
                blob_digest: blob.as_ref().map(|b| hex_encode(&b.digest)),
                blob_size: blob.as_ref().and_then(|b| b.size),
                blob_fast_digest: blob.as_ref().and_then(|b| b.fast_digest),
                mode: entry.mode,
                mtime: entry.mtime.map(|t| t.to_rfc3339()),
            });
            if let Some(b) = &blob {
                if let Some(reps) = replica_map.get(&b.id) {
                    replicas.extend(reps.iter().map(|r| SyncReplica {
                        blob_digest: r.blob_digest.clone(),
                        store_name: r.store_name.clone(),
                        store_url: r.store_url.clone(),
                        path: r.path.clone(),
                        encrypted: r.encrypted,
                    }));
                }
            }
        }

        // deduplicate replicas (same blob may appear in multiple entries)
        replicas.sort_by(|a, b| a.blob_digest.cmp(&b.blob_digest).then(a.path.cmp(&b.path)));
        replicas.dedup_by(|a, b| a.blob_digest == b.blob_digest && a.path == b.path);

        result.push(SyncSnapshot {
            id: snap.id.to_string(),
            parent_id: snap.parent_id.map(|id| id.to_string()),
            message: snap.message,
            metadata: snap.metadata,
            source_machine_id: snap.source_machine_id,
            source_snapshot_id: snap.source_snapshot_id.map(|id| id.to_string()),
            created_at: snap.created_at.to_rfc3339(),
            entries,
            replicas,
        });
    }

    Ok(Json(PullResponse { snapshots: result }))
}

// ── Push ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
pub struct PushQuery {
    /// Repository name.
    pub repo: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct PushRequest {
    pub source_machine_id: Option<i16>,
    /// Decimal snapshot ID from the source machine (used as idempotency key).
    pub source_snapshot_id: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub entries: Vec<SyncEntry>,
    pub replicas: Vec<SyncReplica>,
}

#[derive(Serialize, ToSchema)]
pub struct PushResponse {
    /// Server-side snapshot ID assigned to this push.
    pub snapshot_id: String,
}

#[utoipa::path(
    post,
    path = "/sync/push",
    params(PushQuery),
    request_body = PushRequest,
    responses(
        (status = 200, description = "Snapshot created (or existing returned for idempotent re-push)", body = PushResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
    ),
    tag = "sync"
)]
pub async fn push(db: Db, Query(q): Query<PushQuery>, Json(body): Json<PushRequest>) -> AppResult<Json<PushResponse>> {
    let repo = db.get_or_create_repository(&q.repo).await?;

    // Idempotency: if we already have a snapshot from this source, return it.
    if let (Some(mid), Some(sid_str)) = (body.source_machine_id, &body.source_snapshot_id) {
        let sid: i64 = sid_str.parse()?;
        if let Some(existing) = db.find_snapshot_by_source(repo.id, mid, sid).await? {
            return Ok(Json(PushResponse { snapshot_id: existing.id.to_string() }));
        }
    }

    let parent = db.latest_snapshot(repo.id).await?.map(|s| s.id);

    let snap = if let (Some(mid), Some(sid_str)) = (body.source_machine_id, &body.source_snapshot_id) {
        let sid: i64 = sid_str.parse()?;
        db.create_snapshot_with_source(repo.id, parent, &body.message, mid, sid).await?
    } else {
        db.create_snapshot(repo.id, parent, &body.message).await?
    };

    // Insert entries and update entry_cache.
    for e in &body.entries {
        if e.status == 1 {
            if let (Some(hex), Some(size), Some(fast)) = (&e.blob_digest, e.blob_size, e.blob_fast_digest) {
                let digest_bytes = hex::decode(hex)?;
                let digest_arr: [u8; 32] = digest_bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| AppError::bad_request(format!("invalid digest length for {hex}")))?;
                let fh = FileHash { size: size as u64, fast_digest: fast, digest: digest_arr };
                let blob = db.get_or_create_blob(&fh).await?;

                let mtime =
                    e.mtime.as_deref().map(|s| s.parse::<chrono::DateTime<chrono::FixedOffset>>()).transpose()?;
                let entry = db.insert_entry_present(snap.id, &e.path, blob.id, e.mode, mtime).await?;

                db.upsert_cache_present(tome_db::ops::UpsertCachePresentParams {
                    repository_id: repo.id,
                    path: e.path.clone(),
                    snapshot_id: snap.id,
                    entry_id: entry.id,
                    object_id: blob.id,
                    mtime,
                    digest: Some(blob.digest.clone()),
                    size: blob.size,
                    fast_digest: blob.fast_digest,
                })
                .await?;
            }
        } else {
            let entry = db.insert_entry_deleted(snap.id, &e.path).await?;
            db.upsert_cache_deleted(repo.id, &e.path, snap.id, entry.id).await?;
        }
    }

    // Upsert stores and replicas.
    for r in &body.replicas {
        let store = db.get_or_create_store(&r.store_name, &r.store_url, serde_json::json!({})).await?;
        let digest_bytes = hex::decode(&r.blob_digest)?;
        if let Some(blob) = db.find_object_by_digest(&digest_bytes).await? {
            if !db.replica_exists(blob.id, store.id).await? {
                db.insert_replica(blob.id, store.id, &r.path, r.encrypted).await?;
            }
        }
    }

    if !body.metadata.is_null() {
        db.update_snapshot_metadata(snap.id, body.metadata).await?;
    }

    Ok(Json(PushResponse { snapshot_id: snap.id.to_string() }))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn find_repo_or_404(
    db: &dyn tome_db::store_trait::MetadataStore,
    name: &str,
) -> AppResult<tome_db::entities::repository::Model> {
    db.find_repository_by_name(name)
        .await?
        .ok_or_else(|| AppError::not_found(format!("repository {:?} not found", name)))
}
