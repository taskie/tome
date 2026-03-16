use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use sea_orm::DatabaseConnection;
use serde_json::Value;

use tome_core::hash::FileHash;

use crate::entities::{entry, entry_cache, machine, object, replica, repository, snapshot, store, sync_peer, tag};
use crate::ops;
use crate::ops::{ListCacheEntriesParams, ListDirEntriesParams, UpsertCachePresentParams};
use crate::store_trait::MetadataStore;

/// SeaORM-backed [`MetadataStore`] implementation (PostgreSQL / SQLite).
pub struct SeaOrmStore {
    db: DatabaseConnection,
}

impl SeaOrmStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Access the underlying connection (for CLI-only ops not in the trait).
    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[async_trait]
impl MetadataStore for SeaOrmStore {
    // ── Repository ──────────────────────────────────────────────────────

    async fn list_repositories(&self) -> anyhow::Result<Vec<repository::Model>> {
        ops::list_repositories(&self.db).await
    }

    async fn find_repository_by_name(&self, name: &str) -> anyhow::Result<Option<repository::Model>> {
        ops::find_repository_by_name(&self.db, name).await
    }

    async fn get_or_create_repository(&self, name: &str) -> anyhow::Result<repository::Model> {
        ops::get_or_create_repository(&self.db, name).await
    }

    // ── Snapshot ────────────────────────────────────────────────────────

    async fn create_snapshot(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
    ) -> anyhow::Result<snapshot::Model> {
        ops::create_snapshot(&self.db, repository_id, parent_id, message).await
    }

    async fn create_snapshot_with_source(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
        source_machine_id: i16,
        source_snapshot_id: i64,
    ) -> anyhow::Result<snapshot::Model> {
        ops::create_snapshot_with_source(
            &self.db,
            repository_id,
            parent_id,
            message,
            source_machine_id,
            source_snapshot_id,
        )
        .await
    }

    async fn latest_snapshot(&self, repository_id: i64) -> anyhow::Result<Option<snapshot::Model>> {
        ops::latest_snapshot(&self.db, repository_id).await
    }

    async fn find_snapshot_by_id(&self, id: i64) -> anyhow::Result<Option<snapshot::Model>> {
        ops::find_snapshot_by_id(&self.db, id).await
    }

    async fn find_snapshot_by_source(
        &self,
        repository_id: i64,
        source_machine_id: i16,
        source_snapshot_id: i64,
    ) -> anyhow::Result<Option<snapshot::Model>> {
        ops::find_snapshot_by_source(&self.db, repository_id, source_machine_id, source_snapshot_id).await
    }

    async fn snapshots_after(&self, repository_id: i64, after: Option<i64>) -> anyhow::Result<Vec<snapshot::Model>> {
        ops::snapshots_after(&self.db, repository_id, after).await
    }

    async fn list_snapshots_for_repo(&self, repository_id: i64) -> anyhow::Result<Vec<snapshot::Model>> {
        ops::list_snapshots_for_repo(&self.db, repository_id).await
    }

    async fn update_snapshot_metadata(&self, snapshot_id: i64, metadata: Value) -> anyhow::Result<()> {
        ops::update_snapshot_metadata(&self.db, snapshot_id, metadata).await
    }

    // ── Object ───────────────────────────────────────────────────────────

    async fn get_or_create_blob(&self, file_hash: &FileHash) -> anyhow::Result<object::Model> {
        ops::get_or_create_blob(&self.db, file_hash).await
    }

    async fn find_object_by_digest(&self, digest: &[u8]) -> anyhow::Result<Option<object::Model>> {
        ops::find_object_by_digest(&self.db, digest).await
    }

    async fn objects_by_ids(&self, ids: &[i64]) -> anyhow::Result<Vec<object::Model>> {
        ops::objects_by_ids(&self.db, ids).await
    }

    // ── Entry ───────────────────────────────────────────────────────────

    async fn insert_entry_present(
        &self,
        snapshot_id: i64,
        path: &str,
        object_id: i64,
        mode: Option<i32>,
        mtime: Option<DateTime<FixedOffset>>,
    ) -> anyhow::Result<entry::Model> {
        ops::insert_entry_present(&self.db, snapshot_id, path, object_id, mode, mtime).await
    }

    async fn insert_entry_deleted(&self, snapshot_id: i64, path: &str) -> anyhow::Result<entry::Model> {
        ops::insert_entry_deleted(&self.db, snapshot_id, path).await
    }

    async fn entries_with_digest(
        &self,
        snapshot_id: i64,
        prefix: &str,
    ) -> anyhow::Result<Vec<(entry::Model, Option<object::Model>)>> {
        ops::entries_with_digest(&self.db, snapshot_id, prefix).await
    }

    async fn entries_by_prefix(&self, snapshot_id: i64, prefix: &str) -> anyhow::Result<Vec<entry::Model>> {
        ops::entries_by_prefix(&self.db, snapshot_id, prefix).await
    }

    async fn entries_for_object(&self, object_id: i64) -> anyhow::Result<Vec<(entry::Model, snapshot::Model)>> {
        ops::entries_for_object(&self.db, object_id).await
    }

    async fn path_history(
        &self,
        repository_id: i64,
        path: &str,
    ) -> anyhow::Result<Vec<(entry::Model, Option<object::Model>, snapshot::Model)>> {
        ops::path_history(&self.db, repository_id, path).await
    }

    // ── Entry Cache ─────────────────────────────────────────────────────

    async fn upsert_cache_present(&self, params: UpsertCachePresentParams) -> anyhow::Result<()> {
        ops::upsert_cache_present(&self.db, params).await
    }

    async fn upsert_cache_deleted(
        &self,
        repository_id: i64,
        path: &str,
        snapshot_id: i64,
        entry_id: i64,
    ) -> anyhow::Result<()> {
        ops::upsert_cache_deleted(&self.db, repository_id, path, snapshot_id, entry_id).await
    }

    async fn cache_entries_by_prefix(
        &self,
        repository_id: i64,
        prefix: &str,
        include_deleted: bool,
    ) -> anyhow::Result<Vec<entry_cache::Model>> {
        ops::cache_entries_by_prefix(&self.db, repository_id, prefix, include_deleted).await
    }

    async fn list_cache_entries(
        &self,
        params: &ListCacheEntriesParams,
    ) -> anyhow::Result<(Vec<entry_cache::Model>, u64)> {
        ops::list_cache_entries(&self.db, params).await
    }

    async fn list_dir_entries(&self, params: &ListDirEntriesParams) -> anyhow::Result<(Vec<entry_cache::Model>, u64)> {
        ops::list_dir_entries(&self.db, params).await
    }

    // ── Store ───────────────────────────────────────────────────────────

    async fn list_stores(&self) -> anyhow::Result<Vec<store::Model>> {
        ops::list_stores(&self.db).await
    }

    async fn get_or_create_store(&self, name: &str, url: &str, config: Value) -> anyhow::Result<store::Model> {
        ops::get_or_create_store(&self.db, name, url, config).await
    }

    // ── Replica ─────────────────────────────────────────────────────────

    async fn replica_exists(&self, object_id: i64, store_id: i64) -> anyhow::Result<bool> {
        ops::replica_exists(&self.db, object_id, store_id).await
    }

    async fn insert_replica(
        &self,
        object_id: i64,
        store_id: i64,
        path: &str,
        encrypted: bool,
    ) -> anyhow::Result<replica::Model> {
        ops::insert_replica(&self.db, object_id, store_id, path, encrypted).await
    }

    async fn replicas_for_objects(&self, object_ids: &[i64]) -> anyhow::Result<Vec<(replica::Model, store::Model)>> {
        ops::replicas_for_objects(&self.db, object_ids).await
    }

    // ── Tag ─────────────────────────────────────────────────────────────

    async fn list_all_tags(&self) -> anyhow::Result<Vec<tag::Model>> {
        ops::list_all_tags(&self.db).await
    }

    // ── Sync Peer ───────────────────────────────────────────────────────

    async fn list_all_sync_peers(&self) -> anyhow::Result<Vec<sync_peer::Model>> {
        ops::list_all_sync_peers(&self.db).await
    }

    // ── Machine ─────────────────────────────────────────────────────────

    async fn list_machines(&self) -> anyhow::Result<Vec<machine::Model>> {
        ops::list_machines(&self.db).await
    }

    async fn register_machine(&self, name: &str, description: &str) -> anyhow::Result<machine::Model> {
        ops::register_machine(&self.db, name, description).await
    }

    async fn find_machine_by_id(&self, machine_id: i16) -> anyhow::Result<Option<machine::Model>> {
        ops::find_machine_by_id(&self.db, machine_id).await
    }

    async fn update_machine_last_seen(&self, machine_id: i16) -> anyhow::Result<()> {
        ops::update_machine_last_seen(&self.db, machine_id).await
    }
}
