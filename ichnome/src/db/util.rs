use std::error::Error;

use diesel::prelude::*;

use crate::models::{
    Footprint, FootprintInsertForm, Group, GroupInsertForm, GroupUpdateForm, History, HistoryInsertForm, Stat,
    StatInsertForm, StatUpdateForm,
};

embed_migrations!("migrations");

pub fn migrate(conn: &SqliteConnection) -> Result<(), Box<dyn Error>> {
    embedded_migrations::run(conn)?;
    Ok(())
}

pub struct MysqlFootprints;

impl MysqlFootprints {
    pub fn find(conn: &MysqlConnection, id: i32) -> Result<Option<Footprint>, Box<dyn Error>> {
        use crate::db::schema::footprints::dsl;
        let q = dsl::footprints.find(id);
        Ok(q.first::<Footprint>(conn).optional()?)
    }

    pub fn select(conn: &MysqlConnection, ids: &Vec<i32>) -> Result<Vec<Footprint>, Box<dyn Error>> {
        use crate::db::schema::footprints::dsl;
        let q = dsl::footprints.filter(dsl::id.eq_any(ids));
        Ok(q.load::<Footprint>(conn)?)
    }

    pub fn find_by_digest(conn: &MysqlConnection, digest: &str) -> Result<Option<Footprint>, Box<dyn Error>> {
        use crate::db::schema::footprints::dsl;
        let q = dsl::footprints.filter(dsl::digest.eq(digest));
        Ok(q.first::<Footprint>(conn).optional()?)
    }

    pub fn insert(conn: &MysqlConnection, footprint_form: &FootprintInsertForm) -> Result<(), Box<dyn Error>> {
        use crate::db::schema::footprints::dsl;
        let q = diesel::insert_into(dsl::footprints).values(footprint_form);
        q.execute(conn)?;
        Ok(())
    }

    pub fn insert_and_find(
        conn: &MysqlConnection,
        footprint_form: &FootprintInsertForm,
    ) -> Result<Footprint, Box<dyn Error>> {
        Self::insert(conn, footprint_form)?;
        let footprint = Self::find_by_digest(conn, footprint_form.digest)?;
        Ok(footprint.unwrap())
    }
}

pub struct MysqlHistories;

impl MysqlHistories {
    pub fn find(conn: &MysqlConnection, id: i32) -> Result<Option<History>, Box<dyn Error>> {
        use crate::db::schema::histories::dsl;
        let q = dsl::histories.find(id);
        Ok(q.first::<History>(conn).optional()?)
    }

    pub fn find_latest_by_path(
        conn: &MysqlConnection,
        group_id: &str,
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

    pub fn find_by_path_and_version(
        conn: &MysqlConnection,
        group_id: &str,
        path: &str,
        version: i32,
    ) -> Result<Option<History>, Box<dyn Error>> {
        use crate::db::schema::histories::dsl;
        let q = dsl::histories
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::path.eq(path))
            .filter(dsl::version.eq(version));
        Ok(q.first::<History>(conn).optional()?)
    }

    pub fn select_by_path(conn: &MysqlConnection, group_id: &str, path: &str) -> Result<Vec<History>, Box<dyn Error>> {
        use crate::db::schema::histories::dsl;
        let q = dsl::histories.filter(dsl::group_id.eq(group_id)).filter(dsl::path.eq(path)).order(dsl::version.asc());
        let histories = q.load::<History>(conn)?;
        Ok(histories)
    }

    pub fn select_by_footprint_id(
        conn: &MysqlConnection,
        group_id: Option<&str>,
        footprint_id: i32,
    ) -> Result<Vec<History>, Box<dyn Error>> {
        use crate::db::schema::histories::dsl;
        if let Some(group_id) = group_id {
            let expr = dsl::footprint_id.eq(footprint_id).and(dsl::group_id.eq(group_id));
            let q = dsl::histories.filter(expr).order(dsl::group_id.asc()).order(dsl::path.asc());
            Ok(q.load::<History>(conn)?)
        } else {
            let expr = dsl::footprint_id.eq(footprint_id);
            let q = dsl::histories.filter(expr).order(dsl::group_id.asc()).order(dsl::path.asc());
            Ok(q.load::<History>(conn)?)
        }
    }

    pub fn insert(conn: &MysqlConnection, history_form: &HistoryInsertForm) -> Result<(), Box<dyn Error>> {
        use crate::db::schema::histories::dsl;
        let q = diesel::insert_into(dsl::histories).values(history_form);
        q.execute(conn)?;
        Ok(())
    }

    pub fn insert_and_find(
        conn: &MysqlConnection,
        history_form: &HistoryInsertForm,
    ) -> Result<History, Box<dyn Error>> {
        Self::insert(conn, history_form)?;
        let history =
            Self::find_by_path_and_version(conn, history_form.group_id, history_form.path, history_form.version)?;
        Ok(history.unwrap())
    }
}

pub struct MysqlGroups;

impl MysqlGroups {
    pub fn find(conn: &MysqlConnection, id: &str) -> Result<Option<Group>, Box<dyn Error>> {
        use crate::db::schema::groups::dsl;
        let q = dsl::groups.find(id);
        Ok(q.first::<Group>(conn).optional()?)
    }

    pub fn select(conn: &MysqlConnection, ids: &Vec<&str>) -> Result<Vec<Group>, Box<dyn Error>> {
        use crate::db::schema::groups::dsl;
        let q = dsl::groups.filter(dsl::id.eq_any(ids));
        Ok(q.load::<Group>(conn)?)
    }

    pub fn select_all(conn: &MysqlConnection) -> Result<Vec<Group>, Box<dyn Error>> {
        use crate::db::schema::groups::dsl;
        let q = dsl::groups;
        Ok(q.load::<Group>(conn)?)
    }

    pub fn insert(conn: &MysqlConnection, group_form: &GroupInsertForm) -> Result<(), Box<dyn Error>> {
        use crate::db::schema::groups::dsl;
        let q = diesel::insert_into(dsl::groups).values(group_form);
        q.execute(conn)?;
        Ok(())
    }

    pub fn insert_and_find(conn: &MysqlConnection, group_form: &GroupInsertForm) -> Result<Group, Box<dyn Error>> {
        Self::insert(conn, group_form)?;
        let group = Self::find(conn, group_form.id)?;
        Ok(group.unwrap())
    }

    pub fn update(conn: &MysqlConnection, id: &str, group_form: &GroupUpdateForm) -> Result<(), Box<dyn Error>> {
        use crate::db::schema::groups::dsl;
        let q = diesel::update(dsl::groups.find(id)).set(group_form);
        let n = q.execute(conn)?;
        assert_eq!(1, n);
        Ok(())
    }

    pub fn update_and_find(
        conn: &MysqlConnection,
        id: &str,
        group_form: &GroupUpdateForm,
    ) -> Result<Group, Box<dyn Error>> {
        Self::update(conn, id, group_form)?;
        let group = Self::find(conn, id)?;
        Ok(group.unwrap())
    }
}

pub struct MysqlStats;

impl MysqlStats {
    pub fn find(conn: &MysqlConnection, id: i32) -> Result<Option<Stat>, Box<dyn Error>> {
        use crate::db::schema::stats::dsl;
        let q = dsl::stats.find(id);
        Ok(q.first::<Stat>(conn).optional()?)
    }

    pub fn find_by_path(conn: &MysqlConnection, group_id: &str, path: &str) -> Result<Option<Stat>, Box<dyn Error>> {
        use crate::db::schema::stats::dsl;
        let q = dsl::stats.filter(dsl::group_id.eq(group_id)).filter(dsl::path.eq(path));
        Ok(q.first::<Stat>(conn).optional()?)
    }

    pub fn select_by_group_id(conn: &MysqlConnection, group_id: &str) -> Result<Vec<Stat>, Box<dyn Error>> {
        use crate::db::schema::stats::dsl;
        let q = dsl::stats.filter(dsl::group_id.eq(group_id)).order(dsl::group_id.asc()).order(dsl::path.asc());
        Ok(q.load::<Stat>(conn)?)
    }

    pub fn select_by_footprint_id(
        conn: &MysqlConnection,
        group_id: Option<&str>,
        footprint_id: i32,
    ) -> Result<Vec<Stat>, Box<dyn Error>> {
        use crate::db::schema::stats::dsl;
        if let Some(group_id) = group_id {
            let expr = dsl::footprint_id.eq(footprint_id).and(dsl::group_id.eq(group_id));
            let q = dsl::stats.filter(expr).order(dsl::group_id.asc()).order(dsl::path.asc());
            Ok(q.load::<Stat>(conn)?)
        } else {
            let expr = dsl::footprint_id.eq(footprint_id);
            let q = dsl::stats.filter(expr).order(dsl::group_id.asc()).order(dsl::path.asc());
            Ok(q.load::<Stat>(conn)?)
        }
    }

    pub fn insert(conn: &MysqlConnection, stat_form: &StatInsertForm) -> Result<(), Box<dyn Error>> {
        use crate::db::schema::stats::dsl;
        let q = diesel::insert_into(dsl::stats).values(stat_form);
        q.execute(conn)?;
        Ok(())
    }

    pub fn insert_and_find(conn: &MysqlConnection, stat_form: &StatInsertForm) -> Result<Stat, Box<dyn Error>> {
        Self::insert(conn, stat_form)?;
        let stat = Self::find_by_path(conn, stat_form.group_id, stat_form.path)?;
        Ok(stat.unwrap())
    }

    pub fn update(conn: &MysqlConnection, id: i32, stat_form: &StatUpdateForm) -> Result<(), Box<dyn Error>> {
        use crate::db::schema::stats::dsl;
        let q = diesel::update(dsl::stats.find(id)).set(stat_form);
        let n = q.execute(conn)?;
        assert_eq!(1, n);
        Ok(())
    }

    pub fn update_and_find(
        conn: &MysqlConnection,
        id: i32,
        stat_form: &StatUpdateForm,
    ) -> Result<Stat, Box<dyn Error>> {
        Self::update(conn, id, stat_form)?;
        let stat = Self::find(conn, id)?;
        Ok(stat.unwrap())
    }
}
