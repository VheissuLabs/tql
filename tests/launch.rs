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
