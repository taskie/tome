use serde::{Deserialize, Serialize};

/// Metadata recorded on a snapshot created by `tome scan`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanMetadata {
    pub scan_root: String,
    pub scanned: u64,
    pub added: u64,
    pub modified: u64,
    pub unchanged: u64,
    pub deleted: u64,
    pub errors: u64,
}

/// Metadata recorded on a snapshot created by `tome sync pull`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncPullMetadata {
    pub synced_from: String,
    pub remote_snapshot_id: String,
    pub entries: usize,
}

/// Metadata recorded on a snapshot created by `tome sync push` (DB mode).
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncPushMetadata {
    pub pushed_from_machine_id: i16,
    pub source_snapshot_id: i64,
    pub entries: usize,
}
