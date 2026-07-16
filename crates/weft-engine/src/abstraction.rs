//! The value-abstraction pass: at `commit` time, concrete literals in the
//! recorded diff that exactly match current answer values become
//! `{"answer": id}` segments, so the patch replays correctly under different
//! answers.
//!
//! Decisions have two levels:
//! - **per answer** (`confirmed`): does this answer abstract at all — this
//!   also drives hunk *context* lines, which must mirror the base render;
//! - **per occurrence** (`excepted`): individual matches in **added or
//!   created content** the author keeps literal (a value that happens to
//!   appear in unrelated prose). Keys are `(path, line, nth)` — 1-based
//!   line in the new file, 1-based nth match of that answer within the
//!   line — assigned in scan order, so they are stable for UIs to point at.
//!
//! Secrets are exempt from both: every occurrence is always abstracted
//! (a literal secret in a patch file would persist it).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::{AnswerId, AnswerSet, Content, Line, Segment, TemplatePath, Value};

use crate::interact::Interaction;

/// One candidate substitution: a concrete text and the answer it came from.
struct Sub {
    text: String,
    id: AnswerId,
    /// Secrets are abstracted unconditionally — leaving a secret literal in a
    /// patch file would persist it, which weft must never do.
    mandatory: bool,
}

pub struct Abstractor {
    subs: Vec<Sub>,
}

/// One abstraction proposal: this concrete text occurs in the diff and
/// matches this answer's current value.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Candidate {
    pub id: AnswerId,
    /// The concrete text (for secrets this is the resolved value — callers
    /// exposing candidates externally should use placeholder secrets).
    pub text: String,
    pub occurrences: usize,
    /// Secrets: must be abstracted, never optional.
    pub mandatory: bool,
}

/// One concrete match of an answer value in added/created content — the
/// unit a user can keep literal.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Occurrence {
    pub id: AnswerId,
    pub text: String,
    pub path: Utf8PathBuf,
    /// 1-based line number in the new file content.
    pub line: usize,
    /// 1-based index of this answer's match within the line, in scan order.
    pub nth: usize,
    /// Byte column of the match start within the line (display only).
    pub col: usize,
    pub mandatory: bool,
}

/// Declined occurrences: (answer, path, line, nth).
pub type Excepted = BTreeSet<(AnswerId, Utf8PathBuf, usize, usize)>;

/// How the author decided one candidate, interactively.
pub enum CandidateDecision {
    All,
    None,
    /// Keep these occurrence indices (into the offered list) literal.
    Except(Vec<usize>),
}

impl Abstractor {
    /// Candidates: string/choice answers of length ≥ 2 (shorter values
    /// false-positive too easily; ints/bools are not abstracted in MVP) plus
    /// all secret values. Longest text wins where candidates overlap.
    pub fn from_answers(answers: &AnswerSet) -> Self {
        let mut subs: Vec<Sub> = answers
            .iter()
            .filter_map(|(id, value)| match value {
                Value::String(s) if s.len() >= 2 => Some(Sub {
                    text: s.clone(),
                    id: id.clone(),
                    mandatory: false,
                }),
                Value::Secret(s) if !s.expose().is_empty() => Some(Sub {
                    text: s.expose().to_owned(),
                    id: id.clone(),
                    mandatory: true,
                }),
                _ => None,
            })
            .collect();
        subs.sort_by(|a, b| {
            b.text
                .len()
                .cmp(&a.text.len())
                .then_with(|| a.id.cmp(&b.id))
        });
        Self { subs }
    }

    /// The candidates that actually occur in `texts`, for callers that drive
    /// confirmation themselves (a web UI, a `--yes` batch run, …). Mandatory
    /// candidates (secrets) must always be applied.
    pub fn candidates(&self, texts: &[&str]) -> Vec<Candidate> {
        self.subs
            .iter()
            .filter_map(|sub| {
                let occurrences: usize = texts.iter().map(|t| t.matches(&sub.text).count()).sum();
                (occurrences > 0).then(|| Candidate {
                    id: sub.id.clone(),
                    text: sub.text.clone(),
                    occurrences,
                    mandatory: sub.mandatory,
                })
            })
            .collect()
    }

    /// Enumerate the occurrences in one file's new content, in the exact
    /// order substitution will consume them (longest-sub-first within a
    /// line, left-to-right). `added_lines` restricts to the lines a UI
    /// should offer (1-based); `None` = the whole file (created files).
    pub fn occurrences_in(
        &self,
        path: &Utf8Path,
        text: &str,
        added_lines: Option<&BTreeSet<usize>>,
    ) -> Vec<Occurrence> {
        let mut out = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let line_no = idx + 1;
            if let Some(filter) = added_lines {
                if !filter.contains(&line_no) {
                    continue;
                }
            }
            let mut counters: BTreeMap<AnswerId, usize> = BTreeMap::new();
            let mut collector = Vec::new();
            self.walk(line, 0, 0, &mut counters, &mut |sub, nth, col| {
                collector.push(Occurrence {
                    id: sub.id.clone(),
                    text: sub.text.clone(),
                    path: path.to_owned(),
                    line: line_no,
                    nth,
                    col,
                    mandatory: sub.mandatory,
                });
            });
            collector.sort_by_key(|o| o.col);
            out.extend(collector);
        }
        out
    }

    /// Turn external per-candidate decisions into the confirmed map used by
    /// the substitution methods. Mandatory candidates are always applied;
    /// undecided ones default to `true` (exact matches are the
    /// high-confidence default).
    pub fn confirmed_from_decisions(
        &self,
        texts: &[&str],
        decisions: &BTreeMap<AnswerId, bool>,
    ) -> BTreeMap<AnswerId, bool> {
        self.candidates(texts)
            .into_iter()
            .map(|c| {
                let yes = c.mandatory || decisions.get(&c.id).copied().unwrap_or(true);
                (c.id, yes)
            })
            .collect()
    }

    /// Ask the user which non-mandatory candidates that actually occur in
    /// `texts` should be abstracted. Returns the confirmed candidate ids.
    /// Non-interactive runs confirm everything.
    pub fn confirm(
        &self,
        texts: &[&str],
        interaction: &mut dyn Interaction,
    ) -> Result<BTreeMap<AnswerId, bool>> {
        let mut confirmed = BTreeMap::new();
        for candidate in self.candidates(texts) {
            let yes = candidate.mandatory
                || interaction.confirm(
                    &format!(
                        "abstract {} occurrence(s) of {:?} as answer `{}`?",
                        candidate.occurrences, candidate.text, candidate.id
                    ),
                    true,
                )?;
            confirmed.insert(candidate.id, yes);
        }
        Ok(confirmed)
    }

    /// Interactive per-candidate + per-occurrence confirmation. For each
    /// non-mandatory candidate the interaction may keep individual
    /// occurrences (from `occurrences`) literal. Returns the coarse map plus
    /// the excepted set.
    pub fn confirm_with_occurrences(
        &self,
        texts: &[&str],
        occurrences: &[Occurrence],
        interaction: &mut dyn Interaction,
    ) -> Result<(BTreeMap<AnswerId, bool>, Excepted)> {
        let mut confirmed = BTreeMap::new();
        let mut excepted = Excepted::new();
        for candidate in self.candidates(texts) {
            if candidate.mandatory {
                confirmed.insert(candidate.id, true);
                continue;
            }
            let offered: Vec<&Occurrence> = occurrences
                .iter()
                .filter(|o| o.id == candidate.id)
                .collect();
            let items: Vec<String> = offered
                .iter()
                .map(|o| format!("{}:{} · {}", o.path, o.line, o.text))
                .collect();
            let decision = interaction.decide_candidate(
                &format!(
                    "abstract {} occurrence(s) of {:?} as answer `{}`?",
                    candidate.occurrences, candidate.text, candidate.id
                ),
                &items,
            )?;
            match decision {
                CandidateDecision::All => {
                    confirmed.insert(candidate.id, true);
                }
                CandidateDecision::None => {
                    confirmed.insert(candidate.id, false);
                }
                CandidateDecision::Except(keep_literal) => {
                    confirmed.insert(candidate.id.clone(), true);
                    for i in keep_literal {
                        if let Some(o) = offered.get(i) {
                            excepted.insert((o.id.clone(), o.path.clone(), o.line, o.nth));
                        }
                    }
                }
            }
        }
        Ok((confirmed, excepted))
    }

    /// Split one line of concrete text into segments, replacing confirmed
    /// values with answer references. Context-free (hunk context/removed
    /// lines, deleted paths): per-occurrence exceptions don't apply.
    pub fn line(&self, text: &str, confirmed: &BTreeMap<AnswerId, bool>) -> Line {
        Line(self.substitute(text, confirmed, None, &Excepted::new()))
    }

    /// Like [`Self::line`] but for an added line at `(path, line_no)` —
    /// per-occurrence exceptions apply.
    pub fn line_at(
        &self,
        path: &Utf8Path,
        line_no: usize,
        text: &str,
        confirmed: &BTreeMap<AnswerId, bool>,
        excepted: &Excepted,
    ) -> Line {
        Line(self.substitute(text, confirmed, Some((path, line_no)), excepted))
    }

    pub fn content(&self, text: &str, confirmed: &BTreeMap<AnswerId, bool>) -> Content {
        Content(text.lines().map(|l| self.line(l, confirmed)).collect())
    }

    /// Whole-file content of a created file — every line is authored, so
    /// per-occurrence exceptions apply (lines are 1-based).
    pub fn content_at(
        &self,
        path: &Utf8Path,
        text: &str,
        confirmed: &BTreeMap<AnswerId, bool>,
        excepted: &Excepted,
    ) -> Content {
        Content(
            text.lines()
                .enumerate()
                .map(|(i, l)| self.line_at(path, i + 1, l, confirmed, excepted))
                .collect(),
        )
    }

    pub fn path(&self, text: &str, confirmed: &BTreeMap<AnswerId, bool>) -> TemplatePath {
        TemplatePath(self.substitute(text, confirmed, None, &Excepted::new()))
    }

    /// One-line substitution honoring coarse confirmation and (when a
    /// context is given) per-occurrence exceptions. `nth` counters advance
    /// for every match of a confirmed answer — excepted or not — so keys
    /// stay stable regardless of decisions.
    fn substitute(
        &self,
        text: &str,
        confirmed: &BTreeMap<AnswerId, bool>,
        ctx: Option<(&Utf8Path, usize)>,
        excepted: &Excepted,
    ) -> Vec<Segment> {
        let mut counters: BTreeMap<AnswerId, usize> = BTreeMap::new();
        // (col, len, id, mandatory, nth)
        let mut matches: Vec<(usize, usize, AnswerId, bool, usize)> = Vec::new();
        self.walk(text, 0, 0, &mut counters, &mut |sub, nth, col| {
            if confirmed.get(&sub.id).copied().unwrap_or(false) {
                matches.push((col, sub.text.len(), sub.id.clone(), sub.mandatory, nth));
            }
        });
        matches.sort_by_key(|(col, ..)| *col);

        let mut out = Vec::new();
        let mut cursor = 0usize;
        for (col, len, id, mandatory, nth) in matches {
            let keep_literal = match ctx {
                Some((path, line_no)) if !mandatory => {
                    excepted.contains(&(id.clone(), path.to_owned(), line_no, nth))
                }
                _ => false,
            };
            if keep_literal {
                continue; // stays part of the literal run
            }
            if col > cursor {
                out.push(Segment::Literal(text[cursor..col].to_owned()));
            }
            out.push(Segment::Answer(id));
            cursor = col + len;
        }
        if cursor < text.len() {
            out.push(Segment::Literal(text[cursor..].to_owned()));
        }
        out
    }

    /// Scan `text` exactly as substitution does — longest sub first,
    /// left-to-right, fragments between longer matches offered to shorter
    /// subs — reporting every match with its per-answer `nth` (assigned in
    /// column order per answer) and absolute byte column.
    fn walk(
        &self,
        text: &str,
        sub_index: usize,
        offset: usize,
        counters: &mut BTreeMap<AnswerId, usize>,
        report: &mut dyn FnMut(&Sub, usize, usize),
    ) {
        let Some(sub) = self.subs.get(sub_index) else {
            return;
        };
        let mut rest = text;
        let mut base = offset;
        while let Some(pos) = rest.find(&sub.text) {
            // Fragments before this match go to shorter subs first, so their
            // matches (at smaller columns) must be counted before this one.
            self.walk(&rest[..pos], sub_index + 1, base, counters, report);
            let counter = counters.entry(sub.id.clone()).or_insert(0);
            *counter += 1;
            report(sub, *counter, base + pos);
            base += pos + sub.text.len();
            rest = &rest[pos + sub.text.len()..];
        }
        self.walk(rest, sub_index + 1, base, counters, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interact::NonInteractive;
    use weft_core::SecretValue;

    fn answers() -> AnswerSet {
        [
            (
                AnswerId::from("project_name"),
                Value::String("My Demo".into()),
            ),
            (
                AnswerId::from("package_name"),
                Value::String("my-demo".into()),
            ),
            (
                AnswerId::from("token"),
                Value::Secret(SecretValue::new("s3cr3t".into())),
            ),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn abstracts_confirmed_values_longest_first() {
        let a = Abstractor::from_answers(&answers());
        let confirmed = a
            .confirm(&["title My Demo, pkg my-demo"], &mut NonInteractive)
            .unwrap();
        let line = a.line("title My Demo, pkg my-demo", &confirmed);
        assert_eq!(
            line.0,
            vec![
                Segment::Literal("title ".into()),
                Segment::Answer("project_name".into()),
                Segment::Literal(", pkg ".into()),
                Segment::Answer("package_name".into()),
            ]
        );
    }

    #[test]
    fn secrets_always_abstracted() {
        let a = Abstractor::from_answers(&answers());
        let confirmed = a.confirm(&["key=s3cr3t"], &mut NonInteractive).unwrap();
        let line = a.line("key=s3cr3t", &confirmed);
        assert_eq!(
            line.0,
            vec![
                Segment::Literal("key=".into()),
                Segment::Answer("token".into()),
            ]
        );
    }

    #[test]
    fn unconfirmed_values_stay_literal() {
        let a = Abstractor::from_answers(&answers());
        let confirmed = BTreeMap::new();
        let line = a.line("title My Demo", &confirmed);
        assert_eq!(line.0, vec![Segment::Literal("title My Demo".into())]);
    }

    #[test]
    fn occurrences_are_enumerated_in_scan_order_with_stable_keys() {
        let a = Abstractor::from_answers(&answers());
        let text = "My Demo uses my-demo\nsay My Demo twice: My Demo\n";
        let occ = a.occurrences_in(Utf8Path::new("README.md"), text, None);
        let keys: Vec<(String, usize, usize, usize)> = occ
            .iter()
            .map(|o| (o.id.to_string(), o.line, o.nth, o.col))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("project_name".into(), 1, 1, 0),
                ("package_name".into(), 1, 1, 13),
                ("project_name".into(), 2, 1, 4),
                ("project_name".into(), 2, 2, 19),
            ]
        );
    }

    #[test]
    fn excepted_occurrence_stays_literal_others_abstract() {
        let a = Abstractor::from_answers(&answers());
        let mut confirmed = BTreeMap::new();
        confirmed.insert(AnswerId::from("project_name"), true);
        let mut excepted = Excepted::new();
        // Keep the SECOND "My Demo" on the line literal.
        excepted.insert((
            AnswerId::from("project_name"),
            Utf8PathBuf::from("README.md"),
            1,
            2,
        ));
        let line = a.line_at(
            Utf8Path::new("README.md"),
            1,
            "say My Demo twice: My Demo",
            &confirmed,
            &excepted,
        );
        assert_eq!(
            line.0,
            vec![
                Segment::Literal("say ".into()),
                Segment::Answer("project_name".into()),
                Segment::Literal(" twice: My Demo".into()),
            ]
        );
    }

    #[test]
    fn secret_occurrences_cannot_be_excepted() {
        let a = Abstractor::from_answers(&answers());
        let confirmed = a.confirm(&["key=s3cr3t"], &mut NonInteractive).unwrap();
        let mut excepted = Excepted::new();
        excepted.insert((AnswerId::from("token"), Utf8PathBuf::from("cfg.txt"), 1, 1));
        let line = a.line_at(
            Utf8Path::new("cfg.txt"),
            1,
            "key=s3cr3t",
            &confirmed,
            &excepted,
        );
        assert_eq!(
            line.0,
            vec![
                Segment::Literal("key=".into()),
                Segment::Answer("token".into()),
            ]
        );
    }

    #[test]
    fn nth_keys_are_stable_when_an_occurrence_is_excepted() {
        // Excepting occurrence 1 must not renumber occurrence 2.
        let a = Abstractor::from_answers(&answers());
        let mut confirmed = BTreeMap::new();
        confirmed.insert(AnswerId::from("project_name"), true);
        let mut excepted = Excepted::new();
        excepted.insert((
            AnswerId::from("project_name"),
            Utf8PathBuf::from("README.md"),
            1,
            1,
        ));
        let line = a.line_at(
            Utf8Path::new("README.md"),
            1,
            "say My Demo twice: My Demo",
            &confirmed,
            &excepted,
        );
        assert_eq!(
            line.0,
            vec![
                Segment::Literal("say My Demo twice: ".into()),
                Segment::Answer("project_name".into()),
            ]
        );
    }
}
