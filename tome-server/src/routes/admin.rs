use axum::Json;

use super::Db;
use super::responses::*;
use crate::error::AppResult;

#[utoipa::path(
    get,
    path = "/stores",
    responses(
        (status = 200, description = "List all stores", body = Vec<StoreResponse>),
    ),
    tag = "admin"
)]
pub async fn list_stores(db: Db) -> AppResult<Json<Vec<StoreResponse>>> {
    let stores = db.list_stores().await?;
    Ok(Json(stores.into_iter().map(StoreResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/tags",
    responses(
        (status = 200, description = "List all blob tags", body = Vec<TagResponse>),
    ),
    tag = "admin"
)]
pub async fn list_all_tags(db: Db) -> AppResult<Json<Vec<TagResponse>>> {
    let tags = db.list_all_tags().await?;
    Ok(Json(tags.into_iter().map(TagResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/sync-peers",
    responses(
        (status = 200, description = "List all sync peers", body = Vec<SyncPeerResponse>),
    ),
    tag = "admin"
)]
pub async fn list_all_sync_peers(db: Db) -> AppResult<Json<Vec<SyncPeerResponse>>> {
    let peers = db.list_all_sync_peers().await?;
    Ok(Json(peers.into_iter().map(SyncPeerResponse::from).collect()))
}
