#[cfg(feature = "postgres")]
use diesel::{pg::Pg, PgConnection};
#[cfg(feature = "postgres")]
pub type Connection = PgConnection;
#[cfg(feature = "postgres")]
pub type Backend = Pg;
