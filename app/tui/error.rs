use zerorust_tui::prelude::*;

use super::chrome::theme;
use super::text::width;

pub const WIDTH: u16 = 76;

pub struct ErrorBox<'a> {
    pub message: &'a str,
    pub notes: &'a [String],
    pub offset: usize,
}

impl ErrorBox<'_> {
    pub fn rows(&self, box_width: usize) -> usize {
        let chrome = if self.notes.is_empty() { 5 } else { 6 };
        wrapped(self.message, box_width.saturating_sub(6)).len() + self.notes.len() + chrome
    }

    pub fn content(&self, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::default()];
        lines.extend(
            wrapped(self.message, inner_width.saturating_sub(4))
                .into_iter()
                .skip(self.offset)
                .map(|line| Line::raw(format!("  {line}"))),
        );
        if !self.notes.is_empty() {
            lines.push(Line::default());
            lines.extend(self.notes.iter().map(|note| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(note.clone(), theme::dim()),
                ])
            }));
        }
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("y copies it    esc closes", theme::dim()),
        ]));
        lines.truncate(inner_height);
        lines
    }
}

fn wrapped(message: &str, room: usize) -> Vec<String> {
    message
        .split('\n')
        .flat_map(|paragraph| word_wrap(paragraph, room.max(10)))
        .collect()
}

fn word_wrap(paragraph: &str, room: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in paragraph.split(' ') {
        let mut word = word.to_string();
        if !line.is_empty() && width(&line) + 1 + width(&word) > room {
            lines.push(std::mem::take(&mut line));
        } else if !line.is_empty() {
            line.push(' ');
        }
        while width(&word) > room {
            let head: String = word.chars().take(room).collect();
            word = word.chars().skip(room).collect();
            lines.push(head);
        }
        line.push_str(&word);
    }
    lines.push(line);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_spaces_and_cuts_words_longer_than_the_room() {
        assert_eq!(
            wrapped("no such table: nowhere\n\nselect * from nowhere", 12),
            [
                "no such",
                "table:",
                "nowhere",
                "",
                "select *",
                "from nowhere"
            ]
        );
        assert_eq!(
            word_wrap(&"x".repeat(25), 10),
            ["xxxxxxxxxx", "xxxxxxxxxx", "xxxxx"]
        );
    }

    #[test]
    fn is_as_tall_as_the_message_and_its_notes() {
        let notes = ["u to drop it".to_string()];
        let problem = ErrorBox {
            message: "a refusal",
            notes: &notes,
            offset: 0,
        };

        assert_eq!(problem.rows(40), 8);
        assert_eq!(problem.content(38, 20).len(), 6);
    }
}
