use crate::terminal::prelude::*;

use super::chrome::theme;
use super::keymap::{self, Action};
use super::text::{pad, truncate};

const GAP: usize = 3;
const MOST: usize = 3;

type Entry = (String, String);

fn sections() -> Vec<(&'static str, Vec<Entry>)> {
    let each = |actions: &[Action]| -> Vec<Entry> {
        actions
            .iter()
            .map(|action| {
                (
                    keymap::keys(&[*action]),
                    keymap::binding(*action).description.to_string(),
                )
            })
            .collect()
    };
    let pair =
        |actions: &[Action], description: &str| (keymap::keys(actions), description.to_string());
    let fixed = |entries: &[(&str, &str)]| -> Vec<Entry> {
        entries
            .iter()
            .map(|(key, description)| (key.to_string(), description.to_string()))
            .collect()
    };

    let mut moving = vec![
        pair(
            &[
                Action::MoveLeft,
                Action::MoveDown,
                Action::MoveUp,
                Action::MoveRight,
            ],
            "move",
        ),
        pair(
            &[Action::NextPane, Action::PreviousPane],
            "next / previous pane",
        ),
        pair(
            &[Action::FocusTables, Action::FocusRows, Action::FocusSql],
            "tables / rows / SQL",
        ),
    ];
    moving.push(pair(
        &[Action::ToggleTables, Action::ToggleSql],
        "toggle tables / SQL",
    ));
    moving.push(pair(
        &[Action::NextPage, Action::PreviousPage],
        "next / previous page",
    ));
    moving.push(pair(&[Action::Narrow, Action::Widen], "narrow / widen"));
    moving.extend(each(&[
        Action::ResetWidth,
        Action::SortColumn,
        Action::Reload,
        Action::Connections,
    ]));

    let mut rows = each(&[
        Action::Activate,
        Action::InspectRow,
        Action::ViewValue,
        Action::EditValue,
        Action::EditRow,
        Action::NewRow,
    ]);
    rows.push(pair(
        &[Action::YankValue, Action::YankRow],
        "yank the value / row",
    ));
    rows.extend(each(&[Action::MarkDelete, Action::ClearMarks]));

    vec![
        ("moving", moving),
        ("rows", rows),
        (
            "finding",
            each(&[
                Action::FilterTables,
                Action::FilterRows,
                Action::Structure,
                Action::FollowLink,
                Action::JumpBack,
                Action::Databases,
                Action::Sql,
                Action::History,
                Action::Ask,
                Action::Palette,
                Action::Help,
                Action::Quit,
            ]),
        ),
        (
            "viewing a value",
            fixed(&[
                ("j k", "move a line"),
                ("g G", "top / bottom"),
                ("12G", "go to line 12"),
                ("V", "select lines"),
                ("y", "yank the selection"),
                ("ctrl+t", "type the time"),
                ("esc", "clear, then close"),
            ]),
        ),
        (
            "mouse",
            fixed(&[
                ("click", "select"),
                ("dbl click", "edit the cell"),
                ("header", "sort by it"),
                ("drag edge", "resize the column"),
                ("wheel", "scroll"),
            ]),
        ),
        (
            "commands",
            fixed(&[
                (":export", "table to .sql"),
                (":sql", "SQL editor"),
                (":tables", "focus the tables"),
                (":rows", "focus the rows"),
                (":reload", "reload"),
                (":w", "write the changes"),
                (":c", "connections"),
                (":q", "quit"),
            ]),
        ),
        (
            "from the shell",
            fixed(&[
                ("tql export", "table to .sql"),
                ("tql config", "check it, --tidy"),
            ]),
        ),
    ]
}

fn key_width(sections: &[(&str, Vec<Entry>)]) -> usize {
    sections
        .iter()
        .flat_map(|(_, entries)| entries.iter().map(|(key, _)| key.chars().count()))
        .max()
        .unwrap_or(0)
        .min(12)
}

fn column_width(sections: &[(&str, Vec<Entry>)]) -> usize {
    let widest = sections
        .iter()
        .flat_map(|(_, entries)| {
            entries
                .iter()
                .map(|(_, description)| description.chars().count())
        })
        .max()
        .unwrap_or(0);
    1 + key_width(sections) + 2 + widest
}

fn columns_for(available: usize, column: usize) -> usize {
    ((available + GAP) / (column + GAP)).clamp(1, MOST)
}

pub fn width_for(available: usize) -> usize {
    let sections = sections();
    let column = column_width(&sections);
    let columns = columns_for(available.saturating_sub(2), column);
    available.min(columns * column + (columns - 1) * GAP + 2)
}

pub fn lines(inner_width: usize) -> Vec<Line<'static>> {
    let sections = sections();
    let keys = key_width(&sections);
    let column = inner_width.min(column_width(&sections));

    let blocks: Vec<Vec<Line<'static>>> = sections
        .iter()
        .map(|(heading, entries)| {
            let mut block = vec![Line::from(vec![
                Span::raw(" "),
                Span::styled(truncate(heading, column.saturating_sub(1)), theme::bold()),
            ])];
            for (key, description) in entries {
                block.push(Line::from(vec![
                    Span::raw(format!(" {}  ", pad(&truncate(key, keys), keys))),
                    Span::styled(
                        truncate(description, column.saturating_sub(keys + 3).max(4)),
                        theme::dim(),
                    ),
                ]));
            }
            block.push(Line::raw(""));
            block
        })
        .collect();

    let stacks = split(&blocks, columns_for(inner_width, column_width(&sections)));
    let columns: Vec<Vec<Line<'static>>> = stacks
        .into_iter()
        .map(|stack| stack.into_iter().flatten().collect())
        .collect();

    let tallest = columns.iter().map(Vec::len).max().unwrap_or(0);
    let mut lines: Vec<Line<'static>> = (0..tallest)
        .map(|row| {
            let mut spans = Vec::new();
            for (index, lines) in columns.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw(" ".repeat(GAP)));
                }
                let cell = lines.get(row).cloned().unwrap_or_default();
                let used = cell.width();
                spans.extend(cell.spans);
                spans.push(Span::raw(" ".repeat(column.saturating_sub(used))));
            }
            Line::from(spans)
        })
        .collect();

    while lines
        .last()
        .is_some_and(|line| line.to_string().trim().is_empty())
    {
        lines.pop();
    }
    lines
}

pub fn content(
    inner_width: usize,
    inner_height: usize,
    offset: usize,
) -> (Vec<Line<'static>>, usize) {
    let lines = lines(inner_width);
    let hidden = lines.len().saturating_sub(inner_height);
    let offset = offset.min(hidden);
    let mut visible: Vec<Line<'static>> =
        lines.into_iter().skip(offset).take(inner_height).collect();

    if hidden > 0 && inner_height > 0 {
        let hint = if offset < hidden {
            "j / ↓ for more"
        } else {
            "g returns to the top"
        };
        visible.resize(inner_height, Line::raw(""));
        visible[inner_height - 1] =
            Line::from(vec![Span::raw(" "), Span::styled(hint, theme::dim())]);
    }
    (visible, hidden)
}

fn split(blocks: &[Vec<Line<'static>>], columns: usize) -> Vec<Vec<Vec<Line<'static>>>> {
    let heights: Vec<usize> = blocks.iter().map(Vec::len).collect();
    let mut best: Vec<Vec<usize>> = vec![(0..blocks.len()).collect()];
    let mut tallest: usize = heights.iter().sum();

    for cuts in cuts(blocks.len(), columns.min(blocks.len()).saturating_sub(1), 1) {
        let edges: Vec<usize> = std::iter::once(0)
            .chain(cuts)
            .chain(std::iter::once(blocks.len()))
            .collect();
        let stacks: Vec<Vec<usize>> = edges
            .windows(2)
            .map(|edge| (edge[0]..edge[1]).collect())
            .collect();
        let height = stacks
            .iter()
            .map(|stack| stack.iter().map(|index| heights[*index]).sum::<usize>())
            .max()
            .unwrap_or(0);
        if height < tallest {
            best = stacks;
            tallest = height;
        }
    }

    best.into_iter()
        .map(|stack| {
            stack
                .into_iter()
                .map(|index| blocks[index].clone())
                .collect()
        })
        .collect()
}

fn cuts(count: usize, needed: usize, from: usize) -> Vec<Vec<usize>> {
    if needed == 0 {
        return vec![Vec::new()];
    }
    let mut options = Vec::new();
    for at in from..=count.saturating_sub(needed) {
        for rest in cuts(count, needed - 1, at + 1) {
            options.push(std::iter::once(at).chain(rest).collect());
        }
    }
    options
}

pub fn natural_height(inner_width: usize) -> usize {
    lines(inner_width).len()
}
