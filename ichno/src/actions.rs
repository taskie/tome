use std::{
    convert::AsRef,
    error::Error,
    fs::File,
    hash::Hasher,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use diesel::sqlite::SqliteConnection;
use sha1::Sha1;
use sha2::{digest::FixedOutput, Sha256};
use twox_hash::XxHash64;
use url::Url;

use crate::{
    constants::{GroupType, Status, DEFAULT_WORKSPACE_NAME, META_GROUP_NAME},
    db::{
        actions::{
            calc_digest, calc_fast_digest, create_group_if_needed, create_meta_group_if_needed,
            create_workspace_if_needed, update_disabled_stat_if_needed, update_meta_group_stat,
            update_stat_with_paths_if_needed, update_stat_with_present_paths_if_needed,
        },
        SqliteFootprints, SqliteGroups, SqliteHistories, SqliteStats, SqliteWorkspaces,
    },
    models::{
        FootprintInsertForm, Group, GroupInsertForm, GroupUpdateForm, HistoryInsertForm, Stat, StatInsertForm,
        StatUpdateForm,
    },
    Workspace, WorkspaceInsertForm, DEFAULT_GROUP_NAME,
};

pub struct Context<'c, 'a> {
    pub connection: &'c SqliteConnection,
    pub db_path: &'a Path,
    pub workspace_name: &'a str,
    pub workspace: Option<Workspace>,
    pub group_name: &'a str,
    pub group: Option<Group>,
    pub timer: Box<dyn Fn() -> DateTime<Utc>>,
}

impl<'c, 'a> Context<'c, 'a> {
    pub fn current_time(&self) -> DateTime<Utc> {
        (self.timer)()
    }

    pub fn naive_current_time(&self) -> NaiveDateTime {
        self.current_time().naive_utc()
    }

    pub fn base_directory(&self) -> Option<PathBuf> {
        self.group
            .as_ref()
            .and_then(|ns| Url::parse(&ns.url).ok())
            .map(|url| PathBuf::from(url.path()))
            .and_then(|p| p.parent().map(PathBuf::from))
    }
}

pub fn pre_process(ctx: &mut Context) -> Result<(), Box<dyn Error>> {
    let conn = ctx.connection;
    let now = ctx.naive_current_time();
    let workspace = create_workspace_if_needed(conn, ctx.workspace_name, now)?;
    ctx.workspace = Some(workspace.clone());
    let abs_db_path = ctx.db_path.canonicalize()?;
    let url = Url::from_file_path(abs_db_path).unwrap();
    ctx.group = Some(create_group_if_needed(conn, &workspace, ctx.group_name, &url, GroupType::LOCAL, now)?);
    Ok(())
}

pub fn post_process(ctx: &mut Context) -> Result<(), Box<dyn Error>> {
    let conn = ctx.connection;
    let now = ctx.naive_current_time();
    let workspace = ctx.workspace.as_ref().unwrap();
    let target_group = ctx.group.as_ref().unwrap();
    let group = update_meta_group_stat(conn, workspace, target_group, ctx.db_path, now)?;
    ctx.group = Some(group);
    Ok(())
}

pub fn update_file_stat<P: AsRef<Path>>(ctx: &Context, path: P) -> Result<Option<Stat>, Box<dyn Error>> {
    let conn = ctx.connection;
    let group = ctx.group.as_ref().unwrap();
    let now = ctx.naive_current_time();
    let base_path = ctx.base_directory().unwrap();
    let path = if path.as_ref().is_absolute() { PathBuf::from(path.as_ref()) } else { base_path.join(path) };
    let path_ref = path.strip_prefix(base_path)?;
    let path_str = path_ref.to_str().expect(&format!("invalid path string"));
    update_stat_with_paths_if_needed(conn, group, path_str, path_ref, now)
}
