#[macro_use]
extern crate log;

use std::{env, error::Error, process::exit};

use chrono::Utc;
use diesel::{Connection, MysqlConnection};

use ichnome::{
    action,
    action::{PullOptions, PullRequest, RegisterOptions, RegisterRequest, SetupOptions, SetupRequest},
};
use structopt::{clap, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(name = "ichnome")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(subcommand)]
    sub: SubCommands,

    #[structopt(short, long)]
    pub workspace: Option<String>,
}

#[derive(Debug, StructOpt)]
pub enum SubCommands {
    Setup(Setup),
    Register(Register),
    Pull(Pull),
}

#[derive(Debug, StructOpt)]
pub struct Setup {
    #[structopt(short, long)]
    pub description: Option<String>,

    #[structopt(short, long)]
    pub force: bool,
}

#[derive(Debug, StructOpt)]
pub struct Register {
    #[structopt(name = "GROUP")]
    pub group_name: String,

    #[structopt(name = "URL")]
    pub url: String,

    #[structopt(short, long)]
    pub description: Option<String>,

    #[structopt(short, long)]
    pub force: bool,
}

#[derive(Debug, StructOpt)]
pub struct Pull {
    #[structopt(name = "GROUP")]
    pub group_name: String,
}

fn main_with_error() -> Result<i32, Box<dyn Error>> {
    dotenv::dotenv().ok();
    env_logger::init();
    let database_url = env::var("DATABASE_URL").unwrap_or("ichno.db".to_owned());
    let conn = MysqlConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url));
    let ctx = action::Context { connection: &conn, timer: Box::new(|| Utc::now()) };
    let opt = Opt::from_args();
    let workspace_name = opt.workspace.or_else(|| env::var("ICHNOME_WORKSPACE").ok()).unwrap();
    match opt.sub {
        SubCommands::Setup(setup) => {
            action::setup(
                &ctx,
                &SetupRequest {
                    workspace_name,
                    options: SetupOptions { force: setup.force, description: setup.description },
                },
            )?;
        }
        SubCommands::Register(register) => {
            action::register(
                &ctx,
                &RegisterRequest {
                    workspace_name,
                    group_name: register.group_name,
                    url: register.url,
                    options: RegisterOptions { force: register.force, description: register.description },
                },
            )?;
        }
        SubCommands::Pull(pull) => {
            action::pull(&ctx, &PullRequest { workspace_name, group_name: pull.group_name, options: PullOptions {} })?;
        }
    }
    Ok(0)
}

fn main() {
    match main_with_error() {
        Ok(code) => exit(code),
        Err(e) => error!("{}", e),
    }
}
