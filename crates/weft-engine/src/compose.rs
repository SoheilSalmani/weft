//! Composition: resolving and rendering a template together with its
//! included child templates.
//!
//! A parent template declares `[[include]]`s — child templates mounted at a
//! path prefix. Children are self-contained; the parent seeds their answers
//! via `bind` expressions and per-instance provided answers (namespaced as
//! `<include>.<id>` on the command line). Rendering composes one tree:
//! parent patches at the root, each instance's child render under its mount.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::render::ExprEval;
use weft_core::{AnswerId, AnswerSet, Patch, Question, Tree, Value};

use crate::answers;
use crate::hooks::ChangeSet;
use crate::interact::Interaction;
use crate::template::{LoadedInclude, Template};

/// One include instance with fully resolved child answers.
#[derive(Debug, Clone)]
pub struct ResolvedInstance {
    pub include: String,
    pub key: String,
    pub mount: Utf8PathBuf,
    pub answers: AnswerSet,
}

/// An instance paired with the child template and the (possibly pinned-base
/// filtered) patches to render it from.
pub struct ComposedPart<'t> {
    pub instance: ResolvedInstance,
    pub template: &'t Template,
    pub patches: Vec<Patch>,
}

/// Render an include's mount prefix for an instance key. `{key}` in the
/// declared path substitutes the key; the result must be tree-relative.
pub fn mount_path(decl_path: &str, key: &str) -> Result<Utf8PathBuf> {
    let rendered = decl_path.replace("{key}", key);
    let invalid = rendered.is_empty()
        || rendered.starts_with('/')
        || rendered.ends_with('/')
        || rendered
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..");
    if invalid {
        bail!("invalid include mount path {rendered:?} (must be relative, `/`-separated, no `..`)");
    }
    Ok(Utf8PathBuf::from(rendered))
}

/// Resolve a possibly-namespaced flat answer id to its declaring question:
/// a parent question, or `<include>.<child-id>` into a child's question.
pub fn find_question<'t>(template: &'t Template, flat_id: &str) -> Option<&'t Question> {
    if let Some(q) = template
        .manifest
        .questions
        .iter()
        .find(|q| q.id.0 == flat_id)
    {
        return Some(q);
    }
    let (ns, rest) = flat_id.split_once('.')?;
    let inc = template.include(ns)?;
    inc.template
        .manifest
        .questions
        .iter()
        .find(|q| q.id.0 == rest)
}

/// Split a flat provided answer set into (parent answers, per-include child
/// answer sets keyed by include name).
pub fn split_provided(
    template: &Template,
    flat: &AnswerSet,
) -> Result<(AnswerSet, BTreeMap<String, AnswerSet>)> {
    let mut parent = AnswerSet::new();
    let mut children: BTreeMap<String, AnswerSet> = BTreeMap::new();
    for (id, value) in flat.iter() {
        if template.manifest.questions.iter().any(|q| q.id == *id) {
            parent.insert(id.clone(), value.clone());
            continue;
        }
        if let Some((ns, rest)) = id.0.split_once('.') {
            if let Some(inc) = template.include(ns) {
                if inc
                    .template
                    .manifest
                    .questions
                    .iter()
                    .any(|q| q.id.0 == rest)
                {
                    children
                        .entry(inc.decl.name.clone())
                        .or_default()
                        .insert(AnswerId(rest.to_owned()), value.clone());
                    continue;
                }
            }
        }
        bail!(
            "answer `{id}` does not match any question in template `{}` or its includes",
            template.manifest.template.name
        );
    }
    Ok((parent, children))
}

/// Resolve every include's instance answers: `bind` expressions evaluated
/// over the parent's resolved answers (plus `key`), overlaid by explicitly
/// provided child answers, then gathered like any template (child defaults,
/// secret resolution, prompting).
pub fn resolve_instances(
    template: &Template,
    parent_resolved: &AnswerSet,
    child_provided: &BTreeMap<String, AnswerSet>,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Vec<ResolvedInstance>> {
    let mut out = Vec::new();
    for inc in &template.includes {
        if inc.decl.repeat {
            bail!(
                "include `{}` is repeatable; repeatable instances are not supported yet",
                inc.decl.name
            );
        }
        let key = inc.decl.name.clone();
        let provided = child_provided
            .get(&inc.decl.name)
            .cloned()
            .unwrap_or_default();
        let answers =
            resolve_instance_answers(inc, &key, parent_resolved, &provided, eval, interaction)?;
        out.push(ResolvedInstance {
            include: inc.decl.name.clone(),
            key: key.clone(),
            mount: mount_path(&inc.decl.path, &key)?,
            answers,
        });
    }
    Ok(out)
}

/// Resolve one instance's child answers (see [`resolve_instances`]).
pub fn resolve_instance_answers(
    inc: &LoadedInclude,
    key: &str,
    parent_resolved: &AnswerSet,
    provided: &AnswerSet,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    // Bind scope: parent answers plus the instance key.
    let mut scope = parent_resolved.clone();
    scope.insert(AnswerId::from("key"), Value::String(key.to_owned()));

    let mut seed = AnswerSet::new();
    for (child_id, expr) in &inc.decl.bind {
        let value = eval.eval(expr, &scope).with_context(|| {
            format!(
                "evaluating bind `{child_id}` of include `{}`",
                inc.decl.name
            )
        })?;
        seed.insert(AnswerId(child_id.clone()), value);
    }
    // Explicit per-instance answers win over binds.
    seed.overlay(provided);

    answers::gather(&inc.template, &seed, &AnswerSet::new(), eval, interaction).with_context(|| {
        format!(
            "resolving answers for include `{}` (instance `{key}`); pass child answers as \
                 --answer {}.{{id}}=...",
            inc.decl.name, inc.decl.name
        )
    })
}

/// Build parts from instances using each child template's *full* patch set
/// (the `weft new` case; `update` filters to pinned bases instead).
pub fn full_parts<'t>(
    template: &'t Template,
    instances: Vec<ResolvedInstance>,
) -> Result<Vec<ComposedPart<'t>>> {
    instances
        .into_iter()
        .map(|instance| {
            let inc = template
                .include(&instance.include)
                .with_context(|| format!("unknown include `{}`", instance.include))?;
            Ok(ComposedPart {
                template: &inc.template,
                patches: inc.template.patches.clone(),
                instance,
            })
        })
        .collect()
}

/// Render the composed tree: parent patches at the root, each part's child
/// render mounted under its instance prefix. Path collisions are errors.
pub fn render_composed(
    parent_patches: &[Patch],
    parent_answers: &AnswerSet,
    parts: &[ComposedPart<'_>],
    eval: &dyn ExprEval,
) -> Result<Tree> {
    let mut tree = weft_core::render::render(parent_patches, parent_answers, eval)
        .context("rendering parent template")?;
    for part in parts {
        let child = weft_core::render::render(&part.patches, &part.instance.answers, eval)
            .with_context(|| {
                format!(
                    "rendering include `{}` (instance `{}`)",
                    part.instance.include, part.instance.key
                )
            })?;
        for (path, entry) in child.iter() {
            let mounted = part.instance.mount.join(path);
            if tree.get(&mounted).is_some() {
                bail!(
                    "include `{}` (instance `{}`): mounted path `{mounted}` collides with an \
                     existing file",
                    part.instance.include,
                    part.instance.key
                );
            }
            tree.insert(mounted, entry.clone());
        }
    }
    Ok(tree)
}

/// The child-relative view of a change set: paths under `mount` with the
/// prefix stripped, plus the child answer ids that changed. Drives post-hook
/// re-fire for one instance on `weft update`.
pub fn child_changes(
    changes: &ChangeSet,
    mount: &Utf8Path,
    changed_answers: std::collections::BTreeSet<AnswerId>,
) -> ChangeSet {
    let paths = changes
        .paths
        .iter()
        .filter_map(|p| p.strip_prefix(mount).ok().map(|s| s.to_owned()))
        .collect();
    ChangeSet {
        paths,
        answers: changed_answers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_path_substitutes_key_and_validates() {
        assert_eq!(
            mount_path("connectors/{key}", "github").unwrap(),
            Utf8PathBuf::from("connectors/github")
        );
        assert_eq!(
            mount_path("apps/api", "api").unwrap(),
            Utf8PathBuf::from("apps/api")
        );
        assert!(mount_path("", "x").is_err());
        assert!(mount_path("/abs", "x").is_err());
        assert!(mount_path("a/../b", "x").is_err());
        assert!(mount_path("a/{key}", "..").is_err());
    }
}
