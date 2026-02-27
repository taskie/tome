use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;

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
    pub mode: Option<i32>,
    pub mtime: Option<String>,
    pub created_at: String,
}

impl From<entry::Model> for EntryResponse {
    fn from(m: entry::Model) -> Self {
        Self {
            id: m.id.to_string(),
            snapshot_id: m.snapshot_id.to_string(),
            path: m.path,
            status: m.status,
            blob_id: m.blob_id.map(|id| id.to_string()),
            mode: m.mode,
            mtime: m.mtime.map(|t| t.to_rfc3339()),
            created_at: m.created_at.to_rfc3339(),
        }
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
    (StatusCode::OK, Json(serde_json::json!({
        "service": "tome-server",
        "endpoints": ["/health", "/repositories", "/snapshots/:id/entries", "/blobs/:digest"],
    })))
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

pub async fn list_entries(db: Db, Path(id): Path<String>) -> AppResult<Json<Vec<EntryResponse>>> {
    let snapshot_id: i64 = id.parse().map_err(|_| anyhow::anyhow!("invalid snapshot id"))?;
    let entries = ops::entries_in_snapshot(&db, snapshot_id).await?;
    Ok(Json(entries.into_iter().map(Into::into).collect()))
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
