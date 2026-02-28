use std::{
    error::Error,
    fs::File,
    hash::Hasher,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use chrono::{DateTime, Utc};
use sha2::{Digest as _, Sha256};
use twox_hash::XxHash64;
use url::Url;

use crate::{
    db::{
        config::Connection,
        util::{Attrs, Contents, Footprints, Groups, Histories, Stats, Workspaces},
        IdGenerate,
    },
    Attr, AttrInsertForm, AttrUpdateForm, Content, ContentInsertForm, Footprint, FootprintInsertForm, Group,
    GroupInsertForm, GroupType, GroupUpdateForm, History, HistoryInsertForm, Stat, StatInsertForm, StatUpdateForm,
    Status, Workspace, WorkspaceInsertForm, ATTR_GROUP_NAME, META_GROUP_NAME,
};

pub(crate) fn create_workspace_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    name: &str,
    now: DateTime<Utc>,
) -> Result<Workspace, Box<dyn Error>> {
    let workspace = Workspaces::find_by_name(conn, name)?;
    Ok(if let Some(workspace) = workspace {
        workspace
    } else {
        let workspace = Workspaces::insert_and_find(
            conn,
            &WorkspaceInsertForm {
                id: id_generator.generate_i64(),
                name,
                description: "",
                status: Status::Enabled as i32,
                created_at: now,
                updated_at: now,
            },
        )?;
        info!("workspace created: {}: {}", workspace.id, &workspace.name);
        trace!("workspace created: {:?}", &workspace);
        workspace
    })
}

pub(crate) fn create_group_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    workspace: &Workspace,
    name: &str,
    url: &Url,
    type_: GroupType,
    now: DateTime<Utc>,
) -> Result<Group, Box<dyn Error>> {
    let group = Groups::find_by_name(conn, workspace.id, name)?;
    Ok(if let Some(group) = group {
        group
    } else {
        let group = Groups::insert_and_find(
            conn,
            &GroupInsertForm {
                id: id_generator.generate_i64(),
                workspace_id: workspace.id,
                name,
                url: url.as_str(),
                type_: type_ as i32,
                description: "",
                status: Status::Enabled as i32,
                group_stat_id: None,
                created_at: now,
                updated_at: now,
            },
        )?;
        info!("group created: {}: {}, {}", group.id, group.workspace_id, &group.name);
        trace!("group created: {:?}", &group);
        group
    })
}

pub(crate) fn create_meta_group_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    workspace: &Workspace,
    now: DateTime<Utc>,
) -> Result<Group, Box<dyn Error>> {
    let group_name = META_GROUP_NAME;
    let url = format!("ichno://{}/{}", workspace.name, group_name);
    let url = Url::parse(&url)?;
    create_group_if_needed(conn, id_generator, workspace, group_name, &url, GroupType::Meta, now)
}

#[allow(dead_code)]
pub(crate) fn create_attr_group_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    workspace: &Workspace,
    now: DateTime<Utc>,
) -> Result<Group, Box<dyn Error>> {
    let group_name = ATTR_GROUP_NAME;
    let url = format!("ichno://{}/groups/{}", workspace.name, group_name);
    let url = Url::parse(&url)?;
    create_group_if_needed(conn, id_generator, workspace, group_name, &url, GroupType::Attr, now)
}

pub(crate) fn create_history_with_footprint_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    group: &Group,
    path: &str,
    footprint: &Footprint,
    mtime: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<History, Box<dyn Error>> {
    let last_history = Histories::find_latest_by_path(conn, group.id, path)?;
    let last_history = if let Some(last_history) = last_history {
        if let Some(last_footprint_id) = last_history.footprint_id {
            if last_footprint_id == footprint.id {
                return Ok(last_history);
            }
        }
        Some(last_history)
    } else {
        None
    };
    let version = match last_history.as_ref() {
        Some(h) => h.version + 1,
        None => 1,
    };
    let history = Histories::insert_and_find(
        conn,
        &HistoryInsertForm {
            id: id_generator.generate_i64(),
            workspace_id: group.workspace_id,
            group_id: group.id,
            path,
            version,
            status: Status::Enabled as i32,
            mtime: Some(mtime),
            footprint_id: Some(footprint.id),
            digest: Some(&footprint.digest),
            created_at: now,
            updated_at: now,
        },
    )?;
    info!("history created: {}: {}, {}", history.id, &history.path, history.version);
    trace!("history created: {:?}", &history);
    Ok(history)
}

pub(crate) fn create_disabled_history_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    group: &Group,
    path: &str,
    now: DateTime<Utc>,
) -> Result<History, Box<dyn Error>> {
    let last_history = Histories::find_latest_by_path(conn, group.id, path)?;
    let last_history = if let Some(last_history) = last_history {
        if last_history.status == Status::Disabled as i32 {
            return Ok(last_history);
        }
        Some(last_history)
    } else {
        None
    };
    let version = match last_history.as_ref() {
        Some(h) => h.version + 1,
        None => 1,
    };
    let history = Histories::insert_and_find(
        conn,
        &HistoryInsertForm {
            id: id_generator.generate_i64(),
            workspace_id: group.workspace_id,
            group_id: group.id,
            path,
            version,
            status: Status::Disabled as i32,
            mtime: None,
            footprint_id: None,
            digest: None,
            created_at: now,
            updated_at: now,
        },
    )?;
    info!("disabled history created: {}: {}, {}", history.id, &history.path, history.version);
    trace!("disabled history created: {:?}", &history);
    Ok(history)
}

pub(crate) fn update_stat_with_footprint_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    group: &Group,
    path: &str,
    footprint: &Footprint,
    mtime: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Stat, Box<dyn Error>> {
    let history = create_history_with_footprint_if_needed(conn, id_generator, group, path, footprint, mtime, now)?;
    let old_stat = Stats::find_by_path(conn, group.id, path)?;
    let old_stat = if let Some(old_stat) = old_stat {
        if old_stat.history_id == history.id {
            return Ok(old_stat);
        }
        Some(old_stat)
    } else {
        None
    };
    let insert_form = StatInsertForm {
        id: id_generator.generate_i64(),
        workspace_id: group.workspace_id,
        group_id: group.id,
        path,
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
    };
    let stat = if let Some(old_stat) = old_stat {
        let update_form = StatUpdateForm::from(insert_form);
        let stat = Stats::update_and_find(conn, old_stat.id, &update_form)?;
        info!("stat updated: {}: {}", stat.id, &stat.path);
        trace!("stat updated: {:?}", &stat);
        stat
    } else {
        let stat = Stats::insert_and_find(conn, &insert_form)?;
        info!("stat created: {}: {}", stat.id, &stat.path);
        trace!("stat created: {:?}", &stat);
        stat
    };
    Ok(stat)
}

pub(crate) fn update_disabled_stat_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    group: &Group,
    path: &str,
    now: DateTime<Utc>,
) -> Result<Option<Stat>, Box<dyn Error>> {
    let history = create_disabled_history_if_needed(conn, id_generator, group, path, now)?;
    let old_stat = Stats::find_by_path(conn, group.id, path)?;
    let old_stat = if let Some(old_stat) = old_stat {
        if old_stat.history_id == history.id {
            return Ok(Some(old_stat));
        }
        old_stat
    } else {
        return Ok(None);
    };
    let insert_form = StatInsertForm {
        id: id_generator.generate_i64(),
        workspace_id: group.workspace_id,
        group_id: group.id,
        path,
        history_id: history.id,
        version: history.version,
        status: history.status,
        mtime: history.mtime,
        footprint_id: history.footprint_id,
        digest: None,
        size: None,
        fast_digest: None,
        created_at: now,
        updated_at: now,
    };
    let update_form = StatUpdateForm::from(insert_form.clone());
    let stat = Stats::update_and_find(conn, old_stat.id, &update_form)?;
    info!("stat disabled: {}: {}", stat.id, &stat.path);
    trace!("stat disabled: {:?}", &stat);
    Ok(Some(stat))
}

pub(crate) fn calc_fast_digest<R: Read>(r: &mut R) -> Result<i64, Box<dyn Error>> {
    let mut buf = [0u8; 8192];
    let mut fast_hasher = XxHash64::default();
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        Hasher::write(&mut fast_hasher, &buf[0..n]);
    }
    Ok(Hasher::finish(&fast_hasher) as i64)
}

pub(crate) fn calc_digest<R: Read>(r: &mut R) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut buf = [0u8; 8192];
    let mut hasher = Sha256::default();
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[0..n])?;
    }
    Ok(hasher.finalize().to_vec())
}

pub(crate) fn create_footprint_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    digest: &[u8],
    size: i64,
    fast_digest: i64,
    now: DateTime<Utc>,
) -> Result<Footprint, Box<dyn Error>> {
    let footprint = Footprints::find_by_digest(conn, &digest)?;
    Ok(if let Some(footprint) = footprint {
        footprint
    } else {
        let footprint = Footprints::insert_and_find(
            conn,
            &FootprintInsertForm {
                id: id_generator.generate_i64(),
                digest: &digest,
                size,
                fast_digest,
                created_at: now,
            },
        )?;
        info!("footprint created: {}: {}", footprint.id, &footprint.digest_string());
        trace!("footprint created: {:?}", &footprint);
        footprint
    })
}

#[allow(dead_code)]
pub(crate) fn create_content_with_bytes_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    bytes: &[u8],
    now: DateTime<Utc>,
) -> Result<(Content, Footprint), Box<dyn Error>> {
    let mut slice = &bytes[..];
    let digest = calc_digest(&mut slice)?;
    let mut slice = &bytes[..];
    let fast_digest = calc_fast_digest(&mut slice)?;
    let footprint = create_footprint_if_needed(conn, id_generator, &digest, bytes.len() as i64, fast_digest, now)?;
    let content = Contents::find_by_footprint_id(conn, footprint.id)?;
    let content = if let Some(content) = content {
        content
    } else {
        let content = Contents::insert_and_find(
            conn,
            &ContentInsertForm {
                id: id_generator.generate_i64(),
                footprint_id: footprint.id,
                body: bytes,
                created_at: now,
            },
        )?;
        info!("content created: {}", content.id);
        trace!("content created: {:?}", &content);
        content
    };
    Ok((content, footprint))
}

#[allow(dead_code)]
pub(crate) fn create_attr_and_stat_with_bytes_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    workspace: &Workspace,
    target: &Footprint,
    key: &str,
    value_type: i32,
    value: &[u8],
    value_text: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(Attr, Content, Stat), Box<dyn Error>> {
    let group = create_attr_group_if_needed(conn, id_generator, workspace, now)?;
    let (content, footprint) = create_content_with_bytes_if_needed(conn, id_generator, value, now)?;
    let path = format!("{}/{}", footprint.digest_string(), key);
    let stat = update_stat_with_footprint_if_needed(conn, id_generator, &group, &path, &footprint, now, now)?;
    let attr = Attrs::find_by_target_footprint_id_and_key(conn, workspace.id, target.id, key)?;
    let attr = if let Some(attr) = attr {
        if attr.attr_stat_id.map_or(false, |i| i == stat.id) {
            attr
        } else {
            Attrs::update_and_find(
                conn,
                attr.id,
                &AttrUpdateForm {
                    value_type: Some(value_type),
                    value_footprint_id: Some(footprint.id),
                    value_digest: Some(&footprint.digest),
                    value_text: Some(value_text),
                    status: Some(Status::Enabled as i32),
                    attr_stat_id: Some(Some(stat.id)),
                    updated_at: Some(now),
                },
            )?
        }
    } else {
        Attrs::insert_and_find(
            conn,
            &AttrInsertForm {
                id: id_generator.generate_i64(),
                workspace_id: workspace.id,
                target_footprint_id: target.id,
                target_digest: &target.digest,
                key,
                value_footprint_id: footprint.id,
                value_digest: &footprint.digest,
                value_type,
                value_text,
                status: Status::Enabled as i32,
                attr_stat_id: Some(stat.id),
                created_at: now,
                updated_at: now,
            },
        )?
    };
    Ok((attr, content, stat))
}

// file

#[derive(Debug)]
pub(crate) enum FileState {
    Enabled(FileMetadata),
    Disabled,
}

#[derive(Debug)]
pub(crate) struct FileMetadata {
    pub size: i64,
    pub mtime: DateTime<Utc>,
    pub fast_digest: i64,
    pub digest: Vec<u8>,
}

pub(crate) type FileStateHandler = Box<dyn Fn(Option<&Stat>, &Path) -> Result<Option<FileState>, Box<dyn Error>>>;

pub(crate) fn new_updated_file_state_if_needed(
    stat: Option<&Stat>,
    path: &Path,
) -> Result<Option<FileState>, Box<dyn Error>> {
    let (f, mtime, size) = if let Ok(f) = File::open(path) {
        let md = f.metadata()?;
        let mtime = DateTime::<Utc>::from(md.modified()?);
        let size = md.len() as i64;
        (Some(f), Some(mtime), Some(size))
    } else {
        (None, None, None)
    };
    let not_exists = stat.map_or(false, |s| s.status == Status::Disabled as i32);
    if f.is_none() && not_exists {
        return Ok(None);
    }
    if stat.and_then(|s| s.mtime) == mtime && stat.and_then(|s| s.size) == size {
        return Ok(None);
    }
    if let (Some(mut f), Some(mtime), Some(size)) = (f, mtime, size) {
        let fast_digest = calc_fast_digest(&mut f)?;
        if let Some(stat) = stat {
            if let Some(old_fast_digest) = stat.fast_digest {
                if fast_digest == old_fast_digest {
                    return Ok(None);
                }
            }
        }
        f.seek(SeekFrom::Start(0))?;
        let digest = calc_digest(&mut f)?;
        if let Some(stat) = stat {
            if let Some(old_digest) = stat.digest.clone() {
                if digest == old_digest {
                    return Ok(None);
                }
            }
        }
        Ok(Some(FileState::Enabled(FileMetadata { size, mtime, fast_digest, digest })))
    } else {
        Ok(Some(FileState::Disabled))
    }
}

pub(crate) fn update_stat_with_paths_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    group: &Group,
    stat_path: &str,
    file_path: &Path,
    now: DateTime<Utc>,
    file_state_handler: Option<FileStateHandler>,
) -> Result<Option<Stat>, Box<dyn Error>> {
    let old_stat = Stats::find_by_path(conn, group.id, stat_path)?;
    let file_state_handler = file_state_handler.unwrap_or(Box::new(new_updated_file_state_if_needed));
    let file_state = file_state_handler(old_stat.as_ref(), file_path)?;
    trace!("updated file state: {:?}", file_state);
    if let Some(file_state) = file_state {
        if let FileState::Enabled(md) = file_state {
            let footprint = create_footprint_if_needed(conn, id_generator, &md.digest, md.size, md.fast_digest, now)?;
            let stat =
                update_stat_with_footprint_if_needed(conn, id_generator, group, stat_path, &footprint, md.mtime, now)?;
            Ok(Some(stat))
        } else {
            let stat = update_disabled_stat_if_needed(conn, id_generator, group, stat_path, now)?;
            Ok(stat)
        }
    } else {
        Ok(old_stat)
    }
}

pub(crate) fn update_stat_with_present_paths_if_needed<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    group: &Group,
    stat_path: &str,
    file_path: &Path,
    now: DateTime<Utc>,
) -> Result<Stat, Box<dyn Error>> {
    update_stat_with_paths_if_needed(conn, id_generator, group, stat_path, file_path, now, None).map(|s| s.unwrap())
}

pub(crate) fn update_meta_group_stat<I: IdGenerate>(
    conn: &mut Connection,
    id_generator: &I,
    workspace: &Workspace,
    group: &Group,
    db_path: &Path,
    now: DateTime<Utc>,
) -> Result<Group, Box<dyn Error>> {
    let stat_path = &group.name;
    let meta_group = create_meta_group_if_needed(conn, id_generator, &workspace, now)?;
    let stat = update_stat_with_present_paths_if_needed(conn, id_generator, &meta_group, stat_path, db_path, now)?;
    let group = Groups::update_and_find(
        conn,
        meta_group.id,
        &GroupUpdateForm { group_stat_id: Some(Some(stat.id)), updated_at: Some(now), ..Default::default() },
    )?;
    trace!("meta group updated: {:?}", group);
    Ok(group)
}
