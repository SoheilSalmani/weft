//! How the engine asks a person for answers. The engine only declares the
//! questions; the binary draws them (`weft-cli`'s prompts), and runs without
//! a person use [`NonInteractive`].

use anyhow::{bail, Result};
use weft_core::{Question, SecretValue, Value};

/// How the engine talks to a human. Abstracted so tests and non-interactive
/// runs can plug in deterministic behavior.
pub trait Interaction {
    /// Ask one question. A `default` is offered for the person to accept,
    /// and taken as the answer when there is nobody to ask.
    fn ask(&mut self, question: &Question, default: Option<&Value>) -> Result<Value>;
    fn ask_secret(&mut self, question: &Question) -> Result<SecretValue>;
    fn confirm(&mut self, message: &str, default: bool) -> Result<bool>;

    /// Decide one abstraction candidate: abstract all occurrences, keep all
    /// literal, or keep a chosen subset literal (`occurrences` are display
    /// lines like `README.md:12 · my_project`). The default — used by
    /// non-interactive runs and `--yes` — abstracts everything, preserving
    /// the historical behavior.
    fn decide_candidate(
        &mut self,
        message: &str,
        occurrences: &[String],
    ) -> Result<crate::abstraction::CandidateDecision> {
        let _ = occurrences;
        let yes = self.confirm(message, true)?;
        Ok(if yes {
            crate::abstraction::CandidateDecision::All
        } else {
            crate::abstraction::CandidateDecision::None
        })
    }

    /// Whether a person answers the prompts. Non-interactive runs take each
    /// prompt's default.
    fn interactive(&self) -> bool {
        false
    }

    /// Pick one of `items`; `default` is the index taken without a person.
    fn choose(&mut self, prompt: &str, items: &[&str], default: usize) -> Result<usize> {
        let _ = (prompt, items);
        Ok(default)
    }

    /// Pick any of `items`, starting from `checked`; an unattended run keeps
    /// what is checked.
    fn pick(&mut self, prompt: &str, items: &[&str], checked: &[bool]) -> Result<Vec<usize>> {
        let _ = (prompt, items);
        Ok(checked
            .iter()
            .enumerate()
            .filter_map(|(i, on)| on.then_some(i))
            .collect())
    }

    /// A line of text, `default` when nobody types one.
    fn text(&mut self, prompt: &str, default: &str) -> Result<String> {
        let _ = prompt;
        Ok(default.to_owned())
    }

    /// Let a person edit `text` in their editor, as a file ending in
    /// `extension` (`.json`, or empty); returns it unchanged without one.
    fn edit(&mut self, text: &str, extension: &str) -> Result<String> {
        let _ = extension;
        Ok(text.to_owned())
    }
}

/// Takes the default a prompt offers, and fails on a prompt without one with
/// an actionable message. Used when stdin is not a TTY or `--non-interactive`
/// is set.
pub struct NonInteractive;

impl Interaction for NonInteractive {
    fn ask(&mut self, question: &Question, default: Option<&Value>) -> Result<Value> {
        if let Some(default) = default {
            return Ok(default.clone());
        }
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
