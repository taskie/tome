use std::{io, path::Path};

use crate::mode::HashAlgorithm;

/// Hash raw bytes with the given algorithm.
pub fn hash_bytes(data: &[u8], algorithm: HashAlgorithm) -> Vec<u8> {
    match algorithm {
        HashAlgorithm::Blake3 => blake3::hash(data).as_bytes().to_vec(),
        HashAlgorithm::Sha256 => {
            use digest::Digest;
            sha2::Sha256::digest(data).to_vec()
        }
        HashAlgorithm::Sha1 => {
            use digest::Digest;
            sha1::Sha1::digest(data).to_vec()
        }
        HashAlgorithm::XxHash64 => {
            use std::hash::Hasher as _;
            let mut h = twox_hash::XxHash64::with_seed(0);
            h.write(data);
            h.finish().to_le_bytes().to_vec()
        }
        HashAlgorithm::XxHash3_64 => {
            use std::hash::Hasher as _;
            let mut h = twox_hash::XxHash3_64::with_seed(0);
            h.write(data);
            h.finish().to_le_bytes().to_vec()
        }
    }
}

/// Hash raw file content (no prefix) with the given algorithm.
pub fn hash_file_content(path: &Path, algorithm: HashAlgorithm) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(file);
    let mut buf = [0u8; 8192];

    match algorithm {
        HashAlgorithm::Blake3 => {
            let mut hasher = blake3::Hasher::new();
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(hasher.finalize().as_bytes().to_vec())
        }
        HashAlgorithm::Sha256 => {
            use digest::Digest;
            let mut hasher = sha2::Sha256::new();
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(hasher.finalize().to_vec())
        }
        HashAlgorithm::Sha1 => {
            use digest::Digest;
            let mut hasher = sha1::Sha1::new();
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(hasher.finalize().to_vec())
        }
        HashAlgorithm::XxHash64 => {
            use std::hash::Hasher as _;
            let mut hasher = twox_hash::XxHash64::with_seed(0);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.write(&buf[..n]);
            }
            Ok(hasher.finish().to_le_bytes().to_vec())
        }
        HashAlgorithm::XxHash3_64 => {
            use std::hash::Hasher as _;
            let mut hasher = twox_hash::XxHash3_64::with_seed(0);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.write(&buf[..n]);
            }
            Ok(hasher.finish().to_le_bytes().to_vec())
        }
    }
}

/// Result of tree hash computation, including metadata for storage.
pub struct TreeHashResult {
    /// The tree digest (SHA-256 or BLAKE3).
    pub digest: Vec<u8>,
    /// Size of the serialized tree content (before hashing).
    pub size: u64,
    /// xxHash64 fast digest of the serialized tree content.
    pub fast_digest: i64,
}

/// Compute a Native-mode tree hash from `(kind_byte, name, hash)` tuples.
///
/// Entry encoding:
/// ```text
/// kind_byte || name_bytes(UTF-8) || b'\x00' || hash(N bytes)
/// ```
///
/// Entries are sorted lexicographically by their full byte encoding.
/// Because `b'D' < b'F'`, directories always appear before files with the same name.
/// Empty children list returns `H(b"")` (empty-tree hash).
pub fn compute_tree_hash(children: &[(u8, &str, &[u8])], algorithm: HashAlgorithm) -> TreeHashResult {
    let data = serialize_tree_entries(children);

    let digest = hash_bytes(&data, algorithm);
    let size = data.len() as u64;
    let fast_digest = {
        use std::hash::Hasher as _;
        let mut h = twox_hash::XxHash64::with_seed(0);
        h.write(&data);
        h.finish() as i64
    };

    TreeHashResult { digest, size, fast_digest }
}

/// Serialize tree children into the canonical sorted byte representation.
fn serialize_tree_entries(children: &[(u8, &str, &[u8])]) -> Vec<u8> {
    if children.is_empty() {
        return b"".to_vec();
    }

    let mut entry_bytes: Vec<Vec<u8>> = children
        .iter()
        .map(|(kind, name, hash)| {
            let mut buf = Vec::with_capacity(1 + name.len() + 1 + hash.len());
            buf.push(*kind);
            buf.extend_from_slice(name.as_bytes());
            buf.push(0x00);
            buf.extend_from_slice(hash);
            buf
        })
        .collect();

    entry_bytes.sort();

    let total_len: usize = entry_bytes.iter().map(|e| e.len()).sum();
    let mut data = Vec::with_capacity(total_len);
    for eb in &entry_bytes {
        data.extend_from_slice(eb);
    }

    data
}

/// Hash of an empty tree: `H(b"")`.
pub fn empty_tree_hash(algorithm: HashAlgorithm) -> Vec<u8> {
    hash_bytes(b"", algorithm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hex::to_hex_string, mode::HashAlgorithm};

    #[test]
    fn test_hash_bytes_blake3_known() {
        let h = hash_bytes(b"hello", HashAlgorithm::Blake3);
        assert_eq!(h, blake3::hash(b"hello").as_bytes().to_vec());
    }

    #[test]
    fn test_hash_bytes_sha256_known() {
        let h = hash_bytes(b"hello", HashAlgorithm::Sha256);
        assert_eq!(to_hex_string(&h), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_empty_tree_hash_blake3() {
        let h = empty_tree_hash(HashAlgorithm::Blake3);
        assert_eq!(to_hex_string(&h), "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262");
    }

    #[test]
    fn test_empty_tree_hash_sha256() {
        let h = empty_tree_hash(HashAlgorithm::Sha256);
        assert_eq!(to_hex_string(&h), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_tree_hash_single_file() {
        let hash = vec![0u8; 32];
        let result = compute_tree_hash(&[(b'F', "foo.txt", &hash)], HashAlgorithm::Blake3);

        // Expected: blake3(b'F' || "foo.txt" || b'\x00' || hash)
        let mut entry = Vec::new();
        entry.push(b'F');
        entry.extend_from_slice(b"foo.txt");
        entry.push(0x00);
        entry.extend_from_slice(&hash);
        let expected = blake3::hash(&entry).as_bytes().to_vec();

        assert_eq!(result.digest, expected);
    }

    #[test]
    fn test_compute_tree_hash_dirs_before_files() {
        // "bar" directory should come before "aaa.txt" file because b'D' < b'F'
        let hash = vec![1u8; 32];
        let result = compute_tree_hash(&[(b'F', "aaa.txt", &hash), (b'D', "bar", &hash)], HashAlgorithm::Blake3);

        let mut dir_entry = Vec::new();
        dir_entry.push(b'D');
        dir_entry.extend_from_slice(b"bar");
        dir_entry.push(0x00);
        dir_entry.extend_from_slice(&hash);

        let mut file_entry = Vec::new();
        file_entry.push(b'F');
        file_entry.extend_from_slice(b"aaa.txt");
        file_entry.push(0x00);
        file_entry.extend_from_slice(&hash);

        let mut data = Vec::new();
        data.extend_from_slice(&dir_entry);
        data.extend_from_slice(&file_entry);
        let expected = blake3::hash(&data).as_bytes().to_vec();

        assert_eq!(result.digest, expected);
    }

    #[test]
    fn test_compute_tree_hash_empty_is_empty_hash() {
        let result = compute_tree_hash(&[], HashAlgorithm::Blake3);
        assert_eq!(result.digest, empty_tree_hash(HashAlgorithm::Blake3));
    }

    #[test]
    fn test_compute_tree_hash_sha256() {
        let hash = vec![0u8; 32];
        let result = compute_tree_hash(&[(b'F', "README.md", &hash)], HashAlgorithm::Sha256);

        let mut entry = Vec::new();
        entry.push(b'F');
        entry.extend_from_slice(b"README.md");
        entry.push(0x00);
        entry.extend_from_slice(&hash);

        use digest::Digest;
        let expected: Vec<u8> = sha2::Sha256::digest(&entry).to_vec();
        assert_eq!(result.digest, expected);
    }

    #[test]
    fn test_hash_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let content = b"native mode content";
        std::fs::write(&path, content).unwrap();

        let result = hash_file_content(&path, HashAlgorithm::Blake3).unwrap();
        let expected = blake3::hash(content).as_bytes().to_vec();
        assert_eq!(result, expected);
    }
}
