use zerorust_tui::prelude::*;

use super::ago::{ago, now};
use super::chrome::{self, theme};
use super::text::{pad, truncate, width, window};
use crate::store::Connection;

const MIN_LINES: u16 = 10;
const FIRST_BODY_ROW: i64 = 4;
const BAR: &str = " ↑↓ Move    ↵ Open    :q Quit";

pub struct Picker {
    pub connections: Vec<Connection>,
    pub index: usize,
    pub notice: Option<String>,
    pub status: Option<String>,
    pub command: Option<String>,
    start: usize,
}

struct Row {
    name: String,
    tag: String,
    tag_color: Option<Color>,
    driver: String,
    place: String,
    used: String,
}

impl Picker {
    pub fn new(connections: Vec<Connection>, notice: Option<String>) -> Self {
        Picker {
            connections,
            index: 0,
            notice,
            status: None,
            command: None,
            start: 0,
        }
    }

    fn rows(&self) -> Vec<Row> {
        let now = now();
        self.connections
            .iter()
            .map(|connection| {
                let tag = connection
                    .tag
                    .clone()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                Row {
                    name: connection.name.clone(),
                    tag: if connection.read_only {
                        format!("{tag} ro").trim().to_string()
                    } else {
                        tag
                    },
                    tag_color: connection.tag_color().and_then(theme::named),
                    driver: connection.driver.clone(),
                    place: connection.describe(),
                    used: ago(connection.last_used_at.as_deref(), now),
                }
            })
            .collect()
    }

    fn widths(rows: &[Row], inner: usize) -> [usize; 4] {
        let count = |text: &str| text.chars().count();
        let mut name = rows
            .iter()
            .map(|row| count(&row.name) + 4)
            .fold(8, usize::max)
            .min(32);
        let mut tag = rows
            .iter()
            .map(|row| count(&row.tag))
            .fold(3, usize::max)
            .min(16);
        let mut used = rows
            .iter()
            .map(|row| count(&row.used))
            .fold(9, usize::max)
            .min(20);

        let over = (name + tag + used + 21).saturating_sub(inner);
        let used_gives = over.min(used - 9);
        let tag_gives = (over - used_gives).min(tag - 3);
        let name_gives = (over - used_gives - tag_gives).min(name - 8);
        used -= used_gives;
        tag -= tag_gives;
        name -= name_gives;

        let place = inner.saturating_sub(name + tag + used + 11).max(10);
        [name, tag, place, used]
    }

    fn rule(widths: &[usize; 4], join: char, inner: usize) -> String {
        let body = widths
            .iter()
            .map(|width| "─".repeat(width + 2))
            .collect::<Vec<_>>()
            .join(&join.to_string());
        let used = body.chars().count();
        format!("{body}{}", "─".repeat(inner.saturating_sub(used)))
    }

    fn row_line(&self, row: &Row, selected: bool, widths: &[usize; 4]) -> Line<'static> {
        let grid = theme::border(true, false);
        let icon = icon(&row.driver);
        let marker = if selected { "▸" } else { " " };
        let label = pad(
            &truncate(&row.name, widths[0].saturating_sub(4)),
            widths[0].saturating_sub(4),
        );
        let cell = |text: &str, width: usize| format!(" {} ", pad(&truncate(text, width), width));

        if selected {
            let text = format!(
                " {marker} {icon} {label} │{}│{}│{}",
                cell(&row.tag, widths[1]),
                cell(&row.place, widths[2]),
                cell(&row.used, widths[3])
            );
            let room = widths.iter().sum::<usize>() + 3 * widths.len() - 1;
            return Line::styled(pad(&text, room), theme::selection());
        }

        let shade = row.tag_color.unwrap_or_else(|| driver_color(&row.driver));
        let tag_style = row
            .tag_color
            .map(|color| Style::new().fg(color))
            .unwrap_or_else(theme::dim);
        Line::from(vec![
            Span::raw(format!(" {marker} ")),
            Span::styled(icon.to_string(), Style::new().fg(shade)),
            Span::raw(format!(" {label} ")),
            Span::styled("│", grid),
            Span::styled(cell(&row.tag, widths[1]), tag_style),
            Span::styled("│", grid),
            Span::styled(cell(&row.place, widths[2]), theme::dim()),
            Span::styled("│", grid),
            Span::styled(cell(&row.used, widths[3]), theme::dim()),
        ])
    }

    fn status_line(&self) -> Line<'static> {
        if let Some(command) = &self.command {
            return Line::raw(format!(" :{command}█"));
        }
        let count = format!("{} connections", self.connections.len());
        match (&self.status, &self.notice) {
            (Some(status), _) => Line::styled(format!(" {status}"), theme::dim()),
            (None, None) => Line::styled(format!(" {count}"), theme::dim()),
            (None, Some(notice)) => Line::from(vec![
                Span::styled(format!(" {count} · "), theme::dim()),
                Span::styled(notice.clone(), Style::new().fg(Color::Cyan)),
            ]),
        }
    }

    fn type_into_command(&mut self, key: Key) -> Flow<Option<Connection>> {
        let Some(command) = self.command.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.command = None,
            Key::Enter => {
                let command = command.trim().to_string();
                self.command = None;
                match command.as_str() {
                    "q" | "q!" | "quit" => return Flow::Exit(None),
                    "w" | "write" => self.status = Some("nothing to write".to_string()),
                    "new" => self.status = Some("adding connections is not ported yet".to_string()),
                    other => self.status = Some(format!("unknown command: :{other}")),
                }
            }
            Key::Backspace if command.is_empty() => self.command = None,
            Key::Backspace => {
                command.pop();
            }
            Key::Char(character) => command.push(character),
            _ => {}
        }
        Flow::Continue
    }
}

fn icon(driver: &str) -> char {
    match driver {
        "mysql" | "mariadb" => '\u{e704}',
        "pgsql" => '\u{e76e}',
        "sqlite" => '\u{e7c4}',
        _ => '\u{f1c0}',
    }
}

fn driver_color(driver: &str) -> Color {
    match driver {
        "mysql" | "mariadb" => Color::Yellow,
        "pgsql" => Color::Blue,
        "sqlite" => Color::Green,
        "sqlsrv" => Color::Magenta,
        _ => Color::DarkGray,
    }
}

impl Tui for Picker {
    type Output = Option<Connection>;

    fn on_key(&mut self, key: Key) -> Flow<Option<Connection>> {
        if self.command.is_some() {
            return self.type_into_command(key);
        }

        let last = self.connections.len().saturating_sub(1);
        match key {
            Key::Char(':') => self.command = Some(String::new()),
            Key::Char('q') | Key::Escape | Key::Ctrl('c') => return Flow::Exit(None),
            Key::Char('n') => {
                self.status = Some("adding connections is not ported yet".to_string())
            }
            Key::Char('e') => {
                self.status = Some("editing connections is not ported yet".to_string())
            }
            Key::Char('d' | 'u') => {
                self.status = Some("deleting connections is not ported yet".to_string())
            }
            Key::Up | Key::Char('k') => self.index = self.index.saturating_sub(1),
            Key::Down | Key::Char('j') => self.index = (self.index + 1).min(last),
            Key::Enter => {
                if let Some(connection) = self.connections.get(self.index) {
                    return Flow::Exit(Some(connection.clone()));
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    fn wants_mouse(&self) -> bool {
        crate::settings::with(|settings| settings.mouse)
    }

    fn on_mouse(&mut self, mouse: Mouse) -> Flow<Option<Connection>> {
        let last = self.connections.len().saturating_sub(1);
        match mouse.button {
            Button::WheelUp => self.index = self.index.saturating_sub(1),
            Button::WheelDown => self.index = (self.index + 1).min(last),
            Button::Left => {
                let row_offset = crate::settings::with(|settings| settings.mouse_row_offset);
                let row = mouse.row as i64 - row_offset;
                let target = self.start as i64 + row - FIRST_BODY_ROW;
                if row >= FIRST_BODY_ROW && (target as usize) < self.connections.len() {
                    self.index = target as usize;
                    return Flow::Exit(Some(self.connections[self.index].clone()));
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < chrome::MIN_COLUMNS || area.height < MIN_LINES {
            let message = [
                "Make the window bigger".to_string(),
                format!("tql needs {}×{MIN_LINES}", chrome::MIN_COLUMNS),
                format!("this is {}×{}", area.width, area.height),
            ];
            for (row, text) in message.iter().enumerate().take(area.height as usize) {
                let text: String = text.chars().take(area.width as usize).collect();
                chrome::line_at(frame, row as u16, Line::raw(text));
            }
            return;
        }

        let inner = area.width.saturating_sub(2) as usize;
        let body_height = (area.height as usize).saturating_sub(7).max(3);
        let rows = self.rows();
        let widths = Self::widths(&rows, inner);
        let edge = theme::border(true, false);
        let boxed = |content: Line<'static>| {
            let padding = inner.saturating_sub(content.width());
            let style = content.style;
            let mut spans = vec![Span::styled("│", edge)];
            spans.extend(
                content
                    .spans
                    .into_iter()
                    .map(|span| Span::styled(span.content, style.patch(span.style))),
            );
            spans.push(Span::styled(" ".repeat(padding), style));
            spans.push(Span::styled("│", edge));
            Line::from(spans)
        };

        let title = " CONNECTIONS ";
        let tail: String = Self::rule(&widths, '┬', inner)
            .chars()
            .skip(width(title) + 1)
            .collect();
        let mut lines = vec![Line::from(vec![
            Span::styled("╭─", edge),
            Span::styled(title, theme::title(true, false)),
            Span::styled(format!("{tail}╮"), edge),
        ])];

        let labels = ["NAME", "TAG", "WHERE", "LAST USED"];
        let mut header = Vec::new();
        for (index, (label, width)) in labels.iter().zip(widths).enumerate() {
            if index > 0 {
                header.push(Span::styled("│", edge));
            }
            header.push(Span::styled(
                format!(" {} ", pad(&truncate(label, width), width)),
                theme::bold(),
            ));
        }
        lines.push(boxed(Line::from(header)));
        lines.push(Line::styled(
            format!("├{}┤", Self::rule(&widths, '┼', inner)),
            edge,
        ));

        let blank: Line<'static> = {
            let mut spans = Vec::new();
            for (index, width) in widths.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled("│", edge));
                }
                spans.push(Span::raw(" ".repeat(width + 2)));
            }
            Line::from(spans)
        };

        let body: Vec<Line<'static>> = if rows.is_empty() {
            vec![Line::styled(
                "  No connections yet — add one with the PHP tql: tql open <path or url>",
                theme::dim(),
            )]
        } else {
            let start = window(self.index, rows.len(), body_height);
            self.start = start;
            rows.iter()
                .enumerate()
                .skip(start)
                .take(body_height)
                .map(|(position, row)| self.row_line(row, position == self.index, &widths))
                .collect()
        };
        for row in 0..body_height {
            lines.push(boxed(
                body.get(row).cloned().unwrap_or_else(|| blank.clone()),
            ));
        }
        lines.push(Line::styled(
            format!("╰{}╯", Self::rule(&widths, '┴', inner)),
            edge,
        ));
        lines.push(Line::styled(BAR, theme::dim()));
        lines.push(self.status_line());

        for (row, line) in lines.into_iter().enumerate() {
            chrome::line_at(frame, row as u16, line);
        }
    }
}
