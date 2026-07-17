//! `weft init`: create a blank template skeleton so authoring never starts
//! from copying an existing template.

use anyhow::{bail, Context, Result};
use camino::Utf8Path;

use crate::template::{MANIFEST_FILE, PATCHES_DIR};

/// Create `<dir>/weft.toml` (+ an empty `patches/`) with the given template
/// name (defaults to the directory name). Refuses to overwrite an existing
/// manifest.
pub fn run(dir: &Utf8Path, name: Option<&str>) -> Result<()> {
    let manifest_path = dir.join(MANIFEST_FILE);
    if manifest_path.is_file() {
        bail!("`{manifest_path}` already exists; refusing to overwrite");
    }
    let name = match name {
        Some(n) => n.to_owned(),
        None => dir
            .canonicalize_utf8()
            .ok()
            .and_then(|p| p.file_name().map(str::to_owned))
            .or_else(|| dir.file_name().map(str::to_owned))
            .context("cannot derive a template name from the directory; pass --name")?,
    };
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        bail!("invalid template name {name:?} (alphanumerics, `-`, `_`)");
    }

    std::fs::create_dir_all(dir.join(PATCHES_DIR))
        .with_context(|| format!("creating {}/{}", dir, PATCHES_DIR))?;
    let manifest = format!(
        r#"[template]
name = "{name}"
weft-version = "0.1"
description = "TODO: what this template scaffolds."

# Declare the questions your template asks, then record your first patch:
#
#   weft record --template . --answer "project_name=Demo Project"
#   # …edit files in the printed worktree…
#   weft commit --template . --name base --title "Initialize project" --yes
#
# [[question]]
# id = "project_name"
# kind = "string"
# prompt = "Project name"
# example = "Demo Project"
"#
    );
    std::fs::write(&manifest_path, manifest).with_context(|| format!("writing {manifest_path}"))?;
    let ignore_path = dir.join(crate::weftignore::IGNORE_FILE);
    if !ignore_path.exists() {
        std::fs::write(
            &ignore_path,
            "# gitignore-style patterns excluded when weft reads a recording\n\
             # worktree back (weft commit / diff / patch resync). Keeps tool\n\
             # side-products out of patches. `.git/`, `.weft/`, and `.DS_Store`\n\
             # are always ignored.\n\
             node_modules/\n",
        )
        .with_context(|| format!("writing {ignore_path}"))?;
    }
    eprintln!("initialized template `{name}` in {dir}");
    Ok(())
}
