use chrono::{DateTime, Utc};
use serde::Serialize;
use treblo::hex;

use crate::db::schema::{attrs, contents, footprints, groups, histories, stats, workspaces};

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = footprints)]
pub struct Footprint {
    pub id: i64,

    pub digest: Vec<u8>,
    pub size: i64,
    pub fast_digest: i64,
    pub created_at: DateTime<Utc>,
}

impl Footprint {
    pub fn digest_string(&self) -> String {
        hex::to_hex_string(&self.digest)
    }
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = footprints)]
pub struct FootprintInsertForm<'a> {
    pub id: i64,

    pub digest: &'a [u8],
    pub size: i64,
    pub fast_digest: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = contents)]
pub struct Content {
    pub id: i64,

    pub footprint_id: i64,
    pub body: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = contents)]
pub struct ContentInsertForm<'a> {
    pub id: i64,

    pub footprint_id: i64,
    pub body: &'a [u8],
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = workspaces)]
pub struct Workspace {
    pub id: i64,

    pub name: String,
    pub description: String,
    pub status: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[optional(name = "WorkspaceUpdateForm", derive = "Default, Debug, AsChangeset")]
#[diesel(table_name = workspaces)]
pub struct WorkspaceInsertForm<'a> {
    #[optional(skip = true)]
    pub id: i64,

    pub name: &'a str,
    pub description: &'a str,
    pub status: i32,
    pub created_at: DateTime<Utc>,
    #[optional(skip = true)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = groups)]
pub struct Group {
    pub id: i64,

    pub workspace_id: i64,

    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub type_: i32,
    pub description: String,
    pub status: i32,

    pub group_stat_id: Option<i64>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[optional(name = "GroupUpdateForm", derive = "Default, Debug, AsChangeset")]
#[diesel(table_name = groups)]
pub struct GroupInsertForm<'a> {
    #[optional(skip = true)]
    pub id: i64,

    #[optional(skip = true)]
    pub workspace_id: i64,

    pub name: &'a str,
    pub url: &'a str,
    pub type_: i32,
    pub description: &'a str,
    pub status: i32,

    pub group_stat_id: Option<i64>,

    #[optional(skip = true)]
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = histories)]
pub struct History {
    pub id: i64,

    pub workspace_id: i64,
    pub group_id: i64,
    pub path: String,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<DateTime<Utc>>,
    pub footprint_id: Option<i64>,
    pub digest: Option<Vec<u8>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = histories)]
pub struct HistoryInsertForm<'a> {
    pub id: i64,

    pub workspace_id: i64,
    pub group_id: i64,
    pub path: &'a str,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<DateTime<Utc>>,
    pub footprint_id: Option<i64>,
    pub digest: Option<&'a [u8]>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = stats)]
pub struct Stat {
    pub id: i64,

    pub workspace_id: i64,
    pub group_id: i64,
    pub path: String,

    pub history_id: i64,

    pub version: i32,
    pub status: i32,
    pub mtime: Option<DateTime<Utc>>,
    pub footprint_id: Option<i64>,

    pub digest: Option<Vec<u8>>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[optional(name = "StatUpdateForm", derive = "Default, Debug, AsChangeset")]
#[diesel(table_name = stats)]
pub struct StatInsertForm<'a> {
    #[optional(skip = true)]
    pub id: i64,

    #[optional(skip = true)]
    pub workspace_id: i64,
    #[optional(skip = true)]
    pub group_id: i64,
    #[optional(skip = true)]
    pub path: &'a str,

    pub history_id: i64,

    pub version: i32,
    pub status: i32,
    pub mtime: Option<DateTime<Utc>>,
    pub footprint_id: Option<i64>,

    pub digest: Option<&'a [u8]>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,

    #[optional(skip = true)]
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[diesel(table_name = attrs)]
pub struct Attr {
    pub id: i64,

    pub workspace_id: i64,
    pub target_footprint_id: i64,
    pub target_digest: Vec<u8>,
    pub key: String,
    pub value_type: i32,
    pub value_footprint_id: i64,
    pub value_digest: Vec<u8>,
    pub value_text: Option<String>,
    pub status: i32,
    pub attr_stat_id: Option<i64>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[optional(name = "AttrUpdateForm", derive = "Default, Debug, AsChangeset")]
#[diesel(table_name = attrs)]
pub struct AttrInsertForm<'a> {
    #[optional(skip = true)]
    pub id: i64,

    #[optional(skip = true)]
    pub workspace_id: i64,
    #[optional(skip = true)]
    pub target_footprint_id: i64,
    #[optional(skip = true)]
    pub target_digest: &'a [u8],
    #[optional(skip = true)]
    pub key: &'a str,
    pub value_type: i32,
    pub value_footprint_id: i64,
    pub value_digest: &'a [u8],
    pub value_text: Option<&'a str>,
    pub status: i32,
    pub attr_stat_id: Option<i64>,

    #[optional(skip = true)]
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
