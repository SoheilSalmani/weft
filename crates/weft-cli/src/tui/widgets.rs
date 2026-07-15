//! Rendering for the form engine: header, field rows with an accent focus
//! bar, inline errors, a help pane, a footer keymap, and centered popups
//! for Select/MultiSelect.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::form::{Field, FieldKind, FormState, Mode};
use super::theme::Theme;

/// Label column width (labels are right-aligned into it).
const LABEL_WIDTH: usize = 16;

pub fn draw_form(frame: &mut Frame, state: &FormState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header
            Constraint::Min(4),    // fields
            Constraint::Length(4), // help / error
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], state);
    draw_rows(frame, chunks[1], state);
    draw_help(frame, chunks[2], state);
    draw_footer(frame, chunks[3], state);

    if let Mode::Popup { cursor } = &state.mode {
        if let Some(field) = state.fields.get(state.focus) {
            draw_popup(frame, field, *cursor);
        }
    }
}

fn draw_header(frame: &mut Frame, area: Rect, state: &FormState) {
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            state.title.clone(),
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(state.context.clone(), Theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(title), area);
}

fn row_line(field: &Field, focused: bool, mode: &Mode) -> Line<'static> {
    let bar = if focused {
        Span::styled("▎ ", Theme::accent())
    } else {
        Span::raw("  ")
    };
    let label_style = if focused {
        Theme::label_focused()
    } else {
        Theme::label()
    };
    let mut label = format!("{:>LABEL_WIDTH$}", field.label);
    if field.required {
        label.push('*');
    } else {
        label.push(' ');
    }

    let value_span = match (focused, mode) {
        (true, Mode::EditText) => {
            if let FieldKind::Text { value, cursor, .. } = &field.kind {
                let chars: Vec<char> = value.chars().collect();
                let before: String = chars[..*cursor].iter().collect();
                let after: String = chars[*cursor..].iter().collect();
                return Line::from(vec![
                    bar,
                    Span::styled(label, label_style),
                    Span::raw("  "),
                    Span::styled(before, Theme::text()),
                    Span::styled("▏", Theme::accent()),
                    Span::styled(after, Theme::text()),
                ]);
            }
            Span::raw(String::new())
        }
        (true, Mode::EditListItem { buffer }) => {
            if let FieldKind::StringList { items, .. } = &field.kind {
                let mut existing = items.join(", ");
                if !existing.is_empty() {
                    existing.push_str(", ");
                }
                return Line::from(vec![
                    bar,
                    Span::styled(label, label_style),
                    Span::raw("  "),
                    Span::styled(existing, Theme::text()),
                    Span::styled(buffer.clone(), Theme::text()),
                    Span::styled("▏", Theme::accent()),
                ]);
            }
            Span::raw(String::new())
        }
        _ => {
            let display = field.value_display();
            if display.is_empty() {
                let hint = match &field.kind {
                    FieldKind::Text { placeholder, .. } if !placeholder.is_empty() => {
                        placeholder.clone()
                    }
                    FieldKind::Select { .. } => "choose…".to_owned(),
                    FieldKind::MultiSelect { .. } => "choose…".to_owned(),
                    FieldKind::StringList { .. } => "add…".to_owned(),
                    _ => "—".to_owned(),
                };
                Span::styled(hint, Theme::dim().add_modifier(Modifier::ITALIC))
            } else if let FieldKind::Toggle { value } = &field.kind {
                if *value {
                    Span::styled(" yes ", Style::default().fg(Theme::OK))
                } else {
                    Span::styled(" no ", Theme::dim())
                }
            } else {
                Span::styled(display, Theme::text())
            }
        }
    };

    let mut spans = vec![
        bar,
        Span::styled(label, label_style),
        Span::raw("  "),
        value_span,
    ];
    if let Some(error) = &field.error {
        spans.push(Span::styled(format!("  ✗ {error}"), Theme::error()));
    }
    Line::from(spans)
}

fn draw_rows(frame: &mut Frame, area: Rect, state: &FormState) {
    let mut items: Vec<ListItem> = state
        .fields
        .iter()
        .enumerate()
        .map(|(i, field)| ListItem::new(row_line(field, i == state.focus, &state.mode)))
        .collect();

    // The submit "button" row.
    let on_submit = state.on_submit_row();
    let submit_style = if on_submit {
        Style::default()
            .fg(Theme::ACCENT)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Theme::dim().add_modifier(Modifier::BOLD)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::raw(if on_submit { "▎ " } else { "  " }),
        Span::raw(" ".repeat(LABEL_WIDTH + 3)),
        Span::styled(format!("  {}  ", state.submit_label), submit_style),
    ])));

    // Simple viewport scroll keeping the focus visible.
    let visible = area.height.saturating_sub(2) as usize; // panel borders
    let skip = state.focus.saturating_sub(visible.saturating_sub(1));
    let total = items.len();
    let items: Vec<ListItem> = items.into_iter().skip(skip).collect();

    let mut title = state.title.clone();
    if skip > 0 || total > visible {
        title.push_str(&format!("  ({}/{})", state.focus + 1, total));
    }
    frame.render_widget(List::new(items).block(Theme::panel(&title)), area);
}

fn draw_help(frame: &mut Frame, area: Rect, state: &FormState) {
    let text = state
        .fields
        .get(state.focus)
        .map(|f| {
            if let Some(error) = &f.error {
                return Line::from(Span::styled(format!("✗ {error}"), Theme::error()));
            }
            Line::from(Span::styled(f.help.clone(), Theme::dim()))
        })
        .unwrap_or_else(|| {
            Line::from(Span::styled(
                "review the values, then confirm".to_owned(),
                Theme::dim(),
            ))
        });
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(Theme::panel("about")),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &FormState) {
    let keys: Vec<(&str, &str)> = match &state.mode {
        Mode::Nav => {
            if state.on_submit_row() {
                vec![("enter", "confirm"), ("↑↓", "move"), ("esc", "cancel")]
            } else {
                vec![
                    ("enter", "edit"),
                    ("↑↓", "move"),
                    ("^s", "submit"),
                    ("esc", "cancel"),
                ]
            }
        }
        Mode::EditText => vec![
            ("enter", "done"),
            ("←→", "cursor"),
            ("^u", "clear"),
            ("esc", "done"),
        ],
        Mode::Popup { .. } => vec![
            ("↑↓", "move"),
            ("space", "toggle"),
            ("enter", "choose"),
            ("esc", "close"),
        ],
        Mode::EditListItem { .. } => {
            vec![("enter", "add"), ("esc", "cancel"), ("⌫", "remove last")]
        }
    };
    frame.render_widget(Paragraph::new(Theme::keymap(&keys)), area);
}

fn draw_popup(frame: &mut Frame, field: &Field, cursor: usize) {
    let (options, chosen_multi, chosen_single) = match &field.kind {
        FieldKind::Select { options, chosen } => (options, None, *chosen),
        FieldKind::MultiSelect { options, chosen } => (options, Some(chosen), None),
        _ => return,
    };

    // Never let the min exceed the available max (tiny terminals).
    let max_width = (frame.area().width.saturating_sub(6) as usize).max(1);
    let width = options
        .iter()
        .map(|c| c.label.len() + c.help.len() + 10)
        .max()
        .unwrap_or(20)
        .max(28.min(max_width))
        .min(max_width) as u16;
    let height = (options.len() as u16 + 2).min(frame.area().height.saturating_sub(4).max(3));
    let area = centered(frame.area(), width, height);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let marker = if let Some(chosen) = chosen_multi {
                if chosen.get(i).copied().unwrap_or(false) {
                    Span::styled("◉ ", Theme::accent())
                } else {
                    Span::styled("○ ", Theme::dim())
                }
            } else if chosen_single == Some(i) {
                Span::styled("● ", Theme::accent())
            } else {
                Span::raw("  ")
            };
            let label_style = if i == cursor {
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Theme::text()
            };
            let mut spans = vec![
                Span::raw(if i == cursor { "❯ " } else { "  " }),
                marker,
                Span::styled(choice.label.clone(), label_style),
            ];
            if !choice.help.is_empty() {
                spans.push(Span::styled(format!("  {}", choice.help), Theme::dim()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(Theme::popup_panel(&field.label)),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::form::{Choice, Field, FormState};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_labels_focus_bar_and_footer() {
        let state = FormState::new(
            "weft hook add",
            "template demo",
            vec![
                Field::text("id", "Hook id", Some("verify-go".into())).required(),
                Field::select(
                    "phase",
                    "Phase",
                    vec![Choice::new("pre"), Choice::new("post")],
                    None,
                ),
            ],
        );
        let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
        terminal.draw(|f| draw_form(f, &state)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("weft hook add"), "{text}");
        assert!(text.contains("Hook id*"), "required star: {text}");
        assert!(text.contains("verify-go"), "{text}");
        assert!(text.contains("▎"), "focus bar: {text}");
        assert!(text.contains("choose…"), "empty select hint: {text}");
        assert!(text.contains("submit"), "footer: {text}");
    }

    #[test]
    fn form_survives_a_zero_size_terminal() {
        let mut state = FormState::new(
            "weft hook add",
            "",
            vec![Field::select(
                "phase",
                "Phase",
                vec![Choice::new("pre")],
                None,
            )],
        );
        let mut terminal = Terminal::new(TestBackend::new(0, 0)).unwrap();
        terminal.draw(|f| draw_form(f, &state)).unwrap();
        state.handle_key(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        terminal.draw(|f| draw_form(f, &state)).unwrap();
    }

    #[test]
    fn popup_survives_a_tiny_terminal() {
        let mut state = FormState::new(
            "weft hook add",
            "",
            vec![Field::select(
                "phase",
                "Phase",
                vec![Choice::new("pre").help("a rather long help text for this option")],
                None,
            )],
        );
        state.handle_key(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        // 20 columns: narrower than the popup's preferred minimum.
        let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
        terminal.draw(|f| draw_form(f, &state)).unwrap();
    }

    #[test]
    fn renders_popup_overlay_and_error() {
        let mut state = FormState::new(
            "weft hook add",
            "",
            vec![Field::select(
                "phase",
                "Phase",
                vec![
                    Choice::new("pre").help("guard before writing"),
                    Choice::new("post"),
                ],
                None,
            )
            .required()],
        );
        state.validate_all();
        state.handle_key(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        let mut terminal = Terminal::new(TestBackend::new(72, 16)).unwrap();
        terminal.draw(|f| draw_form(f, &state)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("guard before writing"), "popup help: {text}");
        assert!(text.contains("❯"), "popup cursor: {text}");
    }
}
