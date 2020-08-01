#[macro_use]
extern crate log;

use std::{collections::HashSet, env, error::Error, ffi::OsStr, path::Path, process::exit};

use chrono::Utc;
use diesel::{connection::Connection, sqlite::SqliteConnection};
use dotenv;
use ichno::{actions, db::SqliteStats, DEFAULT_GROUP_NAME, DEFAULT_WORKSPACE_NAME};
use ignore;
use itertools::Itertools;
use structopt::{clap, StructOpt};
use twox_hash::RandomXxHashBuilder64;

#[derive(Debug, StructOpt)]
#[structopt(name = "ichno")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(subcommand)]
    sub: SubCommands,
}

#[derive(Debug, StructOpt)]
pub enum SubCommands {
    Scan(Scan),
}

#[derive(Debug, StructOpt)]
pub struct Scan {
    #[structopt(long, default_value = "100", name = "N")]
    pub commit_interval: usize,
}

fn main_with_error() -> Result<i32, Box<dyn Error>> {
    dotenv::dotenv().ok();
    env_logger::init();

    let database_url = env::var("DATABASE_URL").unwrap_or("ichno.db".to_owned());
    let conn = SqliteConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url));
    let db_path = Path::new(&database_url).canonicalize()?;

    ichno::db::migrate(&conn)?;

    let workspace_name = DEFAULT_WORKSPACE_NAME;
    let group_name = DEFAULT_GROUP_NAME;
    let opt = Opt::from_args();
    match opt.sub {
        SubCommands::Scan(scan) => {
            let mut ctx = actions::Context {
                connection: &conn,
                db_path: &db_path,
                workspace_name,
                workspace: None,
                group_name,
                group: None,
                timer: Box::new(|| Utc::now()),
            };
            let path = Path::new(".");
            let w = {
                let mut wb = ignore::WalkBuilder::new(&path);
                wb.filter_entry(|p| p.file_name() != OsStr::new(".git") && p.file_name() != OsStr::new("ichno.db"));
                wb.build()
            };
            actions::pre_process(&mut ctx)?;
            let group_id = ctx.group.as_ref().unwrap().id;
            let mut path_set: HashSet<_, RandomXxHashBuilder64> = Default::default();
            let commit_interval = scan.commit_interval;
            for result_chunk in &w.chunks(commit_interval) {
                conn.transaction::<_, Box<dyn Error>, _>(|| {
                    for result in result_chunk {
                        match result {
                            Ok(entry) => {
                                if entry.metadata().unwrap().is_file() {
                                    match actions::update_file_stat(&ctx, entry.path()) {
                                        Ok(Some(stat)) => {
                                            path_set.insert(stat.path);
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            warn!("{}", e);
                                        }
                                    }
                                }
                            }
                            Err(err) => warn!("{}", err),
                        }
                    }
                    Ok(())
                })?;
            }
            let stats = SqliteStats::select_by_group_id(&conn, group_id)?;
            for stat_chunk in &stats.iter().chunks(commit_interval) {
                conn.transaction::<_, Box<dyn Error>, _>(|| {
                    for stat in stat_chunk {
                        if path_set.contains(&stat.path) {
                            continue;
                        }
                        let path = Path::new(&stat.path);
                        if !path.exists() {
                            actions::update_file_stat(&ctx, path)?;
                        }
                    }
                    Ok(())
                })?;
            }
            actions::post_process(&mut ctx)?;
        }
    };
    Ok(0)
}

fn main() {
    match main_with_error() {
        Ok(code) => exit(code),
        Err(e) => error!("{}", e),
    }
}
