//! The form model behind every "complete the missing options" screen.
//!
//! Pure state machine (`FormState`) separated from the terminal loop so
//! navigation/editing/validation are unit-testable without a TTY. Provided
//! CLI flags prefill fields; submit validates and the caller reads values
//! back by field key.

use anyhow::{bail, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

/// One option of a Select/MultiSelect.
#[derive(Clone)]
pub struct Choice {
    pub value: String,
    /// Shown in the list (defaults to `value`).
    pub label: String,
    /// One-line explanation shown next to the option.
    pub help: String,
}

impl Choice {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            label: value.clone(),
            value,
            help: String::new(),
        }
    }

    pub fn labeled(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            help: String::new(),
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }
}

pub enum FieldKind {
    Text {
        value: String,
        cursor: usize,
        placeholder: String,
        /// Return an error message for an invalid value.
        validate: Option<fn(&str) -> Option<String>>,
    },
    Select {
        options: Vec<Choice>,
        chosen: Option<usize>,
    },
    MultiSelect {
        options: Vec<Choice>,
        chosen: Vec<bool>,
    },
    Toggle {
        value: bool,
    },
    /// Free-form repeatable strings (tags, hook inputs).
    StringList {
        items: Vec<String>,
        validate: Option<fn(&str) -> Option<String>>,
    },
}

pub struct Field {
    /// Machine key the caller reads back by.
    pub key: &'static str,
    pub label: String,
    pub help: String,
    pub required: bool,
    pub kind: FieldKind,
    pub error: Option<String>,
}

impl Field {
    pub fn text(key: &'static str, label: &str, prefill: Option<String>) -> Self {
        let value = prefill.unwrap_or_default();
        Self {
            key,
            label: label.to_owned(),
            help: String::new(),
            required: false,
            kind: FieldKind::Text {
                cursor: value.chars().count(),
                value,
                placeholder: String::new(),
                validate: None,
            },
            error: None,
        }
    }

    pub fn select(
        key: &'static str,
        label: &str,
        options: Vec<Choice>,
        prefill: Option<&str>,
    ) -> Self {
        let chosen = prefill.and_then(|p| options.iter().position(|c| c.value == p));
        Self {
            key,
            label: label.to_owned(),
            help: String::new(),
            required: false,
            kind: FieldKind::Select { options, chosen },
            error: None,
        }
    }

    pub fn multi_select(
        key: &'static str,
        label: &str,
        options: Vec<Choice>,
        prefill: &[String],
    ) -> Self {
        let chosen = options.iter().map(|c| prefill.contains(&c.value)).collect();
        Self {
            key,
            label: label.to_owned(),
            help: String::new(),
            required: false,
            kind: FieldKind::MultiSelect { options, chosen },
            error: None,
        }
    }

    pub fn toggle(key: &'static str, label: &str, prefill: bool) -> Self {
        Self {
            key,
            label: label.to_owned(),
            help: String::new(),
            required: false,
            kind: FieldKind::Toggle { value: prefill },
            error: None,
        }
    }

    pub fn string_list(key: &'static str, label: &str, prefill: Vec<String>) -> Self {
        Self {
            key,
            label: label.to_owned(),
            help: String::new(),
            required: false,
            kind: FieldKind::StringList {
                items: prefill,
                validate: None,
            },
            error: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    pub fn placeholder(mut self, text: &str) -> Self {
        if let FieldKind::Text { placeholder, .. } = &mut self.kind {
            *placeholder = text.to_owned();
        }
        self
    }

    pub fn validate(mut self, f: fn(&str) -> Option<String>) -> Self {
        match &mut self.kind {
            FieldKind::Text { validate, .. } | FieldKind::StringList { validate, .. } => {
                *validate = Some(f)
            }
            _ => {}
        }
        self
    }

    /// The one-line rendering of the current value (for the row).
    pub fn value_display(&self) -> String {
        match &self.kind {
            FieldKind::Text { value, .. } => value.clone(),
            FieldKind::Select { options, chosen } => {
                chosen.map(|i| options[i].label.clone()).unwrap_or_default()
            }
            FieldKind::MultiSelect { options, chosen } => options
                .iter()
                .zip(chosen)
                .filter(|(_, on)| **on)
                .map(|(c, _)| c.label.clone())
                .collect::<Vec<_>>()
                .join(", "),
            FieldKind::Toggle { value } => (if *value { "yes" } else { "no" }).to_owned(),
            FieldKind::StringList { items, .. } => items.join(", "),
        }
    }

    fn is_empty(&self) -> bool {
        match &self.kind {
            FieldKind::Text { value, .. } => value.is_empty(),
            FieldKind::Select { chosen, .. } => chosen.is_none(),
            FieldKind::MultiSelect { chosen, .. } => !chosen.iter().any(|c| *c),
            FieldKind::Toggle { .. } => false,
            FieldKind::StringList { items, .. } => items.is_empty(),
        }
    }
}

/// What the user is currently doing.
pub enum Mode {
    Nav,
    /// Editing the focused Text field in place.
    EditText,
    /// A Select/MultiSelect popup is open; `cursor` is its highlighted row.
    Popup {
        cursor: usize,
    },
    /// Appending one new StringList item.
    EditListItem {
        buffer: String,
    },
}

pub enum Outcome {
    Submitted,
    Cancelled,
}

pub struct FormState {
    /// e.g. "weft hook add".
    pub title: String,
    /// e.g. "template dbt-project — patches/base.json".
    pub context: String,
    pub fields: Vec<Field>,
    pub submit_label: String,
    pub focus: usize,
    pub mode: Mode,
    done: Option<Outcome>,
}

impl FormState {
    pub fn new(title: &str, context: &str, fields: Vec<Field>) -> Self {
        Self {
            title: title.to_owned(),
            context: context.to_owned(),
            fields,
            submit_label: "Submit".to_owned(),
            focus: 0,
            mode: Mode::Nav,
            done: None,
        }
    }

    pub fn submit_label(mut self, label: &str) -> Self {
        self.submit_label = label.to_owned();
        self
    }

    /// Rows = fields plus the trailing submit button.
    pub fn row_count(&self) -> usize {
        self.fields.len() + 1
    }

    pub fn on_submit_row(&self) -> bool {
        self.focus == self.fields.len()
    }

    fn focused_field(&mut self) -> Option<&mut Field> {
        let i = self.focus;
        self.fields.get_mut(i)
    }

    // ---- reading values back (after Submitted) --------------------------

    pub fn text_value(&self, key: &str) -> Option<String> {
        self.fields.iter().find(|f| f.key == key).and_then(|f| {
            if let FieldKind::Text { value, .. } = &f.kind {
                (!value.is_empty()).then(|| value.clone())
            } else {
                None
            }
        })
    }

    pub fn select_value(&self, key: &str) -> Option<String> {
        self.fields.iter().find(|f| f.key == key).and_then(|f| {
            if let FieldKind::Select { options, chosen } = &f.kind {
                chosen.map(|i| options[i].value.clone())
            } else {
                None
            }
        })
    }

    pub fn multi_values(&self, key: &str) -> Vec<String> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| {
                if let FieldKind::MultiSelect { options, chosen } = &f.kind {
                    options
                        .iter()
                        .zip(chosen)
                        .filter(|(_, on)| **on)
                        .map(|(c, _)| c.value.clone())
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
    }

    pub fn toggle_value(&self, key: &str) -> bool {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| matches!(f.kind, FieldKind::Toggle { value: true }))
            .unwrap_or(false)
    }

    pub fn list_values(&self, key: &str) -> Vec<String> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| {
                if let FieldKind::StringList { items, .. } = &f.kind {
                    items.clone()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
    }

    // ---- state machine ---------------------------------------------------

    pub fn move_focus(&mut self, delta: isize) {
        let rows = self.row_count() as isize;
        self.focus = (self.focus as isize + delta).rem_euclid(rows) as usize;
    }

    /// Validate everything; set per-field errors. True when submittable.
    pub fn validate_all(&mut self) -> bool {
        let mut first_bad = None;
        for (i, field) in self.fields.iter_mut().enumerate() {
            field.error = None;
            if field.required && field.is_empty() {
                field.error = Some("required".to_owned());
            } else {
                match &field.kind {
                    FieldKind::Text {
                        value, validate, ..
                    } => {
                        if !value.is_empty() {
                            if let Some(v) = validate {
                                field.error = v(value);
                            }
                        }
                    }
                    FieldKind::StringList {
                        items,
                        validate: Some(v),
                    } => {
                        field.error = items.iter().find_map(|item| v(item));
                    }
                    _ => {}
                }
            }
            if field.error.is_some() && first_bad.is_none() {
                first_bad = Some(i);
            }
        }
        if let Some(i) = first_bad {
            self.focus = i;
            false
        } else {
            true
        }
    }

    fn try_submit(&mut self) {
        if self.validate_all() {
            self.done = Some(Outcome::Submitted);
        }
    }

    /// Enter on the focused row: start editing / open the popup / toggle.
    fn activate(&mut self) {
        if self.on_submit_row() {
            self.try_submit();
            return;
        }
        let Some(field) = self.focused_field() else {
            return;
        };
        field.error = None;
        match &mut field.kind {
            FieldKind::Text { .. } => self.mode = Mode::EditText,
            FieldKind::Select { options, chosen } => {
                let cursor = chosen.unwrap_or(0).min(options.len().saturating_sub(1));
                self.mode = Mode::Popup { cursor };
            }
            FieldKind::MultiSelect { .. } => self.mode = Mode::Popup { cursor: 0 },
            FieldKind::Toggle { value } => *value = !*value,
            FieldKind::StringList { .. } => {
                self.mode = Mode::EditListItem {
                    buffer: String::new(),
                }
            }
        }
    }

    /// Feed one key event. Returns the outcome when the form finishes.
    pub fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> Option<&Outcome> {
        match &mut self.mode {
            Mode::Nav => match code {
                KeyCode::Up | KeyCode::BackTab => self.move_focus(-1),
                KeyCode::Down | KeyCode::Tab => self.move_focus(1),
                KeyCode::Char('k') => self.move_focus(-1),
                KeyCode::Char('j') => self.move_focus(1),
                KeyCode::Enter => self.activate(),
                KeyCode::Char(' ') => {
                    if let Some(Field {
                        kind: FieldKind::Toggle { value },
                        ..
                    }) = self.focused_field()
                    {
                        *value = !*value;
                    } else {
                        self.activate();
                    }
                }
                KeyCode::Backspace => {
                    // Convenience: trim the last StringList item in place.
                    if let Some(Field {
                        kind: FieldKind::StringList { items, .. },
                        ..
                    }) = self.focused_field()
                    {
                        items.pop();
                    }
                }
                KeyCode::Char('s') if mods.contains(KeyModifiers::CONTROL) => self.try_submit(),
                KeyCode::Esc | KeyCode::Char('q') => self.done = Some(Outcome::Cancelled),
                KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                    self.done = Some(Outcome::Cancelled)
                }
                _ => {}
            },
            Mode::EditText => {
                let Some(Field {
                    kind: FieldKind::Text { value, cursor, .. },
                    ..
                }) = self.fields.get_mut(self.focus)
                else {
                    self.mode = Mode::Nav;
                    return self.done.as_ref();
                };
                match code {
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Tab => self.mode = Mode::Nav,
                    KeyCode::Left => *cursor = cursor.saturating_sub(1),
                    KeyCode::Right => *cursor = (*cursor + 1).min(value.chars().count()),
                    KeyCode::Home => *cursor = 0,
                    KeyCode::End => *cursor = value.chars().count(),
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            let byte = char_byte(value, *cursor - 1);
                            value.remove(byte);
                            *cursor -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if *cursor < value.chars().count() {
                            let byte = char_byte(value, *cursor);
                            value.remove(byte);
                        }
                    }
                    KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                        self.done = Some(Outcome::Cancelled)
                    }
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        value.clear();
                        *cursor = 0;
                    }
                    KeyCode::Char(c) => {
                        let byte = char_byte(value, *cursor);
                        value.insert(byte, c);
                        *cursor += 1;
                    }
                    _ => {}
                }
            }
            Mode::Popup { cursor } => {
                let Some(field) = self.fields.get_mut(self.focus) else {
                    self.mode = Mode::Nav;
                    return self.done.as_ref();
                };
                let len = match &field.kind {
                    FieldKind::Select { options, .. } => options.len(),
                    FieldKind::MultiSelect { options, .. } => options.len(),
                    _ => 0,
                };
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        *cursor = (*cursor + len.saturating_sub(1)) % len.max(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                        *cursor = (*cursor + 1) % len.max(1)
                    }
                    KeyCode::Char(' ') => {
                        if let FieldKind::MultiSelect { chosen, .. } = &mut field.kind {
                            if let Some(on) = chosen.get_mut(*cursor) {
                                *on = !*on;
                            }
                        } else if let FieldKind::Select { chosen, .. } = &mut field.kind {
                            *chosen = Some(*cursor);
                            self.mode = Mode::Nav;
                        }
                    }
                    KeyCode::Enter => {
                        if let FieldKind::Select { chosen, .. } = &mut field.kind {
                            *chosen = Some(*cursor);
                        }
                        self.mode = Mode::Nav;
                    }
                    KeyCode::Esc => self.mode = Mode::Nav,
                    KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                        self.done = Some(Outcome::Cancelled)
                    }
                    _ => {}
                }
            }
            Mode::EditListItem { buffer } => match code {
                KeyCode::Enter => {
                    let item = buffer.trim().to_owned();
                    if let Some(Field {
                        kind: FieldKind::StringList { items, .. },
                        ..
                    }) = self.fields.get_mut(self.focus)
                    {
                        if !item.is_empty() {
                            items.push(item);
                        }
                    }
                    self.mode = Mode::Nav;
                }
                KeyCode::Esc => self.mode = Mode::Nav,
                KeyCode::Backspace => {
                    buffer.pop();
                }
                KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                    self.done = Some(Outcome::Cancelled)
                }
                KeyCode::Char(c) => buffer.push(c),
                _ => {}
            },
        }
        self.done.as_ref()
    }
}

fn char_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Run the form full-screen. `Ok(state)` after Submit (read values back by
/// key); errors with "cancelled" on Esc/q/^C.
pub fn run(mut state: FormState) -> Result<FormState> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut state);
    ratatui::restore();
    match result {
        Ok(Outcome::Submitted) => Ok(state),
        Ok(Outcome::Cancelled) => bail!("cancelled"),
        Err(e) => Err(e),
    }
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, state: &mut FormState) -> Result<Outcome> {
    loop {
        terminal.draw(|frame| super::widgets::draw_form(frame, state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(outcome) = state.handle_key(key.code, key.modifiers) {
            return Ok(match outcome {
                Outcome::Submitted => Outcome::Submitted,
                Outcome::Cancelled => Outcome::Cancelled,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> FormState {
        FormState::new(
            "weft test",
            "ctx",
            vec![
                Field::text("name", "Name", None).required().validate(|v| {
                    (!v.chars().all(|c| c.is_ascii_lowercase()))
                        .then(|| "lowercase only".to_owned())
                }),
                Field::select(
                    "phase",
                    "Phase",
                    vec![Choice::new("pre"), Choice::new("post")],
                    Some("post"),
                ),
                Field::toggle("force", "Force", false),
                Field::string_list("tags", "Tags", vec!["a".into()]),
            ],
        )
    }

    #[test]
    fn prefill_becomes_default() {
        let f = form();
        assert_eq!(f.select_value("phase").as_deref(), Some("post"));
        assert_eq!(f.list_values("tags"), vec!["a".to_owned()]);
    }

    #[test]
    fn focus_wraps_over_fields_and_submit_row() {
        let mut f = form();
        assert_eq!(f.row_count(), 5);
        f.move_focus(-1);
        assert!(f.on_submit_row());
        f.move_focus(1);
        assert_eq!(f.focus, 0);
    }

    #[test]
    fn text_editing_with_cursor() {
        let mut f = form();
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE); // edit name
        for c in "abc".chars() {
            f.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        f.handle_key(KeyCode::Left, KeyModifiers::NONE);
        f.handle_key(KeyCode::Backspace, KeyModifiers::NONE); // remove 'b'
        f.handle_key(KeyCode::Char('x'), KeyModifiers::NONE); // insert 'x'
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(f.text_value("name").as_deref(), Some("axc"));
    }

    #[test]
    fn required_and_validated_fields_block_submit() {
        let mut f = form();
        // Empty required name → submit focuses it with an error.
        f.focus = f.fields.len();
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(f.done.is_none());
        assert_eq!(f.focus, 0);
        assert_eq!(f.fields[0].error.as_deref(), Some("required"));

        // Invalid value → the field's validator message.
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        f.handle_key(KeyCode::Char('A'), KeyModifiers::NONE);
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        f.handle_key(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(f.fields[0].error.as_deref(), Some("lowercase only"));

        // Fix it → submit succeeds.
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        f.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        f.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        f.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        f.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        let done = f.handle_key(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(matches!(done, Some(Outcome::Submitted)));
        assert_eq!(f.text_value("name").as_deref(), Some("ok"));
    }

    #[test]
    fn select_popup_chooses_by_enter() {
        let mut f = form();
        f.focus = 1;
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE); // open popup at current (post)
        f.handle_key(KeyCode::Up, KeyModifiers::NONE); // move to pre
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(f.select_value("phase").as_deref(), Some("pre"));
        assert!(matches!(f.mode, Mode::Nav));
    }

    #[test]
    fn string_list_add_and_remove() {
        let mut f = form();
        f.focus = 3;
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        for c in "glob:x".chars() {
            f.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        f.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            f.list_values("tags"),
            vec!["a".to_owned(), "glob:x".to_owned()]
        );
        f.handle_key(KeyCode::Backspace, KeyModifiers::NONE); // pop last
        assert_eq!(f.list_values("tags"), vec!["a".to_owned()]);
    }

    #[test]
    fn escape_cancels() {
        let mut f = form();
        let done = f.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches!(done, Some(Outcome::Cancelled)));
    }
}
