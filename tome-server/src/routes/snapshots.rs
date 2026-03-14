use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;
use utoipa::IntoParams;

use super::Db;
use super::responses::*;
use crate::error::{AppError, AppResult};

#[derive(Deserialize, IntoParams)]
pub struct EntriesQuery {
    /// Optional path prefix filter.
    #[serde(default)]
    pub prefix: String,
}

#[utoipa::path(
    get,
    path = "/snapshots/{id}/entries",
    params(
        ("id" = String, Path, description = "Snapshot ID (decimal)"),
        EntriesQuery,
    ),
    responses(
        (status = 200, description = "Entries in the snapshot", body = Vec<EntryResponse>),
        (status = 400, description = "Invalid snapshot id", body = ErrorResponse),
    ),
    tag = "snapshots"
)]
pub async fn list_entries(
    db: Db,
    Path(id): Path<String>,
    Query(q): Query<EntriesQuery>,
) -> AppResult<Json<Vec<EntryResponse>>> {
    let snapshot_id: i64 = id.parse().map_err(|_| AppError::bad_request("invalid snapshot id"))?;
    let pairs = db.entries_with_digest(snapshot_id, &q.prefix).await?;
    Ok(Json(pairs.into_iter().map(|(e, b)| EntryResponse::from_with_blob(e, b.as_ref())).collect()))
}
