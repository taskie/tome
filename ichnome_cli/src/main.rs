#[macro_use]
extern crate log;

use std::{env, error::Error, process::exit};

use chrono::Local;
use diesel::{Connection, MysqlConnection};
use ichnome::{
    action,
    action::{PullOptions, PullRequest, RegisterOptions, RegisterRequest},
};
use structopt::{clap, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(name = "ichnome")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(subcommand)]
    sub: SubCommands,
}

#[derive(Debug, StructOpt)]
pub enum SubCommands {
    Register(Register),
    Pull(Pull),
}

#[derive(Debug, StructOpt)]
pub struct Register {
    #[structopt(name = "NAMESPACE")]
    pub group_id: String,

    #[structopt(name = "URL")]
    pub url: String,

    #[structopt(short, long)]
    pub force: bool,
}

#[derive(Debug, StructOpt)]
pub struct Pull {
    #[structopt(name = "NAMESPACE")]
    pub group_id: String,
}

fn main_with_error() -> Result<i32, Box<dyn Error>> {
    dotenv::dotenv().ok();
    env_logger::init();

    let database_url = env::var("DATABASE_URL").unwrap_or("ichno.db".to_owned());
    let conn = MysqlConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url));
    let ctx = action::Context { connection: &conn };
    let current_time = Local::now();
    let opt = Opt::from_args();
    match opt.sub {
        SubCommands::Register(register) => {
            action::register(
                &ctx,
                &RegisterRequest {
                    group_id: register.group_id,
                    url: register.url,
                    current_time,
                    options: RegisterOptions { force: register.force },
                },
            )?;
        }
        SubCommands::Pull(pull) => {
            action::pull(&ctx, &PullRequest { group_id: pull.group_id, current_time, options: PullOptions {} })?;
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
