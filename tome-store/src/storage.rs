use std::path::Path;

use async_trait::async_trait;

use crate::error::Result;

/// A remote/local object storage abstraction.
///
/// Paths within a store are slash-separated strings relative to the store root.
/// Blob objects are stored under `objects/<xx>/<yy>/<full-hex>` by convention,
/// but the trait itself is path-agnostic.
#[async_trait]
pub trait Storage: Send + Sync {
    /// List object paths under `prefix` (non-recursive; returns immediate children).
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;

    /// Upload a local file to `remote_path` within this store.
    async fn upload(&self, remote_path: &str, local_file: &Path) -> Result<()>;

    /// Download `remote_path` from this store to a local file.
    async fn download(&self, remote_path: &str, local_file: &Path) -> Result<()>;

    /// Delete `remote_path` from this store.
    async fn delete(&self, remote_path: &str) -> Result<()>;

    /// Check whether `remote_path` exists.
    async fn exists(&self, remote_path: &str) -> Result<bool>;
}

/// Allow `Box<dyn Storage>` to be used wherever `Storage` is expected.
#[async_trait]
impl Storage for Box<dyn Storage> {
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        (**self).list(prefix).await
    }

    async fn upload(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        (**self).upload(remote_path, local_file).await
    }

    async fn download(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        (**self).download(remote_path, local_file).await
    }

    async fn delete(&self, remote_path: &str) -> Result<()> {
        (**self).delete(remote_path).await
    }

    async fn exists(&self, remote_path: &str) -> Result<bool> {
        (**self).exists(remote_path).await
    }
}

/// Compute the content-addressed path for a blob within a store.
///
/// Layout: `objects/<hex[0..2]>/<hex[2..4]>/<full-hex>`
pub fn blob_path(digest_hex: &str) -> String {
    if digest_hex.len() < 4 {
        return format!("objects/{}", digest_hex);
    }
    format!("objects/{}/{}/{}", &digest_hex[..2], &digest_hex[2..4], digest_hex)
}
