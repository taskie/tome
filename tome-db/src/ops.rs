use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use std::collections::{HashMap, HashSet};

use tome_core::{hash::FileHash, id::next_id};

use crate::entities::{blob, entry, entry_cache, replica, repository, snapshot, store, sync_peer, tag};

// ──────────────────────────────────────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────────────────────────────────────

/// Get or create a repository by name.
pub async fn get_or_create_repository(db: &DatabaseConnection, name: &str) -> anyhow::Result<repository::Model> {
    if let Some(repo) = repository::Entity::find().filter(repository::Column::Name.eq(name)).one(db).await? {
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
    message: &str,
) -> anyhow::Result<snapshot::Model> {
    let now = Utc::now().fixed_offset();
    let am = snapshot::ActiveModel {
        id: Set(next_id()?),
        repository_id: Set(repository_id),
        parent_id: Set(parent_id),
        message: Set(message.to_owned()),
        metadata: Set(serde_json::json!({})),
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
    let rows = entry_cache::Entity::find().filter(Column::RepositoryId.eq(repository_id)).all(db).await?;
    Ok(rows.into_iter().map(|r| (r.path.clone(), r)).collect())
}

pub struct UpsertCachePresentParams {
    pub repository_id: i64,
    pub path: String,
    pub snapshot_id: i64,
    pub entry_id: i64,
    pub blob_id: i64,
    pub mtime: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub digest: Option<Vec<u8>>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,
}

/// Upsert (insert or replace) a cache row for a present file.
pub async fn upsert_cache_present(db: &DatabaseConnection, p: UpsertCachePresentParams) -> anyhow::Result<()> {
    let now = Utc::now().fixed_offset();
    let am = entry_cache::ActiveModel {
        repository_id: Set(p.repository_id),
        path: Set(p.path),
        snapshot_id: Set(p.snapshot_id),
        entry_id: Set(p.entry_id),
        status: Set(1),
        blob_id: Set(Some(p.blob_id)),
        mtime: Set(p.mtime),
        digest: Set(p.digest),
        size: Set(p.size),
        fast_digest: Set(p.fast_digest),
        updated_at: Set(now),
    };
    entry_cache::Entity::insert(am)
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([entry_cache::Column::RepositoryId, entry_cache::Column::Path])
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
            sea_orm::sea_query::OnConflict::columns([entry_cache::Column::RepositoryId, entry_cache::Column::Path])
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

// ──────────────────────────────────────────────────────────────────────────────
// Store
// ──────────────────────────────────────────────────────────────────────────────

/// Get or create a store by name.
pub async fn get_or_create_store(
    db: &DatabaseConnection,
    name: &str,
    url: &str,
    config: serde_json::Value,
) -> anyhow::Result<store::Model> {
    if let Some(s) = store::Entity::find().filter(store::Column::Name.eq(name)).one(db).await? {
        return Ok(s);
    }
    let now = Utc::now().fixed_offset();
    let am = store::ActiveModel {
        id: Set(next_id()?),
        name: Set(name.to_owned()),
        url: Set(url.to_owned()),
        config: Set(config),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Find a store by name.
pub async fn find_store_by_name(db: &DatabaseConnection, name: &str) -> anyhow::Result<Option<store::Model>> {
    Ok(store::Entity::find().filter(store::Column::Name.eq(name)).one(db).await?)
}

/// List all stores.
pub async fn list_stores(db: &DatabaseConnection) -> anyhow::Result<Vec<store::Model>> {
    Ok(store::Entity::find().all(db).await?)
}

// ──────────────────────────────────────────────────────────────────────────────
// Replica
// ──────────────────────────────────────────────────────────────────────────────

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

    // Subquery: blob_ids already in dst.
    let dst_blob_ids: Vec<i64> = replica::Entity::find()
        .select_only()
        .column(replica::Column::BlobId)
        .filter(replica::Column::StoreId.eq(dst_store_id))
        .into_tuple()
        .all(db)
        .await?;

    // Blobs in src but not in dst.
    let src_replicas = replica::Entity::find()
        .filter(replica::Column::StoreId.eq(src_store_id))
        .filter(replica::Column::BlobId.is_not_in(dst_blob_ids))
        .all(db)
        .await?;

    let blob_ids: Vec<i64> = src_replicas.into_iter().map(|r| r.blob_id).collect();
    Ok(blob::Entity::find().filter(blob::Column::Id.is_in(blob_ids)).all(db).await?)
}

// ──────────────────────────────────────────────────────────────────────────────
// Queries for store push
// ──────────────────────────────────────────────────────────────────────────────

/// List all repositories.
pub async fn list_repositories(db: &DatabaseConnection) -> anyhow::Result<Vec<repository::Model>> {
    Ok(repository::Entity::find().all(db).await?)
}

/// Get present (status=1) entries from entry_cache filtered by path prefix, for diff.
pub async fn cache_entries_by_prefix(
    db: &DatabaseConnection,
    repository_id: i64,
    prefix: &str,
) -> anyhow::Result<Vec<entry_cache::Model>> {
    let mut q = entry_cache::Entity::find()
        .filter(entry_cache::Column::RepositoryId.eq(repository_id))
        .filter(entry_cache::Column::Status.eq(1i16));
    if !prefix.is_empty() {
        q = q.filter(entry_cache::Column::Path.like(format!("{prefix}%")));
    }
    Ok(q.order_by_asc(entry_cache::Column::Path).all(db).await?)
}

/// Get present entries from entry_cache for a repository.
pub async fn present_cache_entries(
    db: &DatabaseConnection,
    repository_id: i64,
) -> anyhow::Result<Vec<entry_cache::Model>> {
    Ok(entry_cache::Entity::find()
        .filter(entry_cache::Column::RepositoryId.eq(repository_id))
        .filter(entry_cache::Column::Status.eq(1i16))
        .all(db)
        .await?)
}

pub struct ListCacheEntriesParams {
    pub repository_id: i64,
    pub include_deleted: bool,
    pub prefix: String,
    /// 1-based page number.
    pub page: u64,
    pub per_page: u64,
}

/// List entries from entry_cache for a repository with prefix filter and pagination.
/// Returns `(items, total_count)`.
pub async fn list_cache_entries(
    db: &DatabaseConnection,
    p: &ListCacheEntriesParams,
) -> anyhow::Result<(Vec<entry_cache::Model>, u64)> {
    let mut q = entry_cache::Entity::find().filter(entry_cache::Column::RepositoryId.eq(p.repository_id));
    if !p.include_deleted {
        q = q.filter(entry_cache::Column::Status.eq(1i16));
    }
    if !p.prefix.is_empty() {
        q = q.filter(entry_cache::Column::Path.like(format!("{}%", p.prefix)));
    }

    let total = q.clone().count(db).await?;
    let offset = p.page.saturating_sub(1) * p.per_page;
    let rows = q.order_by_asc(entry_cache::Column::Path).offset(offset).limit(p.per_page).all(db).await?;
    Ok((rows, total))
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

// ──────────────────────────────────────────────────────────────────────────────
// Sync peer management
// ──────────────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────────────
// Snapshot / entry queries for sync
// ──────────────────────────────────────────────────────────────────────────────

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
        // Find created_at of the last known snapshot and fetch snapshots newer than it.
        if let Some(last_snap) = snapshot::Entity::find_by_id(last_id).one(db).await? {
            q = q.filter(snapshot::Column::CreatedAt.gt(last_snap.created_at));
        }
    }

    Ok(q.all(db).await?)
}

/// Get all entries in a snapshot.
pub async fn entries_in_snapshot(db: &DatabaseConnection, snapshot_id: i64) -> anyhow::Result<Vec<entry::Model>> {
    Ok(entry::Entity::find().filter(entry::Column::SnapshotId.eq(snapshot_id)).all(db).await?)
}

/// Find a blob by digest.
pub async fn find_blob_by_digest(db: &DatabaseConnection, digest: &[u8]) -> anyhow::Result<Option<blob::Model>> {
    Ok(blob::Entity::find().filter(blob::Column::Digest.eq(digest)).one(db).await?)
}

/// Find a blob by primary key ID.
pub async fn find_blob_by_id(db: &DatabaseConnection, id: i64) -> anyhow::Result<Option<blob::Model>> {
    Ok(blob::Entity::find_by_id(id).one(db).await?)
}

// ──────────────────────────────────────────────────────────────────────────────
// Entries with blob digest
// ──────────────────────────────────────────────────────────────────────────────

/// Fetch all entries for a snapshot with their associated blob, optionally filtered by path prefix.
pub async fn entries_with_digest(
    db: &DatabaseConnection,
    snapshot_id: i64,
    path_prefix: &str,
) -> anyhow::Result<Vec<(entry::Model, Option<blob::Model>)>> {
    let mut q = entry::Entity::find().filter(entry::Column::SnapshotId.eq(snapshot_id));
    if !path_prefix.is_empty() {
        q = q.filter(entry::Column::Path.like(format!("{path_prefix}%")));
    }
    Ok(q.find_also_related(blob::Entity).all(db).await?)
}

// ──────────────────────────────────────────────────────────────────────────────
// Diff queries
// ──────────────────────────────────────────────────────────────────────────────

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
pub async fn path_history(
    db: &DatabaseConnection,
    repository_id: i64,
    path: &str,
) -> anyhow::Result<Vec<(entry::Model, snapshot::Model)>> {
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

    let entries = entry::Entity::find()
        .filter(entry::Column::SnapshotId.is_in(snapshot_ids))
        .filter(entry::Column::Path.eq(path))
        .all(db)
        .await?;

    let mut result: Vec<(entry::Model, snapshot::Model)> =
        entries.into_iter().filter_map(|e| snapshot_map.get(&e.snapshot_id).map(|s| (e, s.clone()))).collect();
    result.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
    Ok(result)
}

/// Fetch all entries that reference a specific blob, with their snapshot, newest first.
pub async fn entries_for_blob(
    db: &DatabaseConnection,
    blob_id: i64,
) -> anyhow::Result<Vec<(entry::Model, snapshot::Model)>> {
    let entries = entry::Entity::find().filter(entry::Column::BlobId.eq(blob_id)).all(db).await?;

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

// ──────────────────────────────────────────────────────────────────────────────
// Tag operations
// ──────────────────────────────────────────────────────────────────────────────

/// Upsert a tag on a blob: replaces all existing tags with the same (blob_id, key).
pub async fn upsert_tag(
    db: &DatabaseConnection,
    blob_id: i64,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<tag::Model> {
    // Delete existing tags for this (blob_id, key).
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

/// Find a blob by hex digest string (full 64-char or shorter prefix).
/// Returns an error if the prefix is ambiguous.
pub async fn find_blob_by_hex(db: &DatabaseConnection, hex: &str) -> anyhow::Result<Option<blob::Model>> {
    let hex = hex.to_lowercase();
    // Decode the hex prefix to bytes.
    let prefix_bytes = (0..hex.len())
        .step_by(2)
        .filter(|&i| i + 1 < hex.len())
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| anyhow::anyhow!("invalid hex digest: {:?}", hex))?;

    if prefix_bytes.len() == 32 {
        // Exact match.
        return find_blob_by_digest(db, &prefix_bytes).await;
    }

    // Prefix search: fetch all blobs and filter in Rust.
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

// ──────────────────────────────────────────────────────────────────────────────
// GC helpers
// ──────────────────────────────────────────────────────────────────────────────

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

/// Return the set of blob IDs referenced by any entry in the given snapshots.
pub async fn blob_ids_in_snapshots(db: &DatabaseConnection, snapshot_ids: &[i64]) -> anyhow::Result<HashSet<i64>> {
    if snapshot_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let ids: Vec<i64> = entry::Entity::find()
        .filter(entry::Column::SnapshotId.is_in(snapshot_ids.iter().copied()))
        .filter(entry::Column::BlobId.is_not_null())
        .select_only()
        .column(entry::Column::BlobId)
        .into_tuple::<Option<i64>>()
        .all(db)
        .await?
        .into_iter()
        .flatten()
        .collect();
    Ok(ids.into_iter().collect())
}

/// Return blobs that are not referenced by any entry (truly orphaned).
pub async fn unreferenced_blobs(db: &DatabaseConnection) -> anyhow::Result<Vec<blob::Model>> {
    let referenced: HashSet<i64> = entry::Entity::find()
        .filter(entry::Column::BlobId.is_not_null())
        .select_only()
        .column(entry::Column::BlobId)
        .into_tuple::<Option<i64>>()
        .all(db)
        .await?
        .into_iter()
        .flatten()
        .collect();

    let all = blob::Entity::find().all(db).await?;
    Ok(all.into_iter().filter(|b| !referenced.contains(&b.id)).collect())
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

/// Delete blob records by IDs; returns the count deleted.
pub async fn delete_blob_records(db: &DatabaseConnection, ids: &[i64]) -> anyhow::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = blob::Entity::delete_many().filter(blob::Column::Id.is_in(ids.iter().copied())).exec(db).await?;
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
