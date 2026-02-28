//! Shared test helpers for tome-cli integration tests.
#![allow(dead_code)]

use std::path::PathBuf;

use tempfile::TempDir;
use tome_cli::{
    commands::{gc, scan, store, verify},
    config::StoreConfig,
};
use tome_core::hash::DigestAlgorithm;
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

    /// Run `tome scan` with a custom repo name and optional message.
    pub async fn scan_with(&self, repo: &str, message: &str) -> anyhow::Result<()> {
        scan::run(
            &self.db,
            scan::ScanArgs {
                repo: repo.to_string(),
                no_ignore: true, // always ignore .gitignore in tests
                message: message.to_string(),
                digest_algorithm: DigestAlgorithm::Sha256,
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
                quiet: true, // suppress OK lines in test output
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
}

/// Extract a u64 count from snapshot metadata by key.
pub fn meta_count(meta: &serde_json::Value, key: &str) -> u64 {
    meta.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}
