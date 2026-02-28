use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};

use tome_core::id::next_id;

use crate::entities::sync_peer;

/// Insert a new sync peer.
pub async fn insert_sync_peer(
    db: &DatabaseConnection,
    name: &str,
    url: &str,
    repository_id: i64,
    config: serde_json::Value,
) -> anyhow::Result<sync_peer::Model> {
    let now = Utc::now().fixed_offset();
    let am = sync_peer::ActiveModel {
        id: Set(next_id()?),
        name: Set(name.to_owned()),
        url: Set(url.to_owned()),
        repository_id: Set(repository_id),
        last_synced_at: Set(None),
        last_snapshot_id: Set(None),
        config: Set(config),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find a sync peer by name and repository.
pub async fn find_sync_peer(
    db: &DatabaseConnection,
    name: &str,
    repository_id: i64,
) -> anyhow::Result<Option<sync_peer::Model>> {
    Ok(sync_peer::Entity::find()
        .filter(sync_peer::Column::Name.eq(name))
        .filter(sync_peer::Column::RepositoryId.eq(repository_id))
        .one(db)
        .await?)
}

/// List all sync peers for a repository.
pub async fn list_sync_peers(db: &DatabaseConnection, repository_id: i64) -> anyhow::Result<Vec<sync_peer::Model>> {
    Ok(sync_peer::Entity::find().filter(sync_peer::Column::RepositoryId.eq(repository_id)).all(db).await?)
}

pub async fn list_all_sync_peers(db: &DatabaseConnection) -> anyhow::Result<Vec<sync_peer::Model>> {
    Ok(sync_peer::Entity::find()
        .order_by_asc(sync_peer::Column::RepositoryId)
        .order_by_asc(sync_peer::Column::Name)
        .all(db)
        .await?)
}

/// Update a sync peer's URL and/or config.
pub async fn update_sync_peer(
    db: &DatabaseConnection,
    id: i64,
    url: Option<&str>,
    config: Option<serde_json::Value>,
) -> anyhow::Result<sync_peer::Model> {
    let model = sync_peer::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync_peer {} not found", id))?;
    let mut am: sync_peer::ActiveModel = model.into();
    if let Some(u) = url {
        am.url = Set(u.to_owned());
    }
    if let Some(c) = config {
        am.config = Set(c);
    }
    am.updated_at = Set(Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

/// Delete a sync peer by ID.
pub async fn delete_sync_peer(db: &DatabaseConnection, id: i64) -> anyhow::Result<u64> {
    let res = sync_peer::Entity::delete_by_id(id).exec(db).await?;
    Ok(res.rows_affected)
}

/// Update the last_snapshot_id and last_synced_at of a sync peer.
pub async fn update_sync_peer_progress(
    db: &DatabaseConnection,
    peer_id: i64,
    last_snapshot_id: i64,
) -> anyhow::Result<()> {
    let peer = sync_peer::Entity::find_by_id(peer_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync_peer {} not found", peer_id))?;
    let mut am: sync_peer::ActiveModel = peer.into();
    am.last_snapshot_id = Set(Some(last_snapshot_id));
    am.last_synced_at = Set(Some(Utc::now().fixed_offset()));
    am.updated_at = Set(Utc::now().fixed_offset());
    am.update(db).await?;
    Ok(())
}
