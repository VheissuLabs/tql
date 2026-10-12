use app::registry::{self, Shell};

fn tql(arguments: &[&str]) -> app::console::RunResult {
    app::application().run_with(arguments.iter().copied(), "")
}

#[test]
fn every_command_that_needs_the_screen_is_a_shell_command_that_opens_tql() {
    let listed = tql(&["list"]).stdout;

    for entry in registry::opening() {
        assert_eq!(entry.shell, Shell::Opens);
        let line = listed
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{} ", entry.word())))
            .unwrap_or_else(|| panic!("{} is not listed\n{listed}", entry.word()));
        assert!(line.ends_with(", in tql"), "{line}");
    }
}

#[test]
fn a_command_that_opens_tql_says_so_when_there_is_no_terminal() {
    for command in ["new", "sql", "history"] {
        let refused = tql(&[command]);

        assert_eq!(refused.code, 1, "{command}");
        assert!(
            refused.stdout.contains("tql needs a terminal"),
            "{command}: {refused:?}"
        );
    }
}

#[test]
fn a_word_the_shell_already_has_is_marked_as_its_own_and_no_other_word_collides() {
    let listed = tql(&["list"]).stdout;
    let shell_line = |word: &str| {
        listed
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{word} ")))
            .map(str::to_string)
    };

    for entry in registry::ENTRIES {
        let line = shell_line(entry.word());
        match entry.shell {
            Shell::Own => {
                let line =
                    line.unwrap_or_else(|| panic!("{} is not a shell command", entry.word()));
                assert!(!line.ends_with(", in tql"), "{line}");
            }
            Shell::Not => assert!(line.is_none(), "{} collides with {line:?}", entry.word()),
            Shell::Opens => {}
        }
    }
}

#[test]
fn the_browser_palette_lists_a_keyed_action_once_not_again_as_a_command() {
    let names: Vec<&str> = app::tui::keymap::bindings()
        .iter()
        .map(|binding| binding.name)
        .collect();

    for entry in registry::on(registry::Screen::Browser).filter(|entry| entry.palette) {
        let word = entry.word();
        assert!(
            !names.contains(&word) && !names.contains(&format!("focus_{word}").as_str()),
            ":{word} is already in the palette as a keyed action"
        );
    }
}
