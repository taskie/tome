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
pub struct HistoryArgs {
    /// File path to show history for
    pub path: String,
    /// Repository name [default: "default"]
    #[arg(long, short = 'r', env = "TOME_REPO", default_value = "default")]
    pub repo: String,
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: HistoryArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let history = ops::path_history(db, repo.id, &args.path).await?;

    if history.is_empty() {
        println!("no history for {:?} in repository {:?}", args.path, args.repo);
        return Ok(());
    }

    match args.format {
        OutputFormat::Text => {
            for (entry, blob, snap) in &history {
                let date = snap.created_at.format("%Y-%m-%d %H:%M:%S");
                let status = if entry.status == 1 { "A/M" } else { "D  " };
                let digest_str = blob
                    .as_ref()
                    .map(|b| hash::hex_encode(&b.digest)[..12].to_owned())
                    .unwrap_or_else(|| "-".repeat(12));
                let size_str =
                    blob.as_ref().and_then(|b| b.size).map(|s| format!("{:>10}", s)).unwrap_or_else(|| " ".repeat(10));
                println!("{} {} {} {} {}", snap.id, date, status, digest_str, size_str);
            }
        }
        OutputFormat::Json => {
            let items: Vec<serde_json::Value> = history
                .iter()
                .map(|(entry, blob, snap)| {
                    serde_json::json!({
                        "snapshot_id": snap.id,
                        "created_at": snap.created_at.to_rfc3339(),
                        "status": if entry.status == 1 { "present" } else { "deleted" },
                        "digest": blob.as_ref().map(|b| hash::hex_encode(&b.digest)),
                        "size": blob.as_ref().and_then(|b| b.size),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
    }

    Ok(())
}
