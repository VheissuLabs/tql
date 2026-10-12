use std::collections::HashMap;

use crate::database::{
    BackLink, Column, Database, Index, Link, Pivot, Table, is_plumbing, links, primary_key,
    unique_columns,
};

#[derive(Default)]
pub struct Schema {
    columns: HashMap<Table, Vec<Column>>,
    indexes: HashMap<Table, Vec<Index>>,
    links: Option<HashMap<Table, Vec<(String, Link)>>>,
    referenced: HashMap<Table, Vec<BackLink>>,
}

impl Schema {
    pub fn forget(&mut self) {
        *self = Schema::default();
    }

    pub fn columns(&mut self, database: &mut dyn Database, table: &Table) -> Vec<Column> {
        self.columns
            .entry(table.clone())
            .or_insert_with(|| database.columns(table).unwrap_or_default())
            .clone()
    }

    pub fn indexes(&mut self, database: &mut dyn Database, table: &Table) -> Vec<Index> {
        self.indexes
            .entry(table.clone())
            .or_insert_with(|| database.indexes(table).unwrap_or_default())
            .clone()
    }

    pub fn primary_key(&mut self, database: &mut dyn Database, table: &Table) -> Option<String> {
        primary_key(&self.indexes(database, table))
    }

    pub fn links(&mut self, database: &mut dyn Database, table: &Table) -> Vec<(String, Link)> {
        self.learn_links(database);
        self.links
            .as_ref()
            .and_then(|links| links.get(table))
            .cloned()
            .unwrap_or_default()
    }

    pub fn learn_links(&mut self, database: &mut dyn Database) {
        if self.links.is_some() {
            return;
        }
        let keys = database.foreign_keys().unwrap_or_default();
        self.links = Some(
            keys.iter()
                .map(|(table, keys)| (table.clone(), links(keys)))
                .collect(),
        );
    }

    pub fn remembered_link(&self, table: &Table, column: &str) -> Option<&Link> {
        self.links
            .as_ref()?
            .get(table)?
            .iter()
            .find(|(name, _)| name == column)
            .map(|(_, link)| link)
    }

    pub fn link(
        &mut self,
        database: &mut dyn Database,
        table: &Table,
        column: &str,
    ) -> Option<Link> {
        self.links(database, table)
            .into_iter()
            .find(|(name, _)| name == column)
            .map(|(_, link)| link)
    }

    pub fn referenced_by(
        &mut self,
        database: &mut dyn Database,
        tables: &[Table],
        table: &Table,
    ) -> Vec<BackLink> {
        if let Some(found) = self.referenced.get(table) {
            return found.clone();
        }

        let mut found = Vec::new();
        for other in tables.iter().filter(|other| *other != table) {
            for (column, link) in self.links(database, other) {
                if link.table != *table {
                    continue;
                }
                found.push(BackLink {
                    table: other.clone(),
                    unique: unique_columns(&self.indexes(database, other)).contains(&column),
                    column,
                    references: link.column,
                });
            }
        }
        self.referenced.insert(table.clone(), found.clone());
        found
    }

    pub fn pivot(
        &mut self,
        database: &mut dyn Database,
        table: &Table,
        joined_on: &str,
    ) -> Option<Pivot> {
        let keys = self.links(database, table);
        if keys.len() != 2 || !keys.iter().any(|(column, _)| column == joined_on) {
            return None;
        }

        let far = keys
            .iter()
            .find(|(column, _)| column != joined_on)
            .map(|(column, link)| Pivot {
                table: link.table.clone(),
                on: column.clone(),
                references: link.column.clone(),
            })?;

        let carries_data = self.columns(database, table).iter().any(|column| {
            !keys.iter().any(|(key, _)| *key == column.name) && !is_plumbing(&column.name)
        });
        (!carries_data).then_some(far)
    }

    pub fn ids_to_hide(
        &mut self,
        database: &mut dyn Database,
        table: &Table,
        joined_on: &str,
    ) -> Vec<String> {
        let mut hidden = vec![joined_on.to_string()];
        for (column, _) in self.links(database, table) {
            if !hidden.contains(&column) {
                hidden.push(column);
            }
        }
        hidden
    }
}
