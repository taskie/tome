use std::{
    fs::{create_dir_all, File},
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
};

use aws_sdk_s3::{primitives::ByteStream, Client as S3Client};
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::runtime::Handle;
use url::Url;

#[derive(Debug, Error)]
pub enum FilesystemError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SSH error: {0}")]
    Ssh2(#[from] ssh2::Error),
    #[error("AWS S3 error: {0}")]
    AwsS3(#[from] aws_sdk_s3::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
}

pub trait Filesystem {
    fn url(&self) -> Url;

    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, FilesystemError>;
    fn upload(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError>;
    fn download(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError>;
    fn delete(&mut self, path: &str) -> Result<(), FilesystemError>;

    fn read(&mut self, path: &str) -> Result<Vec<u8>, FilesystemError> {
        let tempfile = NamedTempFile::new()?;
        {
            self.download(path, tempfile.path())?;
        }
        let mut content = Vec::new();
        {
            let mut f = tempfile.reopen()?;
            f.read_to_end(&mut content)?;
        }
        Ok(content)
    }

    fn write(&mut self, path: &str, content: &[u8]) -> Result<(), FilesystemError> {
        let tempfile = NamedTempFile::new()?;
        {
            let mut f = tempfile.reopen()?;
            f.write_all(content)?;
        }
        {
            self.upload(path, tempfile.path())?;
        }
        Ok(())
    }
}

pub struct LocalFilesystem {
    base_path: PathBuf,
}

impl LocalFilesystem {
    pub fn new(root: PathBuf) -> Self {
        Self { base_path: root }
    }

    pub fn with_url(url: &Url) -> Option<Self> {
        if url.scheme() != "file" {
            return None;
        }
        let path = PathBuf::from(url.path());
        Some(Self::new(path))
    }

    fn list_sync(&mut self, path: &str) -> Result<Vec<FileEntry>, FilesystemError> {
        let path = self.base_path.join(path);
        let mut result = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let path = path.strip_prefix(&self.base_path).map_err(|e| FilesystemError::Internal(e.to_string()))?;
            let path = path.to_str().unwrap();
            result.push(FileEntry { path: path.to_string(), is_dir: entry.file_type()?.is_dir() });
        }
        Ok(result)
    }

    fn upload_sync(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        let mut file = File::open(file)?;
        let full_path = self.base_path.join(path);
        if let Some(dir) = full_path.parent() {
            if !dir.exists() {
                info!("create dir: {}", dir.to_string_lossy());
                create_dir_all(dir)?;
            }
        }
        info!("upload: {}", full_path.to_string_lossy());
        let mut f = File::create(full_path)?;
        std::io::copy(&mut file, &mut f)?;
        Ok(())
    }

    fn download_sync(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        let mut file = File::create(file)?;
        let full_path = self.base_path.join(path);
        info!("download: {}", full_path.to_string_lossy());
        let mut f = File::open(full_path)?;
        std::io::copy(&mut f, &mut file)?;
        Ok(())
    }

    fn delete_sync(&mut self, path: &str) -> Result<(), FilesystemError> {
        let full_path = self.base_path.join(path);
        info!("delete: {}", full_path.to_string_lossy());
        std::fs::remove_file(full_path)?;
        Ok(())
    }
}

impl Filesystem for LocalFilesystem {
    fn url(&self) -> Url {
        Url::parse(&format!("file://{}", self.base_path.to_string_lossy())).unwrap()
    }

    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, FilesystemError> {
        self.list_sync(path)
    }

    fn upload(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        self.upload_sync(path, file)
    }

    fn download(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        self.download_sync(path, file)
    }

    fn delete(&mut self, path: &str) -> Result<(), FilesystemError> {
        self.delete_sync(path)
    }
}

pub struct SshFilesystem {
    url: Url,
    base_path: PathBuf,
    username: String,
    session: Option<ssh2::Session>,
}

impl SshFilesystem {
    pub fn with_url(url: &Url) -> Option<Self> {
        if url.scheme() != "ssh" {
            return None;
        }
        Some(Self {
            url: url.clone(),
            base_path: PathBuf::from(url.path()),
            username: url.username().to_string(),
            session: None,
        })
    }

    fn session(&mut self) -> Result<&ssh2::Session, FilesystemError> {
        if self.session.is_some() {
            return Ok(&self.session.as_ref().unwrap());
        }
        let host = self.url.host_str().unwrap();
        let port = self.url.port().unwrap_or(22);
        let tcp = TcpStream::connect(&format!("{}:{}", host, port))?;
        let mut sess = ssh2::Session::new().unwrap();
        sess.set_tcp_stream(tcp);
        sess.handshake()?;
        sess.userauth_agent(&self.username)?;
        self.session = Some(sess);
        Ok(&self.session.as_ref().unwrap())
    }

    fn list_sync(&mut self, path: &str) -> Result<Vec<FileEntry>, FilesystemError> {
        let sess = self.session()?;
        let sftp = sess.sftp()?;
        let full_path = self.base_path.join(path);
        let result = sftp
            .readdir(&full_path)?
            .into_iter()
            .flat_map(|(path, stat)| {
                path.strip_prefix(&self.base_path)
                    .map(|p| FileEntry { path: p.to_string_lossy().to_string(), is_dir: stat.is_dir() })
            })
            .collect();
        Ok(result)
    }

    fn upload_sync(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        let mut file = File::open(file)?;
        let sess = self.session()?;
        let sftp = sess.sftp()?;
        let full_path = self.base_path.join(path);
        let mut dirs = full_path.parent().map(|p| vec![p.to_owned()]).unwrap_or_default();
        while let Some(dir) = dirs.pop() {
            if let Some(parent_dir) = dir.parent() {
                let parent_stat = sftp.stat(&parent_dir);
                if let Err(e) = parent_stat {
                    if e.message() == "no such file" {
                        dirs.push(dir.clone());
                        dirs.push(parent_dir.to_owned());
                        continue;
                    } else {
                        return Err(e.into());
                    }
                }
            }
            info!("create dir: {}", dir.to_string_lossy());
            sftp.mkdir(&dir, 0o755)?;
        }
        info!("upload: {}", full_path.to_string_lossy());
        let mut remote_file = sftp.create(&full_path)?;
        std::io::copy(&mut file, &mut remote_file)?;
        Ok(())
    }

    fn download_sync(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        let mut file = File::create(file)?;
        let sess = self.session()?;
        let sftp = sess.sftp()?;
        let full_path = self.base_path.join(path);
        info!("download: {}", full_path.to_string_lossy());
        let mut remote_file = sftp.open(&full_path)?;
        std::io::copy(&mut remote_file, &mut file)?;
        Ok(())
    }

    fn delete_sync(&mut self, path: &str) -> Result<(), FilesystemError> {
        let sess = self.session()?;
        let sftp = sess.sftp()?;
        let full_path = self.base_path.join(path);
        info!("delete: {}", full_path.to_string_lossy());
        sftp.unlink(&full_path)?;
        Ok(())
    }
}

impl Filesystem for SshFilesystem {
    fn url(&self) -> Url {
        self.url.clone()
    }

    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, FilesystemError> {
        self.list_sync(path)
    }

    fn upload(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        self.upload_sync(path, file)
    }

    fn download(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        self.download_sync(path, file).into()
    }

    fn delete(&mut self, path: &str) -> Result<(), FilesystemError> {
        self.delete_sync(path)
    }
}

pub struct S3Filesystem {
    url: Url,
    bucket: String,
    base_path: PathBuf,
    handle: Handle,
    client: S3Client,
}

impl S3Filesystem {
    pub fn with_url(url: &Url, handle: Handle, client: S3Client) -> Option<Self> {
        if url.scheme() != "s3" {
            return None;
        }
        let Some(bucket) = url.host_str().map(|s| s.to_string()) else {
            return None;
        };
        Some(Self { url: url.clone(), bucket, base_path: PathBuf::from(url.path()), handle, client })
    }
}

impl Filesystem for S3Filesystem {
    fn url(&self) -> Url {
        self.url.clone()
    }

    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, FilesystemError> {
        self.handle.block_on(async {
            let resp = self
                .client
                .list_objects_v2()
                .bucket(self.bucket.clone())
                .prefix(path)
                .send()
                .await
                .map_err(|e| FilesystemError::AwsS3(e.into()))?;
            let Some(objects) = resp.contents() else {
                return Ok(Vec::new());
            };
            let result = objects
                .iter()
                .flat_map(|object| {
                    let path = object.key().unwrap();
                    let base_path = self.base_path.to_string_lossy();
                    let path = path
                        .strip_prefix(base_path.as_ref())
                        .map(|p| FileEntry { path: p.to_string(), is_dir: p.ends_with('/') });
                    path
                })
                .collect();
            Ok(result)
        })
    }

    fn upload(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        self.handle.block_on(async {
            let full_path = self.base_path.join(path);
            let body = ByteStream::from_path(file).await.map_err(|e| FilesystemError::Internal(e.to_string()))?;
            let _resp = self
                .client
                .put_object()
                .bucket(self.bucket.clone())
                .key(full_path.to_string_lossy().to_string())
                .body(body)
                .send()
                .await
                .map_err(|e| FilesystemError::AwsS3(e.into()))?;
            Ok(())
        })
    }

    fn download(&mut self, path: &str, file: &Path) -> Result<(), FilesystemError> {
        self.handle.block_on(async {
            let full_path = self.base_path.join(path);
            let resp = self
                .client
                .get_object()
                .bucket(self.bucket.clone())
                .key(full_path.to_string_lossy().to_string())
                .send()
                .await
                .map_err(|e| FilesystemError::AwsS3(e.into()))?;
            let mut reader = resp.body.into_async_read();
            let mut file = tokio::fs::File::create(file).await?;
            tokio::io::copy(&mut reader, &mut file).await?;
            Ok(())
        })
    }

    fn delete(&mut self, path: &str) -> Result<(), FilesystemError> {
        self.handle.block_on(async {
            let full_path = self.base_path.join(path);
            let _resp = self
                .client
                .delete_object()
                .bucket(self.bucket.clone())
                .key(full_path.to_string_lossy().to_string())
                .send()
                .await
                .map_err(|e| FilesystemError::AwsS3(e.into()))?;
            Ok(())
        })
    }
}
