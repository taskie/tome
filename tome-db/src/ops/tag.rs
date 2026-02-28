use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};

use tome_core::id::next_id;

use crate::entities::{blob, tag};

/// Upsert a tag on a blob: replaces all existing tags with the same (blob_id, key).
pub async fn upsert_tag(
    db: &DatabaseConnection,
    blob_id: i64,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<tag::Model> {
    tag::Entity::delete_many()
        .filter(tag::Column::BlobId.eq(blob_id))
        .filter(tag::Column::Key.eq(key))
        .exec(db)
        .await?;

    let now = Utc::now().fixed_offset();
    let am = tag::ActiveModel {
        id: Set(next_id()?),
        blob_id: Set(blob_id),
        key: Set(key.to_owned()),
        value: Set(value.map(str::to_owned)),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Delete all tags with the given key for a blob.
pub async fn delete_tags(db: &DatabaseConnection, blob_id: i64, key: &str) -> anyhow::Result<u64> {
    let res = tag::Entity::delete_many()
        .filter(tag::Column::BlobId.eq(blob_id))
        .filter(tag::Column::Key.eq(key))
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

/// List all tags for a blob, ordered by key then id.
pub async fn list_tags(db: &DatabaseConnection, blob_id: i64) -> anyhow::Result<Vec<tag::Model>> {
    Ok(tag::Entity::find()
        .filter(tag::Column::BlobId.eq(blob_id))
        .order_by_asc(tag::Column::Key)
        .order_by_asc(tag::Column::Id)
        .all(db)
        .await?)
}

pub async fn list_all_tags(db: &DatabaseConnection) -> anyhow::Result<Vec<tag::Model>> {
    Ok(tag::Entity::find().order_by_asc(tag::Column::Key).order_by_asc(tag::Column::Id).all(db).await?)
}

/// Find all blobs that have a tag matching the given key (and optionally value).
pub async fn search_blobs_by_tag(
    db: &DatabaseConnection,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<Vec<(blob::Model, Vec<tag::Model>)>> {
    let mut q = tag::Entity::find().filter(tag::Column::Key.eq(key));
    if let Some(v) = value {
        q = q.filter(tag::Column::Value.eq(v));
    }
    let tags = q.order_by_asc(tag::Column::BlobId).all(db).await?;

    if tags.is_empty() {
        return Ok(vec![]);
    }

    let blob_ids: Vec<i64> =
        tags.iter().map(|t| t.blob_id).collect::<std::collections::HashSet<_>>().into_iter().collect();
    let blobs: HashMap<i64, blob::Model> = blob::Entity::find()
        .filter(blob::Column::Id.is_in(blob_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|b| (b.id, b))
        .collect();

    let mut result: HashMap<i64, (blob::Model, Vec<tag::Model>)> = HashMap::new();
    for t in tags {
        if let Some(blob) = blobs.get(&t.blob_id) {
            result.entry(t.blob_id).or_insert_with(|| (blob.clone(), vec![])).1.push(t);
        }
    }

    let mut out: Vec<(blob::Model, Vec<tag::Model>)> = result.into_values().collect();
    out.sort_by_key(|(b, _)| b.id);
    Ok(out)
}
