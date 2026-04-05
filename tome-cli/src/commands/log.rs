use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;

use tome_db::ops;

use crate::output::OutputFormat;

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct LogArgs {
    /// Repository name [default: "default"]
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Maximum number of snapshots to display
    #[arg(long, short = 'n')]
    pub limit: Option<usize>,
    /// One-line summary per snapshot (ID + date + message)
    #[arg(long)]
    pub oneline: bool,
    /// Only show snapshots after this date (YYYY-MM-DD)
    #[arg(long)]
    pub after: Option<String>,
    /// Only show snapshots before this date (YYYY-MM-DD)
    #[arg(long)]
    pub before: Option<String>,
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: LogArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let mut snaps = ops::list_snapshots_for_repo(db, repo.id).await?; // newest-first

    // Filter by date range.
    if let Some(ref after) = args.after {
        let dt = parse_date_bound(after, false)?;
        snaps.retain(|s| s.created_at > dt);
    }
    if let Some(ref before) = args.before {
        let dt = parse_date_bound(before, true)?;
        snaps.retain(|s| s.created_at < dt);
    }

    // Apply limit.
    if let Some(limit) = args.limit {
        snaps.truncate(limit);
    }

    if snaps.is_empty() {
        println!("no snapshots in repository {:?}", args.repo);
        return Ok(());
    }

    match args.format {
        OutputFormat::Text => {
            if args.oneline {
                for s in &snaps {
                    let date = s.created_at.format("%Y-%m-%d %H:%M:%S");
                    let msg = if s.message.is_empty() { "" } else { &s.message };
                    println!("{} {} {}", s.id, date, msg);
                }
            } else {
                for (i, s) in snaps.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("snapshot {}", s.id);
                    println!("Date:    {}", s.created_at.format("%Y-%m-%d %H:%M:%S %z"));
                    if !s.message.is_empty() {
                        println!("Message: {}", s.message);
                    }
                    if let Some(parent) = s.parent_id {
                        println!("Parent:  {}", parent);
                    }
                    // Show scan statistics from metadata if present.
                    if let Some(added) = s.metadata.get("added").and_then(|v| v.as_u64()) {
                        let modified = s.metadata.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
                        let deleted = s.metadata.get("deleted").and_then(|v| v.as_u64()).unwrap_or(0);
                        println!("Changes: +{} ~{} -{}", added, modified, deleted);
                    }
                }
            }
        }
        OutputFormat::Json => {
            let items: Vec<serde_json::Value> = snaps
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "created_at": s.created_at.to_rfc3339(),
                        "message": s.message,
                        "parent_id": s.parent_id,
                        "metadata": s.metadata,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
    }

    Ok(())
}

fn parse_date_bound(s: &str, end_of_day: bool) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    let nd = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("invalid date {:?} (expected YYYY-MM-DD)", s))?;
    let time = if end_of_day { nd.and_hms_opt(23, 59, 59).unwrap() } else { nd.and_hms_opt(0, 0, 0).unwrap() };
    let local = chrono::Local::now().fixed_offset().timezone();
    Ok(chrono::TimeZone::from_local_datetime(&local, &time).single().unwrap())
}
