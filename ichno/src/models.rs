use chrono::NaiveDateTime;
use serde::Serialize;

use crate::db::schema::{footprints, groups, histories, stats};

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "footprints"]
pub struct Footprint {
    pub id: i32,

    pub digest: String,
    pub size: i64,
    pub fast_digest: i64,
    pub git_object_id: String,
}

#[derive(Debug, Insertable)]
#[table_name = "footprints"]
pub struct FootprintInsertForm<'a> {
    pub digest: &'a str,
    pub size: i64,
    pub fast_digest: i64,
    pub git_object_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Serialize, Identifiable, Queryable)]
#[table_name = "histories"]
pub struct History {
    pub id: i32,

    pub group_id: String,
    pub path: String,
    pub version: i32,

    pub status: i32,
    pub mtime: Option<NaiveDateTime>,
    pub footprint_id: Option<i32>,
    pub digest: Option<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Insertable)]
#[table_name = "histories"]
pub struct HistoryInsertForm<'a> {
    pub group_id: &'a str,
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
#[table_name = "groups"]
pub struct Group {
    pub id: String,

    pub url: String,
    pub type_: i32,

    pub hisotry_id: Option<i32>,

    pub version: Option<i32>,
    pub status: Option<i32>,
    pub mtime: Option<NaiveDateTime>,
    pub footprint_id: Option<i32>,

    pub digest: Option<String>,
    pub size: Option<i64>,
    pub fast_digest: Option<i64>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Insertable, Optional)]
#[table_name = "groups"]
#[optional(name = "GroupUpdateForm", derive = "Default, Debug, AsChangeset")]
pub struct GroupInsertForm<'a> {
    #[optional(skip = true)]
    pub id: &'a str,

    pub url: &'a str,
    pub type_: i32,

    pub history_id: Option<i32>,

    pub version: Option<i32>,
    pub status: Option<i32>,
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
#[table_name = "stats"]
pub struct Stat {
    pub id: i32,

    pub group_id: String,
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

#[derive(Debug, Insertable, Optional)]
#[table_name = "stats"]
#[optional(name = "StatUpdateForm", derive = "Default, Debug, AsChangeset")]
pub struct StatInsertForm<'a> {
    #[optional(skip = true)]
    pub group_id: &'a str,
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
