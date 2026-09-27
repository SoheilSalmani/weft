//! Full-screen terminal wizard for answering a template's questions —
//! the interactive path of `weft new`/`weft record`. Works in any terminal
//! (including editors' embedded terminals); `--no-wizard` falls back to
//! sequential prompts.
//!
//! The pure state machine (`WizardState`) is separate from the terminal
//! loop so gating/readiness logic is unit-testable without a TTY.

use std::collections::{BTreeMap, BTreeSet};

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

/// Preset lock/constraint info the wizard enforces: locked questions are
/// skipped entirely; constrained multichoices pin fixed choices and hide
/// blocked ones.
#[derive(Default)]
pub struct PresetLocks {
    pub locked: BTreeSet<AnswerId>,
    /// id → (fixed, blocked)
    pub constraints: BTreeMap<AnswerId, (Vec<String>, Vec<String>)>,
}

/// Multichoice popup marks. In answering mode only Off/On are used; in
/// preset-authoring mode the three states mean free / fixed / blocked.
pub const MARK_OFF: u8 = 0;
pub const MARK_ON: u8 = 1;
pub const MARK_BLOCKED: u8 = 2;

pub struct WizardState<'a> {
    questions: &'a [Question],
    provided: &'a AnswerSet,
    locks: &'a PresetLocks,
    eval: &'a dyn ExprEval,
    /// Preset-authoring mode (`weft presets save`): nothing is required,
    /// answered questions become locks, and multichoice popups author
    /// fixed/blocked constraints instead of selections.
    authoring: bool,
    /// Reconfigure mode (`weft update --reconfigure`): `provided` holds the
    /// project's current answers, and `x` can hand one back to its default.
    reconfigure: bool,
    /// Answers the user set in the wizard.
    pub entered: BTreeMap<AnswerId, Value>,
    /// Reconfigure mode: provided answers handed back to their default.
    /// `entered` still wins over these.
    pub cleared: BTreeSet<AnswerId>,
    /// Constraints authored in preset mode: id → (fixed, blocked).
    pub authored: BTreeMap<AnswerId, (Vec<String>, Vec<String>)>,
    selected: usize,
    /// In-progress text edit for the selected row (buffer, cursor).
    editing: Option<(String, usize)>,
    /// Open choice popup: (cursor, per-option marks for multichoice).
    /// Indices are over the *visible* choices (blocked ones are hidden).
    choosing: Option<(usize, Option<Vec<u8>>)>,
    error: Option<String>,
}

impl<'a> WizardState<'a> {
    pub fn new(
        questions: &'a [Question],
        provided: &'a AnswerSet,
        locks: &'a PresetLocks,
        eval: &'a dyn ExprEval,
    ) -> Self {
        Self {
            questions,
            provided,
            locks,
            eval,
            authoring: false,
            reconfigure: false,
            entered: BTreeMap::new(),
            cleared: BTreeSet::new(),
            authored: BTreeMap::new(),
            selected: 0,
            editing: None,
            choosing: None,
            error: None,
        }
    }

    /// Preset-authoring state, prefilled from flags (locks + constraints).
    pub fn new_author(
        questions: &'a [Question],
        provided: &'a AnswerSet,
        locks: &'a PresetLocks,
        eval: &'a dyn ExprEval,
        prefill_locks: BTreeMap<AnswerId, Value>,
        prefill_constraints: BTreeMap<AnswerId, (Vec<String>, Vec<String>)>,
    ) -> Self {
        let mut state = Self::new(questions, provided, locks, eval);
        state.authoring = true;
        state.entered = prefill_locks;
        state.authored = prefill_constraints;
        state
    }

    /// Reconfigure state over a project's current answers (`provided`).
    pub fn new_reconfigure(
        questions: &'a [Question],
        provided: &'a AnswerSet,
        locks: &'a PresetLocks,
        eval: &'a dyn ExprEval,
    ) -> Self {
        let mut state = Self::new(questions, provided, locks, eval);
        state.reconfigure = true;
        state
    }

    /// A provided value that still applies: not handed back to its default.
    fn provided_value(&self, id: &AnswerId) -> Option<&Value> {
        if self.cleared.contains(id) {
            None
        } else {
            self.provided.get(id)
        }
    }

    /// How many declared questions are locked by the selected presets.
    pub fn locked_count(&self) -> usize {
        self.questions
            .iter()
            .filter(|q| self.locks.locked.contains(&q.id))
            .count()
    }

    fn constraint_for(&self, id: &AnswerId) -> Option<&(Vec<String>, Vec<String>)> {
        self.locks.constraints.get(id)
    }

    /// The choices shown for a question: declared minus blocked.
    fn visible_choices(&self, q: &Question) -> Vec<String> {
        let choices = match &q.kind {
            AnswerKind::Choice { choices } | AnswerKind::MultiChoice { choices } => choices,
            _ => return vec![],
        };
        match self.constraint_for(&q.id) {
            Some((_, blocked)) => choices
                .iter()
                .filter(|c| !blocked.contains(c))
                .cloned()
                .collect(),
            None => choices.clone(),
        }
    }

    /// Per-visible-choice "pinned by preset" flags (fixed choices).
    fn fixed_flags(&self, q: &Question) -> Vec<bool> {
        let visible = self.visible_choices(q);
        match self.constraint_for(&q.id) {
            Some((fixed, _)) => visible.iter().map(|c| fixed.contains(c)).collect(),
            None => vec![false; visible.len()],
        }
    }

    /// Resolve as rendering would: declaration order, gates against the
    /// answers so far (entered > provided > default; a cleared id skips
    /// provided). Returns the active questions with their effective
    /// value/source.
    fn resolve(&self) -> Vec<(usize, Option<Value>, Source)> {
        if self.authoring {
            // A preset may answer any question (even gated ones), so show
            // them all; only secrets and computed questions are off-limits.
            return self
                .questions
                .iter()
                .enumerate()
                .filter(|(_, q)| !q.computed && !matches!(q.kind, AnswerKind::Secret { .. }))
                .map(|(i, q)| {
                    let value = self.entered.get(&q.id).cloned();
                    let source = if value.is_some() || self.authored.contains_key(&q.id) {
                        Source::You
                    } else {
                        Source::Unset
                    };
                    (i, value, source)
                })
                .collect();
        }
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
            // Locked by a preset: answered, not editable, not shown — keep the
            // value in scope so later gates/defaults resolve. Never clearable.
            if self.locks.locked.contains(&q.id) {
                if let Some(v) = self.provided.get(&q.id) {
                    resolved.insert(q.id.clone(), v.clone());
                }
                continue;
            }
            // Computed questions are derived, not shown: resolve their value so
            // later gates/defaults see it, but don't emit a display row.
            if q.computed {
                let value = self
                    .entered
                    .get(&q.id)
                    .cloned()
                    .or_else(|| self.provided_value(&q.id).cloned())
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
            } else if let Some(v) = self.provided_value(&q.id) {
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
                let value_display = if let Some((fixed, blocked)) = self
                    .authored
                    .get(&q.id)
                    .filter(|_| self.authoring && value.is_none())
                {
                    let mut parts = Vec::new();
                    if !fixed.is_empty() {
                        parts.push(format!("fixed: {}", fixed.join(", ")));
                    }
                    if !blocked.is_empty() {
                        parts.push(format!("blocked: {}", blocked.join(", ")));
                    }
                    parts.join(" · ")
                } else {
                    match (&source, &value) {
                        (Source::Secret, _) => match &q.kind {
                            AnswerKind::Secret { source } => format!("(secret · {source})"),
                            _ => unreachable!(),
                        },
                        (_, Some(v)) => v.render_text(),
                        (_, None) => String::new(),
                    }
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
                    required_missing: source == Source::Unset && !self.authoring,
                    editable: source != Source::Secret,
                }
            })
            .collect()
    }

    /// Ids of active, non-secret questions that still have no value.
    /// A preset is allowed to stay partial, so nothing is required when
    /// authoring.
    pub fn missing(&self) -> Vec<AnswerId> {
        if self.authoring {
            return vec![];
        }
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
            // Comma-separated entry, validated against the declared choices
            // and the preset constraint (blocked rejected, fixed unioned in).
            AnswerKind::MultiChoice { choices } => {
                let (fixed, blocked) = self
                    .constraint_for(&id)
                    .cloned()
                    .unwrap_or((vec![], vec![]));
                let mut picked = Vec::new();
                for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if !choices.iter().any(|c| c == part) {
                        self.error = Some(format!("`{part}` is not one of the choices"));
                        return;
                    }
                    if blocked.iter().any(|b| b == part) {
                        self.error = Some(format!("`{part}` is blocked by preset"));
                        return;
                    }
                    picked.push(part.to_owned());
                }
                for f in &fixed {
                    if !picked.iter().any(|p| p == f) {
                        picked.push(f.clone());
                    }
                }
                Value::List(picked.into_iter().map(Value::String).collect())
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
        let q = q.clone();
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
            AnswerKind::MultiChoice { .. } => {
                let visible = self.visible_choices(&q);
                let marks: Vec<u8> = if self.authoring {
                    // Tri-state authoring: seed from the constraint so far.
                    let (fixed, blocked) = self.authored.get(&id).cloned().unwrap_or_default();
                    visible
                        .iter()
                        .map(|c| {
                            if fixed.contains(c) {
                                MARK_ON
                            } else if blocked.contains(c) {
                                MARK_BLOCKED
                            } else {
                                MARK_OFF
                            }
                        })
                        .collect()
                } else {
                    let fixed = self.fixed_flags(&q);
                    let rows = self.resolve();
                    let current = rows.get(self.selected).and_then(|(_, v, _)| v.clone());
                    let mut marks: Vec<u8> = match current {
                        Some(Value::List(items)) => visible
                            .iter()
                            .map(|c| {
                                let on = items
                                    .iter()
                                    .any(|v| matches!(v, Value::String(s) if s == c));
                                if on {
                                    MARK_ON
                                } else {
                                    MARK_OFF
                                }
                            })
                            .collect(),
                        _ => vec![MARK_OFF; visible.len()],
                    };
                    // Fixed choices are always on.
                    for (mark, pinned) in marks.iter_mut().zip(&fixed) {
                        if *pinned {
                            *mark = MARK_ON;
                        }
                    }
                    marks
                };
                let _ = id;
                self.choosing = Some((0, Some(marks)));
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

    /// Drop the selected question's wizard-entered value (and, when
    /// authoring, its constraint), reverting to provided/default/free. In
    /// reconfigure mode, `x` on a current (provided) value hands it back to
    /// the question's default instead.
    fn clear_selected(&mut self) {
        let Some((i, _, source)) = self.resolve().get(self.selected).cloned() else {
            return;
        };
        let q = &self.questions[i];
        let id = q.id.clone();
        self.error = None;
        self.authored.remove(&id);
        if self.entered.remove(&id).is_some() || !self.reconfigure {
            return;
        }
        // Only a current value can be handed back; locked, computed and
        // secret questions never show one here.
        if source != Source::Provided
            || q.computed
            || matches!(q.kind, AnswerKind::Secret { .. })
            || self.locks.locked.contains(&id)
        {
            return;
        }
        if q.default.is_some() {
            self.cleared.insert(id);
        } else {
            self.error = Some(format!("`{id}` has no default to fall back to"));
        }
    }

    /// The reconfigure result: entered values, and the cleared ids the user
    /// didn't then answer anew.
    pub fn reconfigured(&self) -> Reconfigured {
        Reconfigured {
            entered: self
                .entered
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            cleared: self
                .cleared
                .iter()
                .filter(|id| !self.entered.contains_key(*id))
                .cloned()
                .collect(),
        }
    }
}

/// Run the wizard; returns the user's answers (to be layered on top of the
/// provided set). Errors if the user cancels.
pub fn run(
    template_name: &str,
    questions: &[Question],
    provided: &AnswerSet,
    locks: &PresetLocks,
    eval: &dyn ExprEval,
) -> Result<AnswerSet> {
    let mut state = WizardState::new(questions, provided, locks, eval);
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

/// Preset-authoring output: the locks and multichoice constraints entered.
pub type AuthoredPreset = (
    BTreeMap<AnswerId, Value>,
    BTreeMap<AnswerId, (Vec<String>, Vec<String>)>,
);

/// Run the preset-authoring wizard (`weft presets save`): every question is
/// optional, answered questions become locks, and multichoice popups cycle
/// free → fixed → blocked per choice. Errors if the user cancels.
pub fn run_author(
    template_name: &str,
    questions: &[Question],
    prefill_locks: BTreeMap<AnswerId, Value>,
    prefill_constraints: BTreeMap<AnswerId, (Vec<String>, Vec<String>)>,
    eval: &dyn ExprEval,
) -> Result<AuthoredPreset> {
    let provided = AnswerSet::new();
    let locks = PresetLocks::default();
    let mut state = WizardState::new_author(
        questions,
        &provided,
        &locks,
        eval,
        prefill_locks,
        prefill_constraints,
    );
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut state, template_name);
    ratatui::restore();
    result?;
    Ok((state.entered, state.authored))
}

/// What `weft update --reconfigure` gets back: the values the user set in the
/// wizard, and the ids they handed back to their template default.
pub struct Reconfigured {
    pub entered: AnswerSet,
    pub cleared: BTreeSet<AnswerId>,
}

/// Run the wizard over a project's current answers (`current`: the answers
/// the project stores as given, plus any passed on this command line). Rows
/// show current values as `(current)`; `x`/Delete on a current value hands it
/// back to its default (only when the question has a `default`), which then
/// evaluates live against the other answers. Errors if the user cancels.
pub fn run_reconfigure(
    template_name: &str,
    questions: &[Question],
    current: &AnswerSet,
    locks: &PresetLocks,
    eval: &dyn ExprEval,
) -> Result<Reconfigured> {
    let mut state = WizardState::new_reconfigure(questions, current, locks, eval);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut state, template_name);
    ratatui::restore();
    result?;
    Ok(state.reconfigured())
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
        // Choice popup (single or multi). Blocked choices are hidden, so all
        // indices here are over the visible list; fixed ones can't toggle off.
        if state.choosing.is_some() {
            let qi = selected_question_index(state);
            let (choices, fixed) = match state.questions.get(qi) {
                Some(q) => (state.visible_choices(q), state.fixed_flags(q)),
                None => (vec![], vec![]),
            };
            let Some((cursor, multi)) = &mut state.choosing else {
                continue;
            };
            let len = choices.len().max(1);
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => *cursor = (*cursor + len - 1) % len,
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => *cursor = (*cursor + 1) % len,
                KeyCode::Char(' ') => match multi {
                    Some(marks) => {
                        if state.authoring {
                            // Cycle free → fixed → blocked.
                            if let Some(m) = marks.get_mut(*cursor) {
                                *m = (*m + 1) % 3;
                            }
                        } else if !fixed.get(*cursor).copied().unwrap_or(false) {
                            if let Some(m) = marks.get_mut(*cursor) {
                                *m = if *m == MARK_ON { MARK_OFF } else { MARK_ON };
                            }
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
            KeyCode::Char('x') | KeyCode::Delete => state.clear_selected(),
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

/// Commit the open popup's selection: into `entered` when answering, into
/// `authored` (a fixed/blocked constraint) when authoring a preset.
fn commit_choice(state: &mut WizardState, choices: &[String]) {
    let Some((cursor, multi)) = state.choosing.take() else {
        return;
    };
    let idx = selected_question_index(state);
    let Some(q) = state.questions.get(idx) else {
        return;
    };
    match (&q.kind, multi) {
        (AnswerKind::MultiChoice { .. }, Some(marks)) if state.authoring => {
            let picked = |mark: u8| -> Vec<String> {
                choices
                    .iter()
                    .zip(&marks)
                    .filter(|(_, m)| **m == mark)
                    .map(|(c, _)| c.clone())
                    .collect()
            };
            let (fixed, blocked) = (picked(MARK_ON), picked(MARK_BLOCKED));
            let id = q.id.clone();
            if fixed.is_empty() && blocked.is_empty() {
                state.authored.remove(&id);
            } else {
                state.authored.insert(id.clone(), (fixed, blocked));
            }
            // A constraint replaces any full lock on the same question.
            state.entered.remove(&id);
        }
        (AnswerKind::MultiChoice { .. }, Some(marks)) => {
            let value = Value::List(
                choices
                    .iter()
                    .zip(&marks)
                    .filter(|(_, m)| **m == MARK_ON)
                    .map(|(c, _)| Value::String(c.clone()))
                    .collect(),
            );
            state.entered.insert(q.id.clone(), value);
        }
        _ => {
            let value = Value::String(choices.get(cursor).cloned().unwrap_or_default());
            state.entered.insert(q.id.clone(), value);
        }
    }
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
    let title = if state.authoring {
        "weft preset save"
    } else if state.reconfigure {
        "weft update --reconfigure"
    } else {
        "weft answers"
    };
    let mut header = vec![
        Span::raw(" "),
        Span::styled(
            title.to_owned(),
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(format!("template {template_name}"), Theme::dim()),
    ];
    let locked = state.locked_count();
    if locked > 0 {
        header.push(Span::styled(
            format!("  · {locked} answer{} locked by preset", plural(locked)),
            Theme::dim(),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(header)), chunks[0]);

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
                Source::You if state.authoring => {
                    Span::styled(" (in preset)", Style::default().fg(Theme::OK))
                }
                Source::You => Span::styled(" (you)", Style::default().fg(Theme::OK)),
                Source::Provided if state.reconfigure => {
                    Span::styled(" (current)", Theme::accent())
                }
                Source::Provided => Span::styled(" (flag/preset)", Theme::accent()),
                Source::Default => Span::styled(" (default)", Theme::dim()),
                Source::Secret => Span::raw(""),
                Source::Unset if state.authoring => Span::styled(" free", Theme::dim()),
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
            (
                "space",
                if state.authoring {
                    "cycle free/fixed/blocked"
                } else {
                    "toggle"
                },
            ),
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
            (
                "x",
                if state.reconfigure {
                    "reset to default"
                } else {
                    "clear"
                },
            ),
            ("s", if state.authoring { "save" } else { "continue" }),
            ("q", "cancel"),
        ]
    } else if state.reconfigure {
        vec![
            ("↑↓", "move"),
            ("enter", "edit"),
            ("x", "reset to default"),
            ("answer required fields", "to continue"),
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

    // Choice popup overlay (single or multi). Blocked choices are hidden;
    // fixed ones render pinned and can't be toggled.
    if let Some((cursor, multi)) = &state.choosing {
        if let Some(q) = state.selected_question() {
            let choices = state.visible_choices(q);
            let fixed = state.fixed_flags(q);
            if choices.is_empty() {
                return;
            }
            // Never let the min exceed the available max (tiny terminals).
            let max_width = (frame.area().width.saturating_sub(6) as usize).max(1);
            let width = choices
                .iter()
                .map(|c| c.len() + 18)
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
                    let pinned = fixed.get(i).copied().unwrap_or(false);
                    let mark = multi
                        .as_ref()
                        .map(|marks| marks.get(i).copied().unwrap_or(MARK_OFF));
                    let marker = match (multi, mark) {
                        (Some(_), _) if pinned => Span::styled("◉ ", Theme::dim()),
                        (Some(_), Some(MARK_ON)) => Span::styled("◉ ", Theme::accent()),
                        (Some(_), Some(MARK_BLOCKED)) => Span::styled("✗ ", Theme::error()),
                        (Some(_), _) => Span::styled("○ ", Theme::dim()),
                        (None, _) => Span::raw("  "),
                    };
                    let style = if i == *cursor {
                        Style::default()
                            .fg(Theme::ACCENT)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Theme::text()
                    };
                    let mut spans = vec![
                        Span::raw(if i == *cursor { "❯ " } else { "  " }),
                        marker,
                        Span::styled(choice.clone(), style),
                    ];
                    if pinned {
                        spans.push(Span::styled(" (fixed)", Theme::dim()));
                    } else if state.authoring {
                        match mark {
                            Some(MARK_ON) => {
                                spans.push(Span::styled(" (fixed)", Theme::dim()));
                            }
                            Some(MARK_BLOCKED) => {
                                spans.push(Span::styled(" (blocked)", Theme::dim()));
                            }
                            _ => {}
                        }
                    }
                    ListItem::new(Line::from(spans))
                })
                .collect();
            frame.render_widget(Clear, area);
            frame.render_widget(List::new(items).block(Theme::popup_panel(&q.id.0)), area);
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
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

    fn no_locks() -> PresetLocks {
        PresetLocks::default()
    }

    #[test]
    fn gated_questions_appear_when_gate_opens() {
        let questions = vec![
            question("use_docker", AnswerKind::Bool, Some("False"), None),
            question("registry", AnswerKind::String, None, Some("use_docker")),
        ];
        let provided = AnswerSet::new();
        let locks = no_locks();
        let mut state = WizardState::new(&questions, &provided, &locks, &StubEval);

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
        let locks = no_locks();
        let mut state = WizardState::new(&questions, &provided, &locks, &StubEval);

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
        let locks = no_locks();
        let mut state = WizardState::new(&questions, &provided, &locks, &StubEval);
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
        let locks = no_locks();
        let state = WizardState::new(&questions, &provided, &locks, &StubEval);
        assert!(state.ready());
    }

    #[test]
    fn locked_questions_are_skipped_but_feed_gates() {
        let questions = vec![
            question("use_docker", AnswerKind::Bool, None, None),
            question("registry", AnswerKind::String, None, Some("use_docker")),
            question("name", AnswerKind::String, None, None),
        ];
        // The preset locks use_docker=true; its value arrives via `provided`.
        let provided: AnswerSet = [(AnswerId::from("use_docker"), Value::Bool(true))]
            .into_iter()
            .collect();
        let locks = PresetLocks {
            locked: [AnswerId::from("use_docker")].into_iter().collect(),
            constraints: BTreeMap::new(),
        };
        let state = WizardState::new(&questions, &provided, &locks, &StubEval);

        // use_docker has no row, but its lock opened the registry gate.
        let ids: Vec<_> = state
            .resolve()
            .iter()
            .map(|(i, _, _)| questions[*i].id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![AnswerId::from("registry"), AnswerId::from("name")]
        );
        assert_eq!(state.locked_count(), 1);
    }

    #[test]
    fn constrained_multichoice_hides_blocked_and_pins_fixed() {
        let questions = vec![question(
            "features",
            AnswerKind::MultiChoice {
                choices: vec!["lint".into(), "ci".into(), "experimental".into()],
            },
            None,
            None,
        )];
        let provided = AnswerSet::new();
        let locks = PresetLocks {
            locked: BTreeSet::new(),
            constraints: [(
                AnswerId::from("features"),
                (vec!["lint".to_owned()], vec!["experimental".to_owned()]),
            )]
            .into_iter()
            .collect(),
        };
        let mut state = WizardState::new(&questions, &provided, &locks, &StubEval);

        assert_eq!(state.visible_choices(&questions[0]), vec!["lint", "ci"]);
        assert_eq!(state.fixed_flags(&questions[0]), vec![true, false]);

        // Opening the popup seeds the fixed choice on.
        state.activate();
        assert_eq!(state.choosing, Some((0, Some(vec![MARK_ON, MARK_OFF]))));

        // Text entry: blocked rejected, fixed unioned in.
        state.choosing = None;
        state.commit_text("experimental");
        assert!(state.error.is_some());
        state.commit_text("ci");
        assert_eq!(
            state.entered.get(&"features".into()),
            Some(&Value::List(vec![
                Value::String("ci".into()),
                Value::String("lint".into()),
            ]))
        );
    }

    /// `project_name` (no default) and `package_name` (defaults to it), both
    /// currently set.
    fn reconfigure_fixture() -> (Vec<Question>, AnswerSet) {
        let questions = vec![
            question("project_name", AnswerKind::String, None, None),
            question(
                "package_name",
                AnswerKind::String,
                Some("project_name"),
                None,
            ),
        ];
        let current: AnswerSet = [
            (AnswerId::from("project_name"), Value::String("old".into())),
            (
                AnswerId::from("package_name"),
                Value::String("pinned".into()),
            ),
        ]
        .into_iter()
        .collect();
        (questions, current)
    }

    fn row_value(state: &WizardState, row: usize) -> (Option<Value>, Source) {
        let (_, value, source) = state.resolve()[row].clone();
        (value, source)
    }

    #[test]
    fn reconfigure_clear_falls_back_to_live_default() {
        let (questions, current) = reconfigure_fixture();
        let locks = no_locks();
        let mut state = WizardState::new_reconfigure(&questions, &current, &locks, &StubEval);

        state.selected = 1;
        state.clear_selected();
        assert!(state.error.is_none());
        let (value, source) = row_value(&state, 1);
        assert_eq!(value, Some(Value::String("old".into())));
        assert!(source == Source::Default);

        // The default tracks the other answers live.
        state.selected = 0;
        state.commit_text("new");
        assert_eq!(row_value(&state, 1).0, Some(Value::String("new".into())));

        let out = state.reconfigured();
        assert_eq!(
            out.cleared,
            [AnswerId::from("package_name")].into_iter().collect()
        );
        assert_eq!(
            out.entered.get(&"project_name".into()),
            Some(&Value::String("new".into()))
        );
    }

    #[test]
    fn reconfigure_clear_without_default_keeps_current_and_errors() {
        let (questions, current) = reconfigure_fixture();
        let locks = no_locks();
        let mut state = WizardState::new_reconfigure(&questions, &current, &locks, &StubEval);

        state.clear_selected(); // row 0: project_name, no default
        assert!(state.error.is_some());
        let (value, source) = row_value(&state, 0);
        assert_eq!(value, Some(Value::String("old".into())));
        assert!(source == Source::Provided);
        assert!(state.reconfigured().cleared.is_empty());
    }

    #[test]
    fn reconfigure_reentering_after_clear_reports_entered_not_cleared() {
        let (questions, current) = reconfigure_fixture();
        let locks = no_locks();
        let mut state = WizardState::new_reconfigure(&questions, &current, &locks, &StubEval);

        state.selected = 1;
        state.clear_selected();
        state.commit_text("fresh");
        let (value, source) = row_value(&state, 1);
        assert_eq!(value, Some(Value::String("fresh".into())));
        assert!(source == Source::You);

        let out = state.reconfigured();
        assert!(out.cleared.is_empty());
        assert_eq!(
            out.entered.get(&"package_name".into()),
            Some(&Value::String("fresh".into()))
        );
    }

    #[test]
    fn clear_does_not_drop_provided_value_outside_reconfigure() {
        let (questions, current) = reconfigure_fixture();
        let locks = no_locks();
        let mut state = WizardState::new(&questions, &current, &locks, &StubEval);

        state.selected = 1;
        state.clear_selected();
        assert!(state.cleared.is_empty());
        assert!(state.error.is_none());
        let (value, source) = row_value(&state, 1);
        assert_eq!(value, Some(Value::String("pinned".into())));
        assert!(source == Source::Provided);
    }
}
