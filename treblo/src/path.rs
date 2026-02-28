use std::path::{Path, PathBuf};

pub struct PathWalkState<T> {
    root: T,
    parent_stack: Vec<T>,
}

impl PathWalkState<PathBuf> {
    pub fn new(root: PathBuf, is_dir: bool) -> PathWalkState<PathBuf> {
        let parent_stack = if is_dir { vec![root.clone()] } else { Vec::new() };
        PathWalkState { root, parent_stack }
    }

    pub fn process<P, F>(&mut self, item: Option<P>, f: &mut F)
    where
        P: AsRef<Path>,
        F: FnMut(&Path),
    {
        let parent = item.and_then(|p| p.as_ref().parent().map(|p| p.to_owned()));
        if let Some(parent) = parent {
            while !self.parent_stack.is_empty() {
                let last = self.parent_stack.last().unwrap();
                if &parent == last || parent.starts_with(last) {
                    break;
                }
                f(last);
                self.parent_stack.pop();
            }
            let last = self.parent_stack.last();
            let mut parents = Vec::new();
            let mut cur = parent;
            loop {
                if last.is_some() && &cur == last.unwrap() {
                    break;
                }
                if !cur.starts_with(&self.root) {
                    break;
                }
                parents.push(cur.clone());
                if let Some(next) = cur.parent() {
                    cur = next.to_owned();
                } else {
                    break;
                }
            }
            for pb in parents.iter().rev() {
                self.parent_stack.push(pb.to_owned());
            }
        } else {
            while let Some(last) = self.parent_stack.pop() {
                f(&last);
            }
        }
    }
}
