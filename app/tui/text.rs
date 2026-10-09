use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn truncate(text: &str, room: usize) -> String {
    if width(text) <= room {
        return text.to_string();
    }
    if room == 0 {
        return String::new();
    }

    let mut kept = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > room - 1 {
            break;
        }
        kept.push(character);
        used += character_width;
    }
    kept.push('…');
    kept
}

pub fn pad(text: &str, room: usize) -> String {
    format!("{text}{}", " ".repeat(room.saturating_sub(width(text))))
}

pub fn window(cursor: usize, total: usize, room: usize) -> usize {
    if total <= room {
        return 0;
    }
    cursor.saturating_sub(room / 2).min(total - room)
}

pub fn cell(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "NULL".to_string();
    };

    let compact = match value.trim_start().chars().next() {
        Some('{' | '[') => serde_json::from_str::<serde_json::Value>(value)
            .map(|json| json.to_string())
            .unwrap_or_else(|_| value.to_string()),
        _ => value.to_string(),
    };
    let single_line = compact.split_whitespace().collect::<Vec<_>>().join(" ");

    if single_line.chars().count() > 500 {
        return single_line.chars().take(499).collect::<String>() + "…";
    }
    single_line
}

pub fn pasted(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|character| matches!(character, '\n' | '\t') || !character.is_control())
        .collect()
}

pub fn single_line(text: &str) -> String {
    pasted(text).replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_by_display_width_with_an_ellipsis() {
        assert_eq!(truncate("customers", 6), "custo…");
        assert_eq!(truncate("日本語テキスト", 7), "日本語…");
        assert_eq!(truncate("short", 9), "short");
    }

    #[test]
    fn windows_keep_the_cursor_centred() {
        assert_eq!(window(2, 10, 20), 0);
        assert_eq!(window(50, 100, 10), 45);
        assert_eq!(window(99, 100, 10), 90);
    }

    #[test]
    fn cells_read_like_tqls_row_formatter() {
        assert_eq!(cell(None), "NULL");
        assert_eq!(cell(Some("a\n  b\tc")), "a b c");
        assert_eq!(cell(Some("{\"a\": [1, 2]}")), "{\"a\":[1,2]}");
        assert_eq!(
            cell(Some("{\"tier\": \"gold\", \"tags\": []}")),
            "{\"tier\":\"gold\",\"tags\":[]}"
        );
        assert_eq!(cell(Some(&"x".repeat(600))).chars().count(), 500);
    }
}
