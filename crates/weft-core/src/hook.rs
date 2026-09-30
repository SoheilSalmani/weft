use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::id::{AnswerId, HookId};
use crate::question::StarlarkExpr;
use crate::segment::Segment;

/// When a hook runs relative to writing the rendered tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookPhase {
    /// Runs before any file is written; a non-zero exit aborts the scaffold.
    Pre,
    /// Runs after the tree is written and state is saved.
    Post,
}

/// The blast radius of a hook — the "AI-ready" classification that lets an
/// agent decide whether to run it automatically or surface it for approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookEffect {
    /// Read-only verification (e.g. "is uv installed"). Safe to auto-run; a
    /// failure gates the render.
    Check,
    /// Idempotent local mutation (install deps, generate client, format).
    /// Safe to re-run.
    Setup,
    /// External, potentially irreversible side-effect (deploy, DNS, create
    /// repo). An agent should confirm before running.
    Deploy,
}

/// A shell command as a sequence of segments, so answers and expressions can
/// be interpolated (e.g. `["shadcn add ", {expr = "' '.join(components)"}]`).
/// The file form is a plain string for a fully-literal command, or an array
/// of segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command(pub Vec<Segment>);

impl Command {
    /// A fully-literal command from a plain string.
    pub fn literal(s: &str) -> Self {
        Self(vec![Segment::Literal(s.to_owned())])
    }

    /// Human-readable source form for display (`describe`): literals verbatim,
    /// interpolations shown as `${…}`. Not for execution — use rendering.
    pub fn source(&self) -> String {
        self.0
            .iter()
            .map(|s| match s {
                Segment::Literal(t) => t.clone(),
                Segment::Answer(id) => format!("${{{}}}", id.0),
                Segment::Expr(e) => format!("${{{}}}", e.0),
                Segment::Slot(decl) => format!("${{slot {}}}", decl.slot),
            })
            .collect()
    }

    /// Whether any segment interpolates a value (vs. a fully-literal command).
    /// A literal command can never leak a secret into the run log.
    pub fn is_literal(&self) -> bool {
        self.0.iter().all(|s| matches!(s, Segment::Literal(_)))
    }
}

impl Serialize for Command {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Shorthand: a single literal serializes as a bare string.
        match self.0.as_slice() {
            [Segment::Literal(s)] => serializer.serialize_str(s),
            segs => segs.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Command;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a shell command string or an array of segments")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Command, E> {
                Ok(Command::literal(v))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Command, E> {
                Ok(Command(vec![Segment::Literal(v)]))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Command, A::Error> {
                let mut segs = Vec::new();
                while let Some(seg) = seq.next_element::<Segment>()? {
                    segs.push(seg);
                }
                Ok(Command(segs))
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// A post-hook re-fire trigger: `weft update` re-runs the hook only when one
/// of its inputs changed. Serde form is a prefixed string: `glob:pyproject.toml`,
/// `answer:use_docker`, `hook:uv-sync`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookInput {
    Glob(String),
    Answer(AnswerId),
    Hook(HookId),
}

impl fmt::Display for HookInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookInput::Glob(g) => write!(f, "glob:{g}"),
            HookInput::Answer(a) => write!(f, "answer:{a}"),
            HookInput::Hook(t) => write!(f, "hook:{t}"),
        }
    }
}

impl FromStr for HookInput {
    type Err = HookInputError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(g) = s.strip_prefix("glob:") {
            Ok(HookInput::Glob(g.to_owned()))
        } else if let Some(a) = s.strip_prefix("answer:") {
            Ok(HookInput::Answer(AnswerId(a.to_owned())))
        } else if let Some(t) = s.strip_prefix("hook:") {
            Ok(HookInput::Hook(HookId(t.to_owned())))
        } else {
            Err(HookInputError(s.to_owned()))
        }
    }
}

impl Serialize for HookInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for HookInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid hook input {0:?} (expected `glob:...`, `answer:...`, or `hook:...`)")]
pub struct HookInputError(String);

/// A labeled, patch-scoped side-effect that runs before (`pre`) or after
/// (`post`) the render. Hooks live in a patch's [`crate::PatchMeta`] and are
/// **not** part of the patch content hash, so adding or editing a hook never
/// changes patch ids.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    /// Human slug, unique across the template (referenced by
    /// `after`/`before`/`inputs`).
    pub id: HookId,
    pub phase: HookPhase,
    pub effect: HookEffect,
    /// Short human/agent-facing label, e.g. "Verify uv is installed".
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub action: Command,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<StarlarkExpr>,
    /// Other hooks this one must run after (explicit ordering edges).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<HookId>,
    /// Other hooks this one must run before (the mirror of `after`; how an
    /// extender orders its hooks ahead of one it inherits).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<HookId>,
    /// Post-only: `weft update` re-fires this hook only when one of these
    /// changed. Ignored for `pre` hooks (they always run).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<HookInput>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_input_round_trips() {
        for s in ["glob:pyproject.toml", "answer:use_docker", "hook:uv-sync"] {
            let input: HookInput = s.parse().unwrap();
            assert_eq!(input.to_string(), s);
        }
        assert!("pyproject.toml".parse::<HookInput>().is_err());
    }

    #[test]
    fn literal_command_serializes_as_string() {
        let a = Command::literal("pnpm install");
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#""pnpm install""#);
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), a);
        assert!(a.is_literal());
    }

    #[test]
    fn segmented_command_round_trips_and_previews() {
        let json = r#"["shadcn add ",{"expr":"' '.join(components)"}]"#;
        let a: Command = serde_json::from_str(json).unwrap();
        assert_eq!(a.source(), "shadcn add ${' '.join(components)}");
        assert!(!a.is_literal());
        assert_eq!(serde_json::to_string(&a).unwrap(), json);
    }

    #[test]
    fn hook_round_trips_with_shorthand_action() {
        let src = r#"{
            "id": "verify-uv",
            "phase": "pre",
            "effect": "check",
            "label": "Verify uv is installed",
            "action": "command -v uv"
        }"#;
        let h: Hook = serde_json::from_str(src).unwrap();
        assert_eq!(h.phase, HookPhase::Pre);
        assert_eq!(h.effect, HookEffect::Check);
        assert!(h.after.is_empty() && h.inputs.is_empty());
        // re-serialize and parse back
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(serde_json::from_str::<Hook>(&json).unwrap(), h);
    }
}
