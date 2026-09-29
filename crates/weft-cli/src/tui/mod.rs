//! Full-screen interactive forms: when a command is missing required
//! options in a terminal, weft opens a form prefilled with whatever was
//! provided instead of erroring. One theme across all surfaces.

pub mod form;
pub mod theme;
pub mod widgets;

use std::io::IsTerminal;

/// Is a person at a terminal on both ends, and not opted out? Forms, the
/// answers wizard and an interactive `weft session shell` read stdin and draw
/// on stdout; inside `$(…)` or a pipe they would wait where nobody sees them.
pub fn interactive(no_tui: bool) -> bool {
    !no_tui && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Print the equivalent full command after a form submit, so the TUI
/// teaches the flags. `parts` are already shell-quoted where needed.
pub fn echo_command(parts: &[String]) {
    eprintln!("→ {}", parts.join(" "));
}

/// Quote one shell argument if it needs it.
pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '='))
    {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
