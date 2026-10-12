use super::*;
use crate::driver::Driver;

impl Picker {
    pub(super) fn rows(&self, unfolded: bool) -> Vec<Row> {
        let now = now();
        self.entries(unfolded)
            .into_iter()
            .map(|entry| {
                let (index, group) = match entry {
                    Entry::Header {
                        group,
                        count,
                        collapsed,
                    } => {
                        return Row {
                            header: Some(Header {
                                group,
                                count,
                                collapsed,
                            }),
                            grouped: false,
                            name: String::new(),
                            tag: String::new(),
                            tag_color: None,
                            driver: None,
                            place: String::new(),
                            used: String::new(),
                            marked: false,
                        };
                    }
                    Entry::Connection { index, group } => (index, group),
                };
                let connection = &self.connections[index];
                let tag = connection
                    .tag
                    .clone()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                Row {
                    header: None,
                    grouped: !group.is_empty(),
                    name: connection.name.clone(),
                    tag: if connection.read_only {
                        format!("{tag} ro").trim().to_string()
                    } else {
                        tag
                    },
                    tag_color: connection.tag_color().and_then(theme::named),
                    driver: Some(connection.driver),
                    place: connection.describe_from(&self.places.home),
                    used: ago(connection.last_used_at.as_deref(), now),
                    marked: self.is_marked(connection.id),
                }
            })
            .collect()
    }

    pub(super) fn widths(rows: &[Row], inner: usize) -> [usize; 4] {
        let count = |text: &str| text.chars().count();
        let mut name = rows
            .iter()
            .map(|row| match &row.header {
                Some(header) => count(&header_text(header)) - 1,
                None => count(&row.name) + 4 + if row.grouped { 2 } else { 0 },
            })
            .fold(8, usize::max)
            .min(32);
        let connections = rows.iter().filter(|row| row.header.is_none());
        let mut tag = connections
            .clone()
            .map(|row| count(&row.tag))
            .fold(3, usize::max)
            .min(16);
        let mut used = connections
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

    pub(super) fn rule(widths: &[usize; 4], join: char, inner: usize) -> String {
        let body = widths
            .iter()
            .map(|width| "─".repeat(width + 2))
            .collect::<Vec<_>>()
            .join(&join.to_string());
        let used = body.chars().count();
        format!("{body}{}", "─".repeat(inner.saturating_sub(used)))
    }

    pub(super) fn header_line(
        header: &Header,
        selected: bool,
        widths: &[usize; 4],
    ) -> Line<'static> {
        let grid = theme::border(true, false);
        let arrow = if header.collapsed { "▶" } else { "▼" };
        let count = format!("({})", header.count);
        let room = widths[0] + 1;
        let name = truncate(&header.group, room.saturating_sub(width(&count) + 3).max(1));
        let shown = format!(" {arrow} {name} {count}");
        let plain = format!("{} ", pad(&shown, room));
        let rest: Vec<String> = widths[1..]
            .iter()
            .map(|width| " ".repeat(width + 2))
            .collect();

        if selected {
            let room = widths.iter().sum::<usize>() + 3 * widths.len() - 1;
            let text = std::iter::once(plain)
                .chain(rest)
                .collect::<Vec<_>>()
                .join("│");
            return Line::styled(pad(&text, room), theme::selection());
        }

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(arrow, theme::dim()),
            Span::raw(" "),
            Span::styled(name, theme::bold()),
            Span::raw(" "),
            Span::styled(count, theme::dim()),
            Span::raw(" ".repeat(width(&plain).saturating_sub(width(&shown)))),
        ];
        for cell in rest {
            spans.push(Span::styled("│", grid));
            spans.push(Span::raw(cell));
        }
        Line::from(spans)
    }

    pub(super) fn row_line(&self, row: &Row, selected: bool, widths: &[usize; 4]) -> Line<'static> {
        if let Some(header) = &row.header {
            return Self::header_line(header, selected, widths);
        }
        let grid = theme::border(true, false);
        let icon = settings::with(|settings| {
            settings.icon(row.driver.map(Driver::name).unwrap_or_default())
        });
        let marker = match selected && settings::with(|settings| settings.row_style == "marker") {
            true => "▸",
            false => " ",
        };
        let indent = if row.grouped { "  " } else { "" };
        let room = widths[0].saturating_sub(4 + indent.len());
        let label = pad(&truncate(&row.name, room), room);
        let cell = |text: &str, width: usize| format!(" {} ", pad(&truncate(text, width), width));

        if selected || row.marked {
            let text = format!(
                " {marker} {indent}{icon} {label} │{}│{}│{}",
                cell(&row.tag, widths[1]),
                cell(&row.place, widths[2]),
                cell(&row.used, widths[3])
            );
            let room = widths.iter().sum::<usize>() + 3 * widths.len() - 1;
            let style = match row.marked {
                true => {
                    theme::highlight(&settings::with(|settings| settings.color("deleted", "red")))
                }
                false => theme::selection(),
            };
            return Line::styled(pad(&text, room), style);
        }

        let shade = row.tag_color.unwrap_or_else(|| driver_color(row.driver));
        let tag_style = row
            .tag_color
            .map(|color| Style::new().fg(color))
            .unwrap_or_else(theme::dim);
        Line::from(vec![
            Span::raw(format!(" {marker} {indent}")),
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

    pub(super) fn status_line(&self) -> Line<'static> {
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
}

pub(super) fn header_text(header: &Header) -> String {
    format!(" ▼ {} ({})", header.group, header.count)
}

pub(super) fn driver_color(driver: Option<Driver>) -> Color {
    match driver {
        Some(Driver::Mysql) => Color::Yellow,
        Some(Driver::Pgsql) => Color::Blue,
        Some(Driver::Sqlite) => Color::Green,
        Some(Driver::Sqlsrv) => Color::Magenta,
        Some(Driver::Duckdb) => Color::LightYellow,
        Some(Driver::Libsql) => Color::Cyan,
        Some(Driver::Clickhouse) => Color::LightRed,
        Some(Driver::Redis) => Color::Red,
        None => Color::DarkGray,
    }
}
