//! The prompt rail: the left bar that ties a command's prompts together in
//! a terminal, the way `cliclack` draws them.
//!
//! It opens when the first prompt is drawn (never for a command whose
//! answers all came from flags), carries every message the command prints
//! while it is open, and closes when the command ends: on success with the
//! last finished action as its closing line, on failure with the error's
//! headline. Child processes (hooks, generators) write past it; their
//! output is theirs.

use std::io::{self, Write};
use std::sync::Mutex;

use cliclack::ThemeState;
use console::Style;
use weft_engine::report::Tone;

use super::{highlight, paint, palette, Out};

const BAR: &str = "│";
const BAR_START: &str = "┌";
const BAR_END: &str = "└";

struct State {
    title: String,
    open: bool,
    /// A bar-only line is owed before the next symbol line.
    spacer: bool,
    /// The latest finished action, held back so the rail can close on it.
    held: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    title: String::new(),
    open: false,
    spacer: false,
    held: None,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The rail's heading, such as `weft session new`.
pub(crate) fn set_title(title: String) {
    state().title = title;
}

/// Whether a prompt sequence is open.
pub(crate) fn is_open() -> bool {
    state().open
}

/// Ready the rail for a prompt: open it on the first one, and settle any
/// message still owed to it.
pub(crate) fn before_prompt() {
    let mut s = state();
    let mut err = io::stderr().lock();
    if !s.open {
        let (head, rest) = s.title.split_once(' ').unwrap_or((s.title.as_str(), ""));
        let _ = writeln!(
            err,
            "{}  {} {}\n{}",
            bar(BAR_START),
            paint(Out::Stderr, palette::accent().bold(), head),
            paint(Out::Stderr, palette::bold(), rest),
            bar(BAR)
        );
        s.open = true;
        s.spacer = false;
    }
    flush_held(&mut s, &mut err);
    if std::mem::take(&mut s.spacer) {
        let _ = writeln!(err, "{}", bar(BAR));
    }
}

/// One message while the rail is open.
pub(super) fn line(tone: Tone, text: &str) {
    let mut s = state();
    let mut err = io::stderr().lock();
    flush_held(&mut s, &mut err);
    match tone {
        Tone::Done => s.held = Some(text.to_owned()),
        Tone::Info | Tone::Hint => {
            let base = (tone == Tone::Hint).then(palette::dim);
            for line in text.split('\n') {
                let _ = writeln!(
                    err,
                    "{}  {}",
                    bar(BAR),
                    highlight(Out::Stderr, base.clone(), line)
                );
            }
            s.spacer = true;
        }
        _ => block(&mut s, &mut err, tone, text),
    }
}

/// End the rail after a command succeeded.
pub(crate) fn close_ok() {
    let mut s = state();
    if !s.open {
        return;
    }
    let mut err = io::stderr().lock();
    if std::mem::take(&mut s.spacer) {
        let _ = writeln!(err, "{}", bar(BAR));
    }
    let last = s.held.take().unwrap_or_default();
    let _ = writeln!(
        err,
        "{}  {}",
        bar(BAR_END),
        highlight(Out::Stderr, None, &last)
    );
    s.open = false;
}

/// End the rail after a command failed, closing on `headline` when given.
pub(crate) fn close_failed(headline: Option<&str>) {
    let mut s = state();
    if !s.open {
        return;
    }
    let mut err = io::stderr().lock();
    flush_held(&mut s, &mut err);
    if std::mem::take(&mut s.spacer) {
        let _ = writeln!(err, "{}", bar(BAR));
    }
    let end = paint(Out::Stderr, palette::error(), BAR_END);
    match headline {
        Some(text) => {
            let _ = writeln!(
                err,
                "{end}  {}",
                highlight(Out::Stderr, Some(palette::error()), text)
            );
        }
        None => {
            let _ = writeln!(err, "{end}");
        }
    }
    s.open = false;
}

/// Forget the rail after a cancelled prompt, which drew its own ending.
pub(crate) fn abandon() {
    let mut s = state();
    s.open = false;
    s.held = None;
    s.spacer = false;
}

fn flush_held(s: &mut State, err: &mut impl Write) {
    if let Some(text) = s.held.take() {
        block(s, err, Tone::Done, &text);
    }
}

/// A message with its tone's symbol, continuation lines on the bar, and a
/// bar-only line after it, as cliclack spaces its own steps.
fn block(s: &mut State, err: &mut impl Write, tone: Tone, text: &str) {
    if std::mem::take(&mut s.spacer) {
        let _ = writeln!(err, "{}", bar(BAR));
    }
    let (symbol, style) = match tone {
        Tone::Done => ("◆", palette::success()),
        Tone::Step => ("◇", palette::accent()),
        Tone::Note => ("●", palette::note()),
        Tone::Warning => ("▲", palette::warning()),
        Tone::Problem => ("■", palette::error()),
        Tone::Hint | Tone::Info => (BAR, bar_style()),
    };
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or_default();
    let _ = writeln!(
        err,
        "{}  {}",
        paint(Out::Stderr, style, symbol),
        highlight(Out::Stderr, None, first)
    );
    for line in lines {
        let _ = writeln!(err, "{}  {}", bar(BAR), highlight(Out::Stderr, None, line));
    }
    let _ = writeln!(err, "{}", bar(BAR));
}

fn bar_style() -> Style {
    Style::new().bright().black()
}

fn bar(glyph: &str) -> console::StyledObject<&str> {
    paint(Out::Stderr, bar_style(), glyph)
}

/// cliclack's look in weft's colours: the active prompt in the accent.
pub(crate) struct Theme;

impl cliclack::Theme for Theme {
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => palette::accent(),
            ThemeState::Cancel => palette::error(),
            ThemeState::Submit => bar_style(),
            ThemeState::Error(_) => palette::warning(),
        }
    }
}
