use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryOrder, QuerySelect,
};

use crate::entities::machine;

/// Register a new machine, auto-assigning the next available machine_id.
pub async fn register_machine(
    db: &DatabaseConnection,
    name: &str,
    description: &str,
) -> anyhow::Result<machine::Model> {
    let now = Utc::now().fixed_offset();

    // Find the next available machine_id (start from 1; 0 is reserved for local-only).
    let max_id: Option<i16> = machine::Entity::find()
        .select_only()
        .column_as(machine::Column::MachineId.max(), "max_id")
        .into_tuple()
        .one(db)
        .await?;
    let next_machine_id = max_id.map(|m| m + 1).unwrap_or(1);

    let am = machine::ActiveModel {
        machine_id: Set(next_machine_id),
        name: Set(name.to_owned()),
        description: Set(description.to_owned()),
        last_seen_at: Set(None),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// List all registered machines.
pub async fn list_machines(db: &DatabaseConnection) -> anyhow::Result<Vec<machine::Model>> {
    Ok(machine::Entity::find().order_by_asc(machine::Column::MachineId).all(db).await?)
}

/// Find a machine by its ID.
pub async fn find_machine_by_id(db: &DatabaseConnection, machine_id: i16) -> anyhow::Result<Option<machine::Model>> {
    Ok(machine::Entity::find_by_id(machine_id).one(db).await?)
}

/// Update a machine's last_seen_at timestamp.
pub async fn update_machine_last_seen(db: &DatabaseConnection, machine_id: i16) -> anyhow::Result<()> {
    let m = machine::Entity::find_by_id(machine_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("machine {} not found", machine_id))?;
    let mut am: machine::ActiveModel = m.into();
    am.last_seen_at = Set(Some(Utc::now().fixed_offset()));
    am.update(db).await?;
    Ok(())
}
