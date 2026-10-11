use super::Vim;
use super::motions;
use super::range::Range;
use super::text::Text;
use crate::tui::editor::QueryEditor;

impl Vim {
    pub(super) fn apply(&mut self, editor: &mut QueryEditor, operator: char, range: Range) {
        if range.linewise {
            self.apply_to_lines(editor, operator, range);
        } else {
            self.apply_to_characters(editor, operator, range);
        }
    }

    fn apply_to_characters(&mut self, editor: &mut QueryEditor, operator: char, range: Range) {
        let taken = Text::of(&editor.buffer()).slice(range.start, range.end);
        match operator {
            'd' => self.delete(editor, range, &taken),
            'c' => {
                self.delete(editor, range, &taken);
                self.enter_insert(editor);
            }
            'y' => self.yank(editor, &taken, false, range.start),
            '~' => swap_case(editor, range, &taken),
            '>' | '<' | 'J' => self.apply_to_lines(
                editor,
                operator,
                Range::lines(range.start, range.start.max(range.end.saturating_sub(1))),
            ),
            _ => {}
        }
    }

    fn apply_to_lines(&mut self, editor: &mut QueryEditor, operator: char, range: Range) {
        let text = Text::of(&editor.buffer());
        let first = text.line_of(range.start);
        let last = text.line_of(range.end);
        let start = text.start_of_line(first);
        let end = text.line_end(text.start_of_line(last));
        let taken = format!("{}\n", text.slice(start, end));

        match operator {
            'd' => self.delete_lines(editor, &text, (start, end), &taken),
            'c' => self.change_lines(editor, (start, end), &taken),
            'y' => self.yank_lines(editor, &text, first, &taken),
            '>' => indent(editor, first, last, |line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("  {line}")
                }
            }),
            '<' => indent(editor, first, last, |line| {
                let spaces = line
                    .chars()
                    .take(2)
                    .take_while(|character| *character == ' ')
                    .count();
                line[spaces..].to_string()
            }),
            '~' => swap_case(editor, Range::new(start, end), &text.slice(start, end)),
            'J' => join_lines(editor, first, last),
            _ => {}
        }
    }

    fn delete(&mut self, editor: &mut QueryEditor, range: Range, taken: &str) {
        self.store(taken, false);
        editor.replace(range.start, range.end, "");
    }

    fn yank(&mut self, editor: &mut QueryEditor, taken: &str, linewise: bool, cursor: usize) {
        self.store(taken, linewise);
        self.yanked(taken);
        editor.move_to(cursor);
    }

    fn delete_lines(
        &mut self,
        editor: &mut QueryEditor,
        text: &Text,
        (start, end): (usize, usize),
        taken: &str,
    ) {
        self.store(taken, true);
        let (from, to) = if end < text.length() {
            (start, end + 1)
        } else if start > 0 {
            (start - 1, end)
        } else {
            (start, end)
        };
        editor.replace(from, to, "");

        let after = Text::of(&editor.buffer());
        let line = if from == start {
            start
        } else {
            after.line_start(from)
        };
        editor.move_to(motions::first_non_blank(&after, line));
    }

    fn change_lines(
        &mut self,
        editor: &mut QueryEditor,
        (start, end): (usize, usize),
        taken: &str,
    ) {
        self.store(taken, true);
        editor.move_to(start);
        let indent = if editor.autoindents() {
            editor.indentation()
        } else {
            String::new()
        };
        editor.replace(start, end, &indent);
        self.enter_insert(editor);
    }

    fn yank_lines(&mut self, editor: &mut QueryEditor, text: &Text, first: usize, taken: &str) {
        let cursor = if text.line_of(editor.cursor()) == first {
            editor.cursor()
        } else {
            let start = text.start_of_line(first);
            start
                + editor
                    .cursor_column()
                    .min((text.line_end(start) - start).saturating_sub(1))
        };
        self.yank(editor, taken, true, cursor);
    }
}

pub(super) fn join_lines(editor: &mut QueryEditor, first_line: usize, last_line: usize) {
    let last_line = last_line.max(first_line + 1);
    let mut joined_at = None;

    for _ in first_line..last_line {
        let text = Text::of(&editor.buffer());
        if first_line + 1 >= text.line_count() {
            break;
        }
        let end = text.line_end(text.start_of_line(first_line));
        let mut next = end + 1;
        while matches!(text.at(next), Some(' ' | '\t')) {
            next += 1;
        }
        let glued = next >= text.length()
            || matches!(text.at(next), Some('\n' | ')'))
            || end == text.line_start(end);
        editor.replace(end, next, if glued { "" } else { " " });
        joined_at = Some(end);
    }

    if let Some(at) = joined_at {
        editor.move_to(at);
    }
}

fn indent(editor: &mut QueryEditor, first: usize, last: usize, reshape: impl Fn(&str) -> String) {
    let lines: Vec<String> = editor
        .lines()
        .iter()
        .enumerate()
        .map(|(number, line)| {
            if (first..=last).contains(&number) {
                reshape(line)
            } else {
                line.clone()
            }
        })
        .collect();
    editor.set(&lines.join("\n"));
    let text = Text::of(&editor.buffer());
    editor.move_to(motions::first_non_blank(&text, text.start_of_line(first)));
}

fn swap_case(editor: &mut QueryEditor, range: Range, taken: &str) {
    let swapped: String = taken
        .chars()
        .map(|character| {
            let upper: String = character.to_uppercase().collect();
            if upper == character.to_string() {
                character.to_lowercase().collect()
            } else {
                upper
            }
        })
        .collect();
    editor.replace(range.start, range.end, &swapped);
    editor.move_to(range.start);
}
