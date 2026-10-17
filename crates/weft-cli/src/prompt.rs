//! Prompts for a person at a terminal: cliclack's widgets on stderr, tied
//! together by the prompt rail, plus the `$VISUAL`/`$EDITOR` round trip.
//!
//! Nothing here runs without a person: [`auto_interaction`] hands the engine
//! its `NonInteractive` unless both stdin and stderr are terminals, so a
//! piped or captured run never waits on a prompt nobody can see.

use std::io::{self, IsTerminal, Write as _};
use std::process::Command;

use anyhow::{bail, Context as _, Result};
use weft_core::{AnswerKind, Question, SecretValue, Value};
use weft_engine::abstraction::CandidateDecision;
use weft_engine::interact::{Interaction, NonInteractive};

use crate::ui::{self, palette, rail, Out};

/// Prompt a person when one is there: stdin to read from and stderr to draw
/// on are both terminals, and `--non-interactive` (or `--yes`) is not set.
pub(crate) fn auto_interaction(force_non_interactive: bool) -> Box<dyn Interaction> {
    if !force_non_interactive && io::stdin().is_terminal() && io::stderr().is_terminal() {
        Box::new(TerminalInteraction)
    } else {
        Box::new(NonInteractive)
    }
}

/// The engine's prompts, drawn by cliclack.
pub(crate) struct TerminalInteraction;

/// Lists longer than this can be narrowed by typing.
const FILTER_FROM: usize = 8;

impl Interaction for TerminalInteraction {
    fn ask(&mut self, question: &Question, default: Option<&Value>) -> Result<Value> {
        rail::before_prompt();
        let prompt = header(question);
        let value = match &question.kind {
            AnswerKind::Bool => {
                let mut confirm = cliclack::confirm(&prompt);
                if let Some(Value::Bool(b)) = default {
                    confirm = confirm.initial_value(*b);
                }
                Value::Bool(answered(confirm.interact())?)
            }
            AnswerKind::Int => {
                let mut input = cliclack::input(&prompt).validate(|s: &String| {
                    s.trim()
                        .parse::<i64>()
                        .map(|_| ())
                        .map_err(|_| "Enter a whole number")
                });
                match default {
                    Some(Value::Int(i)) => input = input.default_input(&i.to_string()),
                    _ => {
                        if let Some(example) = &question.example {
                            input = input.placeholder(example);
                        }
                    }
                }
                let typed: String = answered(input.interact())?;
                Value::Int(typed.trim().parse().context("reading a whole number")?)
            }
            AnswerKind::Choice { choices } => {
                let items: Vec<(usize, &str, &str)> = choices
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i, c.as_str(), ""))
                    .collect();
                let mut select = cliclack::select(&prompt).items(&items);
                if choices.len() > FILTER_FROM {
                    select = select.filter_mode();
                }
                if let Some(Value::String(s)) = default {
                    if let Some(pos) = choices.iter().position(|c| c == s) {
                        select = select.initial_value(pos);
                    }
                }
                let idx = answered(select.interact())?;
                Value::String(choices[idx].clone())
            }
            AnswerKind::MultiChoice { choices } => {
                // Template-fixed choices are always selected: they are only
                // named, never offered, and join the answer after the pick.
                let fixed = &question.narrowing.fixed;
                let free: Vec<&String> = choices.iter().filter(|c| !fixed.contains(c)).collect();
                let mut picked = Vec::new();
                if !free.is_empty() {
                    let prompt = if fixed.is_empty() {
                        prompt
                    } else {
                        let always = format!("(always included: {})", fixed.join(", "));
                        with_detail(&prompt, &always)
                    };
                    let items: Vec<(usize, &str, &str)> = free
                        .iter()
                        .enumerate()
                        .map(|(i, c)| (i, c.as_str(), ""))
                        .collect();
                    let mut select = cliclack::multiselect(&prompt).items(&items).required(false);
                    if free.len() > FILTER_FROM {
                        select = select.filter_mode();
                    }
                    if let Some(Value::List(defaults)) = default {
                        let on = (0..free.len())
                            .filter(|&i| {
                                defaults
                                    .iter()
                                    .any(|v| matches!(v, Value::String(s) if s == free[i]))
                            })
                            .collect();
                        select = select.initial_values(on);
                    }
                    picked = answered(select.interact())?;
                }
                Value::List(
                    picked
                        .into_iter()
                        .map(|i| Value::String(free[i].clone()))
                        .chain(fixed.iter().map(|f| Value::String(f.clone())))
                        .collect(),
                )
            }
            AnswerKind::String => {
                let mut input = cliclack::input(&prompt);
                match default {
                    Some(Value::String(s)) if !s.is_empty() => input = input.default_input(s),
                    Some(Value::String(_)) => input = input.required(false),
                    _ => {}
                }
                if !matches!(default, Some(Value::String(s)) if !s.is_empty()) {
                    if let Some(example) = &question.example {
                        input = input.placeholder(example);
                    }
                }
                Value::String(answered(input.interact())?)
            }
            AnswerKind::Secret { .. } => {
                bail!("secret questions are resolved via their source, not ask()")
            }
        };
        Ok(value)
    }

    fn ask_secret(&mut self, question: &Question) -> Result<SecretValue> {
        rail::before_prompt();
        let value = answered(cliclack::password(header(question)).mask('▪').interact())?;
        Ok(SecretValue::new(value))
    }

    fn confirm(&mut self, message: &str, default: bool) -> Result<bool> {
        rail::before_prompt();
        answered(cliclack::confirm(message).initial_value(default).interact())
    }

    fn interactive(&self) -> bool {
        true
    }

    fn choose(&mut self, prompt: &str, items: &[&str], default: usize) -> Result<usize> {
        rail::before_prompt();
        let items: Vec<(usize, &str, &str)> =
            items.iter().enumerate().map(|(i, s)| (i, *s, "")).collect();
        answered(
            cliclack::select(prompt)
                .items(&items)
                .initial_value(default)
                .interact(),
        )
    }

    fn pick(&mut self, prompt: &str, items: &[&str], checked: &[bool]) -> Result<Vec<usize>> {
        rail::before_prompt();
        let items: Vec<(usize, &str, &str)> =
            items.iter().enumerate().map(|(i, s)| (i, *s, "")).collect();
        let on = checked
            .iter()
            .enumerate()
            .filter_map(|(i, on)| on.then_some(i))
            .collect();
        answered(
            cliclack::multiselect(prompt)
                .items(&items)
                .initial_values(on)
                .required(false)
                .interact(),
        )
    }

    fn text(&mut self, prompt: &str, default: &str) -> Result<String> {
        rail::before_prompt();
        let mut input = cliclack::input(prompt);
        input = if default.is_empty() {
            input.required(false)
        } else {
            input.default_input(default)
        };
        answered(input.interact())
    }

    fn edit(&mut self, text: &str, extension: &str) -> Result<String> {
        Ok(edit(text, extension, false)?.unwrap_or_else(|| text.to_owned()))
    }

    fn decide_candidate(
        &mut self,
        message: &str,
        occurrences: &[String],
    ) -> Result<CandidateDecision> {
        if occurrences.is_empty() {
            // Nothing to select from (path-only matches): yes or no.
            let yes = self.confirm(message, true)?;
            return Ok(if yes {
                CandidateDecision::All
            } else {
                CandidateDecision::None
            });
        }
        let choice = self.choose(
            message,
            &[
                "yes — abstract every occurrence",
                "no — keep them all literal",
                "select — choose occurrences to keep literal",
            ],
            0,
        )?;
        match choice {
            0 => Ok(CandidateDecision::All),
            1 => Ok(CandidateDecision::None),
            _ => {
                let items: Vec<&str> = occurrences.iter().map(String::as_str).collect();
                let keep = self.pick(
                    "mark the occurrences to KEEP LITERAL (space toggles)",
                    &items,
                    &vec![false; items.len()],
                )?;
                Ok(CandidateDecision::Except(keep))
            }
        }
    }
}

/// A question's prompt (its id when it has none), with its description
/// under it.
fn header(question: &Question) -> String {
    let prompt = question
        .prompt
        .clone()
        .unwrap_or_else(|| question.id.to_string());
    match &question.description {
        Some(description) => with_detail(&prompt, description),
        None => prompt,
    }
}

/// `prompt` with `detail` dimmed on the lines under it.
fn with_detail(prompt: &str, detail: &str) -> String {
    let detail: Vec<String> = detail
        .lines()
        .map(|l| ui::paint(Out::Stderr, palette::dim(), l).to_string())
        .collect();
    format!("{prompt}\n{}", detail.join("\n"))
}

/// A prompt's answer; Esc or Ctrl-C cancels the command.
fn answered<T>(result: io::Result<T>) -> Result<T> {
    result.map_err(|e| match e.kind() {
        io::ErrorKind::Interrupted => anyhow::Error::new(ui::Cancelled),
        _ => anyhow::Error::new(e).context("reading an answer from the terminal"),
    })
}

/// Open `$VISUAL`/`$EDITOR` (fallback `vi`) on a seeded `.sh` buffer and
/// return the entered command: the seed `instructions` become `#` comment
/// lines, comments are stripped from the result, and multi-line commands
/// are fine (`sh -c`). Errors when the editor closes without saving or
/// nothing was entered.
pub(crate) fn edit_command(instructions: &str) -> Result<String> {
    let seed: String = instructions.lines().map(|l| format!("# {l}\n")).collect();
    let edited = edit(&seed, ".sh", true)?.context("aborted: editor closed without saving")?;
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

/// Edit `text` in the person's editor as a file ending in `extension`.
/// With `require_save`, an editor that exits without saving yields `None`.
/// Trailing newlines are trimmed from the result.
fn edit(text: &str, extension: &str, require_save: bool) -> Result<Option<String>> {
    let open = || -> io::Result<Option<String>> {
        let mut file = tempfile::Builder::new()
            .prefix("edit-")
            .suffix(extension)
            .tempfile()?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        let before = std::fs::metadata(file.path())?.modified()?;
        let command = editor();
        let (program, args) = match shell_words::split(&command) {
            Ok(mut parts) if !parts.is_empty() => {
                let program = parts.remove(0);
                (program, parts)
            }
            _ => (command, Vec::new()),
        };
        let status = Command::new(program).args(args).arg(file.path()).status()?;
        if status.success()
            && require_save
            && before >= std::fs::metadata(file.path())?.modified()?
        {
            return Ok(None);
        }
        let mut edited = std::fs::read_to_string(file.path())?;
        edited.truncate(edited.trim_end_matches(['\n', '\r']).len());
        Ok(Some(edited))
    };
    open().context("opening $VISUAL/$EDITOR")
}

/// `$VISUAL`, else `$EDITOR`, else `vi`; a variable set to blanks counts as
/// unset.
fn editor() -> String {
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|cmd| !cmd.trim().is_empty())
        .unwrap_or_else(|| "vi".to_owned())
}
