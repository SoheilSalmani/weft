//! Getting into a session's worktree: the hints printed when a session
//! starts, and `weft session shell` / `weft session new --shell`, which start
//! a shell (or one command) there.
//!
//! No process can change the directory of the shell that launched it, so
//! entering a worktree means starting a new process in it. On Unix weft
//! replaces itself with that process (`exec`): Ctrl-C, job control and the
//! exit status belong to the shell or command, no weft parent is left
//! waiting on it, and `exit` returns to the shell weft was started from.
//! `exec` also skips destructors, so [`enter`] is the last thing a command
//! does.

use std::ffi::OsString;
use std::io::Write as _;
use std::process::Command;

use camino::Utf8Path;
use weft_engine::discover;
use weft_engine::session::Session;

use crate::tui::shell_quote;

/// Names the session in the shell [`enter`] opens, so a prompt can show it.
/// A command started with `--` does not get it: [`stands_in`] and the
/// nesting note read it as "a session shell", and an editor or agent must not
/// be told to `exit`. weft reads it only to word hints, never to choose a
/// session: a command acts on the session it is run in or is given.
const SESSION_ENV: &str = "WEFT_SESSION";

/// Print a new session's worktree on stdout, so `cd $(weft session new)`
/// lands in it, and what to do next on stderr.
pub fn announce_new(template: &Utf8Path, name: &str, worktree: &Utf8Path, opening_shell: bool) {
    println!("{worktree}");
    if opening_shell {
        eprintln!(
            "session `{name}` started — edit the worktree above, then `weft add` and \
             `weft commit`"
        );
    } else {
        eprintln!(
            "session `{name}` started — `cd` into the worktree above or run `{}`, \
             edit, then `weft add` and `weft commit`",
            command_for(template, name)
        );
    }
}

/// Print an amend session's worktree on stdout and what to do next on stderr.
pub fn announce_amend(template: &Utf8Path, name: &str, worktree: &Utf8Path) {
    println!("{worktree}");
    eprintln!(
        "amending `{name}`: the worktree above is the patch applied on its base — \
         edit it (`{}` opens a shell there), then run `weft commit`",
        command_for(template, name)
    );
}

/// `weft session shell NAME`, plus `--template DIR` when the directory this
/// command runs in would not find the template by itself.
pub fn command_for(template: &Utf8Path, session: &str) -> String {
    format!(
        "weft session shell {}{}",
        shell_quote(session),
        template_arg(template)
    )
}

/// ` --template DIR` for a suggested command, or nothing when the directory
/// this command runs in finds the template by itself. DIR is relative when
/// the template sits below that directory, as it was most likely typed.
pub fn template_arg(template: &Utf8Path) -> String {
    let template = discover::absolute(template);
    let cwd = discover::cwd().ok();
    let found = cwd
        .as_deref()
        .and_then(discover::locate)
        .is_some_and(|loc| loc.template_root == template);
    if found {
        return String::new();
    }
    let shown = cwd
        .as_deref()
        .and_then(|cwd| template.strip_prefix(cwd).ok())
        .filter(|relative| !relative.as_str().is_empty())
        .unwrap_or(template.as_path());
    format!(" --template {}", shell_quote(shown.as_str()))
}

/// Start `command` in `worktree`, or, when it is empty, the user's shell with
/// `WEFT_SESSION` naming the session. Never returns: on Unix the process
/// becomes the command, elsewhere it exits with the command's status. A
/// command that cannot be started exits 127 when it was not found and 126
/// otherwise, as `env` does.
pub fn enter(worktree: &Utf8Path, session: &str, command: &[String]) -> ! {
    let worktree = discover::absolute(worktree);
    let mut child = match command.split_first() {
        Some((program, args)) => {
            let mut child = Command::new(program);
            child.args(args);
            child
        }
        None => {
            let shell = user_shell();
            if let Some(outer) = std::env::var_os(SESSION_ENV) {
                eprintln!(
                    "note: already in session `{}`'s shell; this one nests inside it",
                    outer.to_string_lossy()
                );
            }
            eprintln!(
                "opening {} in session `{session}` — `exit` returns here",
                shell.to_string_lossy()
            );
            let mut child = Command::new(shell);
            child.env(SESSION_ENV, session);
            child
        }
    };
    child
        .current_dir(&worktree)
        // The inherited PWD still names the directory weft was started in;
        // anything that reads it instead of asking the OS would be elsewhere.
        .env("PWD", &worktree);
    let program = child.get_program().to_owned();
    // `exec` discards Rust's stdout buffer along with the rest of the process.
    let _ = std::io::stdout().flush();
    let err = replace_with(child);
    eprintln!(
        "Error: cannot run `{}` in `{worktree}`: {err}",
        program.to_string_lossy()
    );
    std::process::exit(if err.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    })
}

/// Become `child`; returns only when that fails.
#[cfg(unix)]
fn replace_with(mut child: Command) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    child.exec()
}

/// No `exec` here: run `child`, then exit with its status.
#[cfg(not(unix))]
fn replace_with(mut child: Command) -> std::io::Error {
    match child.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(err) => err,
    }
}

/// `$SHELL`, else `/bin/sh` (`%COMSPEC%`, else `cmd.exe`, off Unix).
fn user_shell() -> OsString {
    let (var, fallback) = if cfg!(unix) {
        ("SHELL", "/bin/sh")
    } else {
        ("COMSPEC", "cmd.exe")
    };
    std::env::var_os(var)
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| fallback.into())
}

/// Is this command running inside `sess`'s worktree, in the shell [`enter`]
/// opened for it? Ending the session deletes the directory that shell stands
/// in (an adopted worktree is only unlinked, so it stays), and the caller
/// then prints [`exit_hint`]. Ask before the session ends.
pub fn stands_in(sess: &Session, template: &Utf8Path, name: &str) -> bool {
    !sess.session.adopted
        && std::env::var_os(SESSION_ENV).is_some_and(|s| s == name)
        && discover::cwd()
            .is_ok_and(|cwd| cwd.starts_with(discover::absolute(&sess.worktree(template, name))))
}

/// Printed once a session ended from under its own shell ([`stands_in`]).
pub fn exit_hint() {
    eprintln!("its worktree is gone from under this shell — `exit` returns to where you opened it");
}
