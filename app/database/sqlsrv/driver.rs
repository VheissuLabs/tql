use super::*;
use crate::database::{Total, first_count};

impl SqlServer {
    fn schema_of(&mut self, table: &Table) -> String {
        table
            .schema
            .clone()
            .or_else(|| self.default_schema())
            .unwrap_or_else(|| "dbo".to_string())
    }
}

impl Database for SqlServer {
    fn tables(&mut self) -> anyhow::Result<Vec<Table>> {
        Ok(self
            .query(
                &format!(
                    "select {SCHEMA_OR_DEFAULT}, t.name from sys.tables as t \
                     where t.is_ms_shipped = 0 and t.name <> 'sysdiagrams' \
                     order by case when schema_name(t.schema_id) = schema_name() then 0 else 1 end, \
                     schema_name(t.schema_id), t.name"
                ),
                &[],
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| {
                let name = row[1].take()?;
                Some(Table {
                    schema: row[0].take(),
                    name,
                })
            })
            .collect())
    }

    fn default_schema(&mut self) -> Option<String> {
        if self.default.is_none() {
            self.default = self
                .column("select schema_name()")
                .ok()
                .and_then(|mut found| found.pop());
        }
        self.default.clone()
    }

    fn filtered_rows(
        &mut self,
        table: &Table,
        limit: usize,
        offset: usize,
        sort: Option<&Sort>,
        filters: Option<&Filters>,
    ) -> anyhow::Result<Page> {
        let condition = filters.and_then(|filters| filters.condition(&quote_bracket));
        let statement = |shown: Option<String>| {
            let filter = shown
                .map(|condition| format!(" where {condition}"))
                .unwrap_or_default();
            let order = sort
                .map(|sort| {
                    format!(
                        " order by {} {}",
                        quote_bracket(&sort.column),
                        sort.direction.keyword()
                    )
                })
                .unwrap_or_default();
            page(
                &format!(
                    "select * from {}{filter}{order}",
                    table.quoted(quote_bracket)
                ),
                limit,
                offset,
            )
        };
        let Some(condition) = condition else {
            return self.query(&statement(None), &[]);
        };
        let bindings = condition.bindings();
        let values: Vec<Option<&str>> = bindings.iter().map(|value| Some(value.as_str())).collect();
        let mut rows = self.query(&statement(Some(condition.placeheld())), &values)?;
        rows.statement = statement(Some(condition.readable()));
        Ok(rows)
    }

    fn estimated_rows(&mut self, table: &Table) -> Option<Total> {
        let page = self
            .select(
                "select sum(rows) from sys.partitions \
                 where object_id = object_id(?) and index_id in (0, 1)",
                &[table.quoted(quote_bracket)],
            )
            .ok()?;
        first_count(page).map(Total::About)
    }

    fn primary_keys(&mut self) -> anyhow::Result<HashMap<Table, String>> {
        Ok(single_column_keys(self.key_columns()?))
    }

    fn key_columns(&mut self) -> anyhow::Result<HashMap<Table, Vec<String>>> {
        let pairs = self
            .query(
                &format!(
                    "select {}, t.name, col.name from sys.indexes as idx \
                     join sys.tables as t on idx.object_id = t.object_id \
                     join sys.index_columns as idxcol on idx.object_id = idxcol.object_id and idx.index_id = idxcol.index_id \
                     join sys.columns as col on idxcol.object_id = col.object_id and idxcol.column_id = col.column_id \
                     where idx.is_primary_key = 1 and t.is_ms_shipped = 0 \
                     order by schema_name(t.schema_id), t.name, idxcol.key_ordinal",
                    SCHEMA_OR_DEFAULT
                ),
                &[],
            )?
            .rows
            .into_iter()
            .filter_map(|mut row| {
                let name = row[1].take()?;
                let column = row[2].take()?;
                Some((
                    Table {
                        schema: row[0].take(),
                        name,
                    },
                    column,
                ))
            })
            .collect();
        Ok(keyed(pairs))
    }

    fn select(&mut self, statement: &str, values: &[String]) -> anyhow::Result<Page> {
        let bound: Vec<Option<&str>> = values.iter().map(|value| Some(value.as_str())).collect();
        self.query(statement, &bound)
    }

    fn columns(&mut self, table: &Table) -> anyhow::Result<Vec<Column>> {
        let schema = self.schema_of(table);
        Ok(self
            .select(
                "select col.name, type.name as type_name, \
                 col.max_length as length, col.precision as precision, col.scale as places, \
                 col.is_nullable as nullable, def.definition as [default], \
                 col.is_identity as autoincrement, col.collation_name as collation, \
                 com.definition as [expression], is_persisted as [persisted], \
                 cast(prop.value as nvarchar(max)) as comment \
                 from sys.columns as col \
                 join sys.types as type on col.user_type_id = type.user_type_id \
                 join sys.objects as obj on col.object_id = obj.object_id \
                 join sys.schemas as scm on obj.schema_id = scm.schema_id \
                 left join sys.default_constraints def on col.default_object_id = def.object_id and col.object_id = def.parent_object_id \
                 left join sys.extended_properties as prop on obj.object_id = prop.major_id and col.column_id = prop.minor_id and prop.name = 'MS_Description' \
                 left join sys.computed_columns as com on col.column_id = com.column_id and col.object_id = com.object_id \
                 where obj.type in ('U', 'V') and obj.name = ? and scm.name = ? \
                 order by col.column_id",
                &[table.name.clone(), schema],
            )?
            .rows
            .into_iter()
            .map(|mut row| {
                let type_name = row[1].take().unwrap_or_default();
                let number = |at: usize| row[at].clone().unwrap_or_default();
                let full_type = match type_name.as_str() {
                    "binary" | "varbinary" | "char" | "varchar" | "nchar" | "nvarchar" => {
                        match number(2).as_str() {
                            "-1" => format!("{type_name}(max)"),
                            bytes => match (type_name.starts_with('n'), bytes.parse::<u32>()) {
                                (true, Ok(bytes)) => format!("{type_name}({})", bytes / 2),
                                _ => format!("{type_name}({bytes})"),
                            },
                        }
                    }
                    "decimal" | "numeric" => format!("{type_name}({},{})", number(3), number(4)),
                    "float" => format!("{type_name}({})", number(3)),
                    "datetime2" | "datetimeoffset" | "time" => {
                        format!("{type_name}({})", number(4))
                    }
                    _ => type_name.clone(),
                };
                let expression = row[9].take().filter(|expression| !expression.is_empty());
                Column {
                    name: row[0].take().unwrap_or_default(),
                    type_name,
                    full_type,
                    nullable: flag(&row[5]),
                    default: row[6].take(),
                    auto_increment: flag(&row[7]),
                    collation: row[8].take(),
                    comment: row[11].take(),
                    generation: expression.map(|expression| Generation {
                        kind: Some(
                            match flag(&row[10]) {
                                true => "stored",
                                false => "virtual",
                            }
                            .to_string(),
                        ),
                        expression: Some(expression),
                    }),
                }
            })
            .collect())
    }

    fn indexes(&mut self, table: &Table) -> anyhow::Result<Vec<Index>> {
        let schema = self.schema_of(table);
        Ok(self
            .select(
                "select idx.name as name, string_agg(col.name, ',') within group (order by idxcol.key_ordinal) as columns, \
                 idx.is_unique as [unique], idx.is_primary_key as [primary] \
                 from sys.indexes as idx \
                 join sys.tables as tbl on idx.object_id = tbl.object_id \
                 join sys.schemas as scm on tbl.schema_id = scm.schema_id \
                 join sys.index_columns as idxcol on idx.object_id = idxcol.object_id and idx.index_id = idxcol.index_id \
                 join sys.columns as col on idxcol.object_id = col.object_id and idxcol.column_id = col.column_id \
                 where tbl.name = ? and scm.name = ? \
                 group by idx.name, idx.type_desc, idx.is_unique, idx.is_primary_key",
                &[table.name.clone(), schema],
            )?
            .rows
            .into_iter()
            .map(|mut row| Index {
                name: row[0].take().unwrap_or_default().to_lowercase(),
                columns: split_list(row[1].take()),
                unique: flag(&row[2]),
                primary: flag(&row[3]),
            })
            .collect())
    }

    fn foreign_keys(&mut self) -> anyhow::Result<HashMap<Table, Vec<ForeignKey>>> {
        Ok(keys_by_schema(
            self.query(
                "select case when ls.name = schema_name() then null else ls.name end as [schema], \
                 lt.name as [table], \
                 string_agg(lc.name, ',') within group (order by fkc.constraint_column_id) as columns, \
                 case when fs.name = schema_name() then null else fs.name end as foreign_schema, \
                 ft.name as foreign_table, \
                 string_agg(fc.name, ',') within group (order by fkc.constraint_column_id) as foreign_columns \
                 from sys.foreign_keys as fk \
                 join sys.foreign_key_columns as fkc on fkc.constraint_object_id = fk.object_id \
                 join sys.tables as lt on lt.object_id = fk.parent_object_id \
                 join sys.schemas as ls on lt.schema_id = ls.schema_id \
                 join sys.columns as lc on fkc.parent_object_id = lc.object_id and fkc.parent_column_id = lc.column_id \
                 join sys.tables as ft on ft.object_id = fk.referenced_object_id \
                 join sys.schemas as fs on ft.schema_id = fs.schema_id \
                 join sys.columns as fc on fkc.referenced_object_id = fc.object_id and fkc.referenced_column_id = fc.column_id \
                 group by ls.name, lt.name, fk.name, fs.name, ft.name",
                &[],
            )?
            .rows,
        ))
    }

    fn databases(&mut self) -> anyhow::Result<Vec<String>> {
        self.column("select name from sys.databases order by name")
    }

    fn use_database(&mut self, name: &str) -> anyhow::Result<()> {
        self.client = self
            .reconnect(name)
            .with_context(|| format!("cannot switch to {name}"))?;
        self.connection.database = Some(name.to_string());
        self.default = None;
        Ok(())
    }

    fn run(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let (headers, rows) = self.fetch(statement, &[], text)?;
        if !headers.is_empty() {
            return Ok(Outcome {
                kinds: all_text(&rows),
                ..outcome_or_affected(headers, rows, 0)
            });
        }
        let affected = self
            .column("select @@rowcount")?
            .first()
            .and_then(|count| count.parse().ok())
            .unwrap_or_default();
        Ok(outcome_or_affected(Vec::new(), Vec::new(), affected))
    }

    fn run_reading(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        if let Some(word) = crate::database::lexed::writing_word(statement)
            .or_else(|| crate::database::lexed::tsql_control_word(statement))
        {
            anyhow::bail!(
                "that statement has {word} in it, so tql did not run it; only statements that read run here"
            );
        }
        self.run("begin transaction")?;
        let ran = self.run(statement);
        let _ = self.run("if @@trancount > 0 rollback transaction");
        let outcome = ran?;
        if let Some(changed) = outcome.affected.filter(|changed| *changed > 0) {
            anyhow::bail!(
                "that statement changed {changed} rows, so tql rolled it back; only statements that read run here"
            );
        }
        Ok(outcome)
    }

    fn run_server_wide(&mut self, statement: &str) -> anyhow::Result<Outcome> {
        let mut master = self.reconnect(MASTER)?;
        self.runtime.block_on(async {
            first_result(master.simple_query(statement).await.map_err(said)?, text).await
        })?;
        Ok(Outcome::default())
    }

    fn quote(&self, identifier: &str) -> String {
        quote_bracket(identifier)
    }

    fn column_names(&mut self, table: &Table) -> anyhow::Result<Vec<String>> {
        Ok(self
            .query(
                &format!("select top 0 * from {}", table.quoted(quote_bracket)),
                &[],
            )?
            .headers)
    }

    fn page(&self, statement: &str, limit: usize, offset: usize) -> String {
        page(statement, limit, offset)
    }

    fn scalars(&mut self, statement: &str) -> anyhow::Result<Scalars> {
        let (headers, rows) = self.fetch(statement, &[], scalar)?;
        Ok(Scalars { headers, rows })
    }

    fn highest(&mut self, table: &Table, column: &str) -> anyhow::Result<Option<String>> {
        let page = self.query(
            &format!(
                "select max({}) from {}",
                quote_bracket(column),
                table.quoted(quote_bracket)
            ),
            &[],
        )?;
        Ok(page
            .rows
            .into_iter()
            .next()
            .and_then(|mut row| row.swap_remove(0)))
    }

    fn update(
        &mut self,
        table: &Table,
        column: &str,
        key: &str,
        key_value: &str,
        value: Option<&str>,
    ) -> anyhow::Result<u64> {
        let statement = update_statement(quote_bracket, question, table, column, key);
        let shown = shown_update(quote_bracket, table, column, key, value, key_value);
        self.write(&statement, &[value, Some(key_value)], &shown)
    }

    fn insert(&mut self, table: &Table, values: &Values) -> anyhow::Result<()> {
        let statement = insert_statement(quote_bracket, question, table, values, "default values");
        let bound: Vec<Option<&str>> = values.iter().map(|(_, value)| value.as_deref()).collect();
        let shown = shown_insert(quote_bracket, table, values, "default values");
        self.write(&statement, &bound, &shown).map(|_| ())
    }

    fn delete(&mut self, table: &Table, key: &str, key_values: &[String]) -> anyhow::Result<u64> {
        let statement = delete_statement(quote_bracket, question, table, key, key_values.len());
        let bound: Vec<Option<&str>> = key_values
            .iter()
            .map(|value| Some(value.as_str()))
            .collect();
        let shown = shown_delete(quote_bracket, table, key, key_values);
        self.write(&statement, &bound, &shown)
    }
}
