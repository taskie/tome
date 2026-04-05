//! Snapshot reference parser.
//!
//! Supports the following syntax:
//! - `@latest`         — most recent snapshot
//! - `@latest~N`       — N-th ancestor of the latest snapshot
//! - `@YYYY-MM-DD`     — latest snapshot on or before that date (local timezone)
//! - `@YYYY-MM-DDThh:mm` — latest snapshot on or before that datetime
//! - Raw `i64`         — snapshot ID (backward compatible)

use anyhow::{Context, Result, bail};
use chrono::{FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use sea_orm::DatabaseConnection;

use tome_db::ops;

/// A parsed snapshot reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotRef {
    /// Literal snapshot ID.
    Id(i64),
    /// `@latest` or `@latest~N`.
    Latest(usize),
    /// Timestamp-based lookup: find the latest snapshot on or before this time.
    Before(chrono::DateTime<FixedOffset>),
}

impl std::str::FromStr for SnapshotRef {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix('@') {
            if rest == "latest" {
                return Ok(Self::Latest(0));
            }
            if let Some(n_str) = rest.strip_prefix("latest~") {
                let n: usize = n_str.parse().context("invalid offset in @latest~N")?;
                return Ok(Self::Latest(n));
            }
            // Try datetime: YYYY-MM-DDThh:mm
            if let Ok(ndt) = NaiveDateTime::parse_from_str(rest, "%Y-%m-%dT%H:%M") {
                let local = chrono::Local::now().fixed_offset().timezone();
                let dt = local.from_local_datetime(&ndt).single().context("ambiguous local datetime")?;
                return Ok(Self::Before(dt));
            }
            // Try date: YYYY-MM-DD (end of day)
            if let Ok(nd) = NaiveDate::parse_from_str(rest, "%Y-%m-%d") {
                let eod = nd.and_hms_opt(23, 59, 59).expect("valid time");
                let local = chrono::Local::now().fixed_offset().timezone();
                let dt = local.from_local_datetime(&eod).single().context("ambiguous local date")?;
                return Ok(Self::Before(dt));
            }
            bail!("unrecognized snapshot reference: {:?}", s);
        }
        // Raw i64
        let id: i64 = s.parse().context("invalid snapshot reference (expected @latest, @YYYY-MM-DD, or numeric ID)")?;
        Ok(Self::Id(id))
    }
}

/// Resolve a `SnapshotRef` to a concrete snapshot ID.
pub async fn resolve(db: &DatabaseConnection, repo_id: i64, r: &SnapshotRef) -> Result<i64> {
    match r {
        SnapshotRef::Id(id) => {
            // Validate it exists.
            ops::find_snapshot_by_id(db, *id).await?.ok_or_else(|| anyhow::anyhow!("snapshot {} not found", id))?;
            Ok(*id)
        }
        SnapshotRef::Latest(offset) => {
            let snaps = ops::list_snapshots_for_repo(db, repo_id).await?;
            if snaps.is_empty() {
                bail!("no snapshots in repository");
            }
            // snaps are ordered newest-first.
            snaps
                .get(*offset)
                .map(|s| s.id)
                .ok_or_else(|| anyhow::anyhow!("only {} snapshot(s); @latest~{} is out of range", snaps.len(), offset))
        }
        SnapshotRef::Before(dt) => {
            let snaps = ops::list_snapshots_for_repo(db, repo_id).await?;
            // snaps are newest-first; find the first one <= dt.
            snaps
                .iter()
                .find(|s| s.created_at <= *dt)
                .map(|s| s.id)
                .ok_or_else(|| anyhow::anyhow!("no snapshot on or before {}", dt))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_latest() {
        assert_eq!("@latest".parse::<SnapshotRef>().unwrap(), SnapshotRef::Latest(0));
    }

    #[test]
    fn parse_latest_offset() {
        assert_eq!("@latest~3".parse::<SnapshotRef>().unwrap(), SnapshotRef::Latest(3));
    }

    #[test]
    fn parse_raw_id() {
        assert_eq!("12345".parse::<SnapshotRef>().unwrap(), SnapshotRef::Id(12345));
    }

    #[test]
    fn parse_date() {
        let r: SnapshotRef = "@2025-06-15".parse().unwrap();
        matches!(r, SnapshotRef::Before(_));
    }

    #[test]
    fn parse_datetime() {
        let r: SnapshotRef = "@2025-06-15T14:30".parse().unwrap();
        matches!(r, SnapshotRef::Before(_));
    }

    #[test]
    fn parse_invalid() {
        assert!("@bogus".parse::<SnapshotRef>().is_err());
        assert!("hello".parse::<SnapshotRef>().is_err());
    }
}
