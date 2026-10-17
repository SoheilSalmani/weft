//! What the engine tells a person while it works.
//!
//! The engine never styles output. Each message carries a [`Tone`] and the
//! exact words a log or a pipe shows; the binary installs one [`Sink`] at
//! startup that decides how a terminal presents them (colour, symbols, the
//! prompt rail, progress bars). Without a sink, narration goes to stderr and
//! results to stdout as plain text, which is what library callers and tests
//! see, so a plain sink and no sink print the same bytes.

use std::fmt::Display;
use std::sync::OnceLock;

/// What a message is. A sink may present each tone differently, but never
/// changes its words: plain output is the text exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// A finished action: ``committed patch `docker` …``, `update complete …`.
    Done,
    /// Work as it starts: a hook's label, a dry run's `would …` line.
    Step,
    /// Background worth knowing. The text carries its own prefix, such as
    /// `note: ` or `check: `, so plain output reads as it always has.
    Note,
    /// Something is off and the command carries on; the text starts with
    /// `warning: ` where it always has.
    Warning,
    /// One of the problems that fail the command, such as `error: …` lines.
    Problem,
    /// What to run next.
    Hint,
    /// Anything else: headings, list items, an empty listing.
    Info,
}

/// Where a message goes: narration to stderr, a command's result to stdout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Renders the engine's messages. One per process, set with [`install`].
pub trait Sink: Send + Sync {
    /// One message, without a trailing newline; it may span several lines.
    fn line(&self, stream: Stream, tone: Tone, text: &dyn Display);
    /// A unified diff bound for stdout, exactly as rendered, final newline
    /// included.
    fn diff(&self, text: &str);
    /// Start a long task of `total` steps, described by `label`. The task
    /// ends when the returned handle is dropped.
    fn progress(&self, label: &str, total: u64) -> Box<dyn Progress>;
}

/// A long task in flight, from [`progress`]. Dropping it ends the task.
pub trait Progress {
    /// Count `n` more steps as done.
    fn advance(&self, n: u64);
}

static SINK: OnceLock<Box<dyn Sink>> = OnceLock::new();

/// Route every later message through `sink`. The first call wins; the
/// binary makes it once, before any command runs.
pub fn install(sink: Box<dyn Sink>) {
    let _ = SINK.set(sink);
}

fn sink() -> &'static dyn Sink {
    match SINK.get() {
        Some(sink) => sink.as_ref(),
        None => &Plain,
    }
}

/// Narrate a finished action on stderr.
pub fn done(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Done, &text);
}

/// Narrate work as it starts on stderr.
pub fn step(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Step, &text);
}

/// A note on stderr; `text` includes its own prefix (`note: `, `check: `).
pub fn note(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Note, &text);
}

/// A warning on stderr; `text` includes its own `warning: ` prefix.
pub fn warning(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Warning, &text);
}

/// One of the problems that fail a command, on stderr.
pub fn problem(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Problem, &text);
}

/// What to run next, on stderr.
pub fn hint(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Hint, &text);
}

/// Any other narration on stderr.
pub fn info(text: impl Display) {
    sink().line(Stream::Stderr, Tone::Info, &text);
}

/// A line of a command's result on stdout.
pub fn result(tone: Tone, text: impl Display) {
    sink().line(Stream::Stdout, tone, &text);
}

/// A unified diff on stdout, printed as rendered.
pub fn diff(text: &str) {
    sink().diff(text);
}

/// Start a long task of `total` steps; see [`Sink::progress`].
pub fn progress(label: &str, total: u64) -> Box<dyn Progress> {
    sink().progress(label, total)
}

/// The sink in force until the binary installs its own: plain text, no
/// progress display.
struct Plain;

impl Sink for Plain {
    fn line(&self, stream: Stream, _tone: Tone, text: &dyn Display) {
        match stream {
            Stream::Stdout => println!("{text}"),
            Stream::Stderr => eprintln!("{text}"),
        }
    }

    fn diff(&self, text: &str) {
        print!("{text}");
    }

    fn progress(&self, _label: &str, _total: u64) -> Box<dyn Progress> {
        Box::new(Silent)
    }
}

struct Silent;

impl Progress for Silent {
    fn advance(&self, _n: u64) {}
}
