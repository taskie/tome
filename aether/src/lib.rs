pub mod algorithm;
pub mod cipher;
pub mod error;
pub mod header;
mod parallel;

pub use algorithm::CipherAlgorithm;
pub use cipher::Cipher;
pub use error::AetherError;
pub use header::{
    ChunkKind, HEADER_SIZE, Header, HeaderFlags, Integrity, KEY_SIZE, KdfParams, KeyBlock, KeySlot, read_kdf_params,
};
