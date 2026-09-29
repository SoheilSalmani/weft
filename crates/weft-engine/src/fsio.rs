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
        std::fs::write(&target, entry.content.as_bytes())
            .with_context(|| format!("writing {target}"))?;
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
    std::fs::write(&target, entry.content.as_bytes())
        .with_context(|| format!("writing {target}"))?;
    set_mode(&target, entry.mode)
}

/// Delete a file, then each directory above it, up to `dest`, that the
/// deletion left empty, the way `git checkout` does. A directory that still
/// holds anything (a file the user added, say) stays, and so does `dest`.
pub fn remove_file(dest: &Utf8Path, rel: &Utf8Path) -> Result<()> {
    std::fs::remove_file(dest.join(rel)).with_context(|| format!("deleting {rel}"))?;
    let mut dir = rel.parent();
    while let Some(d) = dir.filter(|d| !d.as_str().is_empty()) {
        // Fails on a directory that isn't empty, which is where pruning stops.
        if std::fs::remove_dir(dest.join(d)).is_err() {
            break;
        }
        dir = d.parent();
    }
    Ok(())
}

/// Read a directory back into a `Tree`, skipping `.weft/` and `.git/`.
/// Valid UTF-8 files become text; other files are opaque binary blobs.
pub fn read_tree(root: &Utf8Path) -> Result<Tree> {
    let mut tree = Tree::new();
    read_dir_into(root, root, &mut tree, None)?;
    Ok(tree)
}

/// [`read_tree`] with the template's [`.weftignore`](crate::weftignore)
/// rules applied — except for paths in `keep` (normally the base render's
/// paths), so deleting a rendered file is still recorded even when a
/// pattern matches it. Ignored directories are pruned whole without
/// reading their contents (a `node_modules/` full of binaries never
/// touches the UTF-8 check).
pub fn read_tree_ignoring(
    root: &Utf8Path,
    rules: &crate::weftignore::IgnoreRules,
    keep: &std::collections::BTreeSet<Utf8PathBuf>,
) -> Result<Tree> {
    let mut tree = Tree::new();
    read_dir_into(root, root, &mut tree, Some((rules, keep)))?;
    Ok(tree)
}

type Filter<'a> = (
    &'a crate::weftignore::IgnoreRules,
    &'a std::collections::BTreeSet<Utf8PathBuf>,
);

fn read_dir_into(
    root: &Utf8Path,
    dir: &Utf8Path,
    tree: &mut Tree,
    filter: Option<Filter<'_>>,
) -> Result<()> {
    for entry in dir
        .read_dir_utf8()
        .with_context(|| format!("reading {dir}"))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().unwrap_or_default();
        let file_type = entry.file_type()?;
        let rel = path
            .strip_prefix(root)
            .expect("walked path is under root")
            .to_owned();
        if file_type.is_dir() {
            if name == STATE_DIR || name == ".git" {
                continue;
            }
            if let Some((rules, keep)) = filter {
                if rules.ignores(&rel, true) && !keep.iter().any(|k| k.starts_with(&rel)) {
                    continue;
                }
            }
            read_dir_into(root, path, tree, filter)?;
        } else if file_type.is_file() {
            if let Some((rules, keep)) = filter {
                if rules.ignores(&rel, false) && !keep.contains(&rel) {
                    continue;
                }
            }
            tree.insert(rel, read_entry(path)?);
        }
    }
    Ok(())
}

/// Read one file: valid UTF-8 becomes normalized text (exactly one trailing
/// newline, matching what rendering produces); anything else is an opaque
/// binary blob.
fn read_entry(path: &Utf8Path) -> Result<FileEntry> {
    let bytes = std::fs::read(path)?;
    let content = match weft_core::FileData::from_bytes(bytes) {
        weft_core::FileData::Text(mut text) => {
            // Weft's text model normalizes files to exactly one trailing
            // newline (rendering always produces that). Apply the same rule
            // on read, or editor-written files without a final newline make
            // replay-vs-worktree comparisons fail spuriously.
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            weft_core::FileData::Text(text)
        }
        binary => binary,
    };
    let mode = get_mode(path)?;
    Ok(FileEntry { content, mode })
}

/// Read one file under `root` (normalized like [`read_tree`]); `None` when
/// absent. Used by `weft update` to read only template-tracked paths from a
/// scaffolded project — the rest of the project (user files, `node_modules/`)
/// is irrelevant to the merge and may be binary.
pub fn read_file(root: &Utf8Path, rel: &Utf8Path) -> Result<Option<FileEntry>> {
    let path = root.join(rel);
    if !path.is_file() {
        return Ok(None);
    }
    read_entry(&path).map(Some)
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
        let text = |p: &str| {
            tree.get(p.into())
                .unwrap()
                .content
                .text()
                .unwrap()
                .to_owned()
        };
        assert_eq!(text("no-newline.txt"), "hello world\n");
        assert_eq!(text("with-newline.txt"), "hello\n");
        assert_eq!(text("empty.txt"), "");
    }

    #[test]
    fn read_tree_preserves_binary_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let bytes = [0xffu8, 0xfe, 0x00, 0x42];
        std::fs::write(root.join("icon.bin"), bytes).unwrap();
        let tree = read_tree(root).unwrap();
        let entry = tree.get("icon.bin".into()).unwrap();
        assert!(entry.content.is_binary());
        assert_eq!(entry.content.as_bytes(), bytes);
    }

    #[test]
    fn remove_file_prunes_only_the_directories_it_empties() {
        let dir = tempfile::tempdir().unwrap();
        let dest = camino::Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(dest.join(".agents/skills/sql")).unwrap();
        std::fs::create_dir_all(dest.join(".agents/skills/python")).unwrap();
        std::fs::write(dest.join(".agents/skills/sql/SKILL.md"), "# sql\n").unwrap();
        std::fs::write(dest.join(".agents/skills/python/SKILL.md"), "# python\n").unwrap();
        std::fs::write(dest.join("README.md"), "# shop\n").unwrap();

        remove_file(dest, ".agents/skills/sql/SKILL.md".into()).unwrap();
        assert!(!dest.join(".agents/skills/sql").exists());
        assert!(dest.join(".agents/skills/python/SKILL.md").exists());

        remove_file(dest, ".agents/skills/python/SKILL.md".into()).unwrap();
        assert!(
            !dest.join(".agents").exists(),
            "every emptied directory goes"
        );

        remove_file(dest, "README.md".into()).unwrap();
        assert!(dest.exists(), "the project directory itself stays");
    }
}
