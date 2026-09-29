//! Deterministic rendering: `render(base, answers, patches) -> Tree`.
//!
//! Same inputs ⇒ byte-identical output tree. Patch application order is the
//! topological order of the dependency DAG with ties broken by patch id, so
//! the input ordering of the patch slice never matters.

use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};

use crate::draft::{Draft, NearMiss, Trace};
use crate::id::{AnswerId, PatchId};
use crate::patch::Patch;
use crate::question::{AnswerKind, Question, StarlarkExpr};
use crate::segment::{join_lines, Content, Line, Segment, TemplatePath};
use crate::tree::Tree;
use crate::value::{AnswerSet, Value};

/// Expression evaluation, implemented by `weft-lang`. Core defines the trait
/// so it stays free of the Starlark dependency (and tests can stub it).
pub trait ExprEval {
    fn eval(&self, expr: &StarlarkExpr, answers: &AnswerSet) -> Result<Value, EvalError>;

    /// Starlark truthiness on the evaluated result.
    fn eval_bool(&self, expr: &StarlarkExpr, answers: &AnswerSet) -> Result<bool, EvalError> {
        Ok(match self.eval(expr, answers)? {
            Value::Bool(b) => b,
            Value::Int(i) => i != 0,
            Value::String(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Secret(_) => {
                return Err(EvalError {
                    message: "secret values cannot be used in conditions".into(),
                })
            }
        })
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct EvalError {
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no answer for question `{0}` and it has no default")]
    Unanswered(AnswerId),
    #[error("answer `{id}` has type {got}, expected {expected}")]
    TypeMismatch {
        id: AnswerId,
        expected: &'static str,
        got: &'static str,
    },
    #[error("answer `{id}` value {value:?} is not one of the declared choices")]
    InvalidChoice { id: AnswerId, value: String },
    #[error("answer `{id}`: {value:?} is blocked by {by}")]
    Blocked {
        id: AnswerId,
        value: String,
        by: String,
    },
    #[error("answer `{id}` is locked to {value} by {by}; drop the explicit answer")]
    Locked {
        id: AnswerId,
        value: String,
        by: String,
    },
    #[error(
        "answer/expression `{0}` is a list; project it with an expression \
         (e.g. `', '.join(x)`) before using it in a path or content segment"
    )]
    ListInContent(String),
    #[error("evaluating `{expr}` for `{context}`: {source}")]
    Eval {
        expr: String,
        context: String,
        source: EvalError,
    },
    #[error("unknown answer reference `{0}` in a path or content segment")]
    UnknownAnswer(AnswerId),
    #[error("patch {patch} depends on unknown patch {dep}")]
    UnknownDep { patch: PatchId, dep: PatchId },
    #[error("patch dependency cycle involving {0}")]
    DependencyCycle(PatchId),
    #[error("patch {patch}: create_file target `{path}` already exists")]
    CreateExists { patch: PatchId, path: Utf8PathBuf },
    #[error("patch {patch}: `{path}` does not exist")]
    MissingFile { patch: PatchId, path: Utf8PathBuf },
    #[error("patch {patch}: rename target `{path}` already exists")]
    RenameExists { patch: PatchId, path: Utf8PathBuf },
    #[error(
        "patch {patch}: hunk {hunk} does not match `{path}` (context not found); \
         the file diverged from what the patch was recorded against"
    )]
    HunkNoMatch {
        patch: PatchId,
        path: Utf8PathBuf,
        hunk: usize,
        /// Where the pattern nearly matched (traced renders only).
        near: Option<Box<NearMiss>>,
    },
    #[error("patch {patch}: hunk {hunk} matches `{path}` in {count} places; context is ambiguous")]
    HunkAmbiguous {
        patch: PatchId,
        path: Utf8PathBuf,
        hunk: usize,
        count: usize,
    },
    #[error("rendered path {0:?} is invalid (must be relative, `/`-separated, no `..`)")]
    InvalidPath(String),
    #[error(
        "patch {patch}: `{path}` is a binary file; text hunks cannot apply — \
         replace it wholesale (delete + create_binary_file)"
    )]
    BinaryModify { patch: PatchId, path: Utf8PathBuf },
    #[error("patch {patch}: create_binary_file `{path}` carries invalid base64 data")]
    InvalidBinaryData { patch: PatchId, path: Utf8PathBuf },
    #[error(
        "slot `{0}` is declared where only text can go; a slot is a line of its own in \
         `create_file` content or in a hunk's `added` lines, and slots do not nest"
    )]
    MisplacedSlot(String),
    #[error(
        "patch {patch}: `{slot}` is not a valid slot name for `{path}` (ASCII letters, digits, \
         `-`, `_` and `.`)"
    )]
    InvalidSlotName {
        patch: PatchId,
        path: Utf8PathBuf,
        slot: String,
    },
    #[error("patch {patch}: `{path}` already has a slot `{slot}`")]
    DuplicateSlot {
        patch: PatchId,
        path: Utf8PathBuf,
        slot: String,
    },
    #[error("patch {patch}: `{path}` has no slot `{slot}`")]
    UnknownSlot {
        patch: PatchId,
        path: Utf8PathBuf,
        slot: String,
    },
    #[error("patch {patch}: key {key:?} for slot `{slot}` of `{path}` must be one non-empty line")]
    InvalidSlotKey {
        patch: PatchId,
        path: Utf8PathBuf,
        slot: String,
        key: String,
    },
    #[error("patch {patch}: its fill of slot `{slot}` of `{path}` adds no lines")]
    EmptySlotFill {
        patch: PatchId,
        path: Utf8PathBuf,
        slot: String,
    },
    #[error(
        "patches {} and {} both fill slot `{}` of `{}` under key `{}`",
        .0.first, .0.second, .0.slot, .0.path, .0.key
    )]
    DuplicateSlotKey(Box<SlotClash>),
    #[error(
        "patch {editor} changes `{path}`, which patch {owner} leaves out while its slots are \
         empty (`omit_when_empty`); fill one of its slots instead, or drop `omit_when_empty`"
    )]
    OmittedFileEdited {
        path: Utf8PathBuf,
        owner: PatchId,
        editor: PatchId,
    },
    #[error("`{path}` lost slot `{slot}`: a hunk removed the line it sits on")]
    SlotLost { path: Utf8PathBuf, slot: String },
}

impl RenderError {
    /// The graph node an op-level error is about, if any.
    pub fn patch(&self) -> Option<PatchId> {
        match self {
            RenderError::CreateExists { patch, .. }
            | RenderError::MissingFile { patch, .. }
            | RenderError::RenameExists { patch, .. }
            | RenderError::HunkNoMatch { patch, .. }
            | RenderError::HunkAmbiguous { patch, .. }
            | RenderError::BinaryModify { patch, .. }
            | RenderError::InvalidBinaryData { patch, .. }
            | RenderError::InvalidSlotName { patch, .. }
            | RenderError::DuplicateSlot { patch, .. }
            | RenderError::UnknownSlot { patch, .. }
            | RenderError::InvalidSlotKey { patch, .. }
            | RenderError::EmptySlotFill { patch, .. }
            | RenderError::UnknownDep { patch, .. } => Some(*patch),
            RenderError::DuplicateSlotKey(clash) => Some(clash.second),
            RenderError::OmittedFileEdited { editor, .. } => Some(*editor),
            _ => None,
        }
    }
}

/// Two contributions under one key in one slot ([`RenderError::DuplicateSlotKey`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotClash {
    pub path: Utf8PathBuf,
    pub slot: String,
    pub key: String,
    /// The node whose fill came first, and the one that clashed with it.
    pub first: PatchId,
    pub second: PatchId,
}

/// Resolve the full answer set for `questions` from layered `provided`
/// answers: evaluate `when` gates in declaration order, type-check provided
/// values, evaluate `default` expressions for the rest. Every value passes
/// through its question's narrowing ([`narrow`]), and a locked question
/// accepts a provided value only when it equals the lock.
///
/// A question whose `when` is false is not prompted, but if it has a
/// `default` it still resolves to that default so its name stays defined for
/// later `when`/`default`/content expressions. (Starlark binds free names
/// eagerly — even the untaken side of `and`/`or` — so an absent name would
/// raise a `NameError` rather than short-circuit.) A gated-off question with
/// no default is simply absent.
pub fn resolve_answers(
    questions: &[Question],
    provided: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<AnswerSet, RenderError> {
    let mut resolved = AnswerSet::new();
    for q in questions {
        let gated_off = if let Some(when) = &q.when {
            !eval
                .eval_bool(when, &resolved)
                .map_err(|e| RenderError::Eval {
                    expr: when.0.clone(),
                    context: format!("when of question `{}`", q.id),
                    source: e,
                })?
        } else {
            false
        };
        // A provided value for a gated-off question is ignored (the gate
        // decided it doesn't apply); its default, if any, still stands in.
        if !gated_off {
            if let Some(value) = provided.get(&q.id) {
                let value = check_type(
                    &q.id,
                    &q.kind,
                    narrow(q, value.clone(), ValueOrigin::Input)?,
                )?;
                let value = match &q.default {
                    Some(lock) if q.narrowing.locked => {
                        let lock = eval.eval(lock, &resolved).map_err(|e| RenderError::Eval {
                            expr: lock.0.clone(),
                            context: format!("lock of question `{}`", q.id),
                            source: e,
                        })?;
                        let lock =
                            check_type(&q.id, &q.kind, narrow(q, lock, ValueOrigin::Default)?)?;
                        if !same_answer(&value, &lock) {
                            return Err(RenderError::Locked {
                                id: q.id.clone(),
                                value: display_value(&lock),
                                by: q.narrowing.refiners(),
                            });
                        }
                        lock
                    }
                    _ => value,
                };
                resolved.insert(q.id.clone(), value);
                continue;
            }
        }
        if let Some(default) = &q.default {
            match eval.eval(default, &resolved) {
                Ok(value) => {
                    let value =
                        check_type(&q.id, &q.kind, narrow(q, value, ValueOrigin::Default)?)?;
                    resolved.insert(q.id.clone(), value);
                }
                // A gated-off question's default is a best-effort convenience
                // (keep the name defined); if it can't compute, leave the name
                // absent rather than failing the whole resolve.
                Err(_) if gated_off => {}
                Err(e) => {
                    return Err(RenderError::Eval {
                        expr: default.0.clone(),
                        context: format!("default of question `{}`", q.id),
                        source: e,
                    })
                }
            }
        } else if q.nothing_to_choose() {
            // Narrowed down to its fixed choices: decided without asking.
            let value = narrow(q, Value::List(Vec::new()), ValueOrigin::Default)?;
            resolved.insert(q.id.clone(), value);
        } else if !gated_off {
            return Err(RenderError::Unanswered(q.id.clone()));
        }
    }
    Ok(resolved)
}

/// Where a candidate value for a question came from (see [`narrow`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueOrigin {
    /// Supplied from outside: a flag, an answers file or JSON, a prompt, a
    /// preset, a bind, or an answer the project stored.
    Input,
    /// The question's default (or lock) expression.
    Default,
}

/// Apply an inherited question's narrowing (ADR-0002) to a candidate value,
/// before it is type-checked:
///
/// - an input may not pick a blocked choice;
/// - a default drops blocked multichoice options (a blocked `choice` default
///   is still an error: there is nothing to fall back to);
/// - a multichoice gains its fixed choices, appended in declared order after
///   the selection, whose own order is kept.
///
/// A value of the wrong kind passes through untouched for the type check.
pub fn narrow(q: &Question, value: Value, origin: ValueOrigin) -> Result<Value, RenderError> {
    let n = &q.narrowing;
    if n.blocked.is_empty() && n.fixed.is_empty() {
        return Ok(value);
    }
    let blocked = |id: &AnswerId, s: &str| RenderError::Blocked {
        id: id.clone(),
        value: s.to_owned(),
        by: n.refiners(),
    };
    match (&q.kind, value) {
        (AnswerKind::Choice { .. }, Value::String(s)) => {
            if n.blocked.contains(&s) {
                return Err(blocked(&q.id, &s));
            }
            Ok(Value::String(s))
        }
        (AnswerKind::MultiChoice { .. }, Value::List(items)) => {
            let mut out = Vec::with_capacity(items.len() + n.fixed.len());
            for item in items {
                if let Value::String(s) = &item {
                    if n.blocked.contains(s) {
                        match origin {
                            ValueOrigin::Input => return Err(blocked(&q.id, s)),
                            ValueOrigin::Default => continue,
                        }
                    }
                }
                out.push(item);
            }
            for f in &n.fixed {
                if !out.iter().any(|v| matches!(v, Value::String(s) if s == f)) {
                    out.push(Value::String(f.clone()));
                }
            }
            Ok(Value::List(out))
        }
        (_, value) => Ok(value),
    }
}

/// Equal answers; a multichoice compares as a set, since the order of a
/// selection means nothing to a lock or to a prompt's default.
pub fn same_answer(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            x.iter().all(|v| y.contains(v)) && y.iter().all(|v| x.contains(v))
        }
        _ => a == b,
    }
}

/// A value as an error message shows it: strings quoted, lists bracketed.
fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        Value::List(items) => {
            let items: Vec<String> = items.iter().map(display_value).collect();
            format!("[{}]", items.join(", "))
        }
        other => other.render_text(),
    }
}

fn check_type(id: &AnswerId, kind: &AnswerKind, value: Value) -> Result<Value, RenderError> {
    let ok = matches!(
        (kind, &value),
        (AnswerKind::String, Value::String(_))
            | (AnswerKind::Bool, Value::Bool(_))
            | (AnswerKind::Int, Value::Int(_))
            | (AnswerKind::Choice { .. }, Value::String(_))
            | (AnswerKind::MultiChoice { .. }, Value::List(_))
            | (AnswerKind::Secret { .. }, Value::Secret(_))
    );
    if !ok {
        return Err(RenderError::TypeMismatch {
            id: id.clone(),
            expected: kind.name(),
            got: value.kind_name(),
        });
    }
    match (kind, &value) {
        (AnswerKind::Choice { choices }, Value::String(s)) => {
            if !choices.contains(s) {
                return Err(RenderError::InvalidChoice {
                    id: id.clone(),
                    value: s.clone(),
                });
            }
        }
        (AnswerKind::MultiChoice { choices }, Value::List(items)) => {
            for item in items {
                let s = match item {
                    Value::String(s) => s,
                    other => {
                        return Err(RenderError::TypeMismatch {
                            id: id.clone(),
                            expected: "string (multichoice element)",
                            got: other.kind_name(),
                        })
                    }
                };
                if !choices.contains(s) {
                    return Err(RenderError::InvalidChoice {
                        id: id.clone(),
                        value: s.clone(),
                    });
                }
            }
        }
        _ => {}
    }
    Ok(value)
}

/// Which questions still need answers (their `when` passes, no provided value,
/// no default). Used by the engine to drive interactive prompting.
pub fn unanswered<'q>(
    questions: &'q [Question],
    provided: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Vec<&'q Question>, RenderError> {
    let mut resolved = AnswerSet::new();
    let mut missing = Vec::new();
    for q in questions {
        let gated_off = if let Some(when) = &q.when {
            // Gate evaluation uses answers resolved so far; questions after a
            // missing answer may be mis-gated, so callers should re-run after
            // filling in the missing ones (the engine prompts in order).
            match eval.eval_bool(when, &resolved) {
                Ok(asked) => !asked,
                // The gate can't be evaluated yet (it references a not-yet-
                // resolved missing answer): treat this question as not-missing
                // for now — a re-run after the missing ones are filled will
                // classify it. Only a genuine error (nothing else missing)
                // surfaces.
                Err(_) if !missing.is_empty() => continue,
                Err(e) => {
                    return Err(RenderError::Eval {
                        expr: when.0.clone(),
                        context: format!("when of question `{}`", q.id),
                        source: e,
                    })
                }
            }
        } else {
            false
        };
        if !gated_off {
            if let Some(v) = provided.get(&q.id) {
                resolved.insert(q.id.clone(), v.clone());
                continue;
            }
        }
        if let Some(default) = &q.default {
            match eval.eval(default, &resolved) {
                Ok(value) => {
                    resolved.insert(q.id.clone(), value);
                }
                // A gated-off default is best-effort (keeps the name defined);
                // a default referencing an already-missing answer can't be
                // computed yet. Either way, don't fail the pass.
                Err(_) if gated_off || !missing.is_empty() => {}
                Err(e) => {
                    return Err(RenderError::Eval {
                        expr: default.0.clone(),
                        context: format!("default of question `{}`", q.id),
                        source: e,
                    })
                }
            }
        } else if !gated_off {
            missing.push(q);
        }
    }
    Ok(missing)
}

/// A patch placed in a frame of a composed graph: it renders against
/// `answers` and its paths land under `mount`. `id`/`depends_on` are the
/// node's identity *in the graph* — for a child template's patch mounted
/// through an include they are keyed ids ([`PatchId::keyed`]), so the same
/// patch mounted twice is two nodes.
#[derive(Clone, Copy)]
pub struct Framed<'a> {
    pub id: PatchId,
    pub depends_on: &'a [PatchId],
    pub patch: &'a Patch,
    pub answers: &'a AnswerSet,
    pub mount: &'a Utf8Path,
}

impl<'a> Framed<'a> {
    /// A patch in the root frame: its own id and dependencies, no mount.
    pub fn root(patch: &'a Patch, answers: &'a AnswerSet) -> Self {
        Framed {
            id: patch.id,
            depends_on: &patch.depends_on,
            patch,
            answers,
            mount: Utf8Path::new(""),
        }
    }
}

/// Deterministic application order for `patches`: topological by dependency,
/// ties broken by patch id. Errors on unknown deps or cycles.
pub fn patch_order(patches: &[Patch]) -> Result<Vec<&Patch>, RenderError> {
    let by_id: BTreeMap<PatchId, &Patch> = patches.iter().map(|p| (p.id, p)).collect();
    let order = topological(patches.iter().map(|p| (p.id, p.depends_on.as_slice())))?;
    Ok(order.into_iter().map(|id| by_id[&id]).collect())
}

/// [`patch_order`] over framed nodes (same tie-breaking, by node id).
pub fn framed_order<'n, 'a>(nodes: &'n [Framed<'a>]) -> Result<Vec<&'n Framed<'a>>, RenderError> {
    let by_id: BTreeMap<PatchId, &Framed<'a>> = nodes.iter().map(|n| (n.id, n)).collect();
    let order = topological(nodes.iter().map(|n| (n.id, n.depends_on)))?;
    Ok(order.into_iter().map(|id| by_id[&id]).collect())
}

fn topological<'d>(
    graph: impl Iterator<Item = (PatchId, &'d [PatchId])>,
) -> Result<Vec<PatchId>, RenderError> {
    let edges: Vec<(PatchId, &[PatchId])> = graph.collect();
    let known: BTreeSet<PatchId> = edges.iter().map(|(id, _)| *id).collect();
    let mut indegree: BTreeMap<PatchId, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<PatchId, Vec<PatchId>> = BTreeMap::new();
    for (id, deps) in &edges {
        indegree.entry(*id).or_insert(0);
        for dep in *deps {
            if !known.contains(dep) {
                return Err(RenderError::UnknownDep {
                    patch: *id,
                    dep: *dep,
                });
            }
            *indegree.entry(*id).or_insert(0) += 1;
            dependents.entry(*dep).or_default().push(*id);
        }
    }
    let mut ready: BTreeSet<PatchId> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(edges.len());
    while let Some(id) = ready.iter().next().copied() {
        ready.remove(&id);
        order.push(id);
        for dep in dependents.get(&id).cloned().unwrap_or_default() {
            let d = indegree.get_mut(&dep).expect("dependent tracked");
            *d -= 1;
            if *d == 0 {
                ready.insert(dep);
            }
        }
    }
    if order.len() != known.len() {
        let stuck = indegree
            .iter()
            .find(|(_, &d)| d > 0)
            .map(|(id, _)| *id)
            .expect("cycle leaves positive indegree");
        return Err(RenderError::DependencyCycle(stuck));
    }
    Ok(order)
}

/// Render the tree for `patches` under `answers` (already resolved via
/// [`resolve_answers`]). Patches whose `when` is false are skipped, along
/// with everything that depends on them (transitively).
pub fn render(
    patches: &[Patch],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Tree, RenderError> {
    let order = patch_order(patches)?;
    render_ordered(&order, answers, eval)
}

/// [`render`], plus where every line of the tree came from and where its
/// slots sit ([`Trace`]): what recording a patch against the tree needs.
pub fn render_traced(
    patches: &[Patch],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<(Tree, Trace), RenderError> {
    let order = patch_order(patches)?;
    let framed: Vec<Framed> = order.iter().map(|p| Framed::root(p, answers)).collect();
    let refs: Vec<&Framed> = framed.iter().collect();
    let (draft, _) = render_framed_into(Draft::traced(), &refs, eval)?;
    draft.finish_traced()
}

/// Render with an explicit, caller-chosen application order (must be a valid
/// linear extension of the dependency DAG — not verified here). Used by
/// `weft check` to prove that independent patches commute: applying them in
/// both orders must produce identical trees.
pub fn render_ordered(
    order: &[&Patch],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Tree, RenderError> {
    let framed: Vec<Framed> = order.iter().map(|p| Framed::root(p, answers)).collect();
    let refs: Vec<&Framed> = framed.iter().collect();
    Ok(render_framed(&refs, eval)?.0)
}

/// Render framed nodes in the given order (a linear extension of the
/// composed DAG). A node whose `when` is false — evaluated against *its
/// frame's* answers — is skipped along with everything that depends on it,
/// across frames: a parent hunk depending on a gated-off child patch is off
/// too. Returns the tree and the skipped node ids.
///
/// Foreach (integration) nodes are skipped here: they apply once per include
/// instance in the engine's composed rendering, after this pass.
pub fn render_framed(
    order: &[&Framed<'_>],
    eval: &dyn ExprEval,
) -> Result<(Tree, BTreeSet<PatchId>), RenderError> {
    let (draft, skipped) = render_framed_into(Draft::new(), order, eval)?;
    Ok((draft.finish()?, skipped))
}

/// [`render_framed`] onto `draft`, left unfinished so the caller can apply
/// more ops (foreach patches) or finish it traced.
pub fn render_framed_into(
    mut draft: Draft,
    order: &[&Framed<'_>],
    eval: &dyn ExprEval,
) -> Result<(Draft, BTreeSet<PatchId>), RenderError> {
    let mut skipped: BTreeSet<PatchId> = BTreeSet::new();
    for &node in order {
        if framed_active(node, &skipped, eval)? {
            draft.apply(node.id, node.patch, node.answers, eval, node.mount)?;
        } else {
            skipped.insert(node.id);
        }
    }
    Ok((draft, skipped))
}

/// The skip set [`render_framed`] would produce, without applying any ops.
pub fn framed_skipped(
    order: &[&Framed<'_>],
    eval: &dyn ExprEval,
) -> Result<BTreeSet<PatchId>, RenderError> {
    let mut skipped: BTreeSet<PatchId> = BTreeSet::new();
    for &node in order {
        if !framed_active(node, &skipped, eval)? {
            skipped.insert(node.id);
        }
    }
    Ok(skipped)
}

fn framed_active(
    node: &Framed<'_>,
    skipped: &BTreeSet<PatchId>,
    eval: &dyn ExprEval,
) -> Result<bool, RenderError> {
    if node.depends_on.iter().any(|d| skipped.contains(d)) || node.patch.foreach.is_some() {
        return Ok(false);
    }
    match &node.patch.when {
        None => Ok(true),
        Some(when) => eval
            .eval_bool(when, node.answers)
            .map_err(|e| RenderError::Eval {
                expr: when.0.clone(),
                context: format!("when of patch {}", node.patch.id.short()),
                source: e,
            }),
    }
}

fn render_segment(
    seg: &Segment,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<String, RenderError> {
    match seg {
        Segment::Literal(s) => Ok(s.clone()),
        Segment::Answer(id) => {
            let value = answers
                .get(id)
                .ok_or_else(|| RenderError::UnknownAnswer(id.clone()))?;
            if value.is_list() {
                return Err(RenderError::ListInContent(id.0.clone()));
            }
            Ok(value.render_text())
        }
        Segment::Expr(expr) => {
            let value = eval.eval(expr, answers).map_err(|e| RenderError::Eval {
                expr: expr.0.clone(),
                context: "content expression".into(),
                source: e,
            })?;
            if value.is_list() {
                return Err(RenderError::ListInContent(expr.0.clone()));
            }
            Ok(value.render_text())
        }
        // A slot line is only rendered as a slot where a patch adds lines
        // (see `Draft::apply`); anywhere else it has no text.
        Segment::Slot(decl) => Err(RenderError::MisplacedSlot(decl.slot.clone())),
    }
}

pub fn render_line(
    line: &Line,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<String, RenderError> {
    let mut out = String::new();
    for seg in &line.0 {
        out.push_str(&render_segment(seg, answers, eval)?);
    }
    Ok(out)
}

pub fn render_content(
    content: &Content,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<String, RenderError> {
    let lines: Result<Vec<String>, _> = content
        .0
        .iter()
        .map(|l| render_line(l, answers, eval))
        .collect();
    Ok(join_lines(&lines?))
}

/// Concatenate a segment sequence into a single string (paths, task commands).
pub fn render_segments(
    segments: &[Segment],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<String, RenderError> {
    let mut out = String::new();
    for seg in segments {
        out.push_str(&render_segment(seg, answers, eval)?);
    }
    Ok(out)
}

pub fn render_path(
    path: &TemplatePath,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Utf8PathBuf, RenderError> {
    let out = render_segments(&path.0, answers, eval)?;
    let invalid = out.is_empty()
        || out.starts_with('/')
        || out.ends_with('/')
        || out
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if invalid {
        return Err(RenderError::InvalidPath(out));
    }
    Ok(Utf8PathBuf::from(out))
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Literal-only evaluator for tests: resolves bare identifiers against the
    /// answer set and the literals `True`/`False`; no real Starlark.
    pub struct StubEval;

    impl ExprEval for StubEval {
        fn eval(&self, expr: &StarlarkExpr, answers: &AnswerSet) -> Result<Value, EvalError> {
            match expr.as_str() {
                "True" => Ok(Value::Bool(true)),
                "False" => Ok(Value::Bool(false)),
                name => answers
                    .get(&AnswerId(name.to_owned()))
                    .cloned()
                    .ok_or_else(|| EvalError {
                        message: format!("unknown name {name}"),
                    }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::StubEval;
    use super::*;
    use crate::patch::{Hunk, Op};

    fn answers(pairs: &[(&str, Value)]) -> AnswerSet {
        pairs
            .iter()
            .map(|(k, v)| (AnswerId::from(*k), v.clone()))
            .collect()
    }

    #[test]
    fn path_rendering_rejects_escapes() {
        let a = answers(&[("name", Value::String("../evil".into()))]);
        let path = TemplatePath(vec![
            Segment::Answer("name".into()),
            Segment::Literal("/x".into()),
        ]);
        assert!(matches!(
            render_path(&path, &a, &StubEval),
            Err(RenderError::InvalidPath(_))
        ));
    }

    #[test]
    fn when_false_skips_patch_and_dependents() {
        let base = Patch::new(
            vec![],
            None,
            vec![Op::CreateFile {
                path: TemplatePath::literal("a.txt"),
                omit_when_empty: vec![],
                content: Content::from_text("a\n"),
                mode: 0o644,
            }],
        );
        let gated = Patch::new(
            vec![base.id],
            Some(StarlarkExpr::from("use_docker")),
            vec![Op::CreateFile {
                path: TemplatePath::literal("Dockerfile"),
                omit_when_empty: vec![],
                content: Content::from_text("FROM x\n"),
                mode: 0o644,
            }],
        );
        let dependent = Patch::new(
            vec![gated.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal("Dockerfile"),
                hunks: vec![],
            }],
        );
        let patches = vec![dependent, gated, base];
        let on = answers(&[("use_docker", Value::Bool(true))]);
        let off = answers(&[("use_docker", Value::Bool(false))]);
        let tree_on = render(&patches, &on, &StubEval).unwrap();
        let tree_off = render(&patches, &off, &StubEval).unwrap();
        assert!(tree_on.get("Dockerfile".into()).is_some());
        assert!(tree_off.get("Dockerfile".into()).is_none());
        assert!(tree_off.get("a.txt".into()).is_some());
    }

    #[test]
    fn multichoice_validates_elements_against_choices() {
        let kind = AnswerKind::MultiChoice {
            choices: vec!["a".into(), "b".into()],
        };
        let ok = Value::List(vec![Value::String("a".into())]);
        assert!(check_type(&"c".into(), &kind, ok).is_ok());
        let bad = Value::List(vec![Value::String("z".into())]);
        assert!(matches!(
            check_type(&"c".into(), &kind, bad),
            Err(RenderError::InvalidChoice { .. })
        ));
        // a scalar where a list is expected is a type mismatch
        assert!(matches!(
            check_type(&"c".into(), &kind, Value::String("a".into())),
            Err(RenderError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn list_in_content_is_rejected() {
        let a = answers(&[("fonts", Value::List(vec![Value::String("besley".into())]))]);
        let content = Content(vec![Line(vec![Segment::Answer("fonts".into())])]);
        assert!(matches!(
            render_content(&content, &a, &StubEval),
            Err(RenderError::ListInContent(_))
        ));
    }

    #[test]
    fn resolve_answers_gates_and_defaults() {
        let questions = vec![
            Question {
                id: "use_docker".into(),
                kind: AnswerKind::Bool,
                prompt: None,
                description: None,
                example: None,
                default: Some(StarlarkExpr::from("True")),
                when: None,
                computed: false,
                section: None,
                narrowing: Default::default(),
            },
            Question {
                id: "registry".into(),
                kind: AnswerKind::String,
                prompt: None,
                description: None,
                example: None,
                default: None,
                when: Some(StarlarkExpr::from("use_docker")),
                computed: false,
                section: None,
                narrowing: Default::default(),
            },
        ];
        // Gated question unanswered -> error while gate is open
        let err = resolve_answers(&questions, &AnswerSet::new(), &StubEval).unwrap_err();
        assert!(matches!(err, RenderError::Unanswered(id) if id == AnswerId::from("registry")));
        // Gate closed -> no error, question absent
        let provided = answers(&[("use_docker", Value::Bool(false))]);
        let resolved = resolve_answers(&questions, &provided, &StubEval).unwrap();
        assert!(!resolved.contains(&"registry".into()));
    }

    /// Evaluates the expressions in its table; anything else falls through
    /// to [`StubEval`].
    struct TableEval(Vec<(&'static str, Value)>);

    impl ExprEval for TableEval {
        fn eval(&self, expr: &StarlarkExpr, answers: &AnswerSet) -> Result<Value, EvalError> {
            match self.0.iter().find(|(e, _)| *e == expr.as_str()) {
                Some((_, v)) => Ok(v.clone()),
                None => StubEval.eval(expr, answers),
            }
        }
    }

    fn list(items: &[&str]) -> Value {
        Value::List(items.iter().map(|s| Value::String((*s).into())).collect())
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// `skills`, declared with github/linear/airflow/dbt and narrowed by a
    /// `dbt` template: airflow blocked (so gone from `choices`), dbt fixed.
    fn skills(default: Option<&str>) -> Question {
        Question {
            id: "skills".into(),
            kind: AnswerKind::MultiChoice {
                choices: strings(&["github", "linear", "dbt"]),
            },
            prompt: None,
            description: None,
            example: None,
            default: default.map(StarlarkExpr::from),
            when: None,
            computed: false,
            section: None,
            narrowing: crate::question::Narrowing {
                locked: false,
                fixed: strings(&["dbt"]),
                blocked: strings(&["airflow"]),
                by: strings(&["dbt"]),
            },
        }
    }

    #[test]
    fn narrowed_default_drops_blocked_choices_and_gains_fixed_ones() {
        let eval = TableEval(vec![("base_skills", list(&["github", "airflow"]))]);
        let resolved =
            resolve_answers(&[skills(Some("base_skills"))], &AnswerSet::new(), &eval).unwrap();
        assert_eq!(
            resolved.get(&"skills".into()),
            Some(&list(&["github", "dbt"]))
        );
    }

    #[test]
    fn narrowed_input_keeps_its_order_and_gains_fixed_choices() {
        let provided = answers(&[("skills", list(&["linear", "github"]))]);
        let resolved = resolve_answers(&[skills(None)], &provided, &StubEval).unwrap();
        assert_eq!(
            resolved.get(&"skills".into()),
            Some(&list(&["linear", "github", "dbt"]))
        );
    }

    #[test]
    fn picking_a_blocked_choice_names_the_refining_template() {
        let provided = answers(&[("skills", list(&["github", "airflow"]))]);
        let err = resolve_answers(&[skills(None)], &provided, &StubEval).unwrap_err();
        assert!(matches!(&err, RenderError::Blocked { value, .. } if value == "airflow"));
        assert_eq!(
            err.to_string(),
            "answer `skills`: \"airflow\" is blocked by template `dbt`"
        );
    }

    #[test]
    fn a_multichoice_with_only_fixed_choices_left_resolves_without_an_answer() {
        let mut q = skills(None);
        q.kind = AnswerKind::MultiChoice {
            choices: strings(&["dbt"]),
        };
        assert!(!q.is_promptable());
        let resolved = resolve_answers(&[q], &AnswerSet::new(), &StubEval).unwrap();
        assert_eq!(resolved.get(&"skills".into()), Some(&list(&["dbt"])));
    }

    #[test]
    fn a_blocked_choice_default_is_an_error() {
        let q = Question {
            id: "adapter".into(),
            kind: AnswerKind::Choice {
                choices: strings(&["postgres"]),
            },
            prompt: None,
            description: None,
            example: None,
            default: Some(StarlarkExpr::from("base_adapter")),
            when: None,
            computed: false,
            section: None,
            narrowing: crate::question::Narrowing {
                blocked: strings(&["duckdb"]),
                by: strings(&["dbt"]),
                ..Default::default()
            },
        };
        let eval = TableEval(vec![("base_adapter", Value::String("duckdb".into()))]);
        let err = resolve_answers(&[q], &AnswerSet::new(), &eval).unwrap_err();
        assert!(matches!(err, RenderError::Blocked { value, .. } if value == "duckdb"));
    }

    #[test]
    fn a_locked_question_accepts_only_its_lock_value() {
        let q = Question {
            id: "use_jira".into(),
            kind: AnswerKind::Bool,
            prompt: None,
            description: None,
            example: None,
            default: Some(StarlarkExpr::from("False")),
            when: None,
            computed: false,
            section: None,
            narrowing: crate::question::Narrowing {
                locked: true,
                by: strings(&["dbt"]),
                ..Default::default()
            },
        };
        assert!(!q.is_promptable());
        let questions = [q];
        let unanswered = resolve_answers(&questions, &AnswerSet::new(), &StubEval).unwrap();
        assert_eq!(
            unanswered.get(&"use_jira".into()),
            Some(&Value::Bool(false))
        );
        let same = answers(&[("use_jira", Value::Bool(false))]);
        assert!(resolve_answers(&questions, &same, &StubEval).is_ok());
        let other = answers(&[("use_jira", Value::Bool(true))]);
        let err = resolve_answers(&questions, &other, &StubEval).unwrap_err();
        assert_eq!(
            err.to_string(),
            "answer `use_jira` is locked to False by template `dbt`; drop the explicit answer"
        );
    }

    #[test]
    fn a_locked_multichoice_compares_as_a_set() {
        let mut q = skills(Some("lock"));
        q.narrowing = crate::question::Narrowing {
            locked: true,
            by: strings(&["dbt"]),
            ..Default::default()
        };
        let eval = TableEval(vec![("lock", list(&["github", "dbt"]))]);
        let reordered = answers(&[("skills", list(&["dbt", "github"]))]);
        let resolved = resolve_answers(&[q.clone()], &reordered, &eval).unwrap();
        assert_eq!(
            resolved.get(&"skills".into()),
            Some(&list(&["github", "dbt"]))
        );
        let fewer = answers(&[("skills", list(&["dbt"]))]);
        assert!(matches!(
            resolve_answers(&[q], &fewer, &eval),
            Err(RenderError::Locked { .. })
        ));
    }

    #[test]
    fn binary_file_renders_and_resists_hunks() {
        use crate::patch::DEFAULT_FILE_MODE;
        use base64::Engine as _;
        let bytes: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x89, 0x50];
        let patch = Patch::new(
            vec![],
            None,
            vec![Op::CreateBinaryFile {
                path: TemplatePath::literal("app/favicon.ico"),
                data: base64::engine::general_purpose::STANDARD.encode(&bytes),
                mode: DEFAULT_FILE_MODE,
            }],
        );
        let tree = render(std::slice::from_ref(&patch), &AnswerSet::new(), &StubEval).unwrap();
        let entry = tree.get("app/favicon.ico".into()).unwrap();
        assert_eq!(entry.content.as_bytes(), bytes.as_slice());
        assert!(entry.content.is_binary());

        // Text hunks cannot apply to a binary file.
        let modify = Patch::new(
            vec![patch.id],
            None,
            vec![Op::ModifyFile {
                path: TemplatePath::literal("app/favicon.ico"),
                hunks: vec![Hunk::default()],
            }],
        );
        let err = render(&[patch.clone(), modify], &AnswerSet::new(), &StubEval).unwrap_err();
        assert!(matches!(err, RenderError::BinaryModify { .. }), "{err}");

        // Replacement is delete + create in one patch, applied in order.
        let replace = Patch::new(
            vec![patch.id],
            None,
            vec![
                Op::DeleteFile {
                    path: TemplatePath::literal("app/favicon.ico"),
                },
                Op::CreateBinaryFile {
                    path: TemplatePath::literal("app/favicon.ico"),
                    data: base64::engine::general_purpose::STANDARD.encode([0u8, 1, 2]),
                    mode: DEFAULT_FILE_MODE,
                },
            ],
        );
        let tree = render(&[patch, replace], &AnswerSet::new(), &StubEval).unwrap();
        assert_eq!(
            tree.get("app/favicon.ico".into())
                .unwrap()
                .content
                .as_bytes(),
            &[0u8, 1, 2]
        );
    }
}
