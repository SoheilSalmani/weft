//! Hook collection, ordering, and execution.
//!
//! Hooks are patch-scoped side-effects (`weft_core::Hook`) that run before
//! (`pre`) or after (`post`) the render. Only hooks belonging to **active**
//! patches (their `when` passes) run. Within a phase they are topologically
//! ordered: the base order is patch render order then in-patch declaration
//! order, with explicit `after` edges layered on top.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use globset::{Glob, GlobMatcher};
use weft_core::render::ExprEval;
use weft_core::{AnswerId, AnswerSet, Hook, HookId, HookInput, HookPhase, Patch, PatchId};

use crate::template::Template;

/// What changed between the previous render and this one. `None` means
/// "initial scaffold": every post-hook's inputs are considered changed.
pub struct ChangeSet {
    pub paths: BTreeSet<camino::Utf8PathBuf>,
    pub answers: BTreeSet<AnswerId>,
}

/// The active hooks of a template, split by phase and already ordered.
pub struct Collected<'t> {
    pub pre: Vec<&'t Hook>,
    pub post: Vec<&'t Hook>,
}

/// Patches whose `when` passes and none of whose dependencies are skipped, in
/// render order. Mirrors the skip logic in `weft_core::render`.
fn active_patches<'t>(
    template: &'t Template,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Vec<&'t Patch>> {
    let order = weft_core::render::patch_order(&template.patches)?;
    let mut skipped: BTreeSet<PatchId> = BTreeSet::new();
    let mut active = Vec::new();
    for patch in order {
        if patch.depends_on.iter().any(|d| skipped.contains(d)) {
            skipped.insert(patch.id);
            continue;
        }
        if let Some(when) = &patch.when {
            let on = eval
                .eval_bool(when, answers)
                .with_context(|| format!("evaluating when of patch {}", patch.id.short()))?;
            if !on {
                skipped.insert(patch.id);
                continue;
            }
        }
        active.push(patch);
    }
    Ok(active)
}

/// Collect and order the active patches' hooks by phase.
pub fn collect<'t>(
    template: &'t Template,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Collected<'t>> {
    let active = active_patches(template, answers, eval)?;
    // Base order: patch render order, then in-patch declaration order.
    let mut pre: Vec<&Hook> = Vec::new();
    let mut post: Vec<&Hook> = Vec::new();
    for patch in &active {
        for hook in &patch.meta.hooks {
            match hook.phase {
                HookPhase::Pre => pre.push(hook),
                HookPhase::Post => post.push(hook),
            }
        }
    }
    Ok(Collected {
        pre: order_phase(pre)?,
        post: order_phase(post)?,
    })
}

/// Topologically order one phase's hooks: base order is the input order (patch
/// render order + declaration order); `after` adds edges. `after` references
/// to hooks outside this set (other phase / inactive patch) are ignored.
fn order_phase(hooks: Vec<&Hook>) -> Result<Vec<&Hook>> {
    let index: BTreeMap<&HookId, usize> =
        hooks.iter().enumerate().map(|(i, h)| (&h.id, i)).collect();
    // indegree over in-set `after` edges
    let mut indegree = vec![0usize; hooks.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); hooks.len()];
    for (i, h) in hooks.iter().enumerate() {
        for dep in &h.after {
            if let Some(&j) = index.get(dep) {
                indegree[i] += 1;
                dependents[j].push(i);
            }
        }
    }
    // Kahn's algorithm; ready set drained in base-order (smallest index first)
    // for a deterministic, stable ordering.
    let mut ready: BTreeSet<usize> = (0..hooks.len()).filter(|&i| indegree[i] == 0).collect();
    let mut out = Vec::with_capacity(hooks.len());
    while let Some(&i) = ready.iter().next() {
        ready.remove(&i);
        out.push(hooks[i]);
        for &d in &dependents[i] {
            indegree[d] -= 1;
            if indegree[d] == 0 {
                ready.insert(d);
            }
        }
    }
    if out.len() != hooks.len() {
        let stuck: Vec<_> = hooks
            .iter()
            .enumerate()
            .filter(|(i, _)| indegree[*i] > 0)
            .map(|(_, h)| h.id.to_string())
            .collect();
        bail!("hook `after` cycle among: {}", stuck.join(", "));
    }
    Ok(out)
}

/// Restrict post-hooks to those that should fire on `weft update`: an input
/// changed (or `changes` is `None`, i.e. initial scaffold — all fire). A hook
/// with no inputs never re-fires on update (only on initial scaffold), which
/// mirrors "copy-only" tasks.
pub fn fire_on_update<'t>(
    post: &[&'t Hook],
    changes: Option<&ChangeSet>,
    eval: &dyn ExprEval,
    answers: &AnswerSet,
) -> Result<Vec<&'t Hook>> {
    let Some(changes) = changes else {
        return Ok(post.to_vec());
    };
    let fired: BTreeSet<&HookId> = post.iter().map(|h| &h.id).collect();
    let mut plan = Vec::new();
    for hook in post {
        // hook-level gate first
        if let Some(when) = &hook.when {
            if !eval.eval_bool(when, answers)? {
                continue;
            }
        }
        let changed = hook.inputs.iter().try_fold(false, |acc, input| {
            anyhow::Ok(
                acc || match input {
                    HookInput::Glob(pattern) => {
                        let matcher = glob_matcher(pattern)?;
                        changes
                            .paths
                            .iter()
                            .any(|p| matcher.is_match(p.as_std_path()))
                    }
                    HookInput::Answer(id) => changes.answers.contains(id),
                    HookInput::Hook(id) => fired.contains(id),
                },
            )
        })?;
        if changed {
            plan.push(*hook);
        }
    }
    Ok(plan)
}

/// Run a sequence of hooks with `sh -c` in `cwd`, stopping at the first
/// failure. Each command is rendered against `answers` (interpolating answers
/// and expressions). A non-zero exit aborts with the hook's label.
pub fn run(
    hooks: &[&Hook],
    cwd: &Utf8Path,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<()> {
    for hook in hooks {
        if let Some(when) = &hook.when {
            if !eval
                .eval_bool(when, answers)
                .with_context(|| format!("evaluating when of hook `{}`", hook.id))?
            {
                continue;
            }
        }
        run_command(
            &hook.action,
            &format!("hook {} — {}", hook.id, hook.label),
            cwd,
            answers,
            eval,
        )?;
    }
    Ok(())
}

/// Render and run one interpolatable command with `sh -c` in `cwd`.
/// Shared by hook execution and generator commands (`weft record --exec`,
/// `weft patch resync`). `label` prefixes the log line and error.
pub fn run_command(
    action: &weft_core::Command,
    label: &str,
    cwd: &Utf8Path,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<()> {
    let command = weft_core::render::render_segments(&action.0, answers, eval)
        .with_context(|| format!("rendering command for {label}"))?;
    // Never echo an interpolated command — it may contain a resolved
    // secret. Literal commands are safe to show in full.
    if action.is_literal() {
        eprintln!("{label}: {command}");
    } else {
        eprintln!("{label}");
    }
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("spawning {label}"))?;
    if !status.success() {
        bail!("{label} failed with {status}");
    }
    Ok(())
}

fn glob_matcher(pattern: &str) -> Result<GlobMatcher> {
    Ok(Glob::new(pattern)
        .with_context(|| format!("invalid glob {pattern:?}"))?
        .compile_matcher())
}

/// Parse a CLI-authored command string (`weft record --exec`, `weft hook
/// add --action`) into segments, interpolating `${…}` **only when
/// relevant**: the inner text must name a declared question (→ answer
/// segment) or be a Starlark expression that evaluates over the declared
/// answers (→ expr segment; trial-run against kind-appropriate stand-ins,
/// like default previews). Anything else — `${HOME}`, `${1:-x}` — stays
/// literal, so plain shell parameter expansion keeps working.
pub fn parse_command(
    input: &str,
    questions: &[weft_core::Question],
    extra: &AnswerSet,
    eval: &dyn ExprEval,
) -> weft_core::Command {
    let mut scope = crate::answers::dummy_answers(questions);
    scope.overlay(extra);
    let mut segments: Vec<weft_core::Segment> = Vec::new();
    let mut literal = String::new();
    let mut rest = input;
    loop {
        let Some(start) = rest.find("${") else {
            literal.push_str(rest);
            break;
        };
        let inner_str = &rest[start + 2..];
        // Balanced-brace scan: Starlark bodies may contain `{…}` themselves.
        let mut depth = 1usize;
        let mut end = None;
        for (i, c) in inner_str.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            // Unterminated `${` — the rest is literal.
            literal.push_str(rest);
            break;
        };
        match interpolation_for(&inner_str[..end], questions, &scope, eval) {
            Some(segment) => {
                literal.push_str(&rest[..start]);
                if !literal.is_empty() {
                    segments.push(weft_core::Segment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(segment);
            }
            // Not relevant: keep the `${…}` byte-for-byte for the shell.
            None => literal.push_str(&rest[..start + 2 + end + 1]),
        }
        rest = &inner_str[end + 1..];
    }
    if !literal.is_empty() || segments.is_empty() {
        segments.push(weft_core::Segment::Literal(literal));
    }
    weft_core::Command(segments)
}

/// The segment for one `${…}` body, or `None` when it isn't a weft
/// interpolation.
fn interpolation_for(
    inner: &str,
    questions: &[weft_core::Question],
    scope: &AnswerSet,
    eval: &dyn ExprEval,
) -> Option<weft_core::Segment> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(q) = questions.iter().find(|q| q.id.0 == trimmed) {
        // A multichoice renders as a list — commands need a projection
        // (`' '.join(id)`), so the bare id stays literal.
        if matches!(q.kind, weft_core::AnswerKind::MultiChoice { .. }) {
            return None;
        }
        return Some(weft_core::Segment::Answer(AnswerId(trimmed.to_owned())));
    }
    if weft_lang::parse_expr(trimmed).is_err() {
        return None;
    }
    let expr = weft_core::StarlarkExpr::from(trimmed);
    match eval.eval(&expr, scope) {
        // A list can't render into a command — project it through an
        // expression (`' '.join(components)`) instead; a bare list stays
        // literal rather than committing a guaranteed render error.
        Ok(weft_core::Value::List(_)) | Err(_) => None,
        Ok(_) => Some(weft_core::Segment::Expr(expr)),
    }
}

/// Static validation of every hook across all patches (for `weft check`):
/// unique ids, `after`/`inputs:hook:` references resolve, no `after` cycle,
/// `inputs` only on post hooks. Returns human-readable issue strings.
pub fn validate_all(template: &Template) -> Vec<String> {
    let mut issues = Vec::new();
    let with_patch: Vec<(&Patch, &Hook)> = template
        .patches
        .iter()
        .flat_map(|p| p.meta.hooks.iter().map(move |h| (p, h)))
        .collect();
    let all: Vec<&Hook> = with_patch.iter().map(|(_, h)| *h).collect();
    let ids: BTreeSet<&HookId> = all.iter().map(|h| &h.id).collect();
    if ids.len() != all.len() {
        issues.push("duplicate hook ids across patches".to_owned());
    }
    let declared_answers: BTreeSet<&AnswerId> =
        template.manifest.questions.iter().map(|q| &q.id).collect();
    for (patch, hook) in &with_patch {
        for a in &hook.after {
            if !ids.contains(a) {
                issues.push(format!("hook `{}`: unknown `after` hook `{a}`", hook.id));
            }
        }
        if matches!(hook.phase, HookPhase::Pre) && !hook.inputs.is_empty() {
            issues.push(format!(
                "hook `{}` is a pre-hook; `inputs` (update re-fire) apply only to post-hooks",
                hook.id
            ));
        }
        for input in &hook.inputs {
            match input {
                HookInput::Glob(g) => {
                    if Glob::new(g).is_err() {
                        issues.push(format!("hook `{}`: invalid glob {g:?}", hook.id));
                    }
                }
                HookInput::Answer(id) => {
                    if !declared_answers.contains(id) {
                        issues.push(format!("hook `{}`: unknown answer input `{id}`", hook.id));
                    }
                }
                HookInput::Hook(id) => {
                    if !ids.contains(id) {
                        issues.push(format!("hook `{}`: unknown hook input `{id}`", hook.id));
                    }
                }
            }
        }
        if let Some(when) = &hook.when {
            if let Err(e) = weft_lang::parse_expr(when.as_str()) {
                issues.push(format!("hook `{}`: when does not parse: {e}", hook.id));
            }
        }
        for seg in &hook.action.0 {
            match seg {
                weft_core::Segment::Expr(e) => {
                    if let Err(err) = weft_lang::parse_expr(e.as_str()) {
                        issues.push(format!(
                            "hook `{}`: command expression does not parse: {err}",
                            hook.id
                        ));
                    }
                }
                weft_core::Segment::Answer(id) => {
                    // Foreach patches additionally see `key` / `instance_<id>`.
                    let foreach_scoped =
                        patch.foreach.is_some() && (id.0 == "key" || id.0.starts_with("instance_"));
                    if !declared_answers.contains(id) && !foreach_scoped {
                        issues.push(format!(
                            "hook `{}`: command references unknown answer `{id}`",
                            hook.id
                        ));
                    }
                }
                weft_core::Segment::Literal(_) => {}
            }
        }
    }
    // cycle detection over the full `after` graph
    let index: BTreeMap<&HookId, usize> = all.iter().enumerate().map(|(i, h)| (&h.id, i)).collect();
    let mut indegree = vec![0usize; all.len()];
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); all.len()];
    for (i, h) in all.iter().enumerate() {
        for a in &h.after {
            if let Some(&j) = index.get(a) {
                indegree[i] += 1;
                deps[j].push(i);
            }
        }
    }
    let mut ready: Vec<usize> = (0..all.len()).filter(|&i| indegree[i] == 0).collect();
    let mut seen = 0;
    while let Some(i) = ready.pop() {
        seen += 1;
        for &d in &deps[i] {
            indegree[d] -= 1;
            if indegree[d] == 0 {
                ready.push(d);
            }
        }
    }
    if seen != all.len() {
        issues.push("hook `after` cycle detected".to_owned());
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use weft_core::{Command, HookEffect};

    fn hook(id: &str, after: &[&str]) -> Hook {
        Hook {
            id: id.into(),
            phase: HookPhase::Post,
            effect: HookEffect::Setup,
            label: id.to_owned(),
            description: None,
            action: Command::literal("true"),
            when: None,
            after: after.iter().map(|a| HookId::from(*a)).collect(),
            inputs: vec![],
        }
    }

    fn order(hooks: &[Hook]) -> Vec<String> {
        order_phase(hooks.iter().collect())
            .unwrap()
            .iter()
            .map(|h| h.id.to_string())
            .collect()
    }

    #[test]
    fn base_order_is_input_order() {
        let hs = [hook("a", &[]), hook("b", &[]), hook("c", &[])];
        assert_eq!(order(&hs), ["a", "b", "c"]);
    }

    #[test]
    fn after_edges_reorder() {
        // format must run after install even though it's declared first
        let hs = [hook("format", &["install"]), hook("install", &[])];
        assert_eq!(order(&hs), ["install", "format"]);
    }

    #[test]
    fn after_to_absent_hook_is_ignored() {
        // referencing a hook not in this phase set doesn't block
        let hs = [hook("a", &["not-here"])];
        assert_eq!(order(&hs), ["a"]);
    }

    #[test]
    fn cycle_is_an_error() {
        let hs = [hook("a", &["b"]), hook("b", &["a"])];
        assert!(order_phase(hs.iter().collect()).is_err());
    }

    fn question(id: &str, kind: weft_core::AnswerKind) -> weft_core::Question {
        weft_core::Question {
            id: id.into(),
            kind,
            prompt: None,
            description: None,
            example: None,
            default: None,
            when: None,
            computed: false,
            section: None,
        }
    }

    fn parse(input: &str) -> Command {
        use weft_core::AnswerKind;
        let questions = vec![
            question("project_name", AnswerKind::String),
            question(
                "components",
                AnswerKind::MultiChoice {
                    choices: vec!["button".into(), "card".into()],
                },
            ),
        ];
        parse_command(
            input,
            &questions,
            &AnswerSet::new(),
            &weft_lang::StarlarkEval,
        )
    }

    #[test]
    fn parse_command_declared_answer_interpolates() {
        use weft_core::Segment;
        let c = parse("echo ${project_name} done");
        assert_eq!(
            c.0,
            vec![
                Segment::Literal("echo ".into()),
                Segment::Answer("project_name".into()),
                Segment::Literal(" done".into()),
            ]
        );
        assert_eq!(c.source(), "echo ${project_name} done");
    }

    #[test]
    fn parse_command_expression_over_answers_interpolates() {
        use weft_core::Segment;
        let c = parse("run ${project_name.lower()}");
        assert!(
            matches!(&c.0[1], Segment::Expr(e) if e.as_str() == "project_name.lower()"),
            "{c:?}"
        );
        // Nested braces inside the expression body survive the scan.
        let c = parse("add ${' '.join([c for c in components if c != 'card'])}");
        assert!(matches!(&c.0[1], Segment::Expr(_)), "{c:?}");
    }

    #[test]
    fn parse_command_irrelevant_stays_literal() {
        // Shell parameter expansion, undeclared names, empty, unterminated.
        for cmd in [
            "echo ${HOME}/${PATH}",
            "echo ${not_a_question}",
            "echo ${}",
            "echo ${unterminated",
        ] {
            let c = parse(cmd);
            assert!(c.is_literal(), "{cmd} → {c:?}");
            assert_eq!(c.source(), cmd);
        }
    }

    #[test]
    fn parse_command_lists_need_projection() {
        // A bare multichoice id would render as a list — stays literal…
        assert!(parse("add ${components}").is_literal());
        // …but a string projection interpolates.
        let c = parse("add ${' '.join(components)}");
        assert!(!c.is_literal());
    }
}
