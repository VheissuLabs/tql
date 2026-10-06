use zerorust_tui::prelude::*;
use zerorust_tui::ratatui::buffer::Buffer;

use super::choice::Choice;
use super::chrome::{Island, Modal, theme};
use super::text::{pad, truncate};
use crate::database::{Filter, Filters, Joiner, operator_names};

pub const WIDTH: u16 = 72;
const COLUMN_WIDTH: usize = 18;
const OPERATOR_WIDTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cell {
    Column,
    Operator,
    Value,
}

impl Cell {
    fn stepped(self, forward: bool, last: Cell) -> Cell {
        let next = match (self, forward) {
            (Cell::Column, true) | (Cell::Value, false) => Cell::Operator,
            (Cell::Operator, true) | (Cell::Value, true) => Cell::Value,
            (Cell::Operator, false) | (Cell::Column, false) => Cell::Column,
        };
        next.min(last)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEditor {
    characters: Vec<char>,
    cursor: usize,
}

impl LineEditor {
    pub fn holding(text: &str) -> Self {
        let characters: Vec<char> = text.chars().collect();
        LineEditor {
            cursor: characters.len(),
            characters,
        }
    }

    pub fn text(&self) -> String {
        self.characters.iter().collect()
    }

    pub fn handle(&mut self, key: Key) {
        match key {
            Key::Backspace | Key::Ctrl('h') if self.cursor > 0 => {
                self.cursor -= 1;
                self.characters.remove(self.cursor);
            }
            Key::Delete if self.cursor < self.characters.len() => {
                self.characters.remove(self.cursor);
            }
            Key::Left | Key::Ctrl('b') => self.cursor = self.cursor.saturating_sub(1),
            Key::Right | Key::Ctrl('f') => {
                self.cursor = (self.cursor + 1).min(self.characters.len())
            }
            Key::Home | Key::Ctrl('a') => self.cursor = 0,
            Key::End | Key::Ctrl('e') => self.cursor = self.characters.len(),
            Key::Char(character) => {
                self.characters.insert(self.cursor, character);
                self.cursor += 1;
            }
            _ => {}
        }
    }

    fn windowed(&self, room: usize) -> Vec<Span<'static>> {
        let start = (self.cursor + 1).saturating_sub(room);
        let shown: Vec<char> = self
            .characters
            .iter()
            .skip(start)
            .take(room)
            .copied()
            .collect();
        let at = self.cursor - start;
        let before: String = shown.iter().take(at).collect();
        let under = shown.get(at).map_or(" ".to_string(), char::to_string);
        let after: String = shown.iter().skip(at + 1).collect();
        let used = shown.len().max(at + 1);
        vec![
            Span::styled(before, theme::dim()),
            Span::styled(under, theme::cursor()),
            Span::styled(
                format!("{after}{}", " ".repeat(room.saturating_sub(used))),
                theme::dim(),
            ),
        ]
    }
}

pub struct FilterForm {
    pub columns: Vec<String>,
    pub conditions: Vec<Filter>,
    pub joiner: Joiner,
    pub row: usize,
    pub cell: Cell,
    pub editor: Option<LineEditor>,
    pub picker: Option<Choice>,
    start_on: Option<String>,
}

impl FilterForm {
    pub fn new(columns: Vec<String>, existing: Option<Filters>, start_on: Option<String>) -> Self {
        let existing = existing.unwrap_or_default();
        let mut form = FilterForm {
            columns,
            conditions: existing.filters,
            joiner: existing.joiner,
            row: 0,
            cell: Cell::Column,
            editor: None,
            picker: None,
            start_on,
        };
        if form.conditions.is_empty() {
            form.add();
            form.cell = Cell::Value;
            form.start_editing();
        }
        form
    }

    pub fn add(&mut self) {
        let column = self
            .start_on
            .clone()
            .filter(|column| self.columns.contains(column))
            .or_else(|| self.columns.first().cloned())
            .unwrap_or_default();
        self.conditions.push(Filter::new(&column, "contains", ""));
        self.row = self.conditions.len() - 1;
        self.cell = Cell::Column;
    }

    pub fn remove(&mut self) {
        if self.conditions.len() <= 1 {
            self.conditions.clear();
            self.add();
            return;
        }
        self.conditions.remove(self.row);
        self.row = self.row.min(self.conditions.len() - 1);
    }

    pub fn current(&self) -> &Filter {
        &self.conditions[self.row]
    }

    fn current_mut(&mut self) -> &mut Filter {
        &mut self.conditions[self.row]
    }

    fn last_cell(&self) -> Cell {
        if self.current().needs_value() {
            Cell::Value
        } else {
            Cell::Operator
        }
    }

    pub fn move_row(&mut self, forward: bool) {
        self.row = if forward {
            (self.row + 1).min(self.conditions.len() - 1)
        } else {
            self.row.saturating_sub(1)
        };
        self.cell = self.cell.min(self.last_cell());
    }

    pub fn move_cell(&mut self, forward: bool) {
        self.cell = self.cell.stepped(forward, self.last_cell());
    }

    pub fn cycle(&mut self, forward: bool) {
        match self.cell {
            Cell::Column => {
                let column = next(&self.columns, &self.current().column, forward);
                self.current_mut().column = column;
            }
            Cell::Operator => {
                let operator = next(&operator_names(), &self.current().operator, forward);
                self.current_mut().operator = operator;
                self.cell = self.cell.min(self.last_cell());
            }
            Cell::Value => {}
        }
    }

    pub fn toggle_joiner(&mut self) {
        self.joiner = self.joiner.toggled();
    }

    pub fn open_picker(&mut self) {
        self.picker = match self.cell {
            Cell::Column => Some(Choice::new(
                "COLUMN",
                self.columns.clone(),
                &self.current().column,
                false,
            )),
            Cell::Operator => Some(Choice::new(
                "OPERATOR",
                operator_names(),
                &self.current().operator,
                false,
            )),
            Cell::Value => None,
        };
    }

    pub fn choose(&mut self) {
        let Some(chosen) = self.picker.take().and_then(|picker| picker.selected()) else {
            return;
        };
        if self.cell == Cell::Column {
            self.current_mut().column = chosen;
            return;
        }
        self.current_mut().operator = chosen;
        self.cell = self.cell.min(self.last_cell());
    }

    pub fn start_editing(&mut self) {
        if self.cell == Cell::Value {
            self.editor = Some(LineEditor::holding(&self.current().value));
        }
    }

    pub fn commit(&mut self) {
        if let Some(editor) = self.editor.take() {
            self.current_mut().value = editor.text();
        }
    }

    pub fn abandon(&mut self) {
        self.editor = None;
    }

    pub fn to_filters(&self) -> Filters {
        Filters::new(self.conditions.clone(), self.joiner)
    }

    pub fn draw_modal(&self, buffer: &mut Buffer, frame_width: u16, frame_height: u16) {
        let width = frame_width.saturating_sub(4).min(WIDTH);
        let island = Island {
            title: "FILTER".to_string(),
            content: self.content(width.saturating_sub(2) as usize),
            ..Island::default()
        };
        Modal::new(frame_width, 1, frame_height, width).draw(buffer, island, self.rows());
        if let Some(picker) = &self.picker {
            picker.draw_modal(buffer, frame_width, frame_height);
        }
    }

    pub fn rows(&self) -> u16 {
        self.conditions.len() as u16 + 5
    }

    pub fn content(&self, inner_width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::raw("")];
        for index in 0..self.conditions.len() {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(self.condition(index, inner_width.saturating_sub(4)));
            lines.push(Line::from(spans));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(self.hint(), theme::dim()),
        ]));
        lines
    }

    fn condition(&self, index: usize, width: usize) -> Vec<Span<'static>> {
        let filter = &self.conditions[index];
        let here = index == self.row;
        let joiner = if index == 0 {
            "where"
        } else {
            self.joiner.keyword()
        };
        let editing = here && self.cell == Cell::Value && self.editor.is_some();
        let room = width.saturating_sub(44).max(4);

        let mut spans = vec![
            Span::styled(pad(joiner, 6), theme::dim()),
            cell(
                &filter.column,
                here && self.cell == Cell::Column,
                COLUMN_WIDTH,
            ),
            Span::raw(" "),
            cell(
                &filter.operator,
                here && self.cell == Cell::Operator,
                OPERATOR_WIDTH,
            ),
            Span::raw(" "),
        ];
        match (&self.editor, editing) {
            (Some(editor), true) => spans.extend(editor.windowed(room)),
            _ => {
                let value = match (filter.needs_value(), filter.value.is_empty()) {
                    (false, _) => String::new(),
                    (true, true) => "…".to_string(),
                    (true, false) => filter.value.clone(),
                };
                spans.push(cell(&value, here && self.cell == Cell::Value, room));
            }
        }
        spans
    }

    fn hint(&self) -> &'static str {
        if self.editor.is_some() {
            return "↵ keeps it    ↵↵ applies    esc goes back to the form";
        }
        match self.cell {
            Cell::Value => "↵ applies    type to edit    ⇥ ⇧⇥ moves    + adds    - removes",
            _ => "← → changes it    ⇥ ⇧⇥ moves    ↵ opens a list    ctrl+s applies",
        }
    }
}

fn cell(text: &str, focused: bool, width: usize) -> Span<'static> {
    let shown = pad(&truncate(text, width), width);
    if focused {
        Span::styled(shown, theme::selection())
    } else {
        Span::styled(shown, theme::dim())
    }
}

fn next(options: &[String], current: &str, forward: bool) -> String {
    if options.is_empty() {
        return current.to_string();
    }
    let at = options
        .iter()
        .position(|option| option == current)
        .unwrap_or(0);
    let count = options.len();
    let step = if forward { 1 } else { count - 1 };
    options[(at + step) % count].clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> FilterForm {
        let mut form = FilterForm::new(
            ["id", "name", "age", "city"].map(String::from).to_vec(),
            None,
            Some("name".to_string()),
        );
        form.abandon();
        form
    }

    #[test]
    fn opens_on_the_column_you_were_on_ready_to_type_a_value() {
        let form = FilterForm::new(
            ["id", "name"].map(String::from).to_vec(),
            None,
            Some("name".to_string()),
        );
        assert_eq!(form.current(), &Filter::new("name", "contains", ""));
        assert_eq!(form.cell, Cell::Value);
        assert!(form.editor.is_some());
    }

    #[test]
    fn falls_back_to_the_first_column_when_the_one_you_were_on_is_gone() {
        let form = FilterForm::new(
            ["id", "name"].map(String::from).to_vec(),
            None,
            Some("email".to_string()),
        );
        assert_eq!(form.current().column, "id");
    }

    #[test]
    fn reopens_with_the_conditions_you_had_on_the_column_cell() {
        let form = FilterForm::new(
            ["id", "name"].map(String::from).to_vec(),
            Some(Filters::new(
                vec![Filter::new("name", "is", "Karl")],
                Joiner::Or,
            )),
            None,
        );
        assert_eq!(form.conditions, [Filter::new("name", "is", "Karl")]);
        assert_eq!(form.joiner, Joiner::Or);
        assert_eq!(form.cell, Cell::Column);
        assert!(form.editor.is_none());
    }

    #[test]
    fn cycles_the_column_and_the_operator_and_wraps() {
        let mut form = form();
        form.move_cell(false);
        form.move_cell(false);
        assert_eq!(form.cell, Cell::Column);

        form.cycle(true);
        assert_eq!(form.current().column, "age");
        form.cycle(true);
        form.cycle(true);
        assert_eq!(form.current().column, "id");
        form.cycle(false);
        assert_eq!(form.current().column, "city");

        form.move_cell(true);
        form.cycle(true);
        assert_eq!(form.current().operator, "starts with");
    }

    #[test]
    fn has_no_value_cell_for_an_operator_that_takes_none() {
        let mut form = form();
        form.move_cell(false);
        form.open_picker();
        for character in "is empty".chars() {
            form.picker.as_mut().unwrap().type_key(Key::Char(character));
        }
        form.choose();
        assert_eq!(form.current().operator, "is empty");

        form.move_cell(true);
        assert_eq!(form.cell, Cell::Operator);
    }

    #[test]
    fn adds_and_removes_but_keeps_one_condition_to_edit() {
        let mut form = form();
        form.add();
        assert_eq!(form.conditions.len(), 2);
        assert_eq!(form.row, 1);

        form.remove();
        form.remove();
        form.remove();
        assert_eq!(form.conditions.len(), 1);
        assert_eq!(form.row, 0);
    }

    #[test]
    fn leaves_the_cell_alone_when_the_list_is_closed_or_matches_nothing() {
        let mut form = form();
        form.move_cell(false);
        form.move_cell(false);
        form.open_picker();
        for character in "zzz".chars() {
            form.picker.as_mut().unwrap().type_key(Key::Char(character));
        }
        form.choose();
        assert!(form.picker.is_none());
        assert_eq!(form.current().column, "name");
    }

    #[test]
    fn edits_anywhere_in_the_line() {
        let mut editor = LineEditor::holding("Karl");
        for key in [
            Key::Home,
            Key::Delete,
            Key::Char('C'),
            Key::End,
            Key::Backspace,
        ] {
            editor.handle(key);
        }
        assert_eq!(editor.text(), "Car");
    }

    #[test]
    fn keeps_the_cursor_in_view_while_typing_a_long_value() {
        let editor = LineEditor::holding("long value long value");
        let shown: String = editor
            .windowed(8)
            .iter()
            .map(|span| span.content.to_string())
            .collect();
        assert_eq!(shown, "g value ");
    }
}
