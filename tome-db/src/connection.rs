use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::migration::run_migrations;

/// Open a database connection and run all pending migrations.
///
/// `url` examples:
/// - SQLite:     `"sqlite:///path/to/db.sqlite?mode=rwc"`
/// - PostgreSQL: `"postgres://user:pass@host/db"`
/// - DSQL:       `"postgres://admin:<token>@<endpoint>.dsql.amazonaws.com:5432/postgres?sslmode=require"`
///
/// DSQL is detected automatically from the URL (`dsql.amazonaws.com`) or by
/// setting the `TOME_DSQL` environment variable to a non-empty value. When
/// DSQL is detected, FK constraints are omitted from migrations because DSQL
/// does not support `FOREIGN KEY` declarations.
pub async fn open(url: &str) -> anyhow::Result<DatabaseConnection> {
    crate::dsql::set_dsql(crate::dsql::detect(url));

    let mut opts = ConnectOptions::new(url);
    opts.sqlx_logging(false);

    let db = Database::connect(opts).await?;
    run_migrations(&db).await?;
    Ok(db)
}
