use chrono::NaiveDateTime;
use serde::Serialize;

use crate::db::schema::{attrs, contents, footprints, groups, histories, stats, workspaces};

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "footprints"]
pub struct Footprint {
    pub id: i32,

    pub digest: String,
    pub size: i64,
    pub fast_digest: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable)]
#[table_name = "footprints"]
pub struct FootprintInsertForm<'a> {
    pub digest: &'a str,
    pub size: i64,
    pub fast_digest: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "contents"]
pub struct Content {
    pub id: i32,

    pub footprint_id: i32,
    pub body: Vec<u8>,
    pub created_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable)]
#[table_name = "contents"]
pub struct ContentInsertForm<'a> {
    pub footprint_id: i32,
    pub body: &'a [u8],
    pub created_at: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "workspaces"]
pub struct Workspace {
    pub id: i32,

    pub name: String,
    pub description: String,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[table_name = "workspaces"]
#[optional(name = "WorkspaceUpdateForm", derive = "Default, Debug, AsChangeset")]
pub struct WorkspaceInsertForm<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub status: i32,
    pub created_at: NaiveDateTime,
    #[optional(skip = true)]
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "groups"]
pub struct Group {
    pub id: i32,

    pub workspace_id: i32,

    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub type_: i32,
    pub description: String,
    pub status: i32,

    pub group_stat_id: Option<i32>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[table_name = "groups"]
#[optional(name = "GroupUpdateForm", derive = "Default, Debug, AsChangeset")]
pub struct GroupInsertForm<'a> {
    #[optional(skip = true)]
    pub workspace_id: i32,

    pub name: &'a str,
    pub url: &'a str,
    pub type_: i32,
    pub description: &'a str,
    pub status: i32,

    pub group_stat_id: Option<i32>,

    #[optional(skip = true)]
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "histories"]
pub struct History {
    pub id: i32,

    pub workspace_id: i32,
    pub group_id: i32,
    pub path: String,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<NaiveDateTime>,
    pub footprint_id: Option<i32>,
    pub digest: Option<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable)]
#[table_name = "histories"]
pub struct HistoryInsertForm<'a> {
    pub workspace_id: i32,
    pub group_id: i32,
    pub path: &'a str,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<NaiveDateTime>,
    pub footprint_id: Option<i32>,
    pub digest: Option<&'a str>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "stats"]
pub struct Stat {
    pub id: i32,

    pub workspace_id: i32,
    pub group_id: i32,
    pub path: String,

    pub history_id: i32,

    pub version: i32,
    pub status: i32,
    pub mtime: Option<NaiveDateTime>,
    pub footprint_id: Option<i32>,

    pub digest: Option<String>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[table_name = "stats"]
#[optional(name = "StatUpdateForm", derive = "Default, Debug, AsChangeset")]
pub struct StatInsertForm<'a> {
    #[optional(skip = true)]
    pub workspace_id: i32,
    #[optional(skip = true)]
    pub group_id: i32,
    #[optional(skip = true)]
    pub path: &'a str,

    pub history_id: i32,

    pub version: i32,
    pub status: i32,
    pub mtime: Option<NaiveDateTime>,
    pub footprint_id: Option<i32>,

    pub digest: Option<&'a str>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,

    #[optional(skip = true)]
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "attrs"]
pub struct Attr {
    pub id: i32,

    pub workspace_id: i32,
    pub target_footprint_id: i32,
    pub target_digest: String,
    pub key: String,
    pub value_footprint_id: i32,
    pub value_digest: String,
    pub value_content_type: i32,
    pub value_summary: Option<String>,
    pub status: i32,
    pub attr_stat_id: Option<i32>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable, Optional)]
#[table_name = "attrs"]
#[optional(name = "AttrUpdateForm", derive = "Default, Debug, AsChangeset")]
pub struct AttrInsertForm<'a> {
    #[optional(skip = true)]
    pub workspace_id: i32,
    #[optional(skip = true)]
    pub target_footprint_id: i32,
    #[optional(skip = true)]
    pub target_digest: &'a str,
    #[optional(skip = true)]
    pub key: &'a str,
    pub value_footprint_id: i32,
    pub value_digest: &'a str,
    pub value_content_type: i32,
    pub value_summary: Option<&'a str>,
    pub status: i32,
    pub attr_stat_id: Option<i32>,

    #[optional(skip = true)]
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
