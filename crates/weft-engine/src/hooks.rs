//! Hook planning, ordering, and execution over the composed graph.
//!
//! Hooks are patch-scoped side-effects (`weft_core::Hook`) that run before
//! (`pre`) or after (`post`) the render. A hook follows its patch's **node**
//! in the composed graph: it runs only when the node is active (its `when`
//! passes and no dependency is skipped), sees the node's frame answers, and
//! post-hooks run inside the node's mount. Within a phase the plan is
//! topologically ordered: the base order is the composed render order then
//! in-patch declaration order, with explicit `after` edges layered on top.
//!
//! Hook ids are **namespaced by frame**: root hooks keep their id, a hook of
//! include `web` is `web/<id>`, nested `web/svc/<id>`. `after` and
//! `inputs: hook:` references are written relative to the referencing hook's
//! frame — `after: ["x"]` inside `web` means `web/x`; a root hook may write
//! `after: ["web/pnpm-install"]`. A child never names its parent's hooks.
//!
//! `foreach` (integration) patches are never active nodes of a plain graph
//! (they apply once per instance in the engine's foreach phase), so their
//! hooks are not planned.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobMatcher};
use weft_core::render::{ExprEval, Framed};
use weft_core::{AnswerId, AnswerSet, Hook, HookId, HookInput, HookPhase, Patch, PatchId};

use crate::compose;
use crate::template::Template;

/// What changed between the previous render and this one, root-relative.
pub struct ChangeSet {
    pub paths: BTreeSet<Utf8PathBuf>,
    pub answers: BTreeSet<AnswerId>,
}

/// A hook of an active node, placed in the node's frame.
pub struct PlannedHook<'t> {
    pub hook: &'t Hook,
    /// Namespaced id: root hooks keep their id; a hook of include `web` is
    /// `web/<id>`, nested `web/svc/<id>` (a repeat instance's segment is
    /// `<include>:<key>`).
    pub id: String,
    /// The `(include, key)` chain from the root; empty for root hooks.
    pub frame: Vec<(String, String)>,
    /// Where the frame's files land, root-relative. Post-hooks run here.
    pub mount: Utf8PathBuf,
    /// The frame's answers: what `when` and the command see.
    pub answers: AnswerSet,
}

impl PlannedHook<'_> {
    /// A reference written inside this hook's frame, as a namespaced id.
    fn qualify(&self, id: &HookId) -> String {
        namespaced(&frame_prefix(&self.frame), &id.0)
    }

    /// The namespace this hook lives in (`web`, `web/svc`); empty at the root.
    pub fn frame_prefix(&self) -> String {
        frame_prefix(&self.frame)
    }
}

/// The active hooks of a composed render, split by phase and already ordered.
pub struct Plan<'t> {
    pub pre: Vec<PlannedHook<'t>>,
    pub post: Vec<PlannedHook<'t>>,
}

/// The namespace prefix of a frame: include names joined by `/`; a repeat
/// instance (key ≠ include name) contributes `<include>:<key>`, so two
/// instances of one include never collide.
fn frame_prefix(frame: &[(String, String)]) -> String {
    frame
        .iter()
        .map(|(include, key)| {
            if key == include {
                include.clone()
            } else {
                format!("{include}:{key}")
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn namespaced(prefix: &str, id: &str) -> String {
    if prefix.is_empty() {
        id.to_owned()
    } else {
        format!("{prefix}/{id}")
    }
}

/// Plan the hooks of a composed render: every hook of every active node
/// (see [`compose::active_nodes`]), in composed render order, then
/// `after`-ordered per phase.
pub fn plan<'a>(
    template: &'a Template,
    answers: &'a AnswerSet,
    parts: &'a [compose::ComposedPart<'a>],
    eval: &dyn ExprEval,
) -> Result<Plan<'a>> {
    let active = compose::active_nodes(&template.patches, answers, parts, eval)?;
    let nodes = compose::graph_nodes(&template.patches, answers, parts);
    let framed: Vec<Framed> = nodes
        .iter()
        .map(|n| Framed {
            id: n.id,
            depends_on: &n.depends_on,
            patch: n.patch,
            answers: n.answers,
            mount: &n.mount,
        })
        .collect();
    let order = weft_core::render::framed_order(&framed).context("ordering the composed graph")?;
    let by_id: BTreeMap<PatchId, &compose::GraphNode> = nodes.iter().map(|n| (n.id, n)).collect();

    let mut pre = Vec::new();
    let mut post = Vec::new();
    for node in order {
        if !active.contains(&node.id) {
            continue;
        }
        let node = by_id[&node.id];
        let prefix = frame_prefix(&node.frame);
        for hook in &node.patch.meta.hooks {
            let planned = PlannedHook {
                hook,
                id: namespaced(&prefix, &hook.id.0),
                frame: node.frame.clone(),
                mount: node.mount.clone(),
                answers: node.answers.clone(),
            };
            match hook.phase {
                HookPhase::Pre => pre.push(planned),
                HookPhase::Post => post.push(planned),
            }
        }
    }
    Ok(Plan {
        pre: order_phase(pre)?,
        post: order_phase(post)?,
    })
}

/// Topologically order one phase's hooks: base order is the input order
/// (composed render order + declaration order); `after` adds edges, resolved
/// to namespaced ids. References to hooks outside this set (other phase /
/// inactive node / unknown) are ignored.
fn order_phase(hooks: Vec<PlannedHook<'_>>) -> Result<Vec<PlannedHook<'_>>> {
    let index: BTreeMap<&str, usize> = hooks
        .iter()
        .enumerate()
        .map(|(i, h)| (h.id.as_str(), i))
        .collect();
    // indegree over in-set `after` edges
    let mut indegree = vec![0usize; hooks.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); hooks.len()];
    for (i, h) in hooks.iter().enumerate() {
        for dep in &h.hook.after {
            if let Some(&j) = index.get(h.qualify(dep).as_str()) {
                indegree[i] += 1;
                dependents[j].push(i);
            }
        }
    }
    // Kahn's algorithm; ready set drained in base-order (smallest index first)
    // for a deterministic, stable ordering.
    let mut ready: BTreeSet<usize> = (0..hooks.len()).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(hooks.len());
    while let Some(&i) = ready.iter().next() {
        ready.remove(&i);
        order.push(i);
        for &d in &dependents[i] {
            indegree[d] -= 1;
            if indegree[d] == 0 {
                ready.insert(d);
            }
        }
    }
    if order.len() != hooks.len() {
        let stuck: Vec<&str> = hooks
            .iter()
            .enumerate()
            .filter(|(i, _)| indegree[*i] > 0)
            .map(|(_, h)| h.id.as_str())
            .collect();
        bail!("hook `after` cycle among: {}", stuck.join(", "));
    }
    let mut slots: Vec<Option<PlannedHook>> = hooks.into_iter().map(Some).collect();
    Ok(order
        .into_iter()
        .map(|i| slots[i].take().expect("each index ordered once"))
        .collect())
}

/// Restrict post-hooks to those that should fire on `weft update`: the hook
/// is on (`when`, in its frame) and one of its inputs changed. Glob inputs
/// match `changes.paths` under the hook's mount with the mount stripped;
/// answer inputs consult the changed answers of the hook's frame (a frame
/// with no entry has no changed answers); `hook:` inputs name planned
/// post-hooks. A hook with no inputs never re-fires on update (only on
/// initial scaffold), which mirrors "copy-only" tasks.
pub fn fire_on_update_planned<'p, 't>(
    post: &'p [PlannedHook<'t>],
    changes: &ChangeSet,
    changed_answers_by_frame: &BTreeMap<Vec<(String, String)>, BTreeSet<AnswerId>>,
    eval: &dyn ExprEval,
) -> Result<Vec<&'p PlannedHook<'t>>> {
    let planned: BTreeSet<&str> = post.iter().map(|h| h.id.as_str()).collect();
    let no_answers = BTreeSet::new();
    let mut fire = Vec::new();
    for hook in post {
        // hook-level gate first
        if let Some(when) = &hook.hook.when {
            if !eval.eval_bool(when, &hook.answers)? {
                continue;
            }
        }
        let changed_answers = changed_answers_by_frame
            .get(&hook.frame)
            .unwrap_or(&no_answers);
        let changed = hook.hook.inputs.iter().try_fold(false, |acc, input| {
            anyhow::Ok(
                acc || match input {
                    HookInput::Glob(pattern) => {
                        let matcher = glob_matcher(pattern)?;
                        changes
                            .paths
                            .iter()
                            .filter_map(|p| p.strip_prefix(&hook.mount).ok())
                            .any(|p| matcher.is_match(p.as_std_path()))
                    }
                    HookInput::Answer(id) => changed_answers.contains(id),
                    HookInput::Hook(id) => planned.contains(hook.qualify(id).as_str()),
                },
            )
        })?;
        if changed {
            fire.push(hook);
        }
    }
    Ok(fire)
}

/// Run planned hooks in order with `sh -c`, stopping at the first failure.
/// Pre-hooks run at `dest` (mounts do not exist yet); post-hooks run inside
/// their mount. Each hook's `when` and command are evaluated against its
/// frame's answers. A non-zero exit aborts with the hook's label.
pub fn run_planned<'p, 't: 'p>(
    hooks: impl IntoIterator<Item = &'p PlannedHook<'t>>,
    dest: &Utf8Path,
    eval: &dyn ExprEval,
) -> Result<()> {
    for planned in hooks {
        let hook = planned.hook;
        if let Some(when) = &hook.when {
            if !eval
                .eval_bool(when, &planned.answers)
                .with_context(|| format!("evaluating when of hook `{}`", planned.id))?
            {
                continue;
            }
        }
        let cwd = match hook.phase {
            HookPhase::Pre => dest.to_owned(),
            HookPhase::Post if planned.mount.as_str().is_empty() => dest.to_owned(),
            HookPhase::Post => dest.join(&planned.mount),
        };
        run_command(
            &hook.action,
            &format!("hook {} — {}", planned.id, hook.label),
            &cwd,
            &planned.answers,
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

/// One hook of the static composed graph: its namespaced id, the patch it
/// belongs to, and the namespace its own references resolve in.
struct StaticHook<'t> {
    id: String,
    prefix: String,
    patch: &'t Patch,
    hook: &'t Hook,
}

/// Every hook of `template`'s composed graph: root patches under `prefix`,
/// then each include's (single or repeat) under `<prefix>/<include>`,
/// recursively.
fn static_hooks<'t>(template: &'t Template, prefix: &str, out: &mut Vec<StaticHook<'t>>) {
    for patch in &template.patches {
        for hook in &patch.meta.hooks {
            out.push(StaticHook {
                id: namespaced(prefix, &hook.id.0),
                prefix: prefix.to_owned(),
                patch,
                hook,
            });
        }
    }
    for inc in &template.includes {
        static_hooks(&inc.template, &namespaced(prefix, &inc.decl.name), out);
    }
}

/// Static validation of a template's hooks over its composed graph (for
/// `weft check`): the root patches' hooks have unique ids, `inputs` only on
/// post-hooks, parseable `when`/command expressions, known answers, and
/// `after`/`inputs:hook:` references that resolve against the composed
/// namespaced id set (root ids plus every include's `<include>/<id>`,
/// recursively); no `after` cycle anywhere in the composed set. Included
/// templates' own hooks are validated by their own check (which `weft
/// check` recurses into), not repeated here. Returns human-readable issue
/// strings.
pub fn validate_all(template: &Template) -> Vec<String> {
    let mut issues = Vec::new();
    let mut all: Vec<StaticHook> = Vec::new();
    static_hooks(template, "", &mut all);
    let ids: BTreeSet<&str> = all.iter().map(|h| h.id.as_str()).collect();
    let own: Vec<&StaticHook> = all.iter().filter(|h| h.prefix.is_empty()).collect();
    let own_ids: BTreeSet<&str> = own.iter().map(|h| h.id.as_str()).collect();
    if own_ids.len() != own.len() {
        issues.push("duplicate hook ids across patches".to_owned());
    }
    let declared_answers: BTreeSet<&AnswerId> =
        template.manifest.questions.iter().map(|q| &q.id).collect();
    for StaticHook {
        hook,
        patch,
        prefix,
        ..
    } in own.iter().copied()
    {
        for a in &hook.after {
            if !ids.contains(namespaced(prefix, &a.0).as_str()) {
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
                    if !ids.contains(namespaced(prefix, &id.0).as_str()) {
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
    // cycle detection over the composed `after` graph
    let index: BTreeMap<&str, usize> = all
        .iter()
        .enumerate()
        .map(|(i, h)| (h.id.as_str(), i))
        .collect();
    let mut indegree = vec![0usize; all.len()];
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); all.len()];
    for (i, h) in all.iter().enumerate() {
        for a in &h.hook.after {
            if let Some(&j) = index.get(namespaced(&h.prefix, &a.0).as_str()) {
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
        let stuck: Vec<&str> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| indegree[*i] > 0)
            .map(|(_, h)| h.id.as_str())
            .collect();
        issues.push(format!("hook `after` cycle among: {}", stuck.join(", ")));
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

    fn planned<'t>(hook: &'t Hook, frame: &[(&str, &str)], mount: &str) -> PlannedHook<'t> {
        let frame: Vec<(String, String)> = frame
            .iter()
            .map(|(i, k)| ((*i).to_owned(), (*k).to_owned()))
            .collect();
        PlannedHook {
            hook,
            id: namespaced(&frame_prefix(&frame), &hook.id.0),
            frame,
            mount: Utf8PathBuf::from(mount),
            answers: AnswerSet::new(),
        }
    }

    fn order(hooks: &[Hook]) -> Vec<String> {
        order_phase(hooks.iter().map(|h| planned(h, &[], "")).collect())
            .unwrap()
            .iter()
            .map(|h| h.id.clone())
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
        assert!(order_phase(hs.iter().map(|h| planned(h, &[], "")).collect()).is_err());
    }

    #[test]
    fn namespaced_after_orders_a_root_hook_after_a_child_hook() {
        // Root `format` declares `after: ["web/install"]`; the child's own
        // `install` is namespaced `web/install` and the child's `lint`
        // writes `after: ["install"]` relative to its frame.
        let format = hook("format", &["web/install"]);
        let lint = hook("lint", &["install"]);
        let install = hook("install", &[]);
        let ordered = order_phase(vec![
            planned(&format, &[], ""),
            planned(&lint, &[("web", "web")], "apps/web"),
            planned(&install, &[("web", "web")], "apps/web"),
        ])
        .unwrap();
        let ids: Vec<&str> = ordered.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["web/install", "format", "web/lint"]);
        // Nested frames namespace through the chain.
        assert_eq!(
            planned(&install, &[("web", "web"), ("svc", "svc")], "").id,
            "web/svc/install"
        );
        // Repeat instances stay distinct per key.
        assert_eq!(
            planned(&install, &[("connector", "github")], "").id,
            "connector:github/install"
        );
    }

    #[test]
    fn child_glob_input_matches_under_its_mount() {
        let mut install = hook("install", &[]);
        install.inputs = vec![HookInput::Glob("package.json".into())];
        let post = [
            planned(&install, &[], ""),
            planned(&install, &[("web", "web")], "apps/web"),
        ];
        let changes = ChangeSet {
            paths: [Utf8PathBuf::from("apps/web/package.json")].into(),
            answers: BTreeSet::new(),
        };
        let fired =
            fire_on_update_planned(&post, &changes, &BTreeMap::new(), &weft_lang::StarlarkEval)
                .unwrap();
        let ids: Vec<&str> = fired.iter().map(|h| h.id.as_str()).collect();
        // The root's `package.json` did not change; the child's did.
        assert_eq!(ids, ["web/install"]);
    }

    #[test]
    fn answer_inputs_use_the_frames_changed_set() {
        let mut hook = hook("cfg", &[]);
        hook.inputs = vec![HookInput::Answer("port".into())];
        let post = [
            planned(&hook, &[], ""),
            planned(&hook, &[("web", "web")], "apps/web"),
        ];
        let changes = ChangeSet {
            paths: BTreeSet::new(),
            answers: BTreeSet::new(),
        };
        let mut by_frame = BTreeMap::new();
        by_frame.insert(
            vec![("web".to_owned(), "web".to_owned())],
            [AnswerId::from("port")].into(),
        );
        let fired =
            fire_on_update_planned(&post, &changes, &by_frame, &weft_lang::StarlarkEval).unwrap();
        let ids: Vec<&str> = fired.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["web/cfg"]);
    }

    /// A parent mounting a child at `apps/web`; the child has one `guard`
    /// pre-hook and one `install` post-hook, the parent one `format`
    /// post-hook `after: ["web/install"]` and a gated-off `deploy` hook.
    fn composed_fixture() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let child = root.join("child");
        std::fs::create_dir_all(child.join("patches")).unwrap();
        std::fs::write(
            child.join("weft.toml"),
            "[template]\nname = \"child\"\nweft-version = \"0.1\"\n",
        )
        .unwrap();
        std::fs::write(
            child.join("patches/base.json"),
            r#"{"ops":[{"op":"create_file","path":"package.json","content":["{}"]}],
                "hooks":[
                  {"id":"guard","phase":"pre","effect":"check","label":"guard","action":"true"},
                  {"id":"install","phase":"post","effect":"setup","label":"install","action":"true"}
                ]}"#,
        )
        .unwrap();
        let parent = root.join("parent");
        std::fs::create_dir_all(parent.join("patches")).unwrap();
        std::fs::write(
            parent.join("weft.toml"),
            "[template]\nname = \"parent\"\nweft-version = \"0.1\"\n\
             [[question]]\nid = \"deploy\"\nkind = \"bool\"\n\
             [[include]]\nname = \"web\"\ntemplate = \"../child\"\npath = \"apps/web\"\n",
        )
        .unwrap();
        std::fs::write(
            parent.join("patches/base.json"),
            r#"{"ops":[{"op":"create_file","path":"README.md","content":["hi"]}],
                "hooks":[{"id":"format","phase":"post","effect":"setup","label":"format",
                          "action":"true","after":["web/install"]}]}"#,
        )
        .unwrap();
        std::fs::write(
            parent.join("patches/deploy.json"),
            r#"{"when":"deploy","hooks":[{"id":"deploy","phase":"post","effect":"deploy",
                "label":"deploy","action":"true"}]}"#,
        )
        .unwrap();
        (dir, parent)
    }

    #[test]
    fn plan_follows_the_composed_graph() {
        let (_dir, parent) = composed_fixture();
        let template = Template::load(&parent).unwrap();
        let eval = weft_lang::StarlarkEval;
        let mut answers = AnswerSet::new();
        answers.insert("deploy".into(), weft_core::Value::Bool(false));
        let parts = compose::resolve_child_parts(
            &template,
            &answers,
            compose::SecretMode::Resolve,
            &eval,
            &mut crate::interact::NonInteractive,
        )
        .unwrap();
        let plan = plan(&template, &answers, &parts, &eval).unwrap();
        let pre: Vec<&str> = plan.pre.iter().map(|h| h.id.as_str()).collect();
        let post: Vec<&str> = plan.post.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(pre, ["web/guard"]);
        // `format` waits for `web/install`; the gated-off `deploy` is absent.
        assert_eq!(post, ["web/install", "format"]);
        let install = &plan.post[0];
        assert_eq!(install.frame, [("web".to_owned(), "web".to_owned())]);
        assert_eq!(install.mount, Utf8PathBuf::from("apps/web"));
        assert!(plan.post[1].frame.is_empty());
        assert_eq!(plan.post[1].answers, answers);
        // Root hooks resolve `after` across the include boundary in check.
        assert!(validate_all(&template).is_empty());
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
            narrowing: Default::default(),
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
