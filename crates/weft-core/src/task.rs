use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::id::{AnswerId, TaskId};
use crate::question::StarlarkExpr;

/// A task node. Tasks live in the same dependency graph as file changes:
/// their inputs name globs over rendered files, answers, or other tasks, and
/// a task re-fires only when one of its inputs changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    #[serde(default)]
    pub inputs: Vec<TaskInput>,
    pub action: TaskAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<StarlarkExpr>,
}

/// Serde form is a prefixed string: `glob:pyproject.toml`, `answer:use_docker`,
/// `task:uv-sync`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskInput {
    Glob(String),
    Answer(AnswerId),
    Task(TaskId),
}

impl fmt::Display for TaskInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskInput::Glob(g) => write!(f, "glob:{g}"),
            TaskInput::Answer(a) => write!(f, "answer:{a}"),
            TaskInput::Task(t) => write!(f, "task:{t}"),
        }
    }
}

impl FromStr for TaskInput {
    type Err = TaskInputError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(g) = s.strip_prefix("glob:") {
            Ok(TaskInput::Glob(g.to_owned()))
        } else if let Some(a) = s.strip_prefix("answer:") {
            Ok(TaskInput::Answer(AnswerId(a.to_owned())))
        } else if let Some(t) = s.strip_prefix("task:") {
            Ok(TaskInput::Task(TaskId(t.to_owned())))
        } else {
            Err(TaskInputError(s.to_owned()))
        }
    }
}

impl Serialize for TaskInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TaskInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid task input {0:?} (expected `glob:...`, `answer:...`, or `task:...`)")]
pub struct TaskInputError(String);

/// What a task does. Shell only for MVP; the manifest form is a plain string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskAction(pub String);

impl TaskAction {
    pub fn shell(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_input_round_trips() {
        for s in ["glob:pyproject.toml", "answer:use_docker", "task:uv-sync"] {
            let input: TaskInput = s.parse().unwrap();
            assert_eq!(input.to_string(), s);
        }
        assert!("pyproject.toml".parse::<TaskInput>().is_err());
    }
}
