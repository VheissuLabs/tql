use super::*;

impl Browser {
    pub(in crate::tui::browser) fn write_pending(&mut self) {
        if !self.edits.pending() {
            self.say("nothing to write");
            return;
        }
        let Some(table) = self.current_table() else {
            return;
        };
        let key = self.key_column();
        if !self.identifies_rows()
            && (!self.edits.updates.is_empty() || !self.edits.deletes.is_empty())
        {
            return;
        }
        let (adds, edits, deletes) = (
            self.edits.inserts.len(),
            self.edits.updates.len(),
            self.edits.deletes.len(),
        );

        if let Err(failure) = self.write_all(&table, key.as_deref().unwrap_or_default()) {
            self.load(true);
            self.fail(
                failure.error.to_string(),
                vec![failure.note.to_string()],
                failure.title,
            );
            return;
        }

        self.load(true);
        let plural = |count: usize| if count == 1 { "" } else { "s" };
        let mut written = Vec::new();
        if adds > 0 {
            written.push(format!("{adds} row{} added", plural(adds)));
        }
        if edits > 0 {
            written.push(format!("{edits} row{} updated", plural(edits)));
        }
        if deletes > 0 {
            written.push(format!("{deletes} deleted"));
        }
        self.say(format!("wrote {}", written.join(" · ")));
    }

    pub(super) fn write_all(&mut self, table: &Table, key: &str) -> Result<(), Failure> {
        let failed = |title, error, note| Failure { title, error, note };

        let defaults_only = match self.connection.driver {
            Driver::Mysql => "() values ()",
            _ => "default values",
        };

        while let Some(values) = self.edits.inserts.first().cloned() {
            let statement = database::insert_statement(
                |identifier| self.database.quote(identifier),
                question,
                table,
                &values,
                defaults_only,
            );
            let started = Instant::now();
            let written = self.database.insert(table, &values);
            self.remember_write(&statement, started, &written, 1);
            written.map_err(|error| {
                failed(
                    "COULD NOT ADD THE ROW",
                    error,
                    "The row is still here. Fix it and :w again, or u to drop it.",
                )
            })?;
            self.edits.inserts.remove(0);
        }

        if key.is_empty() {
            return self.write_matched(table);
        }

        while let Some((key_value, columns)) = self.edits.updates.first().cloned() {
            for (column, value) in &columns {
                let statement = database::update_statement(
                    |identifier| self.database.quote(identifier),
                    question,
                    table,
                    column,
                    key,
                );
                let started = Instant::now();
                let written = self
                    .database
                    .update(table, column, key, &key_value, value.as_deref())
                    .and_then(|reached| match reached {
                        0 => Err(anyhow::anyhow!(
                            "No row has {key} = {key_value} any more, so nothing was written."
                        )),
                        _ => Ok(()),
                    });
                self.remember_write(&statement, started, &written, 1);
                written.map_err(|error| {
                    failed(
                        "COULD NOT WRITE",
                        error,
                        "The change is still here. Fix it and :w again, or u to drop it.",
                    )
                })?;
            }
            self.edits.updates.remove(0);
        }

        if !self.edits.deletes.is_empty() {
            let deletes = self.edits.deletes.clone();
            let statement = database::delete_statement(
                |identifier| self.database.quote(identifier),
                question,
                table,
                key,
                deletes.len(),
            );
            let started = Instant::now();
            let written = self
                .database
                .delete(table, key, &deletes)
                .and_then(|reached| match reached {
                    0 => Err(anyhow::anyhow!(
                        "None of those rows are there any more, so nothing was deleted."
                    )),
                    _ => Ok(()),
                });
            self.remember_write(&statement, started, &written, deletes.len());
            written.map_err(|error| {
                failed(
                    "COULD NOT DELETE",
                    error,
                    "The rows are still marked. u clears them.",
                )
            })?;
            self.edits.deletes.clear();
        }
        Ok(())
    }

    fn write_matched(&mut self, table: &Table) -> Result<(), Failure> {
        let failed = |title, error, note| Failure { title, error, note };
        let gone = || {
            anyhow::anyhow!(
                "No row matches it any more, or tql could not match its values, so nothing was written."
            )
        };
        while let Some((identity, columns)) = self.edits.updates.first().cloned() {
            let row: Values = serde_json::from_str(&identity).unwrap_or_default();
            let statement = format!(
                "update {} set {} where {}",
                self.quoted_table(table),
                self.placeholders(&columns, ", "),
                self.placeholders(&row, " and ")
            );
            let started = Instant::now();
            let written = self
                .database
                .update_row(table, &row, &columns)
                .and_then(|reached| match reached {
                    0 => Err(gone()),
                    _ => Ok(()),
                });
            self.remember_write(&statement, started, &written, 1);
            written.map_err(|error| {
                failed(
                    "COULD NOT WRITE",
                    error,
                    "The change is still here. Fix it and :w again, or u to drop it.",
                )
            })?;
            self.edits.updates.remove(0);
        }

        while let Some(identity) = self.edits.deletes.first().cloned() {
            let row: Values = serde_json::from_str(&identity).unwrap_or_default();
            let statement = format!(
                "delete from {} where {}",
                self.quoted_table(table),
                self.placeholders(&row, " and ")
            );
            let started = Instant::now();
            let written = self
                .database
                .delete_row(table, &row)
                .and_then(|reached| match reached {
                    0 => Err(gone()),
                    _ => Ok(()),
                });
            self.remember_write(&statement, started, &written, 1);
            written.map_err(|error| {
                failed(
                    "COULD NOT DELETE",
                    error,
                    "The rows are still marked. u clears them.",
                )
            })?;
            self.edits.deletes.remove(0);
        }
        Ok(())
    }

    fn quoted_table(&self, table: &Table) -> String {
        table.quoted(|identifier| self.database.quote(identifier))
    }

    fn placeholders(&self, values: &Values, joiner: &str) -> String {
        values
            .iter()
            .map(|(column, _)| format!("{} = ?", self.database.quote(column)))
            .collect::<Vec<_>>()
            .join(joiner)
    }

    pub(super) fn remember_write(
        &self,
        statement: &str,
        started: Instant,
        written: &anyhow::Result<()>,
        rows: usize,
    ) {
        let Some(store) = &self.store else {
            return;
        };
        let error = written.as_ref().err().map(|error| format!("{error:#}"));
        history::record(
            store,
            "tui",
            Execution {
                connection_id: self.connection.id,
                statement,
                error: error.as_deref(),
                rows: written.is_ok().then_some(rows),
                duration_ms: started.elapsed().as_millis(),
            },
        );
    }
}
