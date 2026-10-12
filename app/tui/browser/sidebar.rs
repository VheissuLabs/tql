use super::*;
use crate::updates::Remembered;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarRow<'a> {
    Schema {
        name: &'a str,
        count: usize,
        folded: bool,
    },
    Table(&'a Table),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds {
    pub folded: Vec<String>,
    pub open: Vec<String>,
}

impl Browser {
    pub fn sidebar_rows(&self) -> Vec<SidebarRow<'_>> {
        let needle = self.filter.to_lowercase();
        let matching = |table: &&Table| table.to_string().to_lowercase().contains(&needle);
        let default = self.default_schema.as_deref().unwrap_or("default");

        let mut schemas: Vec<(&str, Vec<&Table>)> = Vec::new();
        for table in &self.tables {
            let schema = table.schema.as_deref().unwrap_or(default);
            match schemas.iter_mut().find(|(name, _)| *name == schema) {
                Some((_, tables)) => tables.push(table),
                None => schemas.push((schema, vec![table])),
            }
        }
        if schemas.len() <= 1 {
            return self
                .tables
                .iter()
                .filter(matching)
                .map(SidebarRow::Table)
                .collect();
        }
        schemas.sort_by_key(|(name, _)| (*name != default, name.to_lowercase()));

        let mut rows = Vec::new();
        for (name, tables) in schemas {
            let shown: Vec<&Table> = tables.iter().copied().filter(matching).collect();
            if !needle.is_empty() && shown.is_empty() {
                continue;
            }
            let folded = needle.is_empty() && self.is_folded(name);
            rows.push(SidebarRow::Schema {
                name,
                count: tables.len(),
                folded,
            });
            if !folded {
                rows.extend(shown.into_iter().map(SidebarRow::Table));
            }
        }
        rows
    }

    pub fn shows_schemas(&self) -> bool {
        self.sidebar_rows()
            .iter()
            .any(|row| matches!(row, SidebarRow::Schema { .. }))
    }

    pub fn is_folded(&self, schema: &str) -> bool {
        if self.folds.folded.iter().any(|folded| folded == schema) {
            return true;
        }
        if self.folds.open.iter().any(|open| open == schema) {
            return false;
        }
        Some(schema) != self.default_schema.as_deref()
    }

    pub(super) fn schema_here(&self) -> Option<(String, bool)> {
        match self.sidebar_rows().get(self.table_index)? {
            SidebarRow::Schema { name, folded, .. } => Some((name.to_string(), *folded)),
            SidebarRow::Table(_) => None,
        }
    }

    pub(super) fn schema_of_row(&self) -> Option<String> {
        if !self.shows_schemas() {
            return None;
        }
        let rows = self.sidebar_rows();
        rows[..=self.table_index.min(rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|row| match row {
                SidebarRow::Schema { name, .. } => Some(name.to_string()),
                SidebarRow::Table(_) => None,
            })
    }

    pub(super) fn fold_schema(&mut self, schema: &str, folded: bool) {
        self.folds.folded.retain(|name| name != schema);
        self.folds.open.retain(|name| name != schema);
        match folded {
            true => self.folds.folded.push(schema.to_string()),
            false => self.folds.open.push(schema.to_string()),
        }
        self.table_index = self
            .sidebar_rows()
            .iter()
            .position(|row| matches!(row, SidebarRow::Schema { name, .. } if *name == schema))
            .unwrap_or(0);
        self.remember_folds();
    }

    pub(super) fn row_of(&self, table: &Table) -> Option<usize> {
        self.sidebar_rows()
            .iter()
            .position(|row| matches!(row, SidebarRow::Table(here) if *here == table))
    }

    pub(super) fn reveal(&mut self, table: &Table) -> Option<usize> {
        if self.row_of(table).is_none() && !self.filter.is_empty() {
            self.filter.clear();
            self.filtering = false;
        }
        if self.row_of(table).is_none()
            && let Some(schema) = self.schema_name_of(table)
        {
            self.fold_schema(&schema, false);
        }
        self.row_of(table)
    }

    fn schema_name_of(&self, table: &Table) -> Option<String> {
        table
            .schema
            .clone()
            .or_else(|| self.default_schema.clone())
            .or_else(|| Some("default".to_string()))
    }

    pub(super) fn first_table_row(&self) -> usize {
        self.sidebar_rows()
            .iter()
            .position(|row| matches!(row, SidebarRow::Table(_)))
            .unwrap_or(0)
    }

    fn folds_key(&self) -> String {
        format!(
            "tables.folded.{}.{}",
            self.connection.id,
            self.active_database.clone().unwrap_or_default()
        )
    }

    fn remember_folds(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let saved = serde_json::json!({ "folded": self.folds.folded, "open": self.folds.open });
        Remembered::new(store).write(&self.folds_key(), &saved.to_string());
    }

    pub(super) fn recall_folds(&mut self) {
        self.default_schema = self.database.default_schema();
        self.folds = self
            .store
            .as_ref()
            .and_then(|store| Remembered::new(store).read(&self.folds_key()))
            .and_then(|saved| serde_json::from_str::<serde_json::Value>(&saved).ok())
            .map(|saved| {
                let names = |key: &str| -> Vec<String> {
                    saved[key]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|name| name.as_str().map(String::from))
                        .collect()
                };
                Folds {
                    folded: names("folded"),
                    open: names("open"),
                }
            })
            .unwrap_or_default();
    }
}
