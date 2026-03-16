use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
};

use tome_core::{hash::FileHash, id::next_id};

use crate::entities::blob;

/// Find blob by digest, or insert and return it.
pub async fn get_or_create_blob<C: ConnectionTrait>(conn: &C, file_hash: &FileHash) -> anyhow::Result<blob::Model> {
    if let Some(b) = blob::Entity::find().filter(blob::Column::Digest.eq(file_hash.digest.as_ref())).one(conn).await? {
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
    Ok(am.insert(conn).await?)
}

/// Find a blob by digest.
pub async fn find_blob_by_digest<C: ConnectionTrait>(conn: &C, digest: &[u8]) -> anyhow::Result<Option<blob::Model>> {
    Ok(blob::Entity::find().filter(blob::Column::Digest.eq(digest)).one(conn).await?)
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

    // For partial prefixes, use a binary range scan so the UNIQUE index on
    // `digest` is used instead of loading all blobs into memory.
    let lower = padded(&prefix_bytes, 0x00);
    let matches = match prefix_upper_bound(&prefix_bytes) {
        Some(upper) => {
            blob::Entity::find()
                .filter(blob::Column::Digest.gte(lower).and(blob::Column::Digest.lt(padded(&upper, 0x00))))
                .all(db)
                .await?
        }
        None => {
            // Prefix is all 0xFF bytes — just scan from lower with no upper bound.
            blob::Entity::find().filter(blob::Column::Digest.gte(lower)).all(db).await?
        }
    };
    // Secondary filter: ensure exact prefix match (the range may include a few
    // non-matching rows at the edges in theory, but is always tight in practice).
    let matches: Vec<_> = matches.into_iter().filter(|b| b.digest.starts_with(&prefix_bytes)).collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.into_iter().next().unwrap())),
        n => anyhow::bail!("ambiguous digest prefix {:?}: {} blobs match", hex, n),
    }
}

/// Pad `bytes` to 32 bytes with `fill`.
fn padded(bytes: &[u8], fill: u8) -> Vec<u8> {
    let mut v = bytes.to_vec();
    v.resize(32, fill);
    v
}

/// Increment `prefix` as a big-endian integer, returning None on overflow (all 0xFF).
fn prefix_upper_bound(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut v = prefix.to_vec();
    for byte in v.iter_mut().rev() {
        if *byte < 0xFF {
            *byte += 1;
            return Some(v);
        }
        *byte = 0x00;
    }
    None // All bytes were 0xFF
}

/// Fetch blobs by a list of IDs.
pub async fn blobs_by_ids(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<Vec<blob::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(blob::Entity::find().filter(blob::Column::Id.is_in(ids.iter().copied())).all(db).await?)
}
