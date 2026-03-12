use std::str::FromStr;

/// Hash computation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashMode {
    /// Git object format.
    /// blob: `"blob {size}\0{content}"`, tree: `"{mode} {name}\0{hash_bytes}"`
    /// Default hash: SHA-1
    Git,

    /// Native treblo format (tree-hash-spec compliant).
    /// Entry: `type_byte || name(UTF-8) || b'\x00' || hash(N bytes)`
    /// Sorted by `(type_byte, name_bytes)` — directories (`D`) before files (`F`).
    /// Default hash: BLAKE3
    #[default]
    Native,
}

impl HashMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Native => "native",
        }
    }
}

impl std::fmt::Display for HashMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HashMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "git" => Ok(Self::Git),
            "native" => Ok(Self::Native),
            other => Err(format!("unknown hash mode {:?}; expected git or native", other)),
        }
    }
}

/// Hash algorithm used for both content hashing and tree hashing.
///
/// Within a single tree computation, the same algorithm is applied to
/// both file contents and tree nodes, ensuring a uniform hash length N
/// throughout the entry binary representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-1 (20 bytes) — Git mode default.
    Sha1,
    /// SHA-256 (32 bytes) — valid in both modes.
    Sha256,
    /// xxHash64 (8 bytes) — non-cryptographic, fast.
    XxHash64,
    /// xxHash3-64 (8 bytes) — non-cryptographic, fast.
    XxHash3_64,
    /// BLAKE3 (32 bytes) — Native mode default.
    Blake3,
}

impl HashAlgorithm {
    /// Byte length of the digest produced by this algorithm.
    pub fn digest_len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::XxHash64 => 8,
            Self::XxHash3_64 => 8,
            Self::Blake3 => 32,
        }
    }

    /// Default algorithm for the given mode.
    pub fn default_for(mode: HashMode) -> Self {
        match mode {
            HashMode::Git => Self::Sha1,
            HashMode::Native => Self::Blake3,
        }
    }

    /// Whether this algorithm is valid for the given mode.
    ///
    /// All algorithms are accepted in both modes.  The restriction that existed
    /// in earlier drafts (Native = Blake3/Sha256 only) has been removed so that
    /// callers can freely choose any algorithm regardless of mode.
    pub fn is_valid_for(self, _mode: HashMode) -> bool {
        true
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::XxHash64 => "xxhash64",
            Self::XxHash3_64 => "xxh3-64",
            Self::Blake3 => "blake3",
        }
    }
}

impl std::fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HashAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha1" => Ok(Self::Sha1),
            "sha256" => Ok(Self::Sha256),
            "xxhash64" => Ok(Self::XxHash64),
            "xxh3-64" | "xxh3_64" | "xxh3" => Ok(Self::XxHash3_64),
            "blake3" => Ok(Self::Blake3),
            other => {
                Err(format!("unknown hash algorithm {:?}; expected sha1, sha256, xxhash64, xxh3-64, or blake3", other))
            }
        }
    }
}

/// Configuration for a tree hash computation.
#[derive(Debug, Clone, Copy)]
pub struct HashConfig {
    pub mode: HashMode,
    pub algorithm: HashAlgorithm,
}

impl HashConfig {
    /// Create a config with the default algorithm for `mode`.
    pub fn new(mode: HashMode) -> Self {
        Self { mode, algorithm: HashAlgorithm::default_for(mode) }
    }

    /// Override the algorithm. Returns an error if `algorithm` is not valid for the mode.
    pub fn with_algorithm(mut self, algorithm: HashAlgorithm) -> Result<Self, String> {
        if !algorithm.is_valid_for(self.mode) {
            return Err(format!("algorithm {} is not valid for {} mode", algorithm, self.mode));
        }
        self.algorithm = algorithm;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_mode_roundtrip() {
        assert_eq!("git".parse::<HashMode>().unwrap(), HashMode::Git);
        assert_eq!("native".parse::<HashMode>().unwrap(), HashMode::Native);
        assert!("unknown".parse::<HashMode>().is_err());
    }

    #[test]
    fn test_hash_algorithm_roundtrip() {
        assert_eq!("sha1".parse::<HashAlgorithm>().unwrap(), HashAlgorithm::Sha1);
        assert_eq!("sha256".parse::<HashAlgorithm>().unwrap(), HashAlgorithm::Sha256);
        assert_eq!("xxhash64".parse::<HashAlgorithm>().unwrap(), HashAlgorithm::XxHash64);
        assert_eq!("xxh3-64".parse::<HashAlgorithm>().unwrap(), HashAlgorithm::XxHash3_64);
        assert_eq!("xxh3".parse::<HashAlgorithm>().unwrap(), HashAlgorithm::XxHash3_64);
        assert_eq!("blake3".parse::<HashAlgorithm>().unwrap(), HashAlgorithm::Blake3);
        assert!("unknown".parse::<HashAlgorithm>().is_err());
    }

    #[test]
    fn test_valid_for() {
        // All algorithms are valid for all modes.
        assert!(HashAlgorithm::Sha1.is_valid_for(HashMode::Git));
        assert!(HashAlgorithm::Sha1.is_valid_for(HashMode::Native));
        assert!(HashAlgorithm::Blake3.is_valid_for(HashMode::Native));
        assert!(HashAlgorithm::Blake3.is_valid_for(HashMode::Git));
        assert!(HashAlgorithm::Sha256.is_valid_for(HashMode::Git));
        assert!(HashAlgorithm::Sha256.is_valid_for(HashMode::Native));
        assert!(HashAlgorithm::XxHash64.is_valid_for(HashMode::Git));
        assert!(HashAlgorithm::XxHash64.is_valid_for(HashMode::Native));
    }

    #[test]
    fn test_defaults() {
        assert_eq!(HashAlgorithm::default_for(HashMode::Git), HashAlgorithm::Sha1);
        assert_eq!(HashAlgorithm::default_for(HashMode::Native), HashAlgorithm::Blake3);
    }

    #[test]
    fn test_digest_len() {
        assert_eq!(HashAlgorithm::Sha1.digest_len(), 20);
        assert_eq!(HashAlgorithm::Sha256.digest_len(), 32);
        assert_eq!(HashAlgorithm::XxHash64.digest_len(), 8);
        assert_eq!(HashAlgorithm::XxHash3_64.digest_len(), 8);
        assert_eq!(HashAlgorithm::Blake3.digest_len(), 32);
    }
}
