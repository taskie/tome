use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::migration::run_migrations;

/// Open a database connection and run all pending migrations.
///
/// `url` examples:
/// - SQLite:     `"sqlite:///path/to/db.sqlite?mode=rwc"`
/// - PostgreSQL: `"postgres://user:pass@host/db"`
pub async fn open(url: &str) -> anyhow::Result<DatabaseConnection> {
    let mut opts = ConnectOptions::new(url);
    opts.sqlx_logging(false);

    let db = Database::connect(opts).await?;
    run_migrations(&db).await?;
    Ok(db)
}
