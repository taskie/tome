pub mod encrypted;
pub mod error;
pub mod factory;
pub mod local;
pub mod s3;
pub mod ssh;
pub mod storage;

pub use error::StoreError;
pub use factory::open_storage;
pub use storage::Storage;
