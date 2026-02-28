use axum::{Json, extract::Path};
use serde::Deserialize;

use tome_db::ops;

use super::Db;
use super::responses::*;
use crate::error::AppResult;

#[derive(Deserialize)]
pub struct RegisterMachineRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

pub async fn list_machines(db: Db) -> AppResult<Json<Vec<MachineResponse>>> {
    let machines = ops::list_machines(&db).await?;
    Ok(Json(machines.into_iter().map(MachineResponse::from).collect()))
}

pub async fn register_machine(db: Db, Json(req): Json<RegisterMachineRequest>) -> AppResult<Json<MachineResponse>> {
    let machine = ops::register_machine(&db, &req.name, &req.description).await?;
    Ok(Json(MachineResponse::from(machine)))
}

pub async fn update_machine(db: Db, Path(id): Path<i16>) -> AppResult<Json<MachineResponse>> {
    ops::update_machine_last_seen(&db, id).await?;
    let machine = ops::find_machine_by_id(&db, id).await?.ok_or_else(|| anyhow::anyhow!("machine {} not found", id))?;
    Ok(Json(MachineResponse::from(machine)))
}
