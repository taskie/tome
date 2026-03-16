use std::sync::Arc;

use axum::{
    Json, Router,
    routing::{get, post, put},
};
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi as _;

use tome_db::store_trait::MetadataStore;

use crate::openapi::ApiDoc;
use crate::routes;

pub fn build_router(store: Arc<dyn MetadataStore>) -> Router {
    Router::new()
        .route("/", get(routes::index))
        .route("/health", get(routes::health))
        .route("/repositories", get(routes::list_repositories))
        .route("/repositories/{name}", get(routes::get_repository))
        .route("/repositories/{name}/snapshots", get(routes::list_snapshots))
        .route("/repositories/{name}/latest", get(routes::get_latest_snapshot))
        .route("/repositories/{name}/diff", get(routes::diff_snapshots))
        .route("/repositories/{name}/files", get(routes::list_files))
        .route("/repositories/{name}/history", get(routes::path_history))
        .route("/diff", get(routes::diff_repos))
        .route("/objects/{digest}/entries", get(routes::list_object_entries))
        .route("/snapshots/{id}/entries", get(routes::list_entries))
        .route("/objects/{digest}", get(routes::get_object))
        .route("/machines", get(routes::list_machines).post(routes::register_machine))
        .route("/machines/{id}", put(routes::update_machine))
        .route("/stores", get(routes::list_stores))
        .route("/tags", get(routes::list_all_tags))
        .route("/sync-peers", get(routes::list_all_sync_peers))
        .route("/sync/pull", get(routes::sync::pull))
        .route("/sync/push", post(routes::sync::push))
        .route("/openapi.json", get(openapi_json))
        .layer(TraceLayer::new_for_http())
        .with_state(store)
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

pub async fn serve(store: Arc<dyn MetadataStore>, addr: &str) -> anyhow::Result<()> {
    let app = build_router(store);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("tome-server listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
