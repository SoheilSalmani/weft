//! Turn (base tree, edited worktree) into a list of abstracted `Op`s with
//! context-anchored hunks.

use std::collections::BTreeMap;

use anyhow::Result;
use similar::{DiffOp, TextDiff};
use weft_core::{AnswerId, Hunk, Op, Tree};

use crate::abstraction::Abstractor;
use crate::interact::Interaction;

const CONTEXT_RADIUS: usize = 2;

/// Everything that will appear in the patch (paths, changed content, base
/// content of modified files), so abstraction confirmation sees the whole
/// picture at once.
pub fn collect_texts<'t>(base: &'t Tree, work: &'t Tree) -> Vec<&'t str> {
    let mut texts: Vec<&str> = Vec::new();
    for (path, entry) in work.iter() {
        match base.get(path) {
            None => {
                texts.push(path.as_str());
                // Binary content is opaque — only paths feed the abstractor.
                texts.extend(entry.content.text());
            }
            Some(base_entry) if base_entry.content != entry.content => {
                texts.push(path.as_str());
                texts.extend(entry.content.text());
                texts.extend(base_entry.content.text());
            }
            Some(_) => {}
        }
    }
    for path in base.paths().filter(|p| work.get(p).is_none()) {
        texts.push(path.as_str());
    }
    texts
}

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
    let texts = collect_texts(base, work);
    let occurrences = added_occurrences(base, work, abstractor);
    let (confirmed, excepted) =
        abstractor.confirm_with_occurrences(&texts, &occurrences, interaction)?;
    Ok(build_ops_decided(
        base, work, abstractor, &confirmed, &excepted,
    ))
}

/// Every abstraction occurrence in *authored* content: created files (all
/// lines) and the added lines of modified files — the units a user may keep
/// literal. Keys are stable across decisions.
pub fn added_occurrences(
    base: &Tree,
    work: &Tree,
    abstractor: &Abstractor,
) -> Vec<crate::abstraction::Occurrence> {
    let mut out = Vec::new();
    for (path, entry) in work.iter() {
        // Binary files carry no abstractable occurrences.
        let Some(text) = entry.content.text() else {
            continue;
        };
        match base.get(path) {
            None => out.extend(abstractor.occurrences_in(path, text, None)),
            Some(base_entry) if base_entry.content != entry.content => {
                let Some(base_text) = base_entry.content.text() else {
                    continue;
                };
                let added = added_line_numbers(base_text, text);
                out.extend(abstractor.occurrences_in(path, text, Some(&added)));
            }
            Some(_) => {}
        }
    }
    out
}

/// 1-based line numbers (in the new content) that a diff marks as added.
pub fn added_line_numbers(old: &str, new: &str) -> std::collections::BTreeSet<usize> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let diff = TextDiff::from_slices(&old_lines, &new_lines);
    let mut added = std::collections::BTreeSet::new();
    for op in diff.ops() {
        if let DiffOp::Insert {
            new_index, new_len, ..
        }
        | DiffOp::Replace {
            new_index, new_len, ..
        } = *op
        {
            for i in new_index..new_index + new_len {
                added.insert(i + 1);
            }
        }
    }
    added
}

/// [`build_ops`] with the abstraction decisions supplied by the caller
/// (e.g. a web UI) instead of an interactive prompt loop.
pub fn build_ops_with(
    base: &Tree,
    work: &Tree,
    abstractor: &Abstractor,
    confirmed: &BTreeMap<AnswerId, bool>,
) -> Vec<Op> {
    build_ops_decided(base, work, abstractor, confirmed, &Default::default())
}

/// [`build_ops_with`] plus per-occurrence exceptions for authored content
/// (added lines + created files).
pub fn build_ops_decided(
    base: &Tree,
    work: &Tree,
    abstractor: &Abstractor,
    confirmed: &BTreeMap<AnswerId, bool>,
    excepted: &crate::abstraction::Excepted,
) -> Vec<Op> {
    let mut ops = Vec::new();

    // Rename detection: a deleted file whose exact content reappears at a
    // created path records as `rename_path` (plus `set_mode` if the mode
    // moved) instead of delete + create. First match wins, by path order.
    let mut renamed_from: std::collections::BTreeSet<&camino::Utf8PathBuf> = Default::default();
    let mut renamed_to: std::collections::BTreeSet<&camino::Utf8PathBuf> = Default::default();
    for (from, base_entry) in base.iter() {
        if work.get(from).is_some() {
            continue;
        }
        let target = work.iter().find(|(to, entry)| {
            base.get(to).is_none()
                && !renamed_to.contains(to)
                && entry.content == base_entry.content
        });
        if let Some((to, entry)) = target {
            ops.push(Op::RenamePath {
                from: abstractor.path(from.as_str(), confirmed),
                to: abstractor.path(to.as_str(), confirmed),
            });
            if entry.mode != base_entry.mode {
                ops.push(Op::SetMode {
                    path: abstractor.path(to.as_str(), confirmed),
                    mode: entry.mode,
                });
            }
            renamed_from.insert(from);
            renamed_to.insert(to);
        }
    }

    // Deletions first (sorted by path via Tree's BTreeMap).
    for path in base
        .paths()
        .filter(|p| work.get(p).is_none() && !renamed_from.contains(p))
    {
        ops.push(Op::DeleteFile {
            path: abstractor.path(path.as_str(), confirmed),
        });
    }
    // Then modifications.
    for (path, entry) in work.iter() {
        if let Some(base_entry) = base.get(path) {
            if base_entry.content != entry.content {
                match (base_entry.content.text(), entry.content.text()) {
                    (Some(base_text), Some(text)) => {
                        let hunks = hunks_between(base_text, text, |line, new_line| {
                            match new_line {
                                // Added lines are authored: exceptions apply.
                                Some(line_no) => {
                                    abstractor.line_at(path, line_no, line, confirmed, excepted)
                                }
                                // Context/removed lines mirror the base render.
                                None => abstractor.line(line, confirmed),
                            }
                        });
                        ops.push(Op::ModifyFile {
                            path: abstractor.path(path.as_str(), confirmed),
                            hunks,
                        });
                    }
                    // Any binary side: no hunks — replace wholesale
                    // (delete + create in this one patch, applied in order).
                    _ => {
                        ops.push(Op::DeleteFile {
                            path: abstractor.path(path.as_str(), confirmed),
                        });
                        ops.push(create_op(path, entry, abstractor, confirmed, excepted));
                    }
                }
            }
            if base_entry.mode != entry.mode {
                ops.push(Op::SetMode {
                    path: abstractor.path(path.as_str(), confirmed),
                    mode: entry.mode,
                });
            }
        }
    }
    // Then creations.
    for (path, entry) in work.iter() {
        if base.get(path).is_none() && !renamed_to.contains(path) {
            ops.push(create_op(path, entry, abstractor, confirmed, excepted));
        }
    }
    ops
}

/// The create op for one worktree file: text content is abstracted; binary
/// content is stored as an opaque base64 blob (the path still abstracts).
fn create_op(
    path: &camino::Utf8Path,
    entry: &weft_core::FileEntry,
    abstractor: &Abstractor,
    confirmed: &std::collections::BTreeMap<weft_core::AnswerId, bool>,
    excepted: &crate::abstraction::Excepted,
) -> Op {
    match entry.content.text() {
        Some(text) => Op::CreateFile {
            path: abstractor.path(path.as_str(), confirmed),
            content: abstractor.content_at(path, text, confirmed, excepted),
            mode: entry.mode,
        },
        None => {
            use base64::Engine as _;
            Op::CreateBinaryFile {
                path: abstractor.path(path.as_str(), confirmed),
                data: base64::engine::general_purpose::STANDARD.encode(entry.content.as_bytes()),
                mode: entry.mode,
            }
        }
    }
}

/// Context-anchored hunks from concrete old/new text. Change runs closer than
/// `2 * CONTEXT_RADIUS` are grouped into a single hunk (intervening equal
/// lines land in both `removed` and `added`) so hunks apply sequentially
/// without stepping on each other's context.
pub fn hunks_between<F>(old: &str, new: &str, mut abstract_line: F) -> Vec<Hunk>
where
    // The second argument is `Some(1-based line number in the new file)` for
    // *inserted* lines (authored content — per-occurrence exceptions apply)
    // and `None` for context/removed lines (which mirror the base render).
    F: FnMut(&str, Option<usize>) -> weft_core::Line,
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
                        hunk.context_before =
                            lines.iter().map(|l| abstract_line(l, None)).collect();
                    } else if i == last {
                        hunk.context_after = lines.iter().map(|l| abstract_line(l, None)).collect();
                    } else {
                        // Equal run inside the group: goes to both sides.
                        hunk.removed
                            .extend(lines.iter().map(|l| abstract_line(l, None)));
                        hunk.added
                            .extend(lines.iter().map(|l| abstract_line(l, None)));
                    }
                }
                DiffOp::Delete {
                    old_index, old_len, ..
                } => {
                    hunk.removed.extend(
                        old_lines[old_index..old_index + old_len]
                            .iter()
                            .map(|l| abstract_line(l, None)),
                    );
                }
                DiffOp::Insert {
                    new_index, new_len, ..
                } => {
                    hunk.added.extend(
                        new_lines[new_index..new_index + new_len]
                            .iter()
                            .enumerate()
                            .map(|(k, l)| abstract_line(l, Some(new_index + k + 1))),
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
                            .map(|l| abstract_line(l, None)),
                    );
                    hunk.added.extend(
                        new_lines[new_index..new_index + new_len]
                            .iter()
                            .enumerate()
                            .map(|(k, l)| abstract_line(l, Some(new_index + k + 1))),
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
    fn moved_file_records_as_rename() {
        let mut base = Tree::new();
        base.insert(
            "old/name.txt".into(),
            weft_core::FileEntry::text("same content\n"),
        );
        let mut work = Tree::new();
        work.insert(
            "new/name.txt".into(),
            weft_core::FileEntry::text("same content\n"),
        );
        let abstractor = Abstractor::from_answers(&weft_core::AnswerSet::new());
        let ops = build_ops_with(&base, &work, &abstractor, &Default::default());
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], Op::RenamePath { .. }), "got {ops:?}");
    }

    #[test]
    fn changed_content_is_not_a_rename() {
        let mut base = Tree::new();
        base.insert("a.txt".into(), weft_core::FileEntry::text("one\n"));
        let mut work = Tree::new();
        work.insert("b.txt".into(), weft_core::FileEntry::text("two\n"));
        let abstractor = Abstractor::from_answers(&weft_core::AnswerSet::new());
        let ops = build_ops_with(&base, &work, &abstractor, &Default::default());
        assert_eq!(ops.len(), 2, "delete + create, not rename: {ops:?}");
    }

    #[test]
    fn single_change_gets_surrounding_context() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let hunks = hunks_between(old, new, |l, _| Line::literal(l));
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
        let hunks = hunks_between(old, new, |l, _| Line::literal(l));
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
        let hunks = hunks_between(old, new, |l, _| Line::literal(l));
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, vec![Line::literal("c")]);
        assert!(hunks[0].context_after.is_empty());
        assert_eq!(hunks[0].removed, Vec::<Line>::new());
    }
}
