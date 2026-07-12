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

/// What a task does: a shell command, as a sequence of segments so answers
/// and expressions can be interpolated (e.g.
/// `["shadcn add ", {expr = "' '.join(components)"}]`). The manifest form is
/// a plain string for a fully-literal command, or an array of segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskAction(pub Vec<crate::segment::Segment>);

impl TaskAction {
    /// A fully-literal command from a plain string.
    pub fn literal(s: &str) -> Self {
        Self(vec![crate::segment::Segment::Literal(s.to_owned())])
    }

    /// Human-readable source form for display (`describe`): literals verbatim,
    /// interpolations shown as `${…}`. Not for execution — use rendering.
    pub fn source(&self) -> String {
        use crate::segment::Segment;
        self.0
            .iter()
            .map(|s| match s {
                Segment::Literal(t) => t.clone(),
                Segment::Answer(id) => format!("${{{}}}", id.0),
                Segment::Expr(e) => format!("${{{}}}", e.0),
            })
            .collect()
    }
}

impl Serialize for TaskAction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use crate::segment::Segment;
        // Shorthand: a single literal serializes as a bare string.
        match self.0.as_slice() {
            [Segment::Literal(s)] => serializer.serialize_str(s),
            segs => segs.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TaskAction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use crate::segment::Segment;
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = TaskAction;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a shell command string or an array of segments")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<TaskAction, E> {
                Ok(TaskAction::literal(v))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<TaskAction, E> {
                Ok(TaskAction(vec![Segment::Literal(v)]))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<TaskAction, A::Error> {
                let mut segs = Vec::new();
                while let Some(seg) = seq.next_element::<Segment>()? {
                    segs.push(seg);
                }
                Ok(TaskAction(segs))
            }
        }
        deserializer.deserialize_any(V)
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

    #[test]
    fn literal_action_serializes_as_string() {
        let a = TaskAction::literal("pnpm install");
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#""pnpm install""#);
        assert_eq!(serde_json::from_str::<TaskAction>(&json).unwrap(), a);
    }

    #[test]
    fn segmented_action_round_trips_and_previews() {
        let json = r#"["shadcn add ",{"expr":"' '.join(components)"}]"#;
        let a: TaskAction = serde_json::from_str(json).unwrap();
        assert_eq!(a.source(), "shadcn add ${' '.join(components)}");
        assert_eq!(serde_json::to_string(&a).unwrap(), json);
    }
}
