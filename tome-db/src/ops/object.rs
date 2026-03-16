use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
};

use tome_core::{hash::FileHash, id::next_id, models::ObjectType};

use crate::entities::object;

/// Find blob object by digest, or insert and return it.
pub async fn get_or_create_blob<C: ConnectionTrait>(conn: &C, file_hash: &FileHash) -> anyhow::Result<object::Model> {
    if let Some(b) =
        object::Entity::find().filter(object::Column::Digest.eq(file_hash.digest.as_ref())).one(conn).await?
    {
        return Ok(b);
    }

    let now = Utc::now().fixed_offset();
    let am = object::ActiveModel {
        id: Set(next_id()?),
        object_type: Set(ObjectType::Blob.as_i16()),
        digest: Set(file_hash.digest.to_vec()),
        size: Set(Some(file_hash.size as i64)),
        fast_digest: Set(Some(file_hash.fast_digest)),
        created_at: Set(now),
    };
    Ok(am.insert(conn).await?)
}

/// Find or create a tree object by digest.
pub async fn get_or_create_tree<C: ConnectionTrait>(
    conn: &C,
    digest: &[u8],
    size: i64,
    fast_digest: i64,
) -> anyhow::Result<object::Model> {
    if let Some(t) = object::Entity::find().filter(object::Column::Digest.eq(digest)).one(conn).await? {
        return Ok(t);
    }

    let now = Utc::now().fixed_offset();
    let am = object::ActiveModel {
        id: Set(next_id()?),
        object_type: Set(ObjectType::Tree.as_i16()),
        digest: Set(digest.to_vec()),
        size: Set(Some(size)),
        fast_digest: Set(Some(fast_digest)),
        created_at: Set(now),
    };
    Ok(am.insert(conn).await?)
}

/// Find an object by digest.
pub async fn find_object_by_digest<C: ConnectionTrait>(
    conn: &C,
    digest: &[u8],
) -> anyhow::Result<Option<object::Model>> {
    Ok(object::Entity::find().filter(object::Column::Digest.eq(digest)).one(conn).await?)
}

/// Find an object by primary key ID.
pub async fn find_object_by_id(db: &DatabaseConnection, id: i64) -> anyhow::Result<Option<object::Model>> {
    Ok(object::Entity::find_by_id(id).one(db).await?)
}

/// Find a blob object by hex digest string (full 64-char or shorter prefix).
/// Returns an error if the prefix is ambiguous.
pub async fn find_blob_by_hex(db: &DatabaseConnection, hex: &str) -> anyhow::Result<Option<object::Model>> {
    let hex = hex.to_lowercase();
    let prefix_bytes = (0..hex.len())
        .step_by(2)
        .filter(|&i| i + 1 < hex.len())
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| anyhow::anyhow!("invalid hex digest: {:?}", hex))?;

    if prefix_bytes.len() == 32 {
        return find_object_by_digest(db, &prefix_bytes).await;
    }

    let lower = padded(&prefix_bytes, 0x00);
    let matches = match prefix_upper_bound(&prefix_bytes) {
        Some(upper) => {
            object::Entity::find()
                .filter(object::Column::Digest.gte(lower).and(object::Column::Digest.lt(padded(&upper, 0x00))))
                .all(db)
                .await?
        }
        None => object::Entity::find().filter(object::Column::Digest.gte(lower)).all(db).await?,
    };
    let matches: Vec<_> = matches.into_iter().filter(|b| b.digest.starts_with(&prefix_bytes)).collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.into_iter().next().unwrap())),
        n => anyhow::bail!("ambiguous digest prefix {:?}: {} objects match", hex, n),
    }
}

fn padded(bytes: &[u8], fill: u8) -> Vec<u8> {
    let mut v = bytes.to_vec();
    v.resize(32, fill);
    v
}

fn prefix_upper_bound(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut v = prefix.to_vec();
    for byte in v.iter_mut().rev() {
        if *byte < 0xFF {
            *byte += 1;
            return Some(v);
        }
        *byte = 0x00;
    }
    None
}

/// Fetch objects by a list of IDs.
pub async fn objects_by_ids(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<Vec<object::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(object::Entity::find().filter(object::Column::Id.is_in(ids.iter().copied())).all(db).await?)
}
