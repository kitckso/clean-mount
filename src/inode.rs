use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

const ROOT_INO: u64 = 1;

struct Entry {
    rel: PathBuf,
    lookups: u64,
}

pub struct InodeTable {
    ino_to_entry: HashMap<u64, Entry>,
    path_to_ino: HashMap<PathBuf, u64>,
    next_ino: u64,
}

impl InodeTable {
    pub fn new() -> Self {
        let mut table = Self {
            ino_to_entry: HashMap::new(),
            path_to_ino: HashMap::new(),
            next_ino: ROOT_INO + 1,
        };

        table.ino_to_entry.insert(
            ROOT_INO,
            Entry {
                rel: PathBuf::new(),
                lookups: u64::MAX,
            },
        );

        table.path_to_ino.insert(PathBuf::new(), ROOT_INO);

        table
    }

    pub fn path(&self, ino: u64) -> Option<&Path> {
        self.ino_to_entry.get(&ino).map(|e| e.rel.as_path())
    }

    pub fn get_or_create(&mut self, rel: &Path) -> u64 {
        let rel = normalize_rel(rel);

        if let Some(&ino) = self.path_to_ino.get(&rel) {
            return ino;
        }

        let ino = self.next_ino;
        self.next_ino = self.next_ino.saturating_add(1);

        self.ino_to_entry.insert(
            ino,
            Entry {
                rel: rel.clone(),
                lookups: 0,
            },
        );

        self.path_to_ino.insert(rel, ino);

        ino
    }

    pub fn add_lookup(&mut self, ino: u64) {
        if let Some(entry) = self.ino_to_entry.get_mut(&ino) {
            entry.lookups = entry.lookups.saturating_add(1);
        }
    }

    pub fn forget(&mut self, ino: u64, nlookup: u64) {
        if ino == ROOT_INO {
            return;
        }

        let should_remove = if let Some(entry) = self.ino_to_entry.get_mut(&ino) {
            entry.lookups = entry.lookups.saturating_sub(nlookup);
            entry.lookups == 0
        } else {
            false
        };

        if should_remove {
            if let Some(entry) = self.ino_to_entry.remove(&ino) {
                self.path_to_ino.remove(&entry.rel);
            }
        }
    }
}

fn normalize_rel(rel: &Path) -> PathBuf {
    let mut out = PathBuf::new();

    for comp in rel.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => out.push(c),
            _ => {}
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_stable() {
        let mut table = InodeTable::new();

        assert_eq!(table.get_or_create(Path::new("")), ROOT_INO);
        assert_eq!(table.path(ROOT_INO).unwrap(), Path::new(""));
    }

    #[test]
    fn creates_and_forgets() {
        let mut table = InodeTable::new();

        let ino = table.get_or_create(Path::new("a"));
        table.add_lookup(ino);

        assert_eq!(table.path(ino).unwrap(), Path::new("a"));

        table.forget(ino, 1);

        assert!(table.path(ino).is_none());
    }

    #[test]
    fn normalizes_relative_paths() {
        assert_eq!(normalize_rel(Path::new("./a/../b")), PathBuf::from("b"));
        assert_eq!(normalize_rel(Path::new("a/./b/c")), PathBuf::from("a/b/c"));
    }
}
