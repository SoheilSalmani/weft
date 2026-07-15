//! The one visual grammar for every weft full-screen surface (command
//! forms and the answers wizard): a single accent color, rounded dim
//! borders, and consistent header/footer styling.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding};

pub struct Theme;

impl Theme {
    /// Indigo-400 — the same accent as Weft Cloud.
    pub const ACCENT: Color = Color::Rgb(129, 140, 248);
    pub const TEXT: Color = Color::White;
    pub const DIM: Color = Color::DarkGray;
    pub const ERROR: Color = Color::Red;
    pub const OK: Color = Color::Green;

    pub fn accent() -> Style {
        Style::default().fg(Self::ACCENT)
    }

    pub fn text() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn dim() -> Style {
        Style::default().fg(Self::DIM)
    }

    pub fn error() -> Style {
        Style::default().fg(Self::ERROR)
    }

    pub fn label() -> Style {
        Style::default().fg(Self::DIM).add_modifier(Modifier::BOLD)
    }

    pub fn label_focused() -> Style {
        Style::default().fg(Self::TEXT).add_modifier(Modifier::BOLD)
    }

    /// A bordered panel: rounded, dim border, bold title.
    pub fn panel(title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Self::dim())
            .padding(Padding::horizontal(1))
            .title(Span::styled(
                format!(" {title} "),
                Style::default().fg(Self::TEXT).add_modifier(Modifier::BOLD),
            ))
    }

    /// The popup variant: accent border so it reads as the active layer.
    pub fn popup_panel(title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Self::accent())
            .padding(Padding::horizontal(1))
            .title(Span::styled(
                format!(" {title} "),
                Style::default()
                    .fg(Self::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))
    }

    /// Footer keymap line: `key` in accent, description dim, ` · ` separators.
    pub fn keymap(pairs: &[(&str, &str)]) -> Line<'static> {
        let mut spans = vec![Span::raw(" ")];
        for (i, (key, what)) in pairs.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ·  ", Self::dim()));
            }
            spans.push(Span::styled(
                (*key).to_owned(),
                Style::default()
                    .fg(Self::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!(" {what}"), Self::dim()));
        }
        Line::from(spans)
    }
}
