use std::collections::HashMap;

use super::{Database, Page, split_list};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub table: String,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub table: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackLink {
    pub table: String,
    pub column: String,
    pub references: String,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pivot {
    pub table: String,
    pub on: String,
    pub references: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Count {
    pub table: String,
    pub column: String,
    pub value: String,
}

pub(crate) fn keys_by_table(rows: Vec<Vec<Option<String>>>) -> HashMap<String, Vec<ForeignKey>> {
    let mut keys: HashMap<String, Vec<ForeignKey>> = HashMap::new();
    for mut row in rows {
        keys.entry(row[0].take().unwrap_or_default())
            .or_default()
            .push(ForeignKey {
                columns: split_list(row[1].take()),
                table: row[2].take().unwrap_or_default(),
                references: split_list(row[3].take()),
            });
    }
    keys
}

pub fn links(keys: &[ForeignKey]) -> Vec<(String, Link)> {
    keys.iter()
        .filter(|key| key.columns.len() == 1 && key.references.len() == 1)
        .map(|key| {
            (
                key.columns[0].clone(),
                Link {
                    table: key.table.clone(),
                    column: key.references[0].clone(),
                },
            )
        })
        .collect()
}

pub fn primary_key(indexes: &[Index]) -> Option<String> {
    indexes
        .iter()
        .find(|index| index.primary && index.columns.len() == 1)
        .map(|index| index.columns[0].clone())
}

pub fn unique_columns(indexes: &[Index]) -> Vec<String> {
    let mut unique: Vec<String> = Vec::new();
    for index in indexes {
        if index.columns.len() == 1
            && (index.unique || index.primary)
            && !unique.contains(&index.columns[0])
        {
            unique.push(index.columns[0].clone());
        }
    }
    unique
}

pub fn is_plumbing(column: &str) -> bool {
    [
        "id",
        "created_at",
        "updated_at",
        "deleted_at",
        "last_update",
        "last_updated",
    ]
    .contains(&column.to_lowercase().as_str())
}

pub(crate) fn fill(statement: &str, values: &[String], literal: impl Fn(&str) -> String) -> String {
    let mut values = values.iter();
    let mut pieces = statement.split('?');
    let mut filled = pieces.next().unwrap_or_default().to_string();
    for piece in pieces {
        match values.next() {
            Some(value) => filled.push_str(&literal(value)),
            None => filled.push('?'),
        }
        filled.push_str(piece);
    }
    filled
}

pub trait Relations {
    fn related(
        &mut self,
        table: &str,
        column: &str,
        value: &str,
        limit: usize,
    ) -> anyhow::Result<Page>;

    fn through(
        &mut self,
        pivot: &str,
        joined_on: &str,
        far: &Pivot,
        value: &str,
        limit: usize,
    ) -> anyhow::Result<Page>;

    fn counts(&mut self, targets: &[Count]) -> Vec<Option<usize>>;
}

impl<Driver: Database + ?Sized> Relations for Driver {
    fn related(
        &mut self,
        table: &str,
        column: &str,
        value: &str,
        limit: usize,
    ) -> anyhow::Result<Page> {
        let statement = format!(
            "select * from {} where {} = ? limit {} offset 0",
            self.quote(table),
            self.quote(column),
            limit + 1
        );
        self.select(&statement, &[value.to_string()])
    }

    fn through(
        &mut self,
        pivot: &str,
        joined_on: &str,
        far: &Pivot,
        value: &str,
        limit: usize,
    ) -> anyhow::Result<Page> {
        let far_table = self.quote(&far.table);
        let pivot_table = self.quote(pivot);
        let statement = format!(
            "select {far_table}.* from {far_table} join {pivot_table} on {pivot_table}.{} = {far_table}.{} \
             where {pivot_table}.{} = ? limit {} offset 0",
            self.quote(&far.on),
            self.quote(&far.references),
            self.quote(joined_on),
            limit + 1
        );
        self.select(&statement, &[value.to_string()])
    }

    fn counts(&mut self, targets: &[Count]) -> Vec<Option<usize>> {
        if targets.is_empty() {
            return Vec::new();
        }
        let count_of = |driver: &Self, target: &Count| {
            format!(
                "(select count(*) from {} where {} = ?)",
                driver.quote(&target.table),
                driver.quote(&target.column)
            )
        };
        let statement = format!(
            "select {}",
            targets
                .iter()
                .enumerate()
                .map(|(position, target)| format!("{} as count_{position}", count_of(self, target)))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let values: Vec<String> = targets.iter().map(|target| target.value.clone()).collect();
        if let Ok(page) = self.select(&statement, &values) {
            return (0..targets.len())
                .map(|position| Some(first_number(page.clone(), position)))
                .collect();
        }

        targets
            .iter()
            .map(|target| {
                let statement = format!("select {} as total", count_of(self, target));
                self.select(&statement, std::slice::from_ref(&target.value))
                    .ok()
                    .map(|page| first_number(page, 0))
            })
            .collect()
    }
}

fn first_number(page: Page, position: usize) -> usize {
    page.rows
        .into_iter()
        .next()
        .and_then(|mut row| row.get_mut(position)?.take())
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}
