use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH error: {0}")]
    Ssh(#[from] ssh2::Error),

    #[error("AWS S3 error: {0}")]
    AwsS3(String),

    #[error("encryption error: {0}")]
    Encryption(String),

    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("key source error: {0}")]
    KeySource(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;
