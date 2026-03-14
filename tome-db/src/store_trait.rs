use async_trait::async_trait;
use chrono::DateTime;
use chrono::FixedOffset;
use serde_json::Value;

use tome_core::hash::FileHash;

use crate::entities::{blob, entry, entry_cache, machine, replica, repository, snapshot, store, sync_peer, tag};
use crate::ops::{ListCacheEntriesParams, UpsertCachePresentParams};

/// Backend-agnostic metadata store trait.
///
/// Implemented by [`crate::sea_orm_store::SeaOrmStore`] (PostgreSQL / SQLite via SeaORM)
/// and (future) `tome-dynamo::DynamoStore` (DynamoDB).
#[async_trait]
pub trait MetadataStore: Send + Sync {
    // ── Repository ──────────────────────────────────────────────────────

    async fn list_repositories(&self) -> anyhow::Result<Vec<repository::Model>>;

    async fn find_repository_by_name(&self, name: &str) -> anyhow::Result<Option<repository::Model>>;

    async fn get_or_create_repository(&self, name: &str) -> anyhow::Result<repository::Model>;

    // ── Snapshot ────────────────────────────────────────────────────────

    async fn create_snapshot(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
    ) -> anyhow::Result<snapshot::Model>;

    async fn create_snapshot_with_source(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
        source_machine_id: i16,
        source_snapshot_id: i64,
    ) -> anyhow::Result<snapshot::Model>;

    async fn latest_snapshot(&self, repository_id: i64) -> anyhow::Result<Option<snapshot::Model>>;

    async fn find_snapshot_by_id(&self, id: i64) -> anyhow::Result<Option<snapshot::Model>>;

    async fn find_snapshot_by_source(
        &self,
        repository_id: i64,
        source_machine_id: i16,
        source_snapshot_id: i64,
    ) -> anyhow::Result<Option<snapshot::Model>>;

    async fn snapshots_after(&self, repository_id: i64, after: Option<i64>) -> anyhow::Result<Vec<snapshot::Model>>;

    async fn list_snapshots_for_repo(&self, repository_id: i64) -> anyhow::Result<Vec<snapshot::Model>>;

    async fn update_snapshot_metadata(&self, snapshot_id: i64, metadata: Value) -> anyhow::Result<()>;

    // ── Blob ────────────────────────────────────────────────────────────

    async fn get_or_create_blob(&self, file_hash: &FileHash) -> anyhow::Result<blob::Model>;

    async fn find_blob_by_digest(&self, digest: &[u8]) -> anyhow::Result<Option<blob::Model>>;

    async fn blobs_by_ids(&self, ids: &[i64]) -> anyhow::Result<Vec<blob::Model>>;

    // ── Entry ───────────────────────────────────────────────────────────

    async fn insert_entry_present(
        &self,
        snapshot_id: i64,
        path: &str,
        blob_id: i64,
        mode: Option<i32>,
        mtime: Option<DateTime<FixedOffset>>,
    ) -> anyhow::Result<entry::Model>;

    async fn insert_entry_deleted(&self, snapshot_id: i64, path: &str) -> anyhow::Result<entry::Model>;

    async fn entries_with_digest(
        &self,
        snapshot_id: i64,
        prefix: &str,
    ) -> anyhow::Result<Vec<(entry::Model, Option<blob::Model>)>>;

    async fn entries_by_prefix(&self, snapshot_id: i64, prefix: &str) -> anyhow::Result<Vec<entry::Model>>;

    async fn entries_for_blob(&self, blob_id: i64) -> anyhow::Result<Vec<(entry::Model, snapshot::Model)>>;

    async fn path_history(
        &self,
        repository_id: i64,
        path: &str,
    ) -> anyhow::Result<Vec<(entry::Model, Option<blob::Model>, snapshot::Model)>>;

    // ── Entry Cache ─────────────────────────────────────────────────────

    async fn upsert_cache_present(&self, params: UpsertCachePresentParams) -> anyhow::Result<()>;

    async fn upsert_cache_deleted(
        &self,
        repository_id: i64,
        path: &str,
        snapshot_id: i64,
        entry_id: i64,
    ) -> anyhow::Result<()>;

    async fn cache_entries_by_prefix(
        &self,
        repository_id: i64,
        prefix: &str,
        include_deleted: bool,
    ) -> anyhow::Result<Vec<entry_cache::Model>>;

    async fn list_cache_entries(
        &self,
        params: &ListCacheEntriesParams,
    ) -> anyhow::Result<(Vec<entry_cache::Model>, u64)>;

    // ── Store ───────────────────────────────────────────────────────────

    async fn list_stores(&self) -> anyhow::Result<Vec<store::Model>>;

    async fn get_or_create_store(&self, name: &str, url: &str, config: Value) -> anyhow::Result<store::Model>;

    // ── Replica ─────────────────────────────────────────────────────────

    async fn replica_exists(&self, blob_id: i64, store_id: i64) -> anyhow::Result<bool>;

    async fn insert_replica(
        &self,
        blob_id: i64,
        store_id: i64,
        path: &str,
        encrypted: bool,
    ) -> anyhow::Result<replica::Model>;

    async fn replicas_for_blobs(&self, blob_ids: &[i64]) -> anyhow::Result<Vec<(replica::Model, store::Model)>>;

    // ── Tag ─────────────────────────────────────────────────────────────

    async fn list_all_tags(&self) -> anyhow::Result<Vec<tag::Model>>;

    // ── Sync Peer ───────────────────────────────────────────────────────

    async fn list_all_sync_peers(&self) -> anyhow::Result<Vec<sync_peer::Model>>;

    // ── Machine ─────────────────────────────────────────────────────────

    async fn list_machines(&self) -> anyhow::Result<Vec<machine::Model>>;

    async fn register_machine(&self, name: &str, description: &str) -> anyhow::Result<machine::Model>;

    async fn find_machine_by_id(&self, machine_id: i16) -> anyhow::Result<Option<machine::Model>>;

    async fn update_machine_last_seen(&self, machine_id: i16) -> anyhow::Result<()>;
}
