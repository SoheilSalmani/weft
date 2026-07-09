//! Deterministic rendering: `render(base, answers, patches) -> Tree`.
//!
//! Same inputs ⇒ byte-identical output tree. Patch application order is the
//! topological order of the dependency DAG with ties broken by patch id, so
//! the input ordering of the patch slice never matters.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;

use crate::id::{AnswerId, PatchId};
use crate::patch::{Hunk, Op, Patch};
use crate::question::{AnswerKind, Question, StarlarkExpr};
use crate::segment::{join_lines, Content, Line, Segment, TemplatePath};
use crate::tree::{FileEntry, Tree};
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
}

/// Resolve the full answer set for `questions` from layered `provided`
/// answers: evaluate `when` gates in declaration order, type-check provided
/// values, evaluate `default` expressions for the rest. Questions whose
/// `when` is false are absent from the result.
pub fn resolve_answers(
    questions: &[Question],
    provided: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<AnswerSet, RenderError> {
    let mut resolved = AnswerSet::new();
    for q in questions {
        if let Some(when) = &q.when {
            let asked = eval
                .eval_bool(when, &resolved)
                .map_err(|e| RenderError::Eval {
                    expr: when.0.clone(),
                    context: format!("when of question `{}`", q.id),
                    source: e,
                })?;
            if !asked {
                continue;
            }
        }
        if let Some(value) = provided.get(&q.id) {
            let value = check_type(&q.id, &q.kind, value.clone())?;
            resolved.insert(q.id.clone(), value);
        } else if let Some(default) = &q.default {
            let value = eval
                .eval(default, &resolved)
                .map_err(|e| RenderError::Eval {
                    expr: default.0.clone(),
                    context: format!("default of question `{}`", q.id),
                    source: e,
                })?;
            let value = check_type(&q.id, &q.kind, value)?;
            resolved.insert(q.id.clone(), value);
        } else {
            return Err(RenderError::Unanswered(q.id.clone()));
        }
    }
    Ok(resolved)
}

fn check_type(id: &AnswerId, kind: &AnswerKind, value: Value) -> Result<Value, RenderError> {
    let ok = matches!(
        (kind, &value),
        (AnswerKind::String, Value::String(_))
            | (AnswerKind::Bool, Value::Bool(_))
            | (AnswerKind::Int, Value::Int(_))
            | (AnswerKind::Choice { .. }, Value::String(_))
            | (AnswerKind::Secret { .. }, Value::Secret(_))
    );
    if !ok {
        return Err(RenderError::TypeMismatch {
            id: id.clone(),
            expected: kind.name(),
            got: value.kind_name(),
        });
    }
    if let (AnswerKind::Choice { choices }, Value::String(s)) = (kind, &value) {
        if !choices.contains(s) {
            return Err(RenderError::InvalidChoice {
                id: id.clone(),
                value: s.clone(),
            });
        }
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
        if let Some(when) = &q.when {
            // Gate evaluation uses answers resolved so far; questions after a
            // missing answer may be mis-gated, so callers should re-run after
            // filling in the missing ones (the engine prompts in order).
            let asked = eval
                .eval_bool(when, &resolved)
                .map_err(|e| RenderError::Eval {
                    expr: when.0.clone(),
                    context: format!("when of question `{}`", q.id),
                    source: e,
                })?;
            if !asked {
                continue;
            }
        }
        if let Some(v) = provided.get(&q.id) {
            resolved.insert(q.id.clone(), v.clone());
        } else if let Some(default) = &q.default {
            let value = eval
                .eval(default, &resolved)
                .map_err(|e| RenderError::Eval {
                    expr: default.0.clone(),
                    context: format!("default of question `{}`", q.id),
                    source: e,
                })?;
            resolved.insert(q.id.clone(), value);
        } else {
            missing.push(q);
        }
    }
    Ok(missing)
}

/// Deterministic application order for `patches`: topological by dependency,
/// ties broken by patch id. Errors on unknown deps or cycles.
pub fn patch_order(patches: &[Patch]) -> Result<Vec<&Patch>, RenderError> {
    let by_id: BTreeMap<PatchId, &Patch> = patches.iter().map(|p| (p.id, p)).collect();
    let mut indegree: BTreeMap<PatchId, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<PatchId, Vec<PatchId>> = BTreeMap::new();
    for p in patches {
        indegree.entry(p.id).or_insert(0);
        for dep in &p.depends_on {
            if !by_id.contains_key(dep) {
                return Err(RenderError::UnknownDep {
                    patch: p.id,
                    dep: *dep,
                });
            }
            *indegree.entry(p.id).or_insert(0) += 1;
            dependents.entry(*dep).or_default().push(p.id);
        }
    }
    let mut ready: BTreeSet<PatchId> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(patches.len());
    while let Some(id) = ready.iter().next().copied() {
        ready.remove(&id);
        order.push(by_id[&id]);
        for dep in dependents.get(&id).cloned().unwrap_or_default() {
            let d = indegree.get_mut(&dep).expect("dependent tracked");
            *d -= 1;
            if *d == 0 {
                ready.insert(dep);
            }
        }
    }
    if order.len() != by_id.len() {
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

/// Render with an explicit, caller-chosen application order (must be a valid
/// linear extension of the dependency DAG — not verified here). Used by
/// `weft check` to prove that independent patches commute: applying them in
/// both orders must produce identical trees.
pub fn render_ordered(
    order: &[&Patch],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Tree, RenderError> {
    let mut skipped: BTreeSet<PatchId> = BTreeSet::new();
    let mut tree = Tree::new();
    for &patch in order {
        if patch.depends_on.iter().any(|d| skipped.contains(d)) {
            skipped.insert(patch.id);
            continue;
        }
        if let Some(when) = &patch.when {
            let active = eval
                .eval_bool(when, answers)
                .map_err(|e| RenderError::Eval {
                    expr: when.0.clone(),
                    context: format!("when of patch {}", patch.id.short()),
                    source: e,
                })?;
            if !active {
                skipped.insert(patch.id);
                continue;
            }
        }
        apply_patch(&mut tree, patch, answers, eval)?;
    }
    Ok(tree)
}

fn apply_patch(
    tree: &mut Tree,
    patch: &Patch,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<(), RenderError> {
    for op in &patch.ops {
        match op {
            Op::CreateFile {
                path,
                content,
                mode,
            } => {
                let path = render_path(path, answers, eval)?;
                if tree.get(&path).is_some() {
                    return Err(RenderError::CreateExists {
                        patch: patch.id,
                        path,
                    });
                }
                let text = render_content(content, answers, eval)?;
                tree.insert(
                    path,
                    FileEntry {
                        content: text,
                        mode: *mode,
                    },
                );
            }
            Op::ModifyFile { path, hunks } => {
                let path = render_path(path, answers, eval)?;
                let entry = tree.get(&path).ok_or_else(|| RenderError::MissingFile {
                    patch: patch.id,
                    path: path.clone(),
                })?;
                let mut lines: Vec<String> = entry.content.lines().map(str::to_owned).collect();
                for (i, hunk) in hunks.iter().enumerate() {
                    apply_hunk(&mut lines, hunk, answers, eval).map_err(|e| match e {
                        HunkApplyError::NoMatch => RenderError::HunkNoMatch {
                            patch: patch.id,
                            path: path.clone(),
                            hunk: i,
                        },
                        HunkApplyError::Ambiguous(count) => RenderError::HunkAmbiguous {
                            patch: patch.id,
                            path: path.clone(),
                            hunk: i,
                            count,
                        },
                        HunkApplyError::Render(e) => e,
                    })?;
                }
                let mode = entry.mode;
                tree.insert(
                    path,
                    FileEntry {
                        content: join_lines(&lines),
                        mode,
                    },
                );
            }
            Op::DeleteFile { path } => {
                let path = render_path(path, answers, eval)?;
                if tree.remove(&path).is_none() {
                    return Err(RenderError::MissingFile {
                        patch: patch.id,
                        path,
                    });
                }
            }
            Op::RenamePath { from, to } => {
                let from = render_path(from, answers, eval)?;
                let to = render_path(to, answers, eval)?;
                let entry = tree.remove(&from).ok_or_else(|| RenderError::MissingFile {
                    patch: patch.id,
                    path: from.clone(),
                })?;
                if tree.get(&to).is_some() {
                    return Err(RenderError::RenameExists {
                        patch: patch.id,
                        path: to,
                    });
                }
                tree.insert(to, entry);
            }
            Op::SetMode { path, mode } => {
                let path = render_path(path, answers, eval)?;
                let entry = tree.get(&path).ok_or_else(|| RenderError::MissingFile {
                    patch: patch.id,
                    path: path.clone(),
                })?;
                let content = entry.content.clone();
                tree.insert(
                    path,
                    FileEntry {
                        content,
                        mode: *mode,
                    },
                );
            }
        }
    }
    Ok(())
}

fn render_segment(
    seg: &Segment,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<String, RenderError> {
    match seg {
        Segment::Literal(s) => Ok(s.clone()),
        Segment::Answer(id) => answers
            .get(id)
            .map(Value::render_text)
            .ok_or_else(|| RenderError::UnknownAnswer(id.clone())),
        Segment::Expr(expr) => eval
            .eval(expr, answers)
            .map(|v| v.render_text())
            .map_err(|e| RenderError::Eval {
                expr: expr.0.clone(),
                context: "content expression".into(),
                source: e,
            }),
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

pub fn render_path(
    path: &TemplatePath,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<Utf8PathBuf, RenderError> {
    let mut out = String::new();
    for seg in &path.0 {
        out.push_str(&render_segment(seg, answers, eval)?);
    }
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

enum HunkApplyError {
    NoMatch,
    Ambiguous(usize),
    Render(RenderError),
}

/// Apply one context-anchored hunk. The pattern
/// `context_before + removed + context_after` must match exactly one position
/// in `lines`; `removed` is replaced with `added`. An entirely empty pattern
/// appends `added` at end of file.
fn apply_hunk(
    lines: &mut Vec<String>,
    hunk: &Hunk,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<(), HunkApplyError> {
    let rl = |ls: &[Line]| -> Result<Vec<String>, HunkApplyError> {
        ls.iter()
            .map(|l| render_line(l, answers, eval))
            .collect::<Result<_, _>>()
            .map_err(HunkApplyError::Render)
    };
    let before = rl(&hunk.context_before)?;
    let removed = rl(&hunk.removed)?;
    let added = rl(&hunk.added)?;
    let after = rl(&hunk.context_after)?;

    let mut pattern: Vec<&str> = Vec::new();
    pattern.extend(before.iter().map(String::as_str));
    pattern.extend(removed.iter().map(String::as_str));
    pattern.extend(after.iter().map(String::as_str));

    if pattern.is_empty() {
        lines.extend(added);
        return Ok(());
    }

    let matches: Vec<usize> = (0..=lines.len().saturating_sub(pattern.len()))
        .filter(|&i| {
            lines.len() - i >= pattern.len()
                && pattern.iter().enumerate().all(|(j, p)| lines[i + j] == *p)
        })
        .collect();

    match matches.as_slice() {
        [] => Err(HunkApplyError::NoMatch),
        [start] => {
            let remove_at = start + before.len();
            lines.splice(remove_at..remove_at + removed.len(), added);
            Ok(())
        }
        many => Err(HunkApplyError::Ambiguous(many.len())),
    }
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

    fn answers(pairs: &[(&str, Value)]) -> AnswerSet {
        pairs
            .iter()
            .map(|(k, v)| (AnswerId::from(*k), v.clone()))
            .collect()
    }

    #[test]
    fn hunk_applies_at_unique_context() {
        let mut lines: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let hunk = Hunk {
            context_before: vec![Line::literal("b")],
            removed: vec![Line::literal("c")],
            added: vec![Line::literal("C1"), Line::literal("C2")],
            context_after: vec![Line::literal("d")],
        };
        apply_hunk(&mut lines, &hunk, &AnswerSet::new(), &StubEval)
            .ok()
            .unwrap();
        assert_eq!(lines, vec!["a", "b", "C1", "C2", "d"]);
    }

    #[test]
    fn hunk_fails_cleanly_on_missing_context() {
        let mut lines: Vec<String> = vec!["a".into()];
        let hunk = Hunk {
            context_before: vec![Line::literal("nope")],
            ..Default::default()
        };
        assert!(matches!(
            apply_hunk(&mut lines, &hunk, &AnswerSet::new(), &StubEval),
            Err(HunkApplyError::NoMatch)
        ));
    }

    #[test]
    fn hunk_fails_on_ambiguous_context() {
        let mut lines: Vec<String> = ["x", "x"].iter().map(|s| s.to_string()).collect();
        let hunk = Hunk {
            context_before: vec![Line::literal("x")],
            added: vec![Line::literal("y")],
            ..Default::default()
        };
        assert!(matches!(
            apply_hunk(&mut lines, &hunk, &AnswerSet::new(), &StubEval),
            Err(HunkApplyError::Ambiguous(2))
        ));
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
                content: Content::from_text("a\n"),
                mode: 0o644,
            }],
        );
        let gated = Patch::new(
            vec![base.id],
            Some(StarlarkExpr::from("use_docker")),
            vec![Op::CreateFile {
                path: TemplatePath::literal("Dockerfile"),
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
    fn resolve_answers_gates_and_defaults() {
        let questions = vec![
            Question {
                id: "use_docker".into(),
                kind: AnswerKind::Bool,
                prompt: None,
                default: Some(StarlarkExpr::from("True")),
                when: None,
            },
            Question {
                id: "registry".into(),
                kind: AnswerKind::String,
                prompt: None,
                default: None,
                when: Some(StarlarkExpr::from("use_docker")),
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
}
