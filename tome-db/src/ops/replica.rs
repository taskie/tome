use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use tome_core::id::next_id;

use crate::entities::{blob, replica, store};

/// Check whether a replica exists for (blob_id, store_id).
pub async fn replica_exists(db: &DatabaseConnection, blob_id: i64, store_id: i64) -> anyhow::Result<bool> {
    Ok(replica::Entity::find()
        .filter(replica::Column::BlobId.eq(blob_id))
        .filter(replica::Column::StoreId.eq(store_id))
        .one(db)
        .await?
        .is_some())
}

/// Record a new replica.
pub async fn insert_replica(
    db: &DatabaseConnection,
    blob_id: i64,
    store_id: i64,
    path: &str,
    encrypted: bool,
) -> anyhow::Result<replica::Model> {
    let now = Utc::now().fixed_offset();
    let am = replica::ActiveModel {
        id: Set(next_id()?),
        blob_id: Set(blob_id),
        store_id: Set(store_id),
        path: Set(path.to_owned()),
        encrypted: Set(encrypted),
        verified_at: Set(None),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find all replicas in a given store.
pub async fn replicas_in_store(db: &DatabaseConnection, store_id: i64) -> anyhow::Result<Vec<replica::Model>> {
    Ok(replica::Entity::find().filter(replica::Column::StoreId.eq(store_id)).all(db).await?)
}

/// Find all (replica, blob) pairs for a store (for verify).
pub async fn replicas_with_blobs_in_store(
    db: &DatabaseConnection,
    store_id: i64,
) -> anyhow::Result<Vec<(replica::Model, blob::Model)>> {
    let rows = replica::Entity::find()
        .filter(replica::Column::StoreId.eq(store_id))
        .find_also_related(blob::Entity)
        .all(db)
        .await?;
    Ok(rows.into_iter().filter_map(|(r, b)| b.map(|b| (r, b))).collect())
}

/// Find all (replica, store) pairs for a blob (used by restore to locate download sources).
pub async fn replicas_for_blob(
    db: &DatabaseConnection,
    blob_id: i64,
) -> anyhow::Result<Vec<(replica::Model, store::Model)>> {
    let rows = replica::Entity::find()
        .filter(replica::Column::BlobId.eq(blob_id))
        .find_also_related(store::Entity)
        .all(db)
        .await?;
    Ok(rows.into_iter().filter_map(|(r, s)| s.map(|s| (r, s))).collect())
}

/// Update the verified_at timestamp of a replica.
pub async fn update_replica_verified_at(
    db: &DatabaseConnection,
    replica_id: i64,
    verified_at: chrono::DateTime<chrono::FixedOffset>,
) -> anyhow::Result<()> {
    replica::ActiveModel { id: Set(replica_id), verified_at: Set(Some(verified_at)), ..Default::default() }
        .update(db)
        .await?;
    Ok(())
}

/// Find blobs that have a replica in src_store_id but NOT in dst_store_id.
pub async fn blobs_missing_in_dst(
    db: &DatabaseConnection,
    src_store_id: i64,
    dst_store_id: i64,
) -> anyhow::Result<Vec<blob::Model>> {
    use sea_orm::query::*;

    let dst_blob_ids: Vec<i64> = replica::Entity::find()
        .select_only()
        .column(replica::Column::BlobId)
        .filter(replica::Column::StoreId.eq(dst_store_id))
        .into_tuple()
        .all(db)
        .await?;

    let src_replicas = replica::Entity::find()
        .filter(replica::Column::StoreId.eq(src_store_id))
        .filter(replica::Column::BlobId.is_not_in(dst_blob_ids))
        .all(db)
        .await?;

    let blob_ids: Vec<i64> = src_replicas.into_iter().map(|r| r.blob_id).collect();
    Ok(blob::Entity::find().filter(blob::Column::Id.is_in(blob_ids)).all(db).await?)
}

/// Return replica records paired with their store for the given blob IDs.
pub async fn replicas_for_blobs(
    db: &DatabaseConnection,
    blob_ids: &[i64],
) -> anyhow::Result<Vec<(replica::Model, store::Model)>> {
    if blob_ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(replica::Entity::find()
        .filter(replica::Column::BlobId.is_in(blob_ids.iter().copied()))
        .find_also_related(store::Entity)
        .all(db)
        .await?
        .into_iter()
        .filter_map(|(r, s)| s.map(|s| (r, s)))
        .collect())
}

/// Delete replica records by IDs; returns the count deleted.
pub async fn delete_replica_records(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = replica::Entity::delete_many().filter(replica::Column::Id.is_in(ids.iter().copied())).exec(db).await?;
    Ok(res.rows_affected)
}
