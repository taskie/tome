use axum::{Json, extract::Path};
use serde::Deserialize;
use utoipa::ToSchema;

use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::{AppError, AppResult};

#[derive(Deserialize, ToSchema)]
pub struct RegisterMachineRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[utoipa::path(
    get,
    path = "/machines",
    responses(
        (status = 200, description = "List registered machines", body = Vec<MachineResponse>),
    ),
    tag = "machines"
)]
pub async fn list_machines(db: Db) -> AppResult<Json<Vec<MachineResponse>>> {
    let machines = ops::list_machines(&db).await?;
    Ok(Json(machines.into_iter().map(MachineResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/machines",
    request_body = RegisterMachineRequest,
    responses(
        (status = 200, description = "Registered machine", body = MachineResponse),
    ),
    tag = "machines"
)]
pub async fn register_machine(db: Db, Json(req): Json<RegisterMachineRequest>) -> AppResult<Json<MachineResponse>> {
    let machine = ops::register_machine(&db, &req.name, &req.description).await?;
    Ok(Json(MachineResponse::from(machine)))
}

#[utoipa::path(
    put,
    path = "/machines/{id}",
    params(("id" = i16, Path, description = "Machine ID")),
    responses(
        (status = 200, description = "Updated machine (last_seen_at refreshed)", body = MachineResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    ),
    tag = "machines"
)]
pub async fn update_machine(db: Db, Path(id): Path<i16>) -> AppResult<Json<MachineResponse>> {
    ops::update_machine_last_seen(&db, id).await?;
    let machine = ops::find_machine_by_id(&db, id)
        .await?
        .ok_or_else(|| AppError::not_found(format!("machine {} not found", id)))?;
    Ok(Json(MachineResponse::from(machine)))
}
