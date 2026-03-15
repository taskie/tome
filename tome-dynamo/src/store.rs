//! DynamoStore — MetadataStore implementation backed by Amazon DynamoDB.

use std::collections::HashMap;

use anyhow::{Context, bail};
use async_trait::async_trait;
use aws_sdk_dynamodb::Client;
use chrono::{DateTime, FixedOffset};
use serde_json::Value;

use tome_core::hash::{FileHash, hex_encode};
use tome_core::id::next_id;
use tome_db::entities::{blob, entry, entry_cache, machine, replica, repository, snapshot, store, sync_peer, tag};
use tome_db::ops::{ListCacheEntriesParams, UpsertCachePresentParams};
use tome_db::store_trait::MetadataStore;

use crate::keys;
use crate::serde_ddb::*;

pub struct DynamoStore {
    client: Client,
    table: String,
}

impl DynamoStore {
    pub fn new(client: Client, table: String) -> Self {
        Self { client, table }
    }

    async fn get_item(&self, pk: &str, sk: &str) -> anyhow::Result<Option<Item>> {
        let out = self
            .client
            .get_item()
            .table_name(&self.table)
            .key("PK", s(pk))
            .key("SK", s(sk))
            .send()
            .await
            .context("DynamoDB GetItem")?;
        Ok(out.item)
    }

    #[allow(dead_code)]
    async fn get_item_consistent(&self, pk: &str, sk: &str) -> anyhow::Result<Option<Item>> {
        let out = self
            .client
            .get_item()
            .table_name(&self.table)
            .key("PK", s(pk))
            .key("SK", s(sk))
            .consistent_read(true)
            .send()
            .await
            .context("DynamoDB GetItem (consistent)")?;
        Ok(out.item)
    }

    async fn put_item(&self, item: Item) -> anyhow::Result<()> {
        self.client.put_item().table_name(&self.table).set_item(Some(item)).send().await.context("DynamoDB PutItem")?;
        Ok(())
    }

    async fn put_item_if_not_exists(&self, item: Item) -> anyhow::Result<bool> {
        let result = self
            .client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(PK)")
            .send()
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(err) => {
                if let Some(service_err) = err.as_service_error() {
                    if service_err.is_conditional_check_failed_exception() {
                        return Ok(false);
                    }
                }
                Err(err).context("DynamoDB PutItem conditional")
            }
        }
    }

    #[allow(dead_code)]
    async fn query_pk(&self, pk: &str) -> anyhow::Result<Vec<Item>> {
        self.query_pk_sk_begins(pk, None).await
    }

    async fn query_pk_sk_begins(&self, pk: &str, sk_prefix: Option<&str>) -> anyhow::Result<Vec<Item>> {
        let mut items = Vec::new();
        let mut last_key = None;
        loop {
            let mut builder = self
                .client
                .query()
                .table_name(&self.table)
                .key_condition_expression(if sk_prefix.is_some() {
                    "PK = :pk AND begins_with(SK, :skp)"
                } else {
                    "PK = :pk"
                })
                .expression_attribute_values(":pk", s(pk));
            if let Some(prefix) = sk_prefix {
                builder = builder.expression_attribute_values(":skp", s(prefix));
            }
            if let Some(key) = last_key.take() {
                builder = builder.set_exclusive_start_key(Some(key));
            }
            let out = builder.send().await.context("DynamoDB Query")?;
            if let Some(page) = out.items {
                items.extend(page);
            }
            last_key = out.last_evaluated_key;
            if last_key.is_none() {
                break;
            }
        }
        Ok(items)
    }

    async fn query_pk_sk_after(&self, pk: &str, sk_prefix: &str, after_sk: &str) -> anyhow::Result<Vec<Item>> {
        let mut items = Vec::new();
        let mut last_key = None;
        loop {
            let mut builder = self
                .client
                .query()
                .table_name(&self.table)
                .key_condition_expression("PK = :pk AND SK > :after")
                .expression_attribute_values(":pk", s(pk))
                .expression_attribute_values(":after", s(after_sk));
            if let Some(key) = last_key.take() {
                builder = builder.set_exclusive_start_key(Some(key));
            }
            let _ = sk_prefix; // used as context hint only
            let out = builder.send().await.context("DynamoDB Query (after)")?;
            if let Some(page) = out.items {
                items.extend(page);
            }
            last_key = out.last_evaluated_key;
            if last_key.is_none() {
                break;
            }
        }
        Ok(items)
    }

    async fn query_gsi(
        &self,
        index: &str,
        pk_attr: &str,
        pk_val: &str,
        sk_attr: Option<&str>,
        sk_val: Option<&str>,
    ) -> anyhow::Result<Vec<Item>> {
        let mut items = Vec::new();
        let mut last_key = None;
        loop {
            let mut builder = self.client.query().table_name(&self.table).index_name(index);

            if let (Some(sk_a), Some(sk_v)) = (sk_attr, sk_val) {
                builder = builder
                    .key_condition_expression(format!("{pk_attr} = :pk AND {sk_a} = :sk"))
                    .expression_attribute_values(":pk", s(pk_val))
                    .expression_attribute_values(":sk", s(sk_v));
            } else {
                builder = builder
                    .key_condition_expression(format!("{pk_attr} = :pk"))
                    .expression_attribute_values(":pk", s(pk_val));
            }
            if let Some(key) = last_key.take() {
                builder = builder.set_exclusive_start_key(Some(key));
            }
            let out = builder.send().await.context("DynamoDB GSI Query")?;
            if let Some(page) = out.items {
                items.extend(page);
            }
            last_key = out.last_evaluated_key;
            if last_key.is_none() {
                break;
            }
        }
        Ok(items)
    }

    /// Resolve a repository name from PK (e.g. "REPO#myrepo" → "myrepo").
    #[allow(dead_code)]
    fn repo_name_from_pk(pk: &str) -> &str {
        pk.strip_prefix("REPO#").unwrap_or(pk)
    }
}

// ── Item ↔ Model conversions ─────────────────────────────────────────────────

fn item_to_repository(item: &Item) -> anyhow::Result<repository::Model> {
    Ok(repository::Model {
        id: get_n_i64(item, "id")?,
        name: get_s(item, "name")?,
        description: get_s(item, "description").unwrap_or_default(),
        config: get_json_or_null(item, "config"),
        created_at: get_datetime(item, "created_at")?,
        updated_at: get_datetime(item, "updated_at")?,
    })
}

fn item_to_snapshot(item: &Item) -> anyhow::Result<snapshot::Model> {
    Ok(snapshot::Model {
        id: get_n_i64(item, "id")?,
        repository_id: get_n_i64(item, "repository_id")?,
        parent_id: get_n_i64_opt(item, "parent_id"),
        message: get_s(item, "message").unwrap_or_default(),
        metadata: get_json_or_null(item, "metadata"),
        source_machine_id: get_n_i16_opt(item, "source_machine_id"),
        source_snapshot_id: get_n_i64_opt(item, "source_snapshot_id"),
        created_at: get_datetime(item, "created_at")?,
    })
}

fn item_to_blob(item: &Item) -> anyhow::Result<blob::Model> {
    Ok(blob::Model {
        id: get_n_i64(item, "id")?,
        digest: get_bytes(item, "digest")?,
        size: get_n_i64(item, "size")?,
        fast_digest: get_n_i64(item, "fast_digest")?,
        created_at: get_datetime(item, "created_at")?,
    })
}

fn item_to_entry(item: &Item) -> anyhow::Result<entry::Model> {
    Ok(entry::Model {
        id: get_n_i64(item, "id")?,
        snapshot_id: get_n_i64(item, "snapshot_id")?,
        path: get_s(item, "path")?,
        status: get_n_i16(item, "status")?,
        blob_id: get_n_i64_opt(item, "blob_id"),
        mode: get_n_i32_opt(item, "mode"),
        mtime: get_datetime_opt(item, "mtime"),
        created_at: get_datetime(item, "created_at")?,
    })
}

fn item_to_entry_cache(item: &Item) -> anyhow::Result<entry_cache::Model> {
    Ok(entry_cache::Model {
        repository_id: get_n_i64(item, "repository_id")?,
        path: get_s(item, "path")?,
        snapshot_id: get_n_i64(item, "snapshot_id")?,
        entry_id: get_n_i64(item, "entry_id")?,
        status: get_n_i16(item, "status")?,
        blob_id: get_n_i64_opt(item, "blob_id"),
        mtime: get_datetime_opt(item, "mtime"),
        digest: get_bytes_opt(item, "digest"),
        size: get_n_i64_opt(item, "size"),
        fast_digest: get_n_i64_opt(item, "fast_digest"),
        updated_at: get_datetime(item, "updated_at")?,
    })
}

fn item_to_store(item: &Item) -> anyhow::Result<store::Model> {
    Ok(store::Model {
        id: get_n_i64(item, "id")?,
        name: get_s(item, "name")?,
        url: get_s(item, "url")?,
        config: get_json_or_null(item, "config"),
        created_at: get_datetime(item, "created_at")?,
        updated_at: get_datetime(item, "updated_at")?,
    })
}

fn item_to_replica(item: &Item) -> anyhow::Result<replica::Model> {
    Ok(replica::Model {
        id: get_n_i64(item, "id")?,
        blob_id: get_n_i64(item, "blob_id")?,
        store_id: get_n_i64(item, "store_id")?,
        path: get_s(item, "path")?,
        encrypted: get_bool(item, "encrypted")?,
        verified_at: get_datetime_opt(item, "verified_at"),
        created_at: get_datetime(item, "created_at")?,
    })
}

fn item_to_machine(item: &Item) -> anyhow::Result<machine::Model> {
    Ok(machine::Model {
        machine_id: get_n_i16(item, "machine_id")?,
        name: get_s(item, "name")?,
        description: get_s(item, "description").unwrap_or_default(),
        last_seen_at: get_datetime_opt(item, "last_seen_at"),
        created_at: get_datetime(item, "created_at")?,
    })
}

fn item_to_tag(item: &Item) -> anyhow::Result<tag::Model> {
    Ok(tag::Model {
        id: get_n_i64(item, "id")?,
        blob_id: get_n_i64(item, "blob_id")?,
        key: get_s(item, "key")?,
        value: get_s_opt(item, "value"),
        created_at: get_datetime(item, "created_at")?,
    })
}

fn item_to_sync_peer(item: &Item) -> anyhow::Result<sync_peer::Model> {
    Ok(sync_peer::Model {
        id: get_n_i64(item, "id")?,
        name: get_s(item, "name")?,
        url: get_s(item, "url")?,
        repository_id: get_n_i64(item, "repository_id")?,
        last_synced_at: get_datetime_opt(item, "last_synced_at"),
        last_snapshot_id: get_n_i64_opt(item, "last_snapshot_id"),
        config: get_json_or_null(item, "config"),
        created_at: get_datetime(item, "created_at")?,
        updated_at: get_datetime(item, "updated_at")?,
    })
}

// ── MetadataStore implementation ─────────────────────────────────────────────

#[async_trait]
impl MetadataStore for DynamoStore {
    // ── Repository ──────────────────────────────────────────────────────

    async fn list_repositories(&self) -> anyhow::Result<Vec<repository::Model>> {
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("REPO"), None, None).await?;
        items.iter().map(item_to_repository).collect()
    }

    async fn find_repository_by_name(&self, name: &str) -> anyhow::Result<Option<repository::Model>> {
        match self.get_item(&keys::repo_pk(name), keys::META_SK).await? {
            Some(item) => Ok(Some(item_to_repository(&item)?)),
            None => Ok(None),
        }
    }

    async fn get_or_create_repository(&self, name: &str) -> anyhow::Result<repository::Model> {
        if let Some(repo) = self.find_repository_by_name(name).await? {
            return Ok(repo);
        }
        let now = now_iso();
        let id = next_id()?;
        let mut item = HashMap::new();
        item.insert("PK".into(), s(&keys::repo_pk(name)));
        item.insert("SK".into(), s(keys::META_SK));
        item.insert("GSI3PK".into(), s(&keys::gsi3pk_type("REPO")));
        item.insert("GSI3SK".into(), s(name));
        item.insert("id".into(), n_i64(id));
        item.insert("name".into(), s(name));
        item.insert("description".into(), s(""));
        item.insert("config".into(), json_val(&Value::Null));
        item.insert("created_at".into(), s(&now));
        item.insert("updated_at".into(), s(&now));

        let created = self.put_item_if_not_exists(item).await?;
        if !created {
            // Race: another writer created it first; re-read.
            return self
                .find_repository_by_name(name)
                .await?
                .ok_or_else(|| anyhow::anyhow!("repository vanished after conflict"));
        }
        self.find_repository_by_name(name).await?.ok_or_else(|| anyhow::anyhow!("repository not found after creation"))
    }

    // ── Snapshot ────────────────────────────────────────────────────────

    async fn create_snapshot(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
    ) -> anyhow::Result<snapshot::Model> {
        self.create_snapshot_inner(repository_id, parent_id, message, None, None).await
    }

    async fn create_snapshot_with_source(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
        source_machine_id: i16,
        source_snapshot_id: i64,
    ) -> anyhow::Result<snapshot::Model> {
        self.create_snapshot_inner(repository_id, parent_id, message, Some(source_machine_id), Some(source_snapshot_id))
            .await
    }

    async fn latest_snapshot(&self, repository_id: i64) -> anyhow::Result<Option<snapshot::Model>> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        // Query snapshots under REPO# in reverse order, limit 1
        let out = self
            .client
            .query()
            .table_name(&self.table)
            .key_condition_expression("PK = :pk AND begins_with(SK, :skp)")
            .expression_attribute_values(":pk", s(&pk))
            .expression_attribute_values(":skp", s("SNAP#"))
            .scan_index_forward(false)
            .limit(1)
            .consistent_read(true)
            .send()
            .await
            .context("DynamoDB Query latest snapshot")?;
        match out.items.and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) }) {
            Some(item) => Ok(Some(item_to_snapshot(&item)?)),
            None => Ok(None),
        }
    }

    async fn find_snapshot_by_id(&self, id: i64) -> anyhow::Result<Option<snapshot::Model>> {
        // Snapshot items are stored under REPO#<name> / SNAP#<id>.
        // We need to find the repo. Use GSI3 to find the snapshot by type.
        // Alternative: store a SNAP#<id> / #META item as well. For now, scan GSI3.
        let items = self
            .query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("SNAP"), Some("GSI3SK"), Some(&keys::pad_id(id)))
            .await?;
        match items.first() {
            Some(item) => Ok(Some(item_to_snapshot(item)?)),
            None => Ok(None),
        }
    }

    async fn find_snapshot_by_source(
        &self,
        repository_id: i64,
        source_machine_id: i16,
        source_snapshot_id: i64,
    ) -> anyhow::Result<Option<snapshot::Model>> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let items = self
            .query_gsi(
                "GSI1",
                "GSI1PK",
                &keys::gsi1pk_source(&repo_name, source_machine_id),
                Some("GSI1SK"),
                Some(&keys::gsi1sk_source(source_snapshot_id)),
            )
            .await?;
        match items.first() {
            Some(item) => Ok(Some(item_to_snapshot(item)?)),
            None => Ok(None),
        }
    }

    async fn snapshots_after(&self, repository_id: i64, after: Option<i64>) -> anyhow::Result<Vec<snapshot::Model>> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        let items = match after {
            Some(after_id) => {
                let after_sk = keys::snap_sk(after_id);
                self.query_pk_sk_after(&pk, "SNAP#", &after_sk).await?
            }
            None => self.query_pk_sk_begins(&pk, Some("SNAP#")).await?,
        };
        // Filter only SNAP# items (query_pk_sk_after might include STORE# etc if SK > after_sk)
        items
            .iter()
            .filter(|item| item.get("SK").and_then(|v| v.as_s().ok()).is_some_and(|sk| sk.starts_with("SNAP#")))
            .map(item_to_snapshot)
            .collect()
    }

    async fn list_snapshots_for_repo(&self, repository_id: i64) -> anyhow::Result<Vec<snapshot::Model>> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        let items = self.query_pk_sk_begins(&pk, Some("SNAP#")).await?;
        items.iter().map(item_to_snapshot).collect()
    }

    async fn update_snapshot_metadata(&self, snapshot_id: i64, metadata: Value) -> anyhow::Result<()> {
        // Find the snapshot to get its repo PK
        let snap = self
            .find_snapshot_by_id(snapshot_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("snapshot {snapshot_id} not found"))?;
        let repo_name = self.resolve_repo_name(snap.repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        let sk = keys::snap_sk(snapshot_id);

        self.client
            .update_item()
            .table_name(&self.table)
            .key("PK", s(&pk))
            .key("SK", s(&sk))
            .update_expression("SET metadata = :m")
            .expression_attribute_values(":m", json_val(&metadata))
            .send()
            .await
            .context("DynamoDB UpdateItem snapshot metadata")?;
        Ok(())
    }

    // ── Blob ────────────────────────────────────────────────────────────

    async fn get_or_create_blob(&self, file_hash: &FileHash) -> anyhow::Result<blob::Model> {
        let digest_hex = hex_encode(&file_hash.digest);
        let pk = keys::blob_pk(&digest_hex);

        if let Some(item) = self.get_item(&pk, keys::META_SK).await? {
            return item_to_blob(&item);
        }

        let now = now_iso();
        let id = next_id()?;
        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(keys::META_SK));
        item.insert("id".into(), n_i64(id));
        item.insert("digest".into(), b(&file_hash.digest));
        item.insert("size".into(), n_i64(file_hash.size as i64));
        item.insert("fast_digest".into(), n_i64(file_hash.fast_digest));
        item.insert("created_at".into(), s(&now));

        let created = self.put_item_if_not_exists(item).await?;
        if !created {
            // Race: re-read
            return self
                .find_blob_by_digest(&file_hash.digest)
                .await?
                .ok_or_else(|| anyhow::anyhow!("blob vanished after conflict"));
        }
        self.find_blob_by_digest(&file_hash.digest)
            .await?
            .ok_or_else(|| anyhow::anyhow!("blob not found after creation"))
    }

    async fn find_blob_by_digest(&self, digest: &[u8]) -> anyhow::Result<Option<blob::Model>> {
        let digest_hex = hex_encode(digest);
        let pk = keys::blob_pk(&digest_hex);
        match self.get_item(&pk, keys::META_SK).await? {
            Some(item) => Ok(Some(item_to_blob(&item)?)),
            None => Ok(None),
        }
    }

    async fn blobs_by_ids(&self, ids: &[i64]) -> anyhow::Result<Vec<blob::Model>> {
        // DynamoDB BatchGetItem requires knowing the PK. Since we have IDs not digests,
        // we use GSI3 to find blobs by type, then filter. For small sets this is acceptable.
        // TODO: consider a BLOB_ID#<id> → digest lookup item for O(1) access
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let all_blobs = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("BLOB"), None, None).await?;
        let id_set: std::collections::HashSet<i64> = ids.iter().copied().collect();
        all_blobs
            .iter()
            .filter_map(|item| {
                let id = get_n_i64(item, "id").ok()?;
                if id_set.contains(&id) { Some(item_to_blob(item)) } else { None }
            })
            .collect()
    }

    // ── Entry ───────────────────────────────────────────────────────────

    async fn insert_entry_present(
        &self,
        snapshot_id: i64,
        path: &str,
        blob_id: i64,
        mode: Option<i32>,
        mtime: Option<DateTime<FixedOffset>>,
    ) -> anyhow::Result<entry::Model> {
        let id = next_id()?;
        let now = now_iso();

        // Denormalize: look up blob for embedded fields
        let snap = self
            .find_snapshot_by_id(snapshot_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("snapshot {snapshot_id} not found"))?;
        let repo_name = self.resolve_repo_name(snap.repository_id).await?;

        let pk = keys::snap_pk(snapshot_id);
        let sk = keys::entry_sk(path);

        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(&sk));
        // GSI2 for path history
        item.insert("GSI2PK".into(), s(&keys::gsi2pk_path(&repo_name, path)));
        item.insert("GSI2SK".into(), s(&keys::gsi2sk_snap(snapshot_id)));
        item.insert("id".into(), n_i64(id));
        item.insert("snapshot_id".into(), n_i64(snapshot_id));
        item.insert("path".into(), s(path));
        item.insert("status".into(), n_i16(1)); // present
        item.insert("blob_id".into(), n_i64(blob_id));
        if let Some(m) = mode {
            item.insert("mode".into(), n_i32(m));
        }
        if let Some(mt) = &mtime {
            item.insert("mtime".into(), s(&datetime_iso(mt)));
        }
        item.insert("created_at".into(), s(&now));

        self.put_item(item).await?;

        Ok(entry::Model {
            id,
            snapshot_id,
            path: path.to_owned(),
            status: 1,
            blob_id: Some(blob_id),
            mode,
            mtime,
            created_at: now.parse()?,
        })
    }

    async fn insert_entry_deleted(&self, snapshot_id: i64, path: &str) -> anyhow::Result<entry::Model> {
        let id = next_id()?;
        let now = now_iso();

        let snap = self
            .find_snapshot_by_id(snapshot_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("snapshot {snapshot_id} not found"))?;
        let repo_name = self.resolve_repo_name(snap.repository_id).await?;

        let pk = keys::snap_pk(snapshot_id);
        let sk = keys::entry_sk(path);

        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(&sk));
        item.insert("GSI2PK".into(), s(&keys::gsi2pk_path(&repo_name, path)));
        item.insert("GSI2SK".into(), s(&keys::gsi2sk_snap(snapshot_id)));
        item.insert("id".into(), n_i64(id));
        item.insert("snapshot_id".into(), n_i64(snapshot_id));
        item.insert("path".into(), s(path));
        item.insert("status".into(), n_i16(2)); // deleted
        item.insert("created_at".into(), s(&now));

        self.put_item(item).await?;

        Ok(entry::Model {
            id,
            snapshot_id,
            path: path.to_owned(),
            status: 2,
            blob_id: None,
            mode: None,
            mtime: None,
            created_at: now.parse()?,
        })
    }

    async fn entries_with_digest(
        &self,
        snapshot_id: i64,
        prefix: &str,
    ) -> anyhow::Result<Vec<(entry::Model, Option<blob::Model>)>> {
        let pk = keys::snap_pk(snapshot_id);
        let sk_prefix = if prefix.is_empty() { "ENTRY#".to_owned() } else { format!("ENTRY#{prefix}") };
        let items = self.query_pk_sk_begins(&pk, Some(&sk_prefix)).await?;

        let mut results = Vec::with_capacity(items.len());
        // Collect unique blob digests for batch lookup
        let mut blob_cache: HashMap<i64, blob::Model> = HashMap::new();

        for item in &items {
            let entry = item_to_entry(item)?;
            let blob = if let Some(blob_id) = entry.blob_id {
                if let Some(cached) = blob_cache.get(&blob_id) {
                    Some(cached.clone())
                } else {
                    // Look up blob by denormalized digest if available, else by ID scan
                    let blob_digest = get_bytes_opt(item, "blob_digest");
                    let found = if let Some(digest) = blob_digest {
                        self.find_blob_by_digest(&digest).await?
                    } else {
                        // Fallback: find blob by ID (expensive)
                        let blobs = self.blobs_by_ids(&[blob_id]).await?;
                        blobs.into_iter().next()
                    };
                    if let Some(ref b) = found {
                        blob_cache.insert(blob_id, b.clone());
                    }
                    found
                }
            } else {
                None
            };
            results.push((entry, blob));
        }

        Ok(results)
    }

    async fn entries_by_prefix(&self, snapshot_id: i64, prefix: &str) -> anyhow::Result<Vec<entry::Model>> {
        let pk = keys::snap_pk(snapshot_id);
        let sk_prefix = if prefix.is_empty() { "ENTRY#".to_owned() } else { format!("ENTRY#{prefix}") };
        let items = self.query_pk_sk_begins(&pk, Some(&sk_prefix)).await?;
        items.iter().map(item_to_entry).collect()
    }

    async fn entries_for_blob(&self, blob_id: i64) -> anyhow::Result<Vec<(entry::Model, snapshot::Model)>> {
        // No direct index on blob_id for entries. Scan GSI3 for all snapshots,
        // then query each. This is expensive but rarely used in the server hot path.
        // TODO: consider adding a GSI or denormalized item for blob → entries
        let _ = blob_id;
        bail!("entries_for_blob is not efficiently supported on DynamoDB; use SQL backend for blob analysis")
    }

    async fn path_history(
        &self,
        repository_id: i64,
        path: &str,
    ) -> anyhow::Result<Vec<(entry::Model, Option<blob::Model>, snapshot::Model)>> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let gsi2pk = keys::gsi2pk_path(&repo_name, path);
        let items = self.query_gsi("GSI2", "GSI2PK", &gsi2pk, None, None).await?;

        let mut results = Vec::with_capacity(items.len());
        for item in &items {
            let entry = item_to_entry(item)?;
            let snap = self
                .find_snapshot_by_id(entry.snapshot_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("snapshot {} not found", entry.snapshot_id))?;
            let blob = if let Some(blob_id) = entry.blob_id {
                let blobs = self.blobs_by_ids(&[blob_id]).await?;
                blobs.into_iter().next()
            } else {
                None
            };
            results.push((entry, blob, snap));
        }
        Ok(results)
    }

    // ── Entry Cache ─────────────────────────────────────────────────────

    async fn upsert_cache_present(&self, params: UpsertCachePresentParams) -> anyhow::Result<()> {
        let repo_name = self.resolve_repo_name(params.repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        let sk = keys::cache_sk(&params.path);
        let now = now_iso();

        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(&sk));
        item.insert("repository_id".into(), n_i64(params.repository_id));
        item.insert("path".into(), s(&params.path));
        item.insert("snapshot_id".into(), n_i64(params.snapshot_id));
        item.insert("entry_id".into(), n_i64(params.entry_id));
        item.insert("status".into(), n_i16(1));
        item.insert("blob_id".into(), n_i64(params.blob_id));
        if let Some(mt) = &params.mtime {
            item.insert("mtime".into(), s(&datetime_iso(mt)));
        }
        if let Some(ref d) = params.digest {
            item.insert("digest".into(), b(d));
        }
        if let Some(sz) = params.size {
            item.insert("size".into(), n_i64(sz));
        }
        if let Some(fd) = params.fast_digest {
            item.insert("fast_digest".into(), n_i64(fd));
        }
        item.insert("updated_at".into(), s(&now));

        self.put_item(item).await
    }

    async fn upsert_cache_deleted(
        &self,
        repository_id: i64,
        path: &str,
        snapshot_id: i64,
        entry_id: i64,
    ) -> anyhow::Result<()> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        let sk = keys::cache_sk(path);
        let now = now_iso();

        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(&sk));
        item.insert("repository_id".into(), n_i64(repository_id));
        item.insert("path".into(), s(path));
        item.insert("snapshot_id".into(), n_i64(snapshot_id));
        item.insert("entry_id".into(), n_i64(entry_id));
        item.insert("status".into(), n_i16(2));
        item.insert("updated_at".into(), s(&now));

        self.put_item(item).await
    }

    async fn cache_entries_by_prefix(
        &self,
        repository_id: i64,
        prefix: &str,
        include_deleted: bool,
    ) -> anyhow::Result<Vec<entry_cache::Model>> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let pk = keys::repo_pk(&repo_name);
        let sk_prefix = if prefix.is_empty() { "CACHE#".to_owned() } else { format!("CACHE#{prefix}") };
        let items = self.query_pk_sk_begins(&pk, Some(&sk_prefix)).await?;

        let mut results = Vec::with_capacity(items.len());
        for item in &items {
            let cache = item_to_entry_cache(item)?;
            if !include_deleted && cache.status == 2 {
                continue;
            }
            results.push(cache);
        }
        Ok(results)
    }

    async fn list_cache_entries(
        &self,
        params: &ListCacheEntriesParams,
    ) -> anyhow::Result<(Vec<entry_cache::Model>, u64)> {
        let all = self.cache_entries_by_prefix(params.repository_id, &params.prefix, params.include_deleted).await?;
        let total = all.len() as u64;
        let start = ((params.page - 1) * params.per_page) as usize;
        let page: Vec<_> = all.into_iter().skip(start).take(params.per_page as usize).collect();
        Ok((page, total))
    }

    // ── Store ───────────────────────────────────────────────────────────

    async fn list_stores(&self) -> anyhow::Result<Vec<store::Model>> {
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("STORE"), None, None).await?;
        items.iter().map(item_to_store).collect()
    }

    async fn get_or_create_store(&self, name: &str, url: &str, config: Value) -> anyhow::Result<store::Model> {
        let pk = keys::store_pk(name);
        if let Some(item) = self.get_item(&pk, keys::META_SK).await? {
            return item_to_store(&item);
        }

        let now = now_iso();
        let id = next_id()?;
        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(keys::META_SK));
        item.insert("GSI3PK".into(), s(&keys::gsi3pk_type("STORE")));
        item.insert("GSI3SK".into(), s(name));
        item.insert("id".into(), n_i64(id));
        item.insert("name".into(), s(name));
        item.insert("url".into(), s(url));
        item.insert("config".into(), json_val(&config));
        item.insert("created_at".into(), s(&now));
        item.insert("updated_at".into(), s(&now));

        let created = self.put_item_if_not_exists(item).await?;
        if !created {
            return self
                .get_item(&pk, keys::META_SK)
                .await?
                .map(|i| item_to_store(&i))
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("store vanished after conflict"));
        }
        self.get_item(&pk, keys::META_SK)
            .await?
            .map(|i| item_to_store(&i))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("store not found after creation"))
    }

    // ── Replica ─────────────────────────────────────────────────────────

    async fn replica_exists(&self, blob_id: i64, store_id: i64) -> anyhow::Result<bool> {
        // We need the blob digest and store name to construct the key.
        let blobs = self.blobs_by_ids(&[blob_id]).await?;
        let blob = match blobs.first() {
            Some(b) => b,
            None => return Ok(false),
        };
        let digest_hex = hex_encode(&blob.digest);
        let pk = keys::blob_pk(&digest_hex);

        // Find the store name
        let store = self.find_store_by_id(store_id).await?;
        let store = match store {
            Some(st) => st,
            None => return Ok(false),
        };

        let sk = keys::replica_sk(&store.name);
        Ok(self.get_item(&pk, &sk).await?.is_some())
    }

    async fn insert_replica(
        &self,
        blob_id: i64,
        store_id: i64,
        path: &str,
        encrypted: bool,
    ) -> anyhow::Result<replica::Model> {
        let blobs = self.blobs_by_ids(&[blob_id]).await?;
        let blob = blobs.first().ok_or_else(|| anyhow::anyhow!("blob {blob_id} not found"))?;
        let digest_hex = hex_encode(&blob.digest);
        let pk = keys::blob_pk(&digest_hex);

        let store =
            self.find_store_by_id(store_id).await?.ok_or_else(|| anyhow::anyhow!("store {store_id} not found"))?;
        let sk = keys::replica_sk(&store.name);

        let now = now_iso();
        let id = next_id()?;
        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(&sk));
        item.insert("id".into(), n_i64(id));
        item.insert("blob_id".into(), n_i64(blob_id));
        item.insert("store_id".into(), n_i64(store_id));
        item.insert("path".into(), s(path));
        item.insert("encrypted".into(), bool_val(encrypted));
        item.insert("store_name".into(), s(&store.name));
        item.insert("store_url".into(), s(&store.url));
        item.insert("created_at".into(), s(&now));

        self.put_item(item).await?;

        Ok(replica::Model {
            id,
            blob_id,
            store_id,
            path: path.to_owned(),
            encrypted,
            verified_at: None,
            created_at: now.parse()?,
        })
    }

    async fn replicas_for_blobs(&self, blob_ids: &[i64]) -> anyhow::Result<Vec<(replica::Model, store::Model)>> {
        if blob_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        // Build a blob_id → digest_hex map
        let blobs = self.blobs_by_ids(blob_ids).await?;
        let blob_map: HashMap<i64, String> = blobs.iter().map(|b| (b.id, hex_encode(&b.digest))).collect();

        // Cache stores by name
        let mut store_cache: HashMap<String, store::Model> = HashMap::new();

        for (blob_id, digest_hex) in &blob_map {
            let pk = keys::blob_pk(digest_hex);
            let items = self.query_pk_sk_begins(&pk, Some("REPLICA#")).await?;

            for item in items {
                let rep = item_to_replica(&item)?;
                let store_name = get_s_opt(&item, "store_name").unwrap_or_default();

                let st = if let Some(cached) = store_cache.get(&store_name) {
                    cached.clone()
                } else {
                    let found = self
                        .get_item(&keys::store_pk(&store_name), keys::META_SK)
                        .await?
                        .map(|i| item_to_store(&i))
                        .transpose()?;
                    if let Some(ref st) = found {
                        store_cache.insert(store_name.clone(), st.clone());
                    }
                    match found {
                        Some(st) => st,
                        None => continue,
                    }
                };
                let _ = blob_id; // used above in map iteration
                results.push((rep, st));
            }
        }

        Ok(results)
    }

    // ── Tag ─────────────────────────────────────────────────────────────

    async fn list_all_tags(&self) -> anyhow::Result<Vec<tag::Model>> {
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("TAG"), None, None).await?;
        items.iter().map(item_to_tag).collect()
    }

    // ── Sync Peer ───────────────────────────────────────────────────────

    async fn list_all_sync_peers(&self) -> anyhow::Result<Vec<sync_peer::Model>> {
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("SYNCPEER"), None, None).await?;
        items.iter().map(item_to_sync_peer).collect()
    }

    // ── Machine ─────────────────────────────────────────────────────────

    async fn list_machines(&self) -> anyhow::Result<Vec<machine::Model>> {
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("MACHINE"), None, None).await?;
        items.iter().map(item_to_machine).collect()
    }

    async fn register_machine(&self, name: &str, description: &str) -> anyhow::Result<machine::Model> {
        let id = next_id()?;
        let machine_id = (id & 0x7FFF) as i16; // fit into i16
        let now = now_iso();
        let pk = keys::machine_pk(machine_id);

        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(keys::META_SK));
        item.insert("GSI3PK".into(), s(&keys::gsi3pk_type("MACHINE")));
        item.insert("GSI3SK".into(), n_i16(machine_id));
        item.insert("machine_id".into(), n_i16(machine_id));
        item.insert("name".into(), s(name));
        item.insert("description".into(), s(description));
        item.insert("created_at".into(), s(&now));

        self.put_item(item).await?;

        Ok(machine::Model {
            machine_id,
            name: name.to_owned(),
            description: description.to_owned(),
            last_seen_at: None,
            created_at: now.parse()?,
        })
    }

    async fn find_machine_by_id(&self, machine_id: i16) -> anyhow::Result<Option<machine::Model>> {
        let pk = keys::machine_pk(machine_id);
        match self.get_item(&pk, keys::META_SK).await? {
            Some(item) => Ok(Some(item_to_machine(&item)?)),
            None => Ok(None),
        }
    }

    async fn update_machine_last_seen(&self, machine_id: i16) -> anyhow::Result<()> {
        let pk = keys::machine_pk(machine_id);
        let now = now_iso();
        self.client
            .update_item()
            .table_name(&self.table)
            .key("PK", s(&pk))
            .key("SK", s(keys::META_SK))
            .update_expression("SET last_seen_at = :t")
            .expression_attribute_values(":t", s(&now))
            .send()
            .await
            .context("DynamoDB UpdateItem machine last_seen")?;
        Ok(())
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

impl DynamoStore {
    /// Resolve repository_id → name. Caching could be added later.
    async fn resolve_repo_name(&self, repository_id: i64) -> anyhow::Result<String> {
        // Query GSI3 for the repo with this ID
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("REPO"), None, None).await?;
        for item in &items {
            if get_n_i64(item, "id").ok() == Some(repository_id) {
                return get_s(item, "name");
            }
        }
        bail!("repository with id {repository_id} not found")
    }

    /// Find store by numeric ID.
    async fn find_store_by_id(&self, store_id: i64) -> anyhow::Result<Option<store::Model>> {
        let items = self.query_gsi("GSI3", "GSI3PK", &keys::gsi3pk_type("STORE"), None, None).await?;
        for item in &items {
            if get_n_i64(item, "id").ok() == Some(store_id) {
                return Ok(Some(item_to_store(item)?));
            }
        }
        Ok(None)
    }

    async fn create_snapshot_inner(
        &self,
        repository_id: i64,
        parent_id: Option<i64>,
        message: &str,
        source_machine_id: Option<i16>,
        source_snapshot_id: Option<i64>,
    ) -> anyhow::Result<snapshot::Model> {
        let repo_name = self.resolve_repo_name(repository_id).await?;
        let id = next_id()?;
        let now = now_iso();
        let pk = keys::repo_pk(&repo_name);
        let sk = keys::snap_sk(id);

        let mut item = HashMap::new();
        item.insert("PK".into(), s(&pk));
        item.insert("SK".into(), s(&sk));
        // GSI3 for find_snapshot_by_id
        item.insert("GSI3PK".into(), s(&keys::gsi3pk_type("SNAP")));
        item.insert("GSI3SK".into(), s(&keys::pad_id(id)));
        item.insert("id".into(), n_i64(id));
        item.insert("repository_id".into(), n_i64(repository_id));
        if let Some(pid) = parent_id {
            item.insert("parent_id".into(), n_i64(pid));
        }
        item.insert("message".into(), s(message));
        item.insert("metadata".into(), json_val(&Value::Null));

        if let Some(mid) = source_machine_id {
            item.insert("source_machine_id".into(), n_i16(mid));
            // GSI1 for idempotency
            item.insert("GSI1PK".into(), s(&keys::gsi1pk_source(&repo_name, mid)));
            if let Some(sid) = source_snapshot_id {
                item.insert("source_snapshot_id".into(), n_i64(sid));
                item.insert("GSI1SK".into(), s(&keys::gsi1sk_source(sid)));
            }
        }
        item.insert("created_at".into(), s(&now));

        self.put_item(item).await?;

        Ok(snapshot::Model {
            id,
            repository_id,
            parent_id,
            message: message.to_owned(),
            metadata: Value::Null,
            source_machine_id,
            source_snapshot_id,
            created_at: now.parse()?,
        })
    }
}
