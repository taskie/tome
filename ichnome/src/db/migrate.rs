use std::error::Error;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use crate::db::config::Connection;

#[cfg(feature = "postgres")]
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations-postgres");
#[cfg(feature = "mysql")]
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations-mysql");

pub fn migrate(conn: &mut Connection) -> Result<(), Box<dyn Error>> {
    conn.run_pending_migrations(MIGRATIONS).unwrap();
    Ok(())
}
