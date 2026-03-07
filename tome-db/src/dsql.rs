use std::sync::atomic::{AtomicBool, Ordering};

/// Set to `true` when connected to AWS DSQL (which does not support FK constraints).
/// Controlled by [`set_dsql`]; read by migrations via [`is_dsql`].
static DSQL_MODE: AtomicBool = AtomicBool::new(false);

/// Returns `true` if the current connection is targeting AWS DSQL.
pub fn is_dsql() -> bool {
    DSQL_MODE.load(Ordering::Relaxed)
}

pub(crate) fn set_dsql(val: bool) {
    DSQL_MODE.store(val, Ordering::Relaxed);
}

/// Detect whether `url` points to AWS DSQL.
///
/// Heuristic: URL hostname contains `dsql.amazonaws.com`, or the env var
/// `TOME_DSQL` is set to any non-empty value.
pub(crate) fn detect(url: &str) -> bool {
    url.contains("dsql.amazonaws.com") || std::env::var("TOME_DSQL").map(|v| !v.is_empty()).unwrap_or(false)
}
