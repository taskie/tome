#[macro_use]
extern crate log;

use std::{env, error::Error, process::exit};

use chrono::Utc;
use diesel::Connection;
use ichno::id::IdGenerator;
use ichnome::{
    action,
    action::{
        CopyOptions, CopyRequest, PullOptions, PullRequest, RegisterOptions, RegisterRequest, SetupOptions,
        SetupRequest,
    },
    db::Connection as OmConnection,
};
use structopt::{clap, StructOpt};
use tokio::runtime::Handle;

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
    Migrate(Migrate),
    Setup(Setup),
    Register(Register),
    Pull(Pull),
    Copy(Copy),
}

#[derive(Debug, StructOpt)]
pub struct Migrate {}

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

#[derive(Debug, StructOpt)]
pub struct Copy {
    #[structopt(name = "SRC")]
    pub src: String,

    #[structopt(name = "DST")]
    pub dst: String,

    #[structopt(long)]
    pub overwrite: bool,
}

fn main_with_error(handle: Handle) -> Result<i32, Box<dyn Error>> {
    dotenv::dotenv().ok();
    env_logger::init();
    let database_url = env::var("ICHNOME_DATABASE_URL").unwrap_or("ichno.db".to_owned());
    let mut conn = OmConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url));

    let machine_id = env::var("ICHNOME_MACHINE_ID").map(|s| u16::from_str_radix(&s, 10).unwrap()).ok();
    let id_generator = IdGenerator::new(machine_id);

    let mut ctx =
        action::Context { handle, connection: &mut conn, id_generator: &id_generator, timer: Box::new(|| Utc::now()) };
    let opt = Opt::from_args();
    let workspace_name = opt.workspace.or_else(|| env::var("ICHNOME_WORKSPACE").ok()).unwrap();
    match opt.sub {
        SubCommands::Migrate(_) => {
            ichnome::db::migrate(&mut ctx.connection)?;
        }
        SubCommands::Setup(setup) => {
            action::setup(
                &mut ctx,
                &SetupRequest {
                    workspace_name,
                    options: SetupOptions { force: setup.force, description: setup.description },
                },
            )?;
        }
        SubCommands::Register(register) => {
            action::register(
                &mut ctx,
                &RegisterRequest {
                    workspace_name,
                    group_name: register.group_name,
                    url: register.url,
                    options: RegisterOptions { force: register.force, description: register.description },
                },
            )?;
        }
        SubCommands::Pull(pull) => {
            action::pull(
                &mut ctx,
                &PullRequest { workspace_name, group_name: pull.group_name, options: PullOptions {} },
            )?;
        }
        SubCommands::Copy(copy) => {
            let src: Vec<&str> = copy.src.splitn(2, ":").collect();
            if src.len() != 2 {
                return Err("invalid src".into());
            }
            let results = action::copy(
                &mut ctx,
                &CopyRequest {
                    workspace_name,
                    src_group_name: src[0].to_owned(),
                    src_path: src[1].to_owned(),
                    dst_group_name: copy.dst.clone(),
                    options: CopyOptions { overwrite: copy.overwrite, ..Default::default() },
                },
            )?;
            let error_count = results.paths.iter().map(|r| !r.0 as i32).reduce(|acc, e| acc + e).unwrap_or_default();
            if error_count > 0 {
                return Err(format!("{} errors occured.", error_count).into());
            }
        }
    }
    Ok(0)
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("failed to create runtime");
    let handle = runtime.handle().clone();
    match main_with_error(handle) {
        Ok(code) => exit(code),
        Err(e) => error!("{}", e),
    }
}
