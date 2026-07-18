//! `weft check`: full template validation — manifest sanity, expression
//! parsing, patch graph structure, and (when answers are resolvable) a full
//! render plus a commutation smoke test over independent patch pairs.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use weft_core::render::ExprEval;
use weft_core::{AnswerId, AnswerKind, Op, Patch, PatchId, Segment, TemplatePath};
use weft_lang::StarlarkEval;

use crate::interact::NonInteractive;
use crate::template::Template;
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
            if choices.is_empty() {
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

    // Includes: mount paths render and don't collide; binds parse, reference
    // real child questions, and trial-evaluate over dummy parent answers;
    // child templates pass their own checks (issues prefixed).
    {
        let mut mounts: BTreeMap<String, &str> = BTreeMap::new();
        let dummy_parent = answers::dummy_answers(questions);
        for inc in &template.includes {
            // Hub includes need a version requirement; path includes must
            // not carry one.
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
                if !inc
                    .template
                    .manifest
                    .questions
                    .iter()
                    .any(|q| q.id.0 == *child_id)
                {
                    issue(
                        &mut report,
                        format!(
                            "include `{}`: bind `{child_id}` names no question in the child \
                             template",
                            inc.decl.name
                        ),
                    );
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
            for other in &template.patches {
                if other.depends_on.contains(&patch.id) {
                    issue(
                        &mut report,
                        format!(
                            "patch `{}` depends on foreach patch `{name}`; foreach patches must \
                             be graph leaves",
                            template.id_to_name[&other.id]
                        ),
                    );
                }
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
                    weft_core::Segment::Literal(_) => {}
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
        Ok(resolved) => {
            let eval = StarlarkEval;
            match weft_core::render::render(&template.patches, &resolved, &eval) {
                Err(e) => issue(&mut report, format!("full render failed: {e}")),
                Ok(mut tree) => {
                    report
                        .notes
                        .push(format!("render ok ({} files)", tree.len()));
                    // Trial-apply each foreach patch once with a dummy
                    // instance scope, so its segments/hunks are validated
                    // even though check has no real instances.
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
                            weft_core::render::apply_ops(&mut tree, patch, &scope, &eval)
                        {
                            issue(
                                &mut report,
                                format!(
                                    "foreach patch `{name}` failed a trial application with a \
                                     dummy instance: {e}"
                                ),
                            );
                        }
                    }
                }
            }
            check_commutation(&template, &resolved, &mut report);
        }
    }

    Ok(report)
}

/// Every pair of patches with no dependency path between them must commute:
/// applying `…A B` and `…B A` has to produce byte-identical trees.
fn check_commutation(
    template: &Template,
    resolved: &weft_core::AnswerSet,
    report: &mut CheckReport,
) {
    let eval = StarlarkEval;
    let patches = &template.patches;
    let by_id: BTreeMap<PatchId, &Patch> = patches.iter().map(|p| (p.id, p)).collect();

    let ancestors = |id: PatchId| -> BTreeSet<PatchId> {
        let mut out = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if out.insert(cur) {
                stack.extend(&by_id[&cur].depends_on);
            }
        }
        out
    };

    let mut pairs_checked = 0usize;
    for (i, p) in patches.iter().enumerate() {
        let p_anc = ancestors(p.id);
        for q in &patches[i + 1..] {
            let q_anc = ancestors(q.id);
            if p_anc.contains(&q.id) || q_anc.contains(&p.id) {
                continue; // ordered by dependency; nothing to prove
            }
            // Shared prelude: all ancestors of either, minus the pair itself.
            let prelude_ids: BTreeSet<PatchId> = p_anc
                .union(&q_anc)
                .copied()
                .filter(|id| *id != p.id && *id != q.id)
                .collect();
            let prelude: Vec<&Patch> = match weft_core::render::patch_order(
                &prelude_ids
                    .iter()
                    .map(|id| (*by_id[id]).clone())
                    .collect::<Vec<_>>(),
            ) {
                Ok(order) => {
                    let ids: Vec<PatchId> = order.iter().map(|p| p.id).collect();
                    ids.iter().map(|id| by_id[id]).collect()
                }
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

            let name_p = &template.id_to_name[&p.id];
            let name_q = &template.id_to_name[&q.id];
            let render =
                |order: &[&Patch]| weft_core::render::render_ordered(order, resolved, &eval);
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
                (Err(e), _) | (_, Err(e)) => {
                    report.issues.push(format!(
                        "independent patches `{name_p}` and `{name_q}` do not commute: \
                         one application order fails: {e}"
                    ));
                }
            }
            pairs_checked += 1;
        }
    }
    report.notes.push(format!(
        "commutation ok for {pairs_checked} independent pair(s)"
    ));
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
        }
    }
    out
}

fn resolve_check_answers(template: &Template, opts: &CheckOptions) -> Result<weft_core::AnswerSet> {
    let provided = answers::layered_answers(
        template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
    )?;
    answers::gather(
        template,
        &provided,
        &weft_core::AnswerSet::new(),
        &StarlarkEval,
        &mut NonInteractive,
    )
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
