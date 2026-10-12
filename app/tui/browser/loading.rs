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
                self.kinds.clear();
                return;
            }
        };
        let elapsed = started.elapsed().as_millis();

        self.loaded = Some(table.clone());
        let mut rows = page.rows;
        self.has_more = rows.len() > PAGE;
        rows.truncate(PAGE);
        self.kinds = page.kinds;
        self.kinds.truncate(PAGE);

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
        let fetched = rows.len();
        self.count_total(&table, fetched);
        self.raw = rows;
        self.prepend_pending_rows();
        self.row_index = 0;
        self.column_index = 0;
        self.column_offset = 0;

        if keep_cursor {
            self.column_index = column
                .and_then(|column| self.headers.iter().position(|header| *header == column))
                .unwrap_or(0);
            self.row_index = row.min(self.rows.len().saturating_sub(1));
        }

        let first = if fetched == 0 { 0 } else { self.offset + 1 };
        let more = if self.has_more { "+" } else { "" };
        self.say(format!(
            "rows {first}-{}{more} · {elapsed}ms",
            self.offset + fetched
        ));
    }

    pub(super) fn select_table(&mut self, index: usize) {
        if index == self.table_index {
            return;
        }
        self.table_index = index;
        if self.schema_here().is_some() {
            return;
        }
        self.offset = 0;
        self.edits.forget();
        self.sort = None;
        self.filters = None;
        self.queue_load(false);
    }

    pub(super) fn reselect(&mut self) {
        let count = self.sidebar_rows().len();
        if count == 0 {
            return;
        }
        self.table_index = self.table_index.min(count - 1);
        if self.schema_here().is_some() {
            self.table_index = self.first_table_row();
        }
        self.offset = 0;
        self.sort = None;
        if self.current_table() == self.loaded {
            self.edits.deletes.clear();
        } else {
            self.edits.forget();
            self.filters = None;
        }
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
        self.turning = true;
        self.load(false);
    }

    fn count_total(&mut self, table: &Table, fetched: usize) {
        let turning = std::mem::take(&mut self.turning);
        let counted = Some((table.clone(), self.filters.clone()));
        if !self.has_more {
            self.total = Some(Total::Exact(self.offset + fetched));
        } else if !(turning && self.counted == counted && self.total.is_some()) {
            self.total = self.counted_total(table);
        }
        self.counted = counted;
    }

    fn counted_total(&mut self, table: &Table) -> Option<Total> {
        let filtered = self.filters.as_ref().is_some_and(Filters::usable);
        let estimate = if filtered {
            None
        } else {
            self.database.estimated_rows(table)
        };
        match estimate {
            Some(Total::About(rows)) if rows >= ESTIMATED_PAST => Some(Total::About(rows)),
            Some(Total::Exact(rows)) => Some(Total::Exact(rows)),
            _ => self
                .database
                .count_rows(table, self.filters.as_ref())
                .ok()
                .map(Total::Exact),
        }
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

        let count = self.sidebar_rows().len();
        self.table_index = table
            .and_then(|table| self.row_of(&table))
            .unwrap_or_else(|| self.table_index.min(count.saturating_sub(1)));
        self.load(true);
    }
}
