//! Errors from parsing a file a person writes: `weft.toml`, a patch, a
//! preset, an answers file. Each keeps the text it failed on and the byte
//! range the parser blamed, so a terminal can show the offending lines.

use std::fmt;
use std::ops::Range;

/// A TOML or JSON source that failed to parse or deserialize.
///
/// It displays exactly as the parser's own error does, so a log or a pipe
/// reads the same as before; [`ParseError::span`] and [`ParseError::text`]
/// are for a renderer that can point at the spot.
#[derive(Debug)]
pub struct ParseError {
    origin: String,
    text: String,
    span: Option<Range<usize>>,
    message: String,
    display: String,
}

impl ParseError {
    /// A TOML error over `text`, which came from `origin` (a path, or a
    /// label such as `--answers-json` for text that has none).
    pub fn toml(origin: impl fmt::Display, text: &str, err: toml::de::Error) -> Self {
        Self {
            origin: origin.to_string(),
            text: text.to_owned(),
            span: err.span(),
            message: err.message().trim_end().to_owned(),
            display: err.to_string(),
        }
    }

    /// A JSON error over `text`, which came from `origin`.
    pub fn json(origin: impl fmt::Display, text: &str, err: serde_json::Error) -> Self {
        let display = err.to_string();
        let suffix = format!(" at line {} column {}", err.line(), err.column());
        let message = display.strip_suffix(&suffix).unwrap_or(&display).to_owned();
        let span = (err.line() > 0).then(|| {
            let at = offset(text, err.line(), err.column());
            at..at
        });
        Self {
            origin: origin.to_string(),
            text: text.to_owned(),
            span,
            message,
            display,
        }
    }

    /// Where the text came from, as the person would name it.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The whole text that failed to parse.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The byte range the parser blamed, when it named one.
    pub fn span(&self) -> Option<Range<usize>> {
        self.span.clone()
    }

    /// The parser's message without its position or snippet.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

impl std::error::Error for ParseError {}

/// The byte offset of a 1-based line and column (serde_json counts columns
/// in bytes, and reports column 0 at the start of a line).
fn offset(text: &str, line: usize, column: usize) -> usize {
    let start: usize = text
        .split_inclusive('\n')
        .take(line - 1)
        .map(str::len)
        .sum();
    let line_len = text[start..].find('\n').unwrap_or(text.len() - start);
    (start + column.saturating_sub(1).min(line_len)).min(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_errors_point_at_the_byte_serde_blames() {
        let text = "{\n  \"a\": 1,\n  \"b\" 2\n}\n";
        let err = serde_json::from_str::<serde_json::Value>(text).unwrap_err();
        let parsed = ParseError::json("p.json", text, err);
        let at = parsed.span().expect("serde names a position").start;
        assert_eq!(&text[at..at + 1], "2");
        assert_eq!(parsed.message(), "expected `:`");
    }
}
