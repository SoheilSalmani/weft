//! `[refine.<id>]`: an extending template narrowing the questions it
//! inherits (ADR-0002). Applied once at load, so every reader of
//! `manifest.questions` sees the effective question; answer resolution then
//! enforces the narrowing through [`weft_core::render::narrow`].
//!
//! A refinement only narrows: it can reword a question, replace its default,
//! lock it, block choices (or keep only some), and fix multichoice options.
//! It cannot add a choice, change a kind, touch a secret, or undo what a
//! template higher up the `extends` chain narrowed.

use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use weft_core::{AnswerKind, Question};

use crate::manifest::RefineDecl;

/// Apply `template`'s `[refine]` tables to its merged question list, whose
/// first `inherited` questions came through `extends`.
pub fn apply(
    template: &str,
    questions: &mut [Question],
    inherited: usize,
    refine: &BTreeMap<String, RefineDecl>,
) -> Result<()> {
    for (id, decl) in refine {
        let Some(pos) = questions.iter().position(|q| q.id.0 == *id) else {
            if id.contains('.') {
                bail!(
                    "refine `{id}`: only questions inherited through `extends` can be \
                     refined, not questions of an include"
                );
            }
            bail!("refine `{id}`: template `{template}` inherits no question `{id}`");
        };
        if pos >= inherited {
            bail!(
                "refine `{id}`: `{id}` is declared by template `{template}` itself; change \
                 its `[[question]]` instead"
            );
        }
        refine_one(template, &mut questions[pos], decl)?;
    }
    Ok(())
}

fn refine_one(template: &str, q: &mut Question, decl: &RefineDecl) -> Result<()> {
    let id = q.id.0.clone();
    let narrows = decl.choices.is_some() || !decl.blocked.is_empty() || !decl.fixed.is_empty();
    let sets_value = decl.default.is_some() || decl.lock.is_some() || narrows;
    let rewords = decl.prompt.is_some() || decl.description.is_some() || decl.example.is_some();
    if !sets_value && !rewords {
        bail!("refine `{id}`: the table sets nothing");
    }
    if matches!(q.kind, AnswerKind::Secret { .. }) {
        bail!("refine `{id}`: secret questions cannot be refined");
    }
    if decl.lock.is_some() && (decl.default.is_some() || narrows) {
        bail!(
            "refine `{id}`: `lock` sets the value on its own; drop `default`, `choices`, \
             `blocked` and `fixed`"
        );
    }
    if decl.choices.is_some() && !decl.blocked.is_empty() {
        bail!(
            "refine `{id}`: use `choices` (the choices to keep) or `blocked` (the choices \
             to remove), not both"
        );
    }
    if q.narrowing.locked && sets_value {
        bail!(
            "refine `{id}`: `{id}` is locked by {}; only `prompt`, `description` and \
             `example` can change",
            q.narrowing.refiners()
        );
    }
    let kind = q.kind.name();
    if (decl.choices.is_some() || !decl.blocked.is_empty()) && q.kind.choices().is_none() {
        bail!(
            "refine `{id}`: `choices` and `blocked` apply to choice and multichoice \
             questions, and `{id}` is {kind}"
        );
    }
    if !decl.fixed.is_empty() && !matches!(q.kind, AnswerKind::MultiChoice { .. }) {
        bail!("refine `{id}`: `fixed` applies to multichoice questions, and `{id}` is {kind}");
    }
    if narrows {
        narrow_domain(q, decl)?;
    }
    if let Some(prompt) = &decl.prompt {
        q.prompt = Some(prompt.clone());
    }
    if let Some(description) = &decl.description {
        q.description = Some(description.clone());
    }
    if let Some(example) = &decl.example {
        q.example = Some(example.clone());
    }
    if let Some(default) = &decl.default {
        q.default = Some(default.clone());
    }
    if let Some(lock) = &decl.lock {
        q.default = Some(lock.clone());
        q.narrowing.locked = true;
    }
    if q.narrowing.by.last().map(String::as_str) != Some(template) {
        q.narrowing.by.push(template.to_owned());
    }
    Ok(())
}

/// Take what `decl` blocks (or does not keep) out of the question's choices
/// and add its fixed choices. Choices stay in declared order.
fn narrow_domain(q: &mut Question, decl: &RefineDecl) -> Result<()> {
    let id = &q.id;
    let domain: Vec<String> = q.kind.choices().map(<[String]>::to_vec).unwrap_or_default();
    let already_blocked = |c: &String| q.narrowing.blocked.contains(c);
    let not_offered = |c: &String| {
        if already_blocked(c) {
            anyhow!(
                "refine `{id}`: `{c}` is already blocked by {}; a refinement cannot bring \
                 it back",
                q.narrowing.refiners()
            )
        } else {
            anyhow!(
                "refine `{id}`: `{c}` is not a choice of `{id}` (choices: {})",
                domain.join(", ")
            )
        }
    };
    let removed: Vec<String> = match &decl.choices {
        Some(keep) => {
            if let Some(c) = keep.iter().find(|c| !domain.contains(c)) {
                return Err(not_offered(c));
            }
            domain
                .iter()
                .filter(|c| !keep.contains(c))
                .cloned()
                .collect()
        }
        None => {
            // Blocking a choice that is already blocked changes nothing.
            if let Some(c) = decl
                .blocked
                .iter()
                .find(|c| !domain.contains(c) && !already_blocked(c))
            {
                return Err(not_offered(c));
            }
            domain
                .iter()
                .filter(|c| decl.blocked.contains(c))
                .cloned()
                .collect()
        }
    };
    if let Some(c) = removed.iter().find(|c| q.narrowing.fixed.contains(c)) {
        bail!(
            "refine `{id}`: `{c}` is fixed by {}; it cannot be blocked",
            q.narrowing.refiners()
        );
    }
    for c in &decl.fixed {
        if removed.contains(c) {
            bail!("refine `{id}`: `{c}` cannot be both fixed and blocked");
        }
        if !domain.contains(c) {
            return Err(not_offered(c));
        }
    }
    let kept: Vec<String> = domain
        .iter()
        .filter(|c| !removed.contains(c))
        .cloned()
        .collect();
    if kept.is_empty() && matches!(q.kind, AnswerKind::Choice { .. }) {
        bail!("refine `{id}`: no choice would be left to pick");
    }
    let fixed: Vec<String> = kept
        .iter()
        .filter(|c| q.narrowing.fixed.contains(c) || decl.fixed.contains(c))
        .cloned()
        .collect();
    if let AnswerKind::Choice { choices } | AnswerKind::MultiChoice { choices } = &mut q.kind {
        *choices = kept;
    }
    q.narrowing.blocked.extend(removed);
    q.narrowing.fixed = fixed;
    Ok(())
}

#[cfg(test)]
mod tests {
    use camino::{Utf8Path, Utf8PathBuf};
    use weft_core::{AnswerKind, Question};

    use crate::template::Template;

    /// A base (`skills` multichoice, `ci` choice, `use_jira` bool, a secret)
    /// and an extender `mid` on it; `refine` is `mid`'s refine tables.
    const BASE: &str = r#"
[template]
name = "base"
weft-version = "0.1"

[[question]]
id = "ci"
kind = "choice"
choices = ["github", "gitlab"]
default = "'github'"

[[question]]
id = "use_jira"
kind = "bool"
default = "False"

[[question]]
id = "skills"
kind = "multichoice"
choices = ["dbt", "sql", "airflow"]
prompt = "Skills"
default = "[]"

[[question]]
id = "token"
kind = "secret"
source = "env:TOKEN"
"#;

    fn template(dir: &Utf8Path, name: &str, manifest: &str) -> Utf8PathBuf {
        let root = dir.join(name);
        std::fs::create_dir_all(root.join("patches")).unwrap();
        std::fs::write(root.join("weft.toml"), manifest).unwrap();
        root
    }

    fn extender(name: &str, extends: &str, rest: &str) -> String {
        format!(
            "[template]\nname = \"{name}\"\nweft-version = \"0.1\"\nextends = \"{extends}\"\n{rest}"
        )
    }

    /// Load `mid` (extending the base) with `refine`, or its load error.
    fn load_mid(refine: &str) -> Result<Template, String> {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        template(root, "base", BASE);
        let mid = template(root, "mid", &extender("mid", "../base", refine));
        Template::load(&mid).map_err(|e| format!("{e:#}"))
    }

    fn question<'t>(template: &'t Template, id: &str) -> &'t Question {
        template
            .manifest
            .questions
            .iter()
            .find(|q| q.id.0 == id)
            .unwrap()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_refinement_narrows_the_inherited_question() {
        let mid = load_mid(
            "[refine.skills]\nchoices = [\"dbt\", \"sql\"]\nfixed = [\"dbt\"]\n\
             default = \"['sql']\"\nprompt = \"dbt skills\"\n\
             [refine.use_jira]\nlock = \"False\"\n",
        )
        .unwrap();
        let skills = question(&mid, "skills");
        assert_eq!(
            skills.kind,
            AnswerKind::MultiChoice {
                choices: strings(&["dbt", "sql"])
            }
        );
        assert_eq!(skills.default.as_ref().unwrap().as_str(), "['sql']");
        assert_eq!(skills.prompt.as_deref(), Some("dbt skills"));
        assert_eq!(skills.narrowing.fixed, strings(&["dbt"]));
        assert_eq!(skills.narrowing.blocked, strings(&["airflow"]));
        assert_eq!(skills.narrowing.by, strings(&["mid"]));
        let jira = question(&mid, "use_jira");
        assert!(jira.narrowing.locked);
        assert!(!jira.is_promptable());
        assert_eq!(jira.default.as_ref().unwrap().as_str(), "False");
    }

    #[test]
    fn only_inherited_questions_can_be_refined() {
        let err = load_mid("[refine.nope]\ndefault = \"1\"\n").unwrap_err();
        assert!(err.contains("inherits no question `nope`"), "{err}");
        let err = load_mid("[refine.\"web.x\"]\ndefault = \"1\"\n").unwrap_err();
        assert!(err.contains("not questions of an include"), "{err}");
        let err = load_mid(
            "[[question]]\nid = \"own\"\nkind = \"bool\"\n[refine.own]\ndefault = \"True\"\n",
        )
        .unwrap_err();
        assert!(err.contains("declared by template `mid` itself"), "{err}");
        let err = load_mid("[refine.token]\ndescription = \"x\"\n").unwrap_err();
        assert!(err.contains("secret questions cannot be refined"), "{err}");

        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let lone = template(
            root,
            "lone",
            "[template]\nname = \"lone\"\nweft-version = \"0.1\"\n[refine.ci]\nblocked = [\"gitlab\"]\n",
        );
        let err = format!("{:#}", Template::load(&lone).unwrap_err());
        assert!(err.contains("extends nothing"), "{err}");
    }

    #[test]
    fn a_refinement_cannot_widen_or_contradict_itself() {
        for (refine, expected) in [
            (
                "[refine.skills]\nchoices = [\"rust\"]\n",
                "`rust` is not a choice of `skills`",
            ),
            (
                "[refine.skills]\nblocked = [\"rust\"]\n",
                "`rust` is not a choice of `skills`",
            ),
            (
                "[refine.skills]\nfixed = [\"rust\"]\n",
                "`rust` is not a choice of `skills`",
            ),
            (
                "[refine.skills]\nfixed = [\"sql\"]\nblocked = [\"sql\"]\n",
                "cannot be both fixed and blocked",
            ),
            (
                "[refine.skills]\nchoices = [\"sql\"]\nblocked = [\"dbt\"]\n",
                "not both",
            ),
            (
                "[refine.use_jira]\nlock = \"False\"\ndefault = \"True\"\n",
                "`lock` sets the value on its own",
            ),
            (
                "[refine.use_jira]\nfixed = [\"x\"]\n",
                "`fixed` applies to multichoice",
            ),
            (
                "[refine.use_jira]\nblocked = [\"x\"]\n",
                "apply to choice and multichoice",
            ),
            ("[refine.ci]\nchoices = []\n", "no choice would be left"),
            ("[refine.ci]\n", "the table sets nothing"),
            ("[refine.ci]\nhidden = true\n", "unknown field `hidden`"),
        ] {
            let err = load_mid(refine).unwrap_err();
            assert!(err.contains(expected), "{refine}: {err}");
        }
    }

    #[test]
    fn a_chain_keeps_narrowing_and_never_widens() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        template(root, "base", BASE);
        template(
            root,
            "mid",
            &extender(
                "mid",
                "../base",
                "[refine.skills]\nblocked = [\"airflow\"]\nfixed = [\"dbt\"]\n\
                 [refine.use_jira]\nlock = \"False\"\n",
            ),
        );
        let leaf = |refine: &str| {
            let leaf = template(root, "leaf", &extender("leaf", "../mid", refine));
            Template::load(&leaf).map_err(|e| format!("{e:#}"))
        };

        let narrowed = leaf("[refine.skills]\nblocked = [\"sql\", \"airflow\"]\n").unwrap();
        let skills = question(&narrowed, "skills");
        assert_eq!(
            skills.kind,
            AnswerKind::MultiChoice {
                choices: strings(&["dbt"])
            }
        );
        assert_eq!(skills.narrowing.blocked, strings(&["airflow", "sql"]));
        assert_eq!(skills.narrowing.by, strings(&["mid", "leaf"]));
        assert!(skills.nothing_to_choose());

        let err = leaf("[refine.skills]\nchoices = [\"airflow\"]\n").unwrap_err();
        assert!(err.contains("already blocked by template `mid`"), "{err}");
        let err = leaf("[refine.skills]\nblocked = [\"dbt\"]\n").unwrap_err();
        assert!(err.contains("`dbt` is fixed by template `mid`"), "{err}");
        let err = leaf("[refine.use_jira]\nlock = \"True\"\n").unwrap_err();
        assert!(err.contains("locked by template `mid`"), "{err}");
        let reworded = leaf("[refine.use_jira]\nprompt = \"Jira?\"\n").unwrap();
        assert_eq!(
            question(&reworded, "use_jira").prompt.as_deref(),
            Some("Jira?")
        );
    }
}
