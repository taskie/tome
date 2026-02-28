use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use tome_core::hash::hex_encode;
use tome_db::{
    entities::{blob, entry, repository, snapshot},
    ops,
};

use crate::error::AppResult;

pub type Db = State<DatabaseConnection>;

// ──────────────────────────────────────────────────────────────────────────────
// Response types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RepositoryResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<repository::Model> for RepositoryResponse {
    fn from(m: repository::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            description: m.description,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub struct SnapshotResponse {
    pub id: String,
    pub repository_id: String,
    pub parent_id: Option<String>,
    pub message: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

impl From<snapshot::Model> for SnapshotResponse {
    fn from(m: snapshot::Model) -> Self {
        Self {
            id: m.id.to_string(),
            repository_id: m.repository_id.to_string(),
            parent_id: m.parent_id.map(|id| id.to_string()),
            message: m.message,
            metadata: m.metadata,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub struct EntryResponse {
    pub id: String,
    pub snapshot_id: String,
    pub path: String,
    pub status: i16,
    pub blob_id: Option<String>,
    pub digest: Option<String>,
    pub mode: Option<i32>,
    pub mtime: Option<String>,
    pub created_at: String,
}

impl EntryResponse {
    fn from_with_blob(e: entry::Model, blob: Option<&blob::Model>) -> Self {
        Self {
            id: e.id.to_string(),
            snapshot_id: e.snapshot_id.to_string(),
            path: e.path,
            status: e.status,
            blob_id: e.blob_id.map(|id| id.to_string()),
            digest: blob.map(|b| hex_encode(&b.digest)),
            mode: e.mode,
            mtime: e.mtime.map(|t| t.to_rfc3339()),
            created_at: e.created_at.to_rfc3339(),
        }
    }
}

impl From<entry::Model> for EntryResponse {
    fn from(m: entry::Model) -> Self {
        Self::from_with_blob(m, None)
    }
}

#[derive(Serialize)]
pub struct BlobResponse {
    pub id: String,
    pub digest: String,
    pub size: i64,
    pub fast_digest: String,
    pub created_at: String,
}

impl From<blob::Model> for BlobResponse {
    fn from(m: blob::Model) -> Self {
        Self {
            id: m.id.to_string(),
            digest: hex_encode(&m.digest),
            size: m.size,
            fast_digest: format!("{:016x}", m.fast_digest as u64),
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /
// ──────────────────────────────────────────────────────────────────────────────

pub async fn index() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "service": "tome-server",
            "endpoints": ["/health", "/repositories", "/snapshots/:id/entries", "/blobs/:digest"],
        })),
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /health
// ──────────────────────────────────────────────────────────────────────────────

pub async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /repositories
// ──────────────────────────────────────────────────────────────────────────────

pub async fn list_repositories(db: Db) -> AppResult<Json<Vec<RepositoryResponse>>> {
    let repos = ops::list_repositories(&db).await?;
    Ok(Json(repos.into_iter().map(Into::into).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /repositories/:name
// ──────────────────────────────────────────────────────────────────────────────

pub async fn get_repository(db: Db, Path(name): Path<String>) -> AppResult<Json<RepositoryResponse>> {
    let repo = repository::Entity::find()
        .filter(repository::Column::Name.eq(&name))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", name))?;
    Ok(Json(repo.into()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /repositories/:name/snapshots
// ──────────────────────────────────────────────────────────────────────────────

pub async fn list_snapshots(db: Db, Path(name): Path<String>) -> AppResult<Json<Vec<SnapshotResponse>>> {
    let repo = repository::Entity::find()
        .filter(repository::Column::Name.eq(&name))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", name))?;

    let snaps = snapshot::Entity::find()
        .filter(snapshot::Column::RepositoryId.eq(repo.id))
        .order_by_asc(snapshot::Column::CreatedAt)
        .all(&*db)
        .await?;

    Ok(Json(snaps.into_iter().map(Into::into).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /snapshots/:id/entries
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EntriesQuery {
    #[serde(default)]
    pub prefix: String,
}

pub async fn list_entries(
    db: Db,
    Path(id): Path<String>,
    Query(q): Query<EntriesQuery>,
) -> AppResult<Json<Vec<EntryResponse>>> {
    let snapshot_id: i64 = id.parse().map_err(|_| anyhow::anyhow!("invalid snapshot id"))?;
    let pairs = ops::entries_with_digest(&db, snapshot_id, &q.prefix).await?;
    Ok(Json(pairs.into_iter().map(|(e, b)| EntryResponse::from_with_blob(e, b.as_ref())).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /blobs/:digest
// ──────────────────────────────────────────────────────────────────────────────

pub async fn get_blob(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<BlobResponse>> {
    let digest = hex::decode(&digest_hex).map_err(|_| anyhow::anyhow!("invalid digest hex"))?;
    let blob = ops::find_blob_by_digest(&db, &digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob {:?} not found", digest_hex))?;
    Ok(Json(blob.into()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /repositories/:name/latest
// ──────────────────────────────────────────────────────────────────────────────

pub async fn get_latest_snapshot(db: Db, Path(name): Path<String>) -> AppResult<Json<Option<SnapshotResponse>>> {
    let repo = repository::Entity::find()
        .filter(repository::Column::Name.eq(&name))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", name))?;

    let snap = ops::latest_snapshot(&db, repo.id).await?;
    Ok(Json(snap.map(Into::into)))
}

// ──────────────────────────────────────────────────────────────────────────────
// Shared: snapshot + entry pair
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SnapshotEntry {
    pub snapshot: SnapshotResponse,
    pub entry: EntryResponse,
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /repositories/:name/history?path=
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub path: String,
}

pub async fn path_history(
    db: Db,
    Path(name): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Vec<SnapshotEntry>>> {
    let repo = repository::Entity::find()
        .filter(repository::Column::Name.eq(&name))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", name))?;

    let history = ops::path_history(&db, repo.id, &q.path).await?;
    Ok(Json(history.into_iter().map(|(e, s)| SnapshotEntry { snapshot: s.into(), entry: e.into() }).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /blobs/:digest/entries
// ──────────────────────────────────────────────────────────────────────────────

pub async fn list_blob_entries(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<Vec<SnapshotEntry>>> {
    let digest = hex::decode(&digest_hex).map_err(|_| anyhow::anyhow!("invalid digest hex"))?;
    let blob = ops::find_blob_by_digest(&db, &digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob {:?} not found", digest_hex))?;
    let entries = ops::entries_for_blob(&db, blob.id).await?;
    Ok(Json(entries.into_iter().map(|(e, s)| SnapshotEntry { snapshot: s.into(), entry: e.into() }).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /repositories/:name/diff
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DiffQuery {
    pub snapshot1: String,
    pub snapshot2: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Serialize)]
pub struct DiffResponse {
    pub snapshot1: SnapshotResponse,
    pub snapshot2: SnapshotResponse,
    pub blobs: HashMap<String, BlobResponse>,
    pub entries: HashMap<String, EntryResponse>,
    /// blob_id → (entry_ids_in_snapshot1, entry_ids_in_snapshot2)
    pub diff: HashMap<String, (Vec<String>, Vec<String>)>,
}

pub async fn diff_snapshots(
    db: Db,
    Path(name): Path<String>,
    Query(q): Query<DiffQuery>,
) -> AppResult<Json<DiffResponse>> {
    let repo = repository::Entity::find()
        .filter(repository::Column::Name.eq(&name))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", name))?;

    let snap_id1: i64 = q.snapshot1.parse().map_err(|_| anyhow::anyhow!("invalid snapshot1 id"))?;
    let snap_id2: i64 = q.snapshot2.parse().map_err(|_| anyhow::anyhow!("invalid snapshot2 id"))?;

    let snap1 = snapshot::Entity::find_by_id(snap_id1)
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("snapshot {} not found", snap_id1))?;
    if snap1.repository_id != repo.id {
        return Err(anyhow::anyhow!("snapshot1 does not belong to this repository").into());
    }

    let snap2 = snapshot::Entity::find_by_id(snap_id2)
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("snapshot {} not found", snap_id2))?;
    if snap2.repository_id != repo.id {
        return Err(anyhow::anyhow!("snapshot2 does not belong to this repository").into());
    }

    let entries1 = ops::entries_by_prefix(&db, snap_id1, &q.prefix).await?;
    let entries2 = ops::entries_by_prefix(&db, snap_id2, &q.prefix).await?;

    let blob_ids: Vec<i64> =
        entries1.iter().chain(entries2.iter()).filter_map(|e| e.blob_id).collect::<HashSet<_>>().into_iter().collect();

    let blobs: HashMap<String, BlobResponse> =
        ops::blobs_by_ids(&db, &blob_ids).await?.into_iter().map(|b| (b.id.to_string(), b.into())).collect();

    let mut entries: HashMap<String, EntryResponse> = HashMap::new();
    let mut diff: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();

    for e in &entries1 {
        let eid = e.id.to_string();
        let key = e.blob_id.map(|id| id.to_string()).unwrap_or_default();
        diff.entry(key).or_default().0.push(eid.clone());
        entries.insert(eid, e.clone().into());
    }
    for e in &entries2 {
        let eid = e.id.to_string();
        let key = e.blob_id.map(|id| id.to_string()).unwrap_or_default();
        diff.entry(key).or_default().1.push(eid.clone());
        entries.insert(eid, e.clone().into());
    }

    Ok(Json(DiffResponse { snapshot1: snap1.into(), snapshot2: snap2.into(), blobs, entries, diff }))
}

// ── GET /diff ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RepoDiffQuery {
    pub repo1: String,
    #[serde(default)]
    pub prefix1: String,
    pub repo2: String,
    #[serde(default)]
    pub prefix2: String,
}

#[derive(Serialize)]
pub struct RepoDiffResponse {
    pub repo1: RepositoryResponse,
    pub repo2: RepositoryResponse,
    pub blobs: HashMap<String, BlobResponse>,
    /// "1:{path}" or "2:{path}" → CacheEntryResponse
    pub entries: HashMap<String, CacheEntryResponse>,
    /// blob_id → ([entry_keys_in_repo1], [entry_keys_in_repo2])
    pub diff: HashMap<String, (Vec<String>, Vec<String>)>,
}

pub async fn diff_repos(db: Db, Query(q): Query<RepoDiffQuery>) -> AppResult<Json<RepoDiffResponse>> {
    let repo1 = repository::Entity::find()
        .filter(repository::Column::Name.eq(&q.repo1))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", q.repo1))?;
    let repo2 = repository::Entity::find()
        .filter(repository::Column::Name.eq(&q.repo2))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", q.repo2))?;

    let entries1 = ops::cache_entries_by_prefix(&db, repo1.id, &q.prefix1).await?;
    let entries2 = ops::cache_entries_by_prefix(&db, repo2.id, &q.prefix2).await?;

    const MAX_ENTRIES: usize = 10_000;
    if entries1.len() > MAX_ENTRIES || entries2.len() > MAX_ENTRIES {
        return Err(anyhow::anyhow!("too many entries (limit {}), narrow the prefix", MAX_ENTRIES).into());
    }

    let blob_ids: Vec<i64> =
        entries1.iter().chain(entries2.iter()).filter_map(|e| e.blob_id).collect::<HashSet<_>>().into_iter().collect();

    let blobs: HashMap<String, BlobResponse> =
        ops::blobs_by_ids(&db, &blob_ids).await?.into_iter().map(|b| (b.id.to_string(), b.into())).collect();

    let mut entries: HashMap<String, CacheEntryResponse> = HashMap::new();
    let mut diff: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();

    for e in &entries1 {
        let key = format!("1:{}", e.path);
        if let Some(blob_id) = e.blob_id {
            diff.entry(blob_id.to_string()).or_default().0.push(key.clone());
        }
        entries.insert(key, cache_entry_to_response(e));
    }
    for e in &entries2 {
        let key = format!("2:{}", e.path);
        if let Some(blob_id) = e.blob_id {
            diff.entry(blob_id.to_string()).or_default().1.push(key.clone());
        }
        entries.insert(key, cache_entry_to_response(e));
    }

    Ok(Json(RepoDiffResponse { repo1: repo1.into(), repo2: repo2.into(), blobs, entries, diff }))
}

fn cache_entry_to_response(e: &tome_db::entities::entry_cache::Model) -> CacheEntryResponse {
    CacheEntryResponse {
        path: e.path.clone(),
        status: e.status,
        size: e.size,
        mtime: e.mtime.map(|t| t.to_rfc3339()),
        digest: e.digest.as_deref().map(hex_encode),
        fast_digest: e.fast_digest.map(|fd| format!("{:016x}", fd as u64)),
        snapshot_id: e.snapshot_id.to_string(),
        entry_id: e.entry_id.to_string(),
    }
}

// ── GET /repositories/:name/files ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FilesQuery {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub include_deleted: bool,
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}
fn default_per_page() -> u64 {
    100
}

#[derive(Serialize)]
pub struct CacheEntryResponse {
    pub path: String,
    pub status: i16,
    pub size: Option<i64>,
    pub mtime: Option<String>,
    pub digest: Option<String>,
    pub fast_digest: Option<String>,
    pub snapshot_id: String,
    pub entry_id: String,
}

#[derive(Serialize)]
pub struct FilesResponse {
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub items: Vec<CacheEntryResponse>,
}

pub async fn list_files(
    db: Db,
    Path(name): Path<String>,
    Query(q): Query<FilesQuery>,
) -> AppResult<Json<FilesResponse>> {
    let repo = repository::Entity::find()
        .filter(repository::Column::Name.eq(&name))
        .one(&*db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("repository {:?} not found", name))?;

    let per_page = q.per_page.clamp(1, 500);
    let page = q.page.max(1);

    let (entries, total) = ops::list_cache_entries(
        &db,
        &ops::ListCacheEntriesParams {
            repository_id: repo.id,
            include_deleted: q.include_deleted,
            prefix: q.prefix,
            page,
            per_page,
        },
    )
    .await?;

    let items = entries
        .into_iter()
        .map(|e| CacheEntryResponse {
            path: e.path,
            status: e.status,
            size: e.size,
            mtime: e.mtime.map(|t| t.to_rfc3339()),
            digest: e.digest.as_deref().map(hex_encode),
            fast_digest: e.fast_digest.map(|fd| format!("{:016x}", fd as u64)),
            snapshot_id: e.snapshot_id.to_string(),
            entry_id: e.entry_id.to_string(),
        })
        .collect();

    Ok(Json(FilesResponse { total, page, per_page, items }))
}

// ──────────────────────────────────────────────────────────────────────────────
// Machine endpoints
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MachineResponse {
    pub machine_id: i16,
    pub name: String,
    pub description: String,
    pub last_seen_at: Option<String>,
    pub created_at: String,
}

impl From<tome_db::entities::machine::Model> for MachineResponse {
    fn from(m: tome_db::entities::machine::Model) -> Self {
        Self {
            machine_id: m.machine_id,
            name: m.name,
            description: m.description,
            last_seen_at: m.last_seen_at.map(|t| t.to_rfc3339()),
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Deserialize)]
pub struct RegisterMachineRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

pub async fn list_machines(db: Db) -> AppResult<Json<Vec<MachineResponse>>> {
    let machines = ops::list_machines(&db).await?;
    Ok(Json(machines.into_iter().map(MachineResponse::from).collect()))
}

pub async fn register_machine(db: Db, Json(req): Json<RegisterMachineRequest>) -> AppResult<Json<MachineResponse>> {
    let machine = ops::register_machine(&db, &req.name, &req.description).await?;
    Ok(Json(MachineResponse::from(machine)))
}

pub async fn update_machine(db: Db, Path(id): Path<i16>) -> AppResult<Json<MachineResponse>> {
    ops::update_machine_last_seen(&db, id).await?;
    let machine = ops::find_machine_by_id(&db, id).await?.ok_or_else(|| anyhow::anyhow!("machine {} not found", id))?;
    Ok(Json(MachineResponse::from(machine)))
}

// ──────────────────────────────────────────────────────────────────────────────
// Store endpoints
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StoreResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<tome_db::entities::store::Model> for StoreResponse {
    fn from(m: tome_db::entities::store::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            url: m.url,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

pub async fn list_stores(db: Db) -> AppResult<Json<Vec<StoreResponse>>> {
    let stores = ops::list_stores(&db).await?;
    Ok(Json(stores.into_iter().map(StoreResponse::from).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// Tag endpoints
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TagResponse {
    pub id: String,
    pub blob_id: String,
    pub key: String,
    pub value: Option<String>,
    pub created_at: String,
}

impl From<tome_db::entities::tag::Model> for TagResponse {
    fn from(m: tome_db::entities::tag::Model) -> Self {
        Self {
            id: m.id.to_string(),
            blob_id: m.blob_id.to_string(),
            key: m.key,
            value: m.value,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

pub async fn list_all_tags(db: Db) -> AppResult<Json<Vec<TagResponse>>> {
    let tags = ops::list_all_tags(&db).await?;
    Ok(Json(tags.into_iter().map(TagResponse::from).collect()))
}

// ──────────────────────────────────────────────────────────────────────────────
// SyncPeer endpoints
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SyncPeerResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub repository_id: String,
    pub last_synced_at: Option<String>,
    pub last_snapshot_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<tome_db::entities::sync_peer::Model> for SyncPeerResponse {
    fn from(m: tome_db::entities::sync_peer::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            url: m.url,
            repository_id: m.repository_id.to_string(),
            last_synced_at: m.last_synced_at.map(|t| t.to_rfc3339()),
            last_snapshot_id: m.last_snapshot_id.map(|id| id.to_string()),
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

pub async fn list_all_sync_peers(db: Db) -> AppResult<Json<Vec<SyncPeerResponse>>> {
    let peers = ops::list_all_sync_peers(&db).await?;
    Ok(Json(peers.into_iter().map(SyncPeerResponse::from).collect()))
}
