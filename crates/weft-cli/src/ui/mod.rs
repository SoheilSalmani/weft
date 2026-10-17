//! How weft looks in a terminal.
//!
//! Everything that styles or draws lives here: the colour policy, the
//! palette, the prompt rail, tables, progress bars and error reports. None
//! of it changes what a message says. Off a terminal every line reads byte
//! for byte as the words the engine or a command produced, which is what
//! scripts, agents and the e2e suite rely on.
//!
//! Two independent switches per stream decide the look:
//! - **colour**: `--color`, then `NO_COLOR`, then `FORCE_COLOR` /
//!   `CLICOLOR_FORCE`, then `CLICOLOR=0` and `TERM=dumb`, then whether the
//!   stream is a terminal. `NO_COLOR` beats `FORCE_COLOR`.
//! - **rich layout** (symbols, tables, the rail, progress bars): the stream
//!   is a terminal and `TERM` is not `dumb`. Colour alone never carries
//!   meaning, so a rich layout without colour still reads.

mod error;
mod narrate;
mod progress;
pub(crate) mod rail;
mod table;

use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::OnceLock;

use console::{Style, StyledObject};

pub(crate) use error::{Cancelled, SilentExit};
pub(crate) use narrate::diff as unified_diff;
pub(crate) use progress::Task;
pub(crate) use table::{cell, Table};

/// `--color`: when weft colours its output. The variants carry no doc
/// comments on purpose: clap would list each one on its own line and lay
/// every option of every command out long-form; the flag's help says it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// What a command's streams are for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presentation {
    /// Output for a person, a script or an agent.
    Terminal,
    /// stdout carries a protocol (LSP, MCP): never style, never draw, and
    /// never write a result line to stdout.
    Protocol,
}

/// One output stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Out {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy)]
struct Caps {
    color: bool,
    rich: bool,
}

struct Ui {
    stdout: Caps,
    stderr: Caps,
    protocol: bool,
}

const PLAIN: Ui = Ui {
    stdout: Caps {
        color: false,
        rich: false,
    },
    stderr: Caps {
        color: false,
        rich: false,
    },
    protocol: false,
};

static UI: OnceLock<Ui> = OnceLock::new();

fn ui() -> &'static Ui {
    UI.get().unwrap_or(&PLAIN)
}

/// Decide how each stream looks and install weft's renderers: the engine's
/// report sink and the prompt theme. Runs once, before any output.
pub(crate) fn init(choice: ColorChoice, presentation: Presentation, title: String) {
    let protocol = presentation == Presentation::Protocol;
    let env = |name: &str| std::env::var_os(name);
    let dumb = env("TERM").is_some_and(|t| t == "dumb");
    let caps = |tty: bool| Caps {
        color: !protocol && wants_color(choice, tty, env),
        rich: !protocol && tty && !dumb,
    };
    let ui = Ui {
        stdout: caps(io::stdout().is_terminal()),
        stderr: caps(io::stderr().is_terminal()),
        protocol,
    };
    // cliclack and indicatif draw on stderr but style through console's
    // stdout switch unless told otherwise, so both switches follow stderr.
    // weft's own styles never read them: `paint` forces each one.
    console::set_colors_enabled(ui.stderr.color);
    console::set_colors_enabled_stderr(ui.stderr.color);
    let _ = UI.set(ui);
    rail::set_title(title);
    cliclack::set_theme(rail::Theme);
    weft_engine::report::install(Box::new(narrate::Narrator));
}

/// Whether a stream that is (or is not) a terminal takes colour, with the
/// environment read through `env`.
fn wants_color(choice: ColorChoice, tty: bool, env: impl Fn(&str) -> Option<OsString>) -> bool {
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    let forced = |name: &str| set(name).is_some_and(|v| v != "0");
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto if set("NO_COLOR").is_some() => false,
        ColorChoice::Auto if forced("FORCE_COLOR") || forced("CLICOLOR_FORCE") => true,
        ColorChoice::Auto => {
            let off = set("CLICOLOR").is_some_and(|v| v == "0")
                || set("TERM").is_some_and(|t| t == "dumb");
            tty && !off
        }
    }
}

/// End the command: close the prompt rail, report an error, and pick the
/// exit status (0, 1 for a failure, 130 for a prompt the person cancelled,
/// or the status a [`SilentExit`] carries).
pub(crate) fn finish(result: anyhow::Result<()>) -> ExitCode {
    let err = match result {
        Ok(()) => {
            rail::close_ok();
            return ExitCode::SUCCESS;
        }
        Err(err) => err,
    };
    if err.chain().any(|e| e.is::<Cancelled>()) {
        rail::abandon();
        return ExitCode::from(130);
    }
    if let Some(SilentExit(code)) = err.chain().find_map(|e| e.downcast_ref::<SilentExit>()) {
        rail::close_failed(None);
        return ExitCode::from(*code);
    }
    error::report(&err);
    ExitCode::FAILURE
}

/// Whether `stream` takes colour.
pub(crate) fn color(stream: Out) -> bool {
    match stream {
        Out::Stdout => ui().stdout.color,
        Out::Stderr => ui().stderr.color,
    }
}

/// Whether `stream` is a terminal that takes a rich layout: symbols,
/// tables, the rail, progress bars.
pub(crate) fn rich(stream: Out) -> bool {
    match stream {
        Out::Stdout => ui().stdout.rich,
        Out::Stderr => ui().stderr.rich,
    }
}

fn protocol() -> bool {
    ui().protocol
}

/// The width to lay `stream` out in: `COLUMNS` when set, else the
/// terminal's, else none (not a terminal).
pub(crate) fn width(stream: Out) -> Option<usize> {
    if let Some(cols) = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()) {
        return Some(cols);
    }
    let term = match stream {
        Out::Stdout => console::Term::stdout(),
        Out::Stderr => console::Term::stderr(),
    };
    term.size_checked().map(|(_, cols)| usize::from(cols))
}

/// weft's palette. One place, so prompts, messages, tables and diffs agree.
pub(crate) mod palette {
    use console::Style;

    /// weft's own colour: names, commands, the active prompt.
    pub(crate) fn accent() -> Style {
        Style::new().color256(141)
    }
    pub(crate) fn success() -> Style {
        Style::new().green()
    }
    pub(crate) fn warning() -> Style {
        Style::new().yellow()
    }
    pub(crate) fn error() -> Style {
        Style::new().red()
    }
    pub(crate) fn note() -> Style {
        Style::new().blue()
    }
    pub(crate) fn dim() -> Style {
        Style::new().dim()
    }
    pub(crate) fn bold() -> Style {
        Style::new().bold()
    }
}

/// `text` in `style`, when `stream` takes colour; as is otherwise.
pub(crate) fn paint<D>(stream: Out, style: Style, text: D) -> StyledObject<D> {
    style.force_styling(color(stream)).apply_to(text)
}

/// `text` with every `` `code` `` span in the accent colour and the rest
/// in `base`, when `stream` takes colour. The backticks stay: colour only
/// repeats what they already say.
pub(crate) fn highlight<'a>(stream: Out, base: Option<Style>, text: &'a str) -> Cow<'a, str> {
    highlight_if(color(stream), base, text)
}

fn highlight_if(on: bool, base: Option<Style>, text: &str) -> Cow<'_, str> {
    if !on {
        return Cow::Borrowed(text);
    }
    let base = base.unwrap_or_default();
    let mut out = String::with_capacity(text.len() + 16);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut rest = line;
        while let Some(open) = rest.find('`') {
            let Some(len) = rest[open + 1..].find('`') else {
                break;
            };
            let close = open + 1 + len;
            push(&mut out, &base, &rest[..open]);
            push(&mut out, &palette::dim(), "`");
            push(&mut out, &palette::accent(), &rest[open + 1..close]);
            push(&mut out, &palette::dim(), "`");
            rest = &rest[close + 1..];
        }
        push(&mut out, &base, rest);
    }
    Cow::Owned(out)
}

fn push(out: &mut String, style: &Style, text: &str) {
    use std::fmt::Write as _;
    if !text.is_empty() {
        let _ = write!(out, "{}", style.clone().force_styling(true).apply_to(text));
    }
}

/// Print a line of a command's result on stdout; see [`outln!`].
pub(crate) fn out_fmt(args: fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let written = stdout
        .write_fmt(args)
        .and_then(|()| stdout.write_all(b"\n"));
    stdout_written(written);
}

/// Print `text` on stdout as is, without adding a newline.
pub(crate) fn out_str(text: &str) {
    stdout_written(io::stdout().lock().write_all(text.as_bytes()));
}

/// A reader that went away (`weft … | head -1`) ends the command quietly;
/// any other stdout failure is reported once.
fn stdout_written(written: io::Result<()>) {
    match written {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(e) => {
            let _ = writeln!(io::stderr(), "weft: writing to stdout: {e}");
            std::process::exit(1);
        }
    }
}

/// `println!` for a command's result, which stops quietly when the reader
/// of stdout goes away instead of panicking.
macro_rules! outln {
    () => {
        $crate::ui::out_str("\n")
    };
    ($($arg:tt)*) => {
        $crate::ui::out_fmt(format_args!($($arg)*))
    };
}
pub(crate) use outln;

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn colour_follows_the_terminal_unless_the_environment_says_otherwise() {
        let auto = |tty, vars: &[(&str, &str)]| wants_color(ColorChoice::Auto, tty, env(vars));
        assert!(auto(true, &[]));
        assert!(!auto(false, &[]));
        assert!(!auto(true, &[("NO_COLOR", "1")]));
        assert!(
            auto(true, &[("NO_COLOR", "")]),
            "an empty NO_COLOR counts as unset"
        );
        assert!(auto(false, &[("FORCE_COLOR", "1")]));
        assert!(auto(false, &[("CLICOLOR_FORCE", "1")]));
        assert!(!auto(false, &[("FORCE_COLOR", "0")]));
        assert!(
            !auto(true, &[("NO_COLOR", "1"), ("FORCE_COLOR", "1")]),
            "NO_COLOR beats FORCE_COLOR"
        );
        assert!(!auto(true, &[("CLICOLOR", "0")]));
        assert!(!auto(true, &[("TERM", "dumb")]));
        assert!(auto(false, &[("TERM", "dumb"), ("FORCE_COLOR", "1")]));
    }

    #[test]
    fn the_flag_beats_the_environment() {
        assert!(wants_color(
            ColorChoice::Always,
            false,
            env(&[("NO_COLOR", "1")])
        ));
        assert!(!wants_color(
            ColorChoice::Never,
            true,
            env(&[("FORCE_COLOR", "1")])
        ));
    }

    #[test]
    fn highlighting_never_changes_the_words() {
        for text in [
            "plain",
            "committed patch `docker` to `base`",
            "`only`",
            "an unmatched ` tick",
            "``",
            "two `a` and `b` then a lone `",
            "first `x`\nsecond line `y` end",
        ] {
            let styled = highlight_if(true, Some(palette::dim()), text);
            assert_ne!(styled, text, "{text:?} is styled");
            assert_eq!(console::strip_ansi_codes(&styled), text);
        }
    }
}
