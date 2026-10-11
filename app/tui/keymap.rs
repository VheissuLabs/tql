use crate::terminal::Key;
use toml::Value;

use crate::settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Activate,
    Escape,
    Command,
    NextPane,
    PreviousPane,
    FocusTables,
    FocusRows,
    FocusSql,
    ToggleTables,
    InspectRow,
    ViewValue,
    EditValue,
    EditRow,
    SortColumn,
    MarkDelete,
    ClearMarks,
    NewRow,
    Ask,
    FilterRows,
    FilterTables,
    Structure,
    Databases,
    Sql,
    History,
    FollowLink,
    JumpBack,
    NextPage,
    PreviousPage,
    Reload,
    Connections,
    YankValue,
    YankRow,
    Narrow,
    Widen,
    ResetWidth,
    Palette,
    Help,
    Quit,
    Redraw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub action: Action,
    pub name: &'static str,
    pub keys: Vec<Key>,
    pub description: &'static str,
    pub rebindable: bool,
}

impl Binding {
    pub fn shown(&self) -> String {
        self.keys
            .iter()
            .map(|key| spell(*key))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

fn ported(action: Action, name: &'static str, keys: &[Key], description: &'static str) -> Binding {
    Binding {
        action,
        name,
        keys: keys.to_vec(),
        description,
        rebindable: true,
    }
}

fn fixed(action: Action, name: &'static str, keys: &[Key], description: &'static str) -> Binding {
    Binding {
        rebindable: false,
        ..ported(action, name, keys, description)
    }
}

pub fn defaults() -> Vec<Binding> {
    vec![
        fixed(
            Action::MoveUp,
            "move_up",
            &[Key::Up, Key::Char('k')],
            "move up",
        ),
        fixed(
            Action::MoveDown,
            "move_down",
            &[Key::Down, Key::Char('j')],
            "move down",
        ),
        fixed(
            Action::MoveLeft,
            "move_left",
            &[Key::Left, Key::Char('h')],
            "column left",
        ),
        fixed(
            Action::MoveRight,
            "move_right",
            &[Key::Right, Key::Char('l')],
            "column right",
        ),
        fixed(Action::Activate, "activate", &[Key::Enter], "open, or edit"),
        fixed(Action::Escape, "escape", &[Key::Escape], "back, or close"),
        fixed(
            Action::Command,
            "command",
            &[Key::Char(':')],
            "command line",
        ),
        fixed(Action::NextPane, "next_pane", &[Key::Tab], "next pane"),
        fixed(
            Action::PreviousPane,
            "previous_pane",
            &[Key::BackTab],
            "previous pane",
        ),
        ported(
            Action::FocusTables,
            "focus_tables",
            &[Key::Alt('1')],
            "go to the table list",
        ),
        ported(
            Action::FocusRows,
            "focus_rows",
            &[Key::Alt('2')],
            "go to the rows",
        ),
        ported(
            Action::FocusSql,
            "focus_sql",
            &[Key::Alt('3')],
            "go to the SQL editor",
        ),
        ported(
            Action::ToggleTables,
            "toggle_tables",
            &[Key::Char('\\')],
            "show or hide tables",
        ),
        ported(
            Action::InspectRow,
            "inspect_row",
            &[Key::Char('i')],
            "inspect the row",
        ),
        ported(
            Action::ViewValue,
            "view_value",
            &[Key::Char('I')],
            "view the value",
        ),
        ported(
            Action::EditValue,
            "edit_value",
            &[Key::Char('e')],
            "edit the value",
        ),
        ported(
            Action::EditRow,
            "edit_row",
            &[Key::Char('E')],
            "edit the row",
        ),
        ported(
            Action::SortColumn,
            "sort_column",
            &[Key::Char('o')],
            "sort: asc, desc, key",
        ),
        ported(
            Action::MarkDelete,
            "mark_delete",
            &[Key::Char('d')],
            "mark for deletion",
        ),
        ported(
            Action::ClearMarks,
            "clear_marks",
            &[Key::Char('u')],
            "drop changes",
        ),
        ported(Action::NewRow, "new_row", &[Key::Char('N')], "add a row"),
        ported(Action::Ask, "ask", &[Key::Char('a')], "ask in english"),
        ported(
            Action::FilterRows,
            "filter_rows",
            &[Key::Char('f')],
            "filter rows",
        ),
        ported(
            Action::FilterTables,
            "filter_tables",
            &[Key::Char('/')],
            "filter tables",
        ),
        ported(
            Action::Structure,
            "structure",
            &[Key::Char('t')],
            "structure",
        ),
        ported(
            Action::Databases,
            "databases",
            &[Key::Char('b')],
            "switch database",
        ),
        ported(Action::Sql, "sql", &[Key::Char('s')], "SQL editor"),
        ported(
            Action::History,
            "history",
            &[Key::Char('H'), Key::Alt('h')],
            "query history",
        ),
        ported(
            Action::FollowLink,
            "follow_link",
            &[Key::Char('L')],
            "follow a link",
        ),
        ported(Action::JumpBack, "jump_back", &[Key::Ctrl('o')], "go back"),
        ported(
            Action::NextPage,
            "next_page",
            &[Key::Char('n')],
            "next page",
        ),
        ported(
            Action::PreviousPage,
            "previous_page",
            &[Key::Char('p')],
            "previous page",
        ),
        ported(Action::Reload, "reload", &[Key::Char('r')], "reload"),
        ported(
            Action::Connections,
            "connections",
            &[Key::Char('c')],
            "connections",
        ),
        ported(
            Action::YankValue,
            "yank_value",
            &[Key::Char('y')],
            "yank the value",
        ),
        ported(
            Action::YankRow,
            "yank_row",
            &[Key::Char('Y')],
            "yank the row",
        ),
        ported(
            Action::Narrow,
            "narrow",
            &[Key::Char(','), Key::Char('<')],
            "narrow column or list",
        ),
        ported(
            Action::Widen,
            "widen",
            &[Key::Char('.'), Key::Char('>')],
            "widen column or list",
        ),
        ported(
            Action::ResetWidth,
            "reset_width",
            &[Key::Char('=')],
            "reset width",
        ),
        ported(
            Action::Palette,
            "palette",
            &[Key::Ctrl('k')],
            "command palette",
        ),
        ported(Action::Help, "help", &[Key::Char('?')], "help"),
        ported(Action::Quit, "quit", &[Key::Char('q')], "quit"),
        fixed(Action::Redraw, "redraw", &[Key::Ctrl('l')], "redraw"),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap {
            bindings: defaults(),
        }
    }
}

impl Keymap {
    pub fn rebind(&mut self, name: &str, value: &Value) -> bool {
        let Some(binding) = self
            .bindings
            .iter_mut()
            .find(|binding| binding.name == name)
        else {
            return false;
        };
        if !binding.rebindable {
            return true;
        }
        let spelled = match value {
            Value::Array(values) => values.iter().filter_map(spelling).collect(),
            other => spelling(other).into_iter().collect::<Vec<_>>(),
        };
        let keys: Vec<Key> = spelled.iter().filter_map(|name| parse(name)).collect();
        if !keys.is_empty() {
            binding.keys = keys;
        }
        true
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn binding(&self, action: Action) -> &Binding {
        self.bindings
            .iter()
            .find(|binding| binding.action == action)
            .expect("every action has a binding")
    }

    pub fn action(&self, key: Key) -> Option<Action> {
        self.bindings
            .iter()
            .find(|binding| binding.keys.contains(&key))
            .map(|binding| binding.action)
    }

    pub fn clashes(&self) -> Vec<String> {
        let mut seen: Vec<(Key, &str)> = Vec::new();
        let mut clashing = Vec::new();
        for binding in &self.bindings {
            for key in &binding.keys {
                if let Some((_, earlier)) = seen.iter().rev().find(|(seen_key, _)| seen_key == key)
                {
                    clashing.push(format!(
                        "{} is both {earlier} and {}",
                        spell(*key),
                        binding.name
                    ));
                }
                seen.push((*key, binding.name));
            }
        }
        clashing
    }
}

fn spelling(value: &Value) -> Option<String> {
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

pub fn bindings() -> Vec<Binding> {
    settings::with(|settings| settings.keymap.bindings().to_vec())
}

pub fn binding(action: Action) -> Binding {
    settings::with(|settings| settings.keymap.binding(action).clone())
}

pub fn action(key: Key) -> Option<Action> {
    settings::with(|settings| settings.keymap.action(key))
}

pub fn clashes() -> Vec<String> {
    settings::with(|settings| settings.keymap.clashes())
}

pub fn key(action: Action, which: usize) -> String {
    let keys = binding(action).keys;
    keys.get(which)
        .or(keys.first())
        .map(|key| glyph(*key))
        .unwrap_or_default()
}

pub fn spelled(action: Action) -> String {
    binding(action)
        .keys
        .first()
        .map(|key| spell(*key))
        .unwrap_or_default()
}

pub fn pane_key(action: Action) -> String {
    match binding(action).keys.first() {
        Some(Key::Alt(digit)) if digit.is_ascii_digit() => format!("[{digit}] "),
        Some(key) => format!("[{}] ", glyph(*key)),
        None => String::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rebound(config: &str) -> Keymap {
        let table: toml::Table = toml::from_str(config).unwrap();
        let mut keymap = Keymap::default();
        for (name, value) in table {
            keymap.rebind(&name, &value);
        }
        keymap
    }

    #[test]
    fn reads_a_name_a_person_would_write_for_a_key() {
        assert_eq!(parse("f"), Some(Key::Char('f')));
        assert_eq!(parse("N"), Some(Key::Char('N')));
        assert_eq!(parse("ctrl+o"), Some(Key::Ctrl('o')));
        assert_eq!(parse("CTRL+O"), Some(Key::Ctrl('o')));
        assert_eq!(parse("tab"), Some(Key::Tab));
        assert_eq!(parse("Escape"), Some(Key::Escape));
        assert_eq!(parse("esc"), Some(Key::Escape));
        assert_eq!(parse("space"), Some(Key::Char(' ')));
        assert_eq!(parse("shift+tab"), Some(Key::BackTab));
        assert_eq!(parse("alt+h"), Some(Key::Alt('h')));
        assert_eq!(parse("option+H"), Some(Key::Alt('H')));
        assert_eq!(parse("meta+1"), Some(Key::Alt('1')));
        assert_eq!(parse(" home "), Some(Key::Home));
        assert_eq!(parse("ctrl+i"), Some(Key::Tab));
        assert_eq!(parse("mash"), None);
        assert_eq!(parse("ctrl+1"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse(" "), None);
    }

    #[test]
    fn spells_a_key_the_way_it_was_written() {
        assert_eq!(spell(Key::Char('f')), "f");
        assert_eq!(spell(Key::Ctrl('o')), "ctrl+o");
        assert_eq!(spell(Key::Tab), "tab");
        assert_eq!(spell(Key::Escape), "escape");
        assert_eq!(spell(Key::Alt('1')), "alt+1");
        assert_eq!(spell(Key::Char(' ')), "space");
    }

    #[test]
    fn binds_an_action_to_the_key_the_config_asks_for() {
        let keymap = rebound("filter_rows = \"F\"");

        assert_eq!(keymap.action(Key::Char('F')), Some(Action::FilterRows));
        assert_eq!(keymap.action(Key::Char('f')), None);
        assert_eq!(keymap.binding(Action::FilterRows).shown(), "F");
    }

    #[test]
    fn leaves_movement_and_the_keys_a_terminal_names_alone() {
        let keymap = rebound("move_up = \"w\"\nactivate = \"x\"\nredraw = \"z\"");

        assert_eq!(keymap.action(Key::Char('w')), None);
        assert_eq!(keymap.action(Key::Enter), Some(Action::Activate));
        assert_eq!(keymap.action(Key::Ctrl('l')), Some(Action::Redraw));
    }

    #[test]
    fn says_when_two_actions_ask_for_the_same_key() {
        let keymap = rebound("filter_rows = \"r\"\nyank_row = [\"q\", \"Y\"]");

        assert_eq!(
            keymap.clashes(),
            [
                "r is both filter_rows and reload",
                "q is both yank_row and quit"
            ]
        );
        assert_eq!(keymap.action(Key::Char('r')), Some(Action::FilterRows));
        assert!(Keymap::default().clashes().is_empty());
    }

    #[test]
    fn ignores_a_key_or_an_action_it_cannot_make_sense_of() {
        let keymap = rebound("filter_rows = \"nonsense\"\nno_such_action = \"z\"\nsql = []");

        assert_eq!(keymap.action(Key::Char('f')), Some(Action::FilterRows));
        assert_eq!(keymap.action(Key::Char('z')), None);
        assert_eq!(keymap.action(Key::Char('s')), Some(Action::Sql));
        assert!(!Keymap::default().rebind("no_such_action", &Value::from("z")));
    }

    #[test]
    fn takes_several_keys_for_one_action_and_skips_the_ones_it_cannot_read() {
        let keymap = rebound("yank_value = [\"y\", \"nonsense\", \"ctrl+y\"]");

        assert_eq!(keymap.action(Key::Char('y')), Some(Action::YankValue));
        assert_eq!(keymap.action(Key::Ctrl('y')), Some(Action::YankValue));
        assert_eq!(keymap.binding(Action::YankValue).shown(), "y / ctrl+y");
    }

    #[test]
    fn a_pane_key_reads_as_its_digit_or_its_glyph() {
        crate::settings::install(crate::settings::Settings::default());
        assert_eq!(pane_key(Action::FocusTables), "[1] ");

        let mut settings = crate::settings::Settings::default();
        settings.set("keys.focus_tables", &Value::from("ctrl+t"));
        settings.set("keys.focus_rows", &Value::from("R"));
        crate::settings::install(settings);

        assert_eq!(pane_key(Action::FocusTables), "[ctrl+t] ");
        assert_eq!(pane_key(Action::FocusRows), "[R] ");
        crate::settings::install(crate::settings::Settings::default());
    }

    #[test]
    fn help_keys_read_like_tqls() {
        assert_eq!(
            keys(&[
                Action::MoveLeft,
                Action::MoveDown,
                Action::MoveUp,
                Action::MoveRight
            ]),
            "←↓↑→ / hjkl"
        );
        assert_eq!(keys(&[Action::NextPane, Action::PreviousPane]), "tab ⇧tab");
        assert_eq!(keys(&[Action::Narrow, Action::Widen]), ", . / < >");
        assert_eq!(keys(&[Action::YankValue, Action::YankRow]), "y Y");
        let alt_h = match cfg!(target_os = "macos") {
            true => "H / ⌥h",
            false => "H / alt+h",
        };
        assert_eq!(keys(&[Action::History]), alt_h);
    }
}
