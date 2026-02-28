use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;

use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::{AppError, AppResult};

#[derive(Deserialize)]
pub struct EntriesQuery {
    #[serde(default)]
    pub prefix: String,
}

pub async fn list_entries(
    db: Db,
    Path(id): Path<String>,
    Query(q): Query<EntriesQuery>,
) -> AppResult<Json<Vec<EntryResponse>>> {
    let snapshot_id: i64 = id.parse().map_err(|_| AppError::bad_request("invalid snapshot id"))?;
    let pairs = ops::entries_with_digest(&db, snapshot_id, &q.prefix).await?;
    Ok(Json(pairs.into_iter().map(|(e, b)| EntryResponse::from_with_blob(e, b.as_ref())).collect()))
}
