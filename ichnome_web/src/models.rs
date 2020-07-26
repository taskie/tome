use chrono::NaiveDateTime;
use ichnome::{Group, History, Stat, Workspace};
use serde::Serialize;

#[derive(Serialize)]
pub struct WebStat {
    pub id: i32,

    pub workspace_name: String,
    pub group_name: String,
    pub path: String,

    pub version: i32,
    pub status: i32,
    pub mtime: Option<NaiveDateTime>,

    pub digest: Option<String>,
    pub size: Option<String>,
    pub fast_digest: Option<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl WebStat {
    pub(crate) fn from(w: &Workspace, g: &Group, s: &Stat) -> Self {
        WebStat {
            id: s.id,

            workspace_name: w.name.clone(),
            group_name: g.name.clone(),
            path: s.path.clone(),

            version: s.version,
            status: s.status,
            mtime: s.mtime,

            digest: s.digest.clone(),
            size: s.size.map(|i| format!("{}", i)),
            fast_digest: s.fast_digest.map(|i| format!("{:016x}", i)),

            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct WebHistory {
    pub id: i32,

    pub workspace_name: String,
    pub group_name: String,
    pub path: String,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<NaiveDateTime>,
    pub digest: Option<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl WebHistory {
    pub(crate) fn from(w: &Workspace, g: &Group, h: &History) -> Self {
        WebHistory {
            id: h.id,

            workspace_name: w.name.clone(),
            group_name: g.name.clone(),
            path: h.path.clone(),
            version: h.version,

            status: h.status,
            mtime: h.mtime,
            digest: h.digest.clone(),

            created_at: h.created_at,
            updated_at: h.updated_at,
        }
    }
}
