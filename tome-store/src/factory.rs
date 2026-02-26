use std::path::PathBuf;

use url::Url;

use crate::{
    error::{Result, StoreError},
    local::LocalStorage,
    s3::S3Storage,
    ssh::SshStorage,
    storage::Storage,
};

/// Create a `Storage` instance from a store URL.
///
/// Supported schemes:
/// - `file:///path`        → LocalStorage
/// - `ssh://user@host/path` → SshStorage (ssh-agent auth)
/// - `s3://bucket/prefix`  → S3Storage (default AWS credential chain)
pub async fn open_storage(url_str: &str) -> Result<Box<dyn Storage>> {
    let url = Url::parse(url_str).map_err(|e| StoreError::InvalidUrl(e.to_string()))?;
    match url.scheme() {
        "file" => {
            let root = PathBuf::from(url.path());
            Ok(Box::new(LocalStorage::new(root)))
        }
        "ssh" => {
            let host = url.host_str().ok_or_else(|| StoreError::InvalidUrl("missing host".into()))?.to_owned();
            let port = url.port().unwrap_or(22);
            let username = if url.username().is_empty() {
                whoami_user()
            } else {
                url.username().to_owned()
            };
            let root = PathBuf::from(url.path());
            Ok(Box::new(SshStorage::new(host, port, username, root)))
        }
        "s3" => {
            let bucket = url.host_str().ok_or_else(|| StoreError::InvalidUrl("missing bucket".into()))?.to_owned();
            let prefix = url.path().trim_start_matches('/').to_owned();
            let config = aws_config::defaults(aws_config::BehaviorVersion::latest()).load().await;
            let client = aws_sdk_s3::Client::new(&config);
            Ok(Box::new(S3Storage::new(client, bucket, prefix)))
        }
        scheme => Err(StoreError::UnsupportedScheme(scheme.to_owned())),
    }
}

fn whoami_user() -> String {
    std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_else(|_| "unknown".to_owned())
}

/// Read a 32-byte key file.
pub fn read_key_file(path: &std::path::Path) -> Result<[u8; 32]> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut key = [0u8; 32];
    f.read_exact(&mut key)?;
    Ok(key)
}

/// Default key directory: `~/.config/tome/keys/`.
pub fn key_dir() -> PathBuf {
    dirs_or_home().join(".config").join("tome").join("keys")
}

fn dirs_or_home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"))
}
