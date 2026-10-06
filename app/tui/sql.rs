use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;
use zerorust_tui::prelude::*;

use super::chrome::theme;
use super::editor::QueryEditor;

pub const KEYWORDS: &[&str] = &[
    "select",
    "from",
    "where",
    "and",
    "or",
    "not",
    "null",
    "is",
    "in",
    "like",
    "ilike",
    "between",
    "order",
    "by",
    "group",
    "having",
    "limit",
    "offset",
    "asc",
    "desc",
    "insert",
    "into",
    "values",
    "update",
    "set",
    "delete",
    "truncate",
    "join",
    "left",
    "right",
    "inner",
    "outer",
    "full",
    "cross",
    "on",
    "using",
    "as",
    "distinct",
    "union",
    "all",
    "with",
    "case",
    "when",
    "then",
    "else",
    "end",
    "exists",
    "create",
    "table",
    "alter",
    "drop",
    "index",
    "view",
    "primary",
    "key",
    "foreign",
    "references",
    "default",
    "unique",
    "constraint",
    "cascade",
    "show",
    "explain",
    "describe",
    "pragma",
    "begin",
    "commit",
    "rollback",
    "true",
    "false",
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "coalesce",
    "cast",
];

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)(--[^\n]*)|(/\*.*?\*/)|('(?:''|[^'])*')|("(?:[^"]|"")*"|`[^`]*`|\[[^\]]*\])|\b({})\b|(-?\d+(?:\.\d+)?)|([=<>!+\-*/%,;()])"#,
        KEYWORDS.join("|")
    ))
    .expect("the SQL token pattern is valid")
});

static LIMIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\blimit \d+").expect("the limit pattern is valid"));

static FETCH_NEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bfetch next \d+ rows only").expect("the fetch next pattern is valid")
});

const KINDS: [&str; 7] = [
    "comment",
    "comment",
    "string",
    "identifier",
    "keyword",
    "number",
    "operator",
];

pub fn tokenise(line: &str) -> Vec<(&'static str, String)> {
    let mut tokens = Vec::new();
    let mut offset = 0;

    for captures in TOKEN.captures_iter(line) {
        let whole = captures.get(0).expect("a match has a whole");
        if whole.start() > offset {
            tokens.push(("plain", line[offset..whole.start()].to_string()));
        }
        let kind = (1..=KINDS.len())
            .find(|group| {
                captures
                    .get(*group)
                    .is_some_and(|found| !found.as_str().is_empty())
            })
            .map(|group| KINDS[group - 1])
            .unwrap_or("plain");
        tokens.push((kind, whole.as_str().to_string()));
        offset = whole.end();
    }

    if offset < line.len() {
        tokens.push(("plain", line[offset..].to_string()));
    }
    tokens
}

pub fn shown(statement: &str, page: usize) -> String {
    let limited = LIMIT.replace_all(statement, format!("limit {page}"));
    FETCH_NEXT
        .replace_all(&limited, format!("fetch next {page} rows only"))
        .into_owned()
}

type Painted = Vec<(&'static str, char)>;

fn characters(line: &str) -> Painted {
    tokenise(line)
        .into_iter()
        .flat_map(|(kind, text)| {
            text.chars()
                .map(move |character| (kind, character))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn wrap(characters: &[(&'static str, char)], width: usize) -> Vec<usize> {
    let mut starts = vec![0];
    let mut at = 0;
    while width > 0 && characters.len() - at > width {
        let space = characters[at..at + width]
            .iter()
            .rposition(|(_, character)| *character == ' ');
        at += match space {
            None | Some(0) => width,
            Some(space) => space + 1,
        };
        starts.push(at);
    }
    starts
}

#[derive(Debug)]
pub struct Pane {
    pub lines: Vec<Line<'static>>,
    pub rows: Vec<(usize, usize)>,
    pub first_line: usize,
    pub cursor: (usize, usize),
}

impl Pane {
    pub fn position_at(&self, local_row: isize, local_column: isize) -> (usize, usize) {
        let column = local_column.max(0) as usize;
        let Some(last) = self.rows.len().checked_sub(1) else {
            return (self.first_line, column);
        };
        let (line, from) = self.rows[(local_row.max(0) as usize).min(last)];
        (line, from + column)
    }
}

struct Row {
    line: usize,
    from: usize,
    characters: Painted,
}

#[derive(Debug, Clone)]
pub struct Marks {
    pub paints_cursor: bool,
    pub selection: Option<Range<usize>>,
}

impl Default for Marks {
    fn default() -> Self {
        Marks {
            paints_cursor: true,
            selection: None,
        }
    }
}

pub fn pane(
    editor: &QueryEditor,
    running: Option<&str>,
    show_cursor: bool,
    marks: &Marks,
    (inner_width, inner_height): (usize, usize),
) -> Pane {
    if let Some(running) = running.filter(|_| !show_cursor && editor.is_empty()) {
        return Pane {
            lines: split(running, inner_width)
                .iter()
                .take(inner_height)
                .map(|line| paint(&characters(line), inner_width, None, None))
                .collect(),
            rows: Vec::new(),
            first_line: 0,
            cursor: (0, 0),
        };
    }

    let (rows, cursor_row) = layout(editor, inner_width);
    let start = (cursor_row + 1).saturating_sub(inner_height);
    let cursor_column = editor.cursor_column();
    let shown = rows.iter().enumerate().skip(start).take(inner_height);
    let line_offsets: Vec<usize> = editor
        .lines()
        .iter()
        .scan(0, |offset, line| {
            let line_offset = *offset;
            *offset += line.chars().count() + 1;
            Some(line_offset)
        })
        .collect();

    Pane {
        lines: shown
            .clone()
            .map(|(index, row)| {
                let cursor = (show_cursor && marks.paints_cursor && index == cursor_row)
                    .then(|| cursor_column - row.from);
                let selected = marks.selection.as_ref().map(|selection| {
                    let offset = line_offsets[row.line] + row.from;
                    selection.start.saturating_sub(offset)..selection.end.saturating_sub(offset)
                });
                paint(&row.characters, inner_width, cursor, selected)
            })
            .collect(),
        rows: shown.map(|(_, row)| (row.line, row.from)).collect(),
        first_line: rows.get(start).map(|row| row.line).unwrap_or_default(),
        cursor: (
            cursor_row - start,
            cursor_column - rows.get(cursor_row).map(|row| row.from).unwrap_or_default(),
        ),
    }
}

fn layout(editor: &QueryEditor, inner_width: usize) -> (Vec<Row>, usize) {
    let (cursor_line, cursor_column) = (editor.cursor_line(), editor.cursor_column());
    let mut rows = Vec::new();
    let mut cursor_row = 0;

    for (number, line) in editor.lines().iter().enumerate() {
        let characters = characters(line);
        let mut starts = wrap(&characters, inner_width);
        let last_start = starts.last().copied().unwrap_or_default();
        if number == cursor_line
            && cursor_column == characters.len()
            && characters.len() - last_start >= inner_width
        {
            starts.push(characters.len());
        }

        for (index, from) in starts.iter().enumerate() {
            let next = starts.get(index + 1).copied();
            let to = next.unwrap_or(characters.len());
            if number == cursor_line
                && cursor_column >= *from
                && (cursor_column < to || next.is_none())
            {
                cursor_row = rows.len();
            }
            rows.push(Row {
                line: number,
                from: *from,
                characters: characters[*from..to].to_vec(),
            });
        }
    }
    (rows, cursor_row)
}

fn paint(
    characters: &[(&'static str, char)],
    width: usize,
    cursor: Option<usize>,
    selected: Option<Range<usize>>,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut style = Style::new();
    let is_selected = |index: usize| {
        selected
            .as_ref()
            .is_some_and(|selected| selected.contains(&index))
    };

    for (index, (kind, character)) in characters.iter().enumerate() {
        let wanted = if Some(index) == cursor {
            theme::cursor()
        } else if is_selected(index) {
            theme::selection()
        } else {
            theme::sql(kind)
        };
        if wanted != style && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        style = wanted;
        run.push(*character);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, style));
    }
    if cursor.is_some_and(|cursor| cursor >= characters.len()) && characters.len() < width {
        spans.push(Span::styled(" ", theme::cursor()));
    }
    if cursor.is_none() && characters.is_empty() && is_selected(0) {
        spans.push(Span::styled(" ", theme::selection()));
    }
    Line::from(spans)
}

fn split(text: &str, width: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|line| {
            let characters: Vec<char> = line.chars().collect();
            if characters.is_empty() || width == 0 {
                return vec![line.to_string()];
            }
            characters
                .chunks(width)
                .map(|chunk| chunk.iter().collect())
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenises_like_tqls_sql_class() {
        let kinds: Vec<_> =
            tokenise("select * from `users` where name = 'O''Brien' and id > 10 -- note")
                .into_iter()
                .filter(|(kind, _)| *kind != "plain")
                .collect();

        assert_eq!(
            kinds,
            [
                ("keyword", "select".to_string()),
                ("operator", "*".to_string()),
                ("keyword", "from".to_string()),
                ("identifier", "`users`".to_string()),
                ("keyword", "where".to_string()),
                ("operator", "=".to_string()),
                ("string", "'O''Brien'".to_string()),
                ("keyword", "and".to_string()),
                ("operator", ">".to_string()),
                ("number", "10".to_string()),
                ("comment", "-- note".to_string()),
            ]
        );
    }

    #[test]
    fn shows_the_page_size_rather_than_the_lookahead_row() {
        assert_eq!(
            shown("select * from \"t\" limit 101 offset 0", 100),
            "select * from \"t\" limit 100 offset 0"
        );
        assert_eq!(
            shown(
                "select * from [t] order by (select null) offset 0 rows fetch next 101 rows only",
                100
            ),
            "select * from [t] order by (select null) offset 0 rows fetch next 100 rows only"
        );
    }

    fn painted(sql: &str, width: usize, height: usize, cursor: Option<usize>) -> Vec<String> {
        let mut editor = QueryEditor::new(false);
        editor.set(sql);
        if let Some(column) = cursor {
            editor.move_to_line_column(0, column);
        }
        pane(&editor, None, true, &Marks::default(), (width, height))
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| {
                        if span.style == theme::cursor() {
                            format!("[{}]", span.content)
                        } else {
                            span.content.to_string()
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn wraps_a_long_line_at_a_space_so_every_row_fits_the_pane() {
        let rows = painted(
            "select id, name, email from customers where name like \"%ada%\"",
            20,
            10,
            Some(0),
        );

        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], "[s]elect id, name, ");
        assert_eq!(rows[1], "email from ");
    }

    #[test]
    fn breaks_a_word_wider_than_the_pane_where_it_has_to() {
        assert_eq!(
            painted(&"x".repeat(25), 10, 10, Some(0))
                .join("|")
                .replace(['[', ']'], ""),
            "xxxxxxxxxx|xxxxxxxxxx|xxxxx"
        );
    }

    #[test]
    fn keeps_the_caret_in_view_at_the_end_of_a_long_line_and_scrolls_to_it() {
        assert!(
            painted("select * from customers", 10, 10, None)
                .last()
                .unwrap()
                .ends_with("[ ]")
        );

        let mut editor = QueryEditor::new(false);
        editor.set(&"word ".repeat(30));
        let scrolled = pane(&editor, None, true, &Marks::default(), (10, 3));
        assert_eq!(scrolled.lines.len(), 3);
        assert!(scrolled.position_at(0, 0).1 > 0);
    }

    #[test]
    fn maps_a_click_on_a_wrapped_row_back_to_the_right_place_in_the_line() {
        let mut editor = QueryEditor::new(false);
        editor.set("select id, name, email from customers");
        editor.move_to_start();
        let laid_out = pane(&editor, None, true, &Marks::default(), (20, 10));

        assert_eq!(laid_out.position_at(1, 2), (0, 19));
        assert_eq!(laid_out.position_at(0, 3), (0, 3));
    }

    #[test]
    fn shows_what_ran_when_the_editor_is_empty_and_not_focused() {
        let text: Vec<String> = pane(
            &QueryEditor::new(false),
            Some("select 1"),
            false,
            &Marks::default(),
            (20, 5),
        )
        .lines
        .iter()
        .map(|line| line.to_string())
        .collect();
        assert_eq!(text, ["select 1"]);
    }
}
