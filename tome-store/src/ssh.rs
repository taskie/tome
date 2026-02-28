use std::{
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use tracing::info;

use crate::{StoreError, error::Result, storage::Storage};

/// Storage backed by SFTP over SSH.
///
/// Uses `ssh2` (blocking) wrapped in `spawn_blocking` for async compatibility.
/// Authentication is done via `ssh-agent`.
pub struct SshStorage {
    host: String,
    port: u16,
    username: String,
    root: PathBuf,
    /// Lazily initialised shared session.
    session: Arc<Mutex<Option<ssh2::Session>>>,
}

impl SshStorage {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>, root: PathBuf) -> Self {
        Self { host: host.into(), port, username: username.into(), root, session: Arc::new(Mutex::new(None)) }
    }

    fn ensure_session(
        session: &Arc<Mutex<Option<ssh2::Session>>>,
        host: &str,
        port: u16,
        username: &str,
    ) -> Result<()> {
        let mut guard = session.lock().map_err(|e| StoreError::Other(format!("mutex poisoned: {e}")))?;
        if guard.is_some() {
            return Ok(());
        }
        let tcp = TcpStream::connect((host, port))?;
        let mut sess = ssh2::Session::new().map_err(StoreError::Ssh)?;
        sess.set_tcp_stream(tcp);
        sess.handshake().map_err(StoreError::Ssh)?;
        sess.userauth_agent(username).map_err(StoreError::Ssh)?;
        *guard = Some(sess);
        Ok(())
    }

    fn with_sftp<F, T>(
        session: &Arc<Mutex<Option<ssh2::Session>>>,
        host: &str,
        port: u16,
        username: &str,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&ssh2::Sftp) -> Result<T>,
    {
        Self::ensure_session(session, host, port, username)?;
        let guard = session.lock().map_err(|e| StoreError::Other(format!("mutex poisoned: {e}")))?;
        let sess = guard.as_ref().ok_or_else(|| StoreError::Other("session not initialized".into()))?;
        let sftp = sess.sftp().map_err(StoreError::Ssh)?;
        f(&sftp)
    }
}

#[async_trait]
impl Storage for SshStorage {
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let dir = self.root.join(prefix);
        let root = self.root.clone();
        let session = Arc::clone(&self.session);
        let host = self.host.clone();
        let port = self.port;
        let username = self.username.clone();

        tokio::task::spawn_blocking(move || {
            Self::with_sftp(&session, &host, port, &username, |sftp| {
                let entries = sftp.readdir(&dir).map_err(StoreError::Ssh)?;
                let result = entries
                    .into_iter()
                    .filter_map(|(path, _stat)| path.strip_prefix(&root).ok().map(|p| p.to_string_lossy().into_owned()))
                    .collect();
                Ok(result)
            })
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))?
    }

    async fn upload(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        let dest = self.root.join(remote_path);
        let local_file = local_file.to_owned();
        let session = Arc::clone(&self.session);
        let host = self.host.clone();
        let port = self.port;
        let username = self.username.clone();
        let dest_display = dest.display().to_string();

        tokio::task::spawn_blocking(move || {
            Self::with_sftp(&session, &host, port, &username, |sftp| {
                // Create parent directories.
                if let Some(parent) = dest.parent() {
                    mkdir_p(sftp, parent)?;
                }
                info!("ssh upload: {:?} -> {}", local_file, dest_display);
                let mut src = std::fs::File::open(&local_file)?;
                let mut dst = sftp.create(&dest).map_err(StoreError::Ssh)?;
                std::io::copy(&mut src, &mut dst)?;
                Ok(())
            })
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))?
    }

    async fn download(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        let src = self.root.join(remote_path);
        let local_file = local_file.to_owned();
        let session = Arc::clone(&self.session);
        let host = self.host.clone();
        let port = self.port;
        let username = self.username.clone();
        let src_display = src.display().to_string();

        tokio::task::spawn_blocking(move || {
            Self::with_sftp(&session, &host, port, &username, |sftp| {
                info!("ssh download: {} -> {:?}", src_display, local_file);
                let mut remote = sftp.open(&src).map_err(StoreError::Ssh)?;
                let mut dst = std::fs::File::create(&local_file)?;
                std::io::copy(&mut remote, &mut dst)?;
                Ok(())
            })
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))?
    }

    async fn delete(&self, remote_path: &str) -> Result<()> {
        let path = self.root.join(remote_path);
        let session = Arc::clone(&self.session);
        let host = self.host.clone();
        let port = self.port;
        let username = self.username.clone();

        tokio::task::spawn_blocking(move || {
            Self::with_sftp(&session, &host, port, &username, |sftp| sftp.unlink(&path).map_err(StoreError::Ssh))
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))?
    }

    async fn exists(&self, remote_path: &str) -> Result<bool> {
        let path = self.root.join(remote_path);
        let session = Arc::clone(&self.session);
        let host = self.host.clone();
        let port = self.port;
        let username = self.username.clone();

        tokio::task::spawn_blocking(move || {
            Self::with_sftp(&session, &host, port, &username, |sftp| match sftp.stat(&path) {
                Ok(_) => Ok(true),
                Err(e) if e.message() == "no such file" => Ok(false),
                Err(e) => Err(StoreError::Ssh(e)),
            })
        })
        .await
        .map_err(|e| StoreError::Other(e.to_string()))?
    }
}

/// Recursively create directories via SFTP (mkdir -p).
fn mkdir_p(sftp: &ssh2::Sftp, dir: &Path) -> Result<()> {
    if sftp.stat(dir).is_ok() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        mkdir_p(sftp, parent)?;
    }
    sftp.mkdir(dir, 0o755).map_err(StoreError::Ssh)
}
