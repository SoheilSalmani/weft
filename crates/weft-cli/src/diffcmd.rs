//! `weft diff`: the pre-commit view of a recording session. Two layers,
//! never mixed on one line: the diff body shows the concrete text the
//! author wrote, with abstraction-candidate spans highlighted (colour when
//! stdout takes it, `⟨…⟩` markers when piped); the legend footer maps highlights to
//! answers. `--abstracted` flips the body to the stored form
//! (`{answer}` placeholders).

use std::collections::BTreeMap;

use anyhow::Result;
use camino::Utf8PathBuf;
use console::Style as Look;
use weft_engine::commit::{Preview, PreviewFile};
use weft_engine::report;

use crate::ui::{self, palette, Out};

/// Colour on stdout → painted segments; off → the `⟨…⟩` markers piped
/// output has always used. Every segment is painted on its own, so no
/// colour carries over from one segment to the next.
struct Style {
    color: bool,
}

impl Style {
    fn paint(&self, look: Look, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        ui::paint(Out::Stdout, look, text).to_string()
    }
    fn span(&self, text: &str) -> String {
        if self.color {
            self.paint(palette::accent().underlined(), text)
        } else {
            format!("⟨{text}⟩")
        }
    }
    fn green(&self, text: &str) -> String {
        self.paint(palette::success(), text)
    }
    fn removed(&self, text: &str) -> String {
        self.paint(palette::error(), &format!("- {text}"))
    }
    fn dim(&self, text: &str) -> String {
        self.paint(palette::dim(), text)
    }
    fn bold(&self, text: &str) -> String {
        self.paint(palette::bold(), text)
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

/// Render one added line (`+ ` prefix included) with its candidate spans
/// highlighted (or, in abstracted mode, replaced by `{answer}` placeholders).
fn render_added(
    style: &Style,
    text: &str,
    spans: Option<&Vec<(usize, usize, String)>>,
    abstracted: bool,
) -> String {
    let Some(spans) = spans else {
        return style.green(&format!("+ {text}"));
    };
    let mut out = style.green("+ ");
    let mut cursor = 0usize;
    for (col, len, id) in spans {
        if *col < cursor {
            continue; // overlap safety
        }
        if cursor < *col {
            out.push_str(&style.green(&text[cursor..*col]));
        }
        if abstracted {
            out.push_str(&style.span(&format!("{{{id}}}")));
        } else {
            out.push_str(&style.span(&text[*col..*col + *len]));
        }
        cursor = col + len;
    }
    if cursor < text.len() {
        out.push_str(&style.green(&text[cursor..]));
    }
    out
}

fn print_file(style: &Style, file: &PreviewFile, spans: &SpanMap, abstracted: bool) {
    let marker = match file.change {
        "created" => style.bold(&format!("+ created   {}", file.path)),
        "deleted" => style.bold(&format!("- deleted   {}", file.path)),
        _ => style.bold(&format!("~ modified  {}", file.path)),
    };
    ui::outln!("{marker}");
    for note in &file.notes {
        ui::outln!("  {}", style.dim(&format!("note: {note}")));
    }

    // Binary content is opaque — one summary line instead of a body.
    if file.binary {
        ui::outln!("  {}", style.dim("(binary file)"));
        ui::outln!();
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
                ui::outln!("  {rendered}");
            }
        }
        "deleted" => {
            for line in file.before.lines() {
                ui::outln!("  {}", style.removed(line));
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
                                ui::outln!("  {}", style.dim(&format!("  {line}")));
                            }
                        }
                        similar::DiffOp::Delete {
                            old_index, old_len, ..
                        } => {
                            for line in &old[old_index..old_index + old_len] {
                                ui::outln!("  {}", style.removed(line));
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
                                    ui::outln!("  {}", style.removed(line));
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
                                ui::outln!("  {rendered}");
                            }
                        }
                    }
                }
                ui::outln!("  {}", style.dim("⋮"));
            }
        }
    }
    ui::outln!();
}

pub fn print(preview: &Preview, abstracted: bool) -> Result<()> {
    let style = Style {
        color: ui::color(Out::Stdout),
    };
    if preview.files.is_empty() {
        report::info("the worktree matches the base — nothing to commit yet");
        return Ok(());
    }
    let spans = span_map(preview);
    for file in &preview.files {
        print_file(&style, file, &spans, abstracted);
    }

    if !preview.candidates.is_empty() {
        ui::outln!("{}", style.bold("abstraction candidates"));
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
            ui::outln!(
                "  {} = {}   {} occurrence(s){}",
                style.span(&c.id.to_string()),
                value,
                c.occurrences,
                style.dim(lock),
            );
        }
        ui::outln!(
            "{}",
            ui::highlight(
                Out::Stdout,
                Some(palette::dim()),
                "\nhighlighted spans become {answer} references at commit. \
                 `weft commit` confirms; keep one literal with \
                 --keep-literal ANSWER@PATH:LINE[:NTH] or the `select` prompt.",
            )
        );
    }
    Ok(())
}
