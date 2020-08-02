use std::error::Error;

use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::{
    db::config::{Backend, Connection},
    impl_crud, impl_select,
    models::{
        Attr, AttrInsertForm, AttrUpdateForm, Content, ContentInsertForm, Footprint, FootprintInsertForm, Group,
        GroupInsertForm, GroupUpdateForm, History, HistoryInsertForm, Stat, StatInsertForm, StatUpdateForm, Workspace,
        WorkspaceInsertForm, WorkspaceUpdateForm,
    },
    Status,
};

embed_migrations!("migrations");

pub fn migrate(conn: &Connection) -> Result<(), Box<dyn Error>> {
    embedded_migrations::run(conn)?;
    Ok(())
}

pub struct Footprints;

impl Footprints {
    impl_crud!(
        Connection, footprints, Footprint, FootprintInsertForm;
        find_by_digest, digest: &str
    );
}

pub struct Contents;

impl Contents {
    impl_crud!(
        Connection, contents, Content, ContentInsertForm;
        find_by_footprint_id, footprint_id: i32
    );
}

pub struct Workspaces;

impl Workspaces {
    impl_crud!(
        Connection, workspaces, Workspace, WorkspaceInsertForm, WorkspaceUpdateForm;
        find_by_name, name: &str
    );
}

pub struct Groups;

impl Groups {
    impl_crud!(
        Connection, groups, Group, GroupInsertForm, GroupUpdateForm;
        find_by_name, workspace_id: i32, name: &str
    );

    impl_select!(Connection, groups, Group; select_all, workspace_id: i32);
}

pub struct Histories;

impl Histories {
    impl_crud!(
        Connection, histories, History, HistoryInsertForm;
        find_by_path_and_version, group_id: i32, path: &str, version: i32
    );

    impl_select!(Connection, histories, History; select_by_path, group_id: i32, path: &str);

    impl_select!(Connection, histories, History; select_by_footprint_id, workspace_id: i32, footprint_id: i32);

    pub fn find_latest_by_path(
        conn: &Connection,
        group_id: i32,
        path: &str,
    ) -> Result<Option<History>, Box<dyn Error>> {
        use crate::db::schema::histories::dsl;
        let q = dsl::histories
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::path.eq(path))
            .order(dsl::version.desc())
            .limit(1);
        Ok(q.first::<History>(conn).optional()?)
    }
}

pub struct Stats;

impl Stats {
    impl_crud!(
        Connection, stats, Stat, StatInsertForm, StatUpdateForm;
        find_by_path, group_id: i32, path: &str
    );

    impl_select!(Connection, stats, Stat; select_by_group_id, group_id: i32);

    impl_select!(Connection, stats, Stat; select_by_footprint_id, workspace_id: i32, footprint_id: i32);

    fn search_condition_to_query<'a>(
        workspace_id: i32,
        cond: &'a StatSearchCondition,
    ) -> crate::db::schema::stats::BoxedQuery<'a, Backend> {
        use crate::db::schema::stats::dsl;
        let mut q = dsl::stats.into_boxed();
        q = q.filter(dsl::workspace_id.eq(workspace_id));
        if let Some(ref group_ids) = cond.group_ids {
            q = q.filter(dsl::group_id.eq_any(group_ids));
        }
        if let Some(ref paths) = cond.paths {
            q = q.filter(dsl::path.eq_any(paths));
        }
        if let Some(path_prefix) = cond.path_prefix {
            q = q.filter(dsl::path.like(format!("{}%", path_prefix)));
        }
        if let Some(path_partial) = cond.path_partial {
            if path_partial.len() >= 2 {
                q = q.filter(dsl::path.like(format!("%{}%", path_partial)));
            }
        }
        if let Some(ref statuses) = cond.statuses {
            q = q.filter(dsl::status.eq_any(statuses.iter().map(|s| *s as i32)));
        }
        if let Some(mtime) = cond.mtime_after {
            q = q.filter(dsl::mtime.ge(mtime));
        }
        if let Some(mtime) = cond.mtime_before {
            q = q.filter(dsl::mtime.lt(mtime));
        }
        if let Some(size) = cond.size_min {
            q = q.filter(dsl::size.ge(size));
        }
        if let Some(size) = cond.size_max {
            q = q.filter(dsl::size.le(size));
        }
        if let Some(updated_at) = cond.updated_at_after {
            q = q.filter(dsl::updated_at.ge(updated_at));
        }
        if let Some(updated_at) = cond.updated_at_before {
            q = q.filter(dsl::updated_at.lt(updated_at));
        }
        if let Some(ref order) = cond.order {
            q = match order {
                StatOrder::PathAsc => q.order(dsl::path.asc()),
                StatOrder::PathDesc => q.order(dsl::path.desc()),
                StatOrder::UpdatedAtAsc => q.order(dsl::updated_at.asc()),
                StatOrder::UpdatedAtDesc => q.order(dsl::updated_at.desc()),
            }
        }
        let limit = cond.limit.unwrap_or(-1);
        if limit >= 0 {
            q = q.limit(limit);
        }
        return q;
    }

    pub fn count(conn: &Connection, workspace_id: i32, cond: &StatSearchCondition) -> Result<i64, Box<dyn Error>> {
        let cond = StatSearchCondition { limit: Some(-1), ..cond.clone() };
        let q = Stats::search_condition_to_query(workspace_id, &cond);
        Ok(q.count().first(conn)?)
    }

    pub fn search(
        conn: &Connection,
        workspace_id: i32,
        cond: &StatSearchCondition,
    ) -> Result<Vec<Stat>, Box<dyn Error>> {
        let q = Stats::search_condition_to_query(workspace_id, cond);
        Ok(q.load::<Stat>(conn)?)
    }
}

#[derive(Default, Debug, Clone)]
pub struct StatSearchCondition<'a> {
    pub group_ids: Option<Vec<i32>>,
    pub paths: Option<Vec<&'a str>>,
    pub path_prefix: Option<&'a str>,
    pub path_partial: Option<&'a str>,
    pub statuses: Option<Vec<Status>>,
    pub mtime_after: Option<NaiveDateTime>,
    pub mtime_before: Option<NaiveDateTime>,
    pub size_min: Option<i64>,
    pub size_max: Option<i64>,
    pub updated_at_after: Option<NaiveDateTime>,
    pub updated_at_before: Option<NaiveDateTime>,
    pub order: Option<StatOrder>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum StatOrder {
    PathAsc,
    PathDesc,
    UpdatedAtAsc,
    UpdatedAtDesc,
}

pub struct Attrs;

impl Attrs {
    impl_crud!(
        Connection, attrs, Attr, AttrInsertForm, AttrUpdateForm;
        find_by_target_footprint_id_and_key, workspace_id: i32, target_footprint_id: i32, key: &str
    );

    impl_select!(Connection, attrs, Attr; select_by_value_footprint_id, workspace_id: i32, value_footprint_id: i32);

    impl_select!(Connection, attrs, Attr; select_by_key_and_value_summary, workspace_id: i32, key: &str, value_summary: &str);

    impl_select!(Connection, attrs, Attr; select_by_target_footprint_id, workspace_id: i32, target_footprint_id: i32);
}
