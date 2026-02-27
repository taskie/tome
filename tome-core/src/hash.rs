use std::{
    hash::Hasher,
    io::{self, Read},
    path::Path,
};

use sha2::{Digest, Sha256};
use twox_hash::XxHash64;

pub const DIGEST_SIZE: usize = 32;

/// SHA-256 digest as 32 raw bytes.
pub type Digest256 = [u8; DIGEST_SIZE];

/// Hex-encode a digest.
pub fn hex_encode(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Compute SHA-256 of a reader, returning the 32-byte digest.
pub fn sha256_reader<R: Read>(mut reader: R) -> io::Result<Digest256> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

/// Compute SHA-256 of bytes.
pub fn sha256_bytes(data: &[u8]) -> Digest256 {
    Sha256::digest(data).into()
}

/// Compute xxHash64 of a reader.
pub fn xxhash64_reader<R: Read>(mut reader: R) -> io::Result<u64> {
    let mut hasher = XxHash64::with_seed(0);
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[..n]);
    }
    Ok(hasher.finish())
}

/// Compute xxHash64 of bytes.
pub fn xxhash64_bytes(data: &[u8]) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(data);
    hasher.finish()
}

/// File metadata used for fast-change detection (mtime + size).
#[derive(Debug, Clone, PartialEq)]
pub struct FileMeta {
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
}

impl FileMeta {
    pub fn from_metadata(meta: &std::fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        Some(Self { size: meta.len(), mtime_secs: meta.mtime(), mtime_nanos: meta.mtime_nsec() as u32 })
    }
}

/// Result of hashing a file through the two-stage pipeline.
#[derive(Debug, Clone)]
pub struct FileHash {
    pub size: u64,
    pub fast_digest: i64,  // xxHash64 stored as i64 (bit-cast)
    pub digest: Digest256, // SHA-256
}

impl FileHash {
    pub fn digest_hex(&self) -> String {
        hex_encode(&self.digest)
    }

    pub fn fast_digest_u64(&self) -> u64 {
        self.fast_digest as u64
    }
}

/// Compute the full two-stage hash (xxHash64 + SHA-256) for a file at `path`.
pub fn hash_file(path: &Path) -> io::Result<FileHash> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let size = meta.len();

    // Single pass: compute both hashes simultaneously.
    let mut xx_hasher = XxHash64::with_seed(0);
    let mut sha_hasher = Sha256::new();
    let mut reader = io::BufReader::new(file);
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        xx_hasher.write(&buf[..n]);
        sha_hasher.update(&buf[..n]);
    }

    let fast_digest = xx_hasher.finish() as i64;
    let digest: Digest256 = sha_hasher.finalize().into();

    Ok(FileHash { size, fast_digest, digest })
}
