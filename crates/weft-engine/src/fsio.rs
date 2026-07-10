//! Reading and writing rendered trees on the real filesystem.

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::{FileEntry, Tree, DEFAULT_FILE_MODE};

use crate::state::STATE_DIR;

/// Write `tree` under `dest`, creating directories as needed.
pub fn write_tree(dest: &Utf8Path, tree: &Tree) -> Result<()> {
    for (path, entry) in tree.iter() {
        let target = dest.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, &entry.content).with_context(|| format!("writing {target}"))?;
        set_mode(&target, entry.mode)?;
    }
    Ok(())
}

/// Write a single file (creating parent dirs) with its mode.
pub fn write_file(dest: &Utf8Path, rel: &Utf8Path, entry: &FileEntry) -> Result<()> {
    let target = dest.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, &entry.content).with_context(|| format!("writing {target}"))?;
    set_mode(&target, entry.mode)
}

/// Read a directory back into a `Tree`, skipping `.weft/` and `.git/`.
/// MVP is text-only: non-UTF-8 files are an error with a clear message.
pub fn read_tree(root: &Utf8Path) -> Result<Tree> {
    let mut tree = Tree::new();
    read_dir_into(root, root, &mut tree)?;
    Ok(tree)
}

fn read_dir_into(root: &Utf8Path, dir: &Utf8Path, tree: &mut Tree) -> Result<()> {
    for entry in dir
        .read_dir_utf8()
        .with_context(|| format!("reading {dir}"))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().unwrap_or_default();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if name == STATE_DIR || name == ".git" {
                continue;
            }
            read_dir_into(root, path, tree)?;
        } else if file_type.is_file() {
            let bytes = std::fs::read(path)?;
            let mut content = String::from_utf8(bytes).map_err(|_| {
                anyhow::anyhow!("`{path}` is not UTF-8 text; weft MVP handles text files only")
            })?;
            // Weft's text model normalizes files to exactly one trailing
            // newline (rendering always produces that). Apply the same rule
            // on read, or editor-written files without a final newline make
            // replay-vs-worktree comparisons fail spuriously.
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            let mode = get_mode(path)?;
            let rel: Utf8PathBuf = path
                .strip_prefix(root)
                .expect("walked path is under root")
                .to_owned();
            tree.insert(rel, FileEntry { content, mode });
        }
    }
    Ok(())
}

/// Ensure `dest` is usable as a scaffold target: nonexistent or an empty dir.
pub fn ensure_empty_dest(dest: &Utf8Path) -> Result<()> {
    if !dest.exists() {
        std::fs::create_dir_all(dest)?;
        return Ok(());
    }
    if !dest.is_dir() {
        bail!("destination `{dest}` exists and is not a directory");
    }
    if dest.read_dir_utf8()?.next().is_some() {
        bail!("destination `{dest}` is not empty");
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Utf8Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode on {path}"))
}

#[cfg(not(unix))]
fn set_mode(_path: &Utf8Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn get_mode(path: &Utf8Path) -> Result<u32> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode();
    // Normalize to the two modes weft distinguishes: executable or not.
    Ok(if mode & 0o100 != 0 {
        0o755
    } else {
        DEFAULT_FILE_MODE
    })
}

#[cfg(not(unix))]
fn get_mode(_path: &Utf8Path) -> Result<u32> {
    Ok(DEFAULT_FILE_MODE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tree_normalizes_missing_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join("no-newline.txt"), "hello world").unwrap();
        std::fs::write(root.join("with-newline.txt"), "hello\n").unwrap();
        std::fs::write(root.join("empty.txt"), "").unwrap();

        let tree = read_tree(root).unwrap();
        assert_eq!(
            tree.get("no-newline.txt".into()).unwrap().content,
            "hello world\n"
        );
        assert_eq!(
            tree.get("with-newline.txt".into()).unwrap().content,
            "hello\n"
        );
        assert_eq!(tree.get("empty.txt".into()).unwrap().content, "");
    }
}
