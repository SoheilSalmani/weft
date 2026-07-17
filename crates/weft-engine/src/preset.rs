//! Presets v2: a preset is a *partial* answer file whose entries **lock**
//! what they answer.
//!
//! - A scalar (or plain list) entry locks that question: it is skipped in
//!   interactive flows and an explicit `--answer` for it is an error.
//! - A table entry constrains a **multichoice**: `fixed` choices are always
//!   selected (and can't be unchecked), `blocked` choices are never
//!   selectable; everything else stays free. The final value is
//!   `(user selection ∪ fixed) − blocked`.
//!
//! Secrets can never appear in presets (they resolve via their source).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use weft_core::{AnswerId, AnswerKind, AnswerSet, Value};

use crate::template::Template;

/// One preset entry.
#[derive(Debug, Clone, PartialEq)]
pub enum PresetEntry {
    /// Locks the question to this value.
    Lock(Value),
    /// Multichoice constraint: `fixed` always on, `blocked` never on.
    Constraint {
        fixed: Vec<String>,
        blocked: Vec<String>,
    },
}

/// A parsed preset file (or the merge of several selected presets).
#[derive(Debug, Clone, Default)]
pub struct PresetSpec {
    pub entries: BTreeMap<AnswerId, PresetEntry>,
    /// Which preset(s) contributed each entry — for error messages.
    pub sources: BTreeMap<AnswerId, String>,
}

/// The multichoice constraint form as it appears in TOML.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstraintForm {
    #[serde(default)]
    fixed: Vec<String>,
    #[serde(default)]
    blocked: Vec<String>,
}

impl PresetSpec {
    /// Parse one preset file's TOML source.
    pub fn parse(name: &str, src: &str) -> Result<Self> {
        let table: toml::Table =
            toml::from_str(src).with_context(|| format!("parsing preset `{name}`"))?;
        let mut entries = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for (key, value) in table {
            let id = AnswerId(key.clone());
            let entry = match value {
                toml::Value::Table(_) => {
                    let form: ConstraintForm = value.try_into().with_context(|| {
                        format!(
                            "preset `{name}`, entry `{key}`: a table entry constrains a \
                             multichoice and takes only `fixed` and `blocked` lists"
                        )
                    })?;
                    PresetEntry::Constraint {
                        fixed: form.fixed,
                        blocked: form.blocked,
                    }
                }
                other => {
                    let v: Value = other.try_into().with_context(|| {
                        format!("preset `{name}`, entry `{key}`: unsupported value")
                    })?;
                    PresetEntry::Lock(v)
                }
            };
            entries.insert(id.clone(), entry);
            sources.insert(id, name.to_owned());
        }
        Ok(Self { entries, sources })
    }

    /// Merge selected presets in order. Locks must agree (same value);
    /// constraints union; a lock and a constraint on the same id conflict.
    pub fn merge(specs: Vec<PresetSpec>) -> Result<PresetSpec> {
        let mut out = PresetSpec::default();
        for spec in specs {
            for (id, entry) in spec.entries {
                let source = spec.sources.get(&id).cloned().unwrap_or_default();
                match out.entries.get_mut(&id) {
                    None => {
                        out.entries.insert(id.clone(), entry);
                        out.sources.insert(id, source);
                    }
                    Some(existing) => match (existing, entry) {
                        (PresetEntry::Lock(a), PresetEntry::Lock(b)) => {
                            if *a != b {
                                bail!(
                                    "presets `{}` and `{source}` lock `{id}` to different values",
                                    out.sources.get(&id).map(String::as_str).unwrap_or("?"),
                                );
                            }
                        }
                        (
                            PresetEntry::Constraint { fixed, blocked },
                            PresetEntry::Constraint {
                                fixed: f2,
                                blocked: b2,
                            },
                        ) => {
                            for c in f2 {
                                if !fixed.contains(&c) {
                                    fixed.push(c);
                                }
                            }
                            for c in b2 {
                                if !blocked.contains(&c) {
                                    blocked.push(c);
                                }
                            }
                        }
                        _ => bail!(
                            "presets `{}` and `{source}` disagree on `{id}` \
                             (one locks it, the other constrains it)",
                            out.sources.get(&id).map(String::as_str).unwrap_or("?"),
                        ),
                    },
                }
            }
        }
        // A choice can't be both fixed and blocked.
        for (id, entry) in &out.entries {
            if let PresetEntry::Constraint { fixed, blocked } = entry {
                if let Some(c) = fixed.iter().find(|c| blocked.contains(c)) {
                    bail!("`{id}`: choice {c:?} is both fixed and blocked across the selected presets");
                }
            }
        }
        Ok(out)
    }

    /// Static validation against the template's questions.
    pub fn validate(&self, template: &Template) -> Result<()> {
        for (id, entry) in &self.entries {
            let question = template
                .manifest
                .questions
                .iter()
                .find(|q| q.id == *id)
                .with_context(|| {
                    format!(
                        "preset `{}` answers unknown question `{id}`",
                        self.sources.get(id).map(String::as_str).unwrap_or("?")
                    )
                })?;
            match (&question.kind, entry) {
                (AnswerKind::Secret { .. }, _) => {
                    bail!("`{id}` is a secret; secrets can never appear in presets")
                }
                (
                    AnswerKind::MultiChoice { choices },
                    PresetEntry::Constraint { fixed, blocked },
                ) => {
                    for c in fixed.iter().chain(blocked) {
                        if !choices.contains(c) {
                            bail!("`{id}`: {c:?} is not one of the declared choices");
                        }
                    }
                    if let Some(c) = fixed.iter().find(|c| blocked.contains(c)) {
                        bail!("`{id}`: choice {c:?} is both fixed and blocked");
                    }
                }
                (_, PresetEntry::Constraint { .. }) => {
                    bail!("`{id}`: fixed/blocked constraints only apply to multichoice questions")
                }
                (kind, PresetEntry::Lock(value)) => {
                    let ok = matches!(
                        (kind, value),
                        (AnswerKind::String, Value::String(_))
                            | (AnswerKind::Bool, Value::Bool(_))
                            | (AnswerKind::Int, Value::Int(_))
                            | (AnswerKind::Choice { .. }, Value::String(_))
                            | (AnswerKind::MultiChoice { .. }, Value::List(_))
                    );
                    if !ok {
                        bail!(
                            "`{id}`: preset value is a {} but the question is a {kind:?}",
                            value.kind_name()
                        );
                    }
                    if let (AnswerKind::Choice { choices }, Value::String(s)) = (kind, value) {
                        if !choices.contains(s) {
                            bail!("`{id}`: {s:?} is not one of the declared choices");
                        }
                    }
                    if let (AnswerKind::MultiChoice { choices }, Value::List(items)) = (kind, value)
                    {
                        for item in items {
                            if let Value::String(s) = item {
                                if !choices.contains(s) {
                                    bail!("`{id}`: {s:?} is not one of the declared choices");
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// The locked ids (both plain locks and — for skipping purposes —
    /// nothing else: constraints leave the question interactive).
    pub fn locked(&self) -> BTreeSet<AnswerId> {
        self.entries
            .iter()
            .filter(|(_, e)| matches!(e, PresetEntry::Lock(_)))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The locked values as an answer layer.
    pub fn lock_answers(&self) -> AnswerSet {
        self.entries
            .iter()
            .filter_map(|(id, e)| match e {
                PresetEntry::Lock(v) => Some((id.clone(), v.clone())),
                _ => None,
            })
            .collect()
    }

    /// Constraints by id: (fixed, blocked).
    pub fn constraints(&self) -> BTreeMap<AnswerId, (Vec<String>, Vec<String>)> {
        self.entries
            .iter()
            .filter_map(|(id, e)| match e {
                PresetEntry::Constraint { fixed, blocked } => {
                    Some((id.clone(), (fixed.clone(), blocked.clone())))
                }
                _ => None,
            })
            .collect()
    }

    /// Enforce the spec over user-provided answers: locked ids must not be
    /// re-answered (unless identical); constrained multichoice values merge
    /// as `(user ∪ fixed) − blocked`, erroring on explicitly blocked picks.
    pub fn apply(&self, user: &AnswerSet) -> Result<AnswerSet> {
        let mut out = self.lock_answers();
        let constraints = self.constraints();
        for (id, value) in user.iter() {
            if let Some(PresetEntry::Lock(locked)) = self.entries.get(id) {
                if locked != value {
                    bail!(
                        "`{id}` is locked by preset `{}` — remove the explicit answer",
                        self.sources.get(id).map(String::as_str).unwrap_or("?")
                    );
                }
                continue;
            }
            if let Some((fixed, blocked)) = constraints.get(id) {
                let mut items: Vec<Value> = match value {
                    Value::List(items) => items.clone(),
                    other => vec![other.clone()],
                };
                for item in &items {
                    if let Value::String(s) = item {
                        if blocked.contains(s) {
                            bail!(
                                "`{id}`: choice {s:?} is blocked by preset `{}`",
                                self.sources.get(id).map(String::as_str).unwrap_or("?")
                            );
                        }
                    }
                }
                for f in fixed {
                    if !items
                        .iter()
                        .any(|v| matches!(v, Value::String(s) if s == f))
                    {
                        items.push(Value::String(f.clone()));
                    }
                }
                out.insert(id.clone(), Value::List(items));
                continue;
            }
            out.insert(id.clone(), value.clone());
        }
        // A constrained multichoice the user never touched still gets its
        // fixed choices as the starting selection.
        for (id, (fixed, _)) in &constraints {
            if !out.contains(id) && !fixed.is_empty() {
                out.insert(
                    id.clone(),
                    Value::List(fixed.iter().map(|s| Value::String(s.clone())).collect()),
                );
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, src: &str) -> PresetSpec {
        PresetSpec::parse(name, src).unwrap()
    }

    #[test]
    fn parses_locks_and_constraints() {
        let s = spec(
            "corp",
            "project_name = \"Acme\"\nuse_docker = true\n\n[features]\nfixed = [\"lint\"]\nblocked = [\"experimental\"]\n",
        );
        assert!(matches!(
            s.entries.get(&AnswerId::from("project_name")),
            Some(PresetEntry::Lock(Value::String(v))) if v == "Acme"
        ));
        assert!(matches!(
            s.entries.get(&AnswerId::from("features")),
            Some(PresetEntry::Constraint { fixed, blocked })
                if fixed == &vec!["lint".to_owned()] && blocked == &vec!["experimental".to_owned()]
        ));
        assert_eq!(s.locked().len(), 2);
    }

    #[test]
    fn lock_conflicts_error_and_identical_locks_merge() {
        let a = spec("a", "name = \"x\"\n");
        let b = spec("b", "name = \"y\"\n");
        assert!(PresetSpec::merge(vec![a.clone(), b]).is_err());
        let same = spec("c", "name = \"x\"\n");
        assert!(PresetSpec::merge(vec![a, same]).is_ok());
    }

    #[test]
    fn constraints_union_and_fixed_blocked_overlap_errors() {
        let a = spec("a", "[features]\nfixed = [\"lint\"]\n");
        let b = spec("b", "[features]\nblocked = [\"exp\"]\nfixed = [\"ci\"]\n");
        let merged = PresetSpec::merge(vec![a, b]).unwrap();
        match merged.entries.get(&AnswerId::from("features")).unwrap() {
            PresetEntry::Constraint { fixed, blocked } => {
                assert_eq!(fixed, &vec!["lint".to_owned(), "ci".to_owned()]);
                assert_eq!(blocked, &vec!["exp".to_owned()]);
            }
            _ => panic!("expected constraint"),
        }
        let c = spec("c", "[features]\nblocked = [\"lint\"]\n");
        let a2 = spec("a", "[features]\nfixed = [\"lint\"]\n");
        assert!(PresetSpec::merge(vec![a2, c]).is_err());
    }

    #[test]
    fn apply_locks_constraints_and_blocked_picks() {
        let s = spec(
            "corp",
            "name = \"Acme\"\n\n[features]\nfixed = [\"lint\"]\nblocked = [\"exp\"]\n",
        );
        // Re-answering a lock identically is fine; differently errors.
        let mut user = AnswerSet::new();
        user.insert(AnswerId::from("name"), Value::String("Acme".into()));
        assert!(s.apply(&user).is_ok());
        user.insert(AnswerId::from("name"), Value::String("Other".into()));
        assert!(s.apply(&user).unwrap_err().to_string().contains("locked"));

        // (user ∪ fixed) − blocked; blocked picks error.
        let mut user = AnswerSet::new();
        user.insert(
            AnswerId::from("features"),
            Value::List(vec![Value::String("docs".into())]),
        );
        let out = s.apply(&user).unwrap();
        assert_eq!(
            out.get(&AnswerId::from("features")),
            Some(&Value::List(vec![
                Value::String("docs".into()),
                Value::String("lint".into())
            ]))
        );
        let mut bad = AnswerSet::new();
        bad.insert(
            AnswerId::from("features"),
            Value::List(vec![Value::String("exp".into())]),
        );
        assert!(s.apply(&bad).unwrap_err().to_string().contains("blocked"));

        // Untouched constrained multichoice starts from its fixed set.
        let out = s.apply(&AnswerSet::new()).unwrap();
        assert_eq!(
            out.get(&AnswerId::from("features")),
            Some(&Value::List(vec![Value::String("lint".into())]))
        );
    }
}
