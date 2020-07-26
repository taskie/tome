use std::error::Error;

use diesel::prelude::*;

use crate::{
    db::config::Connection,
    impl_crud, impl_select,
    models::{
        Attr, AttrInsertForm, AttrUpdateForm, Content, ContentInsertForm, Footprint, FootprintInsertForm, Group,
        GroupInsertForm, GroupUpdateForm, History, HistoryInsertForm, Stat, StatInsertForm, StatUpdateForm, Workspace,
        WorkspaceInsertForm, WorkspaceUpdateForm,
    },
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
