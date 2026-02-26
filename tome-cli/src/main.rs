mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "tome", about = "File change tracking system")]
struct Cli {
    /// SQLite database path (or postgres URL)
    #[arg(long, env = "TOME_DB", default_value = "tome.db")]
    db: String,

    /// Machine ID for Sonyflake ID generation (0–65535)
    #[arg(long, env = "TOME_MACHINE_ID", default_value_t = 0)]
    machine_id: u16,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory and record file changes
    Scan(commands::scan::ScanArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Initialize Sonyflake ID generator.
    tome_core::id::init(cli.machine_id, None::<i64>);

    // Build DB URL for SQLite if a plain path is given.
    let db_url = if cli.db.starts_with("sqlite:") || cli.db.starts_with("postgres") {
        cli.db.clone()
    } else {
        format!("sqlite://{}?mode=rwc", cli.db)
    };

    let db = tome_db::connection::open(&db_url).await?;

    match cli.command {
        Commands::Scan(args) => commands::scan::run(&db, args).await?,
    }

    Ok(())
}
