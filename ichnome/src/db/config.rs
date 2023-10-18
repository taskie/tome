#[cfg(feature = "postgres")]
use diesel::{pg::Pg, PgConnection};
#[cfg(feature = "postgres")]
pub type Connection = PgConnection;
#[cfg(feature = "postgres")]
pub type Backend = Pg;

#[cfg(feature = "mysql")]
use diesel::{mysql::Mysql, MysqlConnection};
#[cfg(feature = "mysql")]
pub type Connection = MysqlConnection;
#[cfg(feature = "mysql")]
pub type Backend = Mysql;
