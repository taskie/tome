#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate log;

use std::{collections::HashMap, env};

use crate::models::{WebHistory, WebStat};
use actix_web::{middleware, web, App, Error, HttpResponse, HttpServer, Responder};
use diesel::{
    r2d2::{self, ConnectionManager},
    MysqlConnection,
};
use ichnome::{
    db::{MysqlFootprints, MysqlGroups, MysqlHistories, MysqlStats, MysqlWorkspaces},
    Footprint, Group, History, Stat, Workspace, META_GROUP_NAME,
};
use serde::Serialize;
use std::collections::HashSet;
use structopt::{clap, StructOpt};

type DbPool = r2d2::Pool<ConnectionManager<MysqlConnection>>;

mod models;

fn find_workspace_and_group(
    conn: &MysqlConnection,
    workspace_name: &str,
    group_name: &str,
) -> Result<Option<(Workspace, Group)>, Box<dyn std::error::Error>> {
    let workspace = MysqlWorkspaces::find_by_name(conn, workspace_name)?;
    if let Some(workspace) = workspace {
        let group = MysqlGroups::find_by_name(conn, workspace.id, group_name)?;
        if let Some(group) = group {
            Ok(Some((workspace, group)))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

#[derive(Serialize)]
struct GetStatsResponse {
    workspace: Workspace,
    group: Group,
    stats: Vec<Stat>,
}

fn get_stats_impl(
    conn: &MysqlConnection,
    workspace_name: &str,
    group_name: &str,
) -> Result<Option<GetStatsResponse>, Box<dyn std::error::Error>> {
    let pair = find_workspace_and_group(conn, workspace_name, group_name)?;
    if let Some((workspace, group)) = pair {
        let stats = MysqlStats::select_by_group_id(conn, group.id)?;
        Ok(Some(GetStatsResponse { workspace, group, stats }))
    } else {
        Ok(None)
    }
}

#[get("/{workspace_name}/stats/{group_name}")]
async fn get_stats(pool: web::Data<DbPool>, path_params: web::Path<(String, String)>) -> Result<impl Responder, Error> {
    let (workspace_name, group_name) = path_params.into_inner();
    let group_name_2 = group_name.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_stats_impl(&conn, &workspace_name, &group_name).map_err(|e| e.to_string()))
        .await
        .map_err(|e| {
            error!("{}", e);
            HttpResponse::InternalServerError().finish()
        })?;
    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No group: {}", &group_name_2));
            Ok(res)
        }
    }
}

#[derive(Serialize)]
struct GetStatResponse {
    workspace: Workspace,
    group: Group,
    stat: Stat,
    histories: Option<Vec<History>>,
    footprints: Option<HashMap<i32, Footprint>>,
    eq_stats: Option<Vec<Stat>>,
}

fn get_stat_impl(
    conn: &MysqlConnection,
    workspace_name: &str,
    group_name: &str,
    path: &str,
) -> Result<Option<GetStatResponse>, Box<dyn std::error::Error>> {
    let pair = find_workspace_and_group(conn, workspace_name, group_name)?;
    if let Some((workspace, group)) = pair {
        let stat = MysqlStats::find_by_path(conn, group.id, path)?;
        if let Some(stat) = stat {
            let histories = Some(MysqlHistories::select_by_path(conn, group.id, path)?);
            let footprints = Some({
                let mut footprint_ids = vec![];
                if let Some(footprint_id) = stat.footprint_id {
                    footprint_ids.push(footprint_id)
                }
                if let Some(histories) = histories.as_ref() {
                    for history in histories.iter() {
                        if let Some(footprint_id) = history.footprint_id {
                            footprint_ids.push(footprint_id);
                        }
                    }
                }
                let footprint_list = MysqlFootprints::select(conn, &footprint_ids)?;
                let mut footprints = HashMap::new();
                for footprint in footprint_list {
                    footprints.insert(footprint.id, footprint);
                }
                footprints
            });
            let eq_stats = Some({
                if let Some(footprint_id) = stat.footprint_id {
                    MysqlStats::select_by_footprint_id(conn, group.workspace_id, footprint_id)?
                } else {
                    vec![]
                }
            });
            Ok(Some(GetStatResponse { workspace, group, stat, histories, footprints, eq_stats }))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

#[get("/{workspace_name}/stats/{group_name}/{path:.*}")]
async fn get_stat(
    pool: web::Data<DbPool>,
    path_params: web::Path<(String, String, String)>,
) -> Result<impl Responder, Error> {
    let (workspace_name, group_name, path) = path_params.into_inner();
    let group_name_2 = group_name.clone();
    let path_2 = path.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_stat_impl(&conn, &workspace_name, &group_name, &path).map_err(|e| e.to_string()))
        .await
        .map_err(|e| {
            error!("{}", e);
            HttpResponse::InternalServerError().finish()
        })?;

    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No stat: {}/{}", &group_name_2, &path_2));
            Ok(res)
        }
    }
}

#[derive(Serialize)]
struct GetFootprintResponse {
    workspace: Workspace,
    footprint: Footprint,
    group_name: Option<String>,
    stats: Option<Vec<WebStat>>,
    histories: Option<Vec<WebHistory>>,
}

fn get_footprint_impl(
    conn: &MysqlConnection,
    workspace_name: &str,
    digest: &str,
) -> Result<Option<GetFootprintResponse>, Box<dyn std::error::Error>> {
    let workspace = MysqlWorkspaces::find_by_name(conn, workspace_name)?;
    let workspace = if let Some(workspace) = workspace {
        workspace
    } else {
        return Ok(None);
    };
    let footprint = MysqlFootprints::find_by_digest(conn, digest)?;
    let group_name: Option<String> = None;
    if let Some(footprint) = footprint {
        let mut group_ids = HashSet::new();
        let stats = MysqlStats::select_by_footprint_id(conn, workspace.id, footprint.id)?;
        let histories = MysqlHistories::select_by_footprint_id(conn, workspace.id, footprint.id)?;
        if stats.is_empty() && histories.is_empty() {
            return Ok(None);
        }
        for s in stats.iter() {
            group_ids.insert(s.group_id);
        }
        for h in histories.iter() {
            group_ids.insert(h.group_id);
        }
        let group_ids: Vec<i32> = group_ids.into_iter().collect();
        let groups = MysqlGroups::select(conn, &group_ids)?;
        let mut group_map = HashMap::new();
        for g in groups.iter() {
            group_map.insert(g.id, g);
        }
        let stats: Vec<WebStat> = stats
            .iter()
            .map(|s| (s, group_map.get(&s.group_id)))
            .filter(|(_, g)| g.is_some())
            .map(|(s, g)| WebStat::from(&workspace, *g.unwrap(), s))
            .collect();
        let histories: Vec<WebHistory> = histories
            .iter()
            .map(|h| (h, group_map.get(&h.group_id)))
            .filter(|(_, g)| g.is_some())
            .map(|(h, g)| WebHistory::from(&workspace, *g.unwrap(), h))
            .collect();
        Ok(Some(GetFootprintResponse {
            workspace,
            footprint,
            group_name,
            stats: Some(stats),
            histories: Some(histories),
        }))
    } else {
        Ok(None)
    }
}

#[get("/{workspace_name}/footprints/{digest}")]
async fn get_footprint(
    pool: web::Data<DbPool>,
    path_params: web::Path<(String, String)>,
) -> Result<impl Responder, Error> {
    let (workspace_name, digest) = path_params.into_inner();
    let digest_2 = digest.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_footprint_impl(&conn, &workspace_name, &digest).map_err(|e| e.to_string()))
        .await
        .map_err(|e| {
            error!("{}", e);
            HttpResponse::InternalServerError().finish()
        })?;

    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No footprint: {}", digest_2));
            Ok(res)
        }
    }
}

#[derive(Serialize)]
struct GetGroupsResponse {
    workspace: Workspace,
    groups: Vec<Group>,
}

fn get_groups_impl(
    conn: &MysqlConnection,
    workspace_name: &str,
) -> Result<Option<GetGroupsResponse>, Box<dyn std::error::Error>> {
    let workspace = MysqlWorkspaces::find_by_name(conn, workspace_name)?;
    if let Some(workspace) = workspace {
        let groups = MysqlGroups::select_all(conn, workspace.id)?;
        Ok(Some(GetGroupsResponse { workspace, groups }))
    } else {
        Ok(None)
    }
}

#[get("/{workspace_name}/groups")]
async fn get_groups(pool: web::Data<DbPool>, path_params: web::Path<(String,)>) -> Result<impl Responder, Error> {
    let (workspace_name,) = path_params.into_inner();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp =
        web::block(move || get_groups_impl(&conn, &workspace_name).map_err(|e| e.to_string())).await.map_err(|e| {
            error!("{}", e);
            HttpResponse::InternalServerError().finish()
        })?;
    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body("No groups");
            Ok(res)
        }
    }
}

#[derive(Serialize)]
struct GetGroupResponse {
    workspace: Workspace,
    group: Group,
    stat: Option<Stat>,
    histories: Option<Vec<History>>,
    footprints: Option<HashMap<i32, Footprint>>,
}

fn get_group_impl(
    conn: &MysqlConnection,
    workspace_name: &str,
    group_name: &str,
) -> Result<Option<GetGroupResponse>, Box<dyn std::error::Error>> {
    let pair = find_workspace_and_group(conn, workspace_name, group_name)?;
    if let Some((workspace, group)) = pair {
        let meta_group = MysqlGroups::find_by_name(conn, group.workspace_id, META_GROUP_NAME)?;
        let meta_group = if let Some(meta_group) = meta_group { meta_group } else { return Ok(None) };
        let stat = MysqlStats::find_by_path(conn, meta_group.id, group_name)?;
        let histories = Some(MysqlHistories::select_by_path(conn, meta_group.id, group_name)?);
        let footprints = Some({
            let mut footprint_ids = vec![];
            if let Some(stat) = stat.as_ref() {
                if let Some(footprint_id) = stat.footprint_id {
                    footprint_ids.push(footprint_id)
                }
            }
            if let Some(histories) = histories.as_ref() {
                for history in histories.iter() {
                    if let Some(footprint_id) = history.footprint_id {
                        footprint_ids.push(footprint_id);
                    }
                }
            }
            let footprint_list = MysqlFootprints::select(conn, &footprint_ids)?;
            let mut footprints = HashMap::new();
            for footprint in footprint_list {
                footprints.insert(footprint.id, footprint);
            }
            footprints
        });
        Ok(Some(GetGroupResponse { workspace, group, stat, histories, footprints }))
    } else {
        Ok(None)
    }
}

#[get("/{workspace_name}/groups/{group_name}")]
async fn get_group(pool: web::Data<DbPool>, path_params: web::Path<(String, String)>) -> Result<impl Responder, Error> {
    let (workspace_name, group_name) = path_params.into_inner();
    let group_name_2 = group_name.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_group_impl(&conn, &workspace_name, &group_name).map_err(|e| e.to_string()))
        .await
        .map_err(|e| {
            error!("{}", e);
            HttpResponse::InternalServerError().finish()
        })?;
    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No group: {}", &group_name_2));
            Ok(res)
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "ichnome-web")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(short, long, default_value = "127.0.0.1:3024")]
    pub address: String,
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    dotenv::dotenv().ok();
    let opt = Opt::from_args();

    let database_url = env::var("DATABASE_URL").unwrap();
    let manager = ConnectionManager::<MysqlConnection>::new(database_url);
    let pool = r2d2::Pool::builder().build(manager).expect("Failed to create pool.");

    HttpServer::new(move || {
        App::new()
            .data(pool.clone())
            .wrap(middleware::Logger::default())
            .service(get_stats)
            .service(get_stat)
            .service(get_footprint)
            .service(get_groups)
            .service(get_group)
    })
    .bind(&opt.address)?
    .run()
    .await
}
