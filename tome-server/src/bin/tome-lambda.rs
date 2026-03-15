//! Lambda entry point.
//! Build with: cargo lambda build --release --features lambda --bin tome-lambda
//! For DynamoDB: cargo lambda build --release --features lambda,dynamodb --bin tome-lambda
//!
//! Environment variables:
//!   TOME_DB         — postgres://... or dynamodb://<table-name>
//!   TOME_MACHINE_ID — optional (default: 0)
//!
//! For PostgreSQL: the schema must be applied beforehand (e.g. via psqldef).
//! Migrations are NOT executed on Lambda startup.
//! For DynamoDB: the table and GSIs must be created beforehand (e.g. via Terraform).

use std::sync::Arc;

use anyhow::Context as _;
use tome_db::store_trait::MetadataStore;

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .json()
        .init();

    let db_url = std::env::var("TOME_DB").context("TOME_DB not set").expect("TOME_DB must be set");
    let machine_id: u16 = std::env::var("TOME_MACHINE_ID").ok().and_then(|s| s.parse().ok()).unwrap_or(0);

    tome_core::id::init(machine_id, None::<i64>).expect("id init failed");

    let store: Arc<dyn MetadataStore> = build_store(&db_url).await;

    tome_server::run_lambda(store).await
}

async fn build_store(db_url: &str) -> Arc<dyn MetadataStore> {
    #[cfg(feature = "dynamodb")]
    if let Some(table) = db_url.strip_prefix("dynamodb://") {
        tracing::info!(table, "using DynamoDB backend");
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_dynamodb::Client::new(&config);
        return Arc::new(tome_dynamo::DynamoStore::new(client, table.to_owned()));
    }

    tracing::info!("using PostgreSQL backend");
    let db = tome_db::connection::connect(db_url).await.expect("DB connection failed");
    Arc::new(tome_db::sea_orm_store::SeaOrmStore::new(db))
}
