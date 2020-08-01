use diesel::{mysql::Mysql, MysqlConnection};

pub type Connection = MysqlConnection;
pub type Backend = Mysql;
