use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::patch::DEFAULT_FILE_MODE;

/// A rendered file: UTF-8 text plus a permission mode. MVP is text-only;
/// binary support is post-MVP (record refuses non-UTF-8 files).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub content: String,
    pub mode: u32,
}

impl FileEntry {
    pub fn text(content: impl Into<String>) -> Self {
        FileEntry {
            content: content.into(),
            mode: DEFAULT_FILE_MODE,
        }
    }
}

/// An in-memory rendered tree. Directories are implicit in the paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree(pub BTreeMap<Utf8PathBuf, FileEntry>);

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, path: &Utf8Path) -> Option<&FileEntry> {
        self.0.get(path)
    }

    pub fn insert(&mut self, path: Utf8PathBuf, entry: FileEntry) -> Option<FileEntry> {
        self.0.insert(path, entry)
    }

    pub fn remove(&mut self, path: &Utf8Path) -> Option<FileEntry> {
        self.0.remove(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Utf8PathBuf, &FileEntry)> {
        self.0.iter()
    }

    pub fn paths(&self) -> impl Iterator<Item = &Utf8PathBuf> {
        self.0.keys()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Content address of the whole tree: blake3 over (path, mode, content)
    /// in sorted path order. Used to pin base states.
    pub fn hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for (path, entry) in &self.0 {
            hasher.update(path.as_str().as_bytes());
            hasher.update(&[0]);
            hasher.update(&entry.mode.to_le_bytes());
            hasher.update(&(entry.content.len() as u64).to_le_bytes());
            hasher.update(entry.content.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }
}

impl FromIterator<(Utf8PathBuf, FileEntry)> for Tree {
    fn from_iter<T: IntoIterator<Item = (Utf8PathBuf, FileEntry)>>(iter: T) -> Self {
        Tree(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_order_independent_and_content_sensitive() {
        let mut a = Tree::new();
        a.insert("b.txt".into(), FileEntry::text("two\n"));
        a.insert("a.txt".into(), FileEntry::text("one\n"));

        let mut b = Tree::new();
        b.insert("a.txt".into(), FileEntry::text("one\n"));
        b.insert("b.txt".into(), FileEntry::text("two\n"));

        assert_eq!(a.hash(), b.hash());

        b.insert("a.txt".into(), FileEntry::text("changed\n"));
        assert_ne!(a.hash(), b.hash());
    }
}
