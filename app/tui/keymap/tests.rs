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
