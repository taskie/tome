use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;
use tracing::warn;

use tome_core::hash;
use tome_db::ops;

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
    /// Only report files with mismatches (suppress OK lines)
    #[arg(long)]
    pub quiet: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: VerifyArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let algo = ops::get_repository_digest_algorithm(&repo)?;

    // Determine scan root: CLI arg > snapshot metadata > error.
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

        let file_hash = match hash::hash_file(&abs_path, algo) {
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
            if !args.quiet {
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

    if modified > 0 || errors > 0 {
        anyhow::bail!("{} modified, {} errors", modified, errors);
    }

    Ok(())
}
