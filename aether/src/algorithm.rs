/// Supported AEAD algorithms.
///
/// All use 32-byte keys and 16-byte tags.
/// Nonce sizes: AES-256-GCM and ChaCha20-Poly1305 use 12 bytes; XChaCha20-Poly1305 uses 24 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CipherAlgorithm {
    #[default]
    Aes256Gcm,
    ChaCha20Poly1305,
    XChaCha20Poly1305,
}

impl CipherAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aes256gcm",
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
            Self::XChaCha20Poly1305 => "xchacha20-poly1305",
        }
    }

    /// Nonce size in bytes for this algorithm.
    pub fn nonce_size(self) -> usize {
        match self {
            Self::Aes256Gcm | Self::ChaCha20Poly1305 => 12,
            Self::XChaCha20Poly1305 => 24,
        }
    }

    /// Encode to the 4-bit algorithm field (bits [3:0] of flags).
    pub(crate) fn to_bits(self) -> u16 {
        match self {
            Self::Aes256Gcm => 0,
            Self::ChaCha20Poly1305 => 1,
            Self::XChaCha20Poly1305 => 2,
        }
    }

    /// Decode from the 4-bit algorithm field.
    pub(crate) fn from_bits(bits: u16) -> Result<Self, crate::error::AetherError> {
        match bits & 0x000F {
            0 => Ok(Self::Aes256Gcm),
            1 => Ok(Self::ChaCha20Poly1305),
            2 => Ok(Self::XChaCha20Poly1305),
            other => Err(crate::error::AetherError::InvalidHeader(format!("unknown algorithm: {other}"))),
        }
    }
}

impl std::fmt::Display for CipherAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CipherAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "aes256gcm" | "aes-256-gcm" | "aes" => Ok(Self::Aes256Gcm),
            "chacha20-poly1305" | "chacha20poly1305" | "chacha20" => Ok(Self::ChaCha20Poly1305),
            "xchacha20-poly1305" | "xchacha20poly1305" | "xchacha20" => Ok(Self::XChaCha20Poly1305),
            other => Err(format!(
                "unknown cipher algorithm {:?}; expected aes256gcm, chacha20-poly1305, or xchacha20-poly1305",
                other
            )),
        }
    }
}
