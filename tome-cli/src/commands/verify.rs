use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;
use tracing::warn;

use tome_core::hash;
use tome_db::ops;

use super::store::{StoreVerifyArgs, store_verify};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct VerifyArgs {
    /// Repository name (default: "default")
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Root directory of scanned files (overrides scan_root from snapshot metadata)
    pub path: Option<std::path::PathBuf>,
    /// Also print a line for each OK file (default: only print problems)
    #[arg(long, short = 'v')]
    pub verbose: bool,
    /// Verify replicas in the named store instead of local files
    #[arg(long)]
    pub store: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: VerifyArgs) -> Result<()> {
    // If --store is given, delegate to store verify.
    if let Some(store_name) = args.store {
        let algo = ops::get_or_create_repository(db, &args.repo)
            .await
            .ok()
            .as_ref()
            .and_then(|r| ops::get_repository_digest_algorithm(r).ok())
            .unwrap_or(hash::DigestAlgorithm::Sha256);
        return store_verify(db, StoreVerifyArgs { store: store_name, digest_algorithm: algo }).await;
    }

    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let algo = ops::get_repository_digest_algorithm(&repo)?;

    let scan_root = super::helpers::resolve_scan_root(db, repo.id, args.path).await?;

    let entries = ops::present_cache_entries(db, repo.id).await?;

    if entries.is_empty() {
        println!("no present files in repository {:?}", args.repo);
        return Ok(());
    }

    println!("verifying {} file(s) in {:?} ({:?}) ...", entries.len(), args.repo, scan_root);

    let mut ok = 0u64;
    let mut modified = 0u64;
    let mut missing = 0u64;
    let mut errors = 0u64;

    for entry in &entries {
        let abs_path = scan_root.join(&entry.path);

        if !abs_path.exists() {
            println!("MISSING    {}", entry.path);
            warn!("missing: {:?}", abs_path);
            missing += 1;
            continue;
        }

        let file_hash = match hash::hash_file(&abs_path, algo, hash::FastHashAlgorithm::default()) {
            Ok(h) => h,
            Err(e) => {
                println!("ERROR      {}  ({})", entry.path, e);
                warn!("hash error for {:?}: {}", abs_path, e);
                errors += 1;
                continue;
            }
        };

        let cached_digest = entry.digest.as_deref().unwrap_or(&[]);
        if file_hash.digest.as_slice() == cached_digest {
            ok += 1;
            if args.verbose {
                println!("OK         {}", entry.path);
            }
        } else {
            let actual_hex = hash::hex_encode(&file_hash.digest);
            let cached_hex = hash::hex_encode(cached_digest);
            println!("MODIFIED   {}  (cached: {}, actual: {})", entry.path, &cached_hex[..12], &actual_hex[..12],);
            modified += 1;
        }
    }

    println!("---");
    println!("{} ok, {} modified, {} missing, {} errors", ok, modified, missing, errors);

    if modified > 0 || missing > 0 || errors > 0 {
        anyhow::bail!("{} modified, {} missing, {} errors", modified, missing, errors);
    }

    Ok(())
}
