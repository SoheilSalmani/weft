//! `weft check`: full template validation — manifest sanity, expression
//! parsing, patch graph structure (including the composed graph's mount
//! rule), and (when answers are resolvable) a full composed render plus a
//! commutation smoke test over independent node pairs.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use weft_core::render::{ExprEval, Framed, RenderError};
use weft_core::{
    AnswerId, AnswerKind, AnswerSet, HunkPart, LineSource, Op, Patch, PatchId, Segment,
    TemplatePath,
};
use weft_lang::StarlarkEval;

use crate::compose::{self, ComposedPart, GraphNode};
use crate::interact::NonInteractive;
use crate::template::{Node, Template};
use crate::{answers, hooks};

pub struct CheckOptions {
    pub template: Utf8PathBuf,
    /// Optional answers to enable the render + commutation checks.
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
}

#[derive(Default)]
pub struct CheckReport {
    pub issues: Vec<String>,
    pub notes: Vec<String>,
}

pub fn run(
    opts: &CheckOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
) -> Result<CheckReport> {
    let template = Template::load_with(&opts.template, resolver)?;
    let mut report = CheckReport::default();
    let issue = |report: &mut CheckReport, msg: String| report.issues.push(msg);

    let questions = &template.manifest.questions;
    let declared: BTreeSet<&AnswerId> = questions.iter().map(|q| &q.id).collect();

    // Questions
    let mut seen = BTreeSet::new();
    for q in questions {
        if !seen.insert(&q.id) {
            issue(&mut report, format!("duplicate question id `{}`", q.id));
        }
        for (what, expr) in [("default", &q.default), ("when", &q.when)] {
            if let Some(expr) = expr {
                if let Err(e) = weft_lang::parse_expr(expr.as_str()) {
                    issue(
                        &mut report,
                        format!("question `{}`: {what} does not parse: {e}", q.id),
                    );
                }
            }
        }
        if let AnswerKind::Choice { choices } | AnswerKind::MultiChoice { choices } = &q.kind {
            // Only as declared: an extender may narrow an inherited multichoice
            // to nothing, so it is never asked (a `choice` left empty is
            // already a load error).
            if choices.is_empty() && q.narrowing.blocked.is_empty() {
                issue(&mut report, format!("question `{}` has no choices", q.id));
            }
        }
        if q.computed {
            if q.default.is_none() {
                issue(
                    &mut report,
                    format!("computed question `{}` needs a `default`", q.id),
                );
            }
            if matches!(q.kind, AnswerKind::Secret { .. }) {
                issue(
                    &mut report,
                    format!("question `{}` cannot be both computed and secret", q.id),
                );
            }
        }
    }

    // Refinements: what load cannot see without evaluating (ADR-0002).
    check_refinements(&template, &mut report);

    // Presets
    let mut preset_names = BTreeSet::new();
    for decl in &template.manifest.presets {
        if !preset_names.insert(&decl.name) {
            issue(
                &mut report,
                format!("duplicate preset name `{}`", decl.name),
            );
        }
        // Full spec validation: entries name real questions of the right
        // kind, constraint choices are declared, no fixed∩blocked, no
        // secrets in presets.
        match template.preset_spec(&decl.name) {
            Err(e) => issue(&mut report, format!("preset `{}`: {e:#}", decl.name)),
            Ok(spec) => {
                if let Err(e) = spec.validate(&template) {
                    issue(&mut report, format!("preset `{}`: {e:#}", decl.name));
                }
            }
        }
    }

    // Hooks (patch-scoped): unique ids, resolvable after/inputs refs, no
    // cycle, parseable when/command expressions.
    for msg in hooks::validate_all(&template) {
        issue(&mut report, msg);
    }

    // Includes: mount paths render and don't collide (a root mount is legal,
    // and so are several — a path both create is a render error); binds
    // parse, reference real child questions, and trial-evaluate over dummy
    // parent answers; child templates pass their own checks (issues
    // prefixed).
    {
        let mut mounts: BTreeMap<String, &str> = BTreeMap::new();
        let dummy_parent = answers::dummy_answers(questions);
        for inc in &template.includes {
            // Hub includes need a version requirement; git includes pin
            // with `@rev` in the ref; path includes carry neither.
            if inc.decl.is_hub() {
                match &inc.decl.version {
                    None => issue(
                        &mut report,
                        format!(
                            "include `{}`: hub template `{}` needs a `version` requirement",
                            inc.decl.name, inc.decl.template
                        ),
                    ),
                    Some(req) => {
                        if semver::VersionReq::parse(req).is_err() {
                            issue(
                                &mut report,
                                format!(
                                    "include `{}`: `{req}` is not a valid semver requirement",
                                    inc.decl.name
                                ),
                            );
                        }
                    }
                }
            } else if inc.decl.is_git() {
                if let Err(e) = crate::source::GitRef::parse(inc.decl.template.as_str()) {
                    issue(&mut report, format!("include `{}`: {e:#}", inc.decl.name));
                }
                if inc.decl.version.is_some() {
                    issue(
                        &mut report,
                        format!(
                            "include `{}`: `version` only applies to `hub:` templates; pin a git \
                             template with `@rev` in the ref (`{}@v1.0.0`)",
                            inc.decl.name, inc.decl.template
                        ),
                    );
                }
            } else if inc.decl.version.is_some() {
                issue(
                    &mut report,
                    format!(
                        "include `{}`: `version` only applies to `hub:` templates, not path `{}`",
                        inc.decl.name, inc.decl.template
                    ),
                );
            }
            let key = if inc.decl.repeat {
                "dummykey"
            } else {
                &inc.decl.name
            };
            if inc.decl.repeat && !inc.decl.path.contains("{key}") {
                issue(
                    &mut report,
                    format!(
                        "include `{}`: repeat = true requires `{{key}}` in the mount path",
                        inc.decl.name
                    ),
                );
            }
            match crate::compose::mount_path(&inc.decl.path, key) {
                Err(e) => issue(&mut report, format!("include `{}`: {e:#}", inc.decl.name)),
                Ok(mount) if mount.as_str().is_empty() => {}
                Ok(mount) => {
                    if let Some(other) = mounts.insert(mount.to_string(), &inc.decl.name) {
                        issue(
                            &mut report,
                            format!(
                                "includes `{other}` and `{}` mount at the same path `{mount}`",
                                inc.decl.name
                            ),
                        );
                    }
                }
            }
            let mut bind_scope = dummy_parent.clone();
            bind_scope.insert(
                weft_core::AnswerId::from("key"),
                weft_core::Value::String(key.to_owned()),
            );
            for (child_id, expr) in &inc.decl.bind {
                let child_questions = &inc.template.manifest.questions;
                match child_questions.iter().find(|q| q.id.0 == *child_id) {
                    None => issue(
                        &mut report,
                        format!(
                            "include `{}`: bind `{child_id}` names no question in the child \
                             template",
                            inc.decl.name
                        ),
                    ),
                    Some(q) if q.narrowing.locked => issue(
                        &mut report,
                        format!(
                            "include `{}`: bind `{child_id}` targets a question locked by {}; \
                             a bind cannot answer it",
                            inc.decl.name,
                            q.narrowing.refiners()
                        ),
                    ),
                    Some(_) => {}
                }
                if let Err(e) = weft_lang::parse_expr(expr.as_str()) {
                    issue(
                        &mut report,
                        format!(
                            "include `{}`: bind `{child_id}` does not parse: {e}",
                            inc.decl.name
                        ),
                    );
                } else if let Err(e) = StarlarkEval.eval(expr, &bind_scope) {
                    issue(
                        &mut report,
                        format!(
                            "include `{}`: bind `{child_id}` cannot be evaluated over the \
                             parent answers: {e}",
                            inc.decl.name
                        ),
                    );
                }
            }
            // Recurse into the child's own checks (structural only; the
            // child's render checks need child answers).
            let child_report = run(
                &CheckOptions {
                    template: inc.template.root.clone(),
                    presets: vec![],
                    answers: vec![],
                    answers_file: None,
                },
                resolver,
            )?;
            for msg in child_report.issues {
                issue(&mut report, format!("include `{}`: {msg}", inc.decl.name));
            }
        }
    }

    // Extends: the base template must pass its own checks (issues prefixed).
    // Question-id and patch-name collisions with it are load errors.
    if let Some(ext) = &template.extends {
        report.notes.push(format!("extends `{}`", ext.root));
        let base_report = run(
            &CheckOptions {
                template: ext.root.clone(),
                presets: vec![],
                answers: vec![],
                answers_file: None,
            },
            resolver,
        )?;
        for msg in base_report.issues {
            issue(&mut report, format!("extends: {msg}"));
        }
    }

    // Static mount rule: a root-frame patch may only reach under an include's
    // mount when it depends on one of that include's nodes (so it renders
    // after them), and only a `foreach` patch may reach a repeat include's
    // instances. Checked on the literal prefix of each op path.
    let mounts = IncludeMounts::of(&template);
    for patch in &template.patches {
        let name = &template.id_to_name[&patch.id];
        let reaches: BTreeSet<&str> = template
            .ancestor_closure(patch.id)
            .into_iter()
            .filter_map(|id| template.node(id).and_then(Node::include))
            .chain(patch.foreach.as_deref())
            .collect();
        for path in op_paths(patch) {
            let Some(prefix) = literal_prefix(path) else {
                continue;
            };
            if let Some(inc) = mounts.single_under(&prefix) {
                if !reaches.contains(inc) {
                    issue(
                        &mut report,
                        format!(
                            "patch `{name}`: `{prefix}` is under include `{inc}`'s mount but \
                             the patch does not depend on any of its nodes (`{inc}/<patch>`); \
                             record it in a session so the dependency is inferred, or add it \
                             to depends_on"
                        ),
                    );
                }
            }
            if let Some(inc) = mounts.repeat_under(&prefix) {
                if patch.foreach.as_deref() != Some(inc) {
                    issue(
                        &mut report,
                        format!(
                            "patch `{name}`: `{prefix}` is under repeat include `{inc}`'s \
                             instances; only a `foreach: \"{inc}\"` patch may reach them"
                        ),
                    );
                }
            }
        }
    }

    // Patches: graph structure was validated at load; check expressions and
    // answer references inside segments.
    for patch in &template.patches {
        let name = &template.id_to_name[&patch.id];
        if let Some(when) = &patch.when {
            if let Err(e) = weft_lang::parse_expr(when.as_str()) {
                issue(
                    &mut report,
                    format!("patch `{name}`: when does not parse: {e}"),
                );
            }
        }
        for problem in slot_issues(patch) {
            issue(&mut report, format!("patch `{name}`: {problem}"));
        }
        // Foreach patches: the include must exist, nothing may depend on
        // them (graph leaves), and their segments may additionally reference
        // `key` and `instance_<child-question>`.
        let foreach_child: Option<&Template> = match &patch.foreach {
            None => None,
            Some(include) => match template.include(include) {
                None => {
                    issue(
                        &mut report,
                        format!("patch `{name}`: foreach names unknown include `{include}`"),
                    );
                    None
                }
                Some(inc) => Some(&inc.template),
            },
        };
        if patch.foreach.is_some() {
            for other in template.direct_dependents(patch.id) {
                issue(
                    &mut report,
                    format!(
                        "patch `{other}` depends on foreach patch `{name}`; foreach patches \
                         must be graph leaves"
                    ),
                );
            }
        }
        for id in answer_refs(patch) {
            let allowed = declared.contains(&id)
                || (patch.foreach.is_some()
                    && (id.0 == "key"
                        || id
                            .0
                            .strip_prefix("instance_")
                            .and_then(|child| {
                                foreach_child
                                    .map(|t| t.manifest.questions.iter().any(|q| q.id.0 == child))
                            })
                            .unwrap_or(false)));
            if !allowed {
                issue(
                    &mut report,
                    format!("patch `{name}` references unknown answer `{id}`"),
                );
            }
        }
        // Generator metadata: the stored command must be replayable —
        // expressions parse, answer refs are declared, keep-literal specs
        // are well-formed, stored answers name declared questions.
        if let Some(generator) = &patch.meta.generator {
            for seg in &generator.command.0 {
                match seg {
                    weft_core::Segment::Expr(e) => {
                        if let Err(err) = weft_lang::parse_expr(e.as_str()) {
                            issue(
                                &mut report,
                                format!(
                                    "patch `{name}`: generator command expression does not \
                                     parse: {err}"
                                ),
                            );
                        }
                    }
                    weft_core::Segment::Answer(id) => {
                        if !declared.contains(id) {
                            issue(
                                &mut report,
                                format!(
                                    "patch `{name}`: generator command references unknown \
                                     answer `{id}`"
                                ),
                            );
                        }
                    }
                    // Commands cannot hold slots: they never parse there.
                    weft_core::Segment::Literal(_) | weft_core::Segment::Slot(_) => {}
                }
            }
            if let Err(e) = crate::commit::parse_keep_literal(&generator.keep_literal) {
                issue(
                    &mut report,
                    format!("patch `{name}`: generator keep-literal: {e:#}"),
                );
            }
            for (id, _) in generator.answers.iter() {
                if !declared.contains(id) {
                    issue(
                        &mut report,
                        format!(
                            "patch `{name}`: generator stores an answer for unknown \
                             question `{id}`"
                        ),
                    );
                }
            }
        }
    }

    // Render + commutation checks need concrete answers.
    match resolve_check_answers(&template, opts) {
        Err(e) => report.notes.push(format!(
            "render/commutation checks skipped (answers unavailable: {e:#}); \
             pass --answer/--answers-file/--preset to enable them"
        )),
        Ok((resolved, child_provided)) => {
            let eval = StarlarkEval;
            // Every single include instance (and the repeat instances the
            // namespaced answers imply), child secrets as placeholders.
            let parts: Vec<ComposedPart> = match compose::preview_parts(
                &template,
                &resolved,
                &child_provided,
                &BTreeSet::new(),
                &eval,
            ) {
                Ok(parts) => parts,
                Err(e) => {
                    report.notes.push(format!(
                        "include instances not rendered (child answers unavailable: {e:#}); \
                         pass `<include>.<question>` answers to enable them"
                    ));
                    let root_ids: BTreeSet<PatchId> =
                        template.patches.iter().map(|p| p.id).collect();
                    if template
                        .patches
                        .iter()
                        .any(|p| p.depends_on.iter().any(|d| !root_ids.contains(d)))
                    {
                        report.notes.push(
                            "render/commutation checks skipped: root patches depend on include \
                             nodes"
                                .to_owned(),
                        );
                        return Ok(report);
                    }
                    Vec::new()
                }
            };
            // Every node on top of its own ancestors first: one that fails
            // there fails every order, so it is reported once, here, and kept
            // out of the full-render and pairwise reports below.
            let nodes = compose::graph_nodes(&template.patches, &resolved, &parts);
            let blocked = check_alone(&template, &nodes, &mut report);
            let full = compose::draft_composed(
                weft_core::Draft::new(),
                &template.patches,
                &resolved,
                &parts,
                &eval,
            )
            .and_then(|draft| {
                let tree = draft
                    .clone()
                    .finish()
                    .context("rendering the composed graph")?;
                Ok((draft, tree))
            });
            // Two patches filling one slot under one key fail every render
            // with both: collected here and from the pairs, reported once.
            let mut duplicates = BTreeSet::new();
            match full {
                Err(e) => {
                    let cause = e.chain().find_map(|c| c.downcast_ref::<RenderError>());
                    if let Some(dup) = cause.and_then(duplicate_fill) {
                        duplicates.insert(dup);
                    } else if !cause
                        .and_then(RenderError::patch)
                        .is_some_and(|id| blocked.contains(&id))
                    {
                        let text = named(&template, &nodes, &format!("{e:#}"));
                        issue(&mut report, format!("full render failed: {text}"));
                    }
                }
                Ok((mut draft, tree)) => {
                    report
                        .notes
                        .push(format!("render ok ({} files)", tree.len()));
                    // Trial-apply each foreach patch once with a dummy
                    // instance scope, so its segments/hunks are validated
                    // even when check has no real instances.
                    for patch in &template.patches {
                        let Some(include) = &patch.foreach else {
                            continue;
                        };
                        let Some(inc) = template.include(include) else {
                            continue; // reported above
                        };
                        let name = &template.id_to_name[&patch.id];
                        let mut scope = resolved.clone();
                        scope.insert(
                            AnswerId::from("key"),
                            weft_core::Value::String("dummykey".into()),
                        );
                        for (id, value) in
                            answers::dummy_answers(&inc.template.manifest.questions).iter()
                        {
                            scope.insert(AnswerId(format!("instance_{id}")), value.clone());
                        }
                        let gated_off = patch
                            .when
                            .as_ref()
                            .map(|w| !StarlarkEval.eval_bool(w, &scope).unwrap_or(true))
                            .unwrap_or(false);
                        if gated_off {
                            continue;
                        }
                        if let Err(e) =
                            draft.apply(patch.id, patch, &scope, &eval, camino::Utf8Path::new(""))
                        {
                            let e = named(&template, &nodes, &e.to_string());
                            issue(
                                &mut report,
                                format!(
                                    "foreach patch `{name}` failed a trial application with a \
                                     dummy instance: {e}"
                                ),
                            );
                        }
                    }
                    // Slots render when the draft finishes.
                    if let Err(e) = draft.finish() {
                        let e = named(&template, &nodes, &e.to_string());
                        issue(
                            &mut report,
                            format!(
                                "the foreach patches failed a trial application with a dummy \
                                 instance: {e}"
                            ),
                        );
                    }
                }
            }
            check_commutation(&template, &nodes, &blocked, &mut duplicates, &mut report);
            for (path, slot, key, first, second) in duplicates {
                let mut pair = [
                    node_name(&template, &nodes, first),
                    node_name(&template, &nodes, second),
                ];
                pair.sort();
                let [a, b] = pair;
                issue(
                    &mut report,
                    format!(
                        "patches `{a}` and `{b}` both fill slot `{slot}` of `{path}` under key \
                         `{key}`; a slot takes one contribution per key"
                    ),
                );
            }
        }
    }

    Ok(report)
}

/// This template's own `[refine]` tables, beyond the structural rules load
/// enforces. A refined default or lock keeps the position of the question it
/// refines, so it may only mention earlier questions, and it has to pick
/// among the narrowed choices; a `choice` whose inherited default the
/// refinement blocks needs a refined default too (a multichoice default
/// just drops blocked options).
fn check_refinements(template: &Template, report: &mut CheckReport) {
    let questions = &template.manifest.questions;
    for (id, decl) in &template.manifest.refine {
        // Load rejects a refinement of a question the template does not
        // inherit, so the question is always there.
        let Some(idx) = questions.iter().position(|q| q.id.0 == *id) else {
            continue;
        };
        let q = &questions[idx];
        let refined = decl.default.is_some() || decl.lock.is_some();
        if !refined && !matches!(q.kind, AnswerKind::Choice { .. }) {
            continue;
        }
        let Some(expr) = &q.default else {
            continue;
        };
        if weft_lang::parse_expr(expr.as_str()).is_err() {
            continue; // reported with the question's own default above
        }
        let what = if decl.lock.is_some() {
            "lock"
        } else {
            "default"
        };
        let earlier = answers::dummy_answers(&questions[..idx]);
        let value = match StarlarkEval.eval(expr, &earlier) {
            Ok(value) => value,
            // An inherited default that fails is the base's to report.
            Err(_) if !refined => continue,
            Err(e) => {
                let all = answers::dummy_answers(questions);
                report
                    .issues
                    .push(if StarlarkEval.eval(expr, &all).is_ok() {
                        format!(
                            "refine `{id}`: its {what} may only mention questions declared before \
                         `{id}`, and inherited questions come first"
                        )
                    } else {
                        format!("refine `{id}`: its {what} cannot be evaluated: {e}")
                    });
                continue;
            }
        };
        let Some(choices) = q.kind.choices() else {
            continue;
        };
        let picked: Vec<&str> = match (&q.kind, &value) {
            (AnswerKind::Choice { .. }, weft_core::Value::String(s)) => vec![s.as_str()],
            (AnswerKind::MultiChoice { .. }, weft_core::Value::List(items)) => items
                .iter()
                .filter_map(|v| match v {
                    weft_core::Value::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect(),
            // A value of the wrong kind fails the render check.
            _ => continue,
        };
        for s in picked {
            if q.narrowing.blocked.iter().any(|b| b == s) {
                report.issues.push(if refined {
                    format!(
                        "refine `{id}`: its {what} picks {s:?}, which is blocked by {}",
                        q.narrowing.refiners()
                    )
                } else {
                    format!(
                        "refine `{id}`: the inherited default {s:?} is blocked; refine the \
                         default too"
                    )
                });
            } else if refined && !choices.iter().any(|c| c == s) {
                report.issues.push(format!(
                    "refine `{id}`: its {what} picks {s:?}, which is not one of the choices"
                ));
            }
        }
    }
}

/// The static mounts of a template's includes: where each single include's
/// files land, and the fixed prefix under which a repeat include's
/// instances land (`connectors/{key}` → `connectors`).
struct IncludeMounts<'t> {
    single: Vec<(&'t str, String)>,
    repeat: Vec<(&'t str, String)>,
}

impl<'t> IncludeMounts<'t> {
    fn of(template: &'t Template) -> Self {
        let mut single = Vec::new();
        let mut repeat = Vec::new();
        for inc in &template.includes {
            let decl = &inc.decl;
            if decl.repeat {
                let prefix = decl
                    .path
                    .split("{key}")
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('/');
                if !prefix.is_empty() {
                    repeat.push((decl.name.as_str(), prefix.to_owned()));
                }
            } else if let Ok(mount) = compose::mount_path(&decl.path, &decl.name) {
                // Invalid mounts are reported by the include checks.
                if !mount.as_str().is_empty() {
                    single.push((decl.name.as_str(), mount.into_string()));
                }
            }
        }
        IncludeMounts { single, repeat }
    }

    fn single_under(&self, path: &str) -> Option<&'t str> {
        Self::under(&self.single, path)
    }

    fn repeat_under(&self, path: &str) -> Option<&'t str> {
        Self::under(&self.repeat, path)
    }

    fn under(mounts: &[(&'t str, String)], path: &str) -> Option<&'t str> {
        mounts
            .iter()
            .find(|(_, mount)| {
                path == mount
                    || path
                        .strip_prefix(mount.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map(|(name, _)| *name)
    }
}

/// Slot mistakes visible without rendering: a slot declared anywhere but as
/// a line of `create_file` content or a hunk's added lines, a slot name
/// that is invalid or declared twice by one op, `omit_when_empty` naming a
/// slot its content does not declare, and a fill adding no lines.
fn slot_issues(patch: &Patch) -> Vec<String> {
    use weft_core::{Line, SlotDecl};
    let mut out = Vec::new();
    let stray = |lines: &[Line], what: &str, out: &mut Vec<String>| {
        for line in lines.iter().filter(|l| l.has_slot()) {
            let name = line
                .0
                .iter()
                .find_map(|s| match s {
                    Segment::Slot(decl) => Some(decl.slot.as_str()),
                    _ => None,
                })
                .unwrap_or_default();
            out.push(format!(
                "slot `{name}` is declared in {what}; a slot is a line of its own in \
                 `create_file` content or a hunk's `added` lines"
            ));
        }
    };
    let declared = |lines: &[Line], path: &TemplatePath, out: &mut Vec<String>| {
        let path = crate::graph::display_path(path);
        let mut names = BTreeSet::new();
        for line in lines {
            match line.as_slot() {
                Some(decl) if !SlotDecl::valid_name(&decl.slot) => out.push(format!(
                    "`{}` in `{path}` is not a valid slot name (ASCII letters, digits, `-`, `_` \
                     and `.`)",
                    decl.slot
                )),
                Some(decl) if !names.insert(decl.slot.clone()) => {
                    out.push(format!("`{path}` declares slot `{}` twice", decl.slot))
                }
                Some(_) => {}
                // A slot inside a line of other segments is no slot line.
                None => stray(
                    std::slice::from_ref(line),
                    &format!("a line of `{path}`"),
                    out,
                ),
            }
        }
        names
    };
    for op in &patch.ops {
        match op {
            Op::CreateFile {
                path,
                omit_when_empty,
                content,
                ..
            } => {
                let names = declared(&content.0, path, &mut out);
                for name in omit_when_empty.iter().filter(|n| !names.contains(*n)) {
                    out.push(format!(
                        "`omit_when_empty` names `{name}`, which the content of `{}` does not \
                         declare",
                        crate::graph::display_path(path)
                    ));
                }
            }
            Op::ModifyFile { path, hunks } => {
                for hunk in hunks {
                    declared(&hunk.added, path, &mut out);
                    let path = crate::graph::display_path(path);
                    let context = format!("a hunk's context or removed lines for `{path}`");
                    stray(&hunk.context_before, &context, &mut out);
                    stray(&hunk.removed, &context, &mut out);
                    stray(&hunk.context_after, &context, &mut out);
                }
            }
            Op::FillSlot {
                path,
                slot,
                key,
                lines,
            } => {
                let path = crate::graph::display_path(path);
                if lines.is_empty() {
                    out.push(format!(
                        "its fill of slot `{slot}` of `{path}` adds no lines"
                    ));
                }
                let what = format!("a fill of slot `{slot}` of `{path}` (slots do not nest)");
                stray(lines, &what, &mut out);
                stray(
                    std::slice::from_ref(key),
                    &format!("the key of {what}"),
                    &mut out,
                );
            }
            Op::CreateBinaryFile { .. }
            | Op::DeleteFile { .. }
            | Op::RenamePath { .. }
            | Op::SetMode { .. } => {}
        }
    }
    out
}

/// Every path an op touches (both sides of a rename).
fn op_paths(patch: &Patch) -> impl Iterator<Item = &TemplatePath> {
    patch.ops.iter().flat_map(|op| match op {
        Op::CreateFile { path, .. }
        | Op::CreateBinaryFile { path, .. }
        | Op::ModifyFile { path, .. }
        | Op::DeleteFile { path }
        | Op::SetMode { path, .. }
        | Op::FillSlot { path, .. } => vec![path],
        Op::RenamePath { from, to } => vec![from, to],
    })
}

/// The leading literal text of a path, up to its first answer/expression
/// segment. None when the path starts with a non-literal segment: nothing
/// about it can be checked statically.
fn literal_prefix(path: &TemplatePath) -> Option<String> {
    let mut prefix = String::new();
    for seg in &path.0 {
        match seg {
            Segment::Literal(text) => prefix.push_str(text),
            Segment::Answer(_) | Segment::Expr(_) | Segment::Slot(_) => break,
        }
    }
    (!prefix.is_empty()).then_some(prefix)
}

/// The graph nodes as the renderer takes them.
fn framed_nodes<'n>(nodes: &'n [GraphNode<'_>]) -> Vec<Framed<'n>> {
    nodes
        .iter()
        .map(|n| Framed {
            id: n.id,
            depends_on: &n.depends_on,
            patch: n.patch,
            answers: n.answers,
            mount: &n.mount,
        })
        .collect()
}

/// A node and everything it transitively depends on.
fn ancestor_ids(
    nodes: &[GraphNode<'_>],
    by_id: &BTreeMap<PatchId, usize>,
    id: PatchId,
) -> BTreeSet<PatchId> {
    let mut out = BTreeSet::new();
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        if out.insert(cur) {
            if let Some(&idx) = by_id.get(&cur) {
                stack.extend(&nodes[idx].depends_on);
            }
        }
    }
    out
}

/// A node's name in reports: its patch name (`<include>/<patch>` for a
/// single include's node), else its frame's keys and short id.
fn node_name(template: &Template, nodes: &[GraphNode<'_>], id: PatchId) -> String {
    if let Some(name) = template.id_to_name.get(&id) {
        return name.clone();
    }
    match nodes.iter().find(|n| n.id == id) {
        Some(node) => {
            let frame: Vec<&str> = node.frame.iter().map(|(_, key)| key.as_str()).collect();
            format!("{}/{}", frame.join("/"), node.patch.id.short())
        }
        None => id.short(),
    }
}

/// Every node on top of only its own ancestors, under the check answers. A
/// node that fails there fails in every order, so it is reported once —
/// for a hunk that matched nowhere, with the line it expected and the
/// segment that rendered the line actually there — and returned, with its
/// descendants, to keep it out of the reports that would repeat the
/// failure pair by pair.
fn check_alone(
    template: &Template,
    nodes: &[GraphNode<'_>],
    report: &mut CheckReport,
) -> BTreeSet<PatchId> {
    let eval = StarlarkEval;
    let by_id: BTreeMap<PatchId, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let framed = framed_nodes(nodes);
    let mut broken = BTreeSet::new();
    for node in nodes.iter().filter(|n| n.patch.foreach.is_none()) {
        let members: Vec<Framed<'_>> = ancestor_ids(nodes, &by_id, node.id)
            .iter()
            .filter_map(|id| by_id.get(id).map(|&idx| framed[idx]))
            .collect();
        // An ordering problem is reported by the pairwise check.
        let Ok(order) = weft_core::render::framed_order(&members) else {
            continue;
        };
        let Err(err) = weft_core::render::render_framed(&order, &eval) else {
            continue;
        };
        if err.patch() != Some(node.id) {
            continue; // an ancestor's failure, reported for that ancestor
        }
        // Once more, traced, to say where the missed lines came from.
        let err = weft_core::render::render_framed_into(weft_core::Draft::traced(), &order, &eval)
            .and_then(|(draft, _)| draft.finish())
            .err()
            .unwrap_or(err);
        report.issues.push(format!(
            "patch `{}` does not apply under these answers: {}",
            node_name(template, nodes, node.id),
            diagnose(template, nodes, &err)
        ));
        broken.insert(node.id);
    }
    nodes
        .iter()
        .map(|n| n.id)
        .filter(|id| {
            ancestor_ids(nodes, &by_id, *id)
                .iter()
                .any(|a| broken.contains(a))
        })
        .collect()
}

/// Why a node failed on top of its ancestors, naming patches rather than ids.
fn diagnose(template: &Template, nodes: &[GraphNode<'_>], err: &RenderError) -> String {
    match err {
        RenderError::HunkNoMatch {
            path,
            hunk,
            near: Some(near),
            ..
        } => {
            let part = match near.part {
                HunkPart::ContextBefore => "before the change",
                HunkPart::Removed => "among the lines it replaces",
                HunkPart::ContextAfter => "after the change",
            };
            let expected = shown(&near.expected);
            let Some((line, found, origin)) = &near.found else {
                return format!(
                    "hunk {hunk} does not match `{path}`: it expects {expected} {part}, past the \
                     edge of the file"
                );
            };
            let owner = node_name(template, nodes, origin.node);
            let reads = || {
                if found.trim().is_empty() {
                    "is empty".to_owned()
                } else {
                    format!("reads `{}`", clipped(found, 70))
                }
            };
            match &origin.source {
                LineSource::Expr(expr) => format!(
                    "hunk {hunk} does not match `{path}`: it expects {expected} {part}, where \
                     line {line} {}, rendered from an `expr` segment of patch `{owner}` \
                     (`{}`); context taken from an expression's output only holds under the \
                     answers it was recorded with, so re-record the hunk with context that \
                     avoids that line",
                    reads(),
                    clipped(expr.as_str(), 48)
                ),
                LineSource::Slot { slot, key: None } => format!(
                    "hunk {hunk} does not match `{path}`: its context runs across slot `{slot}` \
                     (declared by patch `{owner}`), whose lines depend on which patches fill \
                     it; add those lines with a `fill_slot` instead, or anchor the hunk on one \
                     side of the slot"
                ),
                LineSource::Slot {
                    slot,
                    key: Some(key),
                } => format!(
                    "hunk {hunk} does not match `{path}`: it expects {expected} {part}, where \
                     line {line} is patch `{owner}`'s contribution `{key}` to slot `{slot}`"
                ),
                LineSource::Plain => format!(
                    "hunk {hunk} does not match `{path}`: it expects {expected} {part}, where \
                     line {line} {}, written by patch `{owner}`",
                    reads()
                ),
            }
        }
        RenderError::HunkNoMatch { path, hunk, .. } => {
            format!("hunk {hunk} does not match `{path}` (context not found)")
        }
        RenderError::HunkAmbiguous {
            path, hunk, count, ..
        } => format!("hunk {hunk} matches `{path}` in {count} places; its context is ambiguous"),
        RenderError::MissingFile { path, .. } => format!(
            "`{path}` does not exist on top of its dependencies; depend on the patch that \
             creates it"
        ),
        RenderError::CreateExists { path, .. } => {
            format!("`{path}` already exists on top of its dependencies")
        }
        RenderError::OmittedFileEdited { path, owner, .. } => format!(
            "it changes `{path}`, which patch `{}` leaves out while its slots are empty \
             (`omit_when_empty`), and nothing it depends on fills them; fill one of the slots \
             instead, depend on a patch that fills one, or drop `omit_when_empty`",
            node_name(template, nodes, *owner)
        ),
        other => {
            // The report already names the patch: drop the error's own prefix.
            let text = named(template, nodes, &other.to_string());
            let own = other
                .patch()
                .map(|id| format!("patch `{}`: ", node_name(template, nodes, id)));
            match own.as_deref().and_then(|own| text.strip_prefix(own)) {
                Some(rest) => rest.to_owned(),
                None => text,
            }
        }
    }
}

/// `text` with every graph node's id replaced by its name in backticks:
/// render errors carry ids, and a report names patches.
fn named(template: &Template, nodes: &[GraphNode<'_>], text: &str) -> String {
    let mut out = text.to_owned();
    for node in nodes {
        let hex = node.id.to_hex();
        if out.contains(&hex) {
            out = out.replace(&hex, &format!("`{}`", node_name(template, nodes, node.id)));
        }
    }
    out
}

/// A file line as a report quotes it.
fn shown(line: &str) -> String {
    if line.trim().is_empty() {
        "an empty line".to_owned()
    } else {
        format!("`{}`", clipped(line, 70))
    }
}

fn clipped(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

/// A render failure that is two patches filling one slot under one key:
/// (path, slot, key, and the two patches in id order, so the pair reads the
/// same whichever of the two applied first).
type DuplicateFill = (Utf8PathBuf, String, String, PatchId, PatchId);

fn duplicate_fill(err: &RenderError) -> Option<DuplicateFill> {
    match err {
        RenderError::DuplicateSlotKey(clash) => Some((
            clash.path.clone(),
            clash.slot.clone(),
            clash.key.clone(),
            clash.first.min(clash.second),
            clash.first.max(clash.second),
        )),
        _ => None,
    }
}

/// What a node's ops touch, rendered in its frame: the paths it changes by
/// anything but a fill, and its fills as (path, slot, key). `None` when a
/// path or key does not render (the render itself reports that).
type Touches = (
    BTreeSet<Utf8PathBuf>,
    BTreeSet<(Utf8PathBuf, String, String)>,
);

fn touches(node: &Framed<'_>, eval: &dyn ExprEval) -> Option<Touches> {
    let place = |path: &TemplatePath| {
        weft_core::render::render_path(path, node.answers, eval)
            .ok()
            .map(|p| node.mount.join(p))
    };
    let mut edits = BTreeSet::new();
    let mut fills = BTreeSet::new();
    for op in &node.patch.ops {
        match op {
            Op::FillSlot {
                path, slot, key, ..
            } => {
                let key = weft_core::render::render_line(key, node.answers, eval).ok()?;
                fills.insert((place(path)?, slot.clone(), key));
            }
            Op::RenamePath { from, to } => {
                edits.insert(place(from)?);
                edits.insert(place(to)?);
            }
            Op::CreateFile { path, .. }
            | Op::CreateBinaryFile { path, .. }
            | Op::ModifyFile { path, .. }
            | Op::DeleteFile { path }
            | Op::SetMode { path, .. } => {
                edits.insert(place(path)?);
            }
        }
    }
    Some((edits, fills))
}

/// Two nodes whose only common files are ones both merely fill slots of,
/// under different keys, commute by construction: a slot renders its
/// contributions sorted by key, and no other op sees them.
fn fills_only_overlap(p: &Touches, q: &Touches) -> bool {
    let paths = |(edits, fills): &Touches| -> BTreeSet<Utf8PathBuf> {
        edits
            .iter()
            .cloned()
            .chain(fills.iter().map(|(path, _, _)| path.clone()))
            .collect()
    };
    let (p_paths, q_paths) = (paths(p), paths(q));
    let mut shared = p_paths.intersection(&q_paths).peekable();
    shared.peek().is_some()
        && shared.all(|path| !p.0.contains(path) && !q.0.contains(path))
        && p.1.is_disjoint(&q.1)
}

/// Every pair of graph nodes with no dependency path between them must
/// commute: applying `…A B` and `…B A` has to produce byte-identical trees.
/// Pairs inside the same child frame are that child's own business (its
/// recursive check covers them) and are not counted, and neither are pairs
/// with a `blocked` node, which fails in any order ([`check_alone`]). Pairs
/// that only fill the same slots commute by construction and are not
/// rendered; pairs that fill one slot under one key go to `duplicates`.
fn check_commutation(
    template: &Template,
    nodes: &[GraphNode<'_>],
    blocked: &BTreeSet<PatchId>,
    duplicates: &mut BTreeSet<DuplicateFill>,
    report: &mut CheckReport,
) {
    let eval = StarlarkEval;
    let by_id: BTreeMap<PatchId, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let framed = framed_nodes(nodes);
    let touched: Vec<Option<Touches>> = framed.iter().map(|n| touches(n, &eval)).collect();

    let mut pairs_checked = 0usize;
    let mut by_construction = 0usize;
    for (i, p) in framed.iter().enumerate() {
        if blocked.contains(&p.id) {
            continue;
        }
        let p_anc = ancestor_ids(nodes, &by_id, p.id);
        for (j, q) in framed.iter().enumerate().skip(i + 1) {
            if !nodes[i].frame.is_empty() && nodes[i].frame == nodes[j].frame {
                continue; // same child frame: the child's own check proves it
            }
            if blocked.contains(&q.id) {
                continue;
            }
            let q_anc = ancestor_ids(nodes, &by_id, q.id);
            if p_anc.contains(&q.id) || q_anc.contains(&p.id) {
                continue; // ordered by dependency; nothing to prove
            }
            if let (Some(tp), Some(tq)) = (&touched[i], &touched[j]) {
                if fills_only_overlap(tp, tq) {
                    by_construction += 1;
                    continue;
                }
            }
            // Shared prelude: all ancestors of either, minus the pair itself.
            let prelude_nodes: Vec<Framed<'_>> = p_anc
                .union(&q_anc)
                .filter(|id| **id != p.id && **id != q.id)
                .filter_map(|id| by_id.get(id).map(|&idx| framed[idx]))
                .collect();
            let prelude: Vec<&Framed<'_>> = match weft_core::render::framed_order(&prelude_nodes) {
                Ok(order) => order,
                Err(e) => {
                    report
                        .issues
                        .push(format!("commutation prelude failed: {e}"));
                    continue;
                }
            };
            let mut order_pq = prelude.clone();
            order_pq.push(p);
            order_pq.push(q);
            let mut order_qp = prelude;
            order_qp.push(q);
            order_qp.push(p);

            let name_p = node_name(template, nodes, p.id);
            let name_q = node_name(template, nodes, q.id);
            let render = |order: &[&Framed<'_>]| {
                weft_core::render::render_framed(order, &eval).map(|(tree, _)| tree)
            };
            match (render(&order_pq), render(&order_qp)) {
                (Ok(t1), Ok(t2)) => {
                    if t1.hash() != t2.hash() {
                        report.issues.push(format!(
                            "independent patches `{name_p}` and `{name_q}` do not commute: \
                             applying them in either order produces different trees; \
                             add an explicit dependency between them"
                        ));
                    }
                }
                (Err(e), _) | (_, Err(e)) => match duplicate_fill(&e) {
                    Some(dup) => {
                        duplicates.insert(dup);
                    }
                    None => report.issues.push(format!(
                        "independent patches `{name_p}` and `{name_q}` do not commute: \
                         one application order fails: {}{}",
                        named(template, nodes, &e.to_string()),
                        match &e {
                            RenderError::CreateExists { path, .. } => format!(
                                "; if both belong in one project, run `weft share {path} \
                                 --name NAME` so each adds its lines to one shared file"
                            ),
                            _ => String::new(),
                        }
                    )),
                },
            }
            pairs_checked += 1;
        }
    }
    let mut note = format!("commutation ok for {pairs_checked} independent pair(s)");
    if by_construction > 0 {
        note.push_str(&format!(
            "; {by_construction} more only fill the same slots, which commutes by construction"
        ));
    }
    report.notes.push(note);
}

/// Collect every `Segment::Answer` id used in a patch's ops. Public
/// because it is generic impact tooling (weft-cloud uses it for question
/// reference analysis).
pub fn answer_refs(patch: &Patch) -> BTreeSet<AnswerId> {
    let mut out = BTreeSet::new();
    let visit_path = |path: &TemplatePath, out: &mut BTreeSet<AnswerId>| {
        for seg in &path.0 {
            if let Segment::Answer(id) = seg {
                out.insert(id.clone());
            }
        }
    };
    let visit_lines = |lines: &[weft_core::Line], out: &mut BTreeSet<AnswerId>| {
        for line in lines {
            for seg in &line.0 {
                if let Segment::Answer(id) = seg {
                    out.insert(id.clone());
                }
            }
        }
    };
    for op in &patch.ops {
        match op {
            Op::CreateFile { path, content, .. } => {
                visit_path(path, &mut out);
                visit_lines(&content.0, &mut out);
            }
            Op::ModifyFile { path, hunks } => {
                visit_path(path, &mut out);
                for hunk in hunks {
                    visit_lines(&hunk.context_before, &mut out);
                    visit_lines(&hunk.removed, &mut out);
                    visit_lines(&hunk.added, &mut out);
                    visit_lines(&hunk.context_after, &mut out);
                }
            }
            // Binary data is opaque; only the path can reference answers.
            Op::DeleteFile { path }
            | Op::SetMode { path, .. }
            | Op::CreateBinaryFile { path, .. } => visit_path(path, &mut out),
            Op::RenamePath { from, to } => {
                visit_path(from, &mut out);
                visit_path(to, &mut out);
            }
            Op::FillSlot {
                path, key, lines, ..
            } => {
                visit_path(path, &mut out);
                visit_lines(std::slice::from_ref(key), &mut out);
                visit_lines(lines, &mut out);
            }
        }
    }
    out
}

/// Parent answers gathered from the layered set (namespaced `<include>.<q>`
/// answers route to the child instances), plus the routed child answers.
fn resolve_check_answers(
    template: &Template,
    opts: &CheckOptions,
) -> Result<(AnswerSet, compose::ChildProvided)> {
    let provided = answers::layered_answers(
        template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
    )?;
    let (parent_provided, child_provided) = compose::split_provided(template, &provided)?;
    let resolved = answers::gather(
        template,
        &parent_provided,
        &AnswerSet::new(),
        &StarlarkEval,
        &mut NonInteractive,
    )?;
    Ok((resolved, child_provided))
}

/// CLI-facing: print the report, fail if any issues were found.
pub fn finish(template_name: &str, report: &CheckReport) -> Result<()> {
    for note in &report.notes {
        eprintln!("check: {note}");
    }
    if report.issues.is_empty() {
        println!("ok: template `{template_name}` passed all checks");
        Ok(())
    } else {
        for issue in &report.issues {
            eprintln!("error: {issue}");
        }
        bail!("check failed with {} issue(s)", report.issues.len())
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;

    use super::*;
    use crate::template::PathResolver;

    const BASE: &str = r#"
[template]
name = "base"
weft-version = "0.1"

[[question]]
id = "name"
kind = "string"
default = "'demo'"

[[question]]
id = "ci"
kind = "choice"
choices = ["github", "gitlab", "none"]
default = "'github'"
"#;

    /// `weft check` issues about refinements of an extender `mid` of the
    /// base, which declares its own `flavor` after the inherited questions.
    fn refine_issues(refine: &str) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        for (name, manifest) in [
            ("base", BASE.to_owned()),
            (
                "mid",
                format!(
                    "[template]\nname = \"mid\"\nweft-version = \"0.1\"\nextends = \"../base\"\n\n\
                     [[question]]\nid = \"flavor\"\nkind = \"string\"\ndefault = \"'plain'\"\n\n\
                     {refine}"
                ),
            ),
        ] {
            std::fs::create_dir_all(root.join(name).join("patches")).unwrap();
            std::fs::write(root.join(name).join("weft.toml"), manifest).unwrap();
        }
        let opts = CheckOptions {
            template: root.join("mid"),
            presets: vec![],
            answers: vec![],
            answers_file: None,
        };
        let report = run(&opts, &mut PathResolver).unwrap();
        report
            .issues
            .into_iter()
            .filter(|i| i.starts_with("refine "))
            .collect()
    }

    #[test]
    fn a_refined_default_or_lock_may_only_mention_earlier_questions() {
        let earlier_only = |what: &str| {
            format!(
                "refine `name`: its {what} may only mention questions declared before `name`, \
                 and inherited questions come first"
            )
        };
        assert_eq!(
            refine_issues("[refine.name]\ndefault = \"flavor\"\n"),
            [earlier_only("default")]
        );
        assert_eq!(
            refine_issues("[refine.name]\nlock = \"flavor + '-x'\"\n"),
            [earlier_only("lock")]
        );
        assert_eq!(
            refine_issues("[refine.ci]\ndefault = \"'none' if name else 'github'\"\n"),
            Vec::<String>::new()
        );
        let undefined = refine_issues("[refine.name]\ndefault = \"nowhere\"\n");
        let evaluating = "refine `name`: its default cannot be evaluated: ";
        assert!(
            matches!(&undefined[..], [one] if one.starts_with(evaluating)),
            "{undefined:?}"
        );
    }

    #[test]
    fn a_refined_choice_must_not_default_to_what_it_blocks() {
        assert_eq!(
            refine_issues("[refine.ci]\nblocked = [\"github\"]\n"),
            ["refine `ci`: the inherited default \"github\" is blocked; refine the default too"]
        );
        assert_eq!(
            refine_issues("[refine.ci]\nblocked = [\"github\"]\ndefault = \"'github'\"\n"),
            ["refine `ci`: its default picks \"github\", which is blocked by template `mid`"]
        );
        assert_eq!(
            refine_issues("[refine.ci]\nblocked = [\"github\"]\ndefault = \"'azure'\"\n"),
            ["refine `ci`: its default picks \"azure\", which is not one of the choices"]
        );
        assert_eq!(
            refine_issues("[refine.ci]\nblocked = [\"github\"]\ndefault = \"'gitlab'\"\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_multichoice_narrowed_to_nothing_passes() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        for (name, manifest) in [
            (
                "base",
                "[template]\nname = \"base\"\nweft-version = \"0.1\"\n\n\
                 [[question]]\nid = \"skills\"\nkind = \"multichoice\"\n\
                 choices = [\"dbt\", \"slides\"]\ndefault = \"[]\"\n",
            ),
            (
                "mid",
                "[template]\nname = \"mid\"\nweft-version = \"0.1\"\nextends = \"../base\"\n\n\
                 [refine.skills]\nchoices = []\n",
            ),
        ] {
            std::fs::create_dir_all(root.join(name).join("patches")).unwrap();
            std::fs::write(root.join(name).join("weft.toml"), manifest).unwrap();
        }
        let opts = CheckOptions {
            template: root.join("mid"),
            presets: vec![],
            answers: vec![],
            answers_file: None,
        };
        let report = run(&opts, &mut PathResolver).unwrap();
        assert_eq!(report.issues, Vec::<String>::new());
        // Declaring a question with no choices is still an issue.
        std::fs::write(
            root.join("base/weft.toml"),
            "[template]\nname = \"base\"\nweft-version = \"0.1\"\n\n\
             [[question]]\nid = \"skills\"\nkind = \"multichoice\"\nchoices = []\ndefault = \"[]\"\n",
        )
        .unwrap();
        let opts = CheckOptions {
            template: root.join("base"),
            ..opts
        };
        let report = run(&opts, &mut PathResolver).unwrap();
        assert_eq!(report.issues, ["question `skills` has no choices"]);
    }
}
