//! Run all migrations on an in-memory SQLite database and dump the resulting
//! schema as SQL statements on stdout.
//!
//! ```bash
//! cargo run -p tome-db --example generate_schema 2>/dev/null > docs/schema/tome-db.sql
//! ```

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

#[tokio::main]
async fn main() {
    let db = tome_db::connection::open("sqlite::memory:").await.expect("open in-memory SQLite and run migrations");

    let rows = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL \
             AND name NOT LIKE 'sqlite_%' AND name != 'seaql_migrations' \
             ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END, name",
        ))
        .await
        .expect("query sqlite_master");

    for row in rows {
        let sql: String = sea_orm::TryGetable::try_get(&row, "", "sql").expect("get sql column");
        println!("{sql};\n");
    }
}
