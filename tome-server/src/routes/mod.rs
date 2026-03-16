pub(crate) mod admin;
pub(crate) mod machines;
pub(crate) mod objects;
pub(crate) mod repositories;
pub mod responses;
pub(crate) mod snapshots;
pub mod sync;

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};

use tome_db::store_trait::MetadataStore;

pub type Db = State<Arc<dyn MetadataStore>>;

// ── Root / Health ───────────────────────────────────────────────────────────

pub async fn index() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "service": "tome-server",
            "endpoints": ["/health", "/repositories", "/snapshots/:id/entries", "/objects/:digest"],
        })),
    )
}

pub async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

// ── Re-exports ──────────────────────────────────────────────────────────────

pub use admin::{list_all_sync_peers, list_all_tags, list_stores};
pub use machines::{list_machines, register_machine, update_machine};
pub use objects::{get_object, list_object_entries};
pub use repositories::{
    diff_repos, diff_snapshots, get_latest_snapshot, get_repository, list_files, list_repositories, list_snapshots,
    path_history,
};
pub use snapshots::list_entries;
