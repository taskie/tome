use anyhow::{Result, bail};
use clap::Args;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;

use tome_db::{entities::object, ops};

use crate::snapshot_ref::{self, SnapshotRef};

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct DiffArgs {
    /// Snapshot 1 reference (ID, @latest, @latest~N, @YYYY-MM-DD)
    pub snapshot1: String,
    /// Snapshot 2 reference (ID, @latest, @latest~N, @YYYY-MM-DD)
    pub snapshot2: String,
    /// Repository name (required for @-references) [default: "default"]
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,
    /// Path prefix filter (limit diff to files under this prefix)
    #[arg(long, default_value = "")]
    pub prefix: String,
    /// Print only the names of changed files
    #[arg(long)]
    pub name_only: bool,
    /// Print summary with file sizes
    #[arg(long)]
    pub stat: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum DiffStatus {
    Added,
    Deleted,
    Modified,
}

struct DiffRow {
    status: DiffStatus,
    path: String,
    size_before: Option<i64>,
    size_after: Option<i64>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: DiffArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let ref1: SnapshotRef = args.snapshot1.parse()?;
    let ref2: SnapshotRef = args.snapshot2.parse()?;
    let snap1_id = snapshot_ref::resolve(db, repo.id, &ref1).await?;
    let snap2_id = snapshot_ref::resolve(db, repo.id, &ref2).await?;

    let entries1 = ops::entries_with_digest(db, snap1_id, &args.prefix).await?;
    let entries2 = ops::entries_with_digest(db, snap2_id, &args.prefix).await?;

    if entries1.is_empty() && entries2.is_empty() {
        bail!("no entries found — check that the snapshot IDs are correct");
    }

    // Build maps: path → (object_id, object_model) for present entries only.
    let map1: HashMap<String, (Option<i64>, Option<object::Model>)> =
        entries1.into_iter().filter(|(e, _)| e.status == 1).map(|(e, b)| (e.path, (e.object_id, b))).collect();

    let map2: HashMap<String, (Option<i64>, Option<object::Model>)> =
        entries2.into_iter().filter(|(e, _)| e.status == 1).map(|(e, b)| (e.path, (e.object_id, b))).collect();

    // Compute diff rows.
    let mut rows: Vec<DiffRow> = Vec::new();

    for (path, (object_id1, blob1)) in &map1 {
        if let Some((object_id2, blob2)) = map2.get(path) {
            if object_id1 == object_id2 {
                // unchanged — omit
            } else {
                rows.push(DiffRow {
                    status: DiffStatus::Modified,
                    path: path.clone(),
                    size_before: blob1.as_ref().and_then(|b| b.size),
                    size_after: blob2.as_ref().and_then(|b| b.size),
                });
            }
        } else {
            rows.push(DiffRow {
                status: DiffStatus::Deleted,
                path: path.clone(),
                size_before: blob1.as_ref().and_then(|b| b.size),
                size_after: None,
            });
        }
    }

    for (path, (_, blob2)) in &map2 {
        if !map1.contains_key(path) {
            rows.push(DiffRow {
                status: DiffStatus::Added,
                path: path.clone(),
                size_before: None,
                size_after: blob2.as_ref().and_then(|b| b.size),
            });
        }
    }

    rows.sort_by(|a, b| a.path.cmp(&b.path));

    if rows.is_empty() {
        println!("no differences");
        return Ok(());
    }

    // Output.
    if args.name_only {
        for row in &rows {
            println!("{}", row.path);
        }
    } else if args.stat {
        for row in &rows {
            let (label, size_info) = match row.status {
                DiffStatus::Added => {
                    let s = row.size_after.map(|n| format!("({} bytes)", fmt_size(n))).unwrap_or_default();
                    ("A", s)
                }
                DiffStatus::Deleted => {
                    let s = row.size_before.map(|n| format!("({} bytes)", fmt_size(n))).unwrap_or_default();
                    ("D", s)
                }
                DiffStatus::Modified => {
                    let s = match (row.size_before, row.size_after) {
                        (Some(a), Some(b)) => format!("({} → {} bytes)", fmt_size(a), fmt_size(b)),
                        _ => String::new(),
                    };
                    ("M", s)
                }
            };
            println!("{label}  {:<60}  {size_info}", row.path);
        }
        println!("---");
        let added = rows.iter().filter(|r| matches!(r.status, DiffStatus::Added)).count();
        let deleted = rows.iter().filter(|r| matches!(r.status, DiffStatus::Deleted)).count();
        let modified = rows.iter().filter(|r| matches!(r.status, DiffStatus::Modified)).count();
        println!("{} added, {} deleted, {} modified", added, deleted, modified);
    } else {
        for row in &rows {
            let label = match row.status {
                DiffStatus::Added => "A",
                DiffStatus::Deleted => "D",
                DiffStatus::Modified => "M",
            };
            println!("{label}  {}", row.path);
        }
    }

    Ok(())
}

fn fmt_size(n: i64) -> String {
    let n = n as u64;
    if n >= 1_000_000_000 {
        format!("{:.1}G", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}B")
    }
}
