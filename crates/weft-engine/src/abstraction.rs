//! The value-abstraction pass: at `commit` time, concrete literals in the
//! recorded diff that exactly match current answer values become
//! `{"answer": id}` segments, so the patch replays correctly under different
//! answers.

use std::collections::BTreeMap;

use anyhow::Result;
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

    /// Split one line of concrete text into segments, replacing confirmed
    /// values with answer references.
    pub fn line(&self, text: &str, confirmed: &BTreeMap<AnswerId, bool>) -> Line {
        Line(self.segments(text, 0, confirmed))
    }

    pub fn content(&self, text: &str, confirmed: &BTreeMap<AnswerId, bool>) -> Content {
        Content(text.lines().map(|l| self.line(l, confirmed)).collect())
    }

    pub fn path(&self, text: &str, confirmed: &BTreeMap<AnswerId, bool>) -> TemplatePath {
        TemplatePath(self.segments(text, 0, confirmed))
    }

    fn segments(
        &self,
        text: &str,
        sub_index: usize,
        confirmed: &BTreeMap<AnswerId, bool>,
    ) -> Vec<Segment> {
        let Some(sub) = self.subs.get(sub_index) else {
            return if text.is_empty() {
                vec![]
            } else {
                vec![Segment::Literal(text.to_owned())]
            };
        };
        if !confirmed.get(&sub.id).copied().unwrap_or(false) {
            return self.segments(text, sub_index + 1, confirmed);
        }
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(pos) = rest.find(&sub.text) {
            out.extend(self.segments(&rest[..pos], sub_index + 1, confirmed));
            out.push(Segment::Answer(sub.id.clone()));
            rest = &rest[pos + sub.text.len()..];
        }
        out.extend(self.segments(rest, sub_index + 1, confirmed));
        out
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
}
