use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use tome_core::{hash::FileHash, id::next_id};

use crate::entities::blob;

/// Find blob by digest, or insert and return it.
pub async fn get_or_create_blob(db: &DatabaseConnection, file_hash: &FileHash) -> anyhow::Result<blob::Model> {
    if let Some(b) = blob::Entity::find().filter(blob::Column::Digest.eq(file_hash.digest.as_ref())).one(db).await? {
        return Ok(b);
    }

    let now = Utc::now().fixed_offset();
    let am = blob::ActiveModel {
        id: Set(next_id()?),
        digest: Set(file_hash.digest.to_vec()),
        size: Set(file_hash.size as i64),
        fast_digest: Set(file_hash.fast_digest),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find a blob by digest.
pub async fn find_blob_by_digest(db: &DatabaseConnection, digest: &[u8]) -> anyhow::Result<Option<blob::Model>> {
    Ok(blob::Entity::find().filter(blob::Column::Digest.eq(digest)).one(db).await?)
}

/// Find a blob by primary key ID.
pub async fn find_blob_by_id(db: &DatabaseConnection, id: i64) -> anyhow::Result<Option<blob::Model>> {
    Ok(blob::Entity::find_by_id(id).one(db).await?)
}

/// Find a blob by hex digest string (full 64-char or shorter prefix).
/// Returns an error if the prefix is ambiguous.
pub async fn find_blob_by_hex(db: &DatabaseConnection, hex: &str) -> anyhow::Result<Option<blob::Model>> {
    let hex = hex.to_lowercase();
    let prefix_bytes = (0..hex.len())
        .step_by(2)
        .filter(|&i| i + 1 < hex.len())
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| anyhow::anyhow!("invalid hex digest: {:?}", hex))?;

    if prefix_bytes.len() == 32 {
        return find_blob_by_digest(db, &prefix_bytes).await;
    }

    let all = blob::Entity::find().all(db).await?;
    let matches: Vec<blob::Model> = all.into_iter().filter(|b| b.digest.starts_with(&prefix_bytes)).collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.into_iter().next().unwrap())),
        n => anyhow::bail!("ambiguous digest prefix {:?}: {} blobs match", hex, n),
    }
}

/// Fetch blobs by a list of IDs.
pub async fn blobs_by_ids(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<Vec<blob::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(blob::Entity::find().filter(blob::Column::Id.is_in(ids.iter().copied())).all(db).await?)
}
