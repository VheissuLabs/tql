use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b|([\[\]{},:])"#,
    )
    .expect("the json token pattern compiles")
});

fn parse(value: &str) -> Option<Value> {
    let trimmed = value.trim();
    if !trimmed.starts_with(['{', '[']) {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

pub fn looks_like_json(value: Option<&str>) -> bool {
    value.and_then(parse).is_some()
}

pub fn pretty(value: &str) -> String {
    let Some(pretty) = parse(value).and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
    else {
        return value.to_string();
    };
    pretty
        .lines()
        .map(|line| {
            let content = line.trim_start_matches(' ');
            format!("{}{content}", " ".repeat((line.len() - content.len()) * 2))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn compact(value: &str) -> String {
    parse(value)
        .map(|parsed| parsed.to_string())
        .unwrap_or_else(|| value.to_string())
}

pub fn tokenise(line: &str) -> Vec<(&'static str, String)> {
    let mut tokens = Vec::new();
    let mut offset = 0;

    for captures in TOKEN.captures_iter(line) {
        let whole = captures.get(0).expect("a match has a whole");
        if whole.start() > offset {
            tokens.push(("plain", line[offset..whole.start()].to_string()));
        }
        let group = |index: usize| {
            captures
                .get(index)
                .map(|found| found.as_str().to_string())
                .filter(|found| !found.is_empty())
        };

        if let Some(string) = group(1) {
            match group(2) {
                Some(colon) => {
                    tokens.push(("key", string));
                    tokens.push(("punctuation", colon));
                }
                None => tokens.push(("string", string)),
            }
        } else if let Some(number) = group(3) {
            tokens.push(("number", number));
        } else if let Some(literal) = group(4) {
            tokens.push(("literal", literal));
        } else if let Some(punctuation) = group(5) {
            tokens.push(("punctuation", punctuation));
        } else {
            tokens.push(("plain", whole.as_str().to_string()));
        }
        offset = whole.end();
    }

    if offset < line.len() {
        tokens.push(("plain", line[offset..].to_string()));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<&'static str> {
        tokenise(line).into_iter().map(|(kind, _)| kind).collect()
    }

    #[test]
    fn recognises_json_objects_and_arrays() {
        for value in [
            "{\"a\":1}",
            "[1,2,3]",
            "  {\"nested\":{\"deep\":true}}  ",
            "[]",
            "{}",
        ] {
            assert!(looks_like_json(Some(value)), "{value}");
        }
    }

    #[test]
    fn does_not_mistake_other_values_for_json() {
        assert!(!looks_like_json(None));
        for value in [
            "",
            "hello",
            "42",
            "true",
            "{not json}",
            "{\"unterminated\": ",
            "SELECT * FROM users",
        ] {
            assert!(!looks_like_json(Some(value)), "{value}");
        }
    }

    #[test]
    fn pretty_prints_across_lines_with_four_space_indents() {
        assert_eq!(
            pretty("{\"a\":1,\"b\":[2,3]}"),
            "{\n    \"a\": 1,\n    \"b\": [\n        2,\n        3\n    ]\n}"
        );
    }

    #[test]
    fn leaves_unparseable_values_alone() {
        assert_eq!(pretty("not json"), "not json");
    }

    #[test]
    fn does_not_escape_slashes_or_unicode() {
        let pretty = pretty("{\"url\":\"https://example.com\",\"name\":\"café\"}");

        assert!(pretty.contains("https://example.com"));
        assert!(pretty.contains("café"));
        assert_eq!(compact("{ \"name\": \"café\" }"), "{\"name\":\"café\"}");
    }

    #[test]
    fn tokenises_keys_separately_from_string_values() {
        let tokens = tokenise("    \"email\": \"karl@example.com\",");

        assert!(tokens.contains(&("key", "\"email\"".to_string())));
        assert!(tokens.contains(&("string", "\"karl@example.com\"".to_string())));
    }

    #[test]
    fn tokenises_numbers_and_literals() {
        assert!(kinds("\"n\": 9.75,").contains(&"number"));
        assert!(kinds("\"n\": -3,").contains(&"number"));
        assert!(kinds("\"n\": 1e4,").contains(&"number"));
        assert!(kinds("\"ok\": true").contains(&"literal"));
        assert!(kinds("\"ok\": null").contains(&"literal"));
    }

    #[test]
    fn a_string_containing_a_colon_is_not_a_key() {
        let kinds = kinds("    \"https://example.com\"");

        assert!(kinds.contains(&"string"));
        assert!(!kinds.contains(&"key"));
    }

    #[test]
    fn never_drops_characters_while_tokenising() {
        for line in [
            "    \"email\": \"karl@example.com\",",
            "{",
            "    \"a\": 1▏,",
            "not json at all",
            "  trailing spaces   ",
            "    \"emoji\": \"🎉 café\",",
            "",
            "truely",
        ] {
            let rebuilt: String = tokenise(line).into_iter().map(|(_, text)| text).collect();
            assert_eq!(rebuilt, line);
        }
    }

    #[test]
    fn words_that_merely_start_with_a_literal_are_not_literals() {
        assert!(!kinds("\"truely\": \"nullable\"").contains(&"literal"));
    }
}
