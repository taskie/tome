use axum::{Json, extract::Path};

use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::{AppError, AppResult};

#[utoipa::path(
    get,
    path = "/blobs/{digest}",
    params(("digest" = String, Path, description = "Blob SHA-256/BLAKE3 digest (hex)")),
    responses(
        (status = 200, description = "Blob metadata", body = BlobResponse),
        (status = 400, description = "Invalid digest", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "blobs"
)]
pub async fn get_blob(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<BlobResponse>> {
    let digest = hex::decode(&digest_hex).map_err(|_| AppError::bad_request("invalid digest hex"))?;
    let blob = ops::find_blob_by_digest(&*db, &digest)
        .await?
        .ok_or_else(|| AppError::not_found(format!("blob {:?} not found", digest_hex)))?;
    Ok(Json(blob.into()))
}

#[utoipa::path(
    get,
    path = "/blobs/{digest}/entries",
    params(("digest" = String, Path, description = "Blob digest (hex)")),
    responses(
        (status = 200, description = "All snapshot+entry pairs containing this blob", body = Vec<SnapshotEntry>),
        (status = 400, description = "Invalid digest", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "blobs"
)]
pub async fn list_blob_entries(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<Vec<SnapshotEntry>>> {
    let digest = hex::decode(&digest_hex).map_err(|_| AppError::bad_request("invalid digest hex"))?;
    let blob = ops::find_blob_by_digest(&*db, &digest)
        .await?
        .ok_or_else(|| AppError::not_found(format!("blob {:?} not found", digest_hex)))?;
    let entries = ops::entries_for_blob(&db, blob.id).await?;
    Ok(Json(entries.into_iter().map(|(e, s)| SnapshotEntry { snapshot: s.into(), entry: e.into() }).collect()))
}
