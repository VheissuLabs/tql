use crate::terminal::Key;
use toml::Value;

use super::{Action, binding};

pub(super) fn spelling(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Integer(number) => Some(number.to_string()),
        Value::Float(number) => Some(number.to_string()),
        Value::Boolean(true) => Some("1".to_string()),
        _ => None,
    }
}

const NAMED: [(&str, Key); 14] = [
    ("tab", Key::Tab),
    ("shift+tab", Key::BackTab),
    ("enter", Key::Enter),
    ("escape", Key::Escape),
    ("esc", Key::Escape),
    ("space", Key::Char(' ')),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("up", Key::Up),
    ("down", Key::Down),
    ("left", Key::Left),
    ("right", Key::Right),
    ("home", Key::Home),
    ("end", Key::End),
];

pub fn parse(name: &str) -> Option<Key> {
    let name = name.trim();
    let lowered = name.to_lowercase();
    if let Some((_, key)) = NAMED.iter().find(|(named, _)| *named == lowered) {
        return Some(*key);
    }
    if let Some(letter) = lowered.strip_prefix("ctrl+").filter(|letter| {
        letter.len() == 1
            && letter
                .chars()
                .all(|character| character.is_ascii_lowercase())
    }) {
        let byte = (letter.as_bytes()[0] - b'a' + 1) as char;
        return single(&byte.to_string());
    }
    let alt = ["alt+", "option+", "meta+"]
        .iter()
        .find_map(|prefix| {
            lowered
                .starts_with(prefix)
                .then(|| name.get(prefix.len()..))
                .flatten()
        })
        .filter(|rest| rest.chars().count() == 1);
    if let Some(rest) = alt {
        return rest.chars().next().map(Key::Alt);
    }
    (name.chars().count() == 1).then(|| single(name)).flatten()
}

fn single(bytes: &str) -> Option<Key> {
    match Key::parse(bytes).as_slice() {
        [key] => Some(*key),
        _ => None,
    }
}

pub fn spell(key: Key) -> String {
    if let Some((name, _)) = NAMED
        .iter()
        .find(|(name, named)| *named == key && *name != "esc")
    {
        return name.to_string();
    }
    match key {
        Key::Ctrl(character) => format!("ctrl+{character}"),
        Key::Alt(character) => format!("alt+{character}"),
        Key::Char(character) => character.to_string(),
        other => other.glyph(),
    }
}

pub fn glyph(key: Key) -> String {
    match key {
        Key::Up => "↑".into(),
        Key::Down => "↓".into(),
        Key::Left => "←".into(),
        Key::Right => "→".into(),
        Key::Enter => "↵".into(),
        Key::BackTab => "⇧tab".into(),
        Key::Backspace => "⌫".into(),
        Key::Alt(character) if cfg!(target_os = "macos") => format!("⌥{character}"),
        Key::Alt(character) => format!("alt+{character}"),
        Key::Tab => "tab".into(),
        Key::Escape => "escape".into(),
        Key::Char(' ') => "space".into(),
        Key::Delete => "delete".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::Ctrl(character) => format!("ctrl+{character}"),
        Key::Char(character) => character.to_string(),
        other => other.glyph(),
    }
}

pub fn keys(actions: &[Action]) -> String {
    let glyphs: Vec<Vec<String>> = actions
        .iter()
        .map(|action| {
            let mut unique: Vec<String> = Vec::new();
            for key in binding(*action).keys {
                let glyph = glyph(key);
                if !unique.contains(&glyph) {
                    unique.push(glyph);
                }
            }
            unique
        })
        .collect();

    let positions = glyphs.iter().map(Vec::len).max().unwrap_or(0);
    (0..positions)
        .map(|position| {
            let parts: Vec<&str> = glyphs
                .iter()
                .filter_map(|glyph| glyph.get(position).map(String::as_str))
                .collect();
            let tight = parts.len() == 4 && parts.iter().all(|part| part.chars().count() == 1);
            let all_alt = parts.len() > 1 && parts.iter().all(|part| part.starts_with("alt+"));
            if tight {
                parts.concat()
            } else if all_alt {
                format!(
                    "alt+{}",
                    parts
                        .iter()
                        .map(|part| &part[4..])
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            } else {
                parts.join(" ")
            }
        })
        .collect::<Vec<_>>()
        .join(" / ")
}
