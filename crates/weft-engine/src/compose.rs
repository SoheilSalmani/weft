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
use weft_core::{AnswerId, AnswerSet, Patch, PatchId, Question, Tree, Value};

use crate::answers;
use crate::interact::Interaction;
use crate::template::{LoadedInclude, Template};

/// One include instance with fully resolved child answers.
#[derive(Debug, Clone)]
pub struct ResolvedInstance {
    pub include: String,
    pub key: String,
    pub mount: Utf8PathBuf,
    pub answers: AnswerSet,
    /// Instance of a `repeat` include (project-time; its nodes are opaque
    /// to the parent graph) rather than the single instance of a plain one.
    pub repeat: bool,
}

/// An instance paired with the child template and the (possibly pinned-base
/// filtered) patches to render it from. `children` are the child template's
/// own includes, resolved recursively (nested composition).
#[derive(Clone)]
pub struct ComposedPart<'t> {
    pub instance: ResolvedInstance,
    pub template: &'t Template,
    pub patches: Vec<Patch>,
    pub children: Vec<ComposedPart<'t>>,
}

/// Render an include's mount prefix for an instance key. `{key}` in the
/// declared path substitutes the key; the result must be tree-relative. An
/// empty path (or `.`) mounts at the root.
pub fn mount_path(decl_path: &str, key: &str) -> Result<Utf8PathBuf> {
    let rendered = decl_path.replace("{key}", key);
    if rendered.is_empty() || rendered == "." {
        return Ok(Utf8PathBuf::new());
    }
    let invalid = rendered.starts_with('/')
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
            let answers = resolve_instance_answers(
                inc,
                &key,
                parent_resolved,
                &provided,
                &AnswerSet::new(),
                eval,
                interaction,
            )?;
            out.push(ResolvedInstance {
                include: inc.decl.name.clone(),
                key: key.clone(),
                mount: mount_path(&inc.decl.path, &key)?,
                answers,
                repeat: inc.decl.repeat,
            });
        }
    }
    Ok(out)
}

/// Resolve one instance's child answers (see [`resolve_instances`]).
/// `presolved` is passed through to the child's `gather` — pre-resolved
/// child secrets (stored refs on update, placeholders for previews).
pub fn resolve_instance_answers(
    inc: &LoadedInclude,
    key: &str,
    parent_resolved: &AnswerSet,
    provided: &AnswerSet,
    presolved: &AnswerSet,
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

    answers::gather(&inc.template, &seed, presolved, eval, interaction).with_context(|| {
        format!(
            "resolving answers for include `{}` (instance `{key}`); pass child answers as \
                 --answer {}.{{id}}=...",
            inc.decl.name, inc.decl.name
        )
    })
}

/// How child secrets are pre-resolved when building nested parts.
#[derive(Clone, Copy)]
pub enum SecretMode {
    /// Resolve through the declared sources (env/cmd/prompt) — the CLI path.
    Resolve,
    /// Substitute `<secret:id>` placeholders — the preview path (servers,
    /// `weft graph`). Real secrets are never resolved.
    Placeholders,
}

fn presolved_for(template: &Template, mode: SecretMode) -> AnswerSet {
    match mode {
        SecretMode::Resolve => AnswerSet::new(),
        SecretMode::Placeholders => answers::placeholder_secrets(&template.manifest.questions),
    }
}

/// Recursively resolve a template's *own* includes into nested parts.
/// Nested levels take no explicit per-instance answers: they resolve from
/// binds (over the enclosing child's answers) and the grandchild's own
/// defaults/secrets. Repeat includes at nested levels have zero instances
/// (nested instance state is not supported yet).
pub fn resolve_child_parts<'t>(
    template: &'t Template,
    answers: &AnswerSet,
    mode: SecretMode,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Vec<ComposedPart<'t>>> {
    let mut parts = Vec::new();
    for inc in &template.includes {
        if inc.decl.repeat {
            continue;
        }
        let key = inc.decl.name.clone();
        let child_answers = resolve_instance_answers(
            inc,
            &key,
            answers,
            &AnswerSet::new(),
            &presolved_for(&inc.template, mode),
            eval,
            interaction,
        )?;
        let children = resolve_child_parts(&inc.template, &child_answers, mode, eval, interaction)?;
        parts.push(ComposedPart {
            instance: ResolvedInstance {
                include: inc.decl.name.clone(),
                key: key.clone(),
                mount: mount_path(&inc.decl.path, &key)?,
                answers: child_answers,
                repeat: false,
            },
            template: &inc.template,
            patches: inc.template.patches.clone(),
            children,
        });
    }
    Ok(parts)
}

/// Build parts from instances using each child template's *full* patch set
/// (the `weft new` case; `update` filters to pinned bases instead). Nested
/// includes of each child are resolved recursively.
pub fn full_parts<'t>(
    template: &'t Template,
    instances: Vec<ResolvedInstance>,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Vec<ComposedPart<'t>>> {
    instances
        .into_iter()
        .map(|instance| {
            let inc = template
                .include(&instance.include)
                .with_context(|| format!("unknown include `{}`", instance.include))?;
            let children = resolve_child_parts(
                &inc.template,
                &instance.answers,
                SecretMode::Resolve,
                eval,
                interaction,
            )?;
            Ok(ComposedPart {
                template: &inc.template,
                patches: inc.template.patches.clone(),
                instance,
                children,
            })
        })
        .collect()
}

/// The parts of a base state: every single (non-repeat) include resolved
/// with its full patch set — binds over the parent answers, `provided`
/// answers per include winning over them, `presolved` child secrets passed
/// through — nested includes re-derived from binds/defaults. Repeat includes
/// have no instances here (a session mounts one sample via `--foreach`).
/// Callers project a pinned node set onto the result with [`filter_parts`].
pub fn single_parts<'t>(
    template: &'t Template,
    parent_answers: &AnswerSet,
    provided: &BTreeMap<String, AnswerSet>,
    presolved: &BTreeMap<String, AnswerSet>,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Vec<ComposedPart<'t>>> {
    let empty = AnswerSet::new();
    let mut parts = Vec::new();
    for inc in template.includes.iter().filter(|i| !i.decl.repeat) {
        let name = inc.decl.name.clone();
        let answers = resolve_instance_answers(
            inc,
            &name,
            parent_answers,
            provided.get(&name).unwrap_or(&empty),
            presolved.get(&name).unwrap_or(&empty),
            eval,
            interaction,
        )?;
        let children = resolve_child_parts(
            &inc.template,
            &answers,
            SecretMode::Resolve,
            eval,
            interaction,
        )?;
        parts.push(ComposedPart {
            instance: ResolvedInstance {
                include: name.clone(),
                key: name,
                mount: mount_path(&inc.decl.path, &inc.decl.name)?,
                answers,
                repeat: false,
            },
            template: &inc.template,
            patches: inc.template.patches.clone(),
            children,
        });
    }
    Ok(parts)
}

/// Preview-oriented parts for a whole template: every include (single
/// instances; repeat instances only where namespaced answers imply them),
/// child secrets as placeholders, never prompting. Used by servers/UIs.
pub fn preview_parts<'t>(
    template: &'t Template,
    parent_resolved: &AnswerSet,
    child_provided: &ChildProvided,
    declared: &std::collections::BTreeSet<(String, String)>,
    eval: &dyn ExprEval,
) -> Result<Vec<ComposedPart<'t>>> {
    let mut non_interactive = crate::interact::NonInteractive;
    let mut parts = Vec::new();
    for inc in &template.includes {
        let keys: Vec<String> = if inc.decl.repeat {
            // Union of explicit declarations and keys implied by provided
            // answers, sorted (deterministic).
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
            if !valid_key(&key) {
                bail!(
                    "invalid instance key {key:?} for include `{}`",
                    inc.decl.name
                );
            }
            let provided = child_provided
                .get(&(inc.decl.name.clone(), key.clone()))
                .cloned()
                .unwrap_or_default();
            let answers = resolve_instance_answers(
                inc,
                &key,
                parent_resolved,
                &provided,
                &presolved_for(&inc.template, SecretMode::Placeholders),
                eval,
                &mut non_interactive,
            )?;
            let children = resolve_child_parts(
                &inc.template,
                &answers,
                SecretMode::Placeholders,
                eval,
                &mut non_interactive,
            )?;
            parts.push(ComposedPart {
                instance: ResolvedInstance {
                    include: inc.decl.name.clone(),
                    key: key.clone(),
                    mount: mount_path(&inc.decl.path, &key)?,
                    answers,
                    repeat: inc.decl.repeat,
                },
                template: &inc.template,
                patches: inc.template.patches.clone(),
                children,
            });
        }
    }
    Ok(parts)
}

/// One node of a composed render: a patch in the frame it renders in,
/// identified by its id in the composed graph (keyed through the include
/// chain, so the same child patch mounted twice is two nodes).
pub struct GraphNode<'a> {
    pub id: PatchId,
    pub depends_on: Vec<PatchId>,
    pub patch: &'a Patch,
    pub answers: &'a AnswerSet,
    pub mount: Utf8PathBuf,
    /// The instance chain from the root: `(include, key)` per level; empty
    /// for root-frame patches.
    pub frame: Vec<(String, String)>,
}

/// The scope a part's nodes are keyed under: the include name for a single
/// include (what static node ids use), `<include>:<key>` for a repeat
/// instance (unique per instance, never referenced by name).
pub fn part_scope(instance: &ResolvedInstance) -> String {
    if instance.repeat {
        format!("{}:{}", instance.include, instance.key)
    } else {
        instance.include.clone()
    }
}

fn keyed_through(scopes: &[String], id: PatchId) -> PatchId {
    scopes
        .iter()
        .rev()
        .fold(id, |acc, scope| PatchId::keyed(scope, acc))
}

/// Flatten a composed render into its graph nodes: this level's patches in
/// the root frame, then every part's patches (recursively) in the part's
/// frame, keyed through the include chain.
pub fn graph_nodes<'a>(
    parent_patches: &'a [Patch],
    parent_answers: &'a AnswerSet,
    parts: &'a [ComposedPart<'a>],
) -> Vec<GraphNode<'a>> {
    let mut out = Vec::new();
    collect_nodes(
        &mut out,
        &[],
        &[],
        parent_patches,
        parent_answers,
        Utf8Path::new(""),
        parts,
    );
    out
}

fn collect_nodes<'a>(
    out: &mut Vec<GraphNode<'a>>,
    scopes: &[String],
    frame: &[(String, String)],
    patches: &'a [Patch],
    answers: &'a AnswerSet,
    mount: &Utf8Path,
    parts: &'a [ComposedPart<'a>],
) {
    for patch in patches {
        out.push(GraphNode {
            id: keyed_through(scopes, patch.id),
            depends_on: patch
                .depends_on
                .iter()
                .map(|d| keyed_through(scopes, *d))
                .collect(),
            patch,
            answers,
            mount: mount.to_owned(),
            frame: frame.to_vec(),
        });
    }
    for part in parts {
        let mut scopes = scopes.to_vec();
        scopes.push(part_scope(&part.instance));
        let mut frame = frame.to_vec();
        frame.push((part.instance.include.clone(), part.instance.key.clone()));
        collect_nodes(
            out,
            &scopes,
            &frame,
            &part.patches,
            &part.instance.answers,
            &mount.join(&part.instance.mount),
            &part.children,
        );
    }
}

fn framed<'n, 'a>(nodes: &'n [GraphNode<'a>]) -> Vec<weft_core::render::Framed<'n>> {
    nodes
        .iter()
        .map(|n| weft_core::render::Framed {
            id: n.id,
            depends_on: &n.depends_on,
            patch: n.patch,
            answers: n.answers,
            mount: &n.mount,
        })
        .collect()
}

/// Render the composed tree: one topological pass over the combined graph
/// (parent and child patches interleaved by dependency, each applied in its
/// frame; a gated-off node skips its dependents across frames), then the
/// `foreach` integration patches of every level, deepest first, once per
/// matching instance. Two nodes creating the same path is a render error.
pub fn render_composed(
    parent_patches: &[Patch],
    parent_answers: &AnswerSet,
    parts: &[ComposedPart<'_>],
    eval: &dyn ExprEval,
) -> Result<Tree> {
    let nodes = graph_nodes(parent_patches, parent_answers, parts);
    let framed = framed(&nodes);
    let order = weft_core::render::framed_order(&framed).context("ordering the composed graph")?;
    let (mut tree, skipped) =
        weft_core::render::render_framed(&order, eval).context("rendering the composed graph")?;
    apply_foreach(
        &mut tree,
        &skipped,
        &[],
        parent_patches,
        parent_answers,
        Utf8Path::new(""),
        parts,
        eval,
    )?;
    Ok(tree)
}

/// Which nodes of a composed render are active (gate open, no skipped
/// dependency), without rendering. Foreach patches are never in this set.
pub fn active_nodes(
    parent_patches: &[Patch],
    parent_answers: &AnswerSet,
    parts: &[ComposedPart<'_>],
    eval: &dyn ExprEval,
) -> Result<std::collections::BTreeSet<PatchId>> {
    let nodes = graph_nodes(parent_patches, parent_answers, parts);
    let framed = framed(&nodes);
    let order = weft_core::render::framed_order(&framed).context("ordering the composed graph")?;
    let skipped = weft_core::render::framed_skipped(&order, eval)?;
    Ok(nodes
        .iter()
        .map(|n| n.id)
        .filter(|id| !skipped.contains(id))
        .collect())
}

/// Foreach integration patches of one level: deeper levels first (a child's
/// integration lines exist before the parent's), then this level's, in id
/// order, once per instance in (include, key) order. A foreach patch with a
/// skipped dependency is off, like any other patch.
#[allow(clippy::too_many_arguments)]
fn apply_foreach(
    tree: &mut Tree,
    skipped: &std::collections::BTreeSet<PatchId>,
    scopes: &[String],
    patches: &[Patch],
    answers: &AnswerSet,
    mount: &Utf8Path,
    parts: &[ComposedPart<'_>],
    eval: &dyn ExprEval,
) -> Result<()> {
    for part in parts {
        let mut scopes = scopes.to_vec();
        scopes.push(part_scope(&part.instance));
        apply_foreach(
            tree,
            skipped,
            &scopes,
            &part.patches,
            &part.instance.answers,
            &mount.join(&part.instance.mount),
            &part.children,
            eval,
        )?;
    }
    let mut foreach_patches: Vec<&Patch> = patches.iter().filter(|p| p.foreach.is_some()).collect();
    foreach_patches.sort_by_key(|p| p.id);
    for patch in foreach_patches {
        if patch
            .depends_on
            .iter()
            .any(|d| skipped.contains(&keyed_through(scopes, *d)))
        {
            continue;
        }
        let include = patch.foreach.as_deref().expect("filtered");
        // A part with an empty patch set is an instance that doesn't exist on
        // this side yet (`weft instance add` pins base = [] before the update
        // realizes it) — it contributed nothing, including integration lines.
        for part in parts
            .iter()
            .filter(|p| p.instance.include == include && !p.patches.is_empty())
        {
            let scope = foreach_scope(answers, &part.instance);
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
            weft_core::render::apply_ops_in(tree, patch, &scope, eval, mount).with_context(
                || {
                    format!(
                        "applying foreach patch {} for instance `{}` of include `{include}`",
                        patch.id.short(),
                        part.instance.key
                    )
                },
            )?;
        }
    }
    Ok(())
}

/// Restrict every single-include part's patches (recursively) to the nodes
/// in `pinned` (composed-graph ids). What a session or project pins is a set
/// of nodes; this projects it back onto the parts that render them. Repeat
/// instances are opaque and stay whole.
pub fn filter_parts(parts: &mut [ComposedPart<'_>], pinned: &std::collections::BTreeSet<PatchId>) {
    fn walk(
        parts: &mut [ComposedPart<'_>],
        scopes: &[String],
        pinned: &std::collections::BTreeSet<PatchId>,
    ) {
        for part in parts {
            if part.instance.repeat {
                continue;
            }
            let mut scopes = scopes.to_vec();
            scopes.push(part_scope(&part.instance));
            part.patches
                .retain(|p| pinned.contains(&keyed_through(&scopes, p.id)));
            walk(&mut part.children, &scopes, pinned);
        }
    }
    walk(parts, &[], pinned);
}

/// The answer scope a foreach patch sees for one instance: the parent's
/// answers, plus `key`, plus the instance's child answers as
/// `instance_<id>` — an underscore (not a dot) so the same name works both
/// in `{"answer": …}` segments and as a Starlark identifier in expressions.
/// The expression/segment scope of one foreach render: parent answers plus
/// `key` and `instance_<id>` for the instance.
pub fn foreach_scope(parent_answers: &AnswerSet, instance: &ResolvedInstance) -> AnswerSet {
    let mut scope = parent_answers.clone();
    scope.insert(AnswerId::from("key"), Value::String(instance.key.clone()));
    for (id, value) in instance.answers.iter() {
        scope.insert(AnswerId(format!("instance_{id}")), value.clone());
    }
    scope
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
        assert_eq!(mount_path("", "x").unwrap(), Utf8PathBuf::new());
        assert_eq!(mount_path(".", "x").unwrap(), Utf8PathBuf::new());
        assert!(mount_path("/abs", "x").is_err());
        assert!(mount_path("a/../b", "x").is_err());
        assert!(mount_path("a/{key}", "..").is_err());
    }
}
