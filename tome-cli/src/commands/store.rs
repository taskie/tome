use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_core::hash;
use tome_db::ops;
use tome_store::{encrypted::EncryptedStorage, factory, storage::blob_path, Storage};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct StoreArgs {
    #[command(subcommand)]
    pub command: StoreCommands,
}

#[derive(Subcommand)]
pub enum StoreCommands {
    /// Register a new store
    Add(StoreAddArgs),
    /// List registered stores
    List,
    /// Upload scanned files from a repository to a store
    Push(StorePushArgs),
    /// Copy blobs from one store to another
    Copy(StoreCopyArgs),
}

#[derive(Args)]
pub struct StoreAddArgs {
    /// Store name
    pub name: String,
    /// Store URL (file:///path, ssh://user@host/path, s3://bucket/prefix)
    pub url: String,
}

#[derive(Args)]
pub struct StorePushArgs {
    /// Repository name to push (default: "default")
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Store name to push to
    pub store: String,
    /// Root directory where scanned files reside (overrides snapshot metadata)
    pub path: Option<std::path::PathBuf>,
}

#[derive(Args)]
pub struct StoreCopyArgs {
    /// Source store name
    pub src: String,
    /// Destination store name
    pub dst: String,
    /// Encrypt blobs in the destination store
    #[arg(long)]
    pub encrypt: bool,
    /// Path to 32-byte binary key file (required when --encrypt is set)
    #[arg(long)]
    pub key_file: Option<std::path::PathBuf>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: StoreArgs) -> Result<()> {
    match args.command {
        StoreCommands::Add(a) => store_add(db, a).await,
        StoreCommands::List => store_list(db).await,
        StoreCommands::Push(a) => store_push(db, a).await,
        StoreCommands::Copy(a) => store_copy(db, a).await,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// store add
// ──────────────────────────────────────────────────────────────────────────────

async fn store_add(db: &DatabaseConnection, args: StoreAddArgs) -> Result<()> {
    let store = ops::get_or_create_store(db, &args.name, &args.url, serde_json::json!({})).await?;
    println!("store registered: {} (id={}, url={})", store.name, store.id, store.url);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// store list
// ──────────────────────────────────────────────────────────────────────────────

async fn store_list(db: &DatabaseConnection) -> Result<()> {
    let stores = ops::list_stores(db).await?;
    if stores.is_empty() {
        println!("no stores registered");
        return Ok(());
    }
    println!("{:<20} {:<8} {}", "name", "id", "url");
    println!("{}", "-".repeat(60));
    for s in stores {
        println!("{:<20} {:<8} {}", s.name, s.id, s.url);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// store push
// ──────────────────────────────────────────────────────────────────────────────

async fn store_push(db: &DatabaseConnection, args: StorePushArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;

    let store = ops::find_store_by_name(db, &args.store)
        .await?
        .ok_or_else(|| anyhow::anyhow!("store {:?} not found", args.store))?;

    // Determine scan root: CLI arg > snapshot metadata > error
    let scan_root = if let Some(p) = args.path {
        p.canonicalize()?
    } else {
        let meta = ops::latest_snapshot_metadata(db, repo.id).await?;
        let root_str = meta
            .as_ref()
            .and_then(|m| m.get("scan_root"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("could not determine scan_root; pass <path> explicitly"))?
            .to_owned();
        std::path::PathBuf::from(root_str)
    };

    let storage = factory::open_storage(&store.url).await?;
    let entries = ops::present_cache_entries(db, repo.id).await?;

    if entries.is_empty() {
        println!("no present files in repository {:?}", args.repo);
        return Ok(());
    }

    println!("pushing {} file(s) to store {:?} ...", entries.len(), args.store);
    let mut pushed = 0u64;
    let mut skipped = 0u64;
    let mut errors = 0u64;

    for cache in &entries {
        let blob_id = match cache.blob_id {
            Some(id) => id,
            None => continue,
        };

        // Skip if replica already exists.
        if ops::replica_exists(db, blob_id, store.id).await? {
            skipped += 1;
            continue;
        }

        let digest_hex = cache
            .digest
            .as_ref()
            .map(|d| hash::hex_encode(d))
            .unwrap_or_else(|| format!("blob-{}", blob_id));

        let local_file = scan_root.join(&cache.path);
        if !local_file.exists() {
            info!("file not found, skipping: {:?}", local_file);
            errors += 1;
            continue;
        }

        let remote_path = blob_path(&digest_hex);
        match storage.upload(&remote_path, &local_file).await {
            Ok(()) => {
                ops::insert_replica(db, blob_id, store.id, &remote_path, false).await?;
                info!("pushed: {}", cache.path);
                pushed += 1;
            }
            Err(e) => {
                warn!("failed to push {:?}: {}", cache.path, e);
                errors += 1;
            }
        }
    }

    println!("done: {} pushed, {} skipped (already present), {} errors", pushed, skipped, errors);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// store copy
// ──────────────────────────────────────────────────────────────────────────────

async fn store_copy(db: &DatabaseConnection, args: StoreCopyArgs) -> Result<()> {
    let src_store = ops::find_store_by_name(db, &args.src)
        .await?
        .ok_or_else(|| anyhow::anyhow!("source store {:?} not found", args.src))?;
    let dst_store = ops::find_store_by_name(db, &args.dst)
        .await?
        .ok_or_else(|| anyhow::anyhow!("destination store {:?} not found", args.dst))?;

    // Resolve encryption key if needed.
    let key: Option<[u8; 32]> = if args.encrypt {
        let key_path = match args.key_file {
            Some(ref p) => p.clone(),
            None => {
                let default_dir = factory::key_dir();
                bail!(
                    "--key-file is required when --encrypt is set (default key dir: {:?})",
                    default_dir
                )
            }
        };
        Some(factory::read_key_file(&key_path)?)
    } else {
        None
    };

    // Open source storage.
    let src_storage = factory::open_storage(&src_store.url).await?;

    // Find blobs missing in dst.
    let blobs = ops::blobs_missing_in_dst(db, src_store.id, dst_store.id).await?;
    if blobs.is_empty() {
        println!("nothing to copy: all blobs already present in {:?}", args.dst);
        return Ok(());
    }
    println!("copying {} blob(s) from {:?} to {:?} ...", blobs.len(), args.src, args.dst);

    let mut copied = 0u64;
    let mut errors = 0u64;

    // Use a temp dir for intermediate files.
    let tmp_dir = tempfile::tempdir()?;

    for blob in &blobs {
        let digest_hex = hash::hex_encode(&blob.digest);

        // Find source replica path.
        let src_replicas = ops::replicas_in_store(db, src_store.id).await?;
        let src_replica = match src_replicas.iter().find(|r| r.blob_id == blob.id) {
            Some(r) => r.clone(),
            None => {
                warn!("no replica found in src for blob {}", digest_hex);
                errors += 1;
                continue;
            }
        };

        let tmp_file = tmp_dir.path().join(&digest_hex);

        // Download from src.
        match src_storage.download(&src_replica.path, &tmp_file).await {
            Ok(()) => {}
            Err(e) => {
                warn!("failed to download blob {}: {}", digest_hex, e);
                errors += 1;
                continue;
            }
        }

        // Determine destination path and upload.
        let dst_path = blob_path(&digest_hex);
        let upload_result = if let Some(key) = key {
            // Open dst storage wrapped with encryption.
            let dst_inner = factory::open_storage(&dst_store.url).await?;
            let enc = EncryptedStorage::new(dst_inner, key);
            enc.upload(&dst_path, &tmp_file).await
        } else {
            let dst_storage = factory::open_storage(&dst_store.url).await?;
            dst_storage.upload(&dst_path, &tmp_file).await
        };

        match upload_result {
            Ok(()) => {}
            Err(e) => {
                warn!("failed to upload blob {}: {}", digest_hex, e);
                errors += 1;
                continue;
            }
        }

        // Record replica in DB.
        ops::insert_replica(db, blob.id, dst_store.id, &dst_path, args.encrypt).await?;
        info!("copied: {}", digest_hex);
        copied += 1;
    }

    println!("done: {} copied, {} errors", copied, errors);
    Ok(())
}
