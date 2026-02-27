use axum::{Router, routing::get};
use sea_orm::DatabaseConnection;
use tracing::info;

use crate::routes;

pub async fn serve(db: DatabaseConnection, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(routes::index))
        .route("/health", get(routes::health))
        .route("/repositories", get(routes::list_repositories))
        .route("/repositories/:name", get(routes::get_repository))
        .route("/repositories/:name/snapshots", get(routes::list_snapshots))
        .route("/repositories/:name/latest", get(routes::get_latest_snapshot))
        .route("/repositories/:name/diff", get(routes::diff_snapshots))
        .route("/snapshots/:id/entries", get(routes::list_entries))
        .route("/blobs/:digest", get(routes::get_blob))
        .with_state(db);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("tome-server listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
