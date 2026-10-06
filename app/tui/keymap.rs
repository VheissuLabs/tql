use zerorust_tui::Key;

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

pub struct Binding {
    pub action: Action,
    pub keys: &'static [Key],
    pub description: &'static str,
    pub ported: bool,
}

const fn ported(action: Action, keys: &'static [Key], description: &'static str) -> Binding {
    Binding {
        action,
        keys,
        description,
        ported: true,
    }
}

const fn later(action: Action, keys: &'static [Key], description: &'static str) -> Binding {
    Binding {
        action,
        keys,
        description,
        ported: false,
    }
}

pub const BINDINGS: &[Binding] = &[
    ported(Action::MoveUp, &[Key::Up, Key::Char('k')], "move up"),
    ported(Action::MoveDown, &[Key::Down, Key::Char('j')], "move down"),
    ported(
        Action::MoveLeft,
        &[Key::Left, Key::Char('h')],
        "column left",
    ),
    ported(
        Action::MoveRight,
        &[Key::Right, Key::Char('l')],
        "column right",
    ),
    ported(Action::Activate, &[Key::Enter], "open, or edit"),
    ported(Action::Escape, &[Key::Escape], "back, or close"),
    ported(Action::Command, &[Key::Char(':')], "command line"),
    ported(Action::NextPane, &[Key::Tab], "next pane"),
    ported(Action::PreviousPane, &[Key::BackTab], "previous pane"),
    ported(
        Action::FocusTables,
        &[Key::Alt('1')],
        "go to the table list",
    ),
    ported(Action::FocusRows, &[Key::Alt('2')], "go to the rows"),
    ported(Action::FocusSql, &[Key::Alt('3')], "go to the SQL editor"),
    ported(
        Action::ToggleTables,
        &[Key::Char('\\')],
        "show or hide tables",
    ),
    ported(Action::InspectRow, &[Key::Char('i')], "inspect the row"),
    ported(Action::ViewValue, &[Key::Char('I')], "view the value"),
    ported(Action::EditValue, &[Key::Char('e')], "edit the value"),
    ported(Action::EditRow, &[Key::Char('E')], "edit the row"),
    ported(
        Action::SortColumn,
        &[Key::Char('o')],
        "sort: asc, desc, key",
    ),
    ported(Action::MarkDelete, &[Key::Char('d')], "mark for deletion"),
    ported(Action::ClearMarks, &[Key::Char('u')], "drop changes"),
    ported(Action::NewRow, &[Key::Char('N')], "add a row"),
    later(Action::Ask, &[Key::Char('a')], "ask in english"),
    ported(Action::FilterRows, &[Key::Char('f')], "filter rows"),
    ported(Action::FilterTables, &[Key::Char('/')], "filter tables"),
    ported(Action::Structure, &[Key::Char('t')], "structure"),
    ported(Action::Databases, &[Key::Char('b')], "switch database"),
    ported(Action::Sql, &[Key::Char('s')], "SQL editor"),
    ported(
        Action::History,
        &[Key::Char('H'), Key::Alt('h')],
        "query history",
    ),
    ported(Action::FollowLink, &[Key::Char('L')], "follow a link"),
    ported(Action::JumpBack, &[Key::Ctrl('o')], "go back"),
    ported(Action::NextPage, &[Key::Char('n')], "next page"),
    ported(Action::PreviousPage, &[Key::Char('p')], "previous page"),
    ported(Action::Reload, &[Key::Char('r')], "reload"),
    ported(Action::Connections, &[Key::Char('c')], "connections"),
    ported(Action::YankValue, &[Key::Char('y')], "yank the value"),
    ported(Action::YankRow, &[Key::Char('Y')], "yank the row"),
    ported(
        Action::Narrow,
        &[Key::Char(','), Key::Char('<')],
        "narrow column or list",
    ),
    ported(
        Action::Widen,
        &[Key::Char('.'), Key::Char('>')],
        "widen column or list",
    ),
    ported(Action::ResetWidth, &[Key::Char('=')], "reset width"),
    ported(Action::Palette, &[Key::Ctrl('k')], "command palette"),
    ported(Action::Help, &[Key::Char('?')], "help"),
    ported(Action::Quit, &[Key::Char('q')], "quit"),
    ported(Action::Redraw, &[Key::Ctrl('l')], "redraw"),
];

pub fn binding(action: Action) -> &'static Binding {
    BINDINGS
        .iter()
        .find(|binding| binding.action == action)
        .expect("every action has a binding")
}

pub fn action(key: Key) -> Option<&'static Binding> {
    BINDINGS.iter().find(|binding| binding.keys.contains(&key))
}

pub fn key(action: Action, which: usize) -> String {
    let keys = binding(action).keys;
    keys.get(which)
        .or(keys.first())
        .map(|key| glyph(*key))
        .unwrap_or_default()
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
                let glyph = glyph(*key);
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
        assert_eq!(keys(&[Action::History]), "H / ⌥h");
    }
}
