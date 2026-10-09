use unicode_width::UnicodeWidthStr;

const MIN_WIDTH: usize = 60;

pub fn gray(text: &str) -> String {
    format!("\x1b[90m{text}\x1b[39m")
}

pub fn dim(text: &str) -> String {
    format!("\x1b[2m{text}\x1b[22m")
}

pub fn cyan(text: &str) -> String {
    format!("\x1b[36m{text}\x1b[39m")
}

pub fn green(text: &str) -> String {
    format!("\x1b[32m{text}\x1b[39m")
}

pub fn visible_width(text: &str) -> usize {
    strip(text).width()
}

pub fn strip(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\x1b' {
            for inside in chars.by_ref() {
                if inside.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        plain.push(character);
    }
    plain
}

pub fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let room = width.saturating_sub(1);
    let mut kept = String::new();
    let mut used = 0;
    for character in text.chars() {
        let wide = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + wide > room {
            break;
        }
        used += wide;
        kept.push(character);
    }
    format!("{kept}…")
}

pub fn min_width(columns: usize) -> usize {
    MIN_WIDTH.min(columns.saturating_sub(6))
}

pub fn boxed(title: &str, body: &str, info: &str, columns: usize) -> Vec<String> {
    let body_lines: Vec<&str> = body.split('\n').collect();
    let width = body_lines
        .iter()
        .copied()
        .chain([title])
        .map(visible_width)
        .max()
        .unwrap_or(0)
        .max(min_width(columns));

    let title_width = visible_width(title);
    let title_label = match title_width {
        0 => String::new(),
        _ => format!(" {title} "),
    };
    let top = "─".repeat(width - title_width + if title_width > 0 { 0 } else { 2 });

    let mut lines = vec![format!(
        "{}{title_label}{}",
        gray(" ┌"),
        gray(&format!("{top}┐"))
    )];
    for line in body_lines {
        let padding = " ".repeat(width.saturating_sub(visible_width(line)));
        lines.push(format!("{} {line}{padding} {}", gray(" │"), gray("│")));
    }
    let info = match info.is_empty() {
        true => String::new(),
        false => truncate(info, width.saturating_sub(1)),
    };
    let bottom = match info.is_empty() {
        true => format!(" └{}┘", "─".repeat(width + 2)),
        false => format!(
            " └{} {info} ┘",
            "─".repeat(width.saturating_sub(visible_width(&info)))
        ),
    };
    lines.push(gray(&bottom));
    lines
}
