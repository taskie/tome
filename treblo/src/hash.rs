use std::{
    hash::Hasher,
    io::{self, Read},
    path::Path,
    str::FromStr,
};

use sha2::{Digest as _, Sha256};
use twox_hash::{XxHash3_64, XxHash64};

pub const DIGEST_SIZE: usize = 32;

/// SHA-256 / BLAKE3 digest as 32 raw bytes.
pub type Digest256 = [u8; DIGEST_SIZE];

// ──────────────────────────────────────────────────────────────────────────────
// DigestAlgorithm
// ──────────────────────────────────────────────────────────────────────────────

/// Supported digest algorithms.  Both produce 32-byte output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DigestAlgorithm {
    #[default]
    Sha256,
    Blake3,
}

impl DigestAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Blake3 => "blake3",
        }
    }
}

impl std::fmt::Display for DigestAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DigestAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha256" => Ok(Self::Sha256),
            "blake3" => Ok(Self::Blake3),
            other => Err(format!("unknown digest algorithm {:?}; expected sha256 or blake3", other)),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// FastHashAlgorithm
// ──────────────────────────────────────────────────────────────────────────────

/// Supported fast-digest algorithms used for quick change detection.
/// The result is stored as an `i64` in the database (bit-cast from `u64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastHashAlgorithm {
    #[default]
    XxHash64,
    XxHash3_64,
}

impl FastHashAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::XxHash64 => "xxhash64",
            Self::XxHash3_64 => "xxh3-64",
        }
    }
}

impl std::fmt::Display for FastHashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FastHashAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "xxhash64" => Ok(Self::XxHash64),
            "xxh3-64" | "xxh3_64" | "xxh3" => Ok(Self::XxHash3_64),
            other => Err(format!("unknown fast hash algorithm {:?}; expected xxhash64 or xxh3-64", other)),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Low-level hash helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Hex-encode a digest.
pub fn hex_encode(digest: &[u8]) -> String {
    crate::hex::to_hex_string(digest)
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

/// Compute xxHash3-64 of a reader.
pub fn xxhash3_64_reader<R: Read>(mut reader: R) -> io::Result<u64> {
    let mut hasher = XxHash3_64::with_seed(0);
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

/// Compute xxHash3-64 of bytes.
pub fn xxhash3_64_bytes(data: &[u8]) -> u64 {
    let mut hasher = XxHash3_64::with_seed(0);
    hasher.write(data);
    hasher.finish()
}

// ──────────────────────────────────────────────────────────────────────────────
// FileHash
// ──────────────────────────────────────────────────────────────────────────────

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
    pub fast_digest: i64,  // xxHash64 or xxHash3-64 stored as i64 (bit-cast from u64)
    pub digest: Digest256, // SHA-256 or BLAKE3
}

impl FileHash {
    pub fn digest_hex(&self) -> String {
        hex_encode(&self.digest)
    }

    pub fn fast_digest_u64(&self) -> u64 {
        self.fast_digest as u64
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// hash_file
// ──────────────────────────────────────────────────────────────────────────────

/// Compute the full two-stage hash for a file at `path`.
///
/// Stage 1: `fast_algo` (xxHash64 or xxHash3-64 — used for quick change detection)
/// Stage 2: `algo` (SHA-256 or BLAKE3 — content-addressed identity)
///
/// Both hashes are computed in a single pass for efficiency.
pub fn hash_file(path: &Path, algo: DigestAlgorithm, fast_algo: FastHashAlgorithm) -> io::Result<FileHash> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let size = meta.len();
    let mut reader = io::BufReader::new(file);
    let mut buf = [0u8; 8192];

    // Build a boxed fast hasher (allocation is per-file, not per-chunk).
    let mut fast_hasher: Box<dyn std::hash::Hasher> = match fast_algo {
        FastHashAlgorithm::XxHash64 => Box::new(XxHash64::with_seed(0)),
        FastHashAlgorithm::XxHash3_64 => Box::new(XxHash3_64::with_seed(0)),
    };

    let digest: Digest256 = match algo {
        DigestAlgorithm::Sha256 => {
            let mut sha_hasher = Sha256::new();
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                fast_hasher.write(&buf[..n]);
                sha_hasher.update(&buf[..n]);
            }
            sha_hasher.finalize().into()
        }
        DigestAlgorithm::Blake3 => {
            let mut b3_hasher = blake3::Hasher::new();
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                fast_hasher.write(&buf[..n]);
                b3_hasher.update(&buf[..n]);
            }
            *b3_hasher.finalize().as_bytes()
        }
    };

    let fast_digest = fast_hasher.finish() as i64;
    Ok(FileHash { size, fast_digest, digest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_sha256_bytes_known_value() {
        let digest = sha256_bytes(b"hello");
        let hex = hex_encode(&digest);
        assert_eq!(hex, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_sha256_empty() {
        let digest = sha256_bytes(b"");
        let hex = hex_encode(&digest);
        assert_eq!(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_sha256_reader_matches_bytes() {
        let data = b"test data for sha256 reader";
        let from_bytes = sha256_bytes(data);
        let from_reader = sha256_reader(Cursor::new(data)).unwrap();
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn test_xxhash64_deterministic() {
        let a = xxhash64_bytes(b"deterministic");
        let b = xxhash64_bytes(b"deterministic");
        assert_eq!(a, b);
    }

    #[test]
    fn test_xxhash64_different_inputs() {
        let a = xxhash64_bytes(b"input_a");
        let b = xxhash64_bytes(b"input_b");
        assert_ne!(a, b);
    }

    #[test]
    fn test_xxhash64_reader_matches_bytes() {
        let data = b"test data for xxhash64 reader";
        let from_bytes = xxhash64_bytes(data);
        let from_reader = xxhash64_reader(Cursor::new(data)).unwrap();
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn test_hash_file_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let content = b"hash_file test content";
        std::fs::write(&path, content).unwrap();

        let fh = hash_file(&path, DigestAlgorithm::Sha256, FastHashAlgorithm::XxHash64).unwrap();
        assert_eq!(fh.size, content.len() as u64);
        assert_eq!(fh.digest, sha256_bytes(content));
        assert_eq!(fh.fast_digest_u64(), xxhash64_bytes(content));
    }

    #[test]
    fn test_hash_file_blake3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let content = b"hash_file blake3 test content";
        std::fs::write(&path, content).unwrap();

        let fh = hash_file(&path, DigestAlgorithm::Blake3, FastHashAlgorithm::XxHash64).unwrap();
        assert_eq!(fh.size, content.len() as u64);
        let expected: Digest256 = *blake3::hash(content).as_bytes();
        assert_eq!(fh.digest, expected);
        assert_eq!(fh.fast_digest_u64(), xxhash64_bytes(content));
    }

    #[test]
    fn test_hash_file_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.bin");
        std::fs::write(&path, b"").unwrap();

        let fh = hash_file(&path, DigestAlgorithm::Sha256, FastHashAlgorithm::XxHash64).unwrap();
        assert_eq!(fh.size, 0);
        assert_eq!(fh.digest, sha256_bytes(b""));
    }

    #[test]
    fn test_hash_file_xxh3_64() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let content = b"xxh3-64 fast digest test";
        std::fs::write(&path, content).unwrap();

        let fh = hash_file(&path, DigestAlgorithm::Sha256, FastHashAlgorithm::XxHash3_64).unwrap();
        assert_eq!(fh.size, content.len() as u64);
        assert_eq!(fh.digest, sha256_bytes(content));
        assert_eq!(fh.fast_digest_u64(), xxhash3_64_bytes(content));
        // xxh3-64 and xxhash64 should differ for non-trivial input.
        assert_ne!(fh.fast_digest_u64(), xxhash64_bytes(content));
    }

    #[test]
    fn test_xxhash3_64_deterministic() {
        let a = xxhash3_64_bytes(b"deterministic");
        let b = xxhash3_64_bytes(b"deterministic");
        assert_eq!(a, b);
    }

    #[test]
    fn test_xxhash3_64_different_inputs() {
        let a = xxhash3_64_bytes(b"input_a");
        let b = xxhash3_64_bytes(b"input_b");
        assert_ne!(a, b);
    }

    #[test]
    fn test_xxhash3_64_reader_matches_bytes() {
        let data = b"test data for xxhash3-64 reader";
        let from_bytes = xxhash3_64_bytes(data);
        let from_reader = xxhash3_64_reader(Cursor::new(data)).unwrap();
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn test_fast_hash_algorithm_roundtrip() {
        assert_eq!("xxhash64".parse::<FastHashAlgorithm>().unwrap(), FastHashAlgorithm::XxHash64);
        assert_eq!("xxh3-64".parse::<FastHashAlgorithm>().unwrap(), FastHashAlgorithm::XxHash3_64);
        assert_eq!("xxh3".parse::<FastHashAlgorithm>().unwrap(), FastHashAlgorithm::XxHash3_64);
        assert!("unknown".parse::<FastHashAlgorithm>().is_err());
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00, 0xff, 0x0a, 0xbc]), "00ff0abc");
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn test_digest_algorithm_roundtrip() {
        assert_eq!("sha256".parse::<DigestAlgorithm>().unwrap(), DigestAlgorithm::Sha256);
        assert_eq!("blake3".parse::<DigestAlgorithm>().unwrap(), DigestAlgorithm::Blake3);
        assert!("unknown".parse::<DigestAlgorithm>().is_err());
    }
}
