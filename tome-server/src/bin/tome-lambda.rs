//! Lambda entry point.
//! Build with: cargo lambda build --release --features lambda --bin tome-lambda
//!
//! Environment variables:
//!   TOME_DB         — postgres://<user>:<password>@<endpoint>:5432/<database>
//!   TOME_MACHINE_ID — optional (default: 0)
//!
//! The schema must be applied beforehand (e.g. via psqldef).
//! Migrations are NOT executed on Lambda startup.

use anyhow::Context as _;

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .json()
        .init();

    let db_url = std::env::var("TOME_DB").context("TOME_DB not set").expect("TOME_DB must be set");
    let machine_id: u16 = std::env::var("TOME_MACHINE_ID").ok().and_then(|s| s.parse().ok()).unwrap_or(0);

    tome_core::id::init(machine_id, None::<i64>).expect("id init failed");

    let db = tome_db::connection::connect(&db_url).await.expect("DB connection failed");

    tome_server::run_lambda(db).await
}
