pub const OPERATORS: [(&str, &str); 12] = [
    ("is", "="),
    ("is not", "!="),
    ("contains", "like"),
    ("starts with", "like"),
    ("ends with", "like"),
    ("is greater than", ">"),
    ("is at least", ">="),
    ("is less than", "<"),
    ("is at most", "<="),
    ("is empty", "is null"),
    ("is not empty", "is not null"),
    ("is one of", "in"),
];

pub fn operator_names() -> Vec<String> {
    OPERATORS.iter().map(|(name, _)| name.to_string()).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub column: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Joiner {
    #[default]
    And,
    Or,
}

impl Joiner {
    pub fn keyword(self) -> &'static str {
        match self {
            Joiner::And => "and",
            Joiner::Or => "or",
        }
    }

    pub fn toggled(self) -> Joiner {
        match self {
            Joiner::And => Joiner::Or,
            Joiner::Or => Joiner::And,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filters {
    pub filters: Vec<Filter>,
    pub joiner: Joiner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Sql(String),
    Value(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Condition {
    pub parts: Vec<Part>,
}

impl Filter {
    pub fn new(column: &str, operator: &str, value: &str) -> Self {
        Filter {
            column: column.to_string(),
            operator: operator.to_string(),
            value: value.to_string(),
        }
    }

    pub fn needs_value(&self) -> bool {
        !matches!(self.operator.as_str(), "is empty" | "is not empty")
    }

    pub fn usable(&self) -> bool {
        !self.column.is_empty() && (!self.needs_value() || !self.value.is_empty())
    }

    pub fn condition(&self, quote: &dyn Fn(&str) -> String) -> Option<Condition> {
        if !self.usable() {
            return None;
        }
        let column = quote(&self.column);
        let compared = |sql: &str, value: String| {
            Some(Condition {
                parts: vec![Part::Sql(format!("{column} {sql} ")), Part::Value(value)],
            })
        };

        match self.operator.as_str() {
            "is empty" => Some(Condition::sql(format!("{column} is null"))),
            "is not empty" => Some(Condition::sql(format!("{column} is not null"))),
            "contains" => compared("like", format!("%{}%", self.value)),
            "starts with" => compared("like", format!("{}%", self.value)),
            "ends with" => compared("like", format!("%{}", self.value)),
            "is one of" => Some(self.one_of(&column)),
            operator => {
                let sql = OPERATORS
                    .iter()
                    .find(|(name, _)| *name == operator)
                    .map(|(_, sql)| *sql)?;
                compared(sql, self.value.clone())
            }
        }
    }

    fn one_of(&self, column: &str) -> Condition {
        let values: Vec<&str> = self
            .value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();
        if values.is_empty() {
            return Condition::sql("1 = 1".to_string());
        }

        let mut parts = vec![Part::Sql(format!("{column} in ("))];
        for (position, value) in values.iter().enumerate() {
            if position > 0 {
                parts.push(Part::Sql(", ".to_string()));
            }
            parts.push(Part::Value(value.to_string()));
        }
        parts.push(Part::Sql(")".to_string()));
        Condition { parts }
    }
}

impl Filters {
    pub fn new(filters: Vec<Filter>, joiner: Joiner) -> Self {
        Filters { filters, joiner }
    }

    pub fn usable(&self) -> bool {
        self.filters.iter().any(Filter::usable)
    }

    pub fn condition(&self, quote: &dyn Fn(&str) -> String) -> Option<Condition> {
        let separator = format!(" {} ", self.joiner.keyword());
        let mut parts = Vec::new();
        for condition in self
            .filters
            .iter()
            .filter_map(|filter| filter.condition(quote))
        {
            if !parts.is_empty() {
                parts.push(Part::Sql(separator.clone()));
            }
            parts.extend(condition.parts);
        }
        (!parts.is_empty()).then_some(Condition { parts })
    }
}

impl Condition {
    fn sql(sql: String) -> Self {
        Condition {
            parts: vec![Part::Sql(sql)],
        }
    }

    pub fn render(&self, value: impl Fn(&str) -> String) -> String {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Sql(sql) => sql.clone(),
                Part::Value(bound) => value(bound),
            })
            .collect()
    }

    pub fn placeheld(&self) -> String {
        self.render(|_| "?".to_string())
    }

    pub fn readable(&self) -> String {
        self.render(readable_value)
    }

    pub fn bindings(&self) -> Vec<String> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                Part::Value(bound) => Some(bound.clone()),
                Part::Sql(_) => None,
            })
            .collect()
    }
}

pub fn readable_value(value: &str) -> String {
    if is_numeric(value) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "''"))
}

fn is_numeric(value: &str) -> bool {
    let unsigned = value
        .trim()
        .strip_prefix(['+', '-'])
        .unwrap_or(value.trim());
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |text: &str| text.chars().all(|character| character.is_ascii_digit());
    let mantissa_ok =
        (!whole.is_empty() || !fraction.is_empty()) && digits(whole) && digits(fraction);
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    !value.trim().is_empty() && mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::quote_double;

    fn built(filter: Filter) -> (String, Vec<String>) {
        let condition = filter.condition(&quote_double).unwrap();
        (condition.placeheld(), condition.bindings())
    }

    #[test]
    fn builds_each_operator() {
        for (operator, value, clause, binding) in [
            ("is", "karl", "\"name\" = ?", "karl"),
            ("is not", "karl", "\"name\" != ?", "karl"),
            ("contains", "ar", "\"name\" like ?", "%ar%"),
            ("starts with", "ka", "\"name\" like ?", "ka%"),
            ("ends with", "rl", "\"name\" like ?", "%rl"),
            ("is greater than", "5", "\"name\" > ?", "5"),
            ("is at least", "5", "\"name\" >= ?", "5"),
            ("is less than", "5", "\"name\" < ?", "5"),
            ("is at most", "5", "\"name\" <= ?", "5"),
        ] {
            assert_eq!(
                built(Filter::new("name", operator, value)),
                (clause.to_string(), vec![binding.to_string()]),
                "{operator}"
            );
        }
    }

    #[test]
    fn needs_no_value_for_empty_checks() {
        let empty = Filter::new("name", "is empty", "");
        assert!(!empty.needs_value());
        assert!(empty.usable());
        assert_eq!(built(empty), ("\"name\" is null".to_string(), vec![]));
        assert_eq!(
            built(Filter::new("name", "is not empty", "")),
            ("\"name\" is not null".to_string(), vec![])
        );
    }

    #[test]
    fn splits_a_list_for_is_one_of() {
        assert_eq!(
            built(Filter::new("id", "is one of", "1, 2,3 ")),
            (
                "\"id\" in (?, ?, ?)".to_string(),
                vec!["1".to_string(), "2".to_string(), "3".to_string()]
            )
        );
        assert_eq!(
            built(Filter::new("id", "is one of", " , ")),
            ("1 = 1".to_string(), vec![])
        );
    }

    #[test]
    fn binds_the_value_rather_than_putting_it_in_the_sql() {
        assert_eq!(
            built(Filter::new("name", "is", "o'brien; drop table users")),
            (
                "\"name\" = ?".to_string(),
                vec!["o'brien; drop table users".to_string()]
            )
        );
    }

    #[test]
    fn is_not_usable_without_a_value_or_a_column() {
        assert!(!Filter::new("name", "is", "").usable());
        assert!(!Filter::new("", "is", "karl").usable());
        assert!(Filter::new("name", "is", "karl").usable());
    }

    #[test]
    fn joins_conditions_with_and_or_or() {
        let and = Filters::new(
            vec![
                Filter::new("name", "contains", "ar"),
                Filter::new("age", "is at least", "18"),
            ],
            Joiner::And,
        )
        .condition(&quote_double)
        .unwrap();
        assert_eq!(and.placeheld(), "\"name\" like ? and \"age\" >= ?");
        assert_eq!(and.bindings(), ["%ar%", "18"]);

        let or = Filters::new(
            vec![
                Filter::new("name", "is", "a"),
                Filter::new("name", "is", "b"),
            ],
            Joiner::Or,
        )
        .condition(&quote_double)
        .unwrap();
        assert_eq!(or.placeheld(), "\"name\" = ? or \"name\" = ?");
    }

    #[test]
    fn skips_conditions_that_are_not_finished() {
        let condition = Filters::new(
            vec![
                Filter::new("name", "is", ""),
                Filter::new("age", "is", "18"),
            ],
            Joiner::And,
        )
        .condition(&quote_double)
        .unwrap();
        assert_eq!(condition.placeheld(), "\"age\" = ?");
        assert_eq!(condition.bindings(), ["18"]);
    }

    #[test]
    fn is_nothing_at_all_when_no_condition_is_usable() {
        let unfinished = Filters::new(vec![Filter::new("name", "is", "")], Joiner::And);
        assert!(!unfinished.usable());
        assert_eq!(unfinished.condition(&quote_double), None);
        assert_eq!(Filters::default().condition(&quote_double), None);
    }

    #[test]
    fn reads_with_numbers_bare_and_text_quoted() {
        let condition = Filters::new(
            vec![
                Filter::new("name", "is", "O'Brien ?"),
                Filter::new("id", "is one of", "7, 1.5e3"),
            ],
            Joiner::And,
        )
        .condition(&quote_double)
        .unwrap();
        assert_eq!(
            condition.readable(),
            "\"name\" = 'O''Brien ?' and \"id\" in (7, 1.5e3)"
        );
    }

    #[test]
    fn treats_only_whole_numbers_as_numeric() {
        for numeric in ["5", "-5", "+1.5", ".5", "5.", "1e3", "2E-4", " 42 "] {
            assert!(is_numeric(numeric), "{numeric}");
        }
        for text in ["", ".", "e3", "1e", "1-2", "12abc", "0x1A", "1 2"] {
            assert!(!is_numeric(text), "{text}");
        }
    }
}
