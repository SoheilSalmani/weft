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

use anyhow::{anyhow, bail, Context, Result};
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
            // A question an extending template narrowed (ADR-0002): its
            // choices already exclude what the template blocks.
            let narrowing = &question.narrowing;
            if narrowing.locked {
                bail!(
                    "`{id}` is locked by {}; a preset cannot answer it",
                    narrowing.refiners()
                );
            }
            let undeclared = |c: &String| {
                if narrowing.blocked.contains(c) {
                    anyhow!("`{id}`: {c:?} is blocked by {}", narrowing.refiners())
                } else {
                    anyhow!("`{id}`: {c:?} is not one of the declared choices")
                }
            };
            match (&question.kind, entry) {
                (AnswerKind::Secret { .. }, _) => {
                    bail!("`{id}` is a secret; secrets can never appear in presets")
                }
                (
                    AnswerKind::MultiChoice { choices },
                    PresetEntry::Constraint { fixed, blocked },
                ) => {
                    if let Some(c) = fixed.iter().find(|c| !choices.contains(c)) {
                        return Err(undeclared(c));
                    }
                    // Blocking what the template already blocks changes
                    // nothing.
                    if let Some(c) = blocked
                        .iter()
                        .find(|c| !choices.contains(c) && !narrowing.blocked.contains(c))
                    {
                        return Err(undeclared(c));
                    }
                    if let Some(c) = blocked.iter().find(|c| narrowing.fixed.contains(c)) {
                        bail!(
                            "`{id}`: {c:?} is fixed by {}; a preset cannot block it",
                            narrowing.refiners()
                        );
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
                            return Err(undeclared(s));
                        }
                    }
                    if let (AnswerKind::MultiChoice { choices }, Value::List(items)) = (kind, value)
                    {
                        for item in items {
                            if let Value::String(s) = item {
                                if !choices.contains(s) {
                                    return Err(undeclared(s));
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

    /// Serialize back to a preset file body: lock entries first (TOML
    /// scalars must precede tables), then one `[id]` table per constraint.
    pub fn to_toml(&self) -> Result<String> {
        use std::fmt::Write as _;
        let mut out = String::new();
        for (id, entry) in &self.entries {
            if let PresetEntry::Lock(v) = entry {
                let tv = toml::Value::try_from(v.clone())
                    .with_context(|| format!("serializing preset entry `{id}`"))?;
                writeln!(out, "{} = {tv}", toml_key(&id.0))?;
            }
        }
        for (id, entry) in &self.entries {
            if let PresetEntry::Constraint { fixed, blocked } = entry {
                if !out.is_empty() {
                    out.push('\n');
                }
                writeln!(out, "[{}]", toml_key(&id.0))?;
                for (label, list) in [("fixed", fixed), ("blocked", blocked)] {
                    if !list.is_empty() {
                        let arr = toml::Value::Array(
                            list.iter().cloned().map(toml::Value::String).collect(),
                        );
                        writeln!(out, "{label} = {arr}")?;
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Quote a key unless it's a bare TOML key.
fn toml_key(k: &str) -> String {
    let bare = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if bare {
        k.to_owned()
    } else {
        format!("{k:?}")
    }
}

/// Write `presets/<name>.toml` (creating or rewriting) and, when new, append
/// the `[[preset]]` declaration to `weft.toml`. Validates against the
/// template first and re-validates the saved file by reloading; any failure
/// restores the previous state.
pub fn save(
    root: &camino::Utf8Path,
    name: &str,
    spec: &PresetSpec,
    resolver: &mut dyn crate::template::IncludeResolver,
) -> Result<()> {
    let name_ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !name_ok {
        bail!("preset name must be alphanumeric with - or _");
    }
    if spec.entries.is_empty() {
        bail!("preset `{name}` is empty — answer or constrain at least one question");
    }
    let template = Template::load_with(root, resolver)?;
    spec.validate(&template)?;

    let existing = template.manifest.presets.iter().find(|p| p.name == name);
    let file_rel = existing
        .map(|d| d.file.clone())
        .unwrap_or_else(|| format!("presets/{name}.toml").into());
    let file_path = root.join(&file_rel);
    let original_file = std::fs::read_to_string(&file_path).ok();

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file_path, spec.to_toml()?)?;

    let restore_file = |original: &Option<String>| match original {
        Some(content) => {
            let _ = std::fs::write(&file_path, content);
        }
        None => {
            let _ = std::fs::remove_file(&file_path);
        }
    };

    if existing.is_none() {
        let manifest_path = root.join(crate::template::MANIFEST_FILE);
        let manifest_src = std::fs::read_to_string(&manifest_path)?;
        let mut doc: toml_edit::DocumentMut = manifest_src
            .parse()
            .with_context(|| format!("parsing {manifest_path}"))?;
        if doc.get("preset").is_none() {
            doc["preset"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }
        let Some(decls) = doc["preset"].as_array_of_tables_mut() else {
            restore_file(&original_file);
            bail!("`preset` in {manifest_path} is not an array of tables");
        };
        let mut t = toml_edit::Table::new();
        t["name"] = toml_edit::value(name);
        t["file"] = toml_edit::value(file_rel.as_str());
        decls.push(t);
        if let Err(e) = std::fs::write(&manifest_path, doc.to_string()) {
            restore_file(&original_file);
            return Err(e.into());
        }
    }

    // The saved template must load and the preset must round-trip.
    if let Err(e) = Template::load_with(root, resolver).and_then(|t| t.preset_spec(name)) {
        restore_file(&original_file);
        bail!("saved preset failed validation: {e:#}");
    }
    Ok(())
}

/// Remove a preset: its `[[preset]]` declaration and its file.
pub fn remove(
    root: &camino::Utf8Path,
    name: &str,
    resolver: &mut dyn crate::template::IncludeResolver,
) -> Result<()> {
    let template = Template::load_with(root, resolver)?;
    let decl = template
        .manifest
        .presets
        .iter()
        .find(|p| p.name == name)
        .with_context(|| format!("no preset `{name}`"))?;
    let file_path = root.join(&decl.file);

    let manifest_path = root.join(crate::template::MANIFEST_FILE);
    let manifest_src = std::fs::read_to_string(&manifest_path)?;
    let mut doc: toml_edit::DocumentMut = manifest_src
        .parse()
        .with_context(|| format!("parsing {manifest_path}"))?;
    let decls = doc
        .get_mut("preset")
        .and_then(|i| i.as_array_of_tables_mut())
        .with_context(|| "no presets declared")?;
    let idx = (0..decls.len())
        .find(|&i| {
            decls
                .get(i)
                .and_then(|t| t.get("name"))
                .and_then(|v| v.as_str())
                == Some(name)
        })
        .with_context(|| format!("no preset `{name}`"))?;
    decls.remove(idx);
    if decls.is_empty() {
        doc.remove("preset");
    }
    std::fs::write(&manifest_path, doc.to_string())?;
    let _ = std::fs::remove_file(&file_path);
    Ok(())
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

    #[test]
    fn to_toml_round_trips() {
        let s = spec(
            "corp",
            "name = \"Acme\"\nport = 8080\nuse_docker = true\n\n[features]\nfixed = [\"lint\"]\nblocked = [\"exp\"]\n",
        );
        let out = s.to_toml().unwrap();
        let back = PresetSpec::parse("corp", &out).unwrap();
        assert_eq!(back.entries, s.entries);
    }

    #[test]
    fn presets_cannot_undo_what_an_extending_template_narrowed() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let base = "[template]\nname = \"base\"\nweft-version = \"0.1\"\n\n\
                    [[question]]\nid = \"use_jira\"\nkind = \"bool\"\ndefault = \"False\"\n\n\
                    [[question]]\nid = \"skills\"\nkind = \"multichoice\"\n\
                    choices = [\"dbt\", \"sql\", \"airflow\"]\ndefault = \"[]\"\n";
        let dbt = "[template]\nname = \"dbt\"\nweft-version = \"0.1\"\nextends = \"../base\"\n\n\
                   [refine.use_jira]\nlock = \"False\"\n\n\
                   [refine.skills]\nblocked = [\"airflow\"]\nfixed = [\"dbt\"]\n";
        for (name, manifest) in [("base", base), ("dbt", dbt)] {
            std::fs::create_dir_all(root.join(name).join("patches")).unwrap();
            std::fs::write(root.join(name).join("weft.toml"), manifest).unwrap();
        }
        let template = Template::load(&root.join("dbt")).unwrap();
        let validate = |src: &str| {
            spec("p", src)
                .validate(&template)
                .map_err(|e| format!("{e:#}"))
        };

        for (src, expected) in [
            (
                "use_jira = false\n",
                "`use_jira` is locked by template `dbt`; a preset cannot answer it",
            ),
            (
                "[skills]\nblocked = [\"dbt\"]\n",
                "`skills`: \"dbt\" is fixed by template `dbt`; a preset cannot block it",
            ),
            (
                "[skills]\nfixed = [\"airflow\"]\n",
                "`skills`: \"airflow\" is blocked by template `dbt`",
            ),
            (
                "skills = [\"sql\", \"airflow\"]\n",
                "`skills`: \"airflow\" is blocked by template `dbt`",
            ),
            (
                "[skills]\nfixed = [\"rust\"]\n",
                "`skills`: \"rust\" is not one of the declared choices",
            ),
        ] {
            assert_eq!(validate(src), Err(expected.to_owned()), "{src}");
        }
        // Fixing what the template fixes, or blocking what it blocks,
        // changes nothing and is fine.
        assert_eq!(
            validate("[skills]\nfixed = [\"dbt\", \"sql\"]\nblocked = [\"airflow\"]\n"),
            Ok(())
        );
    }
}
