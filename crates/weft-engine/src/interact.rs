use anyhow::{bail, Context, Result};
use weft_core::{AnswerKind, Question, SecretValue, Value};

/// An editor on `$VISUAL`, else `$EDITOR`, else `vi`, skipping a variable
/// that is set but empty, which dialoguer would otherwise panic on.
fn editor() -> dialoguer::Editor {
    let mut editor = dialoguer::Editor::new();
    let chosen = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|cmd| !cmd.trim().is_empty());
    editor.executable(chosen.as_deref().unwrap_or("vi"));
    editor
}

/// Open `$VISUAL`/`$EDITOR` (fallback `vi`) on a seeded `.sh` buffer and
/// return the entered command: the seed `instructions` become `#` comment
/// lines, comments are stripped from the result, and multi-line commands
/// are fine (`sh -c`). Errors when the editor closes without saving or
/// nothing was entered.
pub fn edit_command(instructions: &str) -> Result<String> {
    let seed: String = instructions.lines().map(|l| format!("# {l}\n")).collect();
    let edited = editor()
        .extension(".sh")
        .require_save(true)
        .edit(&seed)
        .context("opening $VISUAL/$EDITOR")?
        .context("aborted: editor closed without saving")?;
    let command = edited
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    if command.is_empty() {
        bail!("aborted: no command entered");
    }
    Ok(command)
}

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
            AnswerKind::MultiChoice { choices } => {
                // Template-fixed choices are always selected: they aren't
                // offered, only named, and join the answer after the pick.
                let fixed = &question.narrowing.fixed;
                let free: Vec<&String> = choices.iter().filter(|c| !fixed.contains(c)).collect();
                let prompt = if fixed.is_empty() {
                    prompt
                } else {
                    format!("{prompt} (always included: {})", fixed.join(", "))
                };
                let mut ms = dialoguer::MultiSelect::new()
                    .with_prompt(prompt)
                    .items(&free);
                if let Some(Value::List(items)) = default {
                    let checked: Vec<bool> = free
                        .iter()
                        .map(|c| {
                            items
                                .iter()
                                .any(|v| matches!(v, Value::String(s) if s == *c))
                        })
                        .collect();
                    ms = ms.defaults(&checked);
                }
                let idxs = ms.interact()?;
                Value::List(
                    idxs.into_iter()
                        .map(|i| Value::String(free[i].clone()))
                        .chain(fixed.iter().map(|f| Value::String(f.clone())))
                        .collect(),
                )
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

    fn interactive(&self) -> bool {
        true
    }

    fn choose(&mut self, prompt: &str, items: &[&str], default: usize) -> Result<usize> {
        Ok(dialoguer::Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()?)
    }

    fn pick(&mut self, prompt: &str, items: &[&str], checked: &[bool]) -> Result<Vec<usize>> {
        Ok(dialoguer::MultiSelect::new()
            .with_prompt(prompt)
            .items(items)
            .defaults(checked)
            .interact()?)
    }

    fn text(&mut self, prompt: &str, default: &str) -> Result<String> {
        Ok(dialoguer::Input::<String>::new()
            .with_prompt(prompt)
            .default(default.to_owned())
            .interact_text()?)
    }

    fn edit(&mut self, text: &str, extension: &str) -> Result<String> {
        let mut editor = editor();
        editor.require_save(false);
        if !extension.is_empty() {
            editor.extension(extension);
        }
        Ok(editor
            .edit(text)
            .context("opening $VISUAL/$EDITOR")?
            .unwrap_or_else(|| text.to_owned()))
    }

    fn decide_candidate(
        &mut self,
        message: &str,
        occurrences: &[String],
    ) -> Result<crate::abstraction::CandidateDecision> {
        use crate::abstraction::CandidateDecision;
        if occurrences.is_empty() {
            // Nothing to select from (e.g. path-only matches): yes/no.
            let yes = self.confirm(message, true)?;
            return Ok(if yes {
                CandidateDecision::All
            } else {
                CandidateDecision::None
            });
        }
        let choice = dialoguer::Select::new()
            .with_prompt(message)
            .items(&[
                "yes — abstract every occurrence",
                "no — keep them all literal",
                "select — choose occurrences to keep literal",
            ])
            .default(0)
            .interact()?;
        match choice {
            0 => Ok(CandidateDecision::All),
            1 => Ok(CandidateDecision::None),
            _ => {
                let keep = dialoguer::MultiSelect::new()
                    .with_prompt("mark the occurrences to KEEP LITERAL (space toggles)")
                    .items(occurrences)
                    .interact()?;
                Ok(CandidateDecision::Except(keep))
            }
        }
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
