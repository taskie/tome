use chrono::{DateTime, Utc};
use ichnome::{Footprint, Group, History, Stat, Workspace};
use serde::Serialize;
use treblo::hex::to_hex_string;

fn fast_digest_string(d: i64) -> String {
    format!("{:016x}", d)
}

#[derive(Serialize)]
pub struct WebFootprint {
    pub id: String,

    pub digest: String,
    pub size: String,
    pub fast_digest: String,

    pub created_at: DateTime<Utc>,
}

impl WebFootprint {
    pub(crate) fn from(f: &Footprint) -> Self {
        WebFootprint {
            id: f.id.to_string(),
            digest: to_hex_string(&f.digest),
            size: format!("{}", f.size),
            fast_digest: fast_digest_string(f.fast_digest),
            created_at: f.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct WebWorkspace {
    pub id: String,

    pub name: String,

    pub description: String,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WebWorkspace {
    pub(crate) fn from(w: &Workspace) -> Self {
        WebWorkspace {
            id: w.id.to_string(),
            name: w.name.clone(),
            description: w.description.clone(),
            created_at: w.created_at,
            updated_at: w.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct WebGroup {
    pub id: String,

    pub workspace_name: String,
    pub name: String,

    pub url: String,
    #[serde(rename = "type")]
    pub type_: i32,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WebGroup {
    pub(crate) fn from(w: &Workspace, g: &Group) -> Self {
        WebGroup {
            id: g.id.to_string(),
            workspace_name: w.name.clone(),
            name: g.name.clone(),
            url: g.url.clone(),
            type_: g.type_,
            created_at: g.created_at,
            updated_at: g.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct WebStat {
    pub id: String,

    pub workspace_name: String,
    pub group_name: String,
    pub path: String,

    pub version: i32,
    pub status: i32,
    pub mtime: Option<DateTime<Utc>>,

    pub digest: Option<String>,
    pub size: Option<String>,
    pub fast_digest: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WebStat {
    pub(crate) fn from(w: &Workspace, g: &Group, s: &Stat) -> Self {
        WebStat {
            id: s.id.to_string(),

            workspace_name: w.name.clone(),
            group_name: g.name.clone(),
            path: s.path.clone(),

            version: s.version,
            status: s.status,
            mtime: s.mtime,

            digest: s.digest.as_ref().map(|d| to_hex_string(d)),
            size: s.size.map(|i| format!("{}", i)),
            fast_digest: s.fast_digest.map(|i| format!("{:016x}", i)),

            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct WebHistory {
    pub id: String,

    pub workspace_name: String,
    pub group_name: String,
    pub path: String,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<DateTime<Utc>>,
    pub digest: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WebHistory {
    pub(crate) fn from(w: &Workspace, g: &Group, h: &History) -> Self {
        WebHistory {
            id: h.id.to_string(),

            workspace_name: w.name.clone(),
            group_name: g.name.clone(),
            path: h.path.clone(),
            version: h.version,

            status: h.status,
            mtime: h.mtime,
            digest: h.digest.as_ref().map(|d| to_hex_string(d)),

            created_at: h.created_at,
            updated_at: h.updated_at,
        }
    }
}
