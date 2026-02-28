use anyhow::Result;
use sea_orm::DatabaseConnection;

use tome_db::{entities::store, ops};

/// Look up a store by name or return a user-friendly error.
pub async fn resolve_store(db: &DatabaseConnection, name: &str) -> Result<store::Model> {
    ops::find_store_by_name(db, name).await?.ok_or_else(|| anyhow::anyhow!("store {:?} not found", name))
}

/// Determine scan root from an explicit CLI path or from the latest snapshot metadata.
pub async fn resolve_scan_root(
    db: &DatabaseConnection,
    repo_id: i64,
    explicit: Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.canonicalize()?);
    }
    let meta = ops::latest_snapshot_metadata(db, repo_id).await?;
    let root_str = meta
        .as_ref()
        .and_then(|m| m.get("scan_root"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("could not determine scan_root; pass <path> explicitly"))?
        .to_owned();
    Ok(std::path::PathBuf::from(root_str))
}
