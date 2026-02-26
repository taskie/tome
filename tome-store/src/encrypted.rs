use std::{
    io::{BufReader, BufWriter},
    path::Path,
};

use async_trait::async_trait;
use tracing::info;

use crate::{error::Result, storage::Storage, StoreError};

/// A `Storage` wrapper that transparently encrypts on upload and decrypts on download
/// using `aether` (AES-256-GCM + Argon2id).
///
/// Remote paths are unchanged by this wrapper — the caller is responsible for
/// using a suitable path (e.g. from `aether::Cipher::encrypt_file_name`).
pub struct EncryptedStorage<S> {
    inner: S,
    key: [u8; 32],
}

impl<S: Storage> EncryptedStorage<S> {
    /// Wrap `inner` with a 32-byte raw key.
    pub fn new(inner: S, key: [u8; 32]) -> Self {
        Self { inner, key }
    }
}

#[async_trait]
impl<S: Storage> Storage for EncryptedStorage<S> {
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.inner.list(prefix).await
    }

    async fn upload(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        // Encrypt to a temp file then upload.
        let tmp = tempfile::NamedTempFile::new().map_err(StoreError::Io)?;
        let tmp_path = tmp.path().to_owned();
        let local_file = local_file.to_owned();
        let key = self.key;

        tokio::task::spawn_blocking(move || -> Result<()> {
            let src = std::fs::File::open(&local_file)?;
            let dst = std::fs::File::create(&tmp_path)?;
            let mut cipher = aether::Cipher::with_key_slice(&key);
            cipher
                .encrypt(BufReader::new(src), BufWriter::new(dst))
                .map_err(|e| StoreError::Encryption(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))??;

        info!("encrypted upload -> {}", remote_path);
        self.inner.upload(remote_path, tmp.path()).await
    }

    async fn download(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        // Download to a temp file then decrypt to destination.
        let tmp = tempfile::NamedTempFile::new().map_err(StoreError::Io)?;
        self.inner.download(remote_path, tmp.path()).await?;

        let tmp_path = tmp.path().to_owned();
        let local_file = local_file.to_owned();
        let key = self.key;

        tokio::task::spawn_blocking(move || -> Result<()> {
            let src = std::fs::File::open(&tmp_path)?;
            let dst = std::fs::File::create(&local_file)?;
            let mut cipher = aether::Cipher::with_key_slice(&key);
            cipher
                .decrypt(BufReader::new(src), BufWriter::new(dst))
                .map_err(|e| StoreError::Encryption(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))??;

        info!("decrypted download: {}", remote_path);
        Ok(())
    }

    async fn delete(&self, remote_path: &str) -> Result<()> {
        self.inner.delete(remote_path).await
    }

    async fn exists(&self, remote_path: &str) -> Result<bool> {
        self.inner.exists(remote_path).await
    }
}
