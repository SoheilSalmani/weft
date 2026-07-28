//! Working out which template and session a command applies to.
//!
//! Session commands are meant to be run from *inside* a worktree, the way git
//! is run from inside a checkout: the template is found by walking up, path
//! arguments are relative to where you stand, and shell completion therefore
//! just works. `--template` / `--session` stay as explicit overrides.

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_engine::discover::{self, WorktreeLink};
use weft_engine::session::Session;

/// The `--template` / `--session` pair shared by every session command.
///
/// `group(skip)`: these are plain flags, and the derived group would collide
/// with the `weft session scope` subcommand's name.
#[derive(clap::Args, Clone, Debug, Default)]
#[group(skip)]
pub struct Scope {
    /// Template directory. Defaults to the template found by walking up from
    /// the current directory.
    #[arg(long)]
    pub template: Option<Utf8PathBuf>,
    /// Session to act on. Defaults to the session whose worktree you are
    /// standing in, or the template's only session.
    #[arg(long, short = 's')]
    pub session: Option<String>,
}

/// A resolved session: where the template is, which session, and where its
/// worktree lives.
pub struct Resolved {
    pub template: Utf8PathBuf,
    pub session: String,
    pub worktree: Utf8PathBuf,
    /// Where the command was run, relative to the worktree root. Path
    /// arguments are resolved against this; empty at the root (or when the
    /// command was run from the template rather than the worktree).
    pub prefix: Utf8PathBuf,
}

impl Scope {
    /// The template alone, for commands that do not need a session.
    pub fn template(&self) -> Result<Utf8PathBuf> {
        if let Some(dir) = &self.template {
            return Ok(discover::absolute(dir));
        }
        let cwd = discover::cwd()?;
        match discover::locate(&cwd) {
            Some(loc) => Ok(loc.template_root),
            None => bail!(
                "no weft template here — run this inside a template or one of its \
                 worktrees, or pass --template DIR"
            ),
        }
    }

    /// The template plus the session to act on.
    pub fn resolve(&self) -> Result<Resolved> {
        let cwd = discover::cwd()?;
        let loc = discover::locate(&cwd);
        let template = match &self.template {
            Some(dir) => discover::absolute(dir),
            None => match &loc {
                Some(l) => l.template_root.clone(),
                None => bail!(
                    "no weft template here — run this inside a template or one of its \
                     worktrees, or pass --template DIR"
                ),
            },
        };

        // Standing inside a worktree names the session, unless overridden.
        let inside = loc
            .as_ref()
            .filter(|l| l.template_root == template)
            .and_then(|l| l.session.clone());
        let session = match (&self.session, inside) {
            (Some(name), _) => name.clone(),
            (None, Some(name)) => name,
            (None, None) => Session::only(&template)?,
        };

        let sess = Session::load(&template, &session)?;
        let recorded = sess.worktree(&template, &session);
        let worktree = heal(&template, &session, &sess, &loc, &recorded)?;

        // Empty unless the command was run inside the worktree.
        let prefix = cwd
            .strip_prefix(discover::absolute(&worktree))
            .map(|p| p.to_owned())
            .unwrap_or_default();
        Ok(Resolved {
            template,
            session,
            worktree,
            prefix,
        })
    }
}

/// If a worktree was moved with `mv` rather than `weft session move`, the
/// session file still points at the old place. Running a command from inside
/// the worktree is proof of where it really is, so fix the record instead of
/// failing (git needs an explicit `worktree repair`).
fn heal(
    template: &Utf8Path,
    name: &str,
    sess: &Session,
    loc: &Option<discover::Location>,
    recorded: &Utf8Path,
) -> Result<Utf8PathBuf> {
    let Some(actual) = loc
        .as_ref()
        .filter(|l| l.session.as_deref() == Some(name) && l.template_root == template)
        .and_then(|l| l.worktree_root.clone())
    else {
        return Ok(recorded.to_owned());
    };
    if discover::absolute(&actual) == discover::absolute(recorded) {
        return Ok(recorded.to_owned());
    }
    let mut updated = Session::load(template, name)?;
    updated.session.worktree = Some(discover::absolute(&actual));
    updated.session.adopted = sess.session.adopted;
    updated.save(template, name)?;
    eprintln!("note: session `{name}` moved to `{actual}`; updated its record");
    Ok(actual)
}

/// Resolve a user-supplied path pattern against the directory the command was
/// run in, like git's pathspecs. `.` means "here and below".
pub fn qualify(prefix: &Utf8Path, pattern: &str) -> String {
    let here = pattern == "." || pattern == "./";
    match (prefix.as_str().is_empty(), here) {
        (true, true) => "**".to_owned(),
        (true, false) => pattern.to_owned(),
        (false, true) => format!("{prefix}/**"),
        (false, false) => format!("{prefix}/{pattern}"),
    }
}

/// Write the back-pointer that makes a directory findable as a worktree.
pub fn link(worktree: &Utf8Path, template: &Utf8Path, session: &str) -> Result<()> {
    WorktreeLink {
        template: discover::absolute(template),
        session: session.to_owned(),
    }
    .save(worktree)
    .with_context(|| format!("linking worktree {worktree}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_are_relative_to_where_you_stand() {
        let root = Utf8Path::new("");
        assert_eq!(qualify(root, "src/**"), "src/**");
        assert_eq!(qualify(root, "."), "**");

        let sub = Utf8Path::new("src/app");
        assert_eq!(qualify(sub, "main.rs"), "src/app/main.rs");
        assert_eq!(qualify(sub, "."), "src/app/**");
    }
}
