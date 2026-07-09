//! Turn (base tree, edited worktree) into a list of abstracted `Op`s with
//! context-anchored hunks.

use std::collections::BTreeMap;

use anyhow::Result;
use similar::{DiffOp, TextDiff};
use weft_core::{AnswerId, Content, Hunk, Op, Tree};

use crate::abstraction::Abstractor;
use crate::interact::Interaction;

const CONTEXT_RADIUS: usize = 2;

/// Diff two trees and build ops. Runs the abstraction confirmation over all
/// texts the ops will contain, then substitutes confirmed answers everywhere
/// (including context lines — a context line that came from an abstracted
/// segment must be abstracted too, or it won't match under other answers).
pub fn build_ops(
    base: &Tree,
    work: &Tree,
    abstractor: &Abstractor,
    interaction: &mut dyn Interaction,
) -> Result<Vec<Op>> {
    // Collect everything that will appear in the patch so confirmation sees
    // the whole picture at once.
    let mut texts: Vec<&str> = Vec::new();
    for (path, entry) in work.iter() {
        match base.get(path) {
            None => {
                texts.push(path.as_str());
                texts.push(&entry.content);
            }
            Some(base_entry) if base_entry.content != entry.content => {
                texts.push(path.as_str());
                texts.push(&entry.content);
                texts.push(&base_entry.content);
            }
            Some(_) => {}
        }
    }
    for path in base.paths().filter(|p| work.get(p).is_none()) {
        texts.push(path.as_str());
    }
    let confirmed = abstractor.confirm(&texts, interaction)?;

    let mut ops = Vec::new();

    // Deletions first (sorted by path via Tree's BTreeMap).
    for path in base.paths().filter(|p| work.get(p).is_none()) {
        ops.push(Op::DeleteFile {
            path: abstractor.path(path.as_str(), &confirmed),
        });
    }
    // Then modifications.
    for (path, entry) in work.iter() {
        if let Some(base_entry) = base.get(path) {
            if base_entry.content != entry.content {
                let hunks = hunks_between(&base_entry.content, &entry.content, |line| {
                    abstractor.line(line, &confirmed)
                });
                ops.push(Op::ModifyFile {
                    path: abstractor.path(path.as_str(), &confirmed),
                    hunks,
                });
            }
            if base_entry.mode != entry.mode {
                ops.push(Op::SetMode {
                    path: abstractor.path(path.as_str(), &confirmed),
                    mode: entry.mode,
                });
            }
        }
    }
    // Then creations.
    for (path, entry) in work.iter() {
        if base.get(path).is_none() {
            ops.push(Op::CreateFile {
                path: abstractor.path(path.as_str(), &confirmed),
                content: abstract_content(&entry.content, abstractor, &confirmed),
                mode: entry.mode,
            });
        }
    }
    Ok(ops)
}

fn abstract_content(
    text: &str,
    abstractor: &Abstractor,
    confirmed: &BTreeMap<AnswerId, bool>,
) -> Content {
    abstractor.content(text, confirmed)
}

/// Context-anchored hunks from concrete old/new text. Change runs closer than
/// `2 * CONTEXT_RADIUS` are grouped into a single hunk (intervening equal
/// lines land in both `removed` and `added`) so hunks apply sequentially
/// without stepping on each other's context.
pub fn hunks_between<F>(old: &str, new: &str, mut abstract_line: F) -> Vec<Hunk>
where
    F: FnMut(&str) -> weft_core::Line,
{
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let diff = TextDiff::from_slices(&old_lines, &new_lines);

    let mut hunks = Vec::new();
    for group in diff.grouped_ops(CONTEXT_RADIUS) {
        let mut hunk = Hunk::default();
        let last = group.len().saturating_sub(1);
        for (i, op) in group.iter().enumerate() {
            match *op {
                DiffOp::Equal { old_index, len, .. } => {
                    let lines = &old_lines[old_index..old_index + len];
                    if i == 0 {
                        hunk.context_before = lines.iter().map(|l| abstract_line(l)).collect();
                    } else if i == last {
                        hunk.context_after = lines.iter().map(|l| abstract_line(l)).collect();
                    } else {
                        // Equal run inside the group: goes to both sides.
                        hunk.removed.extend(lines.iter().map(|l| abstract_line(l)));
                        hunk.added.extend(lines.iter().map(|l| abstract_line(l)));
                    }
                }
                DiffOp::Delete {
                    old_index, old_len, ..
                } => {
                    hunk.removed.extend(
                        old_lines[old_index..old_index + old_len]
                            .iter()
                            .map(|l| abstract_line(l)),
                    );
                }
                DiffOp::Insert {
                    new_index, new_len, ..
                } => {
                    hunk.added.extend(
                        new_lines[new_index..new_index + new_len]
                            .iter()
                            .map(|l| abstract_line(l)),
                    );
                }
                DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    hunk.removed.extend(
                        old_lines[old_index..old_index + old_len]
                            .iter()
                            .map(|l| abstract_line(l)),
                    );
                    hunk.added.extend(
                        new_lines[new_index..new_index + new_len]
                            .iter()
                            .map(|l| abstract_line(l)),
                    );
                }
            }
        }
        hunks.push(hunk);
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use weft_core::Line;

    #[test]
    fn single_change_gets_surrounding_context() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let hunks = hunks_between(old, new, Line::literal);
        assert_eq!(hunks.len(), 1);
        let h = &hunks[0];
        assert_eq!(
            h.context_before,
            vec![Line::literal("a"), Line::literal("b")]
        );
        assert_eq!(h.removed, vec![Line::literal("c")]);
        assert_eq!(h.added, vec![Line::literal("C")]);
        assert_eq!(
            h.context_after,
            vec![Line::literal("d"), Line::literal("e")]
        );
    }

    #[test]
    fn nearby_changes_merge_into_one_hunk() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nB\nc\nD\ne\n";
        let hunks = hunks_between(old, new, Line::literal);
        assert_eq!(hunks.len(), 1, "changes 2 lines apart share one hunk");
        let h = &hunks[0];
        // middle equal line `c` appears on both sides
        assert!(h.removed.contains(&Line::literal("c")));
        assert!(h.added.contains(&Line::literal("c")));
    }

    #[test]
    fn append_at_end_of_file() {
        let old = "a\nb\n";
        let new = "a\nb\nc\n";
        let hunks = hunks_between(old, new, Line::literal);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, vec![Line::literal("c")]);
        assert!(hunks[0].context_after.is_empty());
        assert_eq!(hunks[0].removed, Vec::<Line>::new());
    }
}
