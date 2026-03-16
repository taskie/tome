use axum::{Json, extract::Path};

use super::Db;
use super::responses::*;
use crate::error::{AppError, AppResult};

#[utoipa::path(
    get,
    path = "/objects/{digest}",
    params(("digest" = String, Path, description = "Object SHA-256/BLAKE3 digest (hex)")),
    responses(
        (status = 200, description = "Object metadata", body = ObjectResponse),
        (status = 400, description = "Invalid digest", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "objects"
)]
pub async fn get_object(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<ObjectResponse>> {
    let digest = hex::decode(&digest_hex).map_err(|_| AppError::bad_request("invalid digest hex"))?;
    let obj = db
        .find_object_by_digest(&digest)
        .await?
        .ok_or_else(|| AppError::not_found(format!("object {:?} not found", digest_hex)))?;
    Ok(Json(obj.into()))
}

#[utoipa::path(
    get,
    path = "/objects/{digest}/entries",
    params(("digest" = String, Path, description = "Object digest (hex)")),
    responses(
        (status = 200, description = "All snapshot+entry pairs referencing this object", body = Vec<SnapshotEntry>),
        (status = 400, description = "Invalid digest", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "objects"
)]
pub async fn list_object_entries(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<Vec<SnapshotEntry>>> {
    let digest = hex::decode(&digest_hex).map_err(|_| AppError::bad_request("invalid digest hex"))?;
    let obj = db
        .find_object_by_digest(&digest)
        .await?
        .ok_or_else(|| AppError::not_found(format!("object {:?} not found", digest_hex)))?;
    let entries = db.entries_for_object(obj.id).await?;
    Ok(Json(entries.into_iter().map(|(e, s)| SnapshotEntry { snapshot: s.into(), entry: e.into() }).collect()))
}
