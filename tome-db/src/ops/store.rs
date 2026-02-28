use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use tome_core::id::next_id;

use crate::entities::store;

/// Get or create a store by name.
pub async fn get_or_create_store(
    db: &DatabaseConnection,
    name: &str,
    url: &str,
    config: serde_json::Value,
) -> anyhow::Result<store::Model> {
    if let Some(s) = store::Entity::find().filter(store::Column::Name.eq(name)).one(db).await? {
        return Ok(s);
    }
    let now = Utc::now().fixed_offset();
    let am = store::ActiveModel {
        id: Set(next_id()?),
        name: Set(name.to_owned()),
        url: Set(url.to_owned()),
        config: Set(config),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find a store by name.
pub async fn find_store_by_name(db: &DatabaseConnection, name: &str) -> anyhow::Result<Option<store::Model>> {
    Ok(store::Entity::find().filter(store::Column::Name.eq(name)).one(db).await?)
}

/// Find a store by its ID.
pub async fn find_store_by_id(db: &DatabaseConnection, id: i64) -> anyhow::Result<Option<store::Model>> {
    Ok(store::Entity::find_by_id(id).one(db).await?)
}

/// List all stores.
pub async fn list_stores(db: &DatabaseConnection) -> anyhow::Result<Vec<store::Model>> {
    Ok(store::Entity::find().all(db).await?)
}
