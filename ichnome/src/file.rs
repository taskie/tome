use std::{
    convert::AsRef,
    error::Error,
    fs::File,
    hash::Hasher,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chrono::{DateTime, NaiveDateTime, TimeZone};
use diesel::mysql::MysqlConnection;
use sha1::Sha1;
use sha2::{digest::FixedOutput, Sha256};
use twox_hash::XxHash64;
use url::Url;

use crate::{
    constants::{GroupType, Status, META_NAMESPACE_ID},
    db::{MysqlFootprints, MysqlGroups, MysqlHistories, MysqlStats},
    models::{
        FootprintInsertForm, Group, GroupInsertForm, GroupUpdateForm, HistoryInsertForm, Stat, StatInsertForm,
        StatUpdateForm,
    },
};

#[derive(Debug)]
pub(crate) struct FileMetadata {
    pub size: i64,
    pub mtime: NaiveDateTime,
    pub fast_digest: i64,
    pub digest: String,
}

pub(crate) fn new_updated_metadata_if_needed(
    stat: &Option<Stat>,
    path: &Path,
) -> Result<Option<FileMetadata>, Box<dyn Error>> {
    let mut f = File::open(path)?;
    let mut should_check_fast_digest = false;
    let md = f.metadata()?;
    let modified_unix = md.modified()?.duration_since(UNIX_EPOCH)?;
    let mtime = NaiveDateTime::from_timestamp(modified_unix.as_secs() as i64, modified_unix.subsec_nanos());
    if let Some(stat) = stat {
        if let Some(old_mtime) = stat.mtime {
            if mtime != old_mtime {
                should_check_fast_digest = true;
            }
        }
    }
    let size = md.len() as i64;
    if let Some(stat) = stat {
        if let Some(old_size) = stat.size {
            if size != old_size {
                should_check_fast_digest = true;
            }
        }
    }
    if let Some(_) = stat {
        if !should_check_fast_digest {
            return Ok(None);
        }
    }
    let mut buf = [0u8; 8096];
    let fast_digest = {
        let mut fast_hasher = XxHash64::default();
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            Hasher::write(&mut fast_hasher, &buf[0..n]);
        }
        Hasher::finish(&fast_hasher) as i64
    };
    if let Some(stat) = stat {
        if let Some(old_fast_digest) = stat.fast_digest {
            if fast_digest == old_fast_digest {
                return Ok(None);
            }
        }
    }
    f.seek(SeekFrom::Start(0))?;
    let digest = {
        let mut hasher = Sha256::default();
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.write(&buf[0..n])?;
        }
        treblo::hex::to_hex_string(hasher.fixed_result().as_slice())
    };
    if let Some(stat) = stat {
        if let Some(old_digest) = stat.digest.clone() {
            if digest == old_digest {
                return Ok(None);
            }
        }
    }
    Ok(Some(FileMetadata { size, mtime, fast_digest, digest }))
}

pub struct Context<'c, 'a, Tz: TimeZone> {
    pub connection: &'c MysqlConnection,
    pub db_path: &'a Path,
    pub group_id: &'a str,
    pub group: Option<Group>,
    pub current_time: DateTime<Tz>,
}

impl<'c, 'a, Tz: TimeZone> Context<'c, 'a, Tz> {
    pub fn naive_current_time(&self) -> NaiveDateTime {
        self.current_time.naive_utc()
    }

    pub fn base_directory(&self) -> Option<PathBuf> {
        self.group
            .as_ref()
            .and_then(|ns| Url::parse(&ns.url).ok())
            .map(|url| PathBuf::from(url.path()))
            .and_then(|p| p.parent().map(PathBuf::from))
    }
}

pub fn pre_process<Tz: TimeZone>(ctx: &mut Context<Tz>) -> Result<(), Box<dyn Error>> {
    let conn = ctx.connection;
    let group_id = ctx.group_id;
    let now = ctx.naive_current_time();
    let mut group = MysqlGroups::find(conn, group_id)?;
    if let Some(group) = group {
        ctx.group = Some(group);
    } else {
        let abs_db_path = ctx.db_path.canonicalize()?;
        let url = format!("file://{}", abs_db_path.to_str().unwrap());
        group = Some(MysqlGroups::insert_and_find(
            &conn,
            &GroupInsertForm {
                id: group_id,
                url: &url,
                type_: GroupType::LOCAL as i32,
                history_id: None,
                version: None,
                status: None,
                mtime: None,
                footprint_id: None,
                digest: None,
                size: None,
                fast_digest: None,
                created_at: now,
                updated_at: now,
            },
        )?);
        trace!("- group: {:?}", group.as_ref().unwrap());
        ctx.group = Some(group.unwrap().clone());
    }
    Ok(())
}

pub fn post_process<Tz: TimeZone>(ctx: &mut Context<Tz>) -> Result<(), Box<dyn Error>> {
    let meta_group_id = META_NAMESPACE_ID;
    let stat = upsert_with_file_without_canonicalization(ctx, meta_group_id, ctx.group_id, ctx.db_path)?;
    let group = MysqlGroups::update_and_find(
        ctx.connection,
        ctx.group_id,
        &GroupUpdateForm {
            type_: None,
            url: None,
            history_id: Some(Some(stat.history_id)),
            version: Some(Some(stat.version)),
            status: Some(Some(stat.status)),
            mtime: Some(stat.mtime),
            footprint_id: Some(stat.footprint_id),
            digest: Some(stat.digest.as_ref().map(|s| s.as_ref())),
            size: Some(stat.size),
            fast_digest: Some(stat.fast_digest),
            updated_at: Some(ctx.naive_current_time()),
        },
    )?;
    trace!("- group: {:?}", group);
    ctx.group = Some(group);
    Ok(())
}

pub fn remove_with_file<Tz: TimeZone, P: AsRef<Path>>(
    ctx: &Context<Tz>,
    group_id: &str,
    path: P,
) -> Result<Option<Stat>, Box<dyn Error>> {
    let conn = ctx.connection;
    let base_path = ctx.base_directory().unwrap();
    let path = if path.as_ref().is_absolute() { PathBuf::from(path.as_ref()) } else { base_path.join(path) };
    let path_ref = path.strip_prefix(base_path)?;
    let path_str = path_ref.to_str().expect(&format!("invalid path string"));
    debug!("* {}", path_str);
    let old_stat = MysqlStats::find_by_path(conn, group_id, path_str)?;
    if let Some(old_stat) = old_stat {
        if old_stat.status == Status::DISABLED as i32 {
            return Ok(Some(old_stat));
        }
        let now = ctx.naive_current_time();
        let last_history = MysqlHistories::find_latest_by_path(conn, group_id, path_str)?;
        let version = match last_history {
            Some(h) => h.version + 1,
            None => 1,
        };
        let history = MysqlHistories::insert_and_find(
            conn,
            &HistoryInsertForm {
                group_id,
                path: path_str,
                version,
                status: Status::DISABLED as i32,
                mtime: None,
                footprint_id: None,
                digest: None,
                created_at: now,
                updated_at: now,
            },
        )?;
        trace!("- history: {:?}", history);
        let stat = MysqlStats::update_and_find(
            conn,
            old_stat.id,
            &StatUpdateForm {
                history_id: Some(history.id),
                version: Some(history.version),
                status: Some(history.status),
                mtime: Some(history.mtime),
                footprint_id: Some(history.footprint_id),
                digest: Some(None),
                size: Some(None),
                fast_digest: Some(None),
                updated_at: Some(now),
            },
        )?;
        trace!("- stat: {:?}", stat);
        Ok(Some(stat))
    } else {
        Ok(None)
    }
}

pub fn upsert_with_file<Tz: TimeZone, P: AsRef<Path>>(
    ctx: &Context<Tz>,
    group_id: &str,
    path: P,
) -> Result<Stat, Box<dyn Error>> {
    let base_path = ctx.base_directory().unwrap();
    let path = if path.as_ref().is_absolute() { PathBuf::from(path.as_ref()) } else { base_path.join(path) };
    let path_ref = path.strip_prefix(base_path)?;
    let path_str = path_ref.to_str().expect(&format!("invalid path string"));
    upsert_with_file_without_canonicalization(ctx, group_id, path_str, path_ref)
}

pub(crate) fn upsert_with_file_without_canonicalization<Tz: TimeZone>(
    ctx: &Context<Tz>,
    group_id: &str,
    path_str: &str,
    file_path: &Path,
) -> Result<Stat, Box<dyn Error>> {
    let conn = ctx.connection;
    debug!("* {}", path_str);
    let old_stat = MysqlStats::find_by_path(conn, group_id, path_str)?;
    let now = ctx.naive_current_time();
    let metadata = new_updated_metadata_if_needed(&old_stat, file_path)?;
    trace!("- updated_metadata: {:?}", metadata);
    if let Some(metadata) = metadata {
        let mut footprint = MysqlFootprints::find_by_digest(conn, &metadata.digest)?;
        if footprint == None {
            let mut hasher = Sha1::default();
            treblo::object::blob_from_path(&mut hasher, file_path)?;
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
        let last_history = MysqlHistories::find_latest_by_path(conn, group_id, path_str)?;
        let version = match last_history {
            Some(h) => h.version + 1,
            None => 1,
        };
        let history = MysqlHistories::insert_and_find(
            conn,
            &HistoryInsertForm {
                group_id,
                path: path_str,
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
        let stat = match old_stat {
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
                    group_id,
                    path: path_str,
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
    } else {
        Ok(old_stat.unwrap())
    }
}
