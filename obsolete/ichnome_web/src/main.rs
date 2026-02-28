use std::{
    collections::{HashMap, HashSet},
    env,
};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use deadpool_diesel::postgres::{Manager, Pool};
use ichnome::{
    db::Connection,
    db::{OmFootprints, OmGroups, OmHistories, OmStats, OmWorkspaces, StatOrder, StatSearchCondition},
    error::DomainError,
    Footprint, Group, History, Stat, Status, Workspace, META_GROUP_NAME,
};
use serde::{Deserialize, Serialize};
use structopt::{clap, StructOpt};
use tower_http::trace::TraceLayer;
use tracing::{debug, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::models::{WebFootprint, WebGroup, WebHistory, WebStat, WebWorkspace};

mod models;

fn find_workspace_and_group(
    conn: &mut Connection,
    workspace_name: &str,
    group_name: &str,
) -> Result<Option<(Workspace, Group)>, Box<dyn std::error::Error>> {
    let workspace = OmWorkspaces::find_by_name(conn, workspace_name)?;
    if let Some(workspace) = workspace {
        let group = OmGroups::find_by_name(conn, workspace.id, group_name)?;
        if let Some(group) = group {
            Ok(Some((workspace, group)))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

#[derive(Deserialize)]
struct GetStatsQuery {
    path_prefix: Option<String>,
    path_partial: Option<String>,
    status: Option<String>,
    mtime_after: Option<DateTime<Utc>>,
    mtime_before: Option<DateTime<Utc>>,
    updated_at_after: Option<DateTime<Utc>>,
    updated_at_before: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct GetStatsResponse {
    workspace: WebWorkspace,
    group: WebGroup,
    stats: Vec<WebStat>,
    stats_count: i32,
}

fn get_stats_impl(
    conn: &mut Connection,
    workspace_name: &str,
    group_name: &str,
    q: &GetStatsQuery,
) -> Result<Option<GetStatsResponse>, Box<dyn std::error::Error>> {
    let pair = find_workspace_and_group(conn, workspace_name, group_name)?;
    if let Some((workspace, group)) = pair {
        let status = q
            .status
            .as_ref()
            .map(|s| match s.to_ascii_lowercase().as_str() {
                "0" | "disabled" => Some(Status::Disabled),
                "1" | "enabled" => Some(Status::Enabled),
                _ => None,
            })
            .flatten()
            .unwrap_or(Status::Enabled);
        let count_cond = StatSearchCondition {
            group_ids: Some(vec![group.id]),
            path_prefix: q.path_prefix.as_ref().map(|s| s.as_ref()),
            path_partial: q.path_partial.as_ref().map(|s| s.as_ref()),
            statuses: Some(vec![status]),
            mtime_after: q.mtime_after,
            mtime_before: q.mtime_before,
            updated_at_after: q.updated_at_after,
            updated_at_before: q.updated_at_before,
            ..Default::default()
        };
        let stats_count = OmStats::count(conn, workspace.id, &count_cond)? as i32;
        let cond = StatSearchCondition {
            order: Some(StatOrder::UpdatedAtDesc),
            limit: Some(q.limit.unwrap_or(100)),
            ..count_cond
        };
        debug!("search condition: {:?}", &cond);
        let stats = OmStats::search(conn, workspace.id, &cond)?;
        Ok(Some(GetStatsResponse {
            workspace: WebWorkspace::from(&workspace),
            group: WebGroup::from(&workspace, &group),
            stats: stats.iter().map(|s| WebStat::from(&workspace, &group, &s)).collect(),
            stats_count,
        }))
    } else {
        Ok(None)
    }
}

async fn get_stats(
    State(AppState { pool }): State<AppState>,
    Path(path_params): Path<(String, String)>,
    Query(q): Query<GetStatsQuery>,
) -> Result<Json<GetStatsResponse>, AppError> {
    let (workspace_name, group_name) = path_params;
    let group_name_2 = group_name.clone();
    let conn = pool.get().await.expect("couldn't get db connection from pool");

    conn.interact(move |conn| get_stats_impl(conn, &workspace_name, &group_name, &q).map_err(|e| e.to_string()))
        .await
        .map_err(internal_server_error)?
        .map_err(|e| AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or_else(|| AppError::Simple(StatusCode::NOT_FOUND, format!("No group: {}", &group_name_2)))
}

#[derive(Serialize)]
struct GetStatResponse {
    workspace: WebWorkspace,
    group: WebGroup,
    stat: WebStat,
    histories: Option<Vec<WebHistory>>,
    footprints: Option<HashMap<String, WebFootprint>>,
    eq_stats: Option<Vec<WebStat>>,
}

fn to_web_stats(workspace: &Workspace, group_map: &HashMap<i64, &Group>, stats: &Vec<Stat>) -> Vec<WebStat> {
    stats
        .iter()
        .map(|s| (s, group_map.get(&s.group_id)))
        .filter(|(_, g)| g.is_some())
        .map(|(s, g)| WebStat::from(&workspace, *g.unwrap(), s))
        .collect()
}

fn to_web_histories(workspace: &Workspace, group_map: &HashMap<i64, &Group>, stats: &Vec<History>) -> Vec<WebHistory> {
    stats
        .iter()
        .map(|h| (h, group_map.get(&h.group_id)))
        .filter(|(_, g)| g.is_some())
        .map(|(h, g)| WebHistory::from(&workspace, *g.unwrap(), h))
        .collect()
}

fn get_stat_impl(
    conn: &mut Connection,
    workspace_name: &str,
    group_name: &str,
    path: &str,
) -> Result<Option<GetStatResponse>, Box<dyn std::error::Error>> {
    let pair = find_workspace_and_group(conn, workspace_name, group_name)?;
    if let Some((workspace, group)) = pair {
        let stat = OmStats::find_by_path(conn, group.id, path)?;
        if let Some(stat) = stat {
            let histories = Some(OmHistories::select_by_path(conn, group.id, path)?);
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
                let footprint_list = OmFootprints::select(conn, &footprint_ids)?;
                let mut footprints = HashMap::new();
                for footprint in footprint_list {
                    footprints.insert(footprint.id, footprint);
                }
                footprints
            });
            let eq_stats = Some({
                if let Some(footprint_id) = stat.footprint_id {
                    let stats = OmStats::select_by_footprint_id(conn, group.workspace_id, footprint_id)?;
                    let group_ids: Vec<i64> = stats.iter().map(|s| s.group_id).collect();
                    let groups = OmGroups::select(conn, &group_ids)?;
                    let group_map = groups.iter().map(|g| (g.id, g)).collect();
                    to_web_stats(&workspace, &group_map, &stats)
                } else {
                    vec![]
                }
            });
            Ok(Some(GetStatResponse {
                workspace: WebWorkspace::from(&workspace),
                group: WebGroup::from(&workspace, &group),
                stat: WebStat::from(&workspace, &group, &stat),
                histories: histories.map(|h| h.iter().map(|h| WebHistory::from(&workspace, &group, h)).collect()),
                footprints: footprints
                    .map(|f| f.into_iter().map(|(k, v)| (k.to_string(), WebFootprint::from(&v))).collect()),
                eq_stats,
            }))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

async fn get_stat(
    State(AppState { pool }): State<AppState>,
    Path(path_params): Path<(String, String, String)>,
) -> Result<Json<GetStatResponse>, AppError> {
    let (workspace_name, group_name, path) = path_params;
    let group_name_2 = group_name.clone();
    let path_2 = path.clone();
    let conn = pool.get().await.expect("couldn't get db connection from pool");

    conn.interact(move |conn| get_stat_impl(conn, &workspace_name, &group_name, &path).map_err(|e| e.to_string()))
        .await
        .map_err(internal_server_error)?
        .map_err(|e| AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or_else(|| AppError::Simple(StatusCode::NOT_FOUND, format!("No stat: {}/{}", &group_name_2, &path_2)))
}

#[derive(Serialize)]
struct GetFootprintResponse {
    workspace: WebWorkspace,
    footprint: WebFootprint,
    group_name: Option<String>,
    stats: Option<Vec<WebStat>>,
    histories: Option<Vec<WebHistory>>,
}

fn get_footprint_impl(
    conn: &mut Connection,
    workspace_name: &str,
    digest: &str,
) -> Result<Option<GetFootprintResponse>, Box<dyn std::error::Error>> {
    let workspace = OmWorkspaces::find_by_name(conn, workspace_name)?;
    let workspace = if let Some(workspace) = workspace {
        workspace
    } else {
        return Ok(None);
    };
    let digest = treblo::hex::from_hex_string(digest).unwrap();
    let footprint = OmFootprints::find_by_digest(conn, &digest)?;
    let group_name: Option<String> = None;
    if let Some(footprint) = footprint {
        let mut group_ids = HashSet::new();
        let stats = OmStats::select_by_footprint_id(conn, workspace.id, footprint.id)?;
        let histories = OmHistories::select_by_footprint_id(conn, workspace.id, footprint.id)?;
        if stats.is_empty() && histories.is_empty() {
            return Ok(None);
        }
        for s in stats.iter() {
            group_ids.insert(s.group_id);
        }
        for h in histories.iter() {
            group_ids.insert(h.group_id);
        }
        let group_ids: Vec<i64> = group_ids.into_iter().collect();
        let groups = OmGroups::select(conn, &group_ids)?;
        let group_map = groups.iter().map(|g| (g.id, g)).collect();
        let stats = to_web_stats(&workspace, &group_map, &stats);
        let histories = to_web_histories(&workspace, &group_map, &histories);
        Ok(Some(GetFootprintResponse {
            workspace: WebWorkspace::from(&workspace),
            footprint: WebFootprint::from(&footprint),
            group_name,
            stats: Some(stats),
            histories: Some(histories),
        }))
    } else {
        Ok(None)
    }
}

async fn get_footprint(
    State(AppState { pool }): State<AppState>,
    Path(path_params): Path<(String, String)>,
) -> Result<Json<GetFootprintResponse>, AppError> {
    let (workspace_name, digest) = path_params;
    let digest_2 = digest.clone();
    let conn = pool.get().await.expect("couldn't get db connection from pool");

    conn.interact(move |conn| get_footprint_impl(conn, &workspace_name, &digest).map_err(|e| e.to_string()))
        .await
        .map_err(internal_server_error)?
        .map_err(|e| AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or_else(|| AppError::Simple(StatusCode::NOT_FOUND, format!("No footprint: {}", digest_2)))
}

#[derive(Serialize)]
struct GetGroupsResponse {
    workspace: WebWorkspace,
    groups: Vec<WebGroup>,
}

fn get_groups_impl(
    conn: &mut Connection,
    workspace_name: &str,
) -> Result<Option<GetGroupsResponse>, Box<dyn std::error::Error>> {
    let workspace = OmWorkspaces::find_by_name(conn, workspace_name)?;
    if let Some(workspace) = workspace {
        let groups = OmGroups::select_all(conn, workspace.id)?;
        Ok(Some(GetGroupsResponse {
            workspace: WebWorkspace::from(&workspace),
            groups: groups.iter().map(|g| WebGroup::from(&workspace, g)).collect(),
        }))
    } else {
        Ok(None)
    }
}

async fn get_groups(
    State(AppState { pool }): State<AppState>,
    Path(path_params): Path<(String,)>,
) -> Result<Json<GetGroupsResponse>, AppError> {
    let (workspace_name,) = path_params;
    let conn = pool.get().await.expect("couldn't get db connection from pool");

    conn.interact(move |conn| get_groups_impl(conn, &workspace_name).map_err(|e| e.to_string()))
        .await
        .map_err(internal_server_error)?
        .map_err(|e| AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or_else(|| AppError::Simple(StatusCode::NOT_FOUND, "No groups".to_string()))
}

#[derive(Serialize)]
struct GetGroupResponse {
    workspace: WebWorkspace,
    group: WebGroup,
    stat: Option<WebStat>,
    histories: Option<Vec<WebHistory>>,
    footprints: Option<HashMap<String, WebFootprint>>,
}

fn get_group_impl(
    conn: &mut Connection,
    workspace_name: &str,
    group_name: &str,
) -> Result<Option<GetGroupResponse>, Box<dyn std::error::Error>> {
    let pair = find_workspace_and_group(conn, workspace_name, group_name)?;
    if let Some((workspace, group)) = pair {
        let meta_group = OmGroups::find_by_name(conn, group.workspace_id, META_GROUP_NAME)?;
        let meta_group = if let Some(meta_group) = meta_group { meta_group } else { return Ok(None) };
        let stat = OmStats::find_by_path(conn, meta_group.id, group_name)?;
        let histories = Some(OmHistories::select_by_path(conn, meta_group.id, group_name)?);
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
            let footprint_list = OmFootprints::select(conn, &footprint_ids)?;
            let mut footprints = HashMap::new();
            for footprint in footprint_list {
                footprints.insert(footprint.id, footprint);
            }
            footprints
        });
        Ok(Some(GetGroupResponse {
            workspace: WebWorkspace::from(&workspace),
            group: WebGroup::from(&workspace, &group),
            stat: stat.map(|s| WebStat::from(&workspace, &group, &s)),
            histories: histories.map(|h| h.iter().map(|h| WebHistory::from(&workspace, &group, h)).collect()),
            footprints: footprints
                .map(|f| f.into_iter().map(|(k, v)| (k.to_string(), WebFootprint::from(&v))).collect()),
        }))
    } else {
        Ok(None)
    }
}

async fn get_group(
    State(AppState { pool }): State<AppState>,
    Path(path_params): Path<(String, String)>,
) -> Result<Json<GetGroupResponse>, AppError> {
    let (workspace_name, group_name) = path_params;
    let group_name_2 = group_name.clone();
    let conn = pool.get().await.expect("couldn't get db connection from pool");

    conn.interact(move |conn| get_group_impl(conn, &workspace_name, &group_name).map_err(|e| e.to_string()))
        .await
        .map_err(internal_server_error)?
        .map_err(|e| AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or_else(|| AppError::Simple(StatusCode::NOT_FOUND, format!("No group: {}", &group_name_2)))
}

#[derive(Deserialize)]
struct GetDiffQuery {
    group_name1: String,
    path_prefix1: String,
    group_name2: String,
    path_prefix2: String,
}

#[derive(Serialize)]
struct GetDiffResponse {
    workspace: WebWorkspace,
    group1: WebGroup,
    group2: WebGroup,
    diff: HashMap<String, (Vec<String>, Vec<String>)>,
    stats: HashMap<String, WebStat>,
    footprints: HashMap<String, WebFootprint>,
}

fn get_diff_impl_search_stats(
    conn: &mut Connection,
    workspace: &Workspace,
    group_name: &str,
    path_prefix: &str,
) -> Result<Option<(Group, Vec<Stat>)>, Box<dyn std::error::Error>> {
    let group = OmGroups::find_by_name(conn, workspace.id, &group_name)?;
    let group = if let Some(group) = group { group } else { return Ok(None) };
    let cond =
        StatSearchCondition { group_ids: Some(vec![group.id]), path_prefix: Some(path_prefix), ..Default::default() };
    let stats_count = OmStats::count(conn, workspace.id, &cond)?;
    if stats_count > 1000 {
        return Err(Box::new(DomainError::params("path_prefix", format!("too many stats: {}", stats_count))));
    };
    let stats = OmStats::search(conn, workspace.id, &cond)?;
    Ok(Some((group, stats)))
}

fn get_diff_impl(
    conn: &mut Connection,
    workspace_name: &str,
    q: &GetDiffQuery,
) -> Result<Option<GetDiffResponse>, Box<dyn std::error::Error>> {
    let workspace = OmWorkspaces::find_by_name(conn, workspace_name)?;
    if let Some(workspace) = workspace {
        let result1 = get_diff_impl_search_stats(conn, &workspace, &q.group_name1, &q.path_prefix1)?;
        let (group1, stats1) = if let Some(x) = result1 { x } else { return Ok(None) };
        let result2 = get_diff_impl_search_stats(conn, &workspace, &q.group_name2, &q.path_prefix2)?;
        let (group2, stats2) = if let Some(x) = result2 { x } else { return Ok(None) };
        let stats: HashMap<i64, Stat> = stats1.iter().chain(stats2.iter()).map(|s| (s.id, s.clone())).collect();
        let mut diff = HashMap::<i64, (Vec<i64>, Vec<i64>)>::new();
        for stat1 in stats1.iter() {
            if let Some(footprint_id) = stat1.footprint_id {
                let v = diff.entry(footprint_id).or_insert_with(|| (vec![], vec![]));
                v.0.push(stat1.id);
            }
        }
        for stat2 in stats2.iter() {
            if let Some(footprint_id) = stat2.footprint_id {
                let v = diff.entry(footprint_id).or_insert_with(|| (vec![], vec![]));
                v.1.push(stat2.id);
            }
        }
        let footprints: HashMap<i64, Footprint> =
            OmFootprints::select(conn, &diff.keys().map(|i| *i).collect())?.into_iter().map(|f| (f.id, f)).collect();
        Ok(Some(GetDiffResponse {
            workspace: WebWorkspace::from(&workspace),
            group1: WebGroup::from(&workspace, &group1),
            group2: WebGroup::from(&workspace, &group2),
            diff: diff
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        (v.0.iter().map(|i| i.to_string()).collect(), v.1.iter().map(|i| i.to_string()).collect()),
                    )
                })
                .collect(),
            stats: stats
                .iter()
                .map(|(k, v)| {
                    let group = if v.group_id == group2.id { &group2 } else { &group1 };
                    (k.to_string(), WebStat::from(&workspace, group, v))
                })
                .collect(),
            footprints: footprints.iter().map(|(k, v)| (k.to_string(), WebFootprint::from(&v))).collect(),
        }))
    } else {
        Ok(None)
    }
}

async fn get_diff(
    State(AppState { pool }): State<AppState>,
    Path(path_params): Path<(String,)>,
    Query(q): Query<GetDiffQuery>,
) -> Result<Json<GetDiffResponse>, AppError> {
    let (workspace_name,) = path_params;
    let conn = pool.get().await.expect("couldn't get db connection from pool");

    conn.interact(move |conn| get_diff_impl(conn, &workspace_name, &q).map_err(|e| e.to_string()))
        .await
        .map_err(internal_server_error)?
        .map_err(|e| AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .map(Json)
        .ok_or_else(|| AppError::Simple(StatusCode::NOT_FOUND, "Not found".to_string()))
}

async fn handler_404() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "The requested resource was not found")
}

#[derive(Debug, StructOpt)]
#[structopt(name = "ichnome-web")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(short, long, default_value = "127.0.0.1:3024")]
    pub address: String,
}

#[derive(Clone)]
pub struct AppState {
    pool: Pool,
}

enum AppError {
    Simple(StatusCode, String),
    Any(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Simple(status, body) => (status, body).into_response(),
            AppError::Any(err) => {
                error!("Internal server error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::Any(err.into())
    }
}

fn internal_server_error(e: impl std::error::Error) -> AppError {
    error!("Internal server error: {}", e);
    AppError::Simple(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ichnome_web=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let opt = Opt::from_args();

    let database_url = env::var("ICHNOME_DATABASE_URL").unwrap();
    let manager = Manager::new(database_url, deadpool_diesel::Runtime::Tokio1);
    let pool = Pool::builder(manager).build()?;

    let state = AppState { pool };

    let router = Router::new()
        .route("/:workspace_name/stats/:group_name", get(get_stats))
        .route("/:workspace_name/stats/:group_name/*path", get(get_stat))
        .route("/:workspace_name/footprints/:digest", get(get_footprint))
        .route("/:workspace_name/groups", get(get_groups))
        .route("/:workspace_name/groups/:group_name", get(get_group))
        .route("/:workspace_name/diff", get(get_diff))
        .fallback(handler_404)
        .layer(TraceLayer::new_for_http());
    let app = router.with_state(state);

    let addr = opt.address.parse()?;
    axum::Server::bind(&addr).serve(app.into_make_service()).await?;

    Ok(())
}
