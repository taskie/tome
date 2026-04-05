use anyhow::Result;
use clap::Args;
use sea_orm::DatabaseConnection;

use tome_core::hash;
use tome_db::ops;

use crate::{
    output::OutputFormat,
    snapshot_ref::{self, SnapshotRef},
};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct ShowArgs {
    /// Snapshot reference (ID, @latest, @latest~N, @YYYY-MM-DD)
    #[arg(default_value = "@latest")]
    pub snapshot: String,
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

pub async fn run(db: &DatabaseConnection, args: ShowArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let snap_ref: SnapshotRef = args.snapshot.parse()?;
    let snap_id = snapshot_ref::resolve(db, repo.id, &snap_ref).await?;
    let snap = ops::find_snapshot_by_id(db, snap_id).await?.expect("resolved snapshot must exist");

    let entries = ops::entries_with_digest(db, snap_id, "").await?;

    match args.format {
        OutputFormat::Text => {
            println!("snapshot {}", snap.id);
            println!("Date:    {}", snap.created_at.format("%Y-%m-%d %H:%M:%S %z"));
            if !snap.message.is_empty() {
                println!("Message: {}", snap.message);
            }
            if let Some(parent) = snap.parent_id {
                println!("Parent:  {}", parent);
            }
            if let Some(added) = snap.metadata.get("added").and_then(|v| v.as_u64()) {
                let modified = snap.metadata.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
                let deleted = snap.metadata.get("deleted").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("Changes: +{} ~{} -{}", added, modified, deleted);
            }
            println!();
            for (entry, blob) in &entries {
                let status = if entry.status == 1 { "A" } else { "D" };
                let digest_str = blob
                    .as_ref()
                    .map(|b| hash::hex_encode(&b.digest)[..12].to_owned())
                    .unwrap_or_else(|| "-".to_string());
                println!("{}  {:<12} {}", status, digest_str, entry.path);
            }
        }
        OutputFormat::Json => {
            let entry_items: Vec<serde_json::Value> = entries
                .iter()
                .map(|(e, b)| {
                    serde_json::json!({
                        "path": e.path,
                        "status": if e.status == 1 { "present" } else { "deleted" },
                        "digest": b.as_ref().map(|b| hash::hex_encode(&b.digest)),
                        "size": b.as_ref().and_then(|b| b.size),
                    })
                })
                .collect();
            let out = serde_json::json!({
                "id": snap.id,
                "created_at": snap.created_at.to_rfc3339(),
                "message": snap.message,
                "parent_id": snap.parent_id,
                "metadata": snap.metadata,
                "entries": entry_items,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }

    Ok(())
}
