use chrono::Utc;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

use crate::entities::entry_cache;
use crate::ops::entry::path_depth;

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
    pub object_id: i64,
    pub mtime: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub digest: Option<Vec<u8>>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,
    pub mode: Option<i32>,
}

/// Upsert (insert or replace) a cache row for a present file.
pub async fn upsert_cache_present<C: ConnectionTrait>(conn: &C, p: UpsertCachePresentParams) -> anyhow::Result<()> {
    let now = Utc::now().fixed_offset();
    let depth = path_depth(&p.path);
    let am = entry_cache::ActiveModel {
        repository_id: Set(p.repository_id),
        path: Set(p.path),
        snapshot_id: Set(p.snapshot_id),
        entry_id: Set(p.entry_id),
        status: Set(1),
        object_id: Set(Some(p.object_id)),
        mtime: Set(p.mtime),
        digest: Set(p.digest),
        size: Set(p.size),
        fast_digest: Set(p.fast_digest),
        depth: Set(depth),
        mode: Set(p.mode),
        updated_at: Set(now),
    };
    entry_cache::Entity::insert(am)
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([entry_cache::Column::RepositoryId, entry_cache::Column::Path])
                .update_columns([
                    entry_cache::Column::SnapshotId,
                    entry_cache::Column::EntryId,
                    entry_cache::Column::Status,
                    entry_cache::Column::ObjectId,
                    entry_cache::Column::Mtime,
                    entry_cache::Column::Digest,
                    entry_cache::Column::Size,
                    entry_cache::Column::FastDigest,
                    entry_cache::Column::Depth,
                    entry_cache::Column::Mode,
                    entry_cache::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(conn)
        .await?;
    Ok(())
}

/// Upsert a cache row for a deleted file.
pub async fn upsert_cache_deleted<C: ConnectionTrait>(
    conn: &C,
    repository_id: i64,
    path: &str,
    snapshot_id: i64,
    entry_id: i64,
) -> anyhow::Result<()> {
    let now = Utc::now().fixed_offset();
    let depth = path_depth(path);
    let am = entry_cache::ActiveModel {
        repository_id: Set(repository_id),
        path: Set(path.to_owned()),
        snapshot_id: Set(snapshot_id),
        entry_id: Set(entry_id),
        status: Set(0),
        object_id: Set(None),
        mtime: Set(None),
        digest: Set(None),
        size: Set(None),
        fast_digest: Set(None),
        depth: Set(depth),
        mode: Set(None),
        updated_at: Set(now),
    };
    entry_cache::Entity::insert(am)
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([entry_cache::Column::RepositoryId, entry_cache::Column::Path])
                .update_columns([
                    entry_cache::Column::SnapshotId,
                    entry_cache::Column::EntryId,
                    entry_cache::Column::Status,
                    entry_cache::Column::ObjectId,
                    entry_cache::Column::Mtime,
                    entry_cache::Column::Digest,
                    entry_cache::Column::Size,
                    entry_cache::Column::FastDigest,
                    entry_cache::Column::Depth,
                    entry_cache::Column::Mode,
                    entry_cache::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(conn)
        .await?;
    Ok(())
}

/// Get entries from entry_cache filtered by path prefix.
/// When `include_deleted` is false, only status=1 (present) entries are returned.
pub async fn cache_entries_by_prefix(
    db: &DatabaseConnection,
    repository_id: i64,
    prefix: &str,
    include_deleted: bool,
) -> anyhow::Result<Vec<entry_cache::Model>> {
    let mut q = entry_cache::Entity::find().filter(entry_cache::Column::RepositoryId.eq(repository_id));
    if !include_deleted {
        q = q.filter(entry_cache::Column::Status.eq(1i16));
    }
    if !prefix.is_empty() {
        q = q.filter(entry_cache::Column::Path.like(format!("{prefix}%")));
    }
    Ok(q.order_by_asc(entry_cache::Column::Path).all(db).await?)
}

/// Get all entries from entry_cache for a repository (both present and deleted).
pub async fn all_cache_entries(db: &DatabaseConnection, repository_id: i64) -> anyhow::Result<Vec<entry_cache::Model>> {
    Ok(entry_cache::Entity::find().filter(entry_cache::Column::RepositoryId.eq(repository_id)).all(db).await?)
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

pub struct ListDirEntriesParams {
    pub repository_id: i64,
    pub include_deleted: bool,
    /// Normalized directory prefix (e.g., "src/" or "" for root).
    pub dir: String,
    /// Target depth (number of '/' in the dir prefix).
    pub depth: i16,
    /// 1-based page number.
    pub page: u64,
    pub per_page: u64,
}

/// List direct children of a directory from entry_cache.
/// Uses the (repository_id, depth, path) index for efficient lookup.
/// Returns `(items, total_count)`.
pub async fn list_dir_entries(
    db: &DatabaseConnection,
    p: &ListDirEntriesParams,
) -> anyhow::Result<(Vec<entry_cache::Model>, u64)> {
    let mut q = entry_cache::Entity::find()
        .filter(entry_cache::Column::RepositoryId.eq(p.repository_id))
        .filter(entry_cache::Column::Depth.eq(p.depth));
    if !p.include_deleted {
        q = q.filter(entry_cache::Column::Status.eq(1i16));
    }
    if !p.dir.is_empty() {
        q = q.filter(entry_cache::Column::Path.like(format!("{}%", p.dir)));
    }

    let total = q.clone().count(db).await?;
    let offset = p.page.saturating_sub(1) * p.per_page;
    let rows = q.order_by_asc(entry_cache::Column::Path).offset(offset).limit(p.per_page).all(db).await?;
    Ok((rows, total))
}
