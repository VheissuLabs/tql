use super::*;
use crate::database::{BackLink, Filter, Joiner};

#[derive(Debug, Clone)]
pub(super) struct Jump {
    table: String,
    filters: Option<Filters>,
    row: usize,
}

pub(super) struct LinkPicker {
    choice: Choice,
    links: Vec<BackLink>,
}

impl Browser {
    pub(super) fn follow_link(&mut self) {
        let column = self.headers.get(self.column_index).cloned();
        let link = match (self.browsed_table(), &column) {
            (Some(table), Some(column)) => self.schema.link(self.database.as_mut(), &table, column),
            _ => None,
        };
        let (Some(link), Some(column)) = (link, column) else {
            self.follow_back();
            return;
        };

        let Some(value) = self.raw_value(self.row_index, &column) else {
            self.say(format!("{column} is empty on this row"));
            return;
        };
        if !self.tables.contains(&link.table) {
            self.say(format!("{} is not in this database", link.table));
            return;
        }

        self.remember_jump();
        self.open_linked(&link.table, Some(is(&link.column, &value)));
        self.say(format!(
            "followed {column} → {}  ·  esc goes back",
            link.table
        ));
    }

    fn follow_back(&mut self) {
        let Some(table) = self.current_table() else {
            self.say("nothing to follow");
            return;
        };
        let tables = self.tables.clone();
        let back = match self.browsed_table() {
            Some(browsed) => self
                .schema
                .referenced_by(self.database.as_mut(), &tables, &browsed),
            None => Vec::new(),
        };

        if back.is_empty() {
            match self.headers.get(self.column_index) {
                Some(column) => {
                    let said =
                        format!("nothing links to {table}, and {column} is not a foreign key");
                    self.say(said);
                }
                None => self.say("nothing to follow"),
            }
            return;
        }

        if let [only] = back.as_slice() {
            self.open_back_link(only.clone());
            return;
        }

        let options = back
            .iter()
            .map(|link| format!("{}.{}", link.table, link.column))
            .collect();
        self.link_picker = Some(LinkPicker {
            choice: Choice::new("REFERENCED BY", options, "", false),
            links: back,
        });
    }

    fn open_back_link(&mut self, link: BackLink) {
        let Some(value) = self.raw_value(self.row_index, &link.references) else {
            self.say(format!("{} is empty on this row", link.references));
            return;
        };
        if !self.tables.contains(&link.table) {
            return;
        }

        self.remember_jump();
        self.open_linked(&link.table, Some(is(&link.column, &value)));
        self.say(format!(
            "followed → {} where {} is {value}  ·  esc goes back",
            link.table, link.column
        ));
    }

    pub(super) fn type_into_link_picker(&mut self, key: Key) {
        let Some(picker) = self.link_picker.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.link_picker = None,
            Key::Up | Key::Ctrl('p') => picker.choice.move_by(false),
            Key::Down | Key::Ctrl('n') => picker.choice.move_by(true),
            Key::Enter => {
                let chosen = picker.choice.selected();
                let link = picker
                    .links
                    .iter()
                    .find(|link| Some(format!("{}.{}", link.table, link.column)) == chosen)
                    .cloned();
                self.link_picker = None;
                if let Some(link) = link {
                    self.open_back_link(link);
                }
            }
            other => picker.choice.type_key(other),
        }
    }

    fn remember_jump(&mut self) {
        self.jumps.push(Jump {
            table: self.current_table().unwrap_or_default(),
            filters: self.filters.clone(),
            row: self.row_index,
        });
    }

    pub(super) fn raw_value(&self, row: usize, column: &str) -> Option<String> {
        let at = self.headers.iter().position(|header| header == column)?;
        self.raw.get(row)?.get(at)?.clone()
    }

    pub(super) fn jump_back(&mut self) {
        let Some(jump) = self.jumps.pop() else {
            self.say("nowhere to go back to");
            return;
        };
        if !self.tables.contains(&jump.table) {
            return;
        }

        self.open_linked(&jump.table, jump.filters);
        self.row_index = jump.row.min(self.rows.len().saturating_sub(1));
        self.say(format!("back in {}", jump.table));
    }

    pub(super) fn escape(&mut self) {
        if !self.jumps.is_empty() {
            self.jump_back();
        } else if self.filters.is_some() {
            self.clear_filters();
        } else {
            self.back_to_table();
        }
    }

    pub(super) fn modal_over_grid(&self) -> bool {
        self.structure.is_some() || self.inspector.is_some() || self.link_picker.is_some()
    }

    pub(super) fn can_go_back(&self) -> bool {
        !self.jumps.is_empty() || self.filters.is_some()
    }

    pub(super) fn browsed_table(&self) -> Option<String> {
        self.current_table().filter(|_| !self.query.results)
    }

    fn open_linked(&mut self, table: &str, filters: Option<Filters>) {
        if !self
            .visible_tables()
            .iter()
            .any(|visible| *visible == table)
        {
            self.filter.clear();
            self.filtering = false;
        }
        let Some(at) = self
            .visible_tables()
            .iter()
            .position(|visible| *visible == table)
        else {
            return;
        };

        self.table_index = at;
        self.filters = filters;
        self.offset = 0;
        self.sort = None;
        self.edits.forget();
        self.focus = Focus::Grid;
        self.load(false);
    }

    pub(super) fn forget_links(&mut self) {
        self.jumps.clear();
        self.raw.clear();
        self.schema.forget();
    }

    pub(super) fn link_hint(&self) -> Vec<Span<'static>> {
        if self.mode != Mode::Browse {
            return Vec::new();
        }
        let link = self
            .browsed_table()
            .zip(self.headers.get(self.column_index))
            .and_then(|(table, column)| self.schema.remembered_link(&table, column).cloned());
        let Some(link) = link else {
            return Vec::new();
        };
        vec![
            Span::raw("  "),
            Span::styled(keymap::key(Action::FollowLink, 0), theme::bold()),
            Span::styled(format!(" → {}", link.table), theme::dim()),
        ]
    }

    pub(super) fn draw_link_picker(&self, frame: &mut Frame, frame_height: u16) {
        let Some(picker) = &self.link_picker else {
            return;
        };
        let area = frame.area();
        let width = area.width.saturating_sub(4).min(choice::WIDTH);
        let height = picker.choice.rows();
        let island = Island {
            title: picker.choice.title.clone(),
            content: picker.choice.content(
                width.saturating_sub(2) as usize,
                height.saturating_sub(2) as usize,
            ),
            ..Island::default()
        };
        Modal::new(area.width, 1, frame_height, width).draw(frame.buffer_mut(), island, height);
    }
}

fn is(column: &str, value: &str) -> Filters {
    Filters::new(vec![Filter::new(column, "is", value)], Joiner::And)
}
