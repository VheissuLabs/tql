use std::sync::LazyLock;

use regex::Regex;

use crate::driver::Driver;
use crate::tui::now;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnDefault {
    None,
    Value(String),
    Now,
    Expression(String),
}

static TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(current_timestamp|current_date|current_time|localtimestamp|localtime|getdate|getutcdate|sysdatetime|utc_timestamp)\b|\bnow\s*\(|'now'",
    )
    .expect("the time pattern compiles")
});

static QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^N?'((?:[^']|'')*)'$").expect("the quoted pattern compiles"));

static DOUBLE_QUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)^"((?:[^"]|"")*)"$"#).expect("the double quoted pattern compiles")
});

static NUMERIC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?\s*$")
        .expect("the numeric pattern compiles")
});

static CAST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"::[\w\s"\[\]]+$"#).expect("the cast pattern compiles"));

impl ColumnDefault {
    pub fn read(default: Option<&str>, driver: Driver) -> ColumnDefault {
        let Some(default) = default else {
            return ColumnDefault::None;
        };

        let text = unwrap(default.trim());
        let literal = CAST.replace(&text, "").to_string();

        if text.is_empty() || literal.eq_ignore_ascii_case("null") {
            return ColumnDefault::None;
        }
        if TIME.is_match(&text) {
            return ColumnDefault::Now;
        }
        if let Some(captures) = QUOTED.captures(&literal) {
            return ColumnDefault::Value(captures[1].replace("''", "'"));
        }
        if matches!(driver, Driver::Sqlite | Driver::Libsql)
            && let Some(captures) = DOUBLE_QUOTED.captures(&literal)
        {
            return ColumnDefault::Value(captures[1].replace("\"\"", "\""));
        }
        if NUMERIC.is_match(&literal)
            || ["true", "false"].contains(&literal.to_lowercase().as_str())
        {
            return ColumnDefault::Value(literal);
        }
        if driver == Driver::Mysql && !text.contains('(') {
            return ColumnDefault::Value(default.to_string());
        }
        ColumnDefault::Expression(text)
    }

    pub fn filled(&self) -> Option<String> {
        match self {
            ColumnDefault::Value(value) => Some(value.clone()),
            ColumnDefault::Now => Some(now::LITERAL.to_string()),
            _ => None,
        }
    }
}

fn unwrap(text: &str) -> String {
    let mut text = text.to_string();
    while text.starts_with('(') && closes(&text) == Some(text.chars().count() - 1) {
        let inner: String = text
            .chars()
            .skip(1)
            .take(text.chars().count() - 2)
            .collect();
        text = inner.trim().to_string();
    }
    text
}

fn closes(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (position, character) in text.chars().enumerate() {
        depth += match character {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if depth == 0 {
            return Some(position);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(default: &str, driver: &str) -> ColumnDefault {
        ColumnDefault::read(Some(default), Driver::parse(driver).unwrap())
    }

    #[test]
    fn nothing_and_null_are_no_default() {
        assert_eq!(
            ColumnDefault::read(None, Driver::Sqlite),
            ColumnDefault::None
        );
        assert_eq!(read("NULL", "sqlite"), ColumnDefault::None);
        assert_eq!(
            read("NULL::character varying", "pgsql"),
            ColumnDefault::None
        );
    }

    #[test]
    fn time_defaults_mean_now() {
        assert_eq!(read("current_timestamp", "sqlite"), ColumnDefault::Now);
        assert_eq!(read("CURRENT_TIMESTAMP", "mysql"), ColumnDefault::Now);
        assert_eq!(read("now()", "pgsql"), ColumnDefault::Now);
        assert_eq!(read("(datetime('now'))", "sqlite"), ColumnDefault::Now);
    }

    #[test]
    fn quoted_literals_are_unquoted() {
        assert_eq!(read("'G'", "sqlite"), ColumnDefault::Value("G".into()));
        assert_eq!(
            read("'it''s'::text", "pgsql"),
            ColumnDefault::Value("it's".into())
        );
        assert_eq!(read("\"x\"", "sqlite"), ColumnDefault::Value("x".into()));
        assert_eq!(read("(1)", "sqlite"), ColumnDefault::Value("1".into()));
        assert_eq!(read("false", "pgsql"), ColumnDefault::Value("false".into()));
    }

    #[test]
    fn mysql_gives_plain_defaults_unquoted() {
        assert_eq!(
            read("pending", "mysql"),
            ColumnDefault::Value("pending".into())
        );
    }

    #[test]
    fn anything_else_is_left_to_the_database() {
        assert_eq!(
            read("(lower(hex(randomblob(4))))", "sqlite"),
            ColumnDefault::Expression("lower(hex(randomblob(4)))".into())
        );
        assert_eq!(
            read("nextval('films_id_seq'::regclass)", "pgsql"),
            ColumnDefault::Expression("nextval('films_id_seq'::regclass)".into())
        );
    }
}
