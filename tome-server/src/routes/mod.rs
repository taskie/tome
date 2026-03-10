pub(crate) mod admin;
pub(crate) mod blobs;
pub(crate) mod machines;
pub(crate) mod repositories;
pub mod responses;
pub(crate) mod snapshots;
pub mod sync;

use axum::{Json, extract::State, http::StatusCode};
use sea_orm::DatabaseConnection;

pub type Db = State<DatabaseConnection>;

// ── Root / Health ───────────────────────────────────────────────────────────

pub async fn index() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "service": "tome-server",
            "endpoints": ["/health", "/repositories", "/snapshots/:id/entries", "/blobs/:digest"],
        })),
    )
}

pub async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

// ── Re-exports ──────────────────────────────────────────────────────────────

pub use admin::{list_all_sync_peers, list_all_tags, list_stores};
pub use blobs::{get_blob, list_blob_entries};
pub use machines::{list_machines, register_machine, update_machine};
pub use repositories::{
    diff_repos, diff_snapshots, get_latest_snapshot, get_repository, list_files, list_repositories, list_snapshots,
    path_history,
};
pub use snapshots::list_entries;
