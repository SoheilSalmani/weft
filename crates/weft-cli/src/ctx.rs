//! Working out which template and session a command applies to.
//!
//! The worktree commands (`status`, `add`, `reset`, `diff`, `commit`) act on
//! the worktree you stand in, the way git is run from inside a checkout, or
//! on the session `--session` names. They never pick one by counting: the
//! template root is weft's bare repository, not a checkout, and a path typed
//! there would name a template file while weft read it in the worktree.
//! `weft session …` subcommands manage sessions from anywhere in the
//! template and also accept the template's only session.

use anyhow::{anyhow, bail, Context, Result};
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
    /// standing in.
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
    /// arguments are resolved against this; empty at the root, and when
    /// `--session` names the session from outside its worktree (paths are
    /// then relative to the worktree root, like git's `--work-tree`).
    pub prefix: Utf8PathBuf,
    /// Whether the command was run inside the worktree.
    pub inside: bool,
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

    /// The session a worktree command acts on: `--session`, else the
    /// worktree you are standing in.
    pub fn resolve(&self) -> Result<Resolved> {
        self.resolve_with(|template| Err(not_in_worktree(template)))
    }

    /// For `weft session …` subcommands: like [`Scope::resolve`], but outside
    /// a worktree the template's only session will do.
    pub fn resolve_or_only(&self) -> Result<Resolved> {
        self.resolve_with(Session::only)
    }

    fn resolve_with(&self, outside: impl FnOnce(&Utf8Path) -> Result<String>) -> Result<Resolved> {
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
        let standing_in = loc
            .as_ref()
            .filter(|l| l.template_root == template)
            .and_then(|l| l.session.clone());
        let session = match (&self.session, standing_in) {
            (Some(name), _) => name.clone(),
            (None, Some(name)) => name,
            (None, None) => outside(&template)?,
        };

        let sess = Session::load(&template, &session)?;
        let recorded = sess.worktree(&template, &session);
        let worktree = heal(&template, &session, &sess, &loc, &recorded)?;

        let (prefix, inside) = match cwd.strip_prefix(discover::absolute(&worktree)) {
            Ok(p) => (p.to_owned(), true),
            Err(_) => (Utf8PathBuf::new(), false),
        };
        Ok(Resolved {
            template,
            session,
            worktree,
            prefix,
            inside,
        })
    }
}

/// Why a worktree command has no session to act on, ending with what to run.
fn not_in_worktree(template: &Utf8Path) -> anyhow::Error {
    let sessions = match Session::list(template) {
        Ok(sessions) => sessions,
        Err(e) => return e,
    };
    if sessions.is_empty() {
        return anyhow!(
            "not inside a session worktree, and `{template}` has no sessions\n\
             start one with `weft session new NAME`"
        );
    }
    let rows: Vec<String> = sessions
        .iter()
        .map(|(name, sess)| format!("  {name}\t{}", sess.worktree(template, name)))
        .collect();
    let name = match sessions.as_slice() {
        [(only, _)] => only.as_str(),
        _ => "NAME",
    };
    anyhow!(
        "not inside a session worktree of `{template}`; its sessions:\n{}\n\
         run this inside one (`{}` opens a shell there), or pass --session {name}",
        rows.join("\n"),
        crate::shell::command_for(template, name)
    )
}

/// Refuse the `weft add`/`weft reset` patterns at `missing` (indices into
/// `typed`, the patterns as the user typed them). `.` is exempt: "here and
/// below" is never a typo, even where nothing is left to stage.
pub fn refuse_unmatched(
    here: &Resolved,
    typed: &[String],
    missing: &[usize],
    scope: &[String],
) -> Result<()> {
    let named: Vec<String> = missing
        .iter()
        .map(|&i| typed[i].as_str())
        .filter(|p| !matches!(*p, "." | "./"))
        .map(|p| format!("`{p}`"))
        .collect();
    if named.is_empty() {
        return Ok(());
    }
    let hint = if !here.inside {
        format!(
            "from outside the worktree, paths are relative to its root: {}",
            here.worktree
        )
    } else if !scope.is_empty() {
        format!(
            "this session only looks at {}; widen it with `weft session scope --add GLOB`",
            scope.join(", ")
        )
    } else {
        "paths are relative to where you stand; `dir/**` covers a directory".to_owned()
    };
    bail!(
        "{} matched no file in session `{}`; nothing changed\n{hint}",
        named.join(", "),
        here.session
    )
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
