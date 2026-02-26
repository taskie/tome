use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};

use tome_core::{hash::FileHash, id::next_id};

use crate::entities::{blob, entry, entry_cache, repository, snapshot};

// ──────────────────────────────────────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────────────────────────────────────

/// Get or create a repository by name.
pub async fn get_or_create_repository(
    db: &DatabaseConnection,
    name: &str,
) -> anyhow::Result<repository::Model> {
    if let Some(repo) = repository::Entity::find()
        .filter(repository::Column::Name.eq(name))
        .one(db)
        .await?
    {
        return Ok(repo);
    }

    let now = Utc::now().fixed_offset();
    let am = repository::ActiveModel {
        id: Set(next_id()?),
        name: Set(name.to_owned()),
        description: Set(String::new()),
        config: Set(serde_json::json!({})),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob
// ──────────────────────────────────────────────────────────────────────────────

/// Find blob by digest, or insert and return it.
pub async fn get_or_create_blob(
    db: &DatabaseConnection,
    file_hash: &FileHash,
) -> anyhow::Result<blob::Model> {
    if let Some(b) = blob::Entity::find()
        .filter(blob::Column::Digest.eq(file_hash.digest.as_ref()))
        .one(db)
        .await?
    {
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

// ──────────────────────────────────────────────────────────────────────────────
// Snapshot
// ──────────────────────────────────────────────────────────────────────────────

/// Create a new snapshot for the given repository.
///
/// `parent_id` should be the previous snapshot's ID (if any).
pub async fn create_snapshot(
    db: &DatabaseConnection,
    repository_id: i64,
    parent_id: Option<i64>,
) -> anyhow::Result<snapshot::Model> {
    let now = Utc::now().fixed_offset();
    let am = snapshot::ActiveModel {
        id: Set(next_id()?),
        repository_id: Set(repository_id),
        parent_id: Set(parent_id),
        message: Set(String::new()),
        metadata: Set(serde_json::json!({})),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find the most recent snapshot for a repository (by created_at DESC).
pub async fn latest_snapshot(
    db: &DatabaseConnection,
    repository_id: i64,
) -> anyhow::Result<Option<snapshot::Model>> {
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

// ──────────────────────────────────────────────────────────────────────────────
// Entry
// ──────────────────────────────────────────────────────────────────────────────

/// Insert a new entry (present file).
pub async fn insert_entry_present(
    db: &DatabaseConnection,
    snapshot_id: i64,
    path: &str,
    blob_id: i64,
    mode: Option<i32>,
    mtime: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> anyhow::Result<entry::Model> {
    let now = Utc::now().fixed_offset();
    let am = entry::ActiveModel {
        id: Set(next_id()?),
        snapshot_id: Set(snapshot_id),
        path: Set(path.to_owned()),
        status: Set(1), // present
        blob_id: Set(Some(blob_id)),
        mode: Set(mode),
        mtime: Set(mtime),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Insert a new entry (deleted file).
pub async fn insert_entry_deleted(
    db: &DatabaseConnection,
    snapshot_id: i64,
    path: &str,
) -> anyhow::Result<entry::Model> {
    let now = Utc::now().fixed_offset();
    let am = entry::ActiveModel {
        id: Set(next_id()?),
        snapshot_id: Set(snapshot_id),
        path: Set(path.to_owned()),
        status: Set(0), // deleted
        blob_id: Set(None),
        mode: Set(None),
        mtime: Set(None),
        created_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry cache
// ──────────────────────────────────────────────────────────────────────────────

/// Read the entry cache for a repository. Returns a map of path → cached entry.
pub async fn load_entry_cache(
    db: &DatabaseConnection,
    repository_id: i64,
) -> anyhow::Result<std::collections::HashMap<String, entry_cache::Model>> {
    use entry_cache::Column;
    let rows = entry_cache::Entity::find()
        .filter(Column::RepositoryId.eq(repository_id))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|r| (r.path.clone(), r)).collect())
}

/// Upsert (insert or replace) a cache row for a present file.
pub async fn upsert_cache_present(
    db: &DatabaseConnection,
    repository_id: i64,
    path: &str,
    snapshot_id: i64,
    entry_id: i64,
    blob_id: i64,
    mtime: Option<chrono::DateTime<chrono::FixedOffset>>,
    digest: Option<Vec<u8>>,
    size: Option<i64>,
    fast_digest: Option<i64>,
) -> anyhow::Result<()> {
    let now = Utc::now().fixed_offset();
    let am = entry_cache::ActiveModel {
        repository_id: Set(repository_id),
        path: Set(path.to_owned()),
        snapshot_id: Set(snapshot_id),
        entry_id: Set(entry_id),
        status: Set(1),
        blob_id: Set(Some(blob_id)),
        mtime: Set(mtime),
        digest: Set(digest),
        size: Set(size),
        fast_digest: Set(fast_digest),
        updated_at: Set(now),
    };
    entry_cache::Entity::insert(am)
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([
                entry_cache::Column::RepositoryId,
                entry_cache::Column::Path,
            ])
            .update_columns([
                entry_cache::Column::SnapshotId,
                entry_cache::Column::EntryId,
                entry_cache::Column::Status,
                entry_cache::Column::BlobId,
                entry_cache::Column::Mtime,
                entry_cache::Column::Digest,
                entry_cache::Column::Size,
                entry_cache::Column::FastDigest,
                entry_cache::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

/// Upsert a cache row for a deleted file.
pub async fn upsert_cache_deleted(
    db: &DatabaseConnection,
    repository_id: i64,
    path: &str,
    snapshot_id: i64,
    entry_id: i64,
) -> anyhow::Result<()> {
    let now = Utc::now().fixed_offset();
    let am = entry_cache::ActiveModel {
        repository_id: Set(repository_id),
        path: Set(path.to_owned()),
        snapshot_id: Set(snapshot_id),
        entry_id: Set(entry_id),
        status: Set(0),
        blob_id: Set(None),
        mtime: Set(None),
        digest: Set(None),
        size: Set(None),
        fast_digest: Set(None),
        updated_at: Set(now),
    };
    entry_cache::Entity::insert(am)
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([
                entry_cache::Column::RepositoryId,
                entry_cache::Column::Path,
            ])
            .update_columns([
                entry_cache::Column::SnapshotId,
                entry_cache::Column::EntryId,
                entry_cache::Column::Status,
                entry_cache::Column::BlobId,
                entry_cache::Column::Mtime,
                entry_cache::Column::Digest,
                entry_cache::Column::Size,
                entry_cache::Column::FastDigest,
                entry_cache::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}
