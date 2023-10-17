use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

use digest::Digest;
use ignore;
use sha1;

use crate::{
    object::{blob_from_path, tree_from_entries, FileMode, TreeEntry},
    path::PathWalkState,
};
use sha1::Sha1;

pub trait Hasher: Write {
    fn result_vec(&mut self) -> Vec<u8>;
}

impl Hasher for Sha1 {
    fn result_vec(&mut self) -> Vec<u8> {
        self.clone().finalize().to_vec()
    }
}

pub struct TrebloWalk {
    pub hasher_supplier: fn() -> Box<dyn Hasher>,
    pub blob_only: bool,
    pub no_error: bool,
}

impl Default for TrebloWalk {
    fn default() -> Self {
        TrebloWalk { hasher_supplier: || Box::new(sha1::Sha1::new()), blob_only: false, no_error: false }
    }
}

impl TrebloWalk {
    fn resolve<P, F>(&self, resolving_map: &mut BTreeMap<PathBuf, TreeEntry>, parent: P, f: &mut F)
    where
        P: AsRef<Path>,
        F: FnMut(&Path, &TreeEntry, bool) -> (),
    {
        let mut paths = Vec::new();
        let mut entries = Vec::new();
        for (path, entry) in resolving_map.range(parent.as_ref().to_owned()..).into_iter() {
            if !path.starts_with(parent.as_ref()) {
                break;
            }
            paths.push(path.clone());
            entries.push(entry.clone());
        }
        entries.sort_by_key(|e| {
            let mut bs = e.name.as_bytes().to_vec();
            if e.file_mode == FileMode::DIR {
                bs.push('/' as u8);
            }
            bs
        });
        let mut hasher = (self.hasher_supplier)();
        tree_from_entries(&mut hasher, entries.iter()).unwrap();
        let digest = hasher.result_vec();
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
        F: FnMut(&Path, &TreeEntry, bool) -> (),
    {
        let mut resolving_map = BTreeMap::<PathBuf, TreeEntry>::new();
        let is_dir = path.as_ref().is_dir();
        let mut walk_state = PathWalkState::new(path.as_ref().to_owned(), is_dir);
        for result in walk {
            match result {
                Ok(entry) => {
                    let file_mode = FileMode::from(entry.metadata().unwrap());
                    if file_mode != FileMode::DIR {
                        let mut hasher = (self.hasher_supplier)();
                        if let Err(err) = blob_from_path(&mut hasher, entry.path()) {
                            if self.no_error {
                                warn!("{}", err);
                                continue;
                            } else {
                                panic!("{}", err)
                            }
                        }
                        let digest = hasher.result_vec();
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
