use super::*;
use crate::database::{BackLink, Count, Page, Pivot, Relations};
use crate::tui::document::{self, Fold, Relation, Row, RowDocument, Section};

const MINIMUM_WIDTH: usize = 60;

enum Load {
    Related {
        table: Table,
        column: String,
        value: String,
        limit: usize,
    },
    Through {
        pivot: Table,
        joined_on: String,
        far: Pivot,
        value: String,
        limit: usize,
    },
}

struct Planned {
    table: Table,
    count: Count,
    load: Load,
    limit: usize,
    hide: Vec<String>,
    kind: String,
}

struct Level {
    document: RowDocument,
    line: usize,
    loads: HashMap<String, Load>,
    raw: HashMap<String, Vec<RawRow>>,
}

type RawRow = Vec<(String, Option<String>)>;

pub struct Inspector {
    levels: Vec<Level>,
    anchor: Option<usize>,
}

impl Inspector {
    fn level(&self) -> &Level {
        self.levels.last().expect("an inspector always has a level")
    }

    fn level_mut(&mut self) -> &mut Level {
        self.levels
            .last_mut()
            .expect("an inspector always has a level")
    }

    fn nested(&self) -> bool {
        self.levels.len() > 1
    }

    fn selection(&self) -> Option<(usize, usize)> {
        let line = self.level().line;
        self.anchor
            .map(|anchor| (anchor.min(line), anchor.max(line)))
    }
}

impl Browser {
    pub(super) fn inspect_row(&mut self) {
        if self.raw.is_empty() {
            self.say("nothing to open, this table has no rows");
            return;
        }
        let Some(row) = self.raw_row(self.row_index) else {
            return;
        };
        self.focus = Focus::Grid;
        let level = self.open_level(self.browsed_table(), row);
        self.inspector = Some(Inspector {
            levels: vec![level],
            anchor: None,
        });
    }

    fn raw_row(&self, index: usize) -> Option<RawRow> {
        let values = self.raw.get(index)?;
        Some(
            self.headers
                .iter()
                .cloned()
                .zip(values.iter().cloned())
                .collect(),
        )
    }

    fn open_level(&mut self, table: Option<Table>, row: RawRow) -> Level {
        let types = table
            .as_ref()
            .map(|table| {
                self.schema
                    .columns(self.database.as_mut(), table)
                    .into_iter()
                    .map(|column| (column.name, column.type_name))
                    .collect()
            })
            .unwrap_or_default();
        let (related, loads) = match table {
            Some(table) => self.relations(&table, &row),
            None => (Vec::new(), HashMap::new()),
        };
        Level {
            document: RowDocument::new(readable(&row), types, related),
            line: 1,
            loads,
            raw: HashMap::new(),
        }
    }

    fn relations(&mut self, table: &Table, row: &RawRow) -> (Vec<Relation>, HashMap<String, Load>) {
        let limit = settings::with(|settings| settings.inspect_related);
        if limit < 1 {
            return (Vec::new(), HashMap::new());
        }

        let mut planned: Vec<Planned> = Vec::new();
        for relation in self
            .parents_of(table, row)
            .into_iter()
            .chain(self.children_of(table, row, limit))
        {
            match planned
                .iter_mut()
                .find(|existing| existing.table == relation.table)
            {
                Some(existing) => *existing = relation,
                None => planned.push(relation),
            }
        }

        let counts: Vec<Count> = planned
            .iter()
            .map(|relation| relation.count.clone())
            .collect();
        let totals = self.database.counts(&counts);

        let mut loads = HashMap::new();
        let mut related = Vec::new();
        for (relation, total) in planned.into_iter().zip(totals) {
            if total == Some(0) {
                continue;
            }
            related.push(Relation {
                table: relation.table.to_string(),
                rows: None,
                shown: relation.limit.min(total.unwrap_or(relation.limit)),
                total,
                hide: relation.hide,
                kind: relation.kind,
            });
            loads.insert(relation.table.to_string(), relation.load);
        }
        (related, loads)
    }

    fn parents_of(&mut self, table: &Table, row: &RawRow) -> Vec<Planned> {
        self.schema
            .links(self.database.as_mut(), table)
            .into_iter()
            .filter_map(|(column, link)| {
                let value = value_in(row, &column)?;
                Some(Planned {
                    table: link.table.clone(),
                    count: Count {
                        table: link.table.clone(),
                        column: link.column.clone(),
                        value: value.clone(),
                    },
                    hide: self.schema.ids_to_hide(
                        self.database.as_mut(),
                        &link.table,
                        &link.column,
                    ),
                    load: Load::Related {
                        table: link.table,
                        column: link.column,
                        value,
                        limit: 1,
                    },
                    limit: 1,
                    kind: "belongs to".to_string(),
                })
            })
            .collect()
    }

    fn children_of(&mut self, table: &Table, row: &RawRow, limit: usize) -> Vec<Planned> {
        let tables = self.tables.clone();
        self.schema
            .referenced_by(self.database.as_mut(), &tables, table)
            .into_iter()
            .filter_map(|link| {
                let value = value_in(row, &link.references)?;
                Some(self.child_of(link, value, limit))
            })
            .collect()
    }

    fn child_of(&mut self, link: BackLink, value: String, limit: usize) -> Planned {
        let count = Count {
            table: link.table.clone(),
            column: link.column.clone(),
            value: value.clone(),
        };
        let database = self.database.as_mut();
        if let Some(far) = self.schema.pivot(database, &link.table, &link.column) {
            return Planned {
                table: far.table.clone(),
                count,
                hide: self
                    .schema
                    .ids_to_hide(database, &far.table, &far.references),
                kind: format!("has many through {}", link.table),
                load: Load::Through {
                    pivot: link.table,
                    joined_on: link.column,
                    far,
                    value,
                    limit,
                },
                limit,
            };
        }

        Planned {
            table: link.table.clone(),
            count,
            hide: self.schema.ids_to_hide(database, &link.table, &link.column),
            kind: if link.unique { "has one" } else { "has many" }.to_string(),
            load: Load::Related {
                table: link.table,
                column: link.column,
                value,
                limit,
            },
            limit,
        }
    }

    pub(super) fn load_related(&mut self) -> bool {
        let Some(inspector) = self.inspector.as_mut() else {
            return false;
        };
        let level = inspector.level_mut();
        let Some(table) = level.document.waiting().into_iter().next() else {
            return false;
        };

        let limit = settings::with(|settings| settings.inspect_related);
        let page = match level.loads.get(&table) {
            Some(Load::Related {
                table,
                column,
                value,
                limit,
            }) => self.database.related(table, column, value, *limit),
            Some(Load::Through {
                pivot,
                joined_on,
                far,
                value,
                limit,
            }) => self.database.through(pivot, joined_on, far, value, *limit),
            None => Ok(Page::default()),
        }
        .unwrap_or_default();

        let rows: Vec<RawRow> = page
            .rows
            .into_iter()
            .take(limit)
            .map(|values| page.headers.iter().cloned().zip(values).collect())
            .collect();
        level
            .document
            .fill(&table, rows.iter().map(readable).collect());
        level.raw.insert(table, rows);
        true
    }

    pub(super) fn type_into_inspector(&mut self, key: Key) {
        match key {
            Key::Escape | Key::Char('q') => return self.inspector_back(),
            Key::Char('e') => return self.edit_from_inspector(),
            Key::Char('y') => return self.yank_document(),
            Key::Char('i') => return self.inspect_related_record(),
            Key::Char('V') => return self.toggle_selection(),
            _ => {}
        }

        let Some(inspector) = self.inspector.as_mut() else {
            return;
        };
        let level = inspector.level_mut();
        let stops = stops(level);
        match key {
            Key::Down | Key::Char('j') => move_within_section(level, true),
            Key::Up | Key::Char('k') => move_within_section(level, false),
            Key::Char('g') => level.line = stops.first().copied().unwrap_or(level.line),
            Key::Char('G') => level.line = stops.last().copied().unwrap_or(level.line),
            Key::Enter | Key::Char(' ') => level.document.toggle(level.line),
            Key::Tab | Key::BackTab => switch_section(level),
            _ => {}
        }
        level.line = level
            .line
            .min(level.document.lines().len().saturating_sub(1));
    }

    fn toggle_selection(&mut self) {
        let Some(inspector) = self.inspector.as_mut() else {
            return;
        };
        let line = inspector.level().line;
        inspector.anchor = match inspector.anchor {
            Some(_) => None,
            None => Some(line),
        };
    }

    fn inspector_back(&mut self) {
        let Some(inspector) = self.inspector.as_mut() else {
            return;
        };
        if inspector.anchor.take().is_some() {
            self.say("selection cleared");
            return;
        }
        if inspector.nested() {
            inspector.levels.pop();
            self.hush();
            return;
        }
        self.inspector = None;
        self.hush();
    }

    fn edit_from_inspector(&mut self) {
        let Some(inspector) = &self.inspector else {
            return;
        };
        if inspector.nested() {
            self.say("this record is not in the grid, open its table to edit it");
            return;
        }
        let level = inspector.level();
        if let Some(at) = level
            .document
            .column_at(level.line)
            .and_then(|column| self.headers.iter().position(|header| *header == column))
        {
            self.column_index = at;
        }
        self.inspector = None;
        self.run(Action::EditValue);
    }

    fn inspect_related_record(&mut self) {
        let Some(inspector) = &self.inspector else {
            return;
        };
        let level = inspector.level();
        let line = level.document.lines().get(level.line).cloned();
        let found = line.and_then(|line| {
            let shown = line.table?;
            let row = level.raw.get(&shown)?.get(line.row?)?.clone();
            let table = match level.loads.get(&shown)? {
                Load::Related { table, .. } => table.clone(),
                Load::Through { far, .. } => far.table.clone(),
            };
            Some((table, row))
        });
        let Some((table, row)) = found else {
            self.say("i opens a related record, move onto one first");
            return;
        };

        let level = self.open_level(Some(table.clone()), row);
        if let Some(inspector) = self.inspector.as_mut() {
            inspector.anchor = None;
            inspector.levels.push(level);
        }
        self.say(format!("inspecting {table} · esc goes back"));
    }

    fn yank_document(&mut self) {
        let Some(inspector) = self.inspector.as_mut() else {
            return;
        };
        let lines: Vec<String> = inspector
            .level()
            .document
            .lines()
            .into_iter()
            .map(|line| line.text)
            .collect();
        let (from, to) = inspector
            .selection()
            .unwrap_or((0, lines.len().saturating_sub(1)));
        inspector.anchor = None;

        let count = to - from + 1;
        let reached = self.clipboard.copy(&lines[from..=to].join("\n"));
        let plural = if count == 1 { "" } else { "s" };
        self.say(format!(
            "yanked {count} line{plural} to the {} clipboard",
            reached.name()
        ));
    }

    pub(super) fn inspector_offers(&self, inspector: &Inspector) -> Vec<(String, &'static str)> {
        let level = inspector.level();
        let on_related_record = level
            .document
            .lines()
            .get(level.line)
            .is_some_and(|line| line.row.is_some());

        let mut offers = vec![if on_related_record {
            ("i".to_string(), "Inspect")
        } else {
            ("↵".to_string(), "Fold")
        }];
        if level.document.has_related() {
            offers.push(("tab".into(), "Switch"));
        }
        if !inspector.nested() {
            offers.push(("e".into(), "Edit"));
        }
        offers.push(("y".into(), "Yank"));
        offers.push((
            "esc".into(),
            if inspector.nested() { "Back" } else { "Close" },
        ));
        offers
    }

    pub(super) fn draw_inspector(&mut self, frame: &mut Frame, frame_height: u16) {
        let Some(inspector) = self.inspector.as_mut() else {
            return;
        };
        let width = frame.area().width as usize;
        let selection = inspector.selection();
        let level = inspector.level_mut();

        let box_width =
            MINIMUM_WIDTH.max(width.saturating_sub(6).min(level.document.natural_width()));
        level.document.fit_to(box_width.saturating_sub(8));

        let lines = level.document.lines();
        let cursor_section = lines.get(level.line).map(|line| line.section);
        let sections: Vec<(Section, usize)> = [Section::Record, Section::Related]
            .into_iter()
            .filter_map(|section| Some((section, level.document.heading_at(section)?)))
            .collect();
        let collapsed = |section: Section| {
            level.document.is_folded(match section {
                Section::Record => &Fold::Record,
                Section::Related => &Fold::Related,
            })
        };

        let mut heights: Vec<usize> = sections
            .iter()
            .map(|(section, _)| {
                if collapsed(*section) {
                    1
                } else {
                    (level.document.section(*section).len() + 2).max(3)
                }
            })
            .collect();
        let room = frame_height.saturating_sub(2) as usize;
        if heights
            .iter()
            .map(|height| height + 1)
            .sum::<usize>()
            .saturating_sub(1)
            > room
        {
            shrink(&mut heights, room);
        }

        let inner_width = box_width.saturating_sub(2);
        let boxes = sections
            .iter()
            .zip(heights)
            .map(|((section, heading), height)| {
                let island = Island {
                    title: lines[*heading].text.clone(),
                    focused: cursor_section == Some(*section),
                    title_under_cursor: level.line == *heading,
                    collapsed: collapsed(*section),
                    content: section_lines(
                        &level.document.section(*section),
                        level.line,
                        selection,
                        (inner_width, height.saturating_sub(2)),
                    ),
                    ..Island::default()
                };
                (island, height as u16)
            })
            .collect();
        Modal::new(frame.area().width, 1, frame_height, box_width as u16)
            .draw_stack(frame.buffer_mut(), boxes);
    }
}

pub(super) fn inspector_status() -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled("row", theme::bold()),
        Span::styled(
            "   ↵ folds    j/k moves    e edits    V selects    y yanks    esc closes",
            theme::dim(),
        ),
    ])
}

fn value_in(row: &RawRow, column: &str) -> Option<String> {
    row.iter()
        .find(|(name, _)| name == column)
        .and_then(|(_, value)| value.clone())
}

fn readable(row: &RawRow) -> Row {
    row.iter()
        .map(|(column, value)| (column.clone(), document::shown(value.as_deref())))
        .collect()
}

fn move_within_section(level: &mut Level, forward: bool) {
    let lines = level.document.lines();
    let section = lines
        .get(level.line)
        .map(|line| line.section)
        .unwrap_or(Section::Record);
    let stops: Vec<usize> = level
        .document
        .section(section)
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    if stops.is_empty() {
        return;
    }
    let at = stops
        .iter()
        .position(|stop| *stop == level.line)
        .unwrap_or(0);
    let step = if forward { 1 } else { stops.len() - 1 };
    level.line = stops[(at + step) % stops.len()];
}

fn switch_section(level: &mut Level) {
    let other = match level
        .document
        .lines()
        .get(level.line)
        .map(|line| line.section)
    {
        Some(Section::Record) => Section::Related,
        _ => Section::Record,
    };
    level.line = level
        .document
        .section(other)
        .first()
        .map(|(index, _)| *index)
        .unwrap_or(level.line);
}

fn stops(level: &Level) -> Vec<usize> {
    level
        .document
        .lines()
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.heading)
        .map(|(index, _)| index)
        .collect()
}

fn section_lines(
    members: &[(usize, document::Line)],
    cursor: usize,
    selection: Option<(usize, usize)>,
    (inner_width, inner_height): (usize, usize),
) -> Vec<Line<'static>> {
    let at = members
        .iter()
        .position(|(index, _)| *index == cursor)
        .unwrap_or(0);
    members
        .iter()
        .skip(window(at, members.len(), inner_height))
        .take(inner_height)
        .map(|(index, line)| {
            let fitted = truncate(&line.text, inner_width);
            let selected = selection.is_some_and(|(from, to)| (from..=to).contains(index));
            if selected {
                Line::styled(pad(&fitted, inner_width), theme::selection())
            } else if *index == cursor {
                Line::styled(pad(&fitted, inner_width), theme::cursor())
            } else if line.fold.is_some() {
                Line::styled(fitted, theme::bold())
            } else {
                Line::raw(fitted)
            }
        })
        .collect()
}

fn shrink(heights: &mut [usize], room: usize) {
    let count = heights.len();
    let mut left = room.saturating_sub(count.saturating_sub(1)).max(count * 3);
    let mut remaining = count;
    for height in heights.iter_mut() {
        *height = (*height).min((left / remaining).max(3));
        left = left.saturating_sub(*height);
        remaining -= 1;
    }
}
