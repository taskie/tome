use axum::{Json, extract::Path};

use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::AppResult;

pub async fn get_blob(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<BlobResponse>> {
    let digest = hex::decode(&digest_hex).map_err(|_| anyhow::anyhow!("invalid digest hex"))?;
    let blob = ops::find_blob_by_digest(&db, &digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob {:?} not found", digest_hex))?;
    Ok(Json(blob.into()))
}

pub async fn list_blob_entries(db: Db, Path(digest_hex): Path<String>) -> AppResult<Json<Vec<SnapshotEntry>>> {
    let digest = hex::decode(&digest_hex).map_err(|_| anyhow::anyhow!("invalid digest hex"))?;
    let blob = ops::find_blob_by_digest(&db, &digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob {:?} not found", digest_hex))?;
    let entries = ops::entries_for_blob(&db, blob.id).await?;
    Ok(Json(entries.into_iter().map(|(e, s)| SnapshotEntry { snapshot: s.into(), entry: e.into() }).collect()))
}
