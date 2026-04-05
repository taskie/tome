use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use sea_orm::DatabaseConnection;

use crate::config::StoreConfig;

use super::{
    scan::{self, ScanArgs},
    store::{self, StoreArgs, StoreCommands, StoreCopyArgs, StorePushArgs},
    sync::{self, SyncArgs, SyncCommands, SyncPullArgs, SyncPushArgs},
};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct PushArgs {
    /// Sync peer name
    pub peer: String,

    /// Repository name [default: "default"]
    #[arg(long, short = 'r', env = "TOME_REPO", default_value = "default")]
    pub repo: String,

    /// Store name to push blobs to [config: store.default_store]
    #[arg(long)]
    pub store: Option<String>,

    /// Directory to scan (default: current directory)
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Skip the scan step (use existing snapshot)
    #[arg(long)]
    pub no_scan: bool,

    /// Skip the store push step (sync metadata only)
    #[arg(long)]
    pub no_store: bool,

    /// Machine ID to record as the sync source
    #[arg(long)]
    pub machine_id: Option<i16>,
}

#[derive(Args)]
pub struct PullArgs {
    /// Sync peer name
    pub peer: String,

    /// Repository name [default: "default"]
    #[arg(long, short = 'r', env = "TOME_REPO", default_value = "default")]
    pub repo: String,

    /// Also copy blobs from the remote store to a local store
    #[arg(long)]
    pub with_blobs: bool,

    /// Source store name for blob copy (required with --with-blobs)
    #[arg(long)]
    pub store: Option<String>,

    /// Destination local store name for blob copy [default: "local"]
    #[arg(long)]
    pub local_store: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// push
// ──────────────────────────────────────────────────────────────────────────────

pub async fn push(db: &DatabaseConnection, args: PushArgs, cfg: &StoreConfig) -> Result<()> {
    // 1. scan
    if !args.no_scan {
        scan::run(
            db,
            ScanArgs {
                repo: args.repo.clone(),
                no_ignore: false,
                message: String::new(),
                digest_algorithm: tome_core::hash::DigestAlgorithm::Sha256,
                fast_hash_algorithm: tome_core::hash::FastHashAlgorithm::default(),
                batch_size: 1000,
                allow_empty: false,
                path: args.path.clone(),
            },
        )
        .await
        .context("scan failed")?;
    }

    // 2. store push
    if !args.no_store {
        store::run(
            db,
            StoreArgs {
                command: StoreCommands::Push(StorePushArgs {
                    repo: args.repo.clone(),
                    store: args.store.clone(),
                    path: args.path.clone(),
                    encrypt: false,
                    key_file: None,
                    key_source: None,
                    cipher: None,
                }),
            },
            cfg,
        )
        .await
        .context("store push failed")?;
    }

    // 3. sync push
    sync::run(
        db,
        SyncArgs {
            command: SyncCommands::Push(SyncPushArgs { name: args.peer, repo: args.repo, machine_id: args.machine_id }),
        },
    )
    .await
    .context("sync push failed")?;

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// pull
// ──────────────────────────────────────────────────────────────────────────────

pub async fn pull(db: &DatabaseConnection, args: PullArgs, cfg: &StoreConfig) -> Result<()> {
    // 1. sync pull
    sync::run(db, SyncArgs { command: SyncCommands::Pull(SyncPullArgs { name: args.peer, repo: args.repo.clone() }) })
        .await
        .context("sync pull failed")?;

    // 2. store copy (--with-blobs)
    if args.with_blobs {
        let src = args
            .store
            .or_else(|| cfg.default_store.clone())
            .context("--store is required with --with-blobs (or set store.default_store in tome.toml)")?;
        let dst = args.local_store.unwrap_or_else(|| "local".to_string());

        store::run(
            db,
            StoreArgs {
                command: StoreCommands::Copy(StoreCopyArgs {
                    src,
                    dst,
                    encrypt: false,
                    key_file: None,
                    key_source: None,
                    cipher: None,
                }),
            },
            cfg,
        )
        .await
        .context("store copy failed")?;
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry points (called from main.rs dispatch)
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run_push(db: &DatabaseConnection, args: PushArgs, cfg: &StoreConfig) -> Result<()> {
    push(db, args, cfg).await
}

pub async fn run_pull(db: &DatabaseConnection, args: PullArgs, cfg: &StoreConfig) -> Result<()> {
    pull(db, args, cfg).await
}
