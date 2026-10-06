use std::collections::HashMap;

use zerorust_tui::prelude::*;

use super::chrome::theme;
use super::text::{pad, truncate, width, window};
use crate::database::{Direction, Sort};

pub const GUTTER: usize = 2;
const COMFORTABLE: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Marked,
    Added,
    Edited,
}

impl Tone {
    fn style(self) -> Style {
        let (key, fallback) = match self {
            Tone::Marked => ("deleted", "red"),
            Tone::Added => ("added", "green"),
            Tone::Edited => ("edited", "yellow"),
        };
        theme::highlight(&crate::settings::with(|settings| {
            settings.color(key, fallback)
        }))
    }
}

pub struct Grid<'a> {
    pub headers: &'a [String],
    pub rows: &'a [Vec<String>],
    pub row_index: usize,
    pub column_index: usize,
    pub column_offset: usize,
    pub sort: Option<&'a Sort>,
    pub focused: bool,
    pub overrides: &'a HashMap<String, usize>,
    pub tones: &'a [Option<Tone>],
}

impl Grid<'_> {
    pub fn fit(&self, available: usize) -> (usize, Vec<usize>) {
        let mut all = self.desired_widths(available, false);
        if total(&all) > available {
            all = self.desired_widths(available, true);
        }

        let offset = self.scroll(&all, available);
        let mut widths: Vec<usize> = Vec::new();
        let mut used = 0;

        for desired in &all[offset..] {
            let cost = desired + 2 + usize::from(!widths.is_empty());
            let reached_cursor = !widths.is_empty() && offset + widths.len() > self.column_index;
            if used + cost > available && reached_cursor {
                break;
            }
            widths.push(*desired);
            used += cost;
        }

        let widths = self.grow(offset, widths, available);
        (offset, widths)
    }

    pub fn lines(
        &self,
        inner_width: usize,
        inner_height: usize,
    ) -> (usize, Vec<usize>, Vec<Line<'static>>) {
        if self.headers.is_empty() {
            return (
                0,
                Vec::new(),
                vec![Line::styled("no columns", theme::dim())],
            );
        }

        let (offset, widths) = self.fit(inner_width.saturating_sub(GUTTER));
        let separator = || Span::styled("│", theme::border(self.focused, false));

        let mut header = vec![Span::raw(" ".repeat(GUTTER))];
        for (position, name) in self.headers[offset..offset + widths.len()]
            .iter()
            .enumerate()
        {
            if position > 0 {
                header.push(separator());
            }
            let label = match self.sort {
                Some(sort) if &sort.column == name => {
                    let arrow = match sort.direction {
                        Direction::Asc => "▲",
                        Direction::Desc => "▼",
                    };
                    format!("{name} {arrow}")
                }
                _ => name.clone(),
            };
            let text = format!(
                " {} ",
                pad(&truncate(&label, widths[position]), widths[position])
            );
            let style = if offset + position == self.column_index {
                theme::bold()
            } else {
                theme::dim()
            };
            header.push(Span::styled(text, style));
        }

        let rule_body = format!(
            "{}{}",
            "─".repeat(GUTTER),
            widths
                .iter()
                .map(|width| "─".repeat(width + 2))
                .collect::<Vec<_>>()
                .join("┼")
        );
        let rule = format!(
            "{rule_body}{}",
            "─".repeat(inner_width.saturating_sub(width(&rule_body)))
        );

        let mut lines = vec![
            Line::from(header),
            Line::styled(rule, theme::border(self.focused, false)),
        ];

        let room = inner_height.saturating_sub(2).max(1);
        let start = window(self.row_index, self.rows.len(), room);
        for (position, row) in self.rows.iter().enumerate().skip(start).take(room) {
            lines.push(self.row_line(row, position, offset, &widths, inner_width));
        }

        let blank = widths.iter().enumerate().flat_map(|(position, width)| {
            let cell = Span::raw(" ".repeat(width + 2));
            if position == 0 {
                vec![cell]
            } else {
                vec![separator(), cell]
            }
        });
        let blank: Line = std::iter::once(Span::raw(" ".repeat(GUTTER)))
            .chain(blank)
            .collect();
        while lines.len() < inner_height {
            lines.push(blank.clone());
        }

        (offset, widths, lines)
    }

    fn row_line(
        &self,
        row: &[String],
        absolute: usize,
        offset: usize,
        widths: &[usize],
        inner_width: usize,
    ) -> Line<'static> {
        if let Some(tone) = self.tones.get(absolute).copied().flatten() {
            return self.toned_line(row, absolute, tone, offset, widths, inner_width);
        }

        let selected = absolute == self.row_index;
        let style = crate::settings::with(|settings| settings.row_style.clone());
        let marker = marker(selected);

        let mut spans = vec![Span::raw(marker)];
        for (position, width) in widths.iter().enumerate() {
            if position > 0 {
                spans.push(Span::styled("│", theme::border(self.focused, false)));
            }
            let column = offset + position;
            let value = truncate(
                row.get(column).map(String::as_str).unwrap_or_default(),
                *width,
            );

            if self.focused && selected && column == self.column_index {
                let shown = if value.is_empty() {
                    " ".to_string()
                } else {
                    value.clone()
                };
                spans.push(Span::styled(format!(" {shown} "), theme::cursor()));
                spans.push(Span::raw(
                    " ".repeat(width.saturating_sub(width_of(&shown))),
                ));
            } else {
                spans.push(Span::raw(format!(" {} ", pad(&value, *width))));
            }
        }

        let line = Line::from(spans);
        let padded = |line: Line<'static>, style: Style| {
            let padding = inner_width.saturating_sub(line.width());
            let mut spans = line.spans;
            spans.push(Span::raw(" ".repeat(padding)));
            Line::from(spans).style(style)
        };
        match (selected, style.as_str()) {
            (false, "dim-others") => line.style(theme::dim()),
            (false, _) => line,
            (true, "underline") => padded(line, Style::new().add_modifier(Modifier::UNDERLINED)),
            (true, "inverse") => padded(line, theme::selection()),
            (true, "bold") => line.style(theme::bold()),
            (true, _) => line,
        }
    }

    fn toned_line(
        &self,
        row: &[String],
        absolute: usize,
        tone: Tone,
        offset: usize,
        widths: &[usize],
        inner_width: usize,
    ) -> Line<'static> {
        let selected = absolute == self.row_index;
        let marker = marker(selected);
        let cells: Vec<String> = widths
            .iter()
            .enumerate()
            .map(|(position, width)| {
                let value = row
                    .get(offset + position)
                    .map(String::as_str)
                    .unwrap_or_default();
                format!(" {} ", pad(&truncate(value, *width), *width))
            })
            .collect();
        let cursor_at = self
            .column_index
            .checked_sub(offset)
            .filter(|at| *at < cells.len() && self.focused && selected);

        let Some(at) = cursor_at else {
            return Line::styled(
                pad(&format!("{marker}{}", cells.join("│")), inner_width),
                tone.style(),
            );
        };

        let before = format!(
            "{marker}{}{}",
            cells[..at].join("│"),
            if at > 0 { "│" } else { "" }
        );
        let after = format!(
            "{}{}",
            if at + 1 < cells.len() { "│" } else { "" },
            cells[at + 1..].join("│")
        );
        let used = width(&before) + width(&cells[at]) + width(&after);
        Line::from(vec![
            Span::styled(before, tone.style()),
            Span::styled(cells[at].clone(), theme::cursor()),
            Span::styled(
                format!("{after}{}", " ".repeat(inner_width.saturating_sub(used))),
                tone.style(),
            ),
        ])
    }

    fn natural_width(&self, column: usize) -> usize {
        let name = &self.headers[column];
        let arrow = match self.sort {
            Some(sort) if &sort.column == name => 2,
            _ => 0,
        };
        self.rows
            .iter()
            .map(|row| row.get(column).map(|value| width(value)).unwrap_or(0))
            .fold(width(name) + arrow, usize::max)
    }

    fn desired_widths(&self, available: usize, capped: bool) -> Vec<usize> {
        (0..self.headers.len())
            .map(|column| {
                let width = match self.overrides.get(&self.headers[column]) {
                    Some(width) => *width,
                    None if capped => self.natural_width(column).min(COMFORTABLE),
                    None => self.natural_width(column),
                };
                width.min(available.saturating_sub(3)).max(3)
            })
            .collect()
    }

    fn scroll(&self, all: &[usize], available: usize) -> usize {
        let mut offset = self.column_offset.min(self.column_index);
        while offset < self.column_index {
            let used: usize = all[offset..=self.column_index]
                .iter()
                .map(|width| width + 3)
                .sum();
            if used <= available {
                break;
            }
            offset += 1;
        }
        offset
    }

    fn grow(&self, offset: usize, mut widths: Vec<usize>, available: usize) -> Vec<usize> {
        let leftover = available.saturating_sub(total(&widths));
        if leftover == 0 || widths.is_empty() {
            return widths;
        }

        let mut wanted: Vec<(usize, usize)> = widths
            .iter()
            .enumerate()
            .filter(|(position, _)| {
                !self
                    .overrides
                    .contains_key(&self.headers[offset + position])
            })
            .filter_map(|(position, width)| {
                let deficit = self.natural_width(offset + position).saturating_sub(*width);
                (deficit > 0).then_some((position, deficit))
            })
            .collect();

        let mut leftover = leftover;
        while leftover > 0 && !wanted.is_empty() {
            for (position, deficit) in wanted.iter_mut() {
                if leftover == 0 {
                    break;
                }
                widths[*position] += 1;
                leftover -= 1;
                *deficit -= 1;
            }
            wanted.retain(|(_, deficit)| *deficit > 0);
        }

        let last = offset + widths.len() - 1;
        if !self.overrides.contains_key(&self.headers[last])
            && let Some(width) = widths.last_mut()
        {
            *width += leftover;
        }
        widths
    }
}

fn marker(selected: bool) -> &'static str {
    let style = crate::settings::with(|settings| settings.row_style.clone());
    if selected && style == "marker" {
        " ▸"
    } else {
        "  "
    }
}

pub fn joins(widths: &[usize]) -> Vec<usize> {
    let mut joins = Vec::new();
    let mut x = GUTTER;
    for width in widths.iter().take(widths.len().saturating_sub(1)) {
        x += width + 2;
        joins.push(x);
        x += 1;
    }
    joins
}

pub fn natural_width(headers: &[String], rows: &[Vec<String>], column: usize) -> usize {
    rows.iter()
        .map(|row| row.get(column).map(|value| width(value)).unwrap_or(0))
        .fold(width(&headers[column]), usize::max)
}

fn total(widths: &[usize]) -> usize {
    if widths.is_empty() {
        return 0;
    }
    widths.iter().map(|width| width + 2).sum::<usize>() + widths.len() - 1
}

fn width_of(text: &str) -> usize {
    width(text).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid<'a>(headers: &'a [String], rows: &'a [Vec<String>], column_index: usize) -> Grid<'a> {
        static NO_OVERRIDES: std::sync::LazyLock<HashMap<String, usize>> =
            std::sync::LazyLock::new(HashMap::new);
        Grid {
            overrides: &NO_OVERRIDES,
            headers,
            rows,
            row_index: 0,
            column_index,
            column_offset: 0,
            sort: None,
            focused: true,
            tones: &[],
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn columns_that_fit_share_the_spare_room_and_the_last_takes_the_rest() {
        let headers = strings(&["id", "name"]);
        let rows = vec![strings(&["1", "Ada Lovelace"])];

        let (offset, widths) = grid(&headers, &rows, 0).fit(40);

        assert_eq!(offset, 0);
        assert_eq!(widths[0], 3);
        assert_eq!(total(&widths), 40);
    }

    #[test]
    fn wide_columns_are_capped_at_a_comfortable_width_when_they_do_not_fit() {
        let headers = strings(&["id", "bio", "notes"]);
        let rows = vec![strings(&["1", &"b".repeat(80), &"n".repeat(80)])];

        let (_, widths) = grid(&headers, &rows, 0).fit(70);

        assert_eq!(widths, [3, 30, 29]);
    }

    #[test]
    fn moving_right_past_the_edge_scrolls_columns() {
        let headers = strings(&["a", "b", "c", "d", "e"]);
        let rows = vec![strings(&[
            &"1".repeat(20),
            &"2".repeat(20),
            &"3".repeat(20),
            &"4".repeat(20),
            &"5".repeat(20),
        ])];

        let (offset, widths) = grid(&headers, &rows, 4).fit(50);

        assert_eq!(offset, 3);
        assert_eq!(widths.len(), 2);
    }
}
