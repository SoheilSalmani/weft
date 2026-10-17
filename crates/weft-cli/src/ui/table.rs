//! Listings on a terminal: borderless columns under a header, fitted to
//! the terminal's width.
//!
//! A table is only for stdout on a rich terminal. Through a pipe each
//! command prints its listing one record per line, as it always has, so
//! `grep`, `cut` and the tests read the same bytes; [`Table::fits`] is the
//! one check a command makes to choose.

use comfy_table::{presets, Attribute, Cell as Comfy, CellAlignment, Color, ContentArrangement};

use super::Out;

/// One cell's text and how it stands out.
pub(crate) struct Cell {
    text: String,
    look: Look,
    right: bool,
}

#[derive(Clone, Copy)]
enum Look {
    Plain,
    Accent,
    Dim,
    Success,
    Warning,
}

/// A cell of `text`.
pub(crate) fn cell(text: impl Into<String>) -> Cell {
    Cell {
        text: text.into(),
        look: Look::Plain,
        right: false,
    }
}

impl Cell {
    /// In the accent colour: a name the reader will type back.
    pub(crate) fn accent(self) -> Self {
        Self {
            look: Look::Accent,
            ..self
        }
    }
    /// Receding: ids, paths, secondary detail.
    pub(crate) fn dim(self) -> Self {
        Self {
            look: Look::Dim,
            ..self
        }
    }
    /// Good news: active, up to date.
    pub(crate) fn success(self) -> Self {
        Self {
            look: Look::Success,
            ..self
        }
    }
    /// Worth a second look: yanked, inactive.
    pub(crate) fn warning(self) -> Self {
        Self {
            look: Look::Warning,
            ..self
        }
    }
    /// Right-aligned, for numbers.
    pub(crate) fn right(self) -> Self {
        Self {
            right: true,
            ..self
        }
    }

    fn render(self, color: bool) -> Comfy {
        let mut cell = Comfy::new(self.text);
        if self.right {
            cell = cell.set_alignment(CellAlignment::Right);
        }
        if !color {
            return cell;
        }
        match self.look {
            Look::Plain => cell,
            Look::Accent => cell.fg(Color::AnsiValue(141)),
            Look::Dim => cell.add_attribute(Attribute::Dim),
            Look::Success => cell.fg(Color::Green),
            Look::Warning => cell.fg(Color::Yellow),
        }
    }
}

/// A listing with a header row. A column no row has anything in is left
/// out, header and all.
pub(crate) struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<Cell>>,
}

impl Table {
    /// Whether stdout takes a table rather than plain records.
    pub(crate) fn fits() -> bool {
        super::rich(Out::Stdout)
    }

    /// A table under `headers`.
    pub(crate) fn new(headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|h| (*h).to_owned()).collect(),
            rows: Vec::new(),
        }
    }

    /// Add a row.
    pub(crate) fn row(&mut self, cells: impl IntoIterator<Item = Cell>) -> &mut Self {
        self.rows.push(cells.into_iter().collect());
        self
    }

    /// Print the table on stdout.
    pub(crate) fn print(&mut self) {
        let color = super::color(Out::Stdout);
        let kept: Vec<usize> = (0..self.headers.len())
            .filter(|&i| {
                self.rows
                    .iter()
                    .any(|row| row.get(i).is_some_and(|c| !c.text.is_empty()))
            })
            .collect();
        let mut table = comfy_table::Table::new();
        table.load_style(presets::NOTHING);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        if let Some(width) = super::width(Out::Stdout) {
            table.set_width(u16::try_from(width).unwrap_or(u16::MAX));
        }
        table.set_header(kept.iter().map(|&i| {
            let header = Comfy::new(&self.headers[i]);
            if color {
                header
                    .add_attribute(Attribute::Bold)
                    .add_attribute(Attribute::Dim)
            } else {
                header
            }
        }));
        for row in std::mem::take(&mut self.rows) {
            let mut cells: Vec<Option<Cell>> = row.into_iter().map(Some).collect();
            table.add_row(
                kept.iter()
                    .filter_map(|&i| cells.get_mut(i)?.take())
                    .map(|c| c.render(color)),
            );
        }
        let last = kept.len().saturating_sub(1);
        for column in table.column_iter_mut() {
            let right = if column.index == last { 0 } else { 2 };
            column.set_padding((0, right));
        }
        super::out_str(&table.trim_fmt());
        super::out_str("\n");
    }
}
