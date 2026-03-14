use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::migration::run_migrations;

/// Open a database connection and run all pending migrations.
///
/// Suitable for CLI usage where the database may need to be created and migrated
/// (especially SQLite with `?mode=rwc`).
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

/// Connect to the database **without** running migrations.
///
/// Use this when the schema is managed externally (e.g. via psqldef) and
/// migrations should not run on every startup — typically in Lambda or
/// other server deployments.
pub async fn connect(url: &str) -> anyhow::Result<DatabaseConnection> {
    let mut opts = ConnectOptions::new(url);
    opts.sqlx_logging(false);

    let db = Database::connect(opts).await?;
    Ok(db)
}
