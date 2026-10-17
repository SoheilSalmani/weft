//! The engine's report sink, and how a message looks on stderr.

use std::fmt::Display;
use std::io::{self, Write};

use weft_engine::report::{Progress, Sink, Stream, Tone};

use super::{paint, palette, progress, rail, Out};

pub(super) struct Narrator;

impl Sink for Narrator {
    fn line(&self, stream: Stream, tone: Tone, text: &dyn Display) {
        match stream {
            Stream::Stdout if !super::protocol() => result(tone, text),
            _ => emit(tone, text),
        }
    }

    fn diff(&self, text: &str) {
        if super::protocol() {
            let _ = io::stderr().lock().write_all(text.as_bytes());
        } else {
            super::out_str(&diff(Out::Stdout, text));
        }
    }

    fn progress(&self, label: &str, total: u64) -> Box<dyn Progress> {
        Box::new(progress::Task::steps(label, total))
    }
}

/// Write one message to stderr: plain off a terminal, styled on one, and
/// on the prompt rail while a prompt sequence is open.
pub(crate) fn emit(tone: Tone, text: &dyn Display) {
    progress::suspend(|| {
        if !super::rich(Out::Stderr) && !super::color(Out::Stderr) {
            let _ = writeln!(io::stderr().lock(), "{text}");
            return;
        }
        let text = text.to_string();
        if rail::is_open() {
            rail::line(tone, &text);
        } else {
            let _ = writeln!(io::stderr().lock(), "{}", styled(Out::Stderr, tone, &text));
        }
    });
}

/// A command's result line on stdout. Only `Done` lines (`ok: …`) carry
/// a style; everything else is data and printed as is.
fn result(tone: Tone, text: &dyn Display) {
    if tone == Tone::Done && super::color(Out::Stdout) {
        super::outln!("{}", styled(Out::Stdout, tone, &text.to_string()));
    } else {
        super::outln!("{text}");
    }
}

/// Prefixes the engine and the CLI put in front of a message's words.
/// They keep their words; a terminal sees them in the tone's colour.
const PREFIXES: &[&str] = &[
    "note", "warning", "error", "issue", "check", "dry run", "ok",
];

/// Split `text` after its leading known prefixes (`dry run: note: `).
fn split_prefix(text: &str) -> (&str, &str) {
    let mut at = 0;
    while let Some((head, _)) = text[at..].split_once(": ") {
        if !PREFIXES.contains(&head) {
            break;
        }
        at += head.len() + 2;
    }
    text.split_at(at)
}

/// How `text` reads off the rail: its tone's symbol on a rich terminal,
/// its prefix in the tone's colour, and `code` spans highlighted.
pub(super) fn styled(stream: Out, tone: Tone, text: &str) -> String {
    styled_with(super::rich(stream), super::color(stream), tone, text)
}

/// [`styled`] with the stream's capabilities given: `glyphs` for a tone
/// symbol, `color` for colour. The words never change.
fn styled_with(glyphs: bool, color: bool, tone: Tone, text: &str) -> String {
    let (prefix, words) = split_prefix(text);
    let (symbol, style) = look(tone);
    let glyph = if glyphs && prefix.is_empty() {
        symbol
    } else {
        None
    };
    let base = (tone == Tone::Hint).then(palette::dim);
    let paint =
        |style: console::Style, text: &str| style.force_styling(color).apply_to(text).to_string();
    let mut out = String::with_capacity(text.len() + 24);
    if let Some(glyph) = glyph {
        out.push_str(&paint(style.clone(), glyph));
        out.push(' ');
    }
    if !prefix.is_empty() {
        out.push_str(&paint(style.clone().bold(), prefix.trim_end()));
        out.push(' ');
    }
    let body = super::highlight_if(color, base, words);
    match glyph {
        // Continuation lines sit under the first line's words.
        Some(_) => out.push_str(&body.replace('\n', "\n  ")),
        None => out.push_str(&body),
    }
    out
}

/// A tone's symbol on a rich terminal, and its colour.
pub(super) fn look(tone: Tone) -> (Option<&'static str>, console::Style) {
    match tone {
        Tone::Done => (Some("✓"), palette::success()),
        Tone::Step => (Some("◇"), palette::accent()),
        Tone::Note => (None, palette::note()),
        Tone::Warning => (Some("▲"), palette::warning()),
        Tone::Problem => (Some("■"), palette::error()),
        Tone::Hint => (None, palette::dim()),
        Tone::Info => (None, console::Style::new()),
    }
}

/// A unified diff with its added, removed and hunk-header lines coloured
/// when `stream` takes colour.
pub(crate) fn diff(stream: Out, text: &str) -> std::borrow::Cow<'_, str> {
    if !super::color(stream) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    for line in text.split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let style = if body.starts_with("+++") || body.starts_with("---") {
            Some(palette::bold())
        } else if body.starts_with('+') {
            Some(palette::success())
        } else if body.starts_with('-') {
            Some(palette::error())
        } else if body.starts_with("@@") {
            Some(palette::accent())
        } else {
            None
        };
        match style {
            Some(style) => out.push_str(&paint(stream, style, body).to_string()),
            None => out.push_str(body),
        }
        out.push_str(newline);
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_prefixes_are_split_off() {
        assert_eq!(
            split_prefix("dry run: note: README.md: kept"),
            ("dry run: note: ", "README.md: kept")
        );
        assert_eq!(
            split_prefix("check: render ok (3 files)"),
            ("check: ", "render ok (3 files)")
        );
        assert_eq!(
            split_prefix("README.md: conflict 1 of 2"),
            ("", "README.md: conflict 1 of 2")
        );
        assert_eq!(split_prefix("answers:"), ("", "answers:"));
    }

    #[test]
    fn styling_keeps_every_word_and_adds_only_a_symbol() {
        let cases = [
            (
                Tone::Done,
                "committed patch `docker` with 1 op(s) to `base`",
            ),
            (
                Tone::Note,
                "note: patch `linear` (when use_linear) also creates .mcp.json",
            ),
            (
                Tone::Warning,
                "warning: skipping session `old`: no worktree",
            ),
            (Tone::Problem, "error: question `x`: when does not parse"),
            (
                Tone::Step,
                "hook verify-uv — Verify uv is installed: command -v uv",
            ),
            (Tone::Hint, "review with `git diff patches/`"),
            (Tone::Info, "  line one\n  line two"),
        ];
        for (tone, text) in cases {
            let plain =
                console::strip_ansi_codes(&styled_with(true, true, tone, text)).into_owned();
            let words = match look(tone).0 {
                Some(symbol) if split_prefix(text).0.is_empty() => plain
                    .strip_prefix(&format!("{symbol} "))
                    .expect("symbol first")
                    .replace("\n  ", "\n"),
                _ => plain,
            };
            assert_eq!(words, text, "{tone:?}");
            assert_eq!(
                styled_with(false, false, tone, text),
                text,
                "{tone:?} plain"
            );
        }
    }
}
