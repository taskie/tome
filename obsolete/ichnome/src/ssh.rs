use std::{
    error::Error,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
};

use ssh2::Session;
use tempfile::NamedTempFile;
use url::Url;

pub fn download(url: &Url) -> Result<NamedTempFile, Box<dyn Error>> {
    let host = url.host_str().unwrap();
    let port = url.port().unwrap_or(22);
    let username = url.username();
    let path = Path::new(url.path());
    let tcp = TcpStream::connect(&format!("{}:{}", host, port))?;
    let mut sess = Session::new().unwrap();
    sess.set_tcp_stream(tcp);
    sess.handshake()?;
    sess.userauth_agent(username)?;

    let (mut remote_file, stat) = sess.scp_recv(path)?;
    debug!("remote file size: {}", stat.size());
    let tempfile = NamedTempFile::new()?;
    let mut f = tempfile.reopen()?;
    let mut buf = [0u8; 8192];
    loop {
        let n = remote_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write(&buf[0..n])?;
    }
    Ok(tempfile)
}
