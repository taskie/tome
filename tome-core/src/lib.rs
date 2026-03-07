pub mod error;
pub mod hash;
pub mod id;
pub mod metadata;
pub mod models;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ── hash tests ──────────────────────────────────────────────────────

    #[test]
    fn test_sha256_bytes_known_value() {
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let digest = hash::sha256_bytes(b"hello");
        let hex = hash::hex_encode(&digest);
        assert_eq!(hex, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_sha256_empty() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let digest = hash::sha256_bytes(b"");
        let hex = hash::hex_encode(&digest);
        assert_eq!(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_sha256_reader_matches_bytes() {
        let data = b"test data for sha256 reader";
        let from_bytes = hash::sha256_bytes(data);
        let from_reader = hash::sha256_reader(Cursor::new(data)).unwrap();
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn test_xxhash64_deterministic() {
        let a = hash::xxhash64_bytes(b"deterministic");
        let b = hash::xxhash64_bytes(b"deterministic");
        assert_eq!(a, b);
    }

    #[test]
    fn test_xxhash64_different_inputs() {
        let a = hash::xxhash64_bytes(b"input_a");
        let b = hash::xxhash64_bytes(b"input_b");
        assert_ne!(a, b);
    }

    #[test]
    fn test_xxhash64_reader_matches_bytes() {
        let data = b"test data for xxhash64 reader";
        let from_bytes = hash::xxhash64_bytes(data);
        let from_reader = hash::xxhash64_reader(Cursor::new(data)).unwrap();
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn test_hash_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let content = b"hash_file test content";
        std::fs::write(&path, content).unwrap();

        let fh = hash::hash_file(&path, hash::DigestAlgorithm::Sha256, hash::FastHashAlgorithm::XxHash64).unwrap();
        assert_eq!(fh.size, content.len() as u64);
        assert_eq!(fh.digest, hash::sha256_bytes(content));
        assert_eq!(fh.fast_digest_u64(), hash::xxhash64_bytes(content));
    }

    #[test]
    fn test_hash_file_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.bin");
        std::fs::write(&path, b"").unwrap();

        let fh = hash::hash_file(&path, hash::DigestAlgorithm::Sha256, hash::FastHashAlgorithm::XxHash64).unwrap();
        assert_eq!(fh.size, 0);
        assert_eq!(fh.digest, hash::sha256_bytes(b""));
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hash::hex_encode(&[0x00, 0xff, 0x0a, 0xbc]), "00ff0abc");
        assert_eq!(hash::hex_encode(&[]), "");
    }

    #[test]
    fn test_file_hash_digest_hex() {
        let fh = hash::FileHash { size: 0, fast_digest: 0, digest: hash::sha256_bytes(b"hello") };
        assert_eq!(fh.digest_hex(), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    // ── id tests ────────────────────────────────────────────────────────

    #[test]
    fn test_next_id_returns_positive() {
        let id = id::next_id().unwrap();
        assert!(id > 0, "generated ID should be positive");
    }

    #[test]
    fn test_next_id_unique() {
        let ids: Vec<i64> = (0..100).map(|_| id::next_id().unwrap()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "all 100 IDs should be unique");
    }

    // ── models tests ────────────────────────────────────────────────────

    #[test]
    fn test_entry_status_roundtrip() {
        assert_eq!(models::EntryStatus::from_i16(0), Some(models::EntryStatus::Deleted));
        assert_eq!(models::EntryStatus::from_i16(1), Some(models::EntryStatus::Present));
        assert_eq!(models::EntryStatus::from_i16(2), None);
        assert_eq!(models::EntryStatus::from_i16(-1), None);

        assert_eq!(models::EntryStatus::Deleted.as_i16(), 0);
        assert_eq!(models::EntryStatus::Present.as_i16(), 1);
    }
}
