use axum::{
    Json,
    extract::{Path, Query},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use utoipa::{IntoParams, ToSchema};

use tome_core::hash::hex_encode;
use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::{AppError, AppResult};

async fn find_repo_or_404(
    db: &dyn tome_db::store_trait::MetadataStore,
    name: &str,
) -> AppResult<tome_db::entities::repository::Model> {
    db.find_repository_by_name(name)
        .await?
        .ok_or_else(|| AppError::not_found(format!("repository {:?} not found", name)))
}

#[utoipa::path(
    get,
    path = "/repositories",
    responses(
        (status = 200, description = "List all repositories", body = Vec<RepositoryResponse>),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn list_repositories(db: Db) -> AppResult<Json<Vec<RepositoryResponse>>> {
    let repos = db.list_repositories().await?;
    Ok(Json(repos.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get,
    path = "/repositories/{name}",
    params(("name" = String, Path, description = "Repository name")),
    responses(
        (status = 200, description = "Repository details", body = RepositoryResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn get_repository(db: Db, Path(name): Path<String>) -> AppResult<Json<RepositoryResponse>> {
    let repo = find_repo_or_404(&**db, &name).await?;
    Ok(Json(repo.into()))
}

#[utoipa::path(
    get,
    path = "/repositories/{name}/snapshots",
    params(("name" = String, Path, description = "Repository name")),
    responses(
        (status = 200, description = "List snapshots for the repository", body = Vec<SnapshotResponse>),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn list_snapshots(db: Db, Path(name): Path<String>) -> AppResult<Json<Vec<SnapshotResponse>>> {
    let repo = find_repo_or_404(&**db, &name).await?;
    let snaps = db.list_snapshots_for_repo(repo.id).await?;
    Ok(Json(snaps.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    get,
    path = "/repositories/{name}/latest",
    params(("name" = String, Path, description = "Repository name")),
    responses(
        (status = 200, description = "Latest snapshot, or null if none", body = Option<SnapshotResponse>),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn get_latest_snapshot(db: Db, Path(name): Path<String>) -> AppResult<Json<Option<SnapshotResponse>>> {
    let repo = find_repo_or_404(&**db, &name).await?;
    let snap = db.latest_snapshot(repo.id).await?;
    Ok(Json(snap.map(Into::into)))
}

#[utoipa::path(
    get,
    path = "/repositories/{name}/history",
    params(
        ("name" = String, Path, description = "Repository name"),
        HistoryQuery,
    ),
    responses(
        (status = 200, description = "Snapshot+entry pairs for the given path", body = Vec<SnapshotEntry>),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn path_history(
    db: Db,
    Path(name): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Vec<SnapshotEntry>>> {
    let repo = find_repo_or_404(&**db, &name).await?;
    let history = db.path_history(repo.id, &q.path).await?;
    Ok(Json(
        history
            .into_iter()
            .map(|(e, b, s)| SnapshotEntry { snapshot: s.into(), entry: EntryResponse::from_with_blob(e, b.as_ref()) })
            .collect(),
    ))
}

#[derive(Deserialize, IntoParams)]
pub struct HistoryQuery {
    /// File path to retrieve history for.
    pub path: String,
}

// ── Diff ────────────────────────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
pub struct DiffQuery {
    /// First snapshot ID (decimal).
    pub snapshot1: String,
    /// Second snapshot ID (decimal).
    pub snapshot2: String,
    /// Optional path prefix filter.
    #[serde(default)]
    pub prefix: String,
}

#[derive(Serialize, ToSchema)]
pub struct DiffResponse {
    pub snapshot1: SnapshotResponse,
    pub snapshot2: SnapshotResponse,
    pub blobs: HashMap<String, BlobResponse>,
    pub entries: HashMap<String, EntryResponse>,
    /// blob_id → [entry_ids_in_snapshot1, entry_ids_in_snapshot2]
    #[schema(value_type = HashMap<String, Vec<Vec<String>>>)]
    pub diff: HashMap<String, (Vec<String>, Vec<String>)>,
}

#[utoipa::path(
    get,
    path = "/repositories/{name}/diff",
    params(
        ("name" = String, Path, description = "Repository name"),
        DiffQuery,
    ),
    responses(
        (status = 200, description = "Diff between two snapshots", body = DiffResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn diff_snapshots(
    db: Db,
    Path(name): Path<String>,
    Query(q): Query<DiffQuery>,
) -> AppResult<Json<DiffResponse>> {
    let repo = find_repo_or_404(&**db, &name).await?;

    let snap_id1: i64 = q.snapshot1.parse().map_err(|_| AppError::bad_request("invalid snapshot1 id"))?;
    let snap_id2: i64 = q.snapshot2.parse().map_err(|_| AppError::bad_request("invalid snapshot2 id"))?;

    let snap1 = db
        .find_snapshot_by_id(snap_id1)
        .await?
        .ok_or_else(|| AppError::not_found(format!("snapshot {} not found", snap_id1)))?;
    if snap1.repository_id != repo.id {
        return Err(AppError::bad_request("snapshot1 does not belong to this repository"));
    }

    let snap2 = db
        .find_snapshot_by_id(snap_id2)
        .await?
        .ok_or_else(|| AppError::not_found(format!("snapshot {} not found", snap_id2)))?;
    if snap2.repository_id != repo.id {
        return Err(AppError::bad_request("snapshot2 does not belong to this repository"));
    }

    let entries1 = db.entries_by_prefix(snap_id1, &q.prefix).await?;
    let entries2 = db.entries_by_prefix(snap_id2, &q.prefix).await?;

    let blob_ids: Vec<i64> =
        entries1.iter().chain(entries2.iter()).filter_map(|e| e.blob_id).collect::<HashSet<_>>().into_iter().collect();

    let blobs: HashMap<String, BlobResponse> =
        db.blobs_by_ids(&blob_ids).await?.into_iter().map(|b| (b.id.to_string(), b.into())).collect();

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

// ── Files ───────────────────────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
pub struct FilesQuery {
    /// Optional path prefix filter.
    #[serde(default)]
    pub prefix: String,
    /// Include deleted entries (status=0).
    #[serde(default)]
    pub include_deleted: bool,
    /// Page number (1-based, default 1).
    #[serde(default = "default_page")]
    pub page: u64,
    /// Items per page (1–500, default 100).
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}
fn default_per_page() -> u64 {
    100
}

#[derive(Serialize, ToSchema)]
pub struct FilesResponse {
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub items: Vec<CacheEntryResponse>,
}

#[utoipa::path(
    get,
    path = "/repositories/{name}/files",
    params(
        ("name" = String, Path, description = "Repository name"),
        FilesQuery,
    ),
    responses(
        (status = 200, description = "Paginated list of tracked files from entry_cache", body = FilesResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn list_files(
    db: Db,
    Path(name): Path<String>,
    Query(q): Query<FilesQuery>,
) -> AppResult<Json<FilesResponse>> {
    let repo = find_repo_or_404(&**db, &name).await?;

    let per_page = q.per_page.clamp(1, 500);
    let page = q.page.max(1);

    let (entries, total) = db
        .list_cache_entries(&ops::ListCacheEntriesParams {
            repository_id: repo.id,
            include_deleted: q.include_deleted,
            prefix: q.prefix,
            page,
            per_page,
        })
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

// ── Cross-repo diff ─────────────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
pub struct RepoDiffQuery {
    /// First repository name.
    pub repo1: String,
    /// Optional path prefix for repo1.
    #[serde(default)]
    pub prefix1: String,
    /// Second repository name.
    pub repo2: String,
    /// Optional path prefix for repo2.
    #[serde(default)]
    pub prefix2: String,
}

#[derive(Serialize, ToSchema)]
pub struct RepoDiffResponse {
    pub repo1: RepositoryResponse,
    pub repo2: RepositoryResponse,
    pub blobs: HashMap<String, BlobResponse>,
    /// `"1:{path}"` or `"2:{path}"` → CacheEntryResponse
    pub entries: HashMap<String, CacheEntryResponse>,
    /// blob_id → [entry_keys_in_repo1, entry_keys_in_repo2]
    #[schema(value_type = HashMap<String, Vec<Vec<String>>>)]
    pub diff: HashMap<String, (Vec<String>, Vec<String>)>,
    /// Entry keys for deleted paths (status=0, blob_id=NULL)
    pub deleted: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/diff",
    params(RepoDiffQuery),
    responses(
        (status = 200, description = "Cross-repository diff from entry_cache", body = RepoDiffResponse),
        (status = 400, description = "Too many entries", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "repositories"
)]
pub async fn diff_repos(db: Db, Query(q): Query<RepoDiffQuery>) -> AppResult<Json<RepoDiffResponse>> {
    let repo1 = find_repo_or_404(&**db, &q.repo1).await?;
    let repo2 = find_repo_or_404(&**db, &q.repo2).await?;

    let entries1 = db.cache_entries_by_prefix(repo1.id, &q.prefix1, true).await?;
    let entries2 = db.cache_entries_by_prefix(repo2.id, &q.prefix2, true).await?;

    const MAX_ENTRIES: usize = 10_000;
    if entries1.len() > MAX_ENTRIES || entries2.len() > MAX_ENTRIES {
        return Err(AppError::bad_request(format!("too many entries (limit {}), narrow the prefix", MAX_ENTRIES)));
    }

    let blob_ids: Vec<i64> =
        entries1.iter().chain(entries2.iter()).filter_map(|e| e.blob_id).collect::<HashSet<_>>().into_iter().collect();

    let blobs: HashMap<String, BlobResponse> =
        db.blobs_by_ids(&blob_ids).await?.into_iter().map(|b| (b.id.to_string(), b.into())).collect();

    let mut entries: HashMap<String, CacheEntryResponse> = HashMap::new();
    let mut diff: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    let mut deleted: Vec<String> = Vec::new();

    for e in &entries1 {
        let key = format!("1:{}", e.path);
        match e.blob_id {
            Some(blob_id) => diff.entry(blob_id.to_string()).or_default().0.push(key.clone()),
            None => deleted.push(key.clone()),
        }
        entries.insert(key, cache_entry_to_response(e));
    }
    for e in &entries2 {
        let key = format!("2:{}", e.path);
        match e.blob_id {
            Some(blob_id) => diff.entry(blob_id.to_string()).or_default().1.push(key.clone()),
            None => deleted.push(key.clone()),
        }
        entries.insert(key, cache_entry_to_response(e));
    }

    Ok(Json(RepoDiffResponse { repo1: repo1.into(), repo2: repo2.into(), blobs, entries, diff, deleted }))
}
