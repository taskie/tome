use chrono::{TimeZone, Utc};
use sonyflake::Sonyflake;

use crate::db::IdGenerate;

pub struct IdGenerator {
    sonyflake: Sonyflake,
}

impl IdGenerator {
    pub fn new(machine_id: Option<u16>) -> Self {
        let start_time = Utc.with_ymd_and_hms(2023, 9, 1, 0, 0, 0).unwrap();
        let sonyflake = Sonyflake::builder().start_time(start_time);
        let sonyflake = if let Some(machine_id) = machine_id {
            sonyflake.machine_id(&|| Ok(machine_id)).finalize().unwrap()
        } else {
            sonyflake.finalize().unwrap()
        };
        Self { sonyflake }
    }
}

impl IdGenerate for IdGenerator {
    fn generate_i64(&self) -> i64 {
        self.sonyflake.next_id().unwrap() as i64
    }
}
