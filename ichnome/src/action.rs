use std::{error::Error, path::Path};

use chrono::{DateTime, Utc};
use diesel::{Connection, SqliteConnection};
use ichno::{
    db::{IdGenerate, SqliteFootprints, SqliteGroups, SqliteHistories, SqliteStats, SqliteWorkspaces},
    id::IdGenerator,
    Status,
};

use tempfile::NamedTempFile;
use tokio::runtime::Handle;
use url::Url;

use crate::{
    constants::GroupType,
    db::{
        actions::{
            create_footprint_if_needed, create_group_if_needed, create_meta_group_if_needed,
            create_workspace_if_needed, new_updated_file_state_if_needed, update_meta_group_stat, FileState,
        },
        Connection as OmConnection, OmFootprints, OmGroups, OmHistories, OmStats, OmWorkspaces,
        StatSearchCondition as OmStatSearchCondition,
    },
    fs::{Filesystem, LocalFilesystem, S3Filesystem, SshFilesystem},
    models::{
        Group, GroupUpdateForm, HistoryInsertForm, StatInsertForm, StatUpdateForm, Workspace, WorkspaceUpdateForm,
    },
    ssh,
};

pub struct Context<'c> {
    pub handle: Handle,
    pub connection: &'c mut OmConnection,
    pub id_generator: &'c IdGenerator,
    pub timer: Box<dyn Fn() -> DateTime<Utc>>,
}

impl<'c> Context<'c> {
    pub fn current_time(&self) -> DateTime<Utc> {
        (self.timer)()
    }
}

#[derive(Debug)]
pub struct SetupRequest {
    pub workspace_name: String,
    pub options: SetupOptions,
}

#[derive(Default, Debug)]
pub struct SetupOptions {
    pub description: Option<String>,
    pub force: bool,
}

#[derive(Debug)]
pub struct SetupResponse {
    pub workspace: Workspace,
}

pub fn setup(ctx: &mut Context, req: &SetupRequest) -> Result<SetupResponse, Box<dyn Error>> {
    let now = ctx.current_time();
    let workspace = OmWorkspaces::find_by_name(ctx.connection, &req.workspace_name)?;
    if let Some(workspace) = workspace {
        if req.options.force {
            OmWorkspaces::update_and_find(
                ctx.connection,
                workspace.id,
                &WorkspaceUpdateForm {
                    description: req.options.description.as_ref().map(|s| s.as_str()),
                    ..Default::default()
                },
            )?;
        } else {
            panic!("workspae {} already exists", workspace.name)
        }
    }
    let workspace = create_workspace_if_needed(ctx.connection, ctx.id_generator, &req.workspace_name, now)?;
    Ok(SetupResponse { workspace })
}

#[derive(Debug)]
pub struct RegisterRequest {
    pub workspace_name: String,
    pub group_name: String,
    pub url: String,
    pub options: RegisterOptions,
}

#[derive(Default, Debug)]
pub struct RegisterOptions {
    pub description: Option<String>,
    pub force: bool,
}

#[derive(Debug)]
pub struct RegisterResponse {
    pub workspace: Workspace,
    pub group: Group,
}

pub fn register(ctx: &mut Context, req: &RegisterRequest) -> Result<RegisterResponse, Box<dyn Error>> {
    let now = ctx.current_time();
    let workspace = OmWorkspaces::find_by_name(ctx.connection, &req.workspace_name)?.unwrap();
    let url = Url::parse(&req.url)?;
    let group = OmGroups::find_by_name(ctx.connection, workspace.id, &req.group_name)?;
    if let Some(group) = group {
        if req.options.force {
            OmGroups::update_and_find(
                ctx.connection,
                group.id,
                &GroupUpdateForm {
                    url: Some(&req.url),
                    description: req.options.description.as_ref().map(|s| s.as_str()),
                    ..Default::default()
                },
            )?;
        } else {
            panic!("group {} already exists", group.name)
        }
    }
    let group = create_group_if_needed(
        ctx.connection,
        ctx.id_generator,
        &workspace,
        &req.group_name,
        &url,
        GroupType::Remote,
        now,
    )?;
    Ok(RegisterResponse { workspace, group })
}

#[derive(Debug)]
pub struct PullRequest {
    pub workspace_name: String,
    pub group_name: String,
    pub options: PullOptions,
}

#[derive(Debug)]
pub struct PullOptions {}

impl Default for PullOptions {
    fn default() -> Self {
        PullOptions {}
    }
}

#[derive(Debug)]
pub struct PullResponse {
    pub group: Group,
}

pub fn pull(ctx: &mut Context, req: &PullRequest) -> Result<PullResponse, Box<dyn Error>> {
    let workspace = OmWorkspaces::find_by_name(ctx.connection, &req.workspace_name)?.unwrap();
    let group = OmGroups::find_by_name(ctx.connection, workspace.id, &req.group_name)?.unwrap();
    let url = Url::parse(&group.url)?;
    let scheme = url.scheme();
    if scheme == "ssh" {
        let tempfile = ssh::download(&url)?;
        load_local_db(ctx, req, &workspace, &group, tempfile.path())?;
        tempfile.close()?;
    } else if scheme == "file" {
        let path = Path::new(url.path());
        load_local_db(ctx, req, &workspace, &group, path)?;
    } else {
        panic!("unknown scheme: {}", scheme);
    }
    Ok(PullResponse { group })
}

fn load_local_db(
    ctx: &mut Context,
    _req: &PullRequest,
    glb_workspace: &Workspace,
    glb_group: &Group,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let now = ctx.current_time();
    let meta_group = create_meta_group_if_needed(ctx.connection, ctx.id_generator, glb_workspace, now)?;
    let meta_stat = OmStats::find_by_path(ctx.connection, meta_group.id, &glb_group.name)?;
    let _updated_metadata = if let Some(FileState::Enabled(updated_metadata)) =
        new_updated_file_state_if_needed(meta_stat.as_ref(), path)?
    {
        updated_metadata
    } else {
        return Ok(());
    };

    let mut loc_conn = SqliteConnection::establish(path.to_str().unwrap())?;
    let loc_conn = &mut loc_conn;
    let loc_workspace_name = ichno::DEFAULT_WORKSPACE_NAME;
    let loc_workspace = SqliteWorkspaces::find_by_name(loc_conn, loc_workspace_name)?.unwrap();
    let loc_group_name = ichno::DEFAULT_GROUP_NAME;
    let loc_group = SqliteGroups::find_by_name(loc_conn, loc_workspace.id, loc_group_name)?.unwrap();

    let loc_stats = SqliteStats::select_by_group_id(loc_conn, loc_group.id)?;
    for loc_stat in loc_stats.iter() {
        let path = &loc_stat.path;
        let glb_stat = OmStats::find_by_path(ctx.connection, glb_group.id, path)?;
        if glb_stat == None || glb_stat.as_ref().unwrap().version != loc_stat.version {
            let loc_histories = SqliteHistories::select_by_path(loc_conn, loc_group.id, path)?;
            for loc_history in loc_histories.iter() {
                if let Some(glb_stat) = glb_stat.as_ref() {
                    if loc_history.version <= glb_stat.version {
                        continue;
                    }
                }
                let glb_footprint = if let Some(loc_footprint_id) = loc_history.footprint_id {
                    let digest = loc_history.digest.as_ref().unwrap();
                    let glb_footprint = OmFootprints::find_by_digest(ctx.connection, digest)?;
                    if let Some(_) = glb_footprint {
                        glb_footprint
                    } else {
                        let loc_footprint = SqliteFootprints::find(loc_conn, loc_footprint_id)?;
                        if let Some(loc_footprint) = loc_footprint {
                            Some(create_footprint_if_needed(
                                ctx.connection,
                                ctx.id_generator,
                                loc_footprint.digest.as_slice(),
                                loc_footprint.size,
                                loc_footprint.fast_digest,
                                now,
                            )?)
                        } else {
                            warn!("Footprint (id: {}) is not found in local DB", loc_footprint_id);
                            None
                        }
                    }
                } else {
                    None
                };
                let glb_history = OmHistories::insert_and_find(
                    ctx.connection,
                    &HistoryInsertForm {
                        id: ctx.id_generator.generate_i64(),
                        workspace_id: glb_workspace.id,
                        group_id: glb_group.id,
                        path,
                        version: loc_history.version,
                        status: loc_history.status,
                        mtime: loc_history.mtime,
                        footprint_id: glb_footprint.as_ref().map(|o| o.id),
                        digest: glb_footprint.as_ref().map(|o| o.digest.as_slice()),
                        created_at: loc_history.created_at,
                        updated_at: loc_history.updated_at,
                    },
                )?;
                if loc_history.version == loc_stat.version {
                    let _glb_stat = if let Some(glb_stat) = glb_stat.as_ref() {
                        OmStats::update_and_find(
                            ctx.connection,
                            glb_stat.id,
                            &StatUpdateForm {
                                history_id: Some(glb_history.id),
                                version: Some(glb_history.version),
                                status: Some(glb_history.status),
                                mtime: Some(glb_history.mtime),
                                footprint_id: Some(glb_history.footprint_id),
                                digest: Some(glb_footprint.as_ref().map(|o| o.digest.as_slice())),
                                size: Some(glb_footprint.as_ref().map(|o| o.size)),
                                fast_digest: Some(glb_footprint.as_ref().map(|o| o.fast_digest)),
                                updated_at: Some(loc_stat.updated_at),
                            },
                        )?
                    } else {
                        OmStats::insert_and_find(
                            ctx.connection,
                            &StatInsertForm {
                                id: ctx.id_generator.generate_i64(),
                                workspace_id: glb_workspace.id,
                                group_id: glb_group.id,
                                path,
                                history_id: glb_history.id,
                                version: glb_history.version,
                                status: glb_history.status,
                                mtime: glb_history.mtime,
                                footprint_id: glb_history.footprint_id,
                                digest: glb_footprint.as_ref().map(|o| o.digest.as_slice()),
                                size: glb_footprint.as_ref().map(|o| o.size),
                                fast_digest: glb_footprint.as_ref().map(|o| o.fast_digest),
                                created_at: loc_stat.created_at,
                                updated_at: loc_stat.updated_at,
                            },
                        )?
                    };
                }
            }
        }
    }

    let _group = update_meta_group_stat(ctx.connection, ctx.id_generator, glb_workspace, glb_group, path, now)?;

    Ok(())
}

#[derive(Debug)]
pub struct CopyRequest {
    pub workspace_name: String,
    pub src_group_name: String,
    pub src_path: String,
    pub dst_group_name: String,
    pub options: CopyOptions,
}

#[derive(Debug)]
pub struct CopyOptions {}

impl Default for CopyOptions {
    fn default() -> Self {
        CopyOptions {}
    }
}

#[derive(Debug)]
pub struct CopyResponse {
    pub src_group: Group,
    pub dst_group: Group,
    pub paths: Vec<(bool, String, Option<String>)>,
}

pub fn copy<'a>(ctx: &'a mut Context<'a>, req: &'a CopyRequest) -> Result<CopyResponse, Box<dyn Error>> {
    let workspace = OmWorkspaces::find_by_name(ctx.connection, &req.workspace_name).unwrap().unwrap();
    let src_group = OmGroups::find_by_name(ctx.connection, workspace.id, &req.src_group_name).unwrap().unwrap();
    let mut src_filesystem = group_filesystem(ctx.handle.clone(), &src_group).unwrap();
    let dst_group = OmGroups::find_by_name(ctx.connection, workspace.id, &req.dst_group_name).unwrap().unwrap();
    let mut dst_filesystem = group_filesystem(ctx.handle.clone(), &dst_group).unwrap();
    let cond = OmStatSearchCondition {
        group_ids: Some(vec![src_group.id]),
        path_prefix: Some(&req.src_path),
        statuses: Some(vec![Status::Enabled]),
        ..Default::default()
    };
    let mut paths: Vec<(bool, String, Option<String>)> = Vec::new();
    let stats = OmStats::search(ctx.connection, workspace.id, &cond).unwrap();
    for stat in stats.iter() {
        let tempfile = NamedTempFile::new()?;
        {
            let _file = tempfile.reopen()?;
            if let Err(e) = src_filesystem.download(&stat.path, tempfile.path()) {
                warn!("failed to download: {}", e);
                paths.push((false, stat.path.to_string(), None));
                continue;
            };
        }
        {
            let dst_path = url_parent(&dst_group.url).and_then(|u| u.join(&stat.path).ok()).unwrap().to_string();
            if let Err(e) = dst_filesystem.upload(&stat.path, tempfile.path()) {
                warn!("failed to upload: {}", e);
                paths.push((false, stat.path.to_string(), Some(dst_path)));
                continue;
            };
            paths.push((true, stat.path.to_string(), Some(dst_path)));
        }
    }
    // TODO: sync database
    Ok(CopyResponse { src_group, dst_group, paths })
}

fn group_filesystem(handle: Handle, group: &Group) -> Option<Box<dyn Filesystem>> {
    let mut url = Url::parse(&group.url).unwrap();
    let path = url.path().rsplitn(2, "/").nth(1).unwrap_or("/").to_string();
    url.set_path(&path);
    if url.scheme() == "ssh" {
        Some(Box::new(SshFilesystem::with_url(&url)?))
    } else if url.scheme() == "file" {
        Some(Box::new(LocalFilesystem::with_url(&url)?))
    } else if url.scheme() == "s3" {
        let client = handle.block_on(async {
            let config = aws_config::load_from_env().await;
            aws_sdk_s3::Client::new(&config)
        });
        Some(Box::new(S3Filesystem::with_url(&url, handle, client)?))
    } else {
        None
    }
}

fn url_parent(url: &str) -> Option<Url> {
    let mut url = Url::parse(&url).unwrap();
    let Some(path) = url.path().rsplitn(2, "/").nth(1).map(|s| s.to_owned()) else { return None };
    url.set_path(&path);
    Some(url)
}
