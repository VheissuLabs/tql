use std::sync::LazyLock;

use regex::Regex;

use super::{Direction, Sort};

fn pattern(source: &str) -> Regex {
    Regex::new(source).expect("the order by patterns are valid")
}

static SELECT: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)^\s*select\s"));
static UNION: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\bunion\b"));
static ORDER_BY: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\border\s+by\b"));
static LIMIT: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\s(limit|offset)\s"));
static OFFSET_ROWS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\soffset\s+\d+\s+rows?\b"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| pattern(r"\s{2,}"));
static ORDERED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r#"(?i)\border\s+by\s+[`"\[]?([A-Za-z0-9_]+)[`"\]]?(?:\s+(asc|desc))?"#)
});

pub fn apply(
    statement: &str,
    sort: Option<&Sort>,
    quote: impl Fn(&str) -> String,
) -> Option<String> {
    let statement = statement.trim().trim_end_matches(';');
    if !rewritable(statement) {
        return None;
    }

    let statement = strip(statement);
    let clause = match sort {
        Some(sort) => format!(
            "order by {} {}",
            quote(&sort.column),
            sort.direction.keyword()
        ),
        None if OFFSET_ROWS.is_match(&statement) => "order by (select null)".to_string(),
        None => return Some(statement),
    };
    Some(match LIMIT.find(&statement) {
        Some(limit) => format!(
            "{} {clause} {}",
            statement[..limit.start()].trim_end(),
            statement[limit.start()..].trim_start()
        ),
        None => format!("{statement} {clause}"),
    })
}

pub fn of(statement: &str) -> Option<Sort> {
    let captures = ORDERED.captures(statement)?;
    let descending = captures
        .get(2)
        .is_some_and(|direction| direction.as_str().eq_ignore_ascii_case("desc"));
    Some(Sort {
        column: captures[1].to_string(),
        direction: if descending {
            Direction::Desc
        } else {
            Direction::Asc
        },
    })
}

fn rewritable(statement: &str) -> bool {
    SELECT.is_match(statement)
        && !UNION.is_match(statement)
        && !statement.contains(';')
        && ORDER_BY.find_iter(statement).count() <= 1
}

fn strip(statement: &str) -> String {
    let stripped = match ORDER_BY.find(statement) {
        Some(order) => {
            let rest = LIMIT
                .find_at(statement, order.end())
                .map(|limit| limit.start())
                .unwrap_or(statement.len());
            format!("{}{}", &statement[..order.start()], &statement[rest..])
        }
        None => statement.to_string(),
    };
    SPACES.replace_all(&stripped, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quoted(name: &str) -> String {
        format!("\"{name}\"")
    }

    fn sorted(column: &str, direction: Direction) -> Sort {
        Sort {
            column: column.to_string(),
            direction,
        }
    }

    #[test]
    fn appends_or_replaces_the_order_by_ahead_of_a_limit() {
        let by_name = sorted("name", Direction::Asc);
        assert_eq!(
            apply("select * from events", Some(&by_name), quoted).as_deref(),
            Some("select * from events order by \"name\" asc")
        );
        assert_eq!(
            apply(
                "select * from events limit 50 offset 0",
                Some(&sorted("name", Direction::Desc)),
                quoted
            )
            .as_deref(),
            Some("select * from events order by \"name\" desc limit 50 offset 0")
        );
        assert_eq!(
            apply(
                "select * from events order by id desc limit 10",
                Some(&by_name),
                quoted
            )
            .as_deref(),
            Some("select * from events order by \"name\" asc limit 10")
        );
        assert_eq!(
            apply(
                "select * from events where name = 'a';",
                Some(&sorted("id", Direction::Asc)),
                quoted
            )
            .as_deref(),
            Some("select * from events where name = 'a' order by \"id\" asc")
        );
    }

    #[test]
    fn strips_the_clause_when_the_sort_is_cleared() {
        assert_eq!(
            apply(
                "select * from events order by id desc limit 10",
                None,
                quoted
            )
            .as_deref(),
            Some("select * from events limit 10")
        );
    }

    #[test]
    fn keeps_an_order_sql_server_needs_to_offset_when_the_sort_is_cleared() {
        assert_eq!(
            apply(
                "select * from [events] order by [id] desc offset 0 rows fetch next 100 rows only",
                None,
                quoted
            )
            .as_deref(),
            Some(
                "select * from [events] order by (select null) offset 0 rows fetch next 100 rows only"
            )
        );
        assert_eq!(
            apply(
                "select * from [events] order by (select null) offset 0 rows fetch next 100 rows only",
                Some(&sorted("name", Direction::Asc)),
                quoted
            )
            .as_deref(),
            Some("select * from [events] order by \"name\" asc offset 0 rows fetch next 100 rows only")
        );
    }

    #[test]
    fn refuses_anything_it_cannot_rewrite_safely() {
        for statement in [
            "select a from x union select b from y",
            "select * from x; drop table y",
            "update events set name = 1",
            "with t as (select 1) select * from t",
        ] {
            assert_eq!(apply(statement, None, quoted), None, "{statement}");
        }
    }

    #[test]
    fn reads_the_column_a_statement_orders_by() {
        assert_eq!(
            of("select * from events order by \"id\" asc limit 10"),
            Some(sorted("id", Direction::Asc))
        );
        assert_eq!(
            of("select * from events order by ArtistId desc"),
            Some(sorted("ArtistId", Direction::Desc))
        );
        assert_eq!(
            of("select * from `events` order by `name`"),
            Some(sorted("name", Direction::Asc))
        );
        assert_eq!(of("select * from events"), None);
    }
}
