use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::id::AnswerId;

/// A Starlark expression stored as source text. Parsing/evaluation lives in
/// `weft-lang`; core only carries it around.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StarlarkExpr(pub String);

impl StarlarkExpr {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StarlarkExpr {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A question the template asks. Serialized in `weft.toml` as `[[question]]`.
///
/// Note: no `deny_unknown_fields` — serde can't combine it with the flattened
/// `kind`. Manifest validation of stray keys happens in `weft check`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: AnswerId,
    #[serde(flatten)]
    pub kind: AnswerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Longer human/agent-facing explanation of what this answer controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Example value, shown to humans and agents (display form, not Starlark).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// Starlark expression; may reference answers to earlier questions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<StarlarkExpr>,
    /// Ask only if this evaluates to true (given the answers so far).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<StarlarkExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AnswerKind {
    String,
    Bool,
    Int,
    Choice { choices: Vec<String> },
    Secret { source: SecretSpec },
}

impl AnswerKind {
    pub fn name(&self) -> &'static str {
        match self {
            AnswerKind::String => "string",
            AnswerKind::Bool => "bool",
            AnswerKind::Int => "int",
            AnswerKind::Choice { .. } => "choice",
            AnswerKind::Secret { .. } => "secret",
        }
    }
}

/// Where a secret answer comes from. The answers/state files only ever store
/// this reference — never the resolved value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretSpec {
    /// Read from an environment variable at render time.
    Env(String),
    /// Run a command and use its trimmed stdout.
    Cmd(String),
    /// Prompt interactively; never persisted anywhere.
    Prompt,
}

impl fmt::Display for SecretSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretSpec::Env(var) => write!(f, "env:{var}"),
            SecretSpec::Cmd(cmd) => write!(f, "cmd:{cmd}"),
            SecretSpec::Prompt => f.write_str("prompt"),
        }
    }
}

impl FromStr for SecretSpec {
    type Err = SecretSpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(var) = s.strip_prefix("env:") {
            if var.is_empty() {
                return Err(SecretSpecError(s.to_owned()));
            }
            Ok(SecretSpec::Env(var.to_owned()))
        } else if let Some(cmd) = s.strip_prefix("cmd:") {
            if cmd.trim().is_empty() {
                return Err(SecretSpecError(s.to_owned()));
            }
            Ok(SecretSpec::Cmd(cmd.to_owned()))
        } else if s == "prompt" {
            Ok(SecretSpec::Prompt)
        } else {
            Err(SecretSpecError(s.to_owned()))
        }
    }
}

impl Serialize for SecretSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SecretSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid secret source {0:?} (expected `env:VAR`, `cmd:...`, or `prompt`)")]
pub struct SecretSpecError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_toml_round_trip() {
        let toml_src = r#"
            id = "use_docker"
            kind = "bool"
            prompt = "Enable Docker?"
            default = "True"
        "#;
        let q: Question = toml::from_str(toml_src).unwrap();
        assert_eq!(q.kind, AnswerKind::Bool);
        assert_eq!(q.default, Some(StarlarkExpr::from("True")));
        let back = toml::to_string(&q).unwrap();
        let q2: Question = toml::from_str(&back).unwrap();
        assert_eq!(q, q2);
    }

    #[test]
    fn secret_spec_parses() {
        assert_eq!(
            "env:DATABASE_URL".parse::<SecretSpec>().unwrap(),
            SecretSpec::Env("DATABASE_URL".into())
        );
        assert_eq!("prompt".parse::<SecretSpec>().unwrap(), SecretSpec::Prompt);
        assert!("envDATABASE".parse::<SecretSpec>().is_err());
    }

    #[test]
    fn choice_kind_round_trip() {
        let toml_src = r#"
            id = "license"
            kind = "choice"
            choices = ["mit", "apache"]
        "#;
        let q: Question = toml::from_str(toml_src).unwrap();
        assert_eq!(
            q.kind,
            AnswerKind::Choice {
                choices: vec!["mit".into(), "apache".into()]
            }
        );
    }
}
