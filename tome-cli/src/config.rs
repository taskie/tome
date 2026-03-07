//! tome.toml configuration file loader.
//!
//! Load priority (later overrides earlier):
//!   1. built-in defaults
//!   2. `~/.config/tome/tome.toml`  (global)
//!   3. `./tome.toml`               (project-local)
//!   4. environment variables       (handled by clap)
//!   5. CLI arguments               (handled by clap)

use std::path::{Path, PathBuf};

use serde::Deserialize;

// ──────────────────────────────────────────────────────────────────────────────
// Config structs
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TomeConfig {
    /// SQLite path or postgres URL (overridden by --db / TOME_DB)
    pub db: Option<String>,

    /// Sonyflake machine ID (overridden by --machine-id / TOME_MACHINE_ID)
    pub machine_id: Option<u16>,

    pub scan: ScanConfig,
    pub store: StoreConfig,
    pub serve: ServeConfig,
    pub watch: WatchConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    /// Default repository name
    pub repo: Option<String>,

    /// Skip .gitignore / .ignore files by default
    pub no_ignore: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    /// Default store name for `tome store push`
    pub default_store: Option<String>,

    /// Path to the 32-byte binary key file used for encryption
    pub key_file: Option<PathBuf>,

    /// External secret manager URI for the encryption key.
    /// Overridden by `key_file` when both are set.
    /// Examples:
    ///   `env://TOME_KEY`
    ///   `file:///home/user/.config/tome/keys/mykey`
    ///   `aws-secrets-manager://my-tome-key`
    ///   `vault://secret/data/tome?field=key`
    pub key_source: Option<String>,

    /// Default cipher algorithm ("aes256gcm" or "chacha20poly1305")
    pub cipher: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ServeConfig {
    /// Default listen address
    pub addr: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WatchConfig {
    /// Seconds of inactivity before taking a snapshot (default: 60)
    pub quiet_period: Option<u64>,
    /// Max seconds from first change to forced snapshot (default: 600)
    pub max_delay: Option<u64>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Loading
// ──────────────────────────────────────────────────────────────────────────────

/// Load and merge configuration files.
///
/// Returns a `TomeConfig` with values from the global config and/or the
/// project-local `./tome.toml`, with project-local values taking precedence.
pub fn load_config() -> TomeConfig {
    let mut config = TomeConfig::default();

    // 1. Global: ~/.config/tome/tome.toml
    if let Some(cfg_dir) = dirs::config_dir() {
        merge_file(&mut config, cfg_dir.join("tome/tome.toml"));
    }

    // 2. Project-local: ./tome.toml
    merge_file(&mut config, PathBuf::from("tome.toml"));

    config
}

/// Parse `path` as TOML and overlay any present fields onto `base`.
fn merge_file(base: &mut TomeConfig, path: impl AsRef<Path>) {
    let path = path.as_ref();
    let Ok(text) = std::fs::read_to_string(path) else {
        return; // file absent or unreadable — silently skip
    };
    let overlay: TomeConfig = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: failed to parse {:?}: {}", path, e);
            return;
        }
    };
    merge(base, overlay);
}

/// Overlay non-None fields from `src` onto `dst`.
fn merge(dst: &mut TomeConfig, src: TomeConfig) {
    if src.db.is_some() {
        dst.db = src.db;
    }
    if src.machine_id.is_some() {
        dst.machine_id = src.machine_id;
    }
    merge_scan(&mut dst.scan, src.scan);
    merge_store(&mut dst.store, src.store);
    merge_serve(&mut dst.serve, src.serve);
    merge_watch(&mut dst.watch, src.watch);
}

fn merge_scan(dst: &mut ScanConfig, src: ScanConfig) {
    if src.repo.is_some() {
        dst.repo = src.repo;
    }
    if src.no_ignore.is_some() {
        dst.no_ignore = src.no_ignore;
    }
}

fn merge_store(dst: &mut StoreConfig, src: StoreConfig) {
    if src.default_store.is_some() {
        dst.default_store = src.default_store;
    }
    if src.key_file.is_some() {
        dst.key_file = src.key_file;
    }
    if src.key_source.is_some() {
        dst.key_source = src.key_source;
    }
    if src.cipher.is_some() {
        dst.cipher = src.cipher;
    }
}

fn merge_serve(dst: &mut ServeConfig, src: ServeConfig) {
    if src.addr.is_some() {
        dst.addr = src.addr;
    }
}

fn merge_watch(dst: &mut WatchConfig, src: WatchConfig) {
    if src.quiet_period.is_some() {
        dst.quiet_period = src.quiet_period;
    }
    if src.max_delay.is_some() {
        dst.max_delay = src.max_delay;
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Path helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_owned()
}
