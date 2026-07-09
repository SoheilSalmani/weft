//! Line-based 3-way merge (diff3-flavored), used by `weft update` to overlay
//! template changes onto a user-edited tree. Conflicts render as standard
//! conflict markers; nothing is ever silently clobbered.

use similar::{DiffOp, TextDiff};

pub const MARKER_OURS: &str = "<<<<<<< local";
pub const MARKER_SEP: &str = "=======";
pub const MARKER_THEIRS: &str = ">>>>>>> template";

#[derive(Debug, PartialEq, Eq)]
pub struct MergeOutcome {
    pub text: String,
    pub conflicts: usize,
}

impl MergeOutcome {
    pub fn is_clean(&self) -> bool {
        self.conflicts == 0
    }
}

/// One side's edit against the base: replace base lines `[start, end)` with
/// `lines`. Insertions have `start == end`.
#[derive(Debug, Clone)]
struct Edit {
    start: usize,
    end: usize,
    lines: Vec<String>,
}

fn edits(base: &[&str], side: &[&str]) -> Vec<Edit> {
    TextDiff::from_slices(base, side)
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DiffOp::Equal { .. } => None,
            DiffOp::Delete {
                old_index, old_len, ..
            } => Some(Edit {
                start: old_index,
                end: old_index + old_len,
                lines: vec![],
            }),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => Some(Edit {
                start: old_index,
                end: old_index,
                lines: side[new_index..new_index + new_len]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            }),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => Some(Edit {
                start: old_index,
                end: old_index + old_len,
                lines: side[new_index..new_index + new_len]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            }),
        })
        .collect()
}

/// Apply the subset of `edits` that fall inside base region `[start, end)`
/// to produce that side's version of the region.
fn region_text(base: &[&str], edits: &[Edit], start: usize, end: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = start;
    for edit in edits {
        if edit.end < start || edit.start >= end {
            // Insertions exactly at `end` belong to this region only if the
            // region was expanded to include them (handled by the caller's
            // absorption loop), so a strict window is correct here.
            if !(edit.start == end && edit.end == end) {
                continue;
            }
        }
        let s = edit.start.max(start);
        out.extend(base[cursor..s].iter().map(|s| s.to_string()));
        out.extend(edit.lines.iter().cloned());
        cursor = edit.end.min(end).max(cursor);
    }
    out.extend(base[cursor..end].iter().map(|s| s.to_string()));
    out
}

/// 3-way merge `ours` and `theirs` against `base`. Line-granular; overlapping
/// (or directly adjacent) edit regions from both sides that produce different
/// text become conflicts with `<<<<<<< local` / `>>>>>>> template` markers.
pub fn merge3(base: &str, ours: &str, theirs: &str) -> MergeOutcome {
    let base_lines: Vec<&str> = base.lines().collect();
    let ours_lines: Vec<&str> = ours.lines().collect();
    let theirs_lines: Vec<&str> = theirs.lines().collect();

    let a = edits(&base_lines, &ours_lines);
    let b = edits(&base_lines, &theirs_lines);

    let mut out: Vec<String> = Vec::new();
    let mut conflicts = 0;
    let mut cursor = 0usize; // position in base
    let (mut ai, mut bi) = (0usize, 0usize);

    while ai < a.len() || bi < b.len() {
        // Pick the earliest pending edit and absorb everything (from both
        // sides) whose span touches the growing region.
        let start = match (a.get(ai), b.get(bi)) {
            (Some(x), Some(y)) => x.start.min(y.start),
            (Some(x), None) => x.start,
            (None, Some(y)) => y.start,
            (None, None) => unreachable!(),
        };
        let mut end = start;
        let (a_from, b_from) = (ai, bi);
        loop {
            let mut grew = false;
            while let Some(e) = a.get(ai) {
                if e.start <= end {
                    end = end.max(e.end);
                    ai += 1;
                    grew = true;
                } else {
                    break;
                }
            }
            while let Some(e) = b.get(bi) {
                if e.start <= end {
                    end = end.max(e.end);
                    bi += 1;
                    grew = true;
                } else {
                    break;
                }
            }
            if !grew {
                break;
            }
        }

        out.extend(base_lines[cursor..start].iter().map(|s| s.to_string()));
        cursor = end;

        let a_involved = ai > a_from;
        let b_involved = bi > b_from;
        let ours_region = region_text(&base_lines, &a[a_from..ai], start, end);
        let theirs_region = region_text(&base_lines, &b[b_from..bi], start, end);

        match (a_involved, b_involved) {
            (true, false) => out.extend(ours_region),
            (false, true) => out.extend(theirs_region),
            (true, true) if ours_region == theirs_region => out.extend(ours_region),
            (true, true) => {
                conflicts += 1;
                out.push(MARKER_OURS.to_owned());
                out.extend(ours_region);
                out.push(MARKER_SEP.to_owned());
                out.extend(theirs_region);
                out.push(MARKER_THEIRS.to_owned());
            }
            (false, false) => unreachable!("region always has at least one edit"),
        }
    }
    out.extend(base_lines[cursor..].iter().map(|s| s.to_string()));

    MergeOutcome {
        text: crate::segment::join_lines(&out),
        conflicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_overlapping_edits_both_apply() {
        let base = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let ours = "A\nb\nc\nd\ne\nf\ng\nh\n"; // change first line
        let theirs = "a\nb\nc\nd\ne\nf\ng\nH\n"; // change last line
        let m = merge3(base, ours, theirs);
        assert!(m.is_clean(), "{}", m.text);
        assert_eq!(m.text, "A\nb\nc\nd\ne\nf\ng\nH\n");
    }

    #[test]
    fn same_edit_on_both_sides_is_clean() {
        let base = "a\nb\nc\n";
        let changed = "a\nB\nc\n";
        let m = merge3(base, changed, changed);
        assert!(m.is_clean());
        assert_eq!(m.text, "a\nB\nc\n");
    }

    #[test]
    fn conflicting_edits_get_markers() {
        let base = "a\nb\nc\n";
        let ours = "a\nOURS\nc\n";
        let theirs = "a\nTHEIRS\nc\n";
        let m = merge3(base, ours, theirs);
        assert_eq!(m.conflicts, 1);
        assert_eq!(
            m.text,
            format!("a\n{MARKER_OURS}\nOURS\n{MARKER_SEP}\nTHEIRS\n{MARKER_THEIRS}\nc\n")
        );
    }

    #[test]
    fn ours_only_edit_is_kept() {
        let base = "a\nb\n";
        let ours = "a\nb\nmine\n";
        let m = merge3(base, ours, base);
        assert!(m.is_clean());
        assert_eq!(m.text, "a\nb\nmine\n");
    }

    #[test]
    fn theirs_only_edit_applies_over_untouched_file() {
        let base = "a\nb\n";
        let theirs = "a\nnew\nb\n";
        let m = merge3(base, base, theirs);
        assert!(m.is_clean());
        assert_eq!(m.text, "a\nnew\nb\n");
    }

    #[test]
    fn appends_from_both_sides_at_same_spot_conflict() {
        let base = "a\n";
        let ours = "a\nours\n";
        let theirs = "a\ntheirs\n";
        let m = merge3(base, ours, theirs);
        assert_eq!(m.conflicts, 1);
        assert!(m.text.contains(MARKER_OURS));
    }

    #[test]
    fn distant_edits_with_user_insert_apply_cleanly() {
        let base = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
        let ours = "one\ntwo\nthree\nuser line\nfour\nfive\nsix\nseven\n";
        let theirs = "one\ntwo\nthree\nfour\nfive\nsix\nseven\ntemplate line\n";
        let m = merge3(base, ours, theirs);
        assert!(m.is_clean(), "{}", m.text);
        assert!(m.text.contains("user line"));
        assert!(m.text.contains("template line"));
    }
}
