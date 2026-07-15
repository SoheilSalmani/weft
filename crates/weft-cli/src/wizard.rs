//! Full-screen terminal wizard for answering a template's questions —
//! the interactive path of `weft new`/`weft record`. Works in any terminal
//! (including editors' embedded terminals); `--no-wizard` falls back to
//! sequential prompts.
//!
//! The pure state machine (`WizardState`) is separate from the terminal
//! loop so gating/readiness logic is unit-testable without a TTY.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph, Wrap};

use crate::tui::theme::Theme;
use weft_core::render::ExprEval;
use weft_core::{AnswerId, AnswerKind, AnswerSet, Question, Value};

/// Where a row's current value comes from (display only).
#[derive(Clone, Copy, PartialEq)]
enum Source {
    You,
    Provided,
    Default,
    Unset,
    Secret,
}

struct Row {
    label: String,
    detail: String,
    value_display: String,
    source: Source,
    required_missing: bool,
    editable: bool,
}

pub struct WizardState<'a> {
    questions: &'a [Question],
    provided: &'a AnswerSet,
    eval: &'a dyn ExprEval,
    /// Answers the user set in the wizard.
    pub entered: BTreeMap<AnswerId, Value>,
    selected: usize,
    /// In-progress text edit for the selected row (buffer, cursor).
    editing: Option<(String, usize)>,
    /// Open choice popup: (cursor, per-option toggles for multichoice).
    choosing: Option<(usize, Option<Vec<bool>>)>,
    error: Option<String>,
}

impl<'a> WizardState<'a> {
    pub fn new(questions: &'a [Question], provided: &'a AnswerSet, eval: &'a dyn ExprEval) -> Self {
        Self {
            questions,
            provided,
            eval,
            entered: BTreeMap::new(),
            selected: 0,
            editing: None,
            choosing: None,
            error: None,
        }
    }

    /// Resolve as rendering would: declaration order, gates against the
    /// answers so far (entered > provided > default). Returns the active
    /// questions with their effective value/source.
    fn resolve(&self) -> Vec<(usize, Option<Value>, Source)> {
        let mut resolved = AnswerSet::new();
        let mut rows = Vec::new();
        for (i, q) in self.questions.iter().enumerate() {
            if let Some(when) = &q.when {
                if !self.eval.eval_bool(when, &resolved).unwrap_or(false) {
                    // Not shown, but keep its default in scope so later gates
                    // resolve as they will at render time.
                    if let Some(default) = &q.default {
                        if let Ok(v) = self.eval.eval(default, &resolved) {
                            resolved.insert(q.id.clone(), v);
                        }
                    }
                    continue;
                }
            }
            // Computed questions are derived, not shown: resolve their value so
            // later gates/defaults see it, but don't emit a display row.
            if q.computed {
                let value = self
                    .entered
                    .get(&q.id)
                    .cloned()
                    .or_else(|| self.provided.get(&q.id).cloned())
                    .or_else(|| {
                        q.default
                            .as_ref()
                            .and_then(|d| self.eval.eval(d, &resolved).ok())
                    });
                if let Some(v) = value {
                    resolved.insert(q.id.clone(), v);
                }
                continue;
            }
            if matches!(q.kind, AnswerKind::Secret { .. }) {
                rows.push((i, None, Source::Secret));
                continue;
            }
            let (value, source) = if let Some(v) = self.entered.get(&q.id) {
                (Some(v.clone()), Source::You)
            } else if let Some(v) = self.provided.get(&q.id) {
                (Some(v.clone()), Source::Provided)
            } else if let Some(default) = &q.default {
                (self.eval.eval(default, &resolved).ok(), Source::Default)
            } else {
                (None, Source::Unset)
            };
            if let Some(v) = &value {
                resolved.insert(q.id.clone(), v.clone());
            }
            rows.push((i, value, source));
        }
        rows
    }

    fn rows(&self) -> Vec<Row> {
        self.resolve()
            .into_iter()
            .map(|(i, value, source)| {
                let q = &self.questions[i];
                let value_display = match (&source, &value) {
                    (Source::Secret, _) => match &q.kind {
                        AnswerKind::Secret { source } => format!("(secret · {source})"),
                        _ => unreachable!(),
                    },
                    (_, Some(v)) => v.render_text(),
                    (_, None) => String::new(),
                };
                Row {
                    label: q.id.0.clone(),
                    detail: q
                        .description
                        .clone()
                        .or_else(|| q.prompt.clone())
                        .unwrap_or_default(),
                    value_display,
                    source,
                    required_missing: source == Source::Unset,
                    editable: source != Source::Secret,
                }
            })
            .collect()
    }

    /// Ids of active, non-secret questions that still have no value.
    pub fn missing(&self) -> Vec<AnswerId> {
        self.resolve()
            .into_iter()
            .filter(|(_, value, source)| *source != Source::Secret && value.is_none())
            .map(|(i, _, _)| self.questions[i].id.clone())
            .collect()
    }

    pub fn ready(&self) -> bool {
        self.missing().is_empty()
    }

    fn selected_question(&self) -> Option<&Question> {
        let rows = self.resolve();
        rows.get(self.selected).map(|(i, _, _)| &self.questions[*i])
    }

    fn move_selection(&mut self, delta: i64) {
        let len = self.resolve().len();
        if len == 0 {
            return;
        }
        let next = (self.selected as i64 + delta).rem_euclid(len as i64);
        self.selected = next as usize;
        self.editing = None;
        self.choosing = None;
        self.error = None;
    }

    /// Commit a raw text edit for the selected question, coercing by kind.
    pub fn commit_text(&mut self, raw: &str) {
        let Some(q) = self.selected_question() else {
            return;
        };
        let id = q.id.clone();
        if raw.is_empty() {
            // clear the user's override
            self.entered.remove(&id);
            self.editing = None;
            return;
        }
        let value = match &q.kind {
            AnswerKind::Int => match raw.parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) => {
                    self.error = Some(format!("`{raw}` is not an integer"));
                    return;
                }
            },
            // Comma-separated entry, validated against the declared choices.
            AnswerKind::MultiChoice { choices } => {
                let mut items = Vec::new();
                for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if !choices.iter().any(|c| c == part) {
                        self.error = Some(format!("`{part}` is not one of the choices"));
                        return;
                    }
                    items.push(Value::String(part.to_owned()));
                }
                Value::List(items)
            }
            _ => Value::String(raw.to_owned()),
        };
        self.entered.insert(id, value);
        self.editing = None;
        self.error = None;
    }

    /// Enter/space on a row: toggle bools, cycle choices, start text edit.
    fn activate(&mut self) {
        let Some(q) = self.selected_question() else {
            return;
        };
        let id = q.id.clone();
        match &q.kind {
            AnswerKind::Bool => {
                let rows = self.resolve();
                let current = rows
                    .get(self.selected)
                    .and_then(|(_, v, _)| v.clone())
                    .map(|v| matches!(v, Value::Bool(true)))
                    .unwrap_or(false);
                self.entered.insert(id, Value::Bool(!current));
            }
            AnswerKind::Choice { choices } => {
                let rows = self.resolve();
                let current = rows.get(self.selected).and_then(|(_, v, _)| v.clone());
                let idx = current
                    .and_then(|v| match v {
                        Value::String(s) => choices.iter().position(|c| *c == s),
                        _ => None,
                    })
                    .unwrap_or(0);
                let _ = id;
                self.choosing = Some((idx, None));
            }
            AnswerKind::MultiChoice { choices } => {
                let rows = self.resolve();
                let current = rows.get(self.selected).and_then(|(_, v, _)| v.clone());
                let selected: Vec<bool> = match current {
                    Some(Value::List(items)) => choices
                        .iter()
                        .map(|c| {
                            items
                                .iter()
                                .any(|v| matches!(v, Value::String(s) if s == c))
                        })
                        .collect(),
                    _ => vec![false; choices.len()],
                };
                let _ = id;
                self.choosing = Some((0, Some(selected)));
            }
            AnswerKind::Secret { .. } => {}
            _ => {
                let rows = self.rows();
                let current = rows
                    .get(self.selected)
                    .filter(|r| r.source == Source::You)
                    .map(|r| r.value_display.clone())
                    .unwrap_or_default();
                let cursor = current.chars().count();
                self.editing = Some((current, cursor));
            }
        }
    }
}

/// Run the wizard; returns the user's answers (to be layered on top of the
/// provided set). Errors if the user cancels.
pub fn run(
    template_name: &str,
    questions: &[Question],
    provided: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<AnswerSet> {
    let mut state = WizardState::new(questions, provided, eval);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut state, template_name);
    ratatui::restore();
    result?;
    Ok(state
        .entered
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    state: &mut WizardState,
    template_name: &str,
) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, state, template_name))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        // Choice popup (single or multi).
        if state.choosing.is_some() {
            let qi = selected_question_index(state);
            let choices = match state.questions.get(qi) {
                Some(q) => match &q.kind {
                    AnswerKind::Choice { choices } | AnswerKind::MultiChoice { choices } => {
                        choices.clone()
                    }
                    _ => vec![],
                },
                None => vec![],
            };
            let Some((cursor, multi)) = &mut state.choosing else {
                continue;
            };
            let len = choices.len().max(1);
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => *cursor = (*cursor + len - 1) % len,
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => *cursor = (*cursor + 1) % len,
                KeyCode::Char(' ') => match multi {
                    Some(on) => {
                        if let Some(flag) = on.get_mut(*cursor) {
                            *flag = !*flag;
                        }
                    }
                    None => {
                        commit_choice(state, &choices);
                    }
                },
                KeyCode::Enter => commit_choice(state, &choices),
                KeyCode::Esc => state.choosing = None,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    bail!("cancelled")
                }
                _ => {}
            }
            continue;
        }
        // In-place text editing with a cursor.
        if let Some((buffer, cursor)) = &mut state.editing {
            match key.code {
                KeyCode::Enter => {
                    let raw = buffer.clone();
                    state.commit_text(&raw);
                }
                KeyCode::Esc => state.editing = None,
                KeyCode::Left => *cursor = cursor.saturating_sub(1),
                KeyCode::Right => *cursor = (*cursor + 1).min(buffer.chars().count()),
                KeyCode::Home => *cursor = 0,
                KeyCode::End => *cursor = buffer.chars().count(),
                KeyCode::Backspace => {
                    if *cursor > 0 {
                        let byte = char_byte(buffer, *cursor - 1);
                        buffer.remove(byte);
                        *cursor -= 1;
                    }
                }
                KeyCode::Delete => {
                    if *cursor < buffer.chars().count() {
                        let byte = char_byte(buffer, *cursor);
                        buffer.remove(byte);
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    buffer.clear();
                    *cursor = 0;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    bail!("cancelled")
                }
                KeyCode::Char(c) => {
                    let byte = char_byte(buffer, *cursor);
                    buffer.insert(byte, c);
                    *cursor += 1;
                }
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => state.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => state.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => state.activate(),
            KeyCode::Char('s') if state.ready() => return Ok(()),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                bail!("cancelled")
            }
            KeyCode::Esc | KeyCode::Char('q') => bail!("cancelled"),
            _ => {}
        }
    }
}

fn char_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// The questions-index of the currently selected row.
fn selected_question_index(state: &WizardState) -> usize {
    state
        .resolve()
        .get(state.selected)
        .map(|(i, _, _)| *i)
        .unwrap_or(0)
}

/// Commit the open popup's selection into `entered`.
fn commit_choice(state: &mut WizardState, choices: &[String]) {
    let Some((cursor, multi)) = state.choosing.take() else {
        return;
    };
    let idx = selected_question_index(state);
    let Some(q) = state.questions.get(idx) else {
        return;
    };
    let value = match (&q.kind, multi) {
        (AnswerKind::MultiChoice { .. }, Some(on)) => Value::List(
            choices
                .iter()
                .zip(on)
                .filter(|(_, sel)| *sel)
                .map(|(c, _)| Value::String(c.clone()))
                .collect(),
        ),
        _ => Value::String(choices.get(cursor).cloned().unwrap_or_default()),
    };
    state.entered.insert(q.id.clone(), value);
    state.error = None;
}

fn draw(frame: &mut ratatui::Frame, state: &WizardState, template_name: &str) {
    let rows = state.rows();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    // Header: command + context, same grammar as the command forms.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "weft answers".to_owned(),
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(format!("template {template_name}"), Theme::dim()),
        ])),
        chunks[0],
    );

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let focused = i == state.selected;
            let bar = if focused {
                Span::styled("▎ ", Theme::accent())
            } else {
                Span::raw("  ")
            };
            let label_style = if row.required_missing {
                Style::default()
                    .fg(Theme::ERROR)
                    .add_modifier(Modifier::BOLD)
            } else if focused {
                Theme::label_focused()
            } else {
                Theme::label()
            };
            let source = match row.source {
                Source::You => Span::styled(" (you)", Style::default().fg(Theme::OK)),
                Source::Provided => Span::styled(" (flag/preset)", Theme::accent()),
                Source::Default => Span::styled(" (default)", Theme::dim()),
                Source::Secret => Span::raw(""),
                Source::Unset => Span::styled(" required", Theme::error()),
            };
            // In-place edit shows a real cursor.
            if let (true, Some((buffer, cursor))) = (focused, &state.editing) {
                let chars: Vec<char> = buffer.chars().collect();
                let before: String = chars[..*cursor].iter().collect();
                let after: String = chars[*cursor..].iter().collect();
                return ListItem::new(Line::from(vec![
                    bar,
                    Span::styled(format!("{:>16}  ", row.label), label_style),
                    Span::styled(before, Theme::text()),
                    Span::styled("▏", Theme::accent()),
                    Span::styled(after, Theme::text()),
                ]));
            }
            let value = if row.value_display.is_empty() && row.source == Source::Unset {
                Span::styled("—".to_owned(), Theme::dim().add_modifier(Modifier::ITALIC))
            } else {
                Span::styled(row.value_display.clone(), Theme::text())
            };
            let mut spans = vec![
                bar,
                Span::styled(format!("{:>16}  ", row.label), label_style),
                value,
            ];
            if row.editable || row.source == Source::Secret {
                spans.push(source);
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    frame.render_widget(List::new(items).block(Theme::panel("answers")), chunks[1]);

    let detail = state
        .error
        .clone()
        .map(|e| Line::from(Span::styled(format!("✗ {e}"), Theme::error())))
        .or_else(|| {
            rows.get(state.selected).map(|row| {
                let mut d = row.detail.clone();
                if let Some(q) = state.selected_question() {
                    if let Some(example) = &q.example {
                        d.push_str(&format!("  (e.g. {example})"));
                    }
                }
                Line::from(Span::styled(d, Theme::dim()))
            })
        })
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: true })
            .block(Theme::panel("about")),
        chunks[2],
    );

    let keys: Vec<(&str, &str)> = if state.choosing.is_some() {
        vec![
            ("↑↓", "move"),
            ("space", "toggle"),
            ("enter", "choose"),
            ("esc", "close"),
        ]
    } else if state.editing.is_some() {
        vec![
            ("enter", "done"),
            ("←→", "cursor"),
            ("^u", "clear"),
            ("esc", "back"),
        ]
    } else if state.ready() {
        vec![
            ("↑↓", "move"),
            ("enter", "edit"),
            ("s", "continue"),
            ("q", "cancel"),
        ]
    } else {
        vec![
            ("↑↓", "move"),
            ("enter", "edit"),
            ("answer required fields", "to continue"),
            ("q", "cancel"),
        ]
    };
    frame.render_widget(Paragraph::new(Theme::keymap(&keys)), chunks[3]);

    // Choice popup overlay (single or multi).
    if let Some((cursor, multi)) = &state.choosing {
        if let Some(q) = state.selected_question() {
            let choices = match &q.kind {
                AnswerKind::Choice { choices } | AnswerKind::MultiChoice { choices } => choices,
                _ => return,
            };
            // Never let the min exceed the available max (tiny terminals).
            let max_width = (frame.area().width.saturating_sub(6) as usize).max(1);
            let width = choices
                .iter()
                .map(|c| c.len() + 10)
                .max()
                .unwrap_or(20)
                .max(24.min(max_width))
                .min(max_width) as u16;
            let height =
                (choices.len() as u16 + 2).min(frame.area().height.saturating_sub(4).max(3));
            let x = frame.area().x + frame.area().width.saturating_sub(width) / 2;
            let y = frame.area().y + frame.area().height.saturating_sub(height) / 2;
            let area = ratatui::layout::Rect {
                x,
                y,
                width,
                height,
            };
            let items: Vec<ListItem> = choices
                .iter()
                .enumerate()
                .map(|(i, choice)| {
                    let marker = match multi {
                        Some(on) if on.get(i).copied().unwrap_or(false) => {
                            Span::styled("◉ ", Theme::accent())
                        }
                        Some(_) => Span::styled("○ ", Theme::dim()),
                        None => Span::raw("  "),
                    };
                    let style = if i == *cursor {
                        Style::default()
                            .fg(Theme::ACCENT)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Theme::text()
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(if i == *cursor { "❯ " } else { "  " }),
                        marker,
                        Span::styled(choice.clone(), style),
                    ]))
                })
                .collect();
            frame.render_widget(Clear, area);
            frame.render_widget(List::new(items).block(Theme::popup_panel(&q.id.0)), area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weft_core::render::EvalError;
    use weft_core::StarlarkExpr;

    struct StubEval;
    impl ExprEval for StubEval {
        fn eval(&self, expr: &StarlarkExpr, answers: &AnswerSet) -> Result<Value, EvalError> {
            match expr.as_str() {
                "True" => Ok(Value::Bool(true)),
                "False" => Ok(Value::Bool(false)),
                name => answers
                    .get(&AnswerId(name.to_owned()))
                    .cloned()
                    .ok_or_else(|| EvalError {
                        message: format!("unknown {name}"),
                    }),
            }
        }
    }

    fn question(id: &str, kind: AnswerKind, default: Option<&str>, when: Option<&str>) -> Question {
        Question {
            id: id.into(),
            kind,
            prompt: None,
            description: None,
            example: None,
            default: default.map(StarlarkExpr::from),
            when: when.map(StarlarkExpr::from),
            computed: false,
            section: None,
        }
    }

    #[test]
    fn gated_questions_appear_when_gate_opens() {
        let questions = vec![
            question("use_docker", AnswerKind::Bool, Some("False"), None),
            question("registry", AnswerKind::String, None, Some("use_docker")),
        ];
        let provided = AnswerSet::new();
        let mut state = WizardState::new(&questions, &provided, &StubEval);

        // gate closed: registry hidden, nothing missing
        assert_eq!(state.resolve().len(), 1);
        assert!(state.ready());

        // toggle use_docker on -> registry appears and is required
        state.activate(); // bool toggle on selected row 0
        assert_eq!(state.resolve().len(), 2);
        assert_eq!(state.missing(), vec![AnswerId::from("registry")]);
        assert!(!state.ready());
    }

    #[test]
    fn computed_question_is_hidden_but_feeds_gates() {
        let mut derived = question("add_dotenv", AnswerKind::Bool, Some("use_prisma"), None);
        derived.computed = true;
        let questions = vec![
            question("use_prisma", AnswerKind::Bool, Some("False"), None),
            derived,
            question("dotenv_path", AnswerKind::String, None, Some("add_dotenv")),
        ];
        let provided = AnswerSet::new();
        let mut state = WizardState::new(&questions, &provided, &StubEval);

        // computed row never shows; gate closed -> only use_prisma is a row
        let ids: Vec<_> = state
            .resolve()
            .iter()
            .map(|(i, _, _)| questions[*i].id.clone())
            .collect();
        assert_eq!(ids, vec![AnswerId::from("use_prisma")]);

        // toggle use_prisma on -> add_dotenv derives true -> dotenv_path appears
        state.activate();
        let ids: Vec<_> = state
            .resolve()
            .iter()
            .map(|(i, _, _)| questions[*i].id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![AnswerId::from("use_prisma"), AnswerId::from("dotenv_path")]
        );
    }

    #[test]
    fn int_coercion_and_clear() {
        let questions = vec![question("workers", AnswerKind::Int, None, None)];
        let provided = AnswerSet::new();
        let mut state = WizardState::new(&questions, &provided, &StubEval);
        state.commit_text("abc");
        assert!(state.error.is_some());
        assert!(!state.ready());
        state.commit_text("4");
        assert_eq!(state.entered.get(&"workers".into()), Some(&Value::Int(4)));
        assert!(state.ready());
        state.commit_text("");
        assert!(!state.ready());
    }

    #[test]
    fn provided_answers_count_as_answered() {
        let questions = vec![question("name", AnswerKind::String, None, None)];
        let provided: AnswerSet = [(AnswerId::from("name"), Value::String("x".into()))]
            .into_iter()
            .collect();
        let state = WizardState::new(&questions, &provided, &StubEval);
        assert!(state.ready());
    }
}
