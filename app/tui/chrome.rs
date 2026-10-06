use zerorust_tui::prelude::*;
use zerorust_tui::ratatui::buffer::Buffer;

use super::text::{truncate, width};

pub const MIN_COLUMNS: u16 = 60;
pub const MIN_LINES: u16 = 12;

pub mod theme {
    use zerorust_tui::prelude::*;

    use crate::settings;

    pub fn dim() -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    pub fn bold() -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn paint(color: &str) -> Style {
        match color {
            "default" => Style::new(),
            "black" => Style::new().fg(Color::Black),
            "red" => Style::new().fg(Color::Red),
            "green" => Style::new().fg(Color::Green),
            "yellow" => Style::new().fg(Color::Yellow),
            "blue" => Style::new().fg(Color::Blue),
            "magenta" => Style::new().fg(Color::Magenta),
            "cyan" => Style::new().fg(Color::Cyan),
            "white" => Style::new().fg(Color::White),
            "gray" => Style::new().fg(Color::DarkGray),
            _ => dim(),
        }
    }

    pub fn highlight(color: &str) -> Style {
        match color {
            "default" => Style::new().add_modifier(Modifier::REVERSED),
            other => paint(other).add_modifier(Modifier::REVERSED),
        }
    }

    fn color(key: &str, fallback: &str) -> String {
        settings::with(|settings| settings.color(key, fallback))
    }

    pub fn border(focused: bool, modal: bool) -> Style {
        match (modal, focused) {
            (true, true) => paint(&color("modal_focus_border", "gray")),
            (true, false) => paint(&color("modal_border", "gray")),
            (false, true) => paint(&color("focus_border", "dim")),
            (false, false) => paint(&color("border", "dim")),
        }
    }

    pub fn title(focused: bool, modal: bool) -> Style {
        let style = match (modal, focused) {
            (true, true) => paint(&color("modal_focus_title", "white")),
            (true, false) => paint(&color("modal_title", "white")),
            (false, true) => paint(&color("focus_title", "dim")),
            (false, false) => paint(&color("border", "dim")),
        };
        if focused {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    pub fn cursor() -> Style {
        highlight(&color("cursor", "default"))
    }

    pub fn selection() -> Style {
        highlight(&color("selection", "default"))
    }

    pub fn problem() -> Style {
        paint(&color("deleted", "red"))
    }

    pub fn fresh() -> Style {
        title(true, false)
    }

    pub fn sql(kind: &str) -> Style {
        match kind {
            "keyword" | "literal" => paint("magenta"),
            "identifier" | "key" => paint("cyan"),
            "string" => paint("green"),
            "number" => paint("yellow"),
            "operator" | "comment" | "punctuation" | "gutter" => dim(),
            _ => Style::new(),
        }
    }

    pub fn named(color: &str) -> Option<Color> {
        paint(color).fg
    }
}

#[derive(Default)]
pub struct Island {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub title: String,
    pub focused: bool,
    pub modal: bool,
    pub bare: bool,
    pub collapsed: bool,
    pub title_under_cursor: bool,
    pub content: Vec<Line<'static>>,
    pub joins: Vec<usize>,
    pub rules: Vec<usize>,
}

impl Island {
    pub fn placed(x: u16, y: u16, width: u16, height: u16) -> Self {
        Island {
            x,
            y,
            width,
            height,
            ..Island::default()
        }
    }

    pub fn inner_width(&self) -> usize {
        self.width.saturating_sub(2) as usize
    }

    pub fn inner_height(&self) -> usize {
        self.height.saturating_sub(2) as usize
    }

    pub fn draw(&self, buffer: &mut Buffer) {
        if self.bare {
            for row in 0..self.height {
                let line = self.content.get(row as usize).cloned().unwrap_or_default();
                self.write(buffer, row, line);
            }
            return;
        }

        let inner = self.inner_width();
        let edge = theme::border(self.focused, self.modal);
        let run = |join: char| -> Vec<char> {
            let mut characters = vec!['─'; inner];
            for position in &self.joins {
                if *position < inner {
                    characters[*position] = join;
                }
            }
            characters
        };

        let top = if self.title.is_empty() {
            Line::styled(format!("╭{}╮", run('┬').iter().collect::<String>()), edge)
        } else {
            let label = format!(
                " {} ",
                truncate(&self.title, inner.saturating_sub(4).max(1))
            );
            let tail: String = run('┬')
                .iter()
                .skip((width(&label) + 1).min(inner))
                .collect();
            let label_style = if self.title_under_cursor {
                theme::cursor()
            } else {
                theme::title(self.focused, self.modal)
            };
            Line::from(vec![
                Span::styled("╭─", edge),
                Span::styled(label, label_style),
                Span::styled(format!("{tail}╮"), edge),
            ])
        };
        self.write(buffer, 0, top);
        if self.collapsed {
            return;
        }

        for row in 0..self.inner_height() {
            let (left, right) = if self.rules.contains(&row) {
                ("├", "┤")
            } else {
                ("│", "│")
            };
            let content = self.content.get(row).cloned().unwrap_or_default();
            let padding = inner.saturating_sub(content.width());
            let mut spans = vec![Span::styled(left, edge)];
            let line_style = content.style;
            spans.extend(
                content
                    .spans
                    .into_iter()
                    .map(|span| Span::styled(span.content, line_style.patch(span.style))),
            );
            spans.push(Span::styled(" ".repeat(padding), line_style));
            spans.push(Span::styled(right, edge));
            self.write(buffer, row as u16 + 1, Line::from(spans));
        }

        let bottom = format!("╰{}╯", run('┴').iter().collect::<String>());
        self.write(
            buffer,
            self.height.saturating_sub(1),
            Line::styled(bottom, edge),
        );
    }

    fn write(&self, buffer: &mut Buffer, row: u16, line: Line<'static>) {
        let area = buffer.area;
        let x = self.x.saturating_sub(1);
        let y = self.y.saturating_sub(1) + row;
        if y >= area.bottom() || x >= area.right() {
            return;
        }
        let room = self.width.min(area.right() - x);
        for column in x..x + room {
            buffer[(column, y)].reset();
        }
        buffer.set_line(x, y, &line, room);
    }
}

pub struct Modal {
    frame_width: u16,
    top: u16,
    frame_height: u16,
    box_width: u16,
}

impl Modal {
    pub fn new(frame_width: u16, top: u16, frame_height: u16, box_width: u16) -> Self {
        Modal {
            frame_width,
            top,
            frame_height,
            box_width,
        }
    }

    pub fn draw(&self, buffer: &mut Buffer, mut island: Island, height: u16) {
        island.focused = true;
        self.draw_stack(buffer, vec![(island, height)]);
    }

    pub fn draw_stack(&self, buffer: &mut Buffer, boxes: Vec<(Island, u16)>) {
        const GAP: u16 = 1;
        if boxes.is_empty() {
            return;
        }
        let total =
            boxes.iter().map(|(_, height)| height).sum::<u16>() + (boxes.len() as u16 - 1) * GAP;
        let x = ((self.frame_width.saturating_sub(self.box_width)) / 2 + 1).max(1);
        let mut y = self.top + self.frame_height.saturating_sub(total) / 2;

        let ring = crate::settings::with(|settings| settings.modal_ring);
        let (pad, margin) = if ring { (3u16, 2u16) } else { (2u16, 1u16) };
        let first = self.top.max(y.saturating_sub(margin));
        let last = (self.top + self.frame_height - 1).min(y + total - 1 + margin);
        let mut backdrop = Island::placed(
            x.saturating_sub(pad).max(1),
            first,
            self.frame_width.min(self.box_width + pad * 2),
            total.max(last - first + 1),
        );
        backdrop.modal = true;
        backdrop.focused = true;
        backdrop.bare = !ring;
        backdrop.draw(buffer);

        for (mut island, height) in boxes {
            island.x = x;
            island.y = y;
            island.width = self.box_width;
            island.height = height.max(3);
            island.modal = true;
            island.draw(buffer);
            y += height + GAP;
        }
    }
}

pub fn hotkeys(offers: &[(String, &str)], width: usize) -> Line<'static> {
    const GAP: &str = "   ";
    let offers: Vec<&(String, &str)> = offers.iter().filter(|(key, _)| !key.is_empty()).collect();
    let plain =
        |(key, label): &&(String, &str)| super::text::width(key) + 1 + super::text::width(label);

    let (head, tail) = if offers.len() > 2 {
        offers.split_at(offers.len() - 2)
    } else {
        (offers.as_slice(), &[][..])
    };

    let mut room = width as isize
        - 1
        - tail
            .iter()
            .map(|offer| (plain(offer) + GAP.len()) as isize)
            .sum::<isize>();
    let mut kept: Vec<&&(String, &str)> = Vec::new();
    for offer in head {
        room -= (plain(offer) + GAP.len()) as isize;
        if room < 0 {
            break;
        }
        kept.push(offer);
    }
    kept.extend(tail);

    let mut spans = vec![Span::raw(" ")];
    for (index, (key, label)) in kept.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(GAP));
        }
        spans.push(Span::raw(key.clone()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label.to_string(), theme::dim()));
    }
    Line::from(spans)
}

pub fn too_small(frame: &mut Frame) -> bool {
    let area = frame.area();
    if area.width >= MIN_COLUMNS && area.height >= MIN_LINES {
        return false;
    }

    let message = [
        "Make the window bigger".to_string(),
        format!("tql needs {MIN_COLUMNS}×{MIN_LINES}"),
        format!("this is {}×{}", area.width, area.height),
    ];
    for (row, text) in message.iter().enumerate().take(area.height as usize) {
        let text: String = text.chars().take(area.width.max(1) as usize).collect();
        frame
            .buffer_mut()
            .set_string(0, row as u16, text, Style::new());
    }
    true
}

pub fn line_at(frame: &mut Frame, row: u16, line: Line<'static>) {
    let area = frame.area();
    if row < area.height {
        frame.buffer_mut().set_line(0, row, &line, area.width);
    }
}
