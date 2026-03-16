use serde::Serialize;
use utoipa::ToSchema;

use tome_core::hash::hex_encode;
use tome_db::entities::{entry, object, repository, snapshot};

/// Error response body returned for 4xx/5xx responses.
#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize, ToSchema)]
pub struct RepositoryResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<repository::Model> for RepositoryResponse {
    fn from(m: repository::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            description: m.description,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct SnapshotResponse {
    pub id: String,
    pub repository_id: String,
    pub parent_id: Option<String>,
    pub message: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub root_object_id: Option<String>,
}

impl From<snapshot::Model> for SnapshotResponse {
    fn from(m: snapshot::Model) -> Self {
        Self {
            id: m.id.to_string(),
            repository_id: m.repository_id.to_string(),
            parent_id: m.parent_id.map(|id| id.to_string()),
            message: m.message,
            metadata: m.metadata,
            created_at: m.created_at.to_rfc3339(),
            root_object_id: m.root_object_id.map(|id| id.to_string()),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct EntryResponse {
    pub id: String,
    pub snapshot_id: String,
    pub path: String,
    pub status: i16,
    pub object_id: Option<String>,
    pub digest: Option<String>,
    pub mode: Option<i32>,
    pub mtime: Option<String>,
    pub created_at: String,
}

impl EntryResponse {
    pub fn from_with_object(e: entry::Model, obj: Option<&object::Model>) -> Self {
        Self {
            id: e.id.to_string(),
            snapshot_id: e.snapshot_id.to_string(),
            path: e.path,
            status: e.status,
            object_id: e.object_id.map(|id| id.to_string()),
            digest: obj.map(|b| hex_encode(&b.digest)),
            mode: e.mode,
            mtime: e.mtime.map(|t| t.to_rfc3339()),
            created_at: e.created_at.to_rfc3339(),
        }
    }
}

impl From<entry::Model> for EntryResponse {
    fn from(m: entry::Model) -> Self {
        Self::from_with_object(m, None)
    }
}

#[derive(Serialize, ToSchema)]
pub struct ObjectResponse {
    pub id: String,
    /// 0 = blob, 1 = tree
    pub object_type: i16,
    pub digest: String,
    pub size: i64,
    pub fast_digest: String,
    pub created_at: String,
}

impl From<object::Model> for ObjectResponse {
    fn from(m: object::Model) -> Self {
        Self {
            id: m.id.to_string(),
            object_type: m.object_type,
            digest: hex_encode(&m.digest),
            size: m.size.unwrap_or(0),
            fast_digest: format!("{:016x}", m.fast_digest.unwrap_or(0) as u64),
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct SnapshotEntry {
    pub snapshot: SnapshotResponse,
    pub entry: EntryResponse,
}

#[derive(Serialize, ToSchema)]
pub struct CacheEntryResponse {
    pub path: String,
    pub status: i16,
    pub size: Option<i64>,
    pub mtime: Option<String>,
    pub digest: Option<String>,
    pub fast_digest: Option<String>,
    pub snapshot_id: String,
    pub entry_id: String,
}

pub fn cache_entry_to_response(e: &tome_db::entities::entry_cache::Model) -> CacheEntryResponse {
    CacheEntryResponse {
        path: e.path.clone(),
        status: e.status,
        size: e.size,
        mtime: e.mtime.map(|t| t.to_rfc3339()),
        digest: e.digest.as_deref().map(hex_encode),
        fast_digest: e.fast_digest.map(|fd| format!("{:016x}", fd as u64)),
        snapshot_id: e.snapshot_id.to_string(),
        entry_id: e.entry_id.to_string(),
    }
}

#[derive(Serialize, ToSchema)]
pub struct MachineResponse {
    pub machine_id: i16,
    pub name: String,
    pub description: String,
    pub last_seen_at: Option<String>,
    pub created_at: String,
}

impl From<tome_db::entities::machine::Model> for MachineResponse {
    fn from(m: tome_db::entities::machine::Model) -> Self {
        Self {
            machine_id: m.machine_id,
            name: m.name,
            description: m.description,
            last_seen_at: m.last_seen_at.map(|t| t.to_rfc3339()),
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct StoreResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<tome_db::entities::store::Model> for StoreResponse {
    fn from(m: tome_db::entities::store::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            url: m.url,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TagResponse {
    pub id: String,
    pub object_id: String,
    pub key: String,
    pub value: Option<String>,
    pub created_at: String,
}

impl From<tome_db::entities::tag::Model> for TagResponse {
    fn from(m: tome_db::entities::tag::Model) -> Self {
        Self {
            id: m.id.to_string(),
            object_id: m.object_id.to_string(),
            key: m.key,
            value: m.value,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct SyncPeerResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub repository_id: String,
    pub last_synced_at: Option<String>,
    pub last_snapshot_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<tome_db::entities::sync_peer::Model> for SyncPeerResponse {
    fn from(m: tome_db::entities::sync_peer::Model) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name,
            url: m.url,
            repository_id: m.repository_id.to_string(),
            last_synced_at: m.last_synced_at.map(|t| t.to_rfc3339()),
            last_snapshot_id: m.last_snapshot_id.map(|id| id.to_string()),
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}
