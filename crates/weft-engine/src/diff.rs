//! Turn (base tree, edited worktree) into a list of abstracted `Op`s with
//! context-anchored hunks, and `fill_slot` ops for lines added inside a
//! slot of the base.

use std::collections::BTreeMap;
use std::ops::Range;

use anyhow::{bail, Result};
use camino::{Utf8Path, Utf8PathBuf};
use similar::{DiffOp, DiffTag, TextDiff};
use weft_core::{
    AnswerId, AnswerSet, Hunk, Line, LineSource, Op, Patch, PatchId, Segment, SlotSpan, Trace, Tree,
};

use crate::abstraction::{Abstractor, Excepted};
use crate::interact::Interaction;

const CONTEXT_RADIUS: usize = 2;

/// What a diff records against — the base tree and, from the same render,
/// where its lines came from and where its slots sit — and how the patch
/// it records fills slots.
pub struct Recording<'a> {
    pub base: &'a Tree,
    pub trace: &'a Trace,
    pub keys: &'a SlotKeys,
    /// A patch's name for messages, from its node id.
    pub names: &'a dyn Fn(PatchId) -> String,
}

/// The keys a recorded patch fills slots under: its name (a foreach patch
/// adds the instance key, since it fills once per instance), except where
/// the version being re-recorded — an amend, a resync — already filled the
/// slot under another key, which is kept so the slot's order stays put.
#[derive(Clone, Debug)]
pub struct SlotKeys {
    /// The key, and its text in the session.
    default: (Line, String),
    /// By (rendered path, slot).
    kept: BTreeMap<(Utf8PathBuf, String), (Line, String)>,
}

impl SlotKeys {
    /// Keyed by the patch's name.
    pub fn named(name: &str) -> Self {
        SlotKeys {
            default: (Line::literal(name), name.to_owned()),
            kept: BTreeMap::new(),
        }
    }

    /// Keyed `<name>/<instance key>`, for a foreach patch recorded against
    /// the sample instance `sample`.
    pub fn per_instance(name: &str, sample: &str) -> Self {
        let key = Line(vec![
            Segment::Literal(format!("{name}/")),
            Segment::Answer(AnswerId::from("key")),
        ]);
        SlotKeys {
            default: (key, format!("{name}/{sample}")),
            kept: BTreeMap::new(),
        }
    }

    /// Keep the keys `previous` — the version being re-recorded — filled
    /// slots under, as they render with `answers`.
    pub fn keeping(mut self, previous: &Patch, answers: &AnswerSet) -> Self {
        let eval = weft_lang::StarlarkEval;
        for op in &previous.ops {
            let Op::FillSlot {
                path, slot, key, ..
            } = op
            else {
                continue;
            };
            let rendered = weft_core::render::render_path(path, answers, &eval)
                .ok()
                .zip(weft_core::render::render_line(key, answers, &eval).ok());
            if let Some((path, text)) = rendered {
                self.kept.insert((path, slot.clone()), (key.clone(), text));
            }
        }
        self
    }

    fn for_slot(&self, path: &Utf8Path, slot: &str) -> &(Line, String) {
        self.kept
            .get(&(path.to_owned(), slot.to_owned()))
            .unwrap_or(&self.default)
    }
}

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
/// Lines rendered from `expr` segments and slot content are never taken as
/// hunk context, and lines added inside a slot record as a `fill_slot`.
pub fn build_ops(
    rec: &Recording<'_>,
    work: &Tree,
    abstractor: &Abstractor,
    interaction: &mut dyn Interaction,
) -> Result<Vec<Op>> {
    let texts = collect_texts(rec.base, work);
    let occurrences = added_occurrences(rec.base, work, abstractor);
    let (confirmed, excepted) =
        abstractor.confirm_with_occurrences(&texts, &occurrences, interaction)?;
    build_ops_decided(rec, work, abstractor, &confirmed, &excepted)
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
/// (e.g. a web UI) instead of an interactive prompt loop, plus
/// per-occurrence exceptions for authored content (added lines + created
/// files).
pub fn build_ops_decided(
    rec: &Recording<'_>,
    work: &Tree,
    abstractor: &Abstractor,
    confirmed: &BTreeMap<AnswerId, bool>,
    excepted: &Excepted,
) -> Result<Vec<Op>> {
    let base = rec.base;
    let mut ops = Vec::new();

    // Rename detection: a deleted file whose exact content reappears at a
    // created path records as `rename_path` (plus `set_mode` if the mode
    // moved) instead of delete + create. First match wins, by path order.
    let mut renamed_from: std::collections::BTreeSet<&Utf8PathBuf> = Default::default();
    let mut renamed_to: std::collections::BTreeSet<&Utf8PathBuf> = Default::default();
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
                    (Some(base_text), Some(text)) => ops.extend(record_text_change(
                        path, base_text, text, rec, abstractor, confirmed, excepted,
                    )?),
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
    // Then creations. A file the base left out because its slots were empty
    // (`omit_when_empty`) is no creation: the worktree added to its slots.
    for (path, entry) in work.iter() {
        if base.get(path).is_none() && !renamed_to.contains(path) {
            match (rec.trace.omitted(path), entry.content.text()) {
                (Some(omitted), Some(text)) => ops.extend(record_text_change(
                    path, omitted, text, rec, abstractor, confirmed, excepted,
                )?),
                _ => ops.push(create_op(path, entry, abstractor, confirmed, excepted)),
            }
        }
    }
    Ok(ops)
}

/// Ops for a text file the worktree changed: a `modify_file` for the lines
/// outside its slots and a `fill_slot` for each slot the worktree added
/// lines to. The hunks are taken with every slot collapsed to one line that
/// never anchors, so no context comes from slot content, whose lines depend
/// on which patches are active.
fn record_text_change(
    path: &Utf8Path,
    old_text: &str,
    new_text: &str,
    rec: &Recording<'_>,
    abstractor: &Abstractor,
    confirmed: &BTreeMap<AnswerId, bool>,
    excepted: &Excepted,
) -> Result<Vec<Op>> {
    let old: Vec<&str> = old_text.lines().collect();
    let new: Vec<&str> = new_text.lines().collect();
    let origins = rec.trace.lines(path).filter(|o| o.len() == old.len());
    let plans = plan_slots(path, &old, &new, rec)?;

    // Both texts with each slot collapsed to its marker; `None` in a map
    // stands for a marker.
    let markers: Vec<String> = plans
        .iter()
        .map(|p| format!("{SLOT_MARKER}{}", p.span.name))
        .collect();
    fn collapse<'a>(
        lines: &[&'a str],
        ranges: impl Iterator<Item = Range<usize>>,
        markers: &'a [String],
    ) -> (Vec<&'a str>, Vec<Option<usize>>) {
        let mut out: Vec<&str> = Vec::with_capacity(lines.len());
        let mut map: Vec<Option<usize>> = Vec::with_capacity(lines.len());
        let mut at = 0;
        for (range, marker) in ranges.zip(markers) {
            for (i, line) in lines.iter().enumerate().take(range.start).skip(at) {
                out.push(line);
                map.push(Some(i));
            }
            out.push(marker.as_str());
            map.push(None);
            at = range.end;
        }
        for (i, line) in lines.iter().enumerate().skip(at) {
            out.push(line);
            map.push(Some(i));
        }
        (out, map)
    }
    let (old_c, old_map) = collapse(
        &old,
        plans.iter().map(|p| p.span.start..p.span.end()),
        &markers,
    );
    let (new_c, new_map) = collapse(&new, plans.iter().map(|p| p.region.clone()), &markers);
    let anchors = |k: usize| old_map[k].is_some_and(|i| origins.is_none_or(|o| o[i].anchors()));
    let hunks = hunks_between(&old_c, &new_c, &anchors, |line, new_line| {
        match new_line.and_then(|n| new_map[n - 1]) {
            // Added lines are authored: exceptions apply.
            Some(i) => abstractor.line_at(path, i + 1, line, confirmed, excepted),
            // Context/removed lines mirror the base render.
            None => abstractor.line(line, confirmed),
        }
    })
    .or_else(|u| {
        let around: Vec<String> = u
            .neighbours
            .iter()
            .filter_map(|&k| match old_map[k] {
                None => Some(format!(
                    "slot `{}`",
                    old_c[k].trim_start_matches(SLOT_MARKER)
                )),
                Some(i) => match &origins?[i].source {
                    LineSource::Expr(e) => Some(format!("`expr` segment `{}`", e.as_str())),
                    _ => None,
                },
            })
            .collect();
        let line = new_map
            .get(u.line - 1)
            .copied()
            .flatten()
            .map_or(u.line, |i| i + 1);
        bail!(
            "`{path}`: the change at line {line} sits between lines that read differently \
             under other answers or with other patches active ({}), so no hunk context can \
             anchor it; make the change next to a literal line instead",
            around.join(", ")
        )
    })?;
    let touches_slot = |lines: &[Line]| {
        lines
            .iter()
            .any(|l| l.as_literal().is_some_and(|t| t.starts_with(SLOT_MARKER)))
    };
    if let Some(hunk) = hunks
        .iter()
        .find(|h| touches_slot(&h.removed) || touches_slot(&h.added))
    {
        let slot = hunk
            .removed
            .iter()
            .chain(&hunk.added)
            .find_map(|l| l.as_literal().filter(|t| t.starts_with(SLOT_MARKER)))
            .unwrap_or_default();
        bail!(
            "`{path}`: the edits around slot `{}` move lines across it, so they cannot be told \
             apart from its content; keep lines on their side of the slot",
            slot.trim_start_matches(SLOT_MARKER)
        );
    }

    let mut ops = Vec::new();
    if !hunks.is_empty() {
        ops.push(Op::ModifyFile {
            path: abstractor.path(path.as_str(), confirmed),
            hunks,
        });
    }
    for plan in plans {
        let Some(fill) = plan.fill else {
            continue;
        };
        ops.push(Op::FillSlot {
            path: abstractor.path(path.as_str(), confirmed),
            slot: plan.span.name.clone(),
            key: fill.key,
            lines: fill
                .lines
                .iter()
                .enumerate()
                .map(|(k, text)| {
                    abstractor.line_at(path, fill.first + k + 1, text, confirmed, excepted)
                })
                .collect(),
        });
    }
    Ok(ops)
}

/// What a slot stands for while a diff is collapsed. It never matches a
/// file line: no text holds a NUL.
const SLOT_MARKER: &str = "\u{0}slot\u{0}";

/// One slot of the base, and what the worktree did inside it.
struct SlotPlan<'t> {
    span: &'t SlotSpan,
    /// Where the slot's content is in the new text.
    region: Range<usize>,
    /// The contribution the worktree added to it, if any.
    fill: Option<NewFill>,
}

/// Lines the worktree added to a slot, as the recorded patch's contribution.
struct NewFill {
    key: Line,
    key_text: String,
    /// 0-based line of the new text where the contribution starts.
    first: usize,
    /// Its lines, the separator the slot appends taken off the last.
    lines: Vec<String>,
}

/// Where each slot of `path` in the base is in the new text, and the new
/// contribution, if any, the worktree added to it. A slot owns every line
/// added between the lines around it; its existing contributions belong to
/// other patches and may only move, and the new lines must be one block,
/// placed where the key sorts, the separator ending all but the last.
fn plan_slots<'t>(
    path: &Utf8Path,
    old: &[&str],
    new: &[&str],
    rec: &'t Recording<'_>,
) -> Result<Vec<SlotPlan<'t>>> {
    let spans = rec.trace.slots(path);
    if spans.is_empty() || spans.last().is_some_and(|s| s.end() > old.len()) {
        return Ok(Vec::new());
    }
    let ops = TextDiff::from_slices(old, new).ops().to_vec();
    let mut plans: Vec<SlotPlan<'t>> = Vec::with_capacity(spans.len());
    for span in spans {
        let region = slot_region(path, &ops, span, new.len())?;
        let fill = new_fill(
            path,
            span,
            &old[span.start..span.end()],
            new,
            region.clone(),
            rec,
        )?;
        if let Some(prev) = plans.last() {
            if prev.region.end > region.start {
                bail!(
                    "`{path}`: slots `{}` and `{}` are next to each other, so lines added \
                     between them cannot be told apart; the patch declaring them should put a \
                     line between the two",
                    prev.span.name,
                    span.name
                );
            }
        }
        plans.push(SlotPlan { span, region, fill });
    }
    Ok(plans)
}

/// The lines of the new text a slot of the old text covers: its unchanged
/// and changed content plus every line inserted from just after the line
/// before it to just before the line after it.
fn slot_region(
    path: &Utf8Path,
    ops: &[DiffOp],
    span: &SlotSpan,
    new_len: usize,
) -> Result<Range<usize>> {
    let (s, e) = (span.start, span.end());
    let mut region: Option<Range<usize>> = None;
    let mut claim = |r: Range<usize>| {
        region = Some(match region.take() {
            None => r,
            Some(had) => had.start.min(r.start)..had.end.max(r.end),
        });
    };
    for op in ops {
        let (a, b) = (op.old_range().start, op.old_range().end);
        let c = op.new_range().start;
        match op.tag() {
            DiffTag::Equal => {
                let (lo, hi) = (a.max(s), b.min(e));
                if lo < hi {
                    claim(c + (lo - a)..c + (hi - a));
                }
            }
            DiffTag::Insert => {
                if (s..=e).contains(&a) {
                    claim(op.new_range());
                }
            }
            DiffTag::Delete | DiffTag::Replace => {
                if s <= a && b <= e {
                    claim(op.new_range());
                } else if (a < e && s < b) || (s == e && a < s && s < b) {
                    bail!(
                        "`{path}`: an edit at line {} runs across the edge of slot `{}`; change \
                         the lines around the slot apart from the lines you add to it",
                        c + 1,
                        span.name
                    );
                }
            }
        }
    }
    Ok(region.unwrap_or_else(|| {
        // Untouched and empty: where the slot's position landed.
        let at = ops
            .iter()
            .find(|op| {
                s < op.old_range().end || (op.old_range().is_empty() && op.old_range().start == s)
            })
            .map_or(new_len, |op| match op.tag() {
                DiffTag::Equal => op.new_range().start + (s - op.old_range().start),
                _ => op.new_range().start,
            });
        at..at
    }))
}

/// Split a slot's lines in the new text into its existing contributions and
/// the one block the worktree added, which becomes the recorded patch's fill.
fn new_fill(
    path: &Utf8Path,
    span: &SlotSpan,
    old: &[&str],
    new: &[&str],
    region: Range<usize>,
    rec: &Recording<'_>,
) -> Result<Option<NewFill>> {
    let sep = span.separator.as_str();
    let unseparated = |line: &str| -> Option<String> {
        (!sep.is_empty())
            .then(|| line.strip_suffix(sep).map(str::to_owned))
            .flatten()
    };
    // Each contribution's lines as its patch wrote them: the separator the
    // slot appended taken off.
    let mut fills: Vec<(String, PatchId, Vec<String>)> = Vec::with_capacity(span.fills.len());
    let mut at = 0;
    for (k, fill) in span.fills.iter().enumerate() {
        let mut lines: Vec<String> = old[at..at + fill.lines]
            .iter()
            .map(|l| (*l).to_owned())
            .collect();
        at += fill.lines;
        if k + 1 < span.fills.len() {
            if let Some(last) = lines.last_mut() {
                *last = unseparated(last).unwrap_or_else(|| last.clone());
            }
        }
        fills.push((fill.key.clone(), fill.node, lines));
    }
    let rendered = |fills: &[(String, PatchId, Vec<String>)]| -> Vec<String> {
        let mut out = Vec::new();
        for (k, (_, _, lines)) in fills.iter().enumerate() {
            out.extend(lines.iter().cloned());
            if k + 1 < fills.len() {
                if let Some(last) = out.last_mut() {
                    last.push_str(sep);
                }
            }
        }
        out
    };
    let inside = &new[region.clone()];
    if rendered(&fills) == inside {
        return Ok(None);
    }

    let (key, key_text) = rec.keys.for_slot(path, &span.name);
    if let Some((_, owner, _)) = fills.iter().find(|(k, _, _)| k == key_text) {
        bail!(
            "`{path}`: slot `{}` already has a contribution under key `{key_text}`, from patch \
             `{}`",
            span.name,
            (rec.names)(*owner)
        );
    }
    // The existing contributions keep their order: find the one gap where
    // new lines sit between them.
    let fits = |at: usize, lines: &[String]| -> bool {
        at + lines.len() <= inside.len()
            && lines.iter().enumerate().all(|(j, l)| {
                inside[at + j] == l
                    || (j + 1 == lines.len() && unseparated(inside[at + j]).as_deref() == Some(l))
            })
    };
    let mut gaps: Vec<(usize, Range<usize>)> = Vec::new();
    for g in 0..=fills.len() {
        let mut p = 0;
        let before_fit = fills[..g].iter().all(|(_, _, lines)| {
            let ok = fits(p, lines);
            p += lines.len();
            ok
        });
        let mut q = inside.len();
        let after_fit = fills[g..].iter().rev().all(|(_, _, lines)| {
            let ok = q >= lines.len() && fits(q - lines.len(), lines);
            q = q.saturating_sub(lines.len());
            ok
        });
        if before_fit && after_fit && p < q {
            gaps.push((g, p..q));
        }
    }
    let sorted_at = fills
        .iter()
        .filter(|(k, _, _)| k.as_str() < key_text.as_str())
        .count();
    let Some((_, added)) = gaps
        .iter()
        .find(|(g, _)| *g == sorted_at)
        .or_else(|| gaps.first())
        .cloned()
    else {
        let keys: Vec<String> = fills.iter().map(|(k, _, _)| format!("`{k}`")).collect();
        bail!(
            "`{path}`: the worktree changed slot `{}` in a way no single contribution explains: \
             the contributions it holds ({}) change only by amending the patches that add them, \
             and one patch adds one block of lines to a slot",
            span.name,
            if keys.is_empty() {
                "none".to_owned()
            } else {
                keys.join(", ")
            }
        );
    };
    let mut lines: Vec<String> = inside[added.clone()]
        .iter()
        .map(|l| (*l).to_owned())
        .collect();
    if let Some(last) = lines.last_mut() {
        if let Some(bare) = unseparated(last) {
            *last = bare;
        }
    }
    let mut all = fills.clone();
    all.push((
        key_text.clone(),
        PatchId::from_canonical_bytes(b""),
        lines.clone(),
    ));
    all.sort_by(|a, b| a.0.cmp(&b.0));
    let expected = rendered(&all);
    if expected != inside {
        let show = |lines: &[&str]| -> String {
            lines
                .iter()
                .map(|l| format!("\n    {l}"))
                .collect::<String>()
        };
        let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
        bail!(
            "`{path}`: with this patch's lines under key `{key_text}`, slot `{}` renders as:{}\n\
             but the worktree has:{}\ncontributions are ordered by key{}",
            span.name,
            show(&expected),
            show(inside),
            if sep.is_empty() {
                String::new()
            } else {
                format!(", and every one but the last ends with `{sep}`")
            }
        );
    }
    Ok(Some(NewFill {
        key: key.clone(),
        key_text: key_text.clone(),
        first: region.start + added.start,
        lines,
    }))
}

/// What `weft diff` says about the slots of a text file the worktree
/// changed: the contribution each slot records, or why it cannot record.
pub fn slot_notes(
    path: &Utf8Path,
    old_text: &str,
    new_text: &str,
    rec: &Recording<'_>,
) -> Vec<String> {
    let old: Vec<&str> = old_text.lines().collect();
    let new: Vec<&str> = new_text.lines().collect();
    match plan_slots(path, &old, &new, rec) {
        Ok(plans) => plans
            .iter()
            .filter_map(|plan| {
                let fill = plan.fill.as_ref()?;
                let last = fill.first + fill.lines.len();
                let lines = if fill.lines.len() == 1 {
                    format!("line {last}")
                } else {
                    format!("lines {}-{last}", fill.first + 1)
                };
                Some(format!(
                    "{lines} fill slot `{}` under key `{}`",
                    plan.span.name, fill.key_text
                ))
            })
            .collect(),
        Err(e) => vec![format!("cannot record: {e:#}")],
    }
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
            omit_when_empty: vec![],
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

/// A change [`hunks_between`] cannot anchor: every line next to it reads
/// differently under other answers, so no context can find its place.
#[derive(Debug)]
pub struct Unanchored {
    /// 1-based line in the new text where the change starts.
    pub line: usize,
    /// The old lines just around the change (0-based), which do not anchor.
    pub neighbours: Vec<usize>,
}

/// Context-anchored hunks from concrete old/new lines. Change runs closer
/// than `2 * CONTEXT_RADIUS` are grouped into a single hunk (intervening
/// equal lines land in both `removed` and `added`) so hunks apply
/// sequentially without stepping on each other's context.
///
/// Context is taken only from old lines that `anchors` accepts: a hunk's
/// context stops at the first one it rejects (fewer context lines, down to
/// none), and a group whose equal lines in between include one is split
/// there. A change left with no pattern at all is an append, which is only
/// right at the end of the file; anywhere else it is [`Unanchored`].
pub fn hunks_between<F>(
    old: &[&str],
    new: &[&str],
    anchors: &dyn Fn(usize) -> bool,
    mut abstract_line: F,
) -> Result<Vec<Hunk>, Unanchored>
where
    // The second argument is `Some(1-based line number in the new file)` for
    // *inserted* lines (authored content — per-occurrence exceptions apply)
    // and `None` for context/removed lines (which mirror the base render).
    F: FnMut(&str, Option<usize>) -> weft_core::Line,
{
    let diff = TextDiff::from_slices(old, new);
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(CONTEXT_RADIUS) {
        for part in split_at_unanchored(&group, anchors) {
            hunks.push(hunk_of(&part, old, new, anchors, &mut abstract_line)?);
        }
    }
    Ok(hunks)
}

/// Split a group of diff ops at every interior equal run holding a line
/// that does not anchor; the run ends one part and starts the next.
fn split_at_unanchored(group: &[DiffOp], anchors: &dyn Fn(usize) -> bool) -> Vec<Vec<DiffOp>> {
    let last = group.len().saturating_sub(1);
    let mut parts = vec![Vec::new()];
    for (i, op) in group.iter().enumerate() {
        parts.last_mut().expect("never empty").push(*op);
        let splits = i != 0
            && i != last
            && matches!(op, DiffOp::Equal { .. })
            && op.old_range().any(|k| !anchors(k));
        if splits {
            parts.push(vec![*op]);
        }
    }
    parts
}

/// One hunk from a run of diff ops that holds at least one change: leading
/// and trailing equal ops become context (anchoring lines nearest the change,
/// at most `CONTEXT_RADIUS`), interior ones go to both sides.
fn hunk_of<F>(
    ops: &[DiffOp],
    old: &[&str],
    new: &[&str],
    anchors: &dyn Fn(usize) -> bool,
    abstract_line: &mut F,
) -> Result<Hunk, Unanchored>
where
    F: FnMut(&str, Option<usize>) -> weft_core::Line,
{
    let mut hunk = Hunk::default();
    let last = ops.len().saturating_sub(1);
    for (i, op) in ops.iter().enumerate() {
        match *op {
            DiffOp::Equal { old_index, len, .. } if i == 0 => {
                let mut before: Vec<usize> = (old_index..old_index + len)
                    .rev()
                    .take_while(|&k| anchors(k))
                    .take(CONTEXT_RADIUS)
                    .collect();
                before.reverse();
                hunk.context_before = before
                    .iter()
                    .map(|&k| abstract_line(old[k], None))
                    .collect();
            }
            DiffOp::Equal { old_index, len, .. } if i == last => {
                hunk.context_after = (old_index..old_index + len)
                    .take_while(|&k| anchors(k))
                    .take(CONTEXT_RADIUS)
                    .map(|k| abstract_line(old[k], None))
                    .collect();
            }
            DiffOp::Equal { old_index, len, .. } => {
                // Equal run inside the group: goes to both sides.
                let lines = &old[old_index..old_index + len];
                hunk.removed
                    .extend(lines.iter().map(|l| abstract_line(l, None)));
                hunk.added
                    .extend(lines.iter().map(|l| abstract_line(l, None)));
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                hunk.removed.extend(
                    old[old_index..old_index + old_len]
                        .iter()
                        .map(|l| abstract_line(l, None)),
                );
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                hunk.added.extend(
                    new[new_index..new_index + new_len]
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
                    old[old_index..old_index + old_len]
                        .iter()
                        .map(|l| abstract_line(l, None)),
                );
                hunk.added.extend(
                    new[new_index..new_index + new_len]
                        .iter()
                        .enumerate()
                        .map(|(k, l)| abstract_line(l, Some(new_index + k + 1))),
                );
            }
        }
    }
    let unpatterned =
        hunk.context_before.is_empty() && hunk.removed.is_empty() && hunk.context_after.is_empty();
    if unpatterned {
        let change = ops
            .iter()
            .find(|op| !matches!(op, DiffOp::Equal { .. }))
            .expect("a hunk holds a change");
        let at = change.old_range().start;
        if at != old.len() {
            return Err(Unanchored {
                line: change.new_range().start + 1,
                neighbours: [at.checked_sub(1), Some(at)]
                    .into_iter()
                    .flatten()
                    .collect(),
            });
        }
    }
    Ok(hunk)
}

/// One line of a selectable hunk's unified-diff body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HunkLine {
    Context(String),
    Removed(String),
    Added(String),
}

/// One independently stageable change between two concrete texts (`weft add
/// -p`). Unlike [`Hunk`] this is not abstracted and not context-anchored: it
/// carries the exact line ranges it rewrites, so a subset of hunks can be
/// replayed onto the old text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectableHunk {
    /// Lines of the old text this hunk replaces (0-based, end-exclusive).
    pub old_range: std::ops::Range<usize>,
    /// Lines of the new text it replaces them with.
    pub new_range: std::ops::Range<usize>,
    /// The same spans widened by the displayed context, for the `@@` header.
    pub old_span: std::ops::Range<usize>,
    pub new_span: std::ops::Range<usize>,
    /// Unified-diff body, in order.
    pub lines: Vec<HunkLine>,
}

impl SelectableHunk {
    /// The `@@ -old,len +new,len @@` header (1-based, git-style).
    pub fn header(&self) -> String {
        let (ol, nl) = (self.old_span.len(), self.new_span.len());
        let (os, ns) = (
            if ol == 0 { 0 } else { self.old_span.start + 1 },
            if nl == 0 { 0 } else { self.new_span.start + 1 },
        );
        format!("@@ -{os},{ol} +{ns},{nl} @@")
    }

    /// Can [`split`] break this hunk up? True when it has interior context.
    pub fn splittable(&self) -> bool {
        change_runs(&self.lines).len() > 1
    }
}

/// Split `old` → `new` into independently stageable hunks, grouped the same
/// way [`hunks_between`] groups them (change runs closer than
/// `2 * CONTEXT_RADIUS` share a hunk). An empty `old` with a non-empty `new`
/// yields one all-added hunk.
pub fn selectable_hunks(old: &str, new: &str) -> Vec<SelectableHunk> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let diff = TextDiff::from_slices(&old_lines, &new_lines);
    diff.grouped_ops(CONTEXT_RADIUS)
        .iter()
        .filter_map(|group| selectable_from_ops(group, &old_lines, &new_lines))
        .collect()
}

fn selectable_from_ops(
    ops: &[DiffOp],
    old_lines: &[&str],
    new_lines: &[&str],
) -> Option<SelectableHunk> {
    use similar::DiffTag;
    let first = ops.iter().position(|o| o.tag() != DiffTag::Equal)?;
    let last = ops.iter().rposition(|o| o.tag() != DiffTag::Equal)?;

    let mut lines = Vec::new();
    for op in ops {
        match op.tag() {
            DiffTag::Equal => lines.extend(
                old_lines[op.old_range()]
                    .iter()
                    .map(|l| HunkLine::Context((*l).to_owned())),
            ),
            DiffTag::Delete => lines.extend(
                old_lines[op.old_range()]
                    .iter()
                    .map(|l| HunkLine::Removed((*l).to_owned())),
            ),
            DiffTag::Insert => lines.extend(
                new_lines[op.new_range()]
                    .iter()
                    .map(|l| HunkLine::Added((*l).to_owned())),
            ),
            DiffTag::Replace => {
                lines.extend(
                    old_lines[op.old_range()]
                        .iter()
                        .map(|l| HunkLine::Removed((*l).to_owned())),
                );
                lines.extend(
                    new_lines[op.new_range()]
                        .iter()
                        .map(|l| HunkLine::Added((*l).to_owned())),
                );
            }
        }
    }

    Some(SelectableHunk {
        old_range: ops[first].old_range().start..ops[last].old_range().end,
        new_range: ops[first].new_range().start..ops[last].new_range().end,
        old_span: ops[0].old_range().start..ops[ops.len() - 1].old_range().end,
        new_span: ops[0].new_range().start..ops[ops.len() - 1].new_range().end,
        lines,
    })
}

/// Index ranges (into `lines`) of the maximal runs of non-context lines.
fn change_runs(lines: &[HunkLine]) -> Vec<std::ops::Range<usize>> {
    let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if matches!(line, HunkLine::Context(_)) {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.end == i => run.end = i + 1,
            _ => runs.push(i..i + 1),
        }
    }
    runs
}

/// Break a grouped hunk at its interior context into one hunk per change run
/// (`s` in the picker). Returns `None` when the hunk is a single contiguous
/// change and cannot be split further.
///
/// Sub-hunks may *display* the same context lines, but their `old_range`s are
/// disjoint, so [`apply_selection`] stays well-defined.
pub fn split(hunk: &SelectableHunk) -> Option<Vec<SelectableHunk>> {
    let runs = change_runs(&hunk.lines);
    if runs.len() < 2 {
        return None;
    }

    // Per-display-line positions in the old and new texts.
    let (mut old_at, mut new_at) = (Vec::new(), Vec::new());
    let (mut o, mut n) = (hunk.old_span.start, hunk.new_span.start);
    for line in &hunk.lines {
        old_at.push(o);
        new_at.push(n);
        match line {
            HunkLine::Context(_) => {
                o += 1;
                n += 1;
            }
            HunkLine::Removed(_) => o += 1,
            HunkLine::Added(_) => n += 1,
        }
    }
    old_at.push(o);
    new_at.push(n);

    let parts = runs
        .iter()
        .map(|run| {
            let ctx_start = run.start.saturating_sub(CONTEXT_RADIUS);
            let ctx_end = (run.end + CONTEXT_RADIUS).min(hunk.lines.len());
            SelectableHunk {
                old_range: old_at[run.start]..old_at[run.end],
                new_range: new_at[run.start]..new_at[run.end],
                old_span: old_at[ctx_start]..old_at[ctx_end],
                new_span: new_at[ctx_start]..new_at[ctx_end],
                lines: hunk.lines[ctx_start..ctx_end].to_vec(),
            }
        })
        .collect();
    Some(parts)
}

/// Rebuild the file from `old`, taking the `new` side for the hunks marked in
/// `selected` and the `old` side for the rest. `hunks` must be in order with
/// disjoint `old_range`s (as produced by [`selectable_hunks`] / [`split`]).
///
/// Selecting every hunk reproduces `new`; selecting none reproduces `old`.
pub fn apply_selection(
    old: &str,
    new: &str,
    hunks: &[SelectableHunk],
    selected: &[bool],
) -> String {
    if !hunks.is_empty() {
        if selected.iter().all(|s| *s) {
            return new.to_owned();
        }
        if !selected.iter().any(|s| *s) {
            return old.to_owned();
        }
    }

    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    // Whether the last emitted line came from `new` — decides whether the
    // result keeps `new`'s or `old`'s trailing-newline habit.
    let mut tail_from_new = false;
    let mut cursor = 0usize;

    for (hunk, take_new) in hunks
        .iter()
        .zip(selected.iter().chain(std::iter::repeat(&false)))
    {
        out.extend(&old_lines[cursor..hunk.old_range.start]);
        if hunk.old_range.start > cursor {
            tail_from_new = false;
        }
        if *take_new {
            out.extend(&new_lines[hunk.new_range.clone()]);
            if !hunk.new_range.is_empty() {
                tail_from_new = true;
            }
        } else {
            out.extend(&old_lines[hunk.old_range.clone()]);
            if !hunk.old_range.is_empty() {
                tail_from_new = false;
            }
        }
        cursor = hunk.old_range.end;
    }
    out.extend(&old_lines[cursor..]);
    if cursor < old_lines.len() {
        tail_from_new = false;
    }

    let mut text = out.join("\n");
    let source = if tail_from_new { new } else { old };
    if !text.is_empty() && source.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops_between(base: &Tree, work: &Tree) -> Vec<Op> {
        let abstractor = Abstractor::from_answers(&weft_core::AnswerSet::new());
        let rec = Recording {
            base,
            trace: &Trace::default(),
            keys: &SlotKeys::named("test"),
            names: &|id| id.short(),
        };
        build_ops_decided(
            &rec,
            work,
            &abstractor,
            &Default::default(),
            &Default::default(),
        )
        .unwrap()
    }

    /// Literal hunks from `old` to `new`, where the old lines at
    /// `unanchored` (0-based) do not anchor.
    fn hunks(old: &str, new: &str, unanchored: &[usize]) -> Result<Vec<Hunk>, Unanchored> {
        let old: Vec<&str> = old.lines().collect();
        let new: Vec<&str> = new.lines().collect();
        hunks_between(&old, &new, &|k| !unanchored.contains(&k), |l, _| {
            Line::literal(l)
        })
    }

    fn lines(items: &[&str]) -> Vec<Line> {
        items.iter().map(|l| Line::literal(l)).collect()
    }

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
        let ops = ops_between(&base, &work);
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], Op::RenamePath { .. }), "got {ops:?}");
    }

    #[test]
    fn changed_content_is_not_a_rename() {
        let mut base = Tree::new();
        base.insert("a.txt".into(), weft_core::FileEntry::text("one\n"));
        let mut work = Tree::new();
        work.insert("b.txt".into(), weft_core::FileEntry::text("two\n"));
        let ops = ops_between(&base, &work);
        assert_eq!(ops.len(), 2, "delete + create, not rename: {ops:?}");
    }

    #[test]
    fn single_change_gets_surrounding_context() {
        let hunks = hunks("a\nb\nc\nd\ne\n", "a\nb\nC\nd\ne\n", &[]).unwrap();
        assert_eq!(hunks.len(), 1);
        let h = &hunks[0];
        assert_eq!(h.context_before, lines(&["a", "b"]));
        assert_eq!(h.removed, lines(&["c"]));
        assert_eq!(h.added, lines(&["C"]));
        assert_eq!(h.context_after, lines(&["d", "e"]));
    }

    #[test]
    fn nearby_changes_merge_into_one_hunk() {
        let hunks = hunks("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n", &[]).unwrap();
        assert_eq!(hunks.len(), 1, "changes 2 lines apart share one hunk");
        let h = &hunks[0];
        // middle equal line `c` appears on both sides
        assert!(h.removed.contains(&Line::literal("c")));
        assert!(h.added.contains(&Line::literal("c")));
    }

    #[test]
    fn append_at_end_of_file() {
        let hunks = hunks("a\nb\n", "a\nb\nc\n", &[]).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, lines(&["c"]));
        assert!(hunks[0].context_after.is_empty());
        assert_eq!(hunks[0].removed, Vec::<Line>::new());
    }

    #[test]
    fn context_stops_at_a_line_that_does_not_anchor() {
        // `[servers]` then two expression lines; insert right after it.
        let old = "# head\n\n[servers]\nlinear\n\n";
        let new = "# head\n\n[servers]\nlightdash\nlinear\n\n";
        let h = &hunks(old, new, &[3, 4]).unwrap()[0];
        assert_eq!(h.context_before, lines(&["", "[servers]"]));
        assert_eq!(h.added, lines(&["lightdash"]));
        assert!(h.context_after.is_empty(), "{h:?}");

        // Only the anchoring lines nearest the change count.
        let h = &hunks(old, new, &[1]).unwrap()[0];
        assert_eq!(h.context_before, lines(&["[servers]"]));
        assert_eq!(h.context_after, lines(&["linear", ""]));
    }

    #[test]
    fn a_group_splits_at_an_interior_line_that_does_not_anchor() {
        let hunks = hunks("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n", &[2]).unwrap();
        assert_eq!(hunks.len(), 2, "{hunks:?}");
        assert_eq!(hunks[0].context_before, lines(&["a"]));
        assert_eq!(hunks[0].removed, lines(&["b"]));
        assert!(hunks[0].context_after.is_empty());
        assert!(hunks[1].context_before.is_empty());
        assert_eq!(hunks[1].removed, lines(&["d"]));
        assert_eq!(hunks[1].context_after, lines(&["e"]));
    }

    #[test]
    fn a_change_with_nothing_to_anchor_on_is_refused_unless_it_appends() {
        let err = hunks("x\ny\n", "x\nnew\ny\n", &[0, 1]).unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(err.neighbours, [0, 1]);
        // At the end of the file an empty pattern appends, which is right.
        let h = &hunks("x\ny\n", "x\ny\nnew\n", &[0, 1]).unwrap()[0];
        assert!(h.context_before.is_empty() && h.context_after.is_empty());
        assert_eq!(h.added, lines(&["new"]));
    }

    /// `.mcp.json` with a `servers` slot that `linear` fills, as a traced
    /// render gives it.
    fn slotted_base() -> (Tree, Trace) {
        use weft_core::{Content, Draft, SlotDecl, TemplatePath};
        let owner = Patch::new(
            vec![],
            None,
            vec![Op::CreateFile {
                path: TemplatePath::literal(".mcp.json"),
                omit_when_empty: vec![],
                content: Content(vec![
                    Line::literal("{"),
                    Line::literal("  \"mcpServers\": {"),
                    Line::slot(SlotDecl {
                        slot: "servers".into(),
                        separator: ",".into(),
                    }),
                    Line::literal("  }"),
                    Line::literal("}"),
                ]),
                mode: weft_core::DEFAULT_FILE_MODE,
            }],
        );
        let linear = Patch::new(
            vec![owner.id],
            None,
            vec![Op::FillSlot {
                path: TemplatePath::literal(".mcp.json"),
                slot: "servers".into(),
                key: Line::literal("linear"),
                lines: vec![Line::literal("    \"linear\": {}")],
            }],
        );
        let mut draft = Draft::traced();
        let (answers, eval, root) = (AnswerSet::new(), weft_lang::StarlarkEval, Utf8Path::new(""));
        draft
            .apply(owner.id, &owner, &answers, &eval, root)
            .unwrap();
        draft
            .apply(linear.id, &linear, &answers, &eval, root)
            .unwrap();
        draft.finish_traced().unwrap()
    }

    #[test]
    fn lines_added_in_a_slot_fill_it_and_the_rest_are_hunks_around_it() {
        let (base, trace) = slotted_base();
        let mut work = Tree::new();
        work.insert(
            ".mcp.json".into(),
            weft_core::FileEntry::text(
                "{\n  \"$schema\": \"x\",\n  \"mcpServers\": {\n    \"lightdash\": {},\n    \
                 \"linear\": {}\n  }\n}\n",
            ),
        );
        let abstractor = Abstractor::from_answers(&AnswerSet::new());
        let rec = Recording {
            base: &base,
            trace: &trace,
            keys: &SlotKeys::named("lightdash"),
            names: &|id| id.short(),
        };
        let ops = build_ops_decided(
            &rec,
            &work,
            &abstractor,
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        let [Op::ModifyFile { hunks, .. }, Op::FillSlot {
            slot,
            key,
            lines: fill,
            ..
        }] = &ops[..]
        else {
            panic!("expected a hunk and a fill: {ops:?}");
        };
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].context_before, lines(&["{"]));
        assert_eq!(hunks[0].added, lines(&["  \"$schema\": \"x\","]));
        // Context stops where the slot starts.
        assert_eq!(hunks[0].context_after, lines(&["  \"mcpServers\": {"]));
        assert_eq!(
            (slot.as_str(), key),
            ("servers", &Line::literal("lightdash"))
        );
        assert_eq!(fill, &lines(&["    \"lightdash\": {}"]));
    }

    // ---- selectable hunks (`weft add -p`) -------------------------------

    /// Two edits far enough apart to be separate hunks.
    const FAR_OLD: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    const FAR_NEW: &str = "1\nTWO\n3\n4\n5\n6\n7\n8\nNINE\n10\n";

    #[test]
    fn distant_changes_are_separate_selectable_hunks() {
        let hunks = selectable_hunks(FAR_OLD, FAR_NEW);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].old_range, 1..2);
        assert_eq!(hunks[1].old_range, 8..9);
        assert!(!hunks[0].splittable());
    }

    #[test]
    fn selecting_all_or_none_reproduces_the_sides() {
        let hunks = selectable_hunks(FAR_OLD, FAR_NEW);
        assert_eq!(
            apply_selection(FAR_OLD, FAR_NEW, &hunks, &[true, true]),
            FAR_NEW
        );
        assert_eq!(
            apply_selection(FAR_OLD, FAR_NEW, &hunks, &[false, false]),
            FAR_OLD
        );
    }

    #[test]
    fn selecting_one_hunk_takes_only_that_edit() {
        let hunks = selectable_hunks(FAR_OLD, FAR_NEW);
        assert_eq!(
            apply_selection(FAR_OLD, FAR_NEW, &hunks, &[true, false]),
            "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n"
        );
        assert_eq!(
            apply_selection(FAR_OLD, FAR_NEW, &hunks, &[false, true]),
            "1\n2\n3\n4\n5\n6\n7\n8\nNINE\n10\n"
        );
    }

    #[test]
    fn insertions_and_deletions_apply_selectively() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\n";
        let new = "a\nb\nc\nNEW\nd\ne\nf\ng\nh\ni\nk\n"; // insert after c, delete j
        let hunks = selectable_hunks(old, new);
        assert_eq!(hunks.len(), 2);
        assert_eq!(
            apply_selection(old, new, &hunks, &[true, false]),
            "a\nb\nc\nNEW\nd\ne\nf\ng\nh\ni\nj\nk\n"
        );
        assert_eq!(
            apply_selection(old, new, &hunks, &[false, true]),
            "a\nb\nc\nd\ne\nf\ng\nh\ni\nk\n"
        );
    }

    #[test]
    fn created_file_is_one_all_added_hunk() {
        let hunks = selectable_hunks("", "x\ny\n");
        assert_eq!(hunks.len(), 1);
        assert!(hunks[0]
            .lines
            .iter()
            .all(|l| matches!(l, HunkLine::Added(_))));
        assert_eq!(apply_selection("", "x\ny\n", &hunks, &[true]), "x\ny\n");
        assert_eq!(apply_selection("", "x\ny\n", &hunks, &[false]), "");
    }

    #[test]
    fn split_breaks_a_merged_hunk_at_its_interior_context() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nB\nc\nD\ne\n"; // 2 lines apart: one grouped hunk
        let hunks = selectable_hunks(old, new);
        assert_eq!(hunks.len(), 1);
        assert!(hunks[0].splittable());

        let parts = split(&hunks[0]).expect("splittable");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].old_range, 1..2);
        assert_eq!(parts[1].old_range, 3..4);
        assert_eq!(
            apply_selection(old, new, &parts, &[true, false]),
            "a\nB\nc\nd\ne\n"
        );
        assert_eq!(
            apply_selection(old, new, &parts, &[false, true]),
            "a\nb\nc\nD\ne\n"
        );
        assert_eq!(apply_selection(old, new, &parts, &[true, true]), new);
    }

    #[test]
    fn a_contiguous_change_cannot_be_split() {
        let hunks = selectable_hunks("a\nb\nc\n", "a\nX\nY\nc\n");
        assert_eq!(hunks.len(), 1);
        assert!(!hunks[0].splittable());
        assert!(split(&hunks[0]).is_none());
    }

    #[test]
    fn hunk_header_is_one_based() {
        let hunks = selectable_hunks(FAR_OLD, FAR_NEW);
        assert_eq!(hunks[0].header(), "@@ -1,4 +1,4 @@");
    }

    #[test]
    fn missing_trailing_newline_is_preserved() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10";
        let new = "1\nTWO\n3\n4\n5\n6\n7\n8\nNINE\n10";
        let hunks = selectable_hunks(old, new);
        let partial = apply_selection(old, new, &hunks, &[true, false]);
        assert_eq!(partial, "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10");
    }
}
