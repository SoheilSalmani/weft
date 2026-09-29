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
    /// A derived value: never prompted, always taken from `default` (or an
    /// explicit override). Use for values computed from other answers, e.g.
    /// `selected_fonts = [font_ui, font_heading, ...]`. Requires a `default`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub computed: bool,
    /// Display grouping for UIs (sidebar sections). Pure presentation
    /// metadata — no effect on resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// How extending templates narrowed this inherited question (their
    /// `[refine.<id>]` tables, applied at load). Empty for a question as its
    /// own template declared it; never read from `[[question]]`.
    #[serde(
        default,
        skip_deserializing,
        skip_serializing_if = "Narrowing::is_empty"
    )]
    pub narrowing: Narrowing,
}

impl Question {
    /// Whether this question is presented to a human/agent for answering.
    /// Computed, secret, and locked questions resolve without prompting, and
    /// so does a multichoice whose every remaining choice is fixed.
    pub fn is_promptable(&self) -> bool {
        !self.computed
            && !matches!(self.kind, AnswerKind::Secret { .. })
            && !self.narrowing.locked
            && !self.nothing_to_choose()
    }

    /// A multichoice narrowed so far that every choice left is fixed (or
    /// none is left): its value is decided without asking.
    pub fn nothing_to_choose(&self) -> bool {
        match &self.kind {
            AnswerKind::MultiChoice { choices } if !self.narrowing.is_empty() => {
                choices.iter().all(|c| self.narrowing.fixed.contains(c))
            }
            _ => false,
        }
    }
}

/// How extending templates narrowed an inherited question (ADR-0002). The
/// question's `choices` already exclude `blocked`, and its `default` is the
/// refined one: the lock value when `locked`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Narrowing {
    /// The value is always the default; an explicit answer must equal it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// Multichoice: always selected. In declared order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed: Vec<String>,
    /// Choices removed from `choices`, kept so that picking one is reported
    /// as blocked rather than undeclared. In the order they were blocked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<String>,
    /// The templates whose `[refine]` tables touched this question,
    /// base-most first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub by: Vec<String>,
}

impl Narrowing {
    pub fn is_empty(&self) -> bool {
        !self.locked && self.fixed.is_empty() && self.blocked.is_empty() && self.by.is_empty()
    }

    /// Who narrowed the question, for messages: ``template `dbt` `` or
    /// ``templates `dbt`, `dbt-x` ``.
    pub fn refiners(&self) -> String {
        let names: Vec<String> = self.by.iter().map(|t| format!("`{t}`")).collect();
        match names.len() {
            1 => format!("template {}", names[0]),
            _ => format!("templates {}", names.join(", ")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AnswerKind {
    String,
    Bool,
    Int,
    Choice {
        choices: Vec<String>,
    },
    /// Pick any subset of `choices`; the answer is a `Value::List` of the
    /// selected values (each one of `choices`).
    MultiChoice {
        choices: Vec<String>,
    },
    Secret {
        source: SecretSpec,
    },
}

impl AnswerKind {
    pub fn name(&self) -> &'static str {
        match self {
            AnswerKind::String => "string",
            AnswerKind::Bool => "bool",
            AnswerKind::Int => "int",
            AnswerKind::Choice { .. } => "choice",
            AnswerKind::MultiChoice { .. } => "multichoice",
            AnswerKind::Secret { .. } => "secret",
        }
    }

    /// The declared choices for `choice`/`multichoice` kinds.
    pub fn choices(&self) -> Option<&[String]> {
        match self {
            AnswerKind::Choice { choices } | AnswerKind::MultiChoice { choices } => Some(choices),
            _ => None,
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

    #[test]
    fn multichoice_kind_round_trip() {
        let toml_src = r#"
            id = "components"
            kind = "multichoice"
            choices = ["button", "card", "dialog"]
        "#;
        let q: Question = toml::from_str(toml_src).unwrap();
        assert_eq!(
            q.kind,
            AnswerKind::MultiChoice {
                choices: vec!["button".into(), "card".into(), "dialog".into()]
            }
        );
        assert_eq!(q.kind.name(), "multichoice");
        let back = toml::to_string(&q).unwrap();
        assert_eq!(toml::from_str::<Question>(&back).unwrap(), q);
    }
}
