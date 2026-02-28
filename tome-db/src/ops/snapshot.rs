use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use tome_core::id::next_id;

use crate::entities::snapshot;

/// Create a new snapshot for the given repository.
///
/// `parent_id` should be the previous snapshot's ID (if any).
pub async fn create_snapshot(
    db: &DatabaseConnection,
    repository_id: i64,
    parent_id: Option<i64>,
    message: &str,
) -> anyhow::Result<snapshot::Model> {
    let now = Utc::now().fixed_offset();
    let am = snapshot::ActiveModel {
        id: Set(next_id()?),
        repository_id: Set(repository_id),
        parent_id: Set(parent_id),
        message: Set(message.to_owned()),
        metadata: Set(serde_json::json!({})),
        source_machine_id: Set(None),
        source_snapshot_id: Set(None),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Create a new snapshot with source provenance (for sync push).
pub async fn create_snapshot_with_source(
    db: &DatabaseConnection,
    repository_id: i64,
    parent_id: Option<i64>,
    message: &str,
    source_machine_id: i16,
    source_snapshot_id: i64,
) -> anyhow::Result<snapshot::Model> {
    let now = Utc::now().fixed_offset();
    let am = snapshot::ActiveModel {
        id: Set(next_id()?),
        repository_id: Set(repository_id),
        parent_id: Set(parent_id),
        message: Set(message.to_owned()),
        metadata: Set(serde_json::json!({})),
        source_machine_id: Set(Some(source_machine_id)),
        source_snapshot_id: Set(Some(source_snapshot_id)),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find the most recent snapshot for a repository (by created_at DESC).
pub async fn latest_snapshot(db: &DatabaseConnection, repository_id: i64) -> anyhow::Result<Option<snapshot::Model>> {
    Ok(snapshot::Entity::find()
        .filter(snapshot::Column::RepositoryId.eq(repository_id))
        .order_by_desc(snapshot::Column::CreatedAt)
        .one(db)
        .await?)
}

/// Update snapshot metadata (e.g. scan statistics).
pub async fn update_snapshot_metadata(
    db: &DatabaseConnection,
    snapshot_id: i64,
    metadata: serde_json::Value,
) -> anyhow::Result<()> {
    let snap = snapshot::Entity::find_by_id(snapshot_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("snapshot {} not found", snapshot_id))?;

    let mut am: snapshot::ActiveModel = snap.into();
    am.metadata = Set(metadata);
    am.update(db).await?;
    Ok(())
}

/// Get the latest snapshot for a repository (for metadata/scan_root).
pub async fn latest_snapshot_metadata(
    db: &DatabaseConnection,
    repository_id: i64,
) -> anyhow::Result<Option<serde_json::Value>> {
    Ok(snapshot::Entity::find()
        .filter(snapshot::Column::RepositoryId.eq(repository_id))
        .order_by_desc(snapshot::Column::CreatedAt)
        .one(db)
        .await?
        .map(|s| s.metadata))
}

/// Get snapshots for a repository created after `last_snapshot_id` (ordered by created_at ASC).
pub async fn snapshots_after(
    db: &DatabaseConnection,
    repository_id: i64,
    last_snapshot_id: Option<i64>,
) -> anyhow::Result<Vec<snapshot::Model>> {
    let mut q = snapshot::Entity::find()
        .filter(snapshot::Column::RepositoryId.eq(repository_id))
        .order_by_asc(snapshot::Column::CreatedAt);

    if let Some(last_id) = last_snapshot_id {
        if let Some(last_snap) = snapshot::Entity::find_by_id(last_id).one(db).await? {
            q = q.filter(snapshot::Column::CreatedAt.gt(last_snap.created_at));
        }
    }

    Ok(q.all(db).await?)
}

/// List all snapshots for a repository, ordered by created_at DESC (newest first).
pub async fn list_snapshots_for_repo(
    db: &DatabaseConnection,
    repository_id: i64,
) -> anyhow::Result<Vec<snapshot::Model>> {
    Ok(snapshot::Entity::find()
        .filter(snapshot::Column::RepositoryId.eq(repository_id))
        .order_by_desc(snapshot::Column::CreatedAt)
        .all(db)
        .await?)
}

/// Return all snapshot IDs in the database.
pub async fn all_snapshot_ids(db: &DatabaseConnection) -> anyhow::Result<Vec<i64>> {
    Ok(snapshot::Entity::find().select_only().column(snapshot::Column::Id).into_tuple::<i64>().all(db).await?)
}
