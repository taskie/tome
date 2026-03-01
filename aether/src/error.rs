/// Errors produced by the aether encryption library.
#[derive(Debug, thiserror::Error)]
pub enum AetherError {
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("decryption failed: {0}")]
    Decryption(String),

    #[error("integrity check failed")]
    IntegrityMismatch,

    #[error("KDF error: {0}")]
    Kdf(String),

    #[error("base64 decode error: {0}")]
    Base64(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AetherError>;
