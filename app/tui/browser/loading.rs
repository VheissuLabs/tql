use super::*;

impl Browser {
    pub(super) fn default_sort(&self) -> Option<Sort> {
        let column = self.keys.get(&self.current_table()?)?.clone();
        Some(Sort {
            column,
            direction: Direction::Asc,
        })
    }

    pub(super) fn queue_load(&mut self, keep_cursor: bool) {
        self.pending = Some(keep_cursor || self.pending.unwrap_or(false));
    }

    pub(super) fn load(&mut self, keep_cursor: bool) {
        self.pending = None;
        let Some(table) = self.current_table() else {
            return;
        };
        if self.sort.is_none() {
            self.sort = self.default_sort();
        }

        let column = self.headers.get(self.column_index).cloned();
        let row = self.row_index;

        self.schema.learn_links(self.database.as_mut());
        let started = Instant::now();
        let page = match self.database.filtered_rows(
            &table,
            PAGE + 1,
            self.offset,
            self.sort.as_ref(),
            self.filters.as_ref(),
        ) {
            Ok(page) => page,
            Err(error) => {
                self.say(error.to_string());
                self.headers.clear();
                self.rows.clear();
                self.raw.clear();
                return;
            }
        };
        let elapsed = started.elapsed().as_millis();

        let mut rows = page.rows;
        self.has_more = rows.len() > PAGE;
        rows.truncate(PAGE);

        self.last_statement = Some(sql::shown(&page.statement, PAGE));
        self.query.results = false;
        self.query.table = None;
        if self.mode != Mode::Query {
            self.query.editor.set(&sql::shown(&page.statement, PAGE));
        }
        self.headers = page.headers;
        self.rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| text::cell(value.as_deref()))
                    .collect()
            })
            .collect();
        self.raw = rows;
        self.row_index = 0;
        self.column_index = 0;
        self.column_offset = 0;

        if keep_cursor {
            self.column_index = column
                .and_then(|column| self.headers.iter().position(|header| *header == column))
                .unwrap_or(0);
            self.row_index = row.min(self.rows.len().saturating_sub(1));
        }

        let first = if self.rows.is_empty() {
            0
        } else {
            self.offset + 1
        };
        let more = if self.has_more { "+" } else { "" };
        self.say(format!(
            "rows {first}-{}{more} · {elapsed}ms",
            self.offset + self.rows.len()
        ));
    }

    pub(super) fn select_table(&mut self, index: usize) {
        if index == self.table_index {
            return;
        }
        self.table_index = index;
        self.offset = 0;
        self.sort = None;
        self.filters = None;
        self.queue_load(false);
    }

    pub(super) fn reselect(&mut self) {
        let count = self.visible_tables().len();
        if count == 0 {
            return;
        }
        self.table_index = self.table_index.min(count - 1);
        self.offset = 0;
        self.sort = None;
        self.filters = None;
        self.queue_load(false);
    }

    pub(super) fn sort_by_current_column(&mut self) {
        if let Some(column) = self.headers.get(self.column_index).cloned() {
            self.sort_by(column);
        }
    }

    pub(super) fn sort_by(&mut self, column: String) {
        if self.query.results {
            self.sort_query_results(column);
            return;
        }
        self.sort = match self.sort.take() {
            Some(sort) if sort.column == column && sort.direction == Direction::Asc => Some(Sort {
                column,
                direction: Direction::Desc,
            }),
            Some(sort) if sort.column == column => self.default_sort(),
            _ => Some(Sort {
                column,
                direction: Direction::Asc,
            }),
        };

        self.offset = 0;
        self.load(true);
        let said = match &self.sort {
            Some(sort) => format!("sorted by {} {}", sort.column, sort.direction.keyword()),
            None => "sort cleared".to_string(),
        };
        self.say(said);
    }

    pub(super) fn page(&mut self, forward: bool) {
        if forward && !self.has_more || !forward && self.offset == 0 {
            return;
        }
        self.offset = if forward {
            self.offset + PAGE
        } else {
            self.offset.saturating_sub(PAGE)
        };
        self.load(false);
    }

    pub(super) fn reload(&mut self) {
        let table = self.current_table();
        self.schema.forget();
        self.query.forget_schema();
        self.keys = self.database.primary_keys().unwrap_or_default();
        match self.database.tables() {
            Ok(tables) => self.tables = tables,
            Err(error) => {
                self.fail(error.to_string(), Vec::new(), "ERROR");
                return;
            }
        }

        let visible = self.visible_tables();
        self.table_index = table
            .and_then(|table| visible.iter().position(|candidate| **candidate == table))
            .unwrap_or_else(|| self.table_index.min(visible.len().saturating_sub(1)));
        self.load(true);
    }
}
