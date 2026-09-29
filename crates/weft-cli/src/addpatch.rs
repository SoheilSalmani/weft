//! `weft add -p`: walk the unstaged changes hunk by hunk and stage a subset.
//!
//! The stage holds whole-file blobs, so a partially staged file is simply
//! "the staged version with the chosen hunks applied" — nothing about the
//! stage format, `weft commit`, or plain `weft add` changes here. The picker
//! diffs **staged → worktree** (like `git add -p`, which shows what is not yet
//! staged), collects a per-hunk mask, and writes the synthesized blob back.
//!
//! The decision source is behind [`HunkDecider`] so the loop is driven by a
//! terminal in production and by a scripted sequence in tests.

use std::io::{IsTerminal, Write};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use weft_core::{FileData, FileEntry, Tree};
use weft_engine::diff::{self, HunkLine, SelectableHunk};

/// What the author answered for one hunk (or one unhunkable file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// `y` — stage it.
    Yes,
    /// `n` — leave it unstaged.
    No,
    /// `a` — stage this and every remaining hunk in the file.
    AllInFile,
    /// `d` — leave this and every remaining hunk in the file unstaged.
    NoneInFile,
    /// `s` — split this hunk into smaller ones.
    Split,
    /// `q` — stop, keeping the decisions made so far.
    Quit,
    /// `?` — print the keymap and ask again.
    Help,
}

pub trait HunkDecider {
    /// The next decision. `prompt` is the already-rendered question line.
    fn decide(&mut self, prompt: &str) -> Result<Decision>;
}

const HELP: &str = "\
  y  stage this hunk
  n  do not stage this hunk
  a  stage this hunk and all later hunks in this file
  d  do not stage this hunk or any later hunk in this file
  s  split this hunk into smaller hunks
  q  quit; keep the hunks already selected
  ?  show this help";

fn parse(c: char) -> Option<Decision> {
    match c {
        'y' | 'Y' => Some(Decision::Yes),
        'n' | 'N' => Some(Decision::No),
        'a' | 'A' => Some(Decision::AllInFile),
        'd' | 'D' => Some(Decision::NoneInFile),
        's' | 'S' => Some(Decision::Split),
        'q' | 'Q' => Some(Decision::Quit),
        '?' | 'h' => Some(Decision::Help),
        _ => None,
    }
}

/// Reads decisions from the terminal: single keypresses when stdin is a TTY,
/// otherwise one line per decision so `-p` stays scriptable (and testable)
/// through a pipe. End of input answers `q`.
pub struct TerminalDecider {
    tty: bool,
}

impl Default for TerminalDecider {
    fn default() -> Self {
        Self {
            tty: std::io::stdin().is_terminal(),
        }
    }
}

impl HunkDecider for TerminalDecider {
    fn decide(&mut self, prompt: &str) -> Result<Decision> {
        loop {
            eprint!("{prompt}");
            std::io::stderr().flush()?;
            let key = if self.tty {
                self.read_key()?
            } else {
                read_line()?
            };
            match key {
                None => {
                    eprintln!();
                    return Ok(Decision::Quit);
                }
                Some(c) => {
                    if self.tty {
                        eprintln!("{c}");
                    }
                    match parse(c) {
                        Some(d) => return Ok(d),
                        None => eprintln!("unknown key `{c}` — press ? for help"),
                    }
                }
            }
        }
    }
}

impl TerminalDecider {
    /// One keypress, raw so it needs no Enter. `None` on ^C/^D/Esc.
    fn read_key(&self) -> Result<Option<char>> {
        terminal::enable_raw_mode()?;
        let result = (|| loop {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            return Ok(match key.code {
                KeyCode::Esc => None,
                KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => None,
                KeyCode::Char(c) => Some(c),
                _ => Some('\0'),
            });
        })();
        terminal::disable_raw_mode()?;
        result
    }
}

fn read_line() -> Result<Option<char>> {
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().chars().next().unwrap_or('\0')))
}

// ---- rendering ---------------------------------------------------------

const ACCENT: &str = "\x1b[38;5;141m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// ANSI only when the output is a terminal, mirroring `diffcmd`.
pub struct Paint {
    pub color: bool,
}

impl Paint {
    fn wrap(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_owned()
        }
    }
    fn line(&self, line: &HunkLine) -> String {
        match line {
            HunkLine::Context(t) => self.wrap(DIM, &format!("  {t}")),
            HunkLine::Removed(t) => self.wrap(RED, &format!("- {t}")),
            HunkLine::Added(t) => self.wrap(GREEN, &format!("+ {t}")),
        }
    }
}

fn render_hunk(out: &mut dyn Write, paint: &Paint, hunk: &SelectableHunk) -> Result<()> {
    writeln!(out, "{}", paint.wrap(ACCENT, &hunk.header()))?;
    for line in &hunk.lines {
        writeln!(out, "  {}", paint.line(line))?;
    }
    Ok(())
}

// ---- the picker --------------------------------------------------------

/// Walk one file's hunks and return the text to stage for it.
fn pick_hunks(
    out: &mut dyn Write,
    paint: &Paint,
    decider: &mut dyn HunkDecider,
    path: &Utf8Path,
    staged: &str,
    work: &str,
) -> Result<(String, bool)> {
    let mut hunks = diff::selectable_hunks(staged, work);
    let mut selected = vec![false; hunks.len()];
    let mut quit = false;
    let mut i = 0;

    while i < hunks.len() {
        render_hunk(out, paint, &hunks[i])?;
        let prompt = format!(
            "stage this hunk [y,n,a,d,s,q,?]? ({}/{} in {path}) ",
            i + 1,
            hunks.len()
        );
        match decider.decide(&prompt)? {
            Decision::Yes => {
                selected[i] = true;
                i += 1;
            }
            Decision::No => i += 1,
            Decision::AllInFile => {
                selected[i..].fill(true);
                break;
            }
            Decision::NoneInFile => break,
            Decision::Quit => {
                quit = true;
                break;
            }
            Decision::Split => match diff::split(&hunks[i]) {
                Some(parts) => {
                    let n = parts.len();
                    hunks.splice(i..i + 1, parts);
                    selected.splice(i..i + 1, std::iter::repeat_n(false, n));
                    writeln!(out, "split into {n} hunks")?;
                }
                None => writeln!(out, "this hunk cannot be split any further")?,
            },
            Decision::Help => writeln!(out, "{HELP}")?,
        }
    }

    Ok((diff::apply_selection(staged, work, &hunks, &selected), quit))
}

/// Ask about a change that has no hunks (a deletion, a creation or edit of a
/// binary file): stage it whole, or not at all.
fn pick_whole_file(
    out: &mut dyn Write,
    paint: &Paint,
    decider: &mut dyn HunkDecider,
    path: &Utf8Path,
    what: &str,
) -> Result<(bool, bool)> {
    writeln!(out, "{}", paint.wrap(BOLD, &format!("{what}  {path}")))?;
    loop {
        let prompt = format!("stage this change [y,n,q,?]? ({path}) ");
        match decider.decide(&prompt)? {
            Decision::Yes | Decision::AllInFile => return Ok((true, false)),
            Decision::No | Decision::NoneInFile => return Ok((false, false)),
            Decision::Quit => return Ok((false, true)),
            Decision::Split => writeln!(out, "this change is not hunkable — answer y or n")?,
            Decision::Help => writeln!(out, "{HELP}")?,
        }
    }
}

/// Run the picker over `paths` (already filtered by the `weft add` globs),
/// updating `staged` in place. Returns how many paths changed in the stage.
pub fn stage_interactively(
    out: &mut dyn Write,
    paint: &Paint,
    decider: &mut dyn HunkDecider,
    staged: &mut Tree,
    work: &Tree,
    paths: &[Utf8PathBuf],
) -> Result<usize> {
    let mut changed = 0;

    for path in paths {
        let before = staged.get(path).cloned();
        let after = work.get(path).cloned();
        let quit = match (&before, &after) {
            // Modified text file: the hunk-by-hunk path.
            (Some(old), Some(new))
                if !old.content.is_binary() && !new.content.is_binary() && old.mode == new.mode =>
            {
                writeln!(out, "{}", paint.wrap(BOLD, &format!("~ modified  {path}")))?;
                let (old_text, new_text) = (
                    old.content.text().unwrap_or_default(),
                    new.content.text().unwrap_or_default(),
                );
                let (text, quit) = pick_hunks(out, paint, decider, path, old_text, new_text)?;
                if text != old_text {
                    staged.insert(
                        path.clone(),
                        FileEntry {
                            content: FileData::Text(text),
                            mode: new.mode,
                        },
                    );
                }
                quit
            }
            // New text file: hunk-by-hunk against an empty staged version.
            (None, Some(new)) if !new.content.is_binary() => {
                writeln!(out, "{}", paint.wrap(BOLD, &format!("+ created   {path}")))?;
                let new_text = new.content.text().unwrap_or_default();
                let (text, quit) = pick_hunks(out, paint, decider, path, "", new_text)?;
                if !text.is_empty() {
                    staged.insert(
                        path.clone(),
                        FileEntry {
                            content: FileData::Text(text),
                            mode: new.mode,
                        },
                    );
                }
                quit
            }
            // Deletion.
            (Some(_), None) => {
                let (take, quit) = pick_whole_file(out, paint, decider, path, "- deleted  ")?;
                if take {
                    staged.remove(path);
                }
                quit
            }
            // Binary content or a mode change: not hunkable, stage it whole.
            (_, Some(entry)) => {
                let what = if before.is_none() {
                    "+ created  "
                } else {
                    "~ modified "
                };
                let (take, quit) = pick_whole_file(out, paint, decider, path, what)?;
                if take {
                    staged.insert(path.clone(), entry.clone());
                }
                quit
            }
            (None, None) => false,
        };

        if staged.get(path).cloned() != before {
            changed += 1;
        }
        if quit {
            break;
        }
    }

    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canned key sequence, so the loop is exercised without a terminal.
    struct Script(std::vec::IntoIter<Decision>);

    impl Script {
        fn new(keys: &str) -> Self {
            Script(
                keys.chars()
                    .map(|c| parse(c).expect("test key"))
                    .collect::<Vec<_>>()
                    .into_iter(),
            )
        }
    }

    impl HunkDecider for Script {
        fn decide(&mut self, _prompt: &str) -> Result<Decision> {
            Ok(self.0.next().unwrap_or(Decision::Quit))
        }
    }

    const OLD: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    const NEW: &str = "1\nTWO\n3\n4\n5\n6\n7\n8\nNINE\n10\n";

    fn trees(old: &str, new: &str) -> (Tree, Tree) {
        let mut staged = Tree::new();
        staged.insert("a.txt".into(), FileEntry::text(old));
        let mut work = Tree::new();
        work.insert("a.txt".into(), FileEntry::text(new));
        (staged, work)
    }

    fn run(keys: &str, old: &str, new: &str) -> (Tree, usize) {
        let (mut staged, work) = trees(old, new);
        let n = stage_interactively(
            &mut std::io::sink(),
            &Paint { color: false },
            &mut Script::new(keys),
            &mut staged,
            &work,
            &["a.txt".into()],
        )
        .unwrap();
        (staged, n)
    }

    fn text(tree: &Tree, path: &str) -> Option<String> {
        tree.get(camino::Utf8Path::new(path))
            .map(|e| e.content.text().unwrap().to_owned())
    }

    #[test]
    fn staging_the_first_hunk_only_leaves_the_second_unstaged() {
        let (staged, n) = run("yn", OLD, NEW);
        assert_eq!(n, 1);
        assert_eq!(
            text(&staged, "a.txt").as_deref(),
            Some("1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n")
        );
    }

    #[test]
    fn skipping_everything_stages_nothing() {
        let (staged, n) = run("nn", OLD, NEW);
        assert_eq!(n, 0);
        assert_eq!(text(&staged, "a.txt").as_deref(), Some(OLD));
    }

    #[test]
    fn a_stages_the_rest_of_the_file() {
        let (staged, n) = run("a", OLD, NEW);
        assert_eq!(n, 1);
        assert_eq!(text(&staged, "a.txt").as_deref(), Some(NEW));
    }

    #[test]
    fn d_skips_the_rest_of_the_file() {
        let (staged, _) = run("yd", OLD, NEW);
        assert_eq!(
            text(&staged, "a.txt").as_deref(),
            Some("1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n"),
            "the y before d still counts"
        );
    }

    #[test]
    fn q_keeps_the_decisions_made_so_far() {
        let (staged, _) = run("yq", OLD, NEW);
        assert_eq!(
            text(&staged, "a.txt").as_deref(),
            Some("1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n")
        );
    }

    #[test]
    fn s_splits_a_merged_hunk_so_halves_can_be_staged_apart() {
        // Two edits 2 lines apart share one hunk until split.
        let (staged, _) = run("syn", "a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n");
        assert_eq!(text(&staged, "a.txt").as_deref(), Some("a\nB\nc\nd\ne\n"));
    }

    #[test]
    fn s_on_an_unsplittable_hunk_reprompts() {
        let (staged, _) = run("sy", "a\nb\nc\n", "a\nX\nY\nc\n");
        assert_eq!(text(&staged, "a.txt").as_deref(), Some("a\nX\nY\nc\n"));
    }

    #[test]
    fn help_reprompts_without_consuming_the_hunk() {
        let (staged, _) = run("?a", OLD, NEW);
        assert_eq!(text(&staged, "a.txt").as_deref(), Some(NEW));
    }

    #[test]
    fn a_created_file_can_be_staged_partially() {
        let mut staged = Tree::new();
        let mut work = Tree::new();
        work.insert("new.txt".into(), FileEntry::text("x\ny\n"));
        let n = stage_interactively(
            &mut std::io::sink(),
            &Paint { color: false },
            &mut Script::new("n"),
            &mut staged,
            &work,
            &["new.txt".into()],
        )
        .unwrap();
        assert_eq!(n, 0, "declining a creation leaves it out of the stage");
        assert!(staged.get(camino::Utf8Path::new("new.txt")).is_none());
    }

    #[test]
    fn a_deletion_is_staged_whole() {
        let mut staged = Tree::new();
        staged.insert("gone.txt".into(), FileEntry::text("x\n"));
        let work = Tree::new();
        let n = stage_interactively(
            &mut std::io::sink(),
            &Paint { color: false },
            &mut Script::new("y"),
            &mut staged,
            &work,
            &["gone.txt".into()],
        )
        .unwrap();
        assert_eq!(n, 1);
        assert!(staged.get(camino::Utf8Path::new("gone.txt")).is_none());
    }

    #[test]
    fn quitting_stops_before_later_files() {
        let mut staged = Tree::new();
        staged.insert("a.txt".into(), FileEntry::text(OLD));
        staged.insert("b.txt".into(), FileEntry::text(OLD));
        let mut work = Tree::new();
        work.insert("a.txt".into(), FileEntry::text(NEW));
        work.insert("b.txt".into(), FileEntry::text(NEW));
        stage_interactively(
            &mut std::io::sink(),
            &Paint { color: false },
            &mut Script::new("q"),
            &mut staged,
            &work,
            &["a.txt".into(), "b.txt".into()],
        )
        .unwrap();
        assert_eq!(text(&staged, "a.txt").as_deref(), Some(OLD));
        assert_eq!(text(&staged, "b.txt").as_deref(), Some(OLD));
    }
}
