use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Table {
    pub schema: Option<String>,
    pub name: String,
}

impl Table {
    pub fn bare(name: impl Into<String>) -> Table {
        Table {
            schema: None,
            name: name.into(),
        }
    }

    pub fn in_schema(schema: impl Into<String>, name: impl Into<String>) -> Table {
        Table {
            schema: Some(schema.into()),
            name: name.into(),
        }
    }

    pub fn quoted(&self, quote: impl Fn(&str) -> String) -> String {
        match &self.schema {
            Some(schema) => format!("{}.{}", quote(schema), quote(&self.name)),
            None => quote(&self.name),
        }
    }

    pub fn parse(typed: &str) -> Option<Table> {
        let mut parts = split_name(typed.trim());
        match parts.len() {
            1 if !parts[0].is_empty() => Some(Table::bare(parts.remove(0))),
            2 if !parts[0].is_empty() && !parts[1].is_empty() => {
                let name = parts.remove(1);
                Some(Table::in_schema(parts.remove(0), name))
            }
            _ => None,
        }
    }

    pub fn resolve(typed: &str, tables: &[Table], default_schema: Option<&str>) -> Option<Table> {
        let wanted = Table::parse(typed)?;
        let same = |left: &str, right: &str| left.to_lowercase() == right.to_lowercase();
        let schema = wanted
            .schema
            .filter(|schema| default_schema.is_none_or(|default| !same(schema, default)));
        if let Some(found) = tables.iter().find(|table| {
            same(&table.name, &wanted.name)
                && match (&table.schema, &schema) {
                    (None, None) => true,
                    (Some(left), Some(right)) => same(left, right),
                    _ => false,
                }
        }) {
            return Some(found.clone());
        }
        if schema.is_some() {
            return None;
        }
        let mut elsewhere = tables
            .iter()
            .filter(|table| table.schema.is_some() && same(&table.name, &wanted.name));
        match (elsewhere.next(), elsewhere.next()) {
            (Some(only), None) => Some(only.clone()),
            _ => None,
        }
    }
}

impl fmt::Display for Table {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.schema {
            Some(schema) => write!(formatter, "{schema}.{}", self.name),
            None => formatter.write_str(&self.name),
        }
    }
}

impl From<&str> for Table {
    fn from(name: &str) -> Table {
        Table::bare(name)
    }
}

pub fn split_name(typed: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut closing: Option<char> = None;
    for character in typed.chars() {
        let part = parts.last_mut().expect("there is always a part");
        match closing {
            Some(close) if character == close => closing = None,
            Some(_) => part.push(character),
            None => match character {
                '"' => closing = Some('"'),
                '`' => closing = Some('`'),
                '[' => closing = Some(']'),
                '.' => parts.push(String::new()),
                other => part.push(other),
            },
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    #[test]
    fn writes_a_bare_name_plain_and_another_schema_first() {
        assert_eq!(Table::bare("orders").to_string(), "orders");
        assert_eq!(
            Table::in_schema("ev", "evaluation").to_string(),
            "ev.evaluation"
        );
    }

    #[test]
    fn quotes_each_part_on_its_own() {
        assert_eq!(Table::bare("orders").quoted(quote), "\"orders\"");
        assert_eq!(
            Table::in_schema("Ev", "my.table").quoted(quote),
            "\"Ev\".\"my.table\""
        );
    }

    #[test]
    fn reads_a_typed_name_with_or_without_quotes_and_never_splits_inside_them() {
        assert_eq!(Table::parse("orders"), Some(Table::bare("orders")));
        assert_eq!(
            Table::parse("ev.evaluation"),
            Some(Table::in_schema("ev", "evaluation"))
        );
        assert_eq!(
            Table::parse("\"Ev\".\"my.table\""),
            Some(Table::in_schema("Ev", "my.table"))
        );
        assert_eq!(
            Table::parse("[ev].[order lines]"),
            Some(Table::in_schema("ev", "order lines"))
        );
        assert_eq!(
            Table::parse("`shop`.`orders`"),
            Some(Table::in_schema("shop", "orders"))
        );
        assert_eq!(Table::parse(""), None);
        assert_eq!(Table::parse("a.b.c"), None);
    }

    #[test]
    fn resolves_a_typed_name_to_the_table_it_means() {
        let tables = [
            Table::bare("orders"),
            Table::in_schema("ev", "orders"),
            Table::in_schema("ev", "evaluation"),
        ];
        let resolve = |typed| Table::resolve(typed, &tables, Some("public"));

        assert_eq!(resolve("orders"), Some(Table::bare("orders")));
        assert_eq!(resolve("public.orders"), Some(Table::bare("orders")));
        assert_eq!(resolve("ev.orders"), Some(Table::in_schema("ev", "orders")));
        assert_eq!(
            resolve("evaluation"),
            Some(Table::in_schema("ev", "evaluation"))
        );
        assert_eq!(
            resolve("EV.Evaluation"),
            Some(Table::in_schema("ev", "evaluation"))
        );
        assert_eq!(resolve("nowhere"), None);
    }

    #[test]
    fn an_unqualified_name_in_two_other_schemas_is_not_guessed() {
        let tables = [Table::in_schema("a", "log"), Table::in_schema("b", "log")];

        assert_eq!(Table::resolve("log", &tables, Some("public")), None);
    }
}
