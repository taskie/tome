use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_core::hash::{self, DigestAlgorithm};
use tome_db::ops;
use tome_store::{CipherAlgorithm, Storage, encrypted::EncryptedStorage, factory, storage::blob_path};

use crate::config::{self, StoreConfig};

use super::helpers::resolve_store;

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
    /// Update an existing store
    Set(StoreSetArgs),
    /// Remove a store registration
    Rm(StoreRmArgs),
    /// List registered stores
    List,
    /// Upload scanned files from a repository to a store
    Push(StorePushArgs),
    /// Copy blobs from one store to another
    Copy(StoreCopyArgs),
    /// Verify integrity of blobs in a store
    Verify(StoreVerifyArgs),
}

#[derive(Args)]
pub struct StoreAddArgs {
    /// Store name
    pub name: String,
    /// Store URL (file:///path, ssh://user@host/path, s3://bucket/prefix)
    pub url: String,
}

#[derive(Args)]
pub struct StoreSetArgs {
    /// Store name
    pub name: String,
    /// New URL for the store
    #[arg(long)]
    pub url: Option<String>,
}

#[derive(Args)]
pub struct StoreRmArgs {
    /// Store name
    pub name: String,
    /// Force removal even if replicas exist
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct StorePushArgs {
    /// Repository name to push (default: "default")
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Store name to push to [config: store.default_store]
    pub store: Option<String>,
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
    /// Cipher algorithm for encryption: aes256gcm (default) or chacha20-poly1305
    #[arg(long, default_value = "aes256gcm")]
    pub cipher: String,
}

#[derive(Args)]
pub struct StoreVerifyArgs {
    /// Store name to verify
    pub store: String,
    /// Digest algorithm used when the blobs were scanned [default: sha256]
    #[arg(long, default_value = "sha256")]
    pub digest_algorithm: DigestAlgorithm,
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: StoreArgs, cfg: &StoreConfig) -> Result<()> {
    match args.command {
        StoreCommands::Add(a) => store_add(db, a).await,
        StoreCommands::Set(a) => store_set(db, a).await,
        StoreCommands::Rm(a) => store_rm(db, a).await,
        StoreCommands::List => store_list(db).await,
        StoreCommands::Push(a) => store_push(db, a, cfg).await,
        StoreCommands::Copy(a) => store_copy(db, a, cfg).await,
        StoreCommands::Verify(a) => store_verify(db, a).await,
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
// store set
// ──────────────────────────────────────────────────────────────────────────────

async fn store_set(db: &DatabaseConnection, args: StoreSetArgs) -> Result<()> {
    let store = ops::find_store_by_name(db, &args.name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("store {:?} not found", args.name))?;

    if args.url.is_none() {
        bail!("nothing to update (specify --url)");
    }

    let updated = ops::update_store(db, store.id, args.url.as_deref(), None).await?;
    println!("store updated: {} (id={}, url={})", updated.name, updated.id, updated.url);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// store rm
// ──────────────────────────────────────────────────────────────────────────────

async fn store_rm(db: &DatabaseConnection, args: StoreRmArgs) -> Result<()> {
    let store = ops::find_store_by_name(db, &args.name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("store {:?} not found", args.name))?;

    let replica_count = ops::count_replicas_in_store(db, store.id).await?;
    if replica_count > 0 && !args.force {
        bail!("store {:?} has {} replica(s); use --force to remove anyway", args.name, replica_count);
    }

    if replica_count > 0 {
        let replicas = ops::replicas_in_store(db, store.id).await?;
        let ids: Vec<i64> = replicas.iter().map(|r| r.id).collect();
        let deleted = ops::delete_replica_records(db, &ids).await?;
        println!("deleted {} replica record(s)", deleted);
    }

    ops::delete_store(db, store.id).await?;
    println!("store removed: {} (id={})", store.name, store.id);
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
    println!("{:<20} {:<8} url", "name", "id");
    println!("{}", "-".repeat(60));
    for s in stores {
        println!("{:<20} {:<8} {}", s.name, s.id, s.url);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// store push
// ──────────────────────────────────────────────────────────────────────────────

async fn store_push(db: &DatabaseConnection, args: StorePushArgs, cfg: &StoreConfig) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;

    let store_name = args
        .store
        .or_else(|| cfg.default_store.clone())
        .ok_or_else(|| anyhow::anyhow!("store name required (pass <store> or set store.default_store in tome.toml)"))?;

    let store = resolve_store(db, &store_name).await?;

    // Determine scan root: CLI arg > snapshot metadata > error
    let scan_root = super::helpers::resolve_scan_root(db, repo.id, args.path).await?;

    let storage = factory::open_storage(&store.url).await?;
    let entries = ops::present_cache_entries(db, repo.id).await?;

    if entries.is_empty() {
        println!("no present files in repository {:?}", args.repo);
        return Ok(());
    }

    println!("pushing {} file(s) to store {:?} ...", entries.len(), store_name);
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

        let digest_hex =
            cache.digest.as_ref().map(|d| hash::hex_encode(d)).unwrap_or_else(|| format!("blob-{}", blob_id));

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

async fn store_copy(db: &DatabaseConnection, args: StoreCopyArgs, cfg: &StoreConfig) -> Result<()> {
    let src_store = resolve_store(db, &args.src).await?;
    let dst_store = resolve_store(db, &args.dst).await?;

    // Resolve encryption key if needed.
    // Priority: --key-file CLI arg > store.key_file in tome.toml.
    let key: Option<[u8; 32]> = if args.encrypt {
        let key_path =
            args.key_file.or_else(|| cfg.key_file.as_ref().map(|p| config::expand_tilde(p))).ok_or_else(|| {
                let default_dir = factory::key_dir();
                anyhow::anyhow!(
                    "--key-file is required when --encrypt is set \
                     (or set store.key_file in tome.toml; default key dir: {:?})",
                    default_dir
                )
            })?;
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
            let cipher_algo: CipherAlgorithm = args.cipher.parse().map_err(|e: String| anyhow::anyhow!(e))?;
            let enc = EncryptedStorage::with_algorithm(dst_inner, key, cipher_algo);
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

// ──────────────────────────────────────────────────────────────────────────────
// store verify
// ──────────────────────────────────────────────────────────────────────────────

async fn store_verify(db: &DatabaseConnection, args: StoreVerifyArgs) -> Result<()> {
    let store = resolve_store(db, &args.store).await?;

    let storage = factory::open_storage(&store.url).await?;
    let replicas = ops::replicas_with_blobs_in_store(db, store.id).await?;

    if replicas.is_empty() {
        println!("no replicas in store {:?}", args.store);
        return Ok(());
    }

    println!("verifying {} replica(s) in store {:?} ...", replicas.len(), args.store);
    let mut ok = 0u64;
    let mut failed = 0u64;
    let mut skipped = 0u64;

    let tmp_dir = tempfile::tempdir()?;
    let now = chrono::Utc::now().fixed_offset();

    for (replica, blob) in &replicas {
        let digest_hex = hash::hex_encode(&blob.digest);

        if replica.encrypted {
            warn!("skipping encrypted replica: {}", digest_hex);
            skipped += 1;
            continue;
        }

        let tmp_file = tmp_dir.path().join(&digest_hex);

        match storage.download(&replica.path, &tmp_file).await {
            Ok(()) => {}
            Err(e) => {
                warn!("download failed for {}: {}", digest_hex, e);
                failed += 1;
                continue;
            }
        }

        let file_hash = match hash::hash_file(&tmp_file, args.digest_algorithm) {
            Ok(h) => h,
            Err(e) => {
                warn!("hash failed for {}: {}", digest_hex, e);
                failed += 1;
                continue;
            }
        };

        if file_hash.digest.as_slice() == blob.digest.as_slice() && file_hash.size as i64 == blob.size {
            ops::update_replica_verified_at(db, replica.id, now).await?;
            info!("ok: {}", digest_hex);
            ok += 1;
        } else {
            warn!(
                "digest mismatch for blob {}: stored={}, actual={}",
                blob.id,
                digest_hex,
                hash::hex_encode(&file_hash.digest)
            );
            failed += 1;
        }
    }

    println!("done: {} ok, {} failed, {} skipped (encrypted)", ok, failed, skipped);
    if failed > 0 {
        bail!("{} replica(s) failed verification", failed);
    }
    Ok(())
}
