use anyhow::{bail, Result};
use weft_core::{AnswerKind, Question, SecretValue, Value};

/// How the engine talks to a human. Abstracted so tests and non-interactive
/// runs can plug in deterministic behavior.
pub trait Interaction {
    fn ask(&mut self, question: &Question, default: Option<&Value>) -> Result<Value>;
    fn ask_secret(&mut self, question: &Question) -> Result<SecretValue>;
    fn confirm(&mut self, message: &str, default: bool) -> Result<bool>;
}

/// Fails on any prompt with an actionable message. Used when stdin is not a
/// TTY or `--non-interactive` is set.
pub struct NonInteractive;

impl Interaction for NonInteractive {
    fn ask(&mut self, question: &Question, _default: Option<&Value>) -> Result<Value> {
        bail!(
            "question `{}` is unanswered and this run is non-interactive; \
             pass it with --answer {}=..., --answers-json, an answers file, or a \
             preset (run `weft describe --json` for the full contract)",
            question.id,
            question.id
        );
    }

    fn ask_secret(&mut self, question: &Question) -> Result<SecretValue> {
        bail!(
            "secret `{}` uses `prompt` resolution, which needs an interactive terminal",
            question.id
        );
    }

    fn confirm(&mut self, _message: &str, default: bool) -> Result<bool> {
        Ok(default)
    }
}

/// Real terminal prompts via dialoguer.
pub struct TerminalInteraction;

impl Interaction for TerminalInteraction {
    fn ask(&mut self, question: &Question, default: Option<&Value>) -> Result<Value> {
        let prompt = question
            .prompt
            .clone()
            .unwrap_or_else(|| question.id.to_string());
        let value = match &question.kind {
            AnswerKind::Bool => {
                let mut c = dialoguer::Confirm::new().with_prompt(prompt);
                if let Some(Value::Bool(b)) = default {
                    c = c.default(*b);
                }
                Value::Bool(c.interact()?)
            }
            AnswerKind::Int => {
                let mut input = dialoguer::Input::<i64>::new().with_prompt(prompt);
                if let Some(Value::Int(i)) = default {
                    input = input.default(*i);
                }
                Value::Int(input.interact_text()?)
            }
            AnswerKind::Choice { choices } => {
                let mut select = dialoguer::Select::new().with_prompt(prompt).items(choices);
                if let Some(Value::String(s)) = default {
                    if let Some(pos) = choices.iter().position(|c| c == s) {
                        select = select.default(pos);
                    }
                }
                let idx = select.interact()?;
                Value::String(choices[idx].clone())
            }
            AnswerKind::String => {
                let mut input = dialoguer::Input::<String>::new().with_prompt(prompt);
                if let Some(Value::String(s)) = default {
                    input = input.default(s.clone());
                }
                Value::String(input.interact_text()?)
            }
            AnswerKind::Secret { .. } => {
                bail!("secret questions are resolved via their source, not ask()")
            }
        };
        Ok(value)
    }

    fn ask_secret(&mut self, question: &Question) -> Result<SecretValue> {
        let prompt = question
            .prompt
            .clone()
            .unwrap_or_else(|| question.id.to_string());
        let value = dialoguer::Password::new().with_prompt(prompt).interact()?;
        Ok(SecretValue::new(value))
    }

    fn confirm(&mut self, message: &str, default: bool) -> Result<bool> {
        Ok(dialoguer::Confirm::new()
            .with_prompt(message)
            .default(default)
            .interact()?)
    }
}

/// Pick terminal or non-interactive automatically.
pub fn auto_interaction(force_non_interactive: bool) -> Box<dyn Interaction> {
    use std::io::IsTerminal;
    if !force_non_interactive && std::io::stdin().is_terminal() {
        Box::new(TerminalInteraction)
    } else {
        Box::new(NonInteractive)
    }
}
