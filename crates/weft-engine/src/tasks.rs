//! The task graph: topologically ordered, fired only when declared inputs
//! changed. On first scaffold every `when`-passing task fires.

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use globset::{Glob, GlobMatcher};
use weft_core::render::ExprEval;
use weft_core::{AnswerId, AnswerSet, Task, TaskId, TaskInput};

/// What changed between the previous render and this one. `None` means
/// "initial scaffold": every task's inputs are considered changed.
pub struct ChangeSet {
    pub paths: BTreeSet<Utf8PathBuf>,
    pub answers: BTreeSet<AnswerId>,
}

/// Decide which tasks fire and in what order.
pub fn plan<'t>(
    tasks: &'t [Task],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
    changes: Option<&ChangeSet>,
) -> Result<Vec<&'t Task>> {
    let ordered = topo_order(tasks)?;
    let mut fired: BTreeSet<&TaskId> = BTreeSet::new();
    let mut plan = Vec::new();
    for task in ordered {
        if let Some(when) = &task.when {
            let active = eval
                .eval_bool(when, answers)
                .with_context(|| format!("evaluating when of task `{}`", task.id))?;
            if !active {
                continue;
            }
        }
        let inputs_changed = match changes {
            None => true,
            Some(changes) => task.inputs.iter().try_fold(false, |acc, input| {
                anyhow::Ok(
                    acc | match input {
                        TaskInput::Glob(pattern) => {
                            let matcher = glob_matcher(pattern)?;
                            changes
                                .paths
                                .iter()
                                .any(|p| matcher.is_match(p.as_std_path()))
                        }
                        TaskInput::Answer(id) => changes.answers.contains(id),
                        TaskInput::Task(id) => fired.contains(id),
                    },
                )
            })?,
        };
        if inputs_changed {
            fired.insert(&task.id);
            plan.push(task);
        }
    }
    Ok(plan)
}

/// Run planned tasks with `sh -c` in `dest`, stopping at the first failure.
/// Each command is rendered against `answers` first, so it can interpolate
/// answers and expressions (`{expr = "' '.join(components)"}`).
pub fn run(
    plan: &[&Task],
    dest: &Utf8PathBuf,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<()> {
    for task in plan {
        let command = weft_core::render::render_segments(&task.action.0, answers, eval)
            .with_context(|| format!("rendering command for task `{}`", task.id))?;
        eprintln!("task {}: {}", task.id, command);
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(dest)
            .status()
            .with_context(|| format!("spawning task `{}`", task.id))?;
        if !status.success() {
            bail!("task `{}` failed with {status}", task.id);
        }
    }
    Ok(())
}

fn glob_matcher(pattern: &str) -> Result<GlobMatcher> {
    Ok(Glob::new(pattern)
        .with_context(|| format!("invalid glob {pattern:?}"))?
        .compile_matcher())
}

/// Declaration-order-stable topological sort over `task:` inputs.
pub fn topo_order(tasks: &[Task]) -> Result<Vec<&Task>> {
    let ids: BTreeSet<&TaskId> = tasks.iter().map(|t| &t.id).collect();
    if ids.len() != tasks.len() {
        bail!("duplicate task ids in manifest");
    }
    for task in tasks {
        for input in &task.inputs {
            if let TaskInput::Task(dep) = input {
                if !ids.contains(dep) {
                    bail!("task `{}` depends on unknown task `{dep}`", task.id);
                }
            }
        }
    }
    let mut order: Vec<&Task> = Vec::with_capacity(tasks.len());
    let mut placed: BTreeSet<&TaskId> = BTreeSet::new();
    let mut remaining: Vec<&Task> = tasks.iter().collect();
    while !remaining.is_empty() {
        let before = remaining.len();
        remaining.retain(|task| {
            let ready = task.inputs.iter().all(|i| match i {
                TaskInput::Task(dep) => placed.contains(dep),
                _ => true,
            });
            if ready {
                placed.insert(&task.id);
                order.push(task);
            }
            !ready
        });
        if remaining.len() == before {
            let stuck: Vec<_> = remaining.iter().map(|t| t.id.to_string()).collect();
            bail!("task dependency cycle among: {}", stuck.join(", "));
        }
    }
    Ok(order)
}
