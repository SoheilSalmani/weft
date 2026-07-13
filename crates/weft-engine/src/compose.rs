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

/// Is `key` a legal instance key? (slug: lowercase alphanumerics, `-`, `_`)
pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Resolve a possibly-namespaced flat answer id to its declaring question:
/// a parent question, `<include>.<child-id>` (non-repeat), or
/// `<include>.<key>.<child-id>` (repeat).
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
    let child_id = if inc.decl.repeat {
        let (key, child_id) = rest.split_once('.')?;
        if !valid_key(key) {
            return None;
        }
        child_id
    } else {
        rest
    };
    inc.template
        .manifest
        .questions
        .iter()
        .find(|q| q.id.0 == child_id)
}

/// Per-instance child answer sets, keyed by `(include name, instance key)`.
pub type ChildProvided = BTreeMap<(String, String), AnswerSet>;

/// Split a flat provided answer set into (parent answers, per-instance child
/// answer sets keyed by `(include name, instance key)`). For non-repeat
/// includes the key is the include name; for repeat includes, providing any
/// `<include>.<key>.<id>` answer implicitly declares the instance.
pub fn split_provided(template: &Template, flat: &AnswerSet) -> Result<(AnswerSet, ChildProvided)> {
    let mut parent = AnswerSet::new();
    let mut children: BTreeMap<(String, String), AnswerSet> = BTreeMap::new();
    for (id, value) in flat.iter() {
        if template.manifest.questions.iter().any(|q| q.id == *id) {
            parent.insert(id.clone(), value.clone());
            continue;
        }
        let routed = (|| {
            let (ns, rest) = id.0.split_once('.')?;
            let inc = template.include(ns)?;
            let (key, child_id) = if inc.decl.repeat {
                let (key, child_id) = rest.split_once('.')?;
                if !valid_key(key) {
                    return None;
                }
                (key, child_id)
            } else {
                (inc.decl.name.as_str(), rest)
            };
            inc.template
                .manifest
                .questions
                .iter()
                .any(|q| q.id.0 == child_id)
                .then(|| {
                    (
                        (inc.decl.name.clone(), key.to_owned()),
                        AnswerId(child_id.to_owned()),
                    )
                })
        })();
        match routed {
            Some((slot, child_id)) => {
                children
                    .entry(slot)
                    .or_default()
                    .insert(child_id, value.clone());
            }
            None => bail!(
                "answer `{id}` does not match any question in template `{}` or its includes",
                template.manifest.template.name
            ),
        }
    }
    Ok((parent, children))
}

/// Resolve every instance's child answers: `bind` expressions evaluated over
/// the parent's resolved answers (plus `key`), overlaid by explicitly
/// provided child answers, then gathered like any template (child defaults,
/// secret resolution, prompting).
///
/// Instances: a non-repeat include always has exactly one (key = include
/// name). A repeat include has one per declared key — the union of
/// `declared` entries and keys appearing in `child_provided`. Zero declared
/// instances of a repeat include is valid.
pub fn resolve_instances(
    template: &Template,
    parent_resolved: &AnswerSet,
    child_provided: &ChildProvided,
    declared: &std::collections::BTreeSet<(String, String)>,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Vec<ResolvedInstance>> {
    // Validate declared slots up front.
    for (include, key) in declared {
        let inc = template
            .include(include)
            .with_context(|| format!("--instance {include}={key}: unknown include `{include}`"))?;
        if !inc.decl.repeat {
            bail!("include `{include}` is not repeatable; it always has exactly one instance");
        }
        if !valid_key(key) {
            bail!(
                "invalid instance key {key:?} for include `{include}` \
                 (lowercase alphanumerics, `-`, `_`)"
            );
        }
    }

    let mut out = Vec::new();
    for inc in &template.includes {
        let keys: Vec<String> = if inc.decl.repeat {
            // Union of explicit declarations and keys implied by provided
            // answers, in sorted (deterministic) order.
            declared
                .iter()
                .filter(|(i, _)| *i == inc.decl.name)
                .map(|(_, k)| k.clone())
                .chain(
                    child_provided
                        .keys()
                        .filter(|(i, _)| *i == inc.decl.name)
                        .map(|(_, k)| k.clone()),
                )
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            vec![inc.decl.name.clone()]
        };
        for key in keys {
            let provided = child_provided
                .get(&(inc.decl.name.clone(), key.clone()))
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
/// render mounted under its instance prefix, then `foreach` integration
/// patches applied once per matching instance. Path collisions are errors.
pub fn render_composed(
    parent_patches: &[Patch],
    parent_answers: &AnswerSet,
    parts: &[ComposedPart<'_>],
    eval: &dyn ExprEval,
) -> Result<Tree> {
    // Plain render skips foreach patches (they only apply here, per
    // instance, after the children are mounted).
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

    // Foreach integration patches: graph leaves, applied deterministically —
    // patches in id order, instances in (include, key) order. Scope = parent
    // answers + `key` + the instance's child answers flattened under
    // `instance.<id>`.
    let mut foreach_patches: Vec<&Patch> = parent_patches
        .iter()
        .filter(|p| p.foreach.is_some())
        .collect();
    foreach_patches.sort_by_key(|p| p.id);
    for patch in foreach_patches {
        let include = patch.foreach.as_deref().expect("filtered");
        // A part with an empty patch set is an instance that doesn't exist on
        // this side yet (`weft instance add` pins base = [] before the update
        // realizes it) — it contributed nothing, including integration lines.
        for part in parts
            .iter()
            .filter(|p| p.instance.include == include && !p.patches.is_empty())
        {
            let scope = foreach_scope(parent_answers, &part.instance);
            if let Some(when) = &patch.when {
                let on = eval.eval_bool(when, &scope).with_context(|| {
                    format!(
                        "evaluating when of foreach patch {} (instance `{}`)",
                        patch.id.short(),
                        part.instance.key
                    )
                })?;
                if !on {
                    continue;
                }
            }
            weft_core::render::apply_ops(&mut tree, patch, &scope, eval).with_context(|| {
                format!(
                    "applying foreach patch {} for instance `{}` of include `{include}`",
                    patch.id.short(),
                    part.instance.key
                )
            })?;
        }
    }
    Ok(tree)
}

/// The answer scope a foreach patch sees for one instance: the parent's
/// answers, plus `key`, plus the instance's child answers as
/// `instance_<id>` — an underscore (not a dot) so the same name works both
/// in `{"answer": …}` segments and as a Starlark identifier in expressions.
fn foreach_scope(parent_answers: &AnswerSet, instance: &ResolvedInstance) -> AnswerSet {
    let mut scope = parent_answers.clone();
    scope.insert(AnswerId::from("key"), Value::String(instance.key.clone()));
    for (id, value) in instance.answers.iter() {
        scope.insert(AnswerId(format!("instance_{id}")), value.clone());
    }
    scope
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
