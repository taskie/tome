//! Shared test helpers for tome-cli integration tests.
#![allow(dead_code)]

use std::path::PathBuf;

use tempfile::TempDir;
use tome_cli::{
    commands::{diff, gc, push, remote, restore, scan, store, sync, tag, verify},
    config::StoreConfig,
};
use tome_core::hash::{DigestAlgorithm, FastHashAlgorithm};
use tome_db::{connection, entities::snapshot, ops};

/// A self-contained test environment: temp directory for files + a fresh SQLite DB.
///
/// Layout:
/// ```
/// root/
///   tome.db          ← SQLite database
///   files/           ← directory being scanned
///   store/           ← local store directory (created on first use)
/// ```
pub struct Env {
    /// Root temp directory (keeps the TempDir alive for the test lifetime).
    pub root: TempDir,
    pub db: sea_orm::DatabaseConnection,
}

impl Env {
    pub async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let files_dir = root.path().join("files");
        std::fs::create_dir_all(&files_dir).unwrap();

        let db_path = root.path().join("tome.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let db = connection::open(&db_url).await.unwrap();

        Env { root, db }
    }

    /// Absolute path of the scan root directory.
    pub fn files_dir(&self) -> PathBuf {
        self.root.path().join("files")
    }

    /// Absolute path of the local store directory (created on demand).
    pub fn store_dir(&self) -> PathBuf {
        let p = self.root.path().join("store");
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Write `content` to `files/<rel>`, creating parent directories as needed.
    pub fn write(&self, rel: &str, content: &[u8]) -> PathBuf {
        let path = self.files_dir().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Remove `files/<rel>`.
    pub fn remove(&self, rel: &str) {
        std::fs::remove_file(self.files_dir().join(rel)).unwrap();
    }

    /// Run `tome scan` on `files/` using the "default" repository.
    pub async fn scan(&self) -> anyhow::Result<()> {
        self.scan_with("default", "").await
    }

    /// Run `tome scan` keeping the snapshot even if nothing changed.
    pub async fn scan_allow_empty(&self) -> anyhow::Result<()> {
        scan::run(
            &self.db,
            scan::ScanArgs {
                repo: "default".to_string(),
                no_ignore: true,
                message: String::new(),
                digest_algorithm: DigestAlgorithm::Sha256,
                fast_hash_algorithm: FastHashAlgorithm::default(),
                batch_size: 1000,
                allow_empty: true,
                path: Some(self.files_dir()),
            },
        )
        .await
    }

    /// Run `tome scan` with a custom repo name and optional message.
    pub async fn scan_with(&self, repo: &str, message: &str) -> anyhow::Result<()> {
        self.scan_with_opts(repo, message, false).await
    }

    /// Run `tome scan` with a custom repo name, keeping empty snapshots.
    pub async fn scan_with_allow_empty(&self, repo: &str, message: &str) -> anyhow::Result<()> {
        self.scan_with_opts(repo, message, true).await
    }

    async fn scan_with_opts(&self, repo: &str, message: &str, allow_empty: bool) -> anyhow::Result<()> {
        scan::run(
            &self.db,
            scan::ScanArgs {
                repo: repo.to_string(),
                no_ignore: true,
                message: message.to_string(),
                digest_algorithm: DigestAlgorithm::Sha256,
                fast_hash_algorithm: FastHashAlgorithm::default(),
                batch_size: 1000,
                allow_empty,
                path: Some(self.files_dir()),
            },
        )
        .await
    }

    /// Return all snapshots for the "default" repo, newest first.
    pub async fn snapshots(&self) -> Vec<snapshot::Model> {
        let repo = ops::get_or_create_repository(&self.db, "default").await.unwrap();
        ops::list_snapshots_for_repo(&self.db, repo.id).await.unwrap()
    }

    /// Return the metadata of the most recent snapshot for the "default" repo.
    pub async fn last_meta(&self) -> serde_json::Value {
        self.snapshots().await.into_iter().next().unwrap().metadata
    }

    /// Return all present (status=1) files in the entry_cache for the "default" repo.
    pub async fn present_entries(&self) -> Vec<tome_db::entities::entry_cache::Model> {
        let repo = ops::get_or_create_repository(&self.db, "default").await.unwrap();
        ops::present_cache_entries(&self.db, repo.id).await.unwrap()
    }

    /// Return sorted file paths of all present entries.
    pub async fn present_paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = self.present_entries().await.into_iter().map(|e| e.path).collect();
        paths.sort();
        paths
    }

    /// Run `tome verify` on `files/`, returning Ok or Err depending on file integrity.
    pub async fn verify(&self) -> anyhow::Result<()> {
        verify::run(
            &self.db,
            verify::VerifyArgs {
                repo: "default".to_string(),
                path: Some(self.files_dir()),
                verbose: false,
                store: None,
            },
        )
        .await
    }

    /// Run `tome store add` then `tome store push` using a local filesystem store.
    pub async fn store_add_and_push(&self, store_name: &str) -> anyhow::Result<()> {
        let store_url = format!("file://{}", self.store_dir().display());
        store::run(
            &self.db,
            store::StoreArgs {
                command: store::StoreCommands::Add(store::StoreAddArgs {
                    name: store_name.to_string(),
                    url: store_url,
                    encrypt: false,
                    key_file: None,
                    key_source: None,
                    cipher: None,
                }),
            },
            &StoreConfig::default(),
        )
        .await?;
        store::run(
            &self.db,
            store::StoreArgs {
                command: store::StoreCommands::Push(store::StorePushArgs {
                    repo: "default".to_string(),
                    store: Some(store_name.to_string()),
                    path: Some(self.files_dir()),
                    encrypt: false,
                    key_file: None,
                    key_source: None,
                    cipher: None,
                }),
            },
            &StoreConfig::default(),
        )
        .await
    }

    /// Run `tome gc` with the given arguments.
    pub async fn gc(&self, args: gc::GcArgs) -> anyhow::Result<()> {
        gc::run(&self.db, args).await
    }

    /// Run `tome diff` between two snapshot IDs.
    pub async fn diff(&self, snap1: &str, snap2: &str, prefix: &str) -> anyhow::Result<()> {
        diff::run(
            &self.db,
            diff::DiffArgs {
                snapshot1: snap1.to_string(),
                snapshot2: snap2.to_string(),
                repo: "default".to_string(),
                prefix: prefix.to_string(),
                name_only: false,
                stat: false,
            },
        )
        .await
    }

    /// Run `tome restore` from a snapshot to a destination directory.
    pub async fn restore(
        &self,
        snapshot_id: &str,
        dest: PathBuf,
        store_name: Option<&str>,
        prefix: &str,
    ) -> anyhow::Result<()> {
        restore::run(
            &self.db,
            restore::RestoreArgs {
                snapshot: snapshot_id.to_string(),
                repo: "default".to_string(),
                store: store_name.map(|s| s.to_string()),
                prefix: prefix.to_string(),
                dest,
            },
        )
        .await
    }

    /// Run `tome tag set`.
    pub async fn tag_set(&self, digest: &str, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        tag::run(
            &self.db,
            tag::TagArgs {
                command: tag::TagCommands::Set(tag::TagSetArgs {
                    digest: digest.to_string(),
                    key: key.to_string(),
                    value: value.map(|v| v.to_string()),
                }),
            },
        )
        .await
    }

    /// Run `tome tag delete`.
    pub async fn tag_delete(&self, digest: &str, key: &str) -> anyhow::Result<()> {
        tag::run(
            &self.db,
            tag::TagArgs {
                command: tag::TagCommands::Delete(tag::TagDeleteArgs {
                    digest: digest.to_string(),
                    key: key.to_string(),
                }),
            },
        )
        .await
    }

    /// Run `tome tag rm`.
    pub async fn tag_rm(&self, digest: &str, key: &str) -> anyhow::Result<()> {
        tag::run(
            &self.db,
            tag::TagArgs {
                command: tag::TagCommands::Rm(tag::TagDeleteArgs { digest: digest.to_string(), key: key.to_string() }),
            },
        )
        .await
    }

    /// Run `tome tag list`.
    pub async fn tag_list(&self, digest: &str) -> anyhow::Result<()> {
        tag::run(
            &self.db,
            tag::TagArgs { command: tag::TagCommands::List(tag::TagListArgs { digest: digest.to_string() }) },
        )
        .await
    }

    /// Run `tome tag search`.
    pub async fn tag_search(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        tag::run(
            &self.db,
            tag::TagArgs {
                command: tag::TagCommands::Search(tag::TagSearchArgs {
                    key: key.to_string(),
                    value: value.map(|v| v.to_string()),
                }),
            },
        )
        .await
    }

    /// Run `tome store verify`.
    pub async fn store_verify(&self, store_name: &str) -> anyhow::Result<()> {
        store::run(
            &self.db,
            store::StoreArgs {
                command: store::StoreCommands::Verify(store::StoreVerifyArgs {
                    store: store_name.to_string(),
                    digest_algorithm: DigestAlgorithm::Sha256,
                }),
            },
            &StoreConfig::default(),
        )
        .await
    }

    /// Return the hex-encoded digest of the first blob in the database.
    pub async fn first_blob_digest_hex(&self) -> String {
        let repo = ops::get_or_create_repository(&self.db, "default").await.unwrap();
        let entries = ops::present_cache_entries(&self.db, repo.id).await.unwrap();
        let blob_id = entries[0].object_id.unwrap();
        let blobs = ops::objects_by_ids(&self.db, &[blob_id]).await.unwrap();
        tome_core::hash::hex_encode(&blobs[0].digest)
    }

    // ── store set / rm helpers ──────────────────────────────────────────────

    /// Run `tome store set`.
    pub async fn store_set(&self, name: &str, url: Option<&str>) -> anyhow::Result<()> {
        store::run(
            &self.db,
            store::StoreArgs {
                command: store::StoreCommands::Set(store::StoreSetArgs {
                    name: name.to_string(),
                    url: url.map(|u| u.to_string()),
                    encrypt: false,
                    no_encrypt: false,
                    key_file: None,
                    key_source: None,
                    cipher: None,
                }),
            },
            &StoreConfig::default(),
        )
        .await
    }

    /// Run `tome store rm`.
    pub async fn store_rm(&self, name: &str, force: bool) -> anyhow::Result<()> {
        store::run(
            &self.db,
            store::StoreArgs {
                command: store::StoreCommands::Rm(store::StoreRmArgs { name: name.to_string(), force }),
            },
            &StoreConfig::default(),
        )
        .await
    }

    /// Run `tome store list` (returns Ok if no error).
    pub async fn store_list(&self) -> anyhow::Result<()> {
        store::run(&self.db, store::StoreArgs { command: store::StoreCommands::List }, &StoreConfig::default()).await
    }

    // ── sync helpers ────────────────────────────────────────────────────────

    /// Run `tome sync config`.
    pub async fn sync_config(
        &self,
        name: &str,
        key: Option<&str>,
        value: Option<&str>,
        unset: Option<&str>,
        list: bool,
        repo: &str,
    ) -> anyhow::Result<()> {
        sync::run(
            &self.db,
            sync::SyncArgs {
                command: sync::SyncCommands::Config(sync::SyncConfigArgs {
                    name: name.to_string(),
                    key: key.map(|s| s.to_string()),
                    value: value.map(|s| s.to_string()),
                    unset: unset.map(|s| s.to_string()),
                    list,
                    repo: repo.to_string(),
                }),
            },
        )
        .await
    }

    // ── remote helpers ──────────────────────────────────────────────────────

    /// Run `tome remote add`.
    pub async fn remote_add(
        &self,
        name: &str,
        peer_url: &str,
        repo: &str,
        peer_repo: Option<&str>,
    ) -> anyhow::Result<()> {
        remote::run(
            &self.db,
            remote::RemoteArgs {
                command: remote::RemoteCommands::Add(remote::RemoteAddArgs {
                    name: name.to_string(),
                    peer_url: peer_url.to_string(),
                    repo: repo.to_string(),
                    peer_repo: peer_repo.map(|s| s.to_string()),
                }),
            },
        )
        .await
    }

    /// Run `tome remote set`.
    pub async fn remote_set(
        &self,
        name: &str,
        peer_url: Option<&str>,
        peer_repo: Option<&str>,
        repo: &str,
    ) -> anyhow::Result<()> {
        remote::run(
            &self.db,
            remote::RemoteArgs {
                command: remote::RemoteCommands::Set(remote::RemoteSetArgs {
                    name: name.to_string(),
                    peer_url: peer_url.map(|s| s.to_string()),
                    peer_repo: peer_repo.map(|s| s.to_string()),
                    repo: repo.to_string(),
                }),
            },
        )
        .await
    }

    /// Run `tome remote rm`.
    pub async fn remote_rm(&self, name: &str, repo: &str) -> anyhow::Result<()> {
        remote::run(
            &self.db,
            remote::RemoteArgs {
                command: remote::RemoteCommands::Rm(remote::RemoteRmArgs {
                    name: name.to_string(),
                    repo: repo.to_string(),
                }),
            },
        )
        .await
    }

    /// Run `tome remote list`.
    pub async fn remote_list(&self, repo: &str) -> anyhow::Result<()> {
        remote::run(
            &self.db,
            remote::RemoteArgs {
                command: remote::RemoteCommands::List(remote::RemoteListArgs { repo: repo.to_string() }),
            },
        )
        .await
    }

    // ── push / pull helpers ─────────────────────────────────────────────────

    /// Run `tome push <peer>` (scan + store push + sync push).
    pub async fn push(
        &self,
        peer: &str,
        repo: &str,
        store_name: Option<&str>,
        no_scan: bool,
        no_store: bool,
        machine_id: Option<i16>,
    ) -> anyhow::Result<()> {
        push::run_push(
            &self.db,
            push::PushArgs {
                peer: peer.to_string(),
                repo: repo.to_string(),
                store: store_name.map(|s| s.to_string()),
                path: Some(self.files_dir()),
                no_scan,
                no_store,
                machine_id,
            },
            &StoreConfig::default(),
        )
        .await
    }

    /// Run `tome pull <peer>` (sync pull + optional store copy).
    pub async fn pull(
        &self,
        peer: &str,
        repo: &str,
        with_blobs: bool,
        store_name: Option<&str>,
        local_store: Option<&str>,
    ) -> anyhow::Result<()> {
        push::run_pull(
            &self.db,
            push::PullArgs {
                peer: peer.to_string(),
                repo: repo.to_string(),
                with_blobs,
                store: store_name.map(|s| s.to_string()),
                local_store: local_store.map(|s| s.to_string()),
            },
            &StoreConfig::default(),
        )
        .await
    }
}

/// Extract a u64 count from snapshot metadata by key.
pub fn meta_count(meta: &serde_json::Value, key: &str) -> u64 {
    meta.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}
