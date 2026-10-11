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
static ORDERED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r#"(?i)\border\s+by\s+[`"\[]?([A-Za-z0-9_]+)[`"\]]?(?:\s+(asc|desc))?"#)
});

pub fn apply(
    statement: &str,
    sort: Option<&Sort>,
    quote: impl Fn(&str) -> String,
) -> Option<String> {
    let statement = statement.trim().trim_end_matches(';').trim_end();
    if !rewritable(statement) {
        return None;
    }

    let statement = strip(statement);
    let outside = top_level(&statement);
    let clause = match sort {
        Some(sort) => format!(
            "order by {} {}",
            quote(&sort.column),
            sort.direction.keyword()
        ),
        None if OFFSET_ROWS.is_match(&outside) => "order by (select null)".to_string(),
        None => return Some(statement),
    };
    Some(match LIMIT.find(&outside) {
        Some(limit) => format!(
            "{} {clause} {}",
            statement[..limit.start()].trim_end_matches([' ', '\t']),
            statement[limit.start()..].trim_start()
        ),
        None if ends_in_comment(&statement) => format!("{statement}\n{clause}"),
        None => format!("{statement} {clause}"),
    })
}

pub fn of(statement: &str) -> Option<Sort> {
    let start = ORDER_BY.find(&top_level(statement))?.start();
    let captures = ORDERED.captures_at(statement, start)?;
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
    let outside = top_level(statement);
    SELECT.is_match(&outside)
        && !UNION.is_match(&outside)
        && !outside.contains(';')
        && ORDER_BY.find_iter(&outside).count() <= 1
}

fn strip(statement: &str) -> String {
    let outside = top_level(statement);
    let Some(order) = ORDER_BY.find(&outside) else {
        return statement.to_string();
    };
    let rest = LIMIT
        .find_at(&outside, order.end())
        .map(|limit| limit.start())
        .unwrap_or(statement.len());
    let before = statement[..order.start()].trim_end_matches([' ', '\t']);
    let after = statement[rest..].trim_start();
    match after.is_empty() {
        true => before.to_string(),
        false => format!("{before} {after}"),
    }
}

fn top_level(statement: &str) -> String {
    scan(statement).0
}

fn ends_in_comment(statement: &str) -> bool {
    scan(statement).1
}

fn scan(statement: &str) -> (String, bool) {
    let mut outside = String::with_capacity(statement.len());
    let mut commented = false;
    let mut characters = statement.chars().peekable();
    let mut depth = 0usize;
    let blank = |outside: &mut String, character: char| {
        outside.extend(std::iter::repeat_n(' ', character.len_utf8()));
    };
    while let Some(character) = characters.next() {
        let closing = match character {
            '\'' => Some('\''),
            '"' => Some('"'),
            '`' => Some('`'),
            '[' => Some(']'),
            _ => None,
        };
        if let Some(closing) = closing {
            blank(&mut outside, character);
            for inside in characters.by_ref() {
                blank(&mut outside, inside);
                if inside == closing {
                    break;
                }
            }
            continue;
        }
        if character == '-' && characters.peek() == Some(&'-') {
            blank(&mut outside, character);
            commented = true;
            for inside in characters.by_ref() {
                if inside == '\n' {
                    outside.push('\n');
                    commented = false;
                    break;
                }
                blank(&mut outside, inside);
            }
            continue;
        }
        if character == '/' && characters.peek() == Some(&'*') {
            blank(&mut outside, character);
            let mut previous = ' ';
            for inside in characters.by_ref() {
                blank(&mut outside, inside);
                if previous == '*' && inside == '/' {
                    break;
                }
                previous = inside;
            }
            continue;
        }
        match character {
            '(' => {
                depth += 1;
                blank(&mut outside, character);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                blank(&mut outside, character);
            }
            _ if depth > 0 => blank(&mut outside, character),
            _ => outside.push(character),
        }
    }
    (outside, commented)
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
    fn leaves_words_inside_strings_subqueries_and_comments_alone() {
        let by_id = sorted("id", Direction::Asc);
        assert_eq!(
            apply(
                "select * from notes where body = 'no limit here'",
                Some(&by_id),
                quoted
            )
            .as_deref(),
            Some("select * from notes where body = 'no limit here' order by \"id\" asc")
        );
        assert_eq!(
            apply(
                "select * from t where id in (select id from u order by id limit 5)",
                Some(&sorted("name", Direction::Asc)),
                quoted
            )
            .as_deref(),
            Some(
                "select * from t where id in (select id from u order by id limit 5) order by \"name\" asc"
            )
        );
        assert_eq!(
            apply(
                "select * from t where a = 'x  y' -- order by a\n limit 3",
                Some(&by_id),
                quoted
            )
            .as_deref(),
            Some("select * from t where a = 'x  y' -- order by a\n order by \"id\" asc limit 3")
        );
        assert_eq!(
            apply("select * from t -- newest first", Some(&by_id), quoted).as_deref(),
            Some("select * from t -- newest first\norder by \"id\" asc")
        );
        assert_eq!(
            apply("select 'a;b', \"union\" from t", Some(&by_id), quoted).as_deref(),
            Some("select 'a;b', \"union\" from t order by \"id\" asc")
        );
    }

    #[test]
    fn reads_the_sort_only_from_the_statement_itself() {
        assert_eq!(
            of("select * from t where id in (select id from u order by id desc)"),
            None
        );
        assert_eq!(
            of("select * from t order by \"name\" desc"),
            Some(sorted("name", Direction::Desc))
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
