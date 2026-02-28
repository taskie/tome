use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};

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

/// Update a store's URL and/or config.
pub async fn update_store(
    db: &DatabaseConnection,
    id: i64,
    url: Option<&str>,
    config: Option<serde_json::Value>,
) -> anyhow::Result<store::Model> {
    let model =
        store::Entity::find_by_id(id).one(db).await?.ok_or_else(|| anyhow::anyhow!("store {} not found", id))?;
    let mut am: store::ActiveModel = model.into();
    if let Some(u) = url {
        am.url = Set(u.to_owned());
    }
    if let Some(c) = config {
        am.config = Set(c);
    }
    am.updated_at = Set(Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

/// Delete a store by ID.
pub async fn delete_store(db: &DatabaseConnection, id: i64) -> anyhow::Result<u64> {
    let res = store::Entity::delete_by_id(id).exec(db).await?;
    Ok(res.rows_affected)
}

/// Count replicas in a store.
pub async fn count_replicas_in_store(db: &DatabaseConnection, store_id: i64) -> anyhow::Result<u64> {
    use crate::entities::replica;
    Ok(replica::Entity::find().filter(replica::Column::StoreId.eq(store_id)).count(db).await?)
}
