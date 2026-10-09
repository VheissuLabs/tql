use crate::terminal::Key;
use crate::terminal::prelude::*;

use super::chrome::theme;
use super::editor::QueryEditor;
use super::json;
use super::now;
use super::text::{pad, truncate, window};
use crate::database::{Column, ColumnDefault, Named, Values};
use crate::settings;

pub const WIDTH: usize = 76;
const FOOTER: usize = 3;
const AUTO: &str = "auto";
const REQUIRED: &str = "required";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
    pub hint: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Untouched,
    Null,
    Value,
}

#[derive(Debug, Clone)]
pub struct RecordForm {
    pub table: String,
    pub fields: Vec<Field>,
    values: Values,
    initial: Values,
    pub key: Option<String>,
    pub key_value: Option<String>,
    pub insert_at: Option<usize>,
    pub index: usize,
    pub editor: Option<QueryEditor>,
    pub expanded: bool,
    pub json: bool,
    pub error: Option<String>,
    pub discarding: bool,
    pub width: usize,
}

impl RecordForm {
    fn new(table: &str, fields: Vec<Field>, values: Values) -> Self {
        RecordForm {
            table: table.to_string(),
            fields,
            initial: values.clone(),
            values,
            key: None,
            key_value: None,
            insert_at: None,
            index: 0,
            editor: None,
            expanded: false,
            json: false,
            error: None,
            discarding: false,
            width: WIDTH,
        }
    }

    pub fn adding(
        table: &str,
        columns: &[Column],
        driver: &str,
        generated: &[String],
        given: &Values,
    ) -> Self {
        let mut fields = Vec::new();
        let mut values = Values::new();
        for column in columns {
            let default = ColumnDefault::read(column.default.as_deref(), driver);
            let auto = generated.contains(&column.name);
            let required = !auto && default == ColumnDefault::None && !column.nullable;
            let hint = if required {
                REQUIRED.to_string()
            } else {
                hint(auto, &default)
            };
            fields.push(field(column, hint, false));

            if let Some(value) = given.named(&column.name) {
                values.put(&column.name, value.clone());
            } else if !auto && let Some(filled) = default.filled() {
                values.put(&column.name, Some(filled));
            }
        }

        let mut form = RecordForm::new(table, fields, values);
        form.index = form
            .fields
            .iter()
            .position(|field| field.hint != AUTO && given.named(&field.name).is_none())
            .unwrap_or(0);
        form
    }

    pub fn pending(
        table: &str,
        columns: &[Column],
        driver: &str,
        generated: &[String],
        values: &Values,
        at: usize,
    ) -> Self {
        let fields = columns
            .iter()
            .map(|column| {
                let default = ColumnDefault::read(column.default.as_deref(), driver);
                field(
                    column,
                    hint(generated.contains(&column.name), &default),
                    false,
                )
            })
            .collect();
        let mut form = RecordForm::new(table, fields, values.clone());
        form.insert_at = Some(at);
        form
    }

    pub fn editing(table: &str, columns: &[Column], row: &Values, key: &str) -> Self {
        let fields = columns
            .iter()
            .map(|column| field(column, String::new(), column.name == key))
            .collect();
        let values = columns
            .iter()
            .map(|column| {
                (
                    column.name.clone(),
                    row.named(&column.name).cloned().flatten(),
                )
            })
            .collect();
        let mut form = RecordForm::new(table, fields, values);
        form.key = Some(key.to_string());
        form.key_value = row.named(key).cloned().flatten();
        form
    }

    pub fn adds(&self) -> bool {
        self.key_value.is_none()
    }

    pub fn current(&self) -> &Field {
        &self.fields[self.index]
    }

    pub fn state(&self, name: &str) -> State {
        match self.values.named(name) {
            None => State::Untouched,
            Some(None) => State::Null,
            Some(Some(_)) => State::Value,
        }
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.named(name).and_then(|value| value.as_deref())
    }

    pub fn changed(&self, name: &str) -> bool {
        self.values.named(name) != self.initial.named(name)
    }

    pub fn dirty(&self) -> bool {
        self.fields.iter().any(|field| self.changed(&field.name))
    }

    pub fn move_by(&mut self, step: isize) {
        let last = self.fields.len().saturating_sub(1) as isize;
        self.index = (self.index as isize + step).clamp(0, last) as usize;
        self.error = None;
    }

    pub fn jump(&mut self, index: usize) {
        self.index = index.min(self.fields.len().saturating_sub(1));
        self.error = None;
    }

    pub fn room(&self, field: &Field) -> usize {
        self.width
            .saturating_sub(4 + label(field).chars().count())
            .max(8)
    }

    pub fn start(&mut self) {
        let field = self.current().clone();
        if field.read_only {
            self.error = Some(format!(
                "{} names the row, so it is not edited here",
                field.name
            ));
            return;
        }

        let text = self.value(&field.name).unwrap_or_default().to_string();
        self.json = json::looks_like_json(Some(&text));
        self.expanded =
            self.json || text.contains('\n') || text.chars().count() >= self.room(&field);
        let mut editor = QueryEditor::holding(&if self.json { json::pretty(&text) } else { text });
        if self.json {
            editor.move_to_start();
        }
        self.editor = Some(editor);
        self.error = None;
    }

    pub fn type_key(&mut self, key: Key) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.handle(key);
        if editor.buffer().contains('\n') {
            self.expanded = true;
        }
    }

    pub fn paste(&mut self, text: &str) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.paste(text);
        if editor.buffer().contains('\n') {
            self.expanded = true;
        }
    }

    pub fn now(&mut self) {
        let typed = now::for_type(Some(&self.current().type_name));
        if let Some(editor) = self.editor.as_mut() {
            editor.set(&typed);
        }
    }

    pub fn keep(&mut self) -> bool {
        let Some(editor) = &self.editor else {
            return true;
        };
        let typed = editor.buffer();
        if self.json && !json::looks_like_json(Some(&typed)) {
            self.error = Some("not valid json: fix it, or esc to put it back".to_string());
            return false;
        }
        let kept = if self.json {
            json::compact(&typed)
        } else {
            typed
        };
        let name = self.current().name.clone();
        self.values.put(&name, Some(kept));
        self.abandon();
        true
    }

    pub fn abandon(&mut self) {
        self.editor = None;
        self.expanded = false;
        self.json = false;
        self.error = None;
    }

    pub fn set_null(&mut self) {
        let field = self.current().clone();
        if field.read_only {
            self.error = Some(format!(
                "{} names the row, so it is not edited here",
                field.name
            ));
            return;
        }
        if !field.nullable {
            self.error = Some(format!("{} is not null", field.name));
            return;
        }
        self.values.put(&field.name, None);
        self.error = None;
    }

    pub fn reset(&mut self) {
        let name = self.current().name.clone();
        match self.initial.named(&name).cloned() {
            Some(initial) if !self.adds() => self.values.put(&name, initial),
            _ => self.values.forget(&name),
        }
        self.error = None;
    }

    pub fn values(&self) -> Values {
        self.values
            .iter()
            .filter(|(name, _)| self.adds() || self.changed(name))
            .map(|(name, value)| {
                let type_name = self
                    .fields
                    .iter()
                    .find(|field| &field.name == name)
                    .map(|field| field.type_name.as_str());
                let value = match value {
                    Some(text) if now::asked(text) => Some(now::for_type(type_name)),
                    other => other.clone(),
                };
                (name.clone(), value)
            })
            .collect()
    }

    pub fn title(&self) -> String {
        match (&self.key, &self.key_value) {
            (Some(key), Some(key_value)) if !self.adds() => {
                format!("EDIT ROW  ·  {}  ·  {key} {key_value}", self.table)
            }
            _ => format!("NEW ROW  ·  {}", self.table),
        }
    }

    pub fn natural_width(&self) -> usize {
        let longest = self
            .fields
            .iter()
            .map(|field| {
                let value = self.value(&field.name).unwrap_or("NULL");
                let shown = if value.contains('\n') {
                    0
                } else {
                    value.chars().count()
                };
                label(field).chars().count() + shown.max(field.hint.chars().count()) + 1
            })
            .max()
            .unwrap_or(0);
        WIDTH.max(longest + 8)
    }

    pub fn rows(&self) -> usize {
        self.fields.len() * 2 + FOOTER + 2
    }

    pub fn content(&mut self, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
        self.width = inner_width.saturating_sub(2);

        let mut body = vec![Line::default()];
        for index in 0..self.fields.len() {
            body.push(self.field_line(index));
            body.push(Line::default());
        }

        let room = inner_height.saturating_sub(FOOTER).max(1);
        let start = window(1 + self.index * 2, body.len(), room);
        let shown = body.len().saturating_sub(start).min(room);
        let mut lines: Vec<Line<'static>> = body.into_iter().skip(start).take(room).collect();
        lines.extend((shown..room).map(|_| Line::default()));
        lines.extend(self.footer());
        lines
    }

    fn field_line(&self, index: usize) -> Line<'static> {
        let field = &self.fields[index];
        let here = index == self.index;
        let room = self.room(field);
        let mut spans = vec![
            Span::raw("  "),
            Span::styled(field.name.clone(), theme::bold()),
            Span::styled(format!(" ({}):", field.type_name), theme::dim()),
            Span::raw(" "),
        ];

        if here
            && !self.expanded
            && let Some(editor) = &self.editor
        {
            spans.extend(typing(editor, room));
            return Line::from(spans);
        }

        let state = self.state(&field.name);
        let shown = match state {
            State::Untouched => truncate(&field.hint, room),
            State::Null => "NULL".to_string(),
            State::Value => clamp(self.value(&field.name).unwrap_or_default(), room),
        };

        spans.push(match (here, state) {
            (true, _) => Span::styled(pad(&shown, room), theme::cursor()),
            (false, State::Untouched) => Span::styled(shown, theme::dim()),
            (false, _) if self.changed(&field.name) => Span::styled(shown, changed()),
            (false, _) => Span::raw(shown),
        });
        Line::from(spans)
    }

    fn footer(&self) -> Vec<Line<'static>> {
        let hint = match (&self.editor, self.discarding) {
            (Some(_), _) => "↵ keep  tab next  ctrl+t now  esc put it back",
            (None, true) => "esc again throws the row away  any other key keeps it open",
            (None, false) => "↑↓ move  ↵ edit  ctrl+n null  ⌫ reset  ctrl+s keep  esc cancel",
        };
        let room = self.width.saturating_sub(2);
        let error = self
            .error
            .as_ref()
            .map(|error| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(truncate(error, room), problem()),
                ])
            })
            .unwrap_or_default();
        vec![
            Line::default(),
            error,
            Line::from(vec![
                Span::raw("  "),
                Span::styled(truncate(hint, room), theme::dim()),
            ]),
        ]
    }
}

fn problem() -> Style {
    theme::paint(&settings::with(|settings| settings.color("deleted", "red")))
}

fn changed() -> Style {
    theme::paint(&settings::with(|settings| {
        settings.color("edited", "yellow")
    }))
}

fn field(column: &Column, hint: String, read_only: bool) -> Field {
    Field {
        name: column.name.clone(),
        type_name: column.type_name.clone(),
        nullable: column.nullable,
        hint,
        read_only,
    }
}

fn hint(auto: bool, default: &ColumnDefault) -> String {
    match default {
        _ if auto => AUTO.to_string(),
        ColumnDefault::Expression(expression) => format!("default {expression}"),
        _ => String::new(),
    }
}

fn label(field: &Field) -> String {
    format!("{} ({}): ", field.name, field.type_name)
}

fn clamp(value: &str, room: usize) -> String {
    let line = value.replace('\n', " ");
    if line.chars().count() <= room && !value.contains('\n') {
        return line;
    }
    let kept: String = line.chars().take(room.saturating_sub(3).max(1)).collect();
    format!("{kept}… ↗")
}

fn typing(editor: &QueryEditor, room: usize) -> Vec<Span<'static>> {
    let text: Vec<char> = editor.buffer().chars().collect();
    let cursor = editor.cursor();
    let start = (cursor + 1).saturating_sub(room);
    let visible: Vec<char> = text.iter().skip(start).take(room).copied().collect();
    let at = cursor - start;
    let before: String = visible.iter().take(at).collect();
    let under = visible.get(at).map(char::to_string).unwrap_or(" ".into());
    let after: String = visible.iter().skip(at + 1).collect();
    vec![
        Span::raw(before),
        Span::styled(under, theme::cursor()),
        Span::raw(after),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, type_name: &str, nullable: bool, default: Option<&str>) -> Column {
        Column {
            name: name.to_string(),
            type_name: type_name.to_string(),
            nullable,
            default: default.map(String::from),
            auto_increment: false,
        }
    }

    fn films() -> Vec<Column> {
        vec![
            column("id", "integer", true, None),
            column("title", "text", false, None),
            column("rating", "text", true, Some("'G'")),
            column("created_at", "datetime", true, Some("current_timestamp")),
            column("token", "text", true, Some("(lower(hex(randomblob(4))))")),
        ]
    }

    fn adding() -> RecordForm {
        RecordForm::adding(
            "films",
            &films(),
            "sqlite",
            &["id".to_string()],
            &Values::new(),
        )
    }

    #[test]
    fn a_new_row_fills_plain_defaults_and_marks_what_is_left() {
        let form = adding();

        assert_eq!(form.current().name, "title");
        assert_eq!(form.value("rating"), Some("G"));
        assert_eq!(form.value("created_at"), Some("now()"));
        assert_eq!(form.state("token"), State::Untouched);
        let hints: Vec<&str> = form
            .fields
            .iter()
            .map(|field| field.hint.as_str())
            .collect();
        assert_eq!(
            hints,
            [
                "auto",
                "required",
                "",
                "",
                "default lower(hex(randomblob(4)))"
            ]
        );
    }

    #[test]
    fn a_given_key_is_filled_and_skipped() {
        let given = vec![("id".to_string(), Some("7".to_string()))];
        let form = RecordForm::adding("films", &films(), "sqlite", &[], &given);

        assert_eq!(form.value("id"), Some("7"));
        assert_eq!(form.current().name, "title");
    }

    #[test]
    fn the_key_that_names_the_row_is_not_edited() {
        let row = vec![
            ("id".to_string(), Some("1".to_string())),
            ("title".to_string(), Some("Alien".to_string())),
        ];
        let mut form = RecordForm::editing("films", &films(), &row, "id");
        form.start();

        assert!(form.editor.is_none());
        assert!(form.error.as_deref().unwrap().contains("names the row"));
        assert_eq!(form.title(), "EDIT ROW  ·  films  ·  id 1");
    }

    #[test]
    fn only_changed_fields_are_kept_for_an_existing_row() {
        let row = vec![
            ("id".to_string(), Some("1".to_string())),
            ("title".to_string(), Some("Alien".to_string())),
        ];
        let mut form = RecordForm::editing("films", &films(), &row, "id");
        form.jump(1);
        form.start();
        form.editor.as_mut().unwrap().set("Aliens");
        assert!(form.keep());

        assert_eq!(
            form.values(),
            [("title".to_string(), Some("Aliens".to_string()))]
        );

        form.reset();

        assert!(!form.dirty());
    }

    #[test]
    fn null_is_refused_where_the_column_is_not_null() {
        let mut form = adding();
        form.set_null();

        assert!(form.error.as_deref().unwrap().contains("not null"));

        form.jump(2);
        form.set_null();

        assert_eq!(form.state("rating"), State::Null);
    }

    #[test]
    fn json_is_edited_pretty_and_kept_compact() {
        let mut form = adding();
        form.jump(4);
        form.values
            .put("token", Some("{\"year\":1979}".to_string()));
        form.start();

        assert!(form.expanded);
        assert!(
            form.editor
                .as_ref()
                .unwrap()
                .buffer()
                .contains("\"year\": 1979")
        );

        form.editor.as_mut().unwrap().set("{\"year\":");
        assert!(!form.keep());
        assert!(form.error.as_deref().unwrap().contains("not valid json"));

        form.editor.as_mut().unwrap().set("{\"year\": 1986}");
        assert!(form.keep());
        assert_eq!(form.value("token"), Some("{\"year\":1986}"));
    }

    #[test]
    fn a_long_value_is_clamped_inside_the_box() {
        assert_eq!(clamp("short", 10), "short");
        assert_eq!(clamp("a much longer value", 10), "a much … ↗");
        assert_eq!(clamp("two\nlines", 20), "two lines… ↗");
    }
}
