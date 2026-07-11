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
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
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
    id: AnswerId,
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
    /// In-progress text edit for the selected row.
    editing: Option<String>,
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
                    continue;
                }
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
                    id: q.id.clone(),
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
                    .map(|i| (i + 1) % choices.len())
                    .unwrap_or(0);
                self.entered.insert(id, Value::String(choices[idx].clone()));
            }
            AnswerKind::Secret { .. } => {}
            _ => {
                let rows = self.rows();
                let current = rows
                    .get(self.selected)
                    .filter(|r| r.source == Source::You)
                    .map(|r| r.value_display.clone())
                    .unwrap_or_default();
                self.editing = Some(current);
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
        if let Some(buffer) = &mut state.editing {
            match key.code {
                KeyCode::Enter => {
                    let raw = buffer.clone();
                    state.commit_text(&raw);
                }
                KeyCode::Esc => state.editing = None,
                KeyCode::Backspace => {
                    buffer.pop();
                }
                KeyCode::Char(c) => buffer.push(c),
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => state.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => state.move_selection(1),
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

fn draw(frame: &mut ratatui::Frame, state: &WizardState, template_name: &str) {
    let rows = state.rows();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let marker = if i == state.selected { "❯ " } else { "  " };
            let source = match row.source {
                Source::You => Span::styled(" (you)", Style::default().fg(Color::Green)),
                Source::Provided => {
                    Span::styled(" (flag/preset)", Style::default().fg(Color::Cyan))
                }
                Source::Default => Span::styled(" (default)", Style::default().fg(Color::DarkGray)),
                Source::Secret => Span::raw(""),
                Source::Unset => Span::styled(" required", Style::default().fg(Color::Red)),
            };
            let value = if let (true, Some(buffer)) = (i == state.selected, &state.editing) {
                Span::styled(
                    format!("{buffer}▏"),
                    Style::default().add_modifier(Modifier::UNDERLINED),
                )
            } else {
                Span::styled(row.value_display.clone(), Style::default().fg(Color::White))
            };
            let mut spans = vec![
                Span::raw(marker),
                Span::styled(
                    format!("{:<16}", row.label),
                    if row.required_missing {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().add_modifier(Modifier::BOLD)
                    },
                ),
                value,
            ];
            if row.editable || row.source == Source::Secret {
                spans.push(source);
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {template_name} — answers ")),
        ),
        chunks[0],
    );

    let detail = state
        .error
        .clone()
        .map(|e| format!("✗ {e}"))
        .or_else(|| {
            rows.get(state.selected).map(|row| {
                let mut d = row.detail.clone();
                if let Some(q) = state.selected_question() {
                    if let Some(example) = &q.example {
                        d.push_str(&format!("  (e.g. {example})"));
                    }
                    if let AnswerKind::Choice { choices } = &q.kind {
                        d.push_str(&format!("  [{}]", choices.join(" | ")));
                    }
                }
                d
            })
        })
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" about ")),
        chunks[1],
    );

    let footer = if state.ready() {
        "↑↓ move · enter edit/toggle/cycle · s scaffold · q cancel"
    } else {
        "↑↓ move · enter edit/toggle/cycle · answer required fields to continue · q cancel"
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
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
