use std::{
    error::Error,
    fs::{create_dir_all, File},
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
};

use tempfile::NamedTempFile;
use url::Url;

pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
}

pub trait Filesystem {
    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, Box<dyn Error>>;
    fn upload(&mut self, path: &str, file: File) -> Result<(), Box<dyn Error>>;
    fn download(&mut self, path: &str, file: File) -> Result<(), Box<dyn Error>>;

    fn read(&mut self, path: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let tempfile = NamedTempFile::new()?;
        {
            let f = tempfile.reopen()?;
            self.download(path, f)?;
        }
        let mut content = Vec::new();
        {
            let mut f = tempfile.reopen()?;
            f.read_to_end(&mut content)?;
        }
        Ok(content)
    }

    fn write(&mut self, path: &str, content: &[u8]) -> Result<(), Box<dyn Error>> {
        let tempfile = NamedTempFile::new()?;
        {
            let mut f = tempfile.reopen()?;
            f.write_all(content)?;
        }
        {
            let f = tempfile.reopen()?;
            self.upload(path, f)?;
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
}

impl Filesystem for LocalFilesystem {
    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, Box<dyn Error>> {
        let path = self.base_path.join(path);
        let mut result = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let path = path.strip_prefix(&self.base_path)?;
            let path = path.to_str().unwrap();
            result.push(FileEntry { path: path.to_string(), is_dir: entry.file_type()?.is_dir() });
        }
        Ok(result)
    }

    fn upload(&mut self, path: &str, mut file: File) -> Result<(), Box<dyn Error>> {
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

    fn download(&mut self, path: &str, mut file: File) -> Result<(), Box<dyn Error>> {
        let full_path = self.base_path.join(path);
        info!("download: {}", full_path.to_string_lossy());
        let mut f = File::open(full_path)?;
        std::io::copy(&mut f, &mut file)?;
        Ok(())
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

    fn session(&mut self) -> Result<&ssh2::Session, Box<dyn Error>> {
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
}

impl Filesystem for SshFilesystem {
    fn list(&mut self, path: &str) -> Result<Vec<FileEntry>, Box<dyn Error>> {
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

    fn upload(&mut self, path: &str, mut file: File) -> Result<(), Box<dyn Error>> {
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

    fn download(&mut self, path: &str, mut file: File) -> Result<(), Box<dyn Error>> {
        let sess = self.session()?;
        let sftp = sess.sftp()?;
        let full_path = self.base_path.join(path);
        info!("download: {}", full_path.to_string_lossy());
        let mut remote_file = sftp.open(&full_path)?;
        std::io::copy(&mut remote_file, &mut file)?;
        Ok(())
    }
}
