use std::collections::HashSet;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};

use crate::entities::{entry, object, snapshot};

/// Return the set of object IDs referenced by any entry in the given snapshots.
pub async fn object_ids_in_snapshots(db: &DatabaseConnection, snapshot_ids: &[i64]) -> anyhow::Result<HashSet<i64>> {
    if snapshot_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let ids: Vec<i64> = entry::Entity::find()
        .filter(entry::Column::SnapshotId.is_in(snapshot_ids.iter().copied()))
        .filter(entry::Column::ObjectId.is_not_null())
        .select_only()
        .column(entry::Column::ObjectId)
        .into_tuple::<Option<i64>>()
        .all(db)
        .await?
        .into_iter()
        .flatten()
        .collect();
    Ok(ids.into_iter().collect())
}

/// Return objects that are not referenced by any entry (truly orphaned).
pub async fn unreferenced_objects(db: &DatabaseConnection) -> anyhow::Result<Vec<object::Model>> {
    let referenced: HashSet<i64> = entry::Entity::find()
        .filter(entry::Column::ObjectId.is_not_null())
        .select_only()
        .column(entry::Column::ObjectId)
        .into_tuple::<Option<i64>>()
        .all(db)
        .await?
        .into_iter()
        .flatten()
        .collect();

    let all = object::Entity::find().all(db).await?;
    Ok(all.into_iter().filter(|b| !referenced.contains(&b.id)).collect())
}

/// Delete object records by IDs; returns the count deleted.
pub async fn delete_object_records(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = object::Entity::delete_many().filter(object::Column::Id.is_in(ids.iter().copied())).exec(db).await?;
    Ok(res.rows_affected)
}

/// Delete snapshot records by IDs; returns the count deleted.
pub async fn delete_snapshot_records(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = snapshot::Entity::delete_many().filter(snapshot::Column::Id.is_in(ids.iter().copied())).exec(db).await?;
    Ok(res.rows_affected)
}

/// Delete all entry_cache rows whose snapshot_id is in the given set.
///
/// Must be called **before** `delete_entries_in_snapshots` to avoid FK constraint
/// violations: unchanged files keep their entry_cache row pointing at the entry from
/// the snapshot where they were first recorded.  Clearing those rows first is safe
/// because the cache is rebuilt on the next scan.
pub async fn delete_entry_cache_for_snapshots(db: &DatabaseConnection, snapshot_ids: &[i64]) -> anyhow::Result<u64> {
    if snapshot_ids.is_empty() {
        return Ok(0);
    }
    use crate::entities::entry_cache;
    let res = entry_cache::Entity::delete_many()
        .filter(entry_cache::Column::SnapshotId.is_in(snapshot_ids.iter().copied()))
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

/// Delete all entry records belonging to the given snapshot IDs; returns the count deleted.
pub async fn delete_entries_in_snapshots(db: &DatabaseConnection, snapshot_ids: &[i64]) -> anyhow::Result<u64> {
    if snapshot_ids.is_empty() {
        return Ok(0);
    }
    let res = entry::Entity::delete_many()
        .filter(entry::Column::SnapshotId.is_in(snapshot_ids.iter().copied()))
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}
