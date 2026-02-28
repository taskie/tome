use axum::Json;

use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::AppResult;

pub async fn list_stores(db: Db) -> AppResult<Json<Vec<StoreResponse>>> {
    let stores = ops::list_stores(&db).await?;
    Ok(Json(stores.into_iter().map(StoreResponse::from).collect()))
}

pub async fn list_all_tags(db: Db) -> AppResult<Json<Vec<TagResponse>>> {
    let tags = ops::list_all_tags(&db).await?;
    Ok(Json(tags.into_iter().map(TagResponse::from).collect()))
}

pub async fn list_all_sync_peers(db: Db) -> AppResult<Json<Vec<SyncPeerResponse>>> {
    let peers = ops::list_all_sync_peers(&db).await?;
    Ok(Json(peers.into_iter().map(SyncPeerResponse::from).collect()))
}
