use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};

use tome_core::id::next_id;

use crate::entities::{entry, object, snapshot};

/// Insert a new entry (present file).
pub async fn insert_entry_present<C: ConnectionTrait>(
    conn: &C,
    snapshot_id: i64,
    path: &str,
    object_id: i64,
    mode: Option<i32>,
    mtime: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> anyhow::Result<entry::Model> {
    let now = Utc::now().fixed_offset();
    let am = entry::ActiveModel {
        id: Set(next_id()?),
        snapshot_id: Set(snapshot_id),
        path: Set(path.to_owned()),
        status: Set(1), // present
        object_id: Set(Some(object_id)),
        mode: Set(mode),
        mtime: Set(mtime),
        created_at: Set(now),
    };
    Ok(am.insert(conn).await?)
}

/// Insert a new entry (deleted file).
pub async fn insert_entry_deleted<C: ConnectionTrait>(
    conn: &C,
    snapshot_id: i64,
    path: &str,
) -> anyhow::Result<entry::Model> {
    let now = Utc::now().fixed_offset();
    let am = entry::ActiveModel {
        id: Set(next_id()?),
        snapshot_id: Set(snapshot_id),
        path: Set(path.to_owned()),
        status: Set(0), // deleted
        object_id: Set(None),
        mode: Set(None),
        mtime: Set(None),
        created_at: Set(now),
    };
    Ok(am.insert(conn).await?)
}

/// Get all entries in a snapshot.
pub async fn entries_in_snapshot(db: &DatabaseConnection, snapshot_id: i64) -> anyhow::Result<Vec<entry::Model>> {
    Ok(entry::Entity::find().filter(entry::Column::SnapshotId.eq(snapshot_id)).all(db).await?)
}

/// Fetch all entries for a snapshot with their associated object, optionally filtered by path prefix.
pub async fn entries_with_digest(
    db: &DatabaseConnection,
    snapshot_id: i64,
    path_prefix: &str,
) -> anyhow::Result<Vec<(entry::Model, Option<object::Model>)>> {
    let mut q = entry::Entity::find().filter(entry::Column::SnapshotId.eq(snapshot_id));
    if !path_prefix.is_empty() {
        q = q.filter(entry::Column::Path.like(format!("{path_prefix}%")));
    }
    Ok(q.find_also_related(object::Entity).all(db).await?)
}

/// Fetch present entries (status=1) for a snapshot, optionally filtered by path prefix.
pub async fn entries_by_prefix(
    db: &DatabaseConnection,
    snapshot_id: i64,
    path_prefix: &str,
) -> anyhow::Result<Vec<entry::Model>> {
    let mut q =
        entry::Entity::find().filter(entry::Column::SnapshotId.eq(snapshot_id)).filter(entry::Column::Status.eq(1i16));
    if !path_prefix.is_empty() {
        q = q.filter(entry::Column::Path.like(format!("{path_prefix}%")));
    }
    Ok(q.all(db).await?)
}

/// Fetch the history of a path across all snapshots in a repository, newest first.
/// Returns `(entry, object, snapshot)` triples; `object` is `None` for deleted entries.
pub async fn path_history(
    db: &DatabaseConnection,
    repository_id: i64,
    path: &str,
) -> anyhow::Result<Vec<(entry::Model, Option<object::Model>, snapshot::Model)>> {
    let snapshots = snapshot::Entity::find()
        .filter(snapshot::Column::RepositoryId.eq(repository_id))
        .order_by_desc(snapshot::Column::CreatedAt)
        .all(db)
        .await?;

    if snapshots.is_empty() {
        return Ok(vec![]);
    }

    let snapshot_map: HashMap<i64, snapshot::Model> = snapshots.iter().map(|s| (s.id, s.clone())).collect();
    let snapshot_ids: Vec<i64> = snapshots.into_iter().map(|s| s.id).collect();

    let pairs = entry::Entity::find()
        .filter(entry::Column::SnapshotId.is_in(snapshot_ids))
        .filter(entry::Column::Path.eq(path))
        .find_also_related(object::Entity)
        .all(db)
        .await?;

    let mut result: Vec<(entry::Model, Option<object::Model>, snapshot::Model)> =
        pairs.into_iter().filter_map(|(e, b)| snapshot_map.get(&e.snapshot_id).map(|s| (e, b, s.clone()))).collect();
    result.sort_by(|a, b| b.2.created_at.cmp(&a.2.created_at));
    Ok(result)
}

/// Fetch all entries that reference a specific object, with their snapshot, newest first.
pub async fn entries_for_object(
    db: &DatabaseConnection,
    object_id: i64,
) -> anyhow::Result<Vec<(entry::Model, snapshot::Model)>> {
    let entries = entry::Entity::find().filter(entry::Column::ObjectId.eq(object_id)).all(db).await?;

    if entries.is_empty() {
        return Ok(vec![]);
    }

    let snapshot_ids: Vec<i64> = entries.iter().map(|e| e.snapshot_id).collect::<HashSet<_>>().into_iter().collect();

    let snapshots: HashMap<i64, snapshot::Model> = snapshot::Entity::find()
        .filter(snapshot::Column::Id.is_in(snapshot_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();

    let mut result: Vec<(entry::Model, snapshot::Model)> =
        entries.into_iter().filter_map(|e| snapshots.get(&e.snapshot_id).map(|s| (e, s.clone()))).collect();
    result.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
    Ok(result)
}
