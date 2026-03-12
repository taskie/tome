use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

use crate::{
    mode::{HashAlgorithm, HashConfig},
    walk::WalkOptions,
};

pub mod tree;
pub use tree::{compute_tree_hash, empty_tree_hash, hash_bytes, hash_file_content};

// ──────────────────────────────────────────────────────────────────────────────
// EntryKind
// ──────────────────────────────────────────────────────────────────────────────

/// Kind of a directory entry in Native-mode tree hashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Regular file — encoded as `b'F'`.
    File,
    /// Directory — encoded as `b'D'`.
    Directory,
}

impl EntryKind {
    /// Byte used as the type prefix in the entry encoding.
    pub fn kind_byte(self) -> u8 {
        match self {
            Self::File => b'F',
            Self::Directory => b'D',
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// TreeEntry / TreeNode / TreeResult
// ──────────────────────────────────────────────────────────────────────────────

/// A single child entry within a directory node.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub kind: EntryKind,
    /// File or directory name (not a full path).
    pub name: String,
    /// Content hash (file) or tree hash (directory).
    pub hash: Vec<u8>,
}

/// Hash of one directory node and its direct children.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Path relative to the walk root; `""` for the root itself.
    pub path: String,
    /// Tree hash of this directory.
    pub hash: Vec<u8>,
    /// Direct children at the time of hashing.
    pub children: Vec<TreeEntry>,
}

/// Result of a `compute_root_hash` call.
#[derive(Debug)]
pub struct TreeResult {
    /// Tree hash of the root directory.
    pub root_hash: Vec<u8>,
    /// Algorithm that was used.
    pub algorithm: HashAlgorithm,
    /// All directory nodes, in bottom-up order (deepest first).
    pub nodes: Vec<TreeNode>,
    pub file_count: usize,
    pub dir_count: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// compute_root_hash
// ──────────────────────────────────────────────────────────────────────────────

/// Walk `root` and compute the Native-mode Merkle tree hash.
///
/// Returns a `TreeResult` containing the root hash, per-directory hashes, and
/// file/directory counts.  `root` must be a directory.
pub fn compute_root_hash(root: &Path, config: &HashConfig, options: &WalkOptions) -> io::Result<TreeResult> {
    if !root.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a directory", root.display())));
    }

    let algorithm = config.algorithm;
    let root = root.canonicalize()?;

    // dir_path → direct children (kind_byte, name, hash)
    type ChildVec = Vec<(u8, String, Vec<u8>)>;
    let mut children_map: BTreeMap<PathBuf, ChildVec> = BTreeMap::new();
    children_map.insert(root.clone(), Vec::new());

    let mut file_count = 0usize;

    // Walk phase: hash all files and register all directories.
    for result in options.build_walk(&root) {
        match result {
            Ok(entry) => {
                let path = entry.path().to_owned();
                let Some(ft) = entry.file_type() else {
                    continue;
                };

                if ft.is_dir() {
                    // Ensure the directory has a slot, even if empty.
                    children_map.entry(path).or_default();
                } else if ft.is_file() {
                    let content_hash = hash_file_content(&path, algorithm)
                        .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    if let Some(parent) = path.parent() {
                        children_map.entry(parent.to_owned()).or_default().push((b'F', name, content_hash));
                    }
                    file_count += 1;
                }
                // Symlinks are skipped in Phase 1.
            }
            Err(e) => {
                tracing::warn!("walk error: {}", e);
            }
        }
    }

    // Bottom-up phase: compute tree hashes from deepest to shallowest.
    //
    // Collect all known directory paths and sort by depth descending so that
    // child directories are processed before their parents.
    let mut dir_paths: Vec<PathBuf> = children_map.keys().cloned().collect();
    dir_paths.sort_by(|a, b| {
        let da = a.components().count();
        let db = b.components().count();
        db.cmp(&da).then_with(|| a.cmp(b))
    });

    let mut nodes: Vec<TreeNode> = Vec::new();
    let mut dir_count = 0usize;

    for dir_path in &dir_paths {
        let children = match children_map.remove(dir_path) {
            Some(c) => c,
            None => continue, // already removed
        };

        // Skip empty directories unless explicitly included (root is always kept).
        if children.is_empty() && !options.include_empty_dirs && *dir_path != root {
            continue;
        }

        let refs: Vec<(u8, &str, &[u8])> = children.iter().map(|(k, n, h)| (*k, n.as_str(), h.as_slice())).collect();
        let tree_hash = compute_tree_hash(&refs, algorithm);

        // Relative path from root ("" for root itself).
        let rel_path = dir_path.strip_prefix(&root).unwrap_or(dir_path).to_string_lossy().to_string();

        let tree_children: Vec<TreeEntry> = children
            .into_iter()
            .map(|(k, name, hash)| TreeEntry {
                kind: if k == b'D' { EntryKind::Directory } else { EntryKind::File },
                name,
                hash,
            })
            .collect();

        nodes.push(TreeNode { path: rel_path, hash: tree_hash.clone(), children: tree_children });
        dir_count += 1;

        // Propagate this directory's hash to the parent entry.
        if let Some(parent) = dir_path.parent() {
            if let Some(parent_children) = children_map.get_mut(parent) {
                let dir_name = dir_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                parent_children.push((b'D', dir_name, tree_hash));
            }
        }
    }

    // The root node was processed last in bottom-up order; its hash is the result.
    let root_hash =
        nodes.iter().find(|n| n.path.is_empty()).map(|n| n.hash.clone()).unwrap_or_else(|| empty_tree_hash(algorithm));

    Ok(TreeResult { root_hash, algorithm, nodes, file_count, dir_count })
}

// ──────────────────────────────────────────────────────────────────────────────
// compute_tree_from_entries
// ──────────────────────────────────────────────────────────────────────────────

/// Compute a Native-mode tree hash from externally-supplied entries.
///
/// The caller is responsible for ensuring that all `entry.hash` lengths match
/// `config.algorithm.digest_len()`.  Returns an error if they do not.
pub fn compute_tree_from_entries(entries: &[TreeEntry], config: &HashConfig) -> Result<Vec<u8>, String> {
    let expected_len = config.algorithm.digest_len();
    for entry in entries {
        if entry.hash.len() != expected_len {
            return Err(format!(
                "entry {:?}: hash length {} does not match algorithm {} (expected {})",
                entry.name,
                entry.hash.len(),
                config.algorithm,
                expected_len
            ));
        }
    }

    let refs: Vec<(u8, &str, &[u8])> =
        entries.iter().map(|e| (e.kind.kind_byte(), e.name.as_str(), e.hash.as_slice())).collect();
    Ok(compute_tree_hash(&refs, config.algorithm))
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_tree(tmp: &TempDir) -> PathBuf {
        let root = tmp.path().to_owned();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("README.md"), b"# Test").unwrap();
        fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();
        root
    }

    #[test]
    fn test_compute_root_hash_returns_32_bytes_blake3() {
        let tmp = TempDir::new().unwrap();
        let root = setup_tree(&tmp);
        let config = HashConfig::new(crate::mode::HashMode::Native);
        let options = WalkOptions::default();

        let result = compute_root_hash(&root, &config, &options).unwrap();

        assert_eq!(result.algorithm, HashAlgorithm::Blake3);
        assert_eq!(result.root_hash.len(), 32);
        assert_eq!(result.file_count, 2);
        assert_eq!(result.dir_count, 2); // root + src
    }

    #[test]
    fn test_compute_root_hash_deterministic() {
        let tmp = TempDir::new().unwrap();
        let root = setup_tree(&tmp);
        let config = HashConfig::new(crate::mode::HashMode::Native);
        let options = WalkOptions::default();

        let r1 = compute_root_hash(&root, &config, &options).unwrap();
        let r2 = compute_root_hash(&root, &config, &options).unwrap();

        assert_eq!(r1.root_hash, r2.root_hash);
    }

    #[test]
    fn test_compute_root_hash_changes_on_file_change() {
        let tmp = TempDir::new().unwrap();
        let root = setup_tree(&tmp);
        let config = HashConfig::new(crate::mode::HashMode::Native);
        let options = WalkOptions::default();

        let r1 = compute_root_hash(&root, &config, &options).unwrap();
        fs::write(root.join("README.md"), b"# Modified").unwrap();
        let r2 = compute_root_hash(&root, &config, &options).unwrap();

        assert_ne!(r1.root_hash, r2.root_hash);
    }

    #[test]
    fn test_compute_root_hash_sha256() {
        let tmp = TempDir::new().unwrap();
        let root = setup_tree(&tmp);
        let config = HashConfig::new(crate::mode::HashMode::Native).with_algorithm(HashAlgorithm::Sha256).unwrap();
        let options = WalkOptions::default();

        let result = compute_root_hash(&root, &config, &options).unwrap();

        assert_eq!(result.algorithm, HashAlgorithm::Sha256);
        assert_eq!(result.root_hash.len(), 32);
    }

    #[test]
    fn test_hash_config_all_algorithms_valid() {
        let config = HashConfig::new(crate::mode::HashMode::Native);
        // All algorithms are now valid for all modes.
        assert!(config.with_algorithm(HashAlgorithm::XxHash64).is_ok());
        assert!(config.with_algorithm(HashAlgorithm::Sha1).is_ok());
        assert!(config.with_algorithm(HashAlgorithm::Sha256).is_ok());
        assert!(config.with_algorithm(HashAlgorithm::Blake3).is_ok());
    }

    #[test]
    fn test_compute_tree_from_entries_valid() {
        let config = HashConfig::new(crate::mode::HashMode::Native);
        let entries = vec![TreeEntry { kind: EntryKind::File, name: "foo.txt".to_string(), hash: vec![0u8; 32] }];
        assert!(compute_tree_from_entries(&entries, &config).is_ok());
    }

    #[test]
    fn test_compute_tree_from_entries_wrong_hash_len() {
        let config = HashConfig::new(crate::mode::HashMode::Native); // Blake3 → 32 bytes
        let entries = vec![TreeEntry {
            kind: EntryKind::File,
            name: "foo.txt".to_string(),
            hash: vec![0u8; 20], // SHA-1 length — wrong
        }];
        assert!(compute_tree_from_entries(&entries, &config).is_err());
    }

    #[test]
    fn test_not_a_directory_error() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("file.txt");
        fs::write(&file, b"content").unwrap();

        let config = HashConfig::new(crate::mode::HashMode::Native);
        let options = WalkOptions::default();
        let err = compute_root_hash(&file, &config, &options).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_empty_root_directory() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_owned();
        let config = HashConfig::new(crate::mode::HashMode::Native);
        let options = WalkOptions::default();

        let result = compute_root_hash(&root, &config, &options).unwrap();

        // Empty root → H(b"")
        assert_eq!(result.root_hash, empty_tree_hash(HashAlgorithm::Blake3));
        assert_eq!(result.file_count, 0);
        assert_eq!(result.dir_count, 1); // root itself
    }
}
