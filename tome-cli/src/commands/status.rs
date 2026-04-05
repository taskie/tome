use std::os::unix::fs::MetadataExt as _;

use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;

use tome_core::hash;
use tome_db::ops;

use crate::output::OutputFormat;

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct StatusArgs {
    /// Repository name [default: "default"]
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Root directory (overrides scan_root from snapshot metadata)
    pub path: Option<std::path::PathBuf>,
    /// Compute full content hash (slow but detects content changes, not just mtime/size)
    #[arg(long)]
    pub hash: bool,
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: StatusArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let algo = ops::get_repository_digest_algorithm(&repo)?;

    let scan_root = super::helpers::resolve_scan_root(db, repo.id, args.path).await?;

    let cached = ops::present_cache_entries(db, repo.id).await?;

    // Build a set of cached paths for fast lookup.
    let mut cached_map: std::collections::HashMap<String, &tome_db::entities::entry_cache::Model> =
        cached.iter().map(|e| (e.path.clone(), e)).collect();

    // Walk the filesystem using the same walker as scan.
    let walker = ignore::WalkBuilder::new(&scan_root).hidden(false).git_ignore(true).build();

    let mut added: Vec<String> = Vec::new();
    let mut modified: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();

    for result in walker {
        let dir_entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !dir_entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let abs_path = dir_entry.path();
        let rel_path = match abs_path.strip_prefix(&scan_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if let Some(entry) = cached_map.remove(&rel_path) {
            // Check if modified.
            if args.hash {
                let fh = hash::hash_file(abs_path, algo, hash::FastHashAlgorithm::default())?;
                let cached_digest = entry.digest.as_deref().unwrap_or(&[]);
                if fh.digest.as_slice() != cached_digest {
                    modified.push(rel_path);
                }
            } else {
                // Fast check: mtime + size.
                let meta = std::fs::metadata(abs_path)?;
                let file_size = meta.len() as i64;
                let cached_size = entry.size.unwrap_or(-1);
                if file_size != cached_size {
                    modified.push(rel_path);
                } else if let Some(cached_mtime) = entry.mtime {
                    let mtime_secs = meta.mtime();
                    let mtime_nanos = meta.mtime_nsec() as u32;
                    if cached_mtime.timestamp() != mtime_secs || cached_mtime.timestamp_subsec_nanos() != mtime_nanos {
                        modified.push(rel_path);
                    }
                }
            }
        } else {
            added.push(rel_path);
        }
    }

    // Remaining entries in cached_map are deleted.
    deleted.extend(cached_map.into_keys());
    added.sort();
    modified.sort();
    deleted.sort();

    let total = added.len() + modified.len() + deleted.len();

    match args.format {
        OutputFormat::Text => {
            if total == 0 {
                println!("clean (no changes since last scan)");
                return Ok(());
            }
            for p in &added {
                println!("A  {}", p);
            }
            for p in &modified {
                println!("M  {}", p);
            }
            for p in &deleted {
                println!("D  {}", p);
            }
            println!("---");
            println!("{} added, {} modified, {} deleted", added.len(), modified.len(), deleted.len());
        }
        OutputFormat::Json => {
            let out = serde_json::json!({
                "added": added,
                "modified": modified,
                "deleted": deleted,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }

    Ok(())
}
