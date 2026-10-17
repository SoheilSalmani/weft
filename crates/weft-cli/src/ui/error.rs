//! How a failed command reports its error.
//!
//! Off a rich terminal the report is anyhow's own (`Error: …` and its
//! `Caused by:` list), as weft has always printed it. On one, the headline
//! comes first in red, each cause hangs under it, and a file weft could not
//! parse is drawn by miette with the offending lines and the spot marked.

use std::fmt;
use std::io::{self, Write};

use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, SourceCode,
};
use miette::{ThemeCharacters, ThemeStyles};
use weft_engine::parse_error::ParseError;

use super::{highlight, paint, palette, rail, Out};

/// A prompt the person cancelled with Esc or Ctrl-C. The prompt already
/// said so on screen; the command ends with status 130.
#[derive(Debug)]
pub(crate) struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A failure whose output the command already printed: end with this
/// status and print nothing more.
#[derive(Debug)]
pub(crate) struct SilentExit(pub(crate) u8);

impl fmt::Display for SilentExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit status {}", self.0)
    }
}

impl std::error::Error for SilentExit {}

pub(super) fn report(err: &anyhow::Error) {
    if !super::rich(Out::Stderr) {
        rail::close_failed(None);
        let label = paint(Out::Stderr, palette::error().bold(), "Error:");
        let _ = writeln!(io::stderr().lock(), "{label} {err:?}");
        return;
    }
    let mut links: Vec<&(dyn std::error::Error + 'static)> = Vec::new();
    for link in err.chain() {
        // An error that repeats its cause in its own message (Starlark's
        // snippet, mostly) would print the cause twice.
        let repeats = links
            .last()
            .is_some_and(|prev| prev.to_string().contains(&link.to_string()));
        if !repeats {
            links.push(link);
        }
    }
    let headline = links[0].to_string();
    let (first, rest) = headline.split_once('\n').unwrap_or((&headline, ""));
    let mut out = String::new();
    if rail::is_open() {
        rail::close_failed(Some(first));
    } else {
        out.push_str(&format!(
            "{} {}\n",
            paint(Out::Stderr, palette::error().bold(), "error:"),
            highlight(Out::Stderr, None, first)
        ));
    }
    for line in rest.lines() {
        out.push_str(&format!("  {}\n", highlight(Out::Stderr, None, line)));
    }
    let causes = &links[1..];
    for (i, link) in causes.iter().enumerate() {
        let drawn = link.downcast_ref::<ParseError>().and_then(diagnostic);
        if let Some(drawn) = drawn {
            out.push_str(&drawn);
            continue;
        }
        let text = link.to_string();
        let last = i + 1 == causes.len();
        let mut lines = text.lines();
        let joint = if last { "╰─" } else { "├─" };
        let hang = if last { "   " } else { "│  " };
        if let Some(first) = lines.next() {
            let joint = paint(Out::Stderr, palette::dim(), joint);
            out.push_str(&format!(
                "  {joint} {}\n",
                highlight(Out::Stderr, None, first)
            ));
        }
        for line in lines {
            let hang = paint(Out::Stderr, palette::dim(), hang);
            out.push_str(&format!(
                "  {hang} {}\n",
                highlight(Out::Stderr, None, line)
            ));
        }
    }
    let _ = io::stderr().lock().write_all(out.as_bytes());
}

/// A parse error drawn over its source, when the parser named a spot in
/// text there is something to show of.
fn diagnostic(err: &ParseError) -> Option<String> {
    let span = err.span().filter(|_| !err.text().trim().is_empty())?;
    let located = Located {
        message: err.message().to_owned(),
        source: NamedSource::new(shown_path(err.origin()), err.text().to_owned()),
        span: LabeledSpan::underline(span),
    };
    let styles = if super::color(Out::Stderr) {
        ThemeStyles::ansi()
    } else {
        ThemeStyles::none()
    };
    let theme = GraphicalTheme {
        characters: ThemeCharacters::unicode(),
        styles,
    };
    let width = super::width(Out::Stderr).unwrap_or(100);
    let handler = GraphicalReportHandler::new_themed(theme)
        .with_width(width)
        .with_links(false);
    let mut out = String::new();
    handler.render_report(&mut out, &located).ok()?;
    Some(out)
}

/// `origin` relative to the working directory when it lies inside it.
fn shown_path(origin: &str) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| {
            let rel = std::path::Path::new(origin).strip_prefix(cwd).ok()?;
            Some(rel.display().to_string())
        })
        .unwrap_or_else(|| origin.to_owned())
}

#[derive(Debug)]
struct Located {
    message: String,
    source: NamedSource<String>,
    span: LabeledSpan,
}

impl fmt::Display for Located {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Located {}

impl Diagnostic for Located {
    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.source)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(self.span.clone())))
    }
}
