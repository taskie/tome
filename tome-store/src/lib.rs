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

#[cfg(test)]
mod tests {
    use super::*;
    use local::LocalStorage;

    // ── blob_path tests ─────────────────────────────────────────────────

    #[test]
    fn test_blob_path_normal() {
        let hex = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert_eq!(storage::blob_path(hex), format!("objects/2c/f2/{}", hex));
    }

    #[test]
    fn test_blob_path_short_hex() {
        assert_eq!(storage::blob_path("ab"), "objects/ab");
        assert_eq!(storage::blob_path("abc"), "objects/abc");
    }

    #[test]
    fn test_blob_path_exactly_four_chars() {
        assert_eq!(storage::blob_path("abcd"), "objects/ab/cd/abcd");
    }

    // ── LocalStorage tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_local_storage_upload_download() {
        let store_dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(store_dir.path().to_path_buf());

        let src_dir = tempfile::tempdir().unwrap();
        let src_file = src_dir.path().join("source.bin");
        std::fs::write(&src_file, b"upload test content").unwrap();

        storage.upload("test/file.bin", &src_file).await.unwrap();
        assert!(storage.exists("test/file.bin").await.unwrap());

        let dst_file = src_dir.path().join("downloaded.bin");
        storage.download("test/file.bin", &dst_file).await.unwrap();
        assert_eq!(std::fs::read(&dst_file).unwrap(), b"upload test content");
    }

    #[tokio::test]
    async fn test_local_storage_exists_nonexistent() {
        let store_dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(store_dir.path().to_path_buf());
        assert!(!storage.exists("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_local_storage_delete() {
        let store_dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(store_dir.path().to_path_buf());

        let src_dir = tempfile::tempdir().unwrap();
        let src_file = src_dir.path().join("to_delete.bin");
        std::fs::write(&src_file, b"delete me").unwrap();

        storage.upload("deletable.bin", &src_file).await.unwrap();
        assert!(storage.exists("deletable.bin").await.unwrap());

        storage.delete("deletable.bin").await.unwrap();
        assert!(!storage.exists("deletable.bin").await.unwrap());
    }

    #[tokio::test]
    async fn test_local_storage_list() {
        let store_dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(store_dir.path().to_path_buf());

        let src_dir = tempfile::tempdir().unwrap();
        let src_file = src_dir.path().join("data.bin");
        std::fs::write(&src_file, b"data").unwrap();

        storage.upload("objects/aa/file1.bin", &src_file).await.unwrap();
        storage.upload("objects/aa/file2.bin", &src_file).await.unwrap();
        storage.upload("objects/bb/file3.bin", &src_file).await.unwrap();

        let mut items = storage.list("objects/aa").await.unwrap();
        items.sort();
        assert_eq!(items.len(), 2);
        assert!(items[0].ends_with("file1.bin"));
        assert!(items[1].ends_with("file2.bin"));
    }

    #[tokio::test]
    async fn test_local_storage_list_empty_prefix() {
        let store_dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(store_dir.path().to_path_buf());
        let items = storage.list("nonexistent/prefix").await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_local_storage_upload_creates_parent_dirs() {
        let store_dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(store_dir.path().to_path_buf());

        let src_dir = tempfile::tempdir().unwrap();
        let src_file = src_dir.path().join("nested.bin");
        std::fs::write(&src_file, b"nested content").unwrap();

        // Deeply nested path — parent dirs should be created automatically.
        storage.upload("a/b/c/d/e.bin", &src_file).await.unwrap();
        assert!(storage.exists("a/b/c/d/e.bin").await.unwrap());

        let dst = src_dir.path().join("out.bin");
        storage.download("a/b/c/d/e.bin", &dst).await.unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"nested content");
    }
}
