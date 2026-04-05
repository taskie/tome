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
pub struct FilesArgs {
    /// Repository name [default: "default"]
    #[arg(long, short = 'r', env = "TOME_REPO", default_value = "default")]
    pub repo: String,
    /// Filter by path prefix
    #[arg(long, default_value = "")]
    pub prefix: String,
    /// Include deleted files (default: only present files)
    #[arg(long)]
    pub include_deleted: bool,
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: FilesArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;

    if args.include_deleted {
        let entries = ops::all_cache_entries(db, repo.id).await?;
        let filtered: Vec<_> = if args.prefix.is_empty() {
            entries
        } else {
            entries.into_iter().filter(|e| e.path.starts_with(&args.prefix)).collect()
        };
        print_cache_entries(&filtered, &args)?;
    } else {
        let entries = ops::present_cache_entries(db, repo.id).await?;
        let filtered: Vec<_> = if args.prefix.is_empty() {
            entries
        } else {
            entries.into_iter().filter(|e| e.path.starts_with(&args.prefix)).collect()
        };
        print_cache_entries(&filtered, &args)?;
    }

    Ok(())
}

fn print_cache_entries(entries: &[tome_db::entities::entry_cache::Model], args: &FilesArgs) -> Result<()> {
    if entries.is_empty() {
        println!("no files in repository {:?}", args.repo);
        return Ok(());
    }

    match args.format {
        OutputFormat::Text => {
            for e in entries {
                let status = if e.status == 1 { " " } else { "D" };
                let digest_str =
                    e.digest.as_ref().map(|d| hash::hex_encode(d)[..12].to_owned()).unwrap_or_else(|| "-".repeat(12));
                let size_str = e.size.map(|s| format!("{:>10}", s)).unwrap_or_else(|| " ".repeat(10));
                println!("{} {} {} {}", status, digest_str, size_str, e.path);
            }
            println!("---");
            println!("{} file(s)", entries.len());
        }
        OutputFormat::Json => {
            let items: Vec<serde_json::Value> = entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "path": e.path,
                        "status": if e.status == 1 { "present" } else { "deleted" },
                        "digest": e.digest.as_ref().map(|d| hash::hex_encode(d)),
                        "size": e.size,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
    }

    Ok(())
}
