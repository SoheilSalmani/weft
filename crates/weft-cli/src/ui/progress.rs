//! Spinners and progress bars for work that takes a while: fetching a
//! template, cloning a repository, checking every pair of patches.
//!
//! They draw on stderr, and only on a rich terminal. Off one, a counted
//! task prints a plain progress line every few seconds instead, so a log or
//! an agent can see a long check is alive; a spinner prints nothing.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::{rail, Out};

/// The bar on screen, if any, so messages printed meanwhile can lift it.
static ACTIVE: Mutex<Option<ProgressBar>> = Mutex::new(None);

const HEARTBEAT: Duration = Duration::from_secs(5);

/// A task in flight. Dropping it clears its line.
pub(crate) struct Task {
    bar: Option<ProgressBar>,
    beat: Option<Heartbeat>,
}

struct Heartbeat {
    label: String,
    total: u64,
    done: AtomicU64,
    last: Mutex<Instant>,
}

impl Task {
    /// Work of unknown length, such as a registry request.
    pub(crate) fn spinner(label: impl Into<String>) -> Self {
        Self::draw(label.into(), None, "{spinner:.141} {msg}", true)
    }

    /// A line saying what is running, drawn once and never redrawn: for a
    /// child process that may prompt on the terminal itself (git asking for
    /// credentials), which a ticking spinner would draw over.
    pub(crate) fn status(label: impl Into<String>) -> Self {
        Self::draw(label.into(), None, "{msg}", false)
    }

    /// A download of `total` bytes, or of an unknown size.
    pub(crate) fn bytes(label: impl Into<String>, total: Option<u64>) -> Self {
        match total {
            Some(total) => Self::draw(
                label.into(),
                Some(total),
                "{spinner:.141} {msg} {bar:24.141/dim} {bytes}/{total_bytes}",
                true,
            ),
            None => Self::draw(label.into(), None, "{spinner:.141} {msg} {bytes}", true),
        }
    }

    /// `total` counted steps, described by `label`.
    pub(crate) fn steps(label: &str, total: u64) -> Self {
        let mut task = Self::draw(
            label.to_owned(),
            Some(total),
            "{spinner:.141} {msg} {bar:24.141/dim} {pos}/{len} {eta}",
            true,
        );
        if task.bar.is_none() && total > 0 {
            task.beat = Some(Heartbeat {
                label: label.to_owned(),
                total,
                done: AtomicU64::new(0),
                last: Mutex::new(Instant::now()),
            });
        }
        task
    }

    fn draw(label: String, total: Option<u64>, template: &str, ticking: bool) -> Self {
        if !super::rich(Out::Stderr) {
            return Self {
                bar: None,
                beat: None,
            };
        }
        let template = if rail::is_open() {
            let bar = super::paint(Out::Stderr, console::Style::new().bright().black(), "│");
            format!("{bar}  {template}")
        } else {
            template.to_owned()
        };
        let style = ProgressStyle::with_template(&template)
            .expect("weft's progress templates parse")
            .progress_chars("━╸─")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ");
        let bar = ProgressBar::with_draw_target(total, ProgressDrawTarget::stderr());
        bar.set_style(style);
        bar.set_message(label);
        if ticking {
            bar.enable_steady_tick(Duration::from_millis(80));
        } else {
            bar.tick();
        }
        *active() = Some(bar.clone());
        Self {
            bar: Some(bar),
            beat: None,
        }
    }

    /// Count `n` more steps (or bytes) as done.
    pub(crate) fn inc(&self, n: u64) {
        if let Some(bar) = &self.bar {
            bar.inc(n);
        }
        if let Some(beat) = &self.beat {
            let done = beat.done.fetch_add(n, Ordering::Relaxed) + n;
            let mut last = beat.last.lock().unwrap_or_else(|p| p.into_inner());
            if last.elapsed() >= HEARTBEAT && done < beat.total {
                *last = Instant::now();
                drop(last);
                weft_engine::report::info(format_args!("{} {done}/{}", beat.label, beat.total));
            }
        }
    }
}

impl weft_engine::report::Progress for Task {
    fn advance(&self, n: u64) {
        self.inc(n);
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
            *active() = None;
        }
    }
}

fn active() -> std::sync::MutexGuard<'static, Option<ProgressBar>> {
    ACTIVE.lock().unwrap_or_else(|p| p.into_inner())
}

/// Run `f`, which writes to stderr, with any bar lifted off the screen.
pub(super) fn suspend<R>(f: impl FnOnce() -> R) -> R {
    let bar = active().clone();
    match bar {
        Some(bar) => bar.suspend(f),
        None => f(),
    }
}
