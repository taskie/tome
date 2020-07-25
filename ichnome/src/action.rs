use std::{error::Error, path::Path};

use chrono::{DateTime, TimeZone};
use diesel::{Connection, MysqlConnection, SqliteConnection};
use ichno::{
    db::{SqliteFootprints, SqliteHistories, SqliteStats},
    Status,
};
use sha1::Sha1;
use url::Url;

use crate::{
    constants::{GroupType, META_NAMESPACE_ID},
    db::{MysqlFootprints, MysqlGroups, MysqlHistories, MysqlStats},
    file,
    file::FileMetadata,
    models::{
        FootprintInsertForm, Group, GroupInsertForm, GroupUpdateForm, HistoryInsertForm, Stat, StatInsertForm,
        StatUpdateForm,
    },
    ssh,
};
use sha1::digest::FixedOutput;

pub struct Context<'c> {
    pub connection: &'c MysqlConnection,
}

#[derive(Debug)]
pub struct RegisterRequest<Tz: TimeZone> {
    pub group_id: String,
    pub url: String,
    pub current_time: DateTime<Tz>,
    pub options: RegisterOptions,
}

#[derive(Debug)]
pub struct RegisterOptions {
    pub force: bool,
}

#[derive(Debug)]
pub struct RegisterResponse {
    pub group: Group,
}

impl Default for RegisterOptions {
    fn default() -> Self {
        RegisterOptions { force: false }
    }
}

pub fn register<Tz: TimeZone>(ctx: &Context, req: &RegisterRequest<Tz>) -> Result<RegisterResponse, Box<dyn Error>> {
    let conn = ctx.connection;
    Url::parse(&req.url)?;
    let group = MysqlGroups::find(conn, &req.group_id)?;
    let group = if let Some(_) = group {
        if !req.options.force {
            panic!(format!("group duplicated: {}", req.group_id));
        }
        MysqlGroups::update_and_find(
            conn,
            &req.group_id,
            &GroupUpdateForm {
                url: Some(&req.url),
                type_: Some(GroupType::REMOTE as i32),
                history_id: Some(None),
                version: Some(None),
                status: Some(None),
                mtime: Some(None),
                footprint_id: Some(None),
                digest: Some(None),
                size: Some(None),
                fast_digest: Some(None),
                updated_at: Some(req.current_time.naive_utc()),
            },
        )?
    } else {
        MysqlGroups::insert_and_find(
            conn,
            &GroupInsertForm {
                id: &req.group_id,
                url: &req.url,
                type_: GroupType::REMOTE as i32,
                history_id: None,
                version: None,
                status: None,
                mtime: None,
                footprint_id: None,
                digest: None,
                size: None,
                fast_digest: None,
                created_at: req.current_time.naive_utc(),
                updated_at: req.current_time.naive_utc(),
            },
        )?
    };
    Ok(RegisterResponse { group })
}

#[derive(Debug)]
pub struct PullRequest<Tz: TimeZone> {
    pub group_id: String,
    pub current_time: DateTime<Tz>,
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

pub fn pull<Tz: TimeZone>(ctx: &Context, req: &PullRequest<Tz>) -> Result<PullResponse, Box<dyn Error>> {
    let conn = ctx.connection;
    let group = MysqlGroups::find(conn, &req.group_id)?;
    let group = group.unwrap();
    let url = Url::parse(&group.url)?;
    let scheme = url.scheme();
    if scheme == "ssh" {
        let tempfile = ssh::download(&url)?;
        load_local_db(ctx, req, &group, tempfile.path())?;
        tempfile.close()?;
    } else if scheme == "file" {
        let path = Path::new(url.path());
        load_local_db(ctx, req, &group, path)?;
    } else {
        panic!(format!("unknown scheme: {}", scheme));
    }
    Ok(PullResponse { group })
}

fn load_local_db<Tz: TimeZone>(
    ctx: &Context,
    req: &PullRequest<Tz>,
    _group: &Group,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let global_conn = ctx.connection;
    let global_group_id = req.group_id.as_str();

    let meta_stat = MysqlStats::find_by_path(global_conn, META_NAMESPACE_ID, global_group_id)?;
    let updated_metadata = file::new_updated_metadata_if_needed(&meta_stat, path)?;
    match updated_metadata {
        None => {
            return Ok(());
        }
        _ => {}
    }
    let updated_metadata = updated_metadata.unwrap();

    let local_conn = SqliteConnection::establish(path.to_str().unwrap())?;
    let local_conn = &local_conn;
    let local_group_id = ichno::DEFAULT_NAMESPACE_ID;

    let local_stats = SqliteStats::select_by_group_id(&local_conn, local_group_id)?;
    for local_stat in local_stats.iter() {
        let path = &local_stat.path;
        let global_stat = MysqlStats::find_by_path(global_conn, global_group_id, path)?;
        if global_stat == None || global_stat.as_ref().unwrap().version != local_stat.version {
            let local_histories = SqliteHistories::select_by_path(local_conn, local_group_id, path)?;
            for local_history in local_histories.iter() {
                if let Some(global_stat) = global_stat.as_ref() {
                    if local_history.version <= global_stat.version {
                        continue;
                    }
                }
                let global_footprint = if let Some(local_footprint_id) = local_history.footprint_id {
                    let digest = local_history.digest.as_ref().unwrap();
                    let global_footprint = MysqlFootprints::find_by_digest(global_conn, digest)?;
                    if let Some(_) = global_footprint {
                        global_footprint
                    } else {
                        let local_footprint = SqliteFootprints::find(local_conn, local_footprint_id)?;
                        if let Some(local_footprint) = local_footprint {
                            Some(MysqlFootprints::insert_and_find(
                                global_conn,
                                &FootprintInsertForm {
                                    digest: local_footprint.digest.as_str(),
                                    size: local_footprint.size,
                                    fast_digest: local_footprint.fast_digest,
                                    git_object_id: local_footprint.git_object_id.as_str(),
                                },
                            )?)
                        } else {
                            warn!("Footprint (id: {}) is not found in local DB", local_footprint_id);
                            None
                        }
                    }
                } else {
                    None
                };
                let global_history = MysqlHistories::insert_and_find(
                    global_conn,
                    &HistoryInsertForm {
                        group_id: global_group_id,
                        path,
                        version: local_history.version,
                        status: local_history.status,
                        mtime: local_history.mtime,
                        footprint_id: global_footprint.as_ref().map(|o| o.id),
                        digest: global_footprint.as_ref().map(|o| o.digest.as_str()),
                        created_at: local_history.created_at,
                        updated_at: local_history.updated_at,
                    },
                )?;
                if local_history.version == local_stat.version {
                    let _global_stat = if let Some(global_stat) = global_stat.as_ref() {
                        MysqlStats::update_and_find(
                            global_conn,
                            global_stat.id,
                            &StatUpdateForm {
                                history_id: Some(global_history.id),
                                version: Some(global_history.version),
                                status: Some(global_history.status),
                                mtime: Some(global_history.mtime),
                                footprint_id: Some(global_history.footprint_id),
                                digest: Some(global_footprint.as_ref().map(|o| o.digest.as_str())),
                                size: Some(global_footprint.as_ref().map(|o| o.size)),
                                fast_digest: Some(global_footprint.as_ref().map(|o| o.fast_digest)),
                                updated_at: Some(local_stat.updated_at),
                            },
                        )?
                    } else {
                        MysqlStats::insert_and_find(
                            global_conn,
                            &StatInsertForm {
                                group_id: global_group_id,
                                path,
                                history_id: global_history.id,
                                version: global_history.version,
                                status: global_history.status,
                                mtime: global_history.mtime,
                                footprint_id: global_history.footprint_id,
                                digest: global_footprint.as_ref().map(|o| o.digest.as_str()),
                                size: global_footprint.as_ref().map(|o| o.size),
                                fast_digest: global_footprint.as_ref().map(|o| o.fast_digest),
                                created_at: local_stat.created_at,
                                updated_at: local_stat.updated_at,
                            },
                        )?
                    };
                }
            }
        }
    }

    let stat = update_metadata(ctx, req, path, meta_stat.as_ref(), &updated_metadata)?;
    MysqlGroups::update(
        global_conn,
        global_group_id,
        &GroupUpdateForm {
            url: None,
            type_: None,
            history_id: Some(Some(stat.history_id)),
            version: Some(Some(stat.version)),
            status: Some(Some(stat.status)),
            mtime: Some(stat.mtime),
            footprint_id: Some(stat.footprint_id),
            digest: Some(stat.digest.as_ref().map(|s| s.as_str())),
            size: Some(stat.size),
            fast_digest: Some(stat.fast_digest),
            updated_at: Some(stat.updated_at),
        },
    )?;

    Ok(())
}

fn update_metadata<Tz: TimeZone>(
    ctx: &Context,
    req: &PullRequest<Tz>,
    path: &Path,
    meta_stat: Option<&Stat>,
    metadata: &FileMetadata,
) -> Result<Stat, Box<dyn Error>> {
    let conn = ctx.connection;
    let meta_group_id = META_NAMESPACE_ID;
    let global_group_id = &req.group_id;
    let now = req.current_time.naive_utc();
    let mut footprint = MysqlFootprints::find_by_digest(conn, &metadata.digest)?;
    if footprint == None {
        let mut hasher = Sha1::default();
        treblo::object::blob_from_path(&mut hasher, path)?;
        let git_object_id = treblo::hex::to_hex_string(hasher.fixed_result().as_slice());
        footprint = Some(MysqlFootprints::insert_and_find(
            conn,
            &FootprintInsertForm {
                digest: &metadata.digest,
                size: metadata.size,
                fast_digest: metadata.fast_digest,
                git_object_id: &git_object_id,
            },
        )?);
    }
    let footprint = footprint.unwrap();
    trace!("- footprint: {:?}", footprint);
    let last_history = MysqlHistories::find_latest_by_path(conn, meta_group_id, global_group_id)?;
    let version = match last_history {
        Some(h) => h.version + 1,
        None => 1,
    };
    let history = MysqlHistories::insert_and_find(
        conn,
        &HistoryInsertForm {
            group_id: meta_group_id,
            path: global_group_id,
            version,
            status: Status::ENABLED as i32,
            mtime: Some(metadata.mtime),
            footprint_id: Some(footprint.id),
            digest: Some(&footprint.digest),
            created_at: now,
            updated_at: now,
        },
    )?;
    trace!("- history: {:?}", history);
    let stat = match meta_stat {
        Some(old_stat) => MysqlStats::update_and_find(
            conn,
            old_stat.id,
            &StatUpdateForm {
                history_id: Some(history.id),
                version: Some(history.version),
                status: Some(history.status),
                mtime: Some(history.mtime),
                footprint_id: Some(history.footprint_id),
                digest: Some(Some(footprint.digest.as_str())),
                size: Some(Some(footprint.size)),
                fast_digest: Some(Some(footprint.fast_digest)),
                updated_at: Some(now),
            },
        )?,
        None => MysqlStats::insert_and_find(
            conn,
            &StatInsertForm {
                group_id: meta_group_id,
                path: global_group_id,
                history_id: history.id,
                version: history.version,
                status: history.status,
                mtime: history.mtime,
                footprint_id: history.footprint_id,
                digest: Some(&footprint.digest),
                size: Some(footprint.size),
                fast_digest: Some(footprint.fast_digest),
                created_at: now,
                updated_at: now,
            },
        )?,
    };
    trace!("- stat: {:?}", stat);
    Ok(stat)
}
