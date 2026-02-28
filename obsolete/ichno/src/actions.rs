use std::{
    convert::AsRef,
    error::Error,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use diesel::sqlite::SqliteConnection;

use url::Url;

use crate::{
    constants::GroupType,
    db::actions::{
        create_group_if_needed, create_workspace_if_needed, update_meta_group_stat, update_stat_with_paths_if_needed,
    },
    error::DomainError,
    id::IdGenerator,
    models::{Group, Stat, Workspace},
};

pub struct Context<'c, 'a> {
    pub connection: &'c mut SqliteConnection,
    pub id_generator: &'c IdGenerator,
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

    pub fn base_directory(&self) -> Option<PathBuf> {
        self.group
            .as_ref()
            .and_then(|ns| Url::parse(&ns.url).ok())
            .map(|url| PathBuf::from(url.path()))
            .and_then(|p| p.parent().map(PathBuf::from))
    }
}

pub fn pre_process(ctx: &mut Context) -> Result<(), Box<dyn Error>> {
    let now = ctx.current_time();
    let workspace = create_workspace_if_needed(ctx.connection, ctx.id_generator, ctx.workspace_name, now)?;
    ctx.workspace = Some(workspace.clone());
    let abs_db_path = ctx.db_path.canonicalize()?;
    let url = Url::from_file_path(abs_db_path).unwrap();
    ctx.group = Some(create_group_if_needed(
        ctx.connection,
        ctx.id_generator,
        &workspace,
        ctx.group_name,
        &url,
        GroupType::Local,
        now,
    )?);
    Ok(())
}

pub fn post_process(ctx: &mut Context) -> Result<(), Box<dyn Error>> {
    let now = ctx.current_time();
    let workspace = ctx.workspace.as_ref().unwrap();
    let target_group = ctx.group.as_ref().unwrap();
    let group = update_meta_group_stat(ctx.connection, ctx.id_generator, workspace, target_group, ctx.db_path, now)?;
    ctx.group = Some(group);
    Ok(())
}

pub fn update_file_stat<P: AsRef<Path>>(ctx: &mut Context, path: P) -> Result<Option<Stat>, Box<dyn Error>> {
    let group = ctx.group.as_ref().unwrap();
    let now = ctx.current_time();
    let base_path = ctx.base_directory().unwrap();
    let path = if path.as_ref().is_absolute() { PathBuf::from(path.as_ref()) } else { base_path.join(path) };
    let path_ref = path.strip_prefix(base_path)?;
    let path_str = if let Some(s) = path_ref.to_str() {
        s
    } else {
        return Err(Box::new(DomainError::params("path", format!("can't convert to UTF-8: {:?}", path_ref))));
    };
    update_stat_with_paths_if_needed(ctx.connection, ctx.id_generator, group, path_str, path_ref, now, None)
}
