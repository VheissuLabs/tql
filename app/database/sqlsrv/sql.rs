use super::*;

static ORDERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\border\s+by\b").expect("the order by pattern is valid"));

pub fn page(statement: &str, limit: usize, offset: usize) -> String {
    let ordered = if ORDERED.is_match(statement) {
        statement.to_string()
    } else {
        format!("{statement} order by (select null)")
    };
    format!("{ordered} offset {offset} rows fetch next {limit} rows only")
}

pub fn quote_bracket(identifier: &str) -> String {
    format!("[{}]", identifier.replace(']', "]]"))
}

pub fn numbered_parameters(statement: &str) -> String {
    let mut numbered = String::with_capacity(statement.len());
    let mut closing: Option<char> = None;
    let mut position = 0;
    for character in statement.chars() {
        match closing {
            Some(close) if character == close => closing = None,
            Some(_) => {}
            None => match character {
                '\'' => closing = Some('\''),
                '"' => closing = Some('"'),
                '[' => closing = Some(']'),
                '?' => {
                    position += 1;
                    numbered.push_str(&format!("@P{position}"));
                    continue;
                }
                _ => {}
            },
        }
        numbered.push(character);
    }
    numbered
}

pub(super) fn question(_: usize) -> String {
    "?".to_string()
}
