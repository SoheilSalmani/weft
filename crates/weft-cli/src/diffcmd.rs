//! `weft diff`: the pre-commit view of a recording session. Two layers,
//! never mixed on one line: the diff body shows the concrete text the
//! author wrote, with abstraction-candidate spans highlighted (ANSI when a
//! TTY, `⟨…⟩` markers when piped); the legend footer maps highlights to
//! answers. `--abstracted` flips the body to the stored form
//! (`{answer}` placeholders).

use std::collections::BTreeMap;
use std::io::IsTerminal;

use anyhow::Result;
use camino::Utf8PathBuf;
use weft_engine::commit::{Preview, PreviewFile};

const ACCENT: &str = "\x1b[38;5;141m"; // soft indigo
const UNDER: &str = "\x1b[4m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

struct Style {
    tty: bool,
}

impl Style {
    fn span(&self, text: &str) -> String {
        if self.tty {
            format!("{ACCENT}{UNDER}{text}{RESET}{GREEN}")
        } else {
            format!("⟨{text}⟩")
        }
    }
    fn added(&self, text: &str) -> String {
        if self.tty {
            format!("{GREEN}+ {text}{RESET}")
        } else {
            format!("+ {text}")
        }
    }
    fn removed(&self, text: &str) -> String {
        if self.tty {
            format!("{RED}- {text}{RESET}")
        } else {
            format!("- {text}")
        }
    }
    fn dim(&self, text: &str) -> String {
        if self.tty {
            format!("{DIM}{text}{RESET}")
        } else {
            text.to_owned()
        }
    }
    fn bold(&self, text: &str) -> String {
        if self.tty {
            format!("{BOLD}{text}{RESET}")
        } else {
            text.to_owned()
        }
    }
}

/// Spans to highlight on one line: (col, len, answer id), sorted by col.
type SpanMap = BTreeMap<(Utf8PathBuf, usize), Vec<(usize, usize, String)>>;

fn span_map(preview: &Preview) -> SpanMap {
    let mut map: SpanMap = BTreeMap::new();
    for occ in &preview.occurrences {
        map.entry((occ.path.clone(), occ.line)).or_default().push((
            occ.col,
            occ.text.len(),
            occ.id.to_string(),
        ));
    }
    for spans in map.values_mut() {
        spans.sort_by_key(|(col, ..)| *col);
    }
    map
}

/// Render one added line with its candidate spans highlighted (or, in
/// abstracted mode, replaced by `{answer}` placeholders).
fn render_added(
    style: &Style,
    text: &str,
    spans: Option<&Vec<(usize, usize, String)>>,
    abstracted: bool,
) -> String {
    let Some(spans) = spans else {
        return text.to_owned();
    };
    let mut out = String::new();
    let mut cursor = 0usize;
    for (col, len, id) in spans {
        if *col < cursor {
            continue; // overlap safety
        }
        out.push_str(&text[cursor..*col]);
        if abstracted {
            out.push_str(&style.span(&format!("{{{id}}}")));
        } else {
            out.push_str(&style.span(&text[*col..*col + *len]));
        }
        cursor = col + len;
    }
    out.push_str(&text[cursor..]);
    out
}

fn print_file(style: &Style, file: &PreviewFile, spans: &SpanMap, abstracted: bool) {
    let marker = match file.change {
        "created" => style.bold(&format!("+ created   {}", file.path)),
        "deleted" => style.bold(&format!("- deleted   {}", file.path)),
        _ => style.bold(&format!("~ modified  {}", file.path)),
    };
    println!("{marker}");
    for note in &file.notes {
        println!("  {}", style.dim(&format!("note: {note}")));
    }

    // Binary content is opaque — one summary line instead of a body.
    if file.binary {
        println!("  {}", style.dim("(binary file)"));
        println!();
        return;
    }
    match file.change {
        "created" => {
            for (i, line) in file.after.lines().enumerate() {
                let rendered = render_added(
                    style,
                    line,
                    spans.get(&(file.path.clone(), i + 1)),
                    abstracted,
                );
                println!("  {}", style.added(&rendered));
            }
        }
        "deleted" => {
            for line in file.before.lines() {
                println!("  {}", style.removed(line));
            }
        }
        _ => {
            // Unified diff with line numbers on the new side.
            let old: Vec<&str> = file.before.lines().collect();
            let new: Vec<&str> = file.after.lines().collect();
            let diff = similar::TextDiff::from_slices(&old, &new);
            for group in diff.grouped_ops(2) {
                for op in group {
                    match op {
                        similar::DiffOp::Equal { old_index, len, .. } => {
                            for line in &old[old_index..old_index + len] {
                                println!("  {}", style.dim(&format!("  {line}")));
                            }
                        }
                        similar::DiffOp::Delete {
                            old_index, old_len, ..
                        } => {
                            for line in &old[old_index..old_index + old_len] {
                                println!("  {}", style.removed(line));
                            }
                        }
                        similar::DiffOp::Insert {
                            new_index, new_len, ..
                        }
                        | similar::DiffOp::Replace {
                            new_index, new_len, ..
                        } => {
                            if let similar::DiffOp::Replace {
                                old_index, old_len, ..
                            } = op
                            {
                                for line in &old[old_index..old_index + old_len] {
                                    println!("  {}", style.removed(line));
                                }
                            }
                            for (k, line) in new[new_index..new_index + new_len].iter().enumerate()
                            {
                                let rendered = render_added(
                                    style,
                                    line,
                                    spans.get(&(file.path.clone(), new_index + k + 1)),
                                    abstracted,
                                );
                                println!("  {}", style.added(&rendered));
                            }
                        }
                    }
                }
                println!("  {}", style.dim("⋮"));
            }
        }
    }
    println!();
}

pub fn print(preview: &Preview, abstracted: bool) -> Result<()> {
    let style = Style {
        tty: std::io::stdout().is_terminal(),
    };
    if preview.files.is_empty() {
        eprintln!("the worktree matches the base — nothing to commit yet");
        return Ok(());
    }
    let spans = span_map(preview);
    for file in &preview.files {
        print_file(&style, file, &spans, abstracted);
    }

    if !preview.candidates.is_empty() {
        println!("{}", style.bold("abstraction candidates"));
        for c in &preview.candidates {
            let value = if c.mandatory {
                "(secret)".to_owned()
            } else {
                format!("{:?}", c.text)
            };
            let lock = if c.mandatory {
                "  · always abstracted"
            } else {
                ""
            };
            println!(
                "  {} = {}   {} occurrence(s){}",
                style.span(&c.id.to_string()),
                value,
                c.occurrences,
                style.dim(lock),
            );
        }
        println!(
            "{}",
            style.dim(
                "\nhighlighted spans become {answer} references at commit. \
                 `weft commit` confirms; keep one literal with \
                 --keep-literal ANSWER@PATH:LINE[:NTH] or the `select` prompt.",
            )
        );
    }
    Ok(())
}
