use std::sync::LazyLock;

use regex::Regex;

static DOLLAR_QUOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\$([A-Za-z_][A-Za-z0-9_]*)?\$").expect("the dollar quote pattern is valid")
});

static ROUTINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)^\s*(create|alter)\s+(or\s+(alter|replace)\s+)?(definer\s*=\s*\S+\s+)?(procedure|proc|function|trigger)\b",
    )
    .expect("the routine pattern is valid")
});

static BATCH_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[ \t]*go[ \t]*$").expect("the GO pattern is valid"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub sql: String,
    pub start: usize,
    pub end: usize,
    pub number: usize,
    pub of: usize,
}

pub fn at(sql: &str, cursor: usize) -> Option<Statement> {
    let statements = split(&sql.chars().collect::<Vec<_>>());
    let of = statements.len();
    let index = statements
        .iter()
        .position(|statement| cursor <= statement.end)
        .unwrap_or(of.checked_sub(1)?);
    Some(Statement {
        number: index + 1,
        of,
        ..statements[index].clone()
    })
}

fn split(characters: &[char]) -> Vec<Statement> {
    let mut statements = Vec::new();
    let mut start = 0;
    let mut at = 0;
    let mut depth = 0usize;

    while at < characters.len() {
        if at == 0 || characters[at - 1] == '\n' {
            let line_end = (at..characters.len())
                .find(|end| characters[*end] == '\n')
                .unwrap_or(characters.len());
            let line: String = characters[at..line_end].iter().collect();
            if BATCH_SEPARATOR.is_match(&line) {
                statements.extend(statement(characters, start, at, None));
                start = line_end;
                at = line_end;
                depth = 0;
                continue;
            }
        }
        let skipped = skip_quoted(characters, at);
        if skipped != at {
            at = skipped;
            continue;
        }
        if characters[at].is_alphabetic() && (at == 0 || !is_word(characters[at - 1])) {
            let end = (at..characters.len())
                .find(|end| !is_word(characters[*end]))
                .unwrap_or(characters.len());
            let word: String = characters[at..end]
                .iter()
                .collect::<String>()
                .to_lowercase();
            let next = next_word(characters, end);
            match word.as_str() {
                "begin" if !matches!(next.as_str(), "tran" | "transaction" | "distributed") => {
                    depth += 1
                }
                "case" => depth += 1,
                "end" => depth = depth.saturating_sub(1),
                _ => {}
            }
            at = end;
            continue;
        }
        if characters[at] == ';' {
            let so_far: String = characters[start..at].iter().collect();
            if depth > 0 && ROUTINE.is_match(&so_far) {
                at += 1;
                continue;
            }
            statements.extend(statement(characters, start, at, Some(at + 1)));
            start = at + 1;
            depth = 0;
        }
        at += 1;
    }
    statements.extend(statement(characters, start, characters.len(), None));
    statements
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '@' || character == '#'
}

fn next_word(characters: &[char], from: usize) -> String {
    characters[from..]
        .iter()
        .skip_while(|character| character.is_whitespace())
        .take_while(|character| is_word(**character))
        .collect::<String>()
        .to_lowercase()
}

fn statement(
    characters: &[char],
    start: usize,
    end: usize,
    terminator: Option<usize>,
) -> Option<Statement> {
    let body = &characters[start..end];
    let leading = body
        .iter()
        .take_while(|character| character.is_whitespace())
        .count();
    let trailing = body
        .iter()
        .rev()
        .take_while(|character| character.is_whitespace())
        .count();
    if leading == body.len() {
        return None;
    }
    Some(Statement {
        sql: body[leading..body.len() - trailing].iter().collect(),
        start: start + leading,
        end: terminator.unwrap_or(end - trailing),
        number: 0,
        of: 0,
    })
}

fn skip_quoted(characters: &[char], at: usize) -> usize {
    let next = characters.get(at + 1).copied().unwrap_or_default();
    match characters[at] {
        '-' if next == '-' => skip_until(characters, at + 2, &['\n']),
        '/' if next == '*' => skip_until(characters, at + 2, &['*', '/']),
        quote @ ('\'' | '"' | '`') => skip_string(characters, at, quote),
        '[' => skip_bracketed(characters, at),
        '$' => skip_dollar_quote(characters, at),
        _ => at,
    }
}

fn skip_string(characters: &[char], at: usize, quote: char) -> usize {
    let mut at = at + 1;
    while at < characters.len() {
        if characters[at] == quote && characters.get(at + 1) == Some(&quote) {
            at += 2;
            continue;
        }
        if characters[at] == quote {
            return at + 1;
        }
        at += 1;
    }
    at
}

fn skip_bracketed(characters: &[char], at: usize) -> usize {
    let mut at = at + 1;
    while at < characters.len() {
        if characters[at] == ']' && characters.get(at + 1) == Some(&']') {
            at += 2;
            continue;
        }
        if characters[at] == ']' {
            return at + 1;
        }
        if characters[at] == '\n' {
            return at;
        }
        at += 1;
    }
    at
}

fn skip_dollar_quote(characters: &[char], at: usize) -> usize {
    let ahead: String = characters[at..].iter().take(64).collect();
    let Some(opener) = DOLLAR_QUOTE.find(&ahead) else {
        return at;
    };
    let closer: Vec<char> = opener.as_str().chars().collect();
    skip_until(characters, at + closer.len(), &closer)
}

fn skip_until(characters: &[char], from: usize, closer: &[char]) -> usize {
    (from..characters.len())
        .find(|at| characters[*at..].starts_with(closer))
        .map(|at| at + closer.len())
        .unwrap_or(characters.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement_at(marked: &str) -> (String, usize, usize) {
        let cursor = marked
            .chars()
            .position(|character| character == '|')
            .unwrap();
        let found = at(&marked.replace('|', ""), cursor).unwrap();
        (found.sql, found.number, found.of)
    }

    #[test]
    fn takes_the_statement_the_cursor_is_in() {
        let cases = [
            ("select * |from fruit", "select * from fruit", 1, 1),
            ("sel|ect 1; select 2", "select 1", 1, 2),
            ("select 1; sel|ect 2", "select 2", 2, 2),
            ("select 1|; select 2", "select 1", 1, 2),
            ("select 1;\nselect 2;|", "select 2", 2, 2),
            ("select 1;|\n\nselect 2;", "select 1", 1, 2),
            ("select 1;\n\n|select 2;\n", "select 2", 2, 2),
            ("select 1;\n|\n\nselect 2", "select 2", 2, 2),
        ];
        for (marked, sql, number, of) in cases {
            assert_eq!(
                statement_at(marked),
                (sql.to_string(), number, of),
                "{marked}"
            );
        }
    }

    #[test]
    fn does_not_split_on_a_semicolon_that_is_quoted_or_commented() {
        let cases = [
            ("select ';' as x; select 2", "select ';' as x"),
            ("select 'it''s;' as x; select 2", "select 'it''s;' as x"),
            ("select \";\" from t; select 2", "select \";\" from t"),
            (
                "select 1 -- one; two\n, 2; select 3",
                "select 1 -- one; two\n, 2",
            ),
            ("select /* a;\nb */ 1; select 2", "select /* a;\nb */ 1"),
            (
                "create function f() returns int as $$ begin; return 1; end $$ language plpgsql; select 2",
                "create function f() returns int as $$ begin; return 1; end $$ language plpgsql",
            ),
            (
                "select $tag$ a; b $tag$; select 2",
                "select $tag$ a; b $tag$",
            ),
        ];
        for (sql, first) in cases {
            assert_eq!(at(sql, 0).unwrap().sql, first);
        }
    }

    #[test]
    fn reads_t_sql_brackets_batches_and_routine_bodies() {
        let cases = [
            ("select [a;b] from t; select 2", "select [a;b] from t"),
            ("select [x]]y;] from t; select 2", "select [x]]y;] from t"),
            ("select a[1] from t; select 2", "select a[1] from t"),
            ("select 1\nGO\nselect 2", "select 1"),
            (
                "create procedure p as begin select 1; select case when 1 = 1 then 2 end; end\ngo\nselect 3",
                "create procedure p as begin select 1; select case when 1 = 1 then 2 end; end",
            ),
            (
                "create trigger t after insert on x begin update y set n = n + 1; end; select 2",
                "create trigger t after insert on x begin update y set n = n + 1; end",
            ),
            (
                "create procedure p as begin begin transaction; commit; end; select 2",
                "create procedure p as begin begin transaction; commit; end",
            ),
            ("begin; select 1; commit", "begin"),
        ];
        for (sql, first) in cases {
            assert_eq!(at(sql, 0).unwrap().sql, first, "{sql}");
        }
        assert_eq!(at("select 1\ngo\nselect 2", 15).unwrap().sql, "select 2");
    }

    #[test]
    fn finds_nothing_to_run_in_a_blank_buffer() {
        assert_eq!(at("  \n ", 1), None);
        assert_eq!(at(";;", 1), None);
    }
}
