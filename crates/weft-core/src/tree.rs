use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::patch::DEFAULT_FILE_MODE;

/// File content: UTF-8 text (line-based, abstractable, hunk-mergeable) or
/// an opaque binary blob (replaced wholesale, never abstracted).
///
/// The variant is a **function of the bytes** — [`FileData::from_bytes`]
/// classifies valid UTF-8 as `Text` — so a rendered tree and its re-read
/// worktree always agree, and derived equality is consistent.
#[derive(Clone, PartialEq, Eq)]
pub enum FileData {
    Text(String),
    Binary(Vec<u8>),
}

impl FileData {
    /// Canonical classification: valid UTF-8 ⇒ `Text`, else `Binary`.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        match String::from_utf8(bytes) {
            Ok(text) => FileData::Text(text),
            Err(e) => FileData::Binary(e.into_bytes()),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            FileData::Text(s) => s.as_bytes(),
            FileData::Binary(b) => b,
        }
    }

    /// The text form, when this is a text file.
    pub fn text(&self) -> Option<&str> {
        match self {
            FileData::Text(s) => Some(s),
            FileData::Binary(_) => None,
        }
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, FileData::Binary(_))
    }

    /// Byte length.
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

impl From<String> for FileData {
    fn from(s: String) -> Self {
        FileData::Text(s)
    }
}

impl From<&str> for FileData {
    fn from(s: &str) -> Self {
        FileData::Text(s.to_owned())
    }
}

impl std::fmt::Debug for FileData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileData::Text(s) => write!(f, "Text({s:?})"),
            FileData::Binary(b) => write!(f, "Binary({} byte(s))", b.len()),
        }
    }
}

/// A rendered file: text or binary content plus a permission mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub content: FileData,
    pub mode: u32,
}

impl FileEntry {
    pub fn text(content: impl Into<String>) -> Self {
        FileEntry {
            content: FileData::Text(content.into()),
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

    /// Content address of the whole tree: blake3 over (path, mode, content
    /// bytes) in sorted path order. Used to pin base states. Text entries
    /// feed the exact bytes they always have — pre-binary hashes are stable.
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
