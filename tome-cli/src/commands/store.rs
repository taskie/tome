use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use tome_core::hash::{self, DigestAlgorithm};
use tome_db::ops;
use tome_store::{CipherAlgorithm, Storage, encrypted::EncryptedStorage, factory, key_source, storage::blob_path};

use crate::config::{self, StoreConfig};

use super::helpers::resolve_store;

// ──────────────────────────────────────────────────────────────────────────────
// Per-store encryption config (stored in store.config JSON)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PerStoreEncryptConfig {
    #[serde(default)]
    encrypt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cipher: Option<String>,
}

fn parse_store_config(config: &serde_json::Value) -> PerStoreEncryptConfig {
    serde_json::from_value(config.clone()).unwrap_or_default()
}

fn build_encrypt_config(
    encrypt: bool,
    key_source: Option<String>,
    key_file: Option<String>,
    cipher: Option<String>,
) -> PerStoreEncryptConfig {
    PerStoreEncryptConfig { encrypt, key_source, key_file, cipher }
}

/// Resolve encryption key and cipher for a store operation.
///
/// Priority: CLI key_file > CLI key_source > per-store key_file > per-store key_source
///           > global store.key_file > global store.key_source > error.
async fn resolve_encryption_for_store(
    cli_encrypt: Option<bool>,
    cli_key_file: Option<&std::path::Path>,
    cli_key_source: Option<&str>,
    cli_cipher: Option<&str>,
    per_store: &PerStoreEncryptConfig,
    global_cfg: &StoreConfig,
) -> Result<Option<([u8; 32], CipherAlgorithm)>> {
    let should_encrypt = cli_encrypt.unwrap_or(per_store.encrypt);
    if !should_encrypt {
        return Ok(None);
    }

    // Resolve key
    let key: [u8; 32] = if let Some(path) = cli_key_file {
        factory::read_key_file(path)?
    } else if let Some(source) = cli_key_source {
        key_source::resolve(source).await.map_err(|e| anyhow::anyhow!("{}", e))?
    } else if let Some(ref path_str) = per_store.key_file {
        let path = config::expand_tilde(std::path::Path::new(path_str));
        factory::read_key_file(&path)?
    } else if let Some(ref source) = per_store.key_source {
        key_source::resolve(source).await.map_err(|e| anyhow::anyhow!("{}", e))?
    } else if let Some(ref path) = global_cfg.key_file {
        factory::read_key_file(&config::expand_tilde(path))?
    } else if let Some(ref source) = global_cfg.key_source {
        key_source::resolve(source).await.map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        let default_dir = factory::key_dir();
        bail!(
            "--key-file or --key-source is required when encryption is enabled \
             (or set key_file/key_source in store config or store.key_file/store.key_source in tome.toml; \
              default key dir: {:?})",
            default_dir
        );
    };

    // Resolve cipher
    let cipher_str = cli_cipher.or(per_store.cipher.as_deref()).unwrap_or("xchacha20-poly1305");
    let cipher_algo: CipherAlgorithm = cipher_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;

    Ok(Some((key, cipher_algo)))
}

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
    /// Enable encryption for this store
    #[arg(long)]
    pub encrypt: bool,
    /// Path to 32-byte binary key file for encryption
    #[arg(long)]
    pub key_file: Option<String>,
    /// External secret manager URI for the encryption key
    #[arg(long)]
    pub key_source: Option<String>,
    /// Cipher algorithm: xchacha20-poly1305, chacha20-poly1305, or aes256gcm
    #[arg(long)]
    pub cipher: Option<String>,
}

#[derive(Args)]
pub struct StoreSetArgs {
    /// Store name
    pub name: String,
    /// New URL for the store
    #[arg(long)]
    pub url: Option<String>,
    /// Enable encryption for this store
    #[arg(long)]
    pub encrypt: bool,
    /// Disable encryption for this store (clears all encryption fields)
    #[arg(long, conflicts_with = "encrypt")]
    pub no_encrypt: bool,
    /// Path to 32-byte binary key file for encryption
    #[arg(long)]
    pub key_file: Option<String>,
    /// External secret manager URI for the encryption key
    #[arg(long)]
    pub key_source: Option<String>,
    /// Cipher algorithm: xchacha20-poly1305, chacha20-poly1305, or aes256gcm
    #[arg(long)]
    pub cipher: Option<String>,
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
    #[arg(long, short = 'r', env = "TOME_REPO", default_value = "default")]
    pub repo: String,
    /// Store name to push to [config: store.default_store]
    pub store: Option<String>,
    /// Root directory where scanned files reside (overrides snapshot metadata)
    pub path: Option<std::path::PathBuf>,
    /// Encrypt blobs (overrides per-store config)
    #[arg(long)]
    pub encrypt: bool,
    /// Path to 32-byte binary key file for encryption
    #[arg(long)]
    pub key_file: Option<std::path::PathBuf>,
    /// External secret manager URI for the encryption key
    #[arg(long)]
    pub key_source: Option<String>,
    /// Cipher algorithm: xchacha20-poly1305 (default), chacha20-poly1305, or aes256gcm
    #[arg(long)]
    pub cipher: Option<String>,
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
    /// Path to 32-byte binary key file (required when --encrypt is set, unless --key-source is used)
    #[arg(long)]
    pub key_file: Option<std::path::PathBuf>,
    /// External secret manager URI for the encryption key (e.g. env://VAR, aws-secrets-manager://id, vault://path)
    #[arg(long)]
    pub key_source: Option<String>,
    /// Cipher algorithm for encryption: aes256gcm (default) or chacha20-poly1305
    #[arg(long)]
    pub cipher: Option<String>,
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
    let enc_cfg = build_encrypt_config(args.encrypt, args.key_source, args.key_file, args.cipher);
    let config = serde_json::to_value(&enc_cfg)?;
    let store = ops::get_or_create_store(db, &args.name, &args.url, config).await?;
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

    let has_encrypt_change = args.encrypt
        || args.no_encrypt
        || args.key_file.is_some()
        || args.key_source.is_some()
        || args.cipher.is_some();

    if args.url.is_none() && !has_encrypt_change {
        bail!("nothing to update (specify --url or encryption flags)");
    }

    let new_config = if has_encrypt_change {
        let mut cfg = parse_store_config(&store.config);
        if args.no_encrypt {
            cfg = PerStoreEncryptConfig::default();
        } else {
            if args.encrypt {
                cfg.encrypt = true;
            }
            if args.key_file.is_some() {
                cfg.key_file = args.key_file;
            }
            if args.key_source.is_some() {
                cfg.key_source = args.key_source;
            }
            if args.cipher.is_some() {
                cfg.cipher = args.cipher;
            }
        }
        Some(serde_json::to_value(&cfg)?)
    } else {
        None
    };

    let updated = ops::update_store(db, store.id, args.url.as_deref(), new_config).await?;
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
    println!("{:<20} {:<8} {:<10} url", "name", "id", "encrypt");
    println!("{}", "-".repeat(70));
    for s in stores {
        let enc = parse_store_config(&s.config);
        let enc_label = if enc.encrypt { "yes" } else { "-" };
        println!("{:<20} {:<8} {:<10} {}", s.name, s.id, enc_label, s.url);
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

    // Resolve encryption: CLI flags override per-store config.
    let per_store = parse_store_config(&store.config);
    let cli_encrypt = if args.encrypt { Some(true) } else { None };
    let encryption = resolve_encryption_for_store(
        cli_encrypt,
        args.key_file.as_deref(),
        args.key_source.as_deref(),
        args.cipher.as_deref(),
        &per_store,
        cfg,
    )
    .await?;
    let is_encrypted = encryption.is_some();

    let storage: Box<dyn Storage> = if let Some((key, cipher_algo)) = encryption {
        let inner = factory::open_storage(&store.url).await?;
        Box::new(EncryptedStorage::with_algorithm(inner, key, cipher_algo))
    } else {
        factory::open_storage(&store.url).await?
    };

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
        let object_id = match cache.object_id {
            Some(id) => id,
            None => continue,
        };

        // Skip if replica already exists.
        if ops::replica_exists(db, object_id, store.id).await? {
            skipped += 1;
            continue;
        }

        let digest_hex =
            cache.digest.as_ref().map(|d| hash::hex_encode(d)).unwrap_or_else(|| format!("object-{}", object_id));

        let local_file = scan_root.join(&cache.path);
        if !local_file.exists() {
            info!("file not found, skipping: {:?}", local_file);
            errors += 1;
            continue;
        }

        let remote_path = blob_path(&digest_hex);
        match storage.upload(&remote_path, &local_file).await {
            Ok(()) => {
                ops::insert_replica(db, object_id, store.id, &remote_path, is_encrypted).await?;
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

    // Resolve encryption: CLI flags override per-store config on the destination store.
    let dst_per_store = parse_store_config(&dst_store.config);
    let cli_encrypt = if args.encrypt { Some(true) } else { None };
    let encryption = resolve_encryption_for_store(
        cli_encrypt,
        args.key_file.as_deref(),
        args.key_source.as_deref(),
        args.cipher.as_deref(),
        &dst_per_store,
        cfg,
    )
    .await?;
    let is_encrypted = encryption.is_some();

    // Open source storage.
    let src_storage = factory::open_storage(&src_store.url).await?;

    // Find blobs missing in dst.
    let blobs = ops::objects_missing_in_dst(db, src_store.id, dst_store.id).await?;
    if blobs.is_empty() {
        println!("nothing to copy: all blobs already present in {:?}", args.dst);
        return Ok(());
    }
    println!("copying {} blob(s) from {:?} to {:?} ...", blobs.len(), args.src, args.dst);

    // Fetch all source replicas once and index by object_id (avoids N+1 queries).
    let src_replica_map: std::collections::HashMap<i64, _> =
        ops::replicas_in_store(db, src_store.id).await?.into_iter().map(|r| (r.object_id, r)).collect();

    // Open destination storage once (upload takes &self, so it can be reused).
    let dst_storage: Box<dyn Storage> = if let Some((key, cipher_algo)) = encryption {
        let dst_inner = factory::open_storage(&dst_store.url).await?;
        Box::new(EncryptedStorage::with_algorithm(dst_inner, key, cipher_algo))
    } else {
        factory::open_storage(&dst_store.url).await?
    };

    let mut copied = 0u64;
    let mut errors = 0u64;

    // Use a temp dir for intermediate files.
    let tmp_dir = tempfile::tempdir()?;

    for blob in &blobs {
        let digest_hex = hash::hex_encode(&blob.digest);

        // Find source replica path.
        let src_replica = match src_replica_map.get(&blob.id) {
            Some(r) => r,
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

        // Upload to dst.
        let dst_path = blob_path(&digest_hex);
        match dst_storage.upload(&dst_path, &tmp_file).await {
            Ok(()) => {}
            Err(e) => {
                warn!("failed to upload blob {}: {}", digest_hex, e);
                errors += 1;
                continue;
            }
        }

        // Record replica in DB.
        ops::insert_replica(db, blob.id, dst_store.id, &dst_path, is_encrypted).await?;
        info!("copied: {}", digest_hex);
        copied += 1;
    }

    println!("done: {} copied, {} errors", copied, errors);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// store verify
// ──────────────────────────────────────────────────────────────────────────────

pub(crate) async fn store_verify(db: &DatabaseConnection, args: StoreVerifyArgs) -> Result<()> {
    let store = resolve_store(db, &args.store).await?;

    let storage = factory::open_storage(&store.url).await?;
    let replicas = ops::replicas_with_objects_in_store(db, store.id).await?;

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

        let file_hash = match hash::hash_file(&tmp_file, args.digest_algorithm, hash::FastHashAlgorithm::default()) {
            Ok(h) => h,
            Err(e) => {
                warn!("hash failed for {}: {}", digest_hex, e);
                failed += 1;
                continue;
            }
        };

        if file_hash.digest.as_slice() == blob.digest.as_slice() && Some(file_hash.size as i64) == blob.size {
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
