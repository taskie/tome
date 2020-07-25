#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate log;

use std::{collections::HashMap, env};

use actix_web::{middleware, web, App, Error, HttpResponse, HttpServer, Responder};
use diesel::{
    r2d2::{self, ConnectionManager},
    MysqlConnection,
};
use ichnome::{
    db::{MysqlFootprints, MysqlGroups, MysqlHistories, MysqlStats},
    Footprint, Group, History, Stat, META_NAMESPACE_ID,
};
use serde::Serialize;
use structopt::{clap, StructOpt};

type DbPool = r2d2::Pool<ConnectionManager<MysqlConnection>>;

#[derive(Serialize)]
struct GetStatsResponse {
    group: Group,
    stats: Vec<Stat>,
}

fn get_stats_impl(
    conn: &MysqlConnection,
    group_id: &str,
) -> Result<Option<GetStatsResponse>, Box<dyn std::error::Error>> {
    let group = MysqlGroups::find(conn, group_id)?;
    match group {
        Some(group) => {
            let stats = MysqlStats::select_by_group_id(conn, group_id)?;
            Ok(Some(GetStatsResponse { group, stats }))
        }
        None => Ok(None),
    }
}

#[get("/stats/{group_id}")]
async fn get_stats(pool: web::Data<DbPool>, path_params: web::Path<String>) -> Result<impl Responder, Error> {
    let group_id = path_params.into_inner();
    let group_id_2 = group_id.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_stats_impl(&conn, &group_id).map_err(|e| e.to_string())).await.map_err(|e| {
        error!("{}", e);
        HttpResponse::InternalServerError().finish()
    })?;
    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No group: {}", &group_id_2));
            Ok(res)
        }
    }
}

#[derive(Serialize)]
struct GetStatResponse {
    group: Group,
    stat: Stat,
    histories: Option<Vec<History>>,
    footprints: Option<HashMap<i32, Footprint>>,
    eq_stats: Option<Vec<Stat>>,
}

fn get_stat_impl(
    conn: &MysqlConnection,
    group_id: &str,
    path: &str,
) -> Result<Option<GetStatResponse>, Box<dyn std::error::Error>> {
    let group = MysqlGroups::find(conn, group_id).map_err(|e| e.to_string())?;
    if let Some(group) = group {
        let stat = MysqlStats::find_by_path(conn, group_id, path)?;
        if let Some(stat) = stat {
            let histories = Some(MysqlHistories::select_by_path(conn, group_id, path)?);
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
                    MysqlStats::select_by_footprint_id(conn, None, footprint_id)?
                } else {
                    vec![]
                }
            });
            Ok(Some(GetStatResponse { group, stat, histories, footprints, eq_stats }))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

#[get("/stats/{group_id}/{path:.*}")]
async fn get_stat(pool: web::Data<DbPool>, path_params: web::Path<(String, String)>) -> Result<impl Responder, Error> {
    let (group_id, path) = path_params.into_inner();
    let group_id_2 = group_id.clone();
    let path_2 = path.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp =
        web::block(move || get_stat_impl(&conn, &group_id, &path).map_err(|e| e.to_string())).await.map_err(|e| {
            error!("{}", e);
            HttpResponse::InternalServerError().finish()
        })?;

    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No stat: {}/{}", &group_id_2, &path_2));
            Ok(res)
        }
    }
}

#[derive(Serialize)]
struct GetFootprintResponse {
    footprint: Footprint,
    group_id: Option<String>,
    stats: Option<Vec<Stat>>,
    histories: Option<Vec<History>>,
}

fn get_footprint_impl(
    conn: &MysqlConnection,
    digest: &str,
) -> Result<Option<GetFootprintResponse>, Box<dyn std::error::Error>> {
    let footprint = MysqlFootprints::find_by_digest(conn, digest)?;
    let group_id: Option<String> = None;
    if let Some(footprint) = footprint {
        let stats =
            Some(MysqlStats::select_by_footprint_id(conn, group_id.as_ref().map(|s| s.as_str()), footprint.id)?);
        let histories =
            Some(MysqlHistories::select_by_footprint_id(conn, group_id.as_ref().map(|s| s.as_str()), footprint.id)?);
        Ok(Some(GetFootprintResponse { footprint, group_id, stats, histories }))
    } else {
        Ok(None)
    }
}

#[get("/footprints/{digest}")]
async fn get_footprint(pool: web::Data<DbPool>, path_params: web::Path<String>) -> Result<impl Responder, Error> {
    let digest = path_params.into_inner();
    let digest_2 = digest.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp =
        web::block(move || get_footprint_impl(&conn, &digest).map_err(|e| e.to_string())).await.map_err(|e| {
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

#[derive(Debug, StructOpt)]
#[structopt(name = "ichnome-web")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(short, long, default_value = "127.0.0.1:3024")]
    pub address: String,
}

#[derive(Serialize)]
struct GetGroupsResponse {
    groups: Vec<Group>,
}

fn get_groups_impl(conn: &MysqlConnection) -> Result<Option<GetGroupsResponse>, Box<dyn std::error::Error>> {
    let groups = MysqlGroups::select_all(conn)?;
    Ok(Some(GetGroupsResponse { groups }))
}

#[get("/groups")]
async fn get_groups(pool: web::Data<DbPool>) -> Result<impl Responder, Error> {
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_groups_impl(&conn).map_err(|e| e.to_string())).await.map_err(|e| {
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
    group: Group,
    stat: Option<Stat>,
    histories: Option<Vec<History>>,
    footprints: Option<HashMap<i32, Footprint>>,
}

fn get_group_impl(
    conn: &MysqlConnection,
    group_id: &str,
) -> Result<Option<GetGroupResponse>, Box<dyn std::error::Error>> {
    let group = MysqlGroups::find(conn, group_id)?;
    if let Some(group) = group {
        let stat = MysqlStats::find_by_path(conn, META_NAMESPACE_ID, group_id)?;
        let histories = Some(MysqlHistories::select_by_path(conn, META_NAMESPACE_ID, group_id)?);
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
        Ok(Some(GetGroupResponse { group, stat, histories, footprints }))
    } else {
        Ok(None)
    }
}

#[get("/groups/{group_id}")]
async fn get_group(pool: web::Data<DbPool>, path_params: web::Path<String>) -> Result<impl Responder, Error> {
    let group_id = path_params.into_inner();
    let group_id_2 = group_id.clone();
    let conn = pool.get().expect("couldn't get db connection from pool");

    let resp = web::block(move || get_group_impl(&conn, &group_id).map_err(|e| e.to_string())).await.map_err(|e| {
        error!("{}", e);
        HttpResponse::InternalServerError().finish()
    })?;
    match resp {
        Some(resp) => Ok(HttpResponse::Ok().json(&resp)),
        None => {
            let res = HttpResponse::NotFound().body(format!("No group: {}", &group_id_2));
            Ok(res)
        }
    }
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
