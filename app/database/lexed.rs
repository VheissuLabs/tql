use std::sync::LazyLock;

use regex::Regex;

static WRITING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(insert|update|delete|merge|into|exec|execute|create|alter|drop|truncate|grant|revoke|deny)\b",
    )
    .expect("the writing pattern is valid")
});

pub(crate) fn writing_word(statement: &str) -> Option<String> {
    WRITING
        .find(&code(statement, false).0)
        .map(|found| found.as_str().to_lowercase())
}

pub(crate) fn code(statement: &str, blank_parentheses: bool) -> (String, bool) {
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
            '(' if blank_parentheses => {
                depth += 1;
                blank(&mut outside, character);
            }
            ')' if blank_parentheses => {
                depth = depth.saturating_sub(1);
                blank(&mut outside, character);
            }
            _ if blank_parentheses && depth > 0 => blank(&mut outside, character),
            _ => outside.push(character),
        }
    }
    (outside, commented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_write_anywhere_in_the_code_but_not_in_strings_names_or_comments() {
        assert_eq!(
            writing_word("select * from t where note = 'update me'"),
            None
        );
        assert_eq!(
            writing_word("select [delete], \"insert\" from t -- drop it"),
            None
        );
        assert_eq!(writing_word("select 1 /* merge */"), None);
        assert_eq!(
            writing_word("with c as (select 1 as x) update t set x = 2 output inserted.x"),
            Some("update".to_string())
        );
        assert_eq!(
            writing_word("select * from (delete from t output deleted.* ) gone"),
            Some("delete".to_string())
        );
        assert_eq!(
            writing_word("select * into copy from t"),
            Some("into".to_string())
        );
        assert_eq!(writing_word("select updated_at from t"), None);
    }
}
