use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::{Path, PathBuf},
};

use ignore;
use tracing::warn;

use crate::{
    mode::{HashAlgorithm, HashConfig, HashMode},
    native::tree::{compute_tree_hash, hash_file_content},
    object::{FileMode, TreeEntry, blob_from_path, tree_from_entries},
    path::PathWalkState,
};

// ──────────────────────────────────────────────────────────────────────────────
// Hasher trait + implementations
// ──────────────────────────────────────────────────────────────────────────────

pub trait Hasher: Write {
    fn result_vec(&mut self) -> Vec<u8>;
}

impl Hasher for sha1::Sha1 {
    fn result_vec(&mut self) -> Vec<u8> {
        use digest::Digest;
        self.clone().finalize().to_vec()
    }
}

struct Sha256Holder(sha2::Sha256);

impl Write for Sha256Holder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl Hasher for Sha256Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        use digest::Digest;
        self.0.clone().finalize().to_vec()
    }
}

struct Blake3Holder(blake3::Hasher);

impl Write for Blake3Holder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Hasher for Blake3Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        self.0.finalize().as_bytes().to_vec()
    }
}

struct XxHash64Holder(twox_hash::XxHash64);

impl Write for XxHash64Holder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        use std::hash::Hasher as _;
        self.0.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Hasher for XxHash64Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        use std::hash::Hasher as _;
        self.0.finish().to_le_bytes().to_vec()
    }
}

struct XxHash3_64Holder(twox_hash::XxHash3_64);

impl Write for XxHash3_64Holder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        use std::hash::Hasher as _;
        self.0.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Hasher for XxHash3_64Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        use std::hash::Hasher as _;
        self.0.finish().to_le_bytes().to_vec()
    }
}

/// Create a boxed hasher for the given algorithm.
pub fn make_hasher(algorithm: HashAlgorithm) -> Box<dyn Hasher> {
    match algorithm {
        HashAlgorithm::Sha1 => {
            use digest::Digest;
            Box::new(sha1::Sha1::new())
        }
        HashAlgorithm::Sha256 => {
            use digest::Digest;
            Box::new(Sha256Holder(sha2::Sha256::new()))
        }
        HashAlgorithm::Blake3 => Box::new(Blake3Holder(blake3::Hasher::new())),
        HashAlgorithm::XxHash64 => Box::new(XxHash64Holder(twox_hash::XxHash64::default())),
        HashAlgorithm::XxHash3_64 => Box::new(XxHash3_64Holder(twox_hash::XxHash3_64::default())),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WalkOptions
// ──────────────────────────────────────────────────────────────────────────────

/// Options controlling directory traversal.
#[derive(Debug, Default)]
pub struct WalkOptions {
    /// Disable `.gitignore` / `.trebloignore` pattern filtering.
    pub no_ignore: bool,
    /// Follow symbolic links.
    pub follow_symlinks: bool,
    /// Include directories that contain no files (`H(b"")`).
    /// Only effective when using `compute_root_hash`; `TrebloWalk::walk` does not
    /// visit empty directories.
    pub include_empty_dirs: bool,
}

impl WalkOptions {
    /// Build an [`ignore::Walk`] from these options, sorted by file name for
    /// deterministic output.
    pub fn build_walk(&self, root: &Path) -> ignore::Walk {
        let mut builder = ignore::WalkBuilder::new(root);
        builder.follow_links(self.follow_symlinks);
        if self.no_ignore {
            builder.ignore(false).git_ignore(false).git_global(false).git_exclude(false);
        }
        builder.sort_by_file_name(|a, b| a.cmp(b));
        builder.build()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// TrebloWalk
// ──────────────────────────────────────────────────────────────────────────────

pub struct TrebloWalk {
    pub config: HashConfig,
    pub blob_only: bool,
    pub no_error: bool,
}

impl Default for TrebloWalk {
    fn default() -> Self {
        TrebloWalk { config: HashConfig::new(HashMode::Git), blob_only: false, no_error: false }
    }
}

impl TrebloWalk {
    fn resolve<P, F>(&self, resolving_map: &mut BTreeMap<PathBuf, TreeEntry>, parent: P, f: &mut F)
    where
        P: AsRef<Path>,
        F: FnMut(&Path, &TreeEntry, bool),
    {
        let mut paths = Vec::new();
        let mut entries = Vec::new();
        for (path, entry) in resolving_map.range(parent.as_ref().to_owned()..) {
            if !path.starts_with(parent.as_ref()) {
                break;
            }
            paths.push(path.clone());
            entries.push(entry.clone());
        }

        let digest = match self.config.mode {
            HashMode::Git => {
                entries.sort_by_key(|e| {
                    let mut bs = e.name.as_bytes().to_vec();
                    if e.file_mode == FileMode::DIR {
                        bs.push(b'/');
                    }
                    bs
                });
                let mut hasher = make_hasher(self.config.algorithm);
                tree_from_entries(&mut hasher, entries.iter()).unwrap();
                hasher.result_vec()
            }
            HashMode::Native => {
                let refs: Vec<(u8, &str, &[u8])> = entries
                    .iter()
                    .map(|e| {
                        let kind = if e.file_mode == FileMode::DIR { b'D' } else { b'F' };
                        (kind, e.name.as_str(), e.digest.as_slice())
                    })
                    .collect();
                compute_tree_hash(&refs, self.config.algorithm)
            }
        };

        for path in paths.iter() {
            resolving_map.remove(path);
        }
        let name = parent.as_ref().file_name().unwrap_or_default();
        let parent_entry = TreeEntry::new(FileMode::DIR, name.to_str().unwrap().to_owned(), digest);
        f(parent.as_ref(), &parent_entry, true);
        resolving_map.insert(parent.as_ref().to_owned(), parent_entry);
    }

    pub fn walk<P: AsRef<Path>, F>(&self, path: P, walk: ignore::Walk, f: &mut F)
    where
        F: FnMut(&Path, &TreeEntry, bool),
    {
        let mut resolving_map = BTreeMap::<PathBuf, TreeEntry>::new();
        let is_dir = path.as_ref().is_dir();
        let mut walk_state = PathWalkState::new(path.as_ref().to_owned(), is_dir);
        for result in walk {
            match result {
                Ok(entry) => {
                    let file_mode = FileMode::from(entry.metadata().unwrap());
                    if file_mode != FileMode::DIR {
                        let digest = match self.config.mode {
                            HashMode::Git => {
                                let mut hasher = make_hasher(self.config.algorithm);
                                if let Err(err) = blob_from_path(&mut hasher, entry.path()) {
                                    if self.no_error {
                                        warn!("{}", err);
                                        continue;
                                    } else {
                                        panic!("{}", err)
                                    }
                                }
                                hasher.result_vec()
                            }
                            HashMode::Native => match hash_file_content(entry.path(), self.config.algorithm) {
                                Ok(h) => h,
                                Err(err) => {
                                    if self.no_error {
                                        warn!("{}", err);
                                        continue;
                                    } else {
                                        panic!("{}", err)
                                    }
                                }
                            },
                        };
                        let path = entry.path();
                        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                        let te = TreeEntry::new(file_mode, name, digest);
                        f(path, &te, false);
                        if !self.blob_only {
                            resolving_map.insert(path.to_owned(), te);
                            walk_state.process(Some(&path), &mut |p| self.resolve(&mut resolving_map, p, f));
                        }
                    }
                }
                Err(err) => {
                    if self.no_error {
                        warn!("{}", err)
                    } else {
                        panic!("{}", err)
                    }
                }
            }
        }
        if !self.blob_only {
            walk_state.process::<&Path, _>(None, &mut |p| self.resolve(&mut resolving_map, p, f));
        }
    }
}
