use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tome_cli::{commands, config};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "tome", about = "File change tracking system")]
struct Cli {
    /// SQLite database path (or postgres URL) [env: TOME_DB] [config: db]
    #[arg(long, env = "TOME_DB")]
    db: Option<String>,

    /// Machine ID for Sonyflake ID generation (0–65535) [env: TOME_MACHINE_ID] [config: machine_id]
    #[arg(long, env = "TOME_MACHINE_ID")]
    machine_id: Option<u16>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory and record file changes
    Scan(commands::scan::ScanArgs),
    /// Show differences between two snapshots
    Diff(commands::diff::DiffArgs),
    /// Restore files from a snapshot via a store
    Restore(commands::restore::RestoreArgs),
    /// Manage object stores
    Store(commands::store::StoreArgs),
    /// Manage remote peers (add, set, rm, list)
    Remote(commands::remote::RemoteArgs),
    /// Low-level sync operations (config, pull, push)
    Sync(commands::sync::SyncArgs),
    /// Manage blob tags (key=value metadata)
    Tag(commands::tag::TagArgs),
    /// Verify scanned files against entry cache (bit-rot detection)
    Verify(commands::verify::VerifyArgs),
    /// Garbage-collect unreferenced blobs and old snapshots
    Gc(commands::gc::GcArgs),
    /// Register this machine with a central tome-server
    Init(commands::init::InitArgs),
    /// Scan, push blobs to a store, and sync to a peer (scan + store push + sync push)
    Push(commands::push::PushArgs),
    /// Pull changes from a sync peer (sync pull + optional blob copy)
    Pull(commands::push::PullArgs),
    /// Start the HTTP API server
    Serve(ServeArgs),
    /// Watch a directory and automatically take snapshots on changes
    Watch(commands::watch::WatchArgs),
}

#[derive(clap::Args)]
struct ServeArgs {
    /// Address to listen on [config: serve.addr]
    #[arg(long)]
    addr: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).with_writer(std::io::stderr).init();

    // Load tome.toml (global + project-local) before parsing CLI args so that
    // config values can be used as fallbacks.
    let cfg = config::load_config();

    let cli = Cli::parse();

    // `init` doesn't need a DB connection — handle it early.
    if let Commands::Init(args) = cli.command {
        return commands::init::run(args).await;
    }

    // Resolve final values: CLI arg > env var > config file > built-in default.
    // (clap already handles CLI > env; we add the config layer here.)
    let db = cli.db.or(cfg.db).unwrap_or_else(|| "tome.db".to_owned());
    let machine_id = cli.machine_id.or(cfg.machine_id).unwrap_or(0);

    // Initialize Sonyflake ID generator.
    tome_core::id::init(machine_id, None::<i64>).context("failed to initialize ID generator")?;

    // Build DB URL for SQLite if a plain path is given.
    let db_url = if db.starts_with("sqlite:") || db.starts_with("postgres") {
        db.clone()
    } else {
        format!("sqlite://{}?mode=rwc", db)
    };

    let db_conn = tome_db::connection::open(&db_url).await.context("failed to open database")?;

    match cli.command {
        Commands::Init(_) => unreachable!(),
        Commands::Scan(args) => commands::scan::run(&db_conn, args).await?,
        Commands::Diff(args) => commands::diff::run(&db_conn, args).await?,
        Commands::Restore(args) => commands::restore::run(&db_conn, args).await?,
        Commands::Store(args) => commands::store::run(&db_conn, args, &cfg.store).await?,
        Commands::Remote(args) => commands::remote::run(&db_conn, args).await?,
        Commands::Sync(args) => commands::sync::run(&db_conn, args).await?,
        Commands::Tag(args) => commands::tag::run(&db_conn, args).await?,
        Commands::Verify(args) => commands::verify::run(&db_conn, args).await?,
        Commands::Gc(args) => commands::gc::run(&db_conn, args).await?,
        Commands::Push(args) => commands::push::run_push(&db_conn, args, &cfg.store).await?,
        Commands::Pull(args) => commands::push::run_pull(&db_conn, args, &cfg.store).await?,
        Commands::Serve(args) => {
            let addr = args.addr.or(cfg.serve.addr).unwrap_or_else(|| "127.0.0.1:8080".to_owned());
            let store = std::sync::Arc::new(tome_db::sea_orm_store::SeaOrmStore::new(db_conn));
            tome_server::serve(store, &addr).await?
        }
        Commands::Watch(mut args) => {
            if args.quiet_period.is_none() {
                args.quiet_period = cfg.watch.quiet_period;
            }
            if args.max_delay.is_none() {
                args.max_delay = cfg.watch.max_delay;
            }
            commands::watch::run(&db_conn, args).await?
        }
    }

    Ok(())
}
