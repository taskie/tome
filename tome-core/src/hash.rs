use std::{
    hash::Hasher,
    io::{self, Read},
    path::Path,
    str::FromStr,
};

use sha2::{Digest, Sha256};
use twox_hash::XxHash64;

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
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha256" => Ok(Self::Sha256),
            "blake3" => Ok(Self::Blake3),
            other => anyhow::bail!("unknown digest algorithm {:?}; expected sha256 or blake3", other),
        }
    }
}

// Implement clap::ValueEnum so DigestAlgorithm can be used as a CLI arg directly.
impl clap::ValueEnum for DigestAlgorithm {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Sha256, Self::Blake3]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(self.as_str()))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Low-level hash helpers
// ──────────────────────────────────────────────────────────────────────────────

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
    pub fast_digest: i64,  // xxHash64 stored as i64 (bit-cast)
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
/// Stage 1: xxHash64 (fast — used for quick change detection)
/// Stage 2: SHA-256 or BLAKE3 (slow — content-addressed identity)
///
/// Both hashes are computed in a single pass for efficiency.
pub fn hash_file(path: &Path, algo: DigestAlgorithm) -> io::Result<FileHash> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    let size = meta.len();
    let mut reader = io::BufReader::new(file);
    let mut buf = [0u8; 8192];
    let mut xx_hasher = XxHash64::with_seed(0);

    let digest: Digest256 = match algo {
        DigestAlgorithm::Sha256 => {
            let mut sha_hasher = Sha256::new();
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                xx_hasher.write(&buf[..n]);
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
                xx_hasher.write(&buf[..n]);
                b3_hasher.update(&buf[..n]);
            }
            *b3_hasher.finalize().as_bytes()
        }
    };

    let fast_digest = xx_hasher.finish() as i64;
    Ok(FileHash { size, fast_digest, digest })
}
