#[macro_use]
extern crate log;

use std::{collections::HashSet, env, error::Error, ffi::OsStr, path::Path, process::exit};

use chrono::Local;
use diesel::{connection::Connection, sqlite::SqliteConnection};
use dotenv;
use ignore;
use structopt::{clap, StructOpt};
use twox_hash::RandomXxHashBuilder64;

use ichno::{db::SqliteStats, file, DEFAULT_NAMESPACE_ID};

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
    // nop
}

fn main_with_error() -> Result<i32, Box<dyn Error>> {
    dotenv::dotenv().ok();
    env_logger::init();

    let database_url = env::var("DATABASE_URL").unwrap_or("ichno.db".to_owned());
    let conn = SqliteConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url));
    let db_path = Path::new(&database_url).canonicalize()?;

    ichno::db::migrate(&conn)?;

    let group_id = DEFAULT_NAMESPACE_ID;
    let opt = Opt::from_args();
    match opt.sub {
        SubCommands::Scan(_) => {
            let mut ctx = file::Context {
                connection: &conn,
                db_path: &db_path,
                group_id,
                group: None,
                current_time: Local::now(),
            };
            let path = Path::new(".");
            let w = {
                let mut wb = ignore::WalkBuilder::new(&path);
                wb.filter_entry(|p| p.file_name() != OsStr::new(".git") && p.file_name() != OsStr::new("ichno.db"));
                wb.build()
            };
            file::pre_process(&mut ctx)?;
            let mut path_set: HashSet<_, RandomXxHashBuilder64> = Default::default();
            for result in w {
                match result {
                    Ok(entry) => {
                        if entry.metadata().unwrap().is_file() {
                            let path = file::upsert_with_file(&ctx, group_id, entry.path())?.path;
                            path_set.insert(path);
                        }
                    }
                    Err(err) => warn!("{}", err),
                }
            }
            let stats = SqliteStats::select_by_group_id(&conn, group_id)?;
            for stat in stats.iter() {
                if path_set.contains(&stat.path) {
                    continue;
                }
                let path = Path::new(&stat.path);
                if !path.exists() {
                    file::remove_with_file(&ctx, group_id, path)?;
                }
            }
            file::post_process(&mut ctx)?;
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
