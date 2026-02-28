use std::sync::Mutex;

use chrono::{TimeZone, Utc};
use sonyflake::{Builder, Sonyflake};

use crate::error::{CoreError, Result};

static SONYFLAKE: Mutex<Option<Sonyflake>> = Mutex::new(None);

/// Initialize the Sonyflake ID generator.
///
/// `machine_id`: 0–65535. Defaults to 0 if not called.
/// `start_time`: Unix seconds since epoch. Use None for default (2023-09-01 UTC = 1693526400).
pub fn init(machine_id: u16, start_time: Option<i64>) -> Result<()> {
    let start_secs = start_time.unwrap_or(1_693_526_400);
    let start = Utc
        .timestamp_opt(start_secs, 0)
        .single()
        .ok_or_else(|| CoreError::IdGeneration(format!("invalid start_time: {start_secs}")))?;
    let sf = Builder::new()
        .machine_id(&move || Ok(machine_id))
        .start_time(start)
        .finalize()
        .map_err(|e| CoreError::IdGeneration(format!("Sonyflake init failed: {e}")))?;
    *SONYFLAKE.lock().map_err(|e| CoreError::Other(format!("mutex poisoned: {e}")))? = Some(sf);
    Ok(())
}

/// Generate a new unique ID. Calls `init(0, None)` lazily if not already initialized.
pub fn next_id() -> Result<i64> {
    let mut guard = SONYFLAKE.lock().map_err(|e| CoreError::Other(format!("mutex poisoned: {e}")))?;
    if guard.is_none() {
        drop(guard);
        init(0, None)?;
        guard = SONYFLAKE.lock().map_err(|e| CoreError::Other(format!("mutex poisoned: {e}")))?;
    }
    guard.as_mut().unwrap().next_id().map(|id| id as i64).map_err(|e| CoreError::IdGeneration(e.to_string()))
}
