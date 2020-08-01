use diesel::{sqlite::Sqlite, SqliteConnection};

pub type Connection = SqliteConnection;
pub type Backend = Sqlite;
