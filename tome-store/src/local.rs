use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tracing::info;

use crate::{StoreError, error::Result, storage::Storage};

/// Storage backed by the local filesystem.
pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn full_path(&self, remote_path: &str) -> PathBuf {
        self.root.join(remote_path)
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let dir = self.full_path(prefix);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut result = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let rel = path
                .strip_prefix(&self.root)
                .map_err(|e| StoreError::Other(e.to_string()))?
                .to_string_lossy()
                .into_owned();
            result.push(rel);
        }
        Ok(result)
    }

    async fn upload(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        let dest = self.full_path(remote_path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        info!("local upload: {:?} -> {:?}", local_file, dest);
        tokio::fs::copy(local_file, &dest).await?;
        Ok(())
    }

    async fn download(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        let src = self.full_path(remote_path);
        info!("local download: {:?} -> {:?}", src, local_file);
        tokio::fs::copy(&src, local_file).await?;
        Ok(())
    }

    async fn delete(&self, remote_path: &str) -> Result<()> {
        let path = self.full_path(remote_path);
        info!("local delete: {:?}", path);
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn exists(&self, remote_path: &str) -> Result<bool> {
        Ok(self.full_path(remote_path).exists())
    }
}
