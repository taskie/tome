#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fmt,
    fmt::{Debug, Formatter},
    fs::{self, File},
    io::{self, Error, Read, Write},
    path::Path,
    result,
};

use crate::hex::to_hex_string;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FileMode(i32);

impl FileMode {
    pub const DIR: FileMode = FileMode(0o40000);
    pub const EXECUTABLE: FileMode = FileMode(0o100755);
    pub const REGULAR: FileMode = FileMode(0o100644);
    pub const SYMLINK: FileMode = FileMode(0o120000);

    fn is_executable(md: fs::Metadata) -> bool {
        if cfg!(unix) { md.permissions().mode() & 0o111 != 0 } else { false }
    }

    pub fn from(md: fs::Metadata) -> FileMode {
        if md.is_dir() {
            FileMode::DIR
        } else if md.file_type().is_symlink() {
            FileMode::SYMLINK
        } else if FileMode::is_executable(md) {
            FileMode::EXECUTABLE
        } else {
            FileMode::REGULAR
        }
    }

    pub fn as_i32(&self) -> i32 {
        self.0
    }

    pub fn is_dir(&self) -> bool {
        *self == FileMode::DIR
    }
}

type Result<T> = result::Result<T, Error>;

fn object_header<W>(w: &mut W, type_bs: &[u8], size: usize) -> Result<usize>
where
    W: Write,
{
    let mut n = 0;
    n += w.write(type_bs)?;
    n += w.write(b" ")?;
    n += w.write(format!("{}", size).as_bytes())?;
    n += w.write(b"\0")?;
    Ok(n)
}

fn blob_from_read<W, R>(w: &mut W, r: &mut R, size: usize) -> Result<usize>
where
    W: Write,
    R: Read,
{
    let mut n = 0;
    n += object_header(w, b"blob", size)?;
    let copied_n = io::copy(r, w)? as usize;
    assert_eq!(size, copied_n);
    Ok(n)
}

pub fn blob_from_path<W, P>(w: &mut W, path: P) -> Result<usize>
where
    W: Write,
    P: AsRef<Path>,
{
    let _n = 0;
    if let Ok(pb) = fs::read_link(path.as_ref()) {
        let mut buf = Vec::new();
        let mut buf_n = 0;
        for (i, comp) in pb.iter().enumerate() {
            if i != 0 {
                buf_n += buf.write(b"/")?
            }
            if let Some(s) = comp.to_str() {
                buf_n += buf.write(s.as_bytes())?
            } else {
                panic!("illegal path component: {:?}", comp)
            }
        }
        assert_eq!(buf.len(), buf_n);
        blob_from_read(w, &mut buf.as_slice(), buf_n)
    } else {
        let mut f = File::open(path.as_ref())?;
        let metadata_n = f.metadata()?.len() as usize;
        blob_from_read(w, &mut f, metadata_n)
    }
}

pub fn tree_from_entries<'e, W, I>(w: &mut W, entries: I) -> Result<usize>
where
    W: Write,
    I: Iterator<Item = &'e TreeEntry>,
{
    let mut buf = Vec::new();
    let mut buf_n = 0;
    for entry in entries {
        buf_n += entry.bytes(&mut buf)?
    }
    assert_eq!(buf.len(), buf_n);
    let mut n = 0;
    n += object_header(w, b"tree", buf_n)?;
    n += w.write(buf.as_slice())?;
    Ok(n)
}

fn tree_entry<W>(w: &mut W, file_mode: FileMode, name: &str, digest: &[u8]) -> Result<usize>
where
    W: Write,
{
    let mut n = 0;
    n += w.write(format!("{:o}", file_mode.0).as_bytes())?;
    n += w.write(b" ")?;
    n += w.write(name.as_bytes())?;
    n += w.write(b"\0")?;
    n += w.write(digest)?;
    Ok(n)
}

#[derive(Clone)]
pub struct TreeEntry {
    pub file_mode: FileMode,
    pub name: String,
    pub digest: Vec<u8>,
}

impl TreeEntry {
    pub fn new(file_mode: FileMode, name: String, digest: Vec<u8>) -> TreeEntry {
        TreeEntry { file_mode, name, digest }
    }

    fn bytes<W>(&self, w: &mut W) -> Result<usize>
    where
        W: Write,
    {
        tree_entry(w, self.file_mode, self.name.as_str(), self.digest.as_slice())
    }
}

impl Debug for TreeEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.write_str(
            format!("{:06o} {}\t{}", self.file_mode.0, to_hex_string(self.digest.as_slice()), self.name).as_str(),
        )?;
        Ok(())
    }
}
