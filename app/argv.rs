use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::dsn;
use crate::registry;

pub const COMMANDS: [&str; 18] = [
    "browse",
    "open",
    "connect",
    "export",
    "dump",
    "load",
    "import",
    "sync",
    "tableplus",
    "config",
    "update",
    "connections",
    "tables",
    "describe",
    "query",
    "list",
    "help",
    "mcp:start",
];

static DATABASE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.(sqlite3?|db)$").unwrap());

pub fn rewrite(mut argv: Vec<String>) -> Vec<String> {
    let Some(first) = argv.get(1) else {
        return argv;
    };
    // connect opens a saved connection by name; a path or connection string
    // given to it is still for open, as it was when connect was open's alias.
    if first == "connect"
        && argv[2..]
            .iter()
            .any(|argument| !argument.starts_with('-') && is_path(argument))
    {
        argv[1] = "open".to_string();
        return argv;
    }
    if first.starts_with('-')
        || COMMANDS.contains(&first.as_str())
        || registry::opening().any(|entry| entry.word() == first)
        || !is_path(first)
    {
        return argv;
    }
    argv.insert(1, "open".to_string());
    argv
}

fn is_path(argument: &str) -> bool {
    dsn::looks_like_one(argument)
        || argument.contains('/')
        || DATABASE_FILE.is_match(argument)
        || Path::new(argument).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| argument.to_string())
            .collect()
    }

    #[test]
    fn treats_a_bare_file_path_as_open() {
        assert_eq!(
            rewrite(argv(&["tql", "/tmp/shop.sqlite"])),
            argv(&["tql", "open", "/tmp/shop.sqlite"])
        );
    }

    #[test]
    fn leaves_a_real_command_alone() {
        for arguments in [
            &["tql", "export", "demo"][..],
            &["tql"][..],
            &["tql", "--version"][..],
        ] {
            assert_eq!(rewrite(argv(arguments)), argv(arguments));
        }
    }

    #[test]
    fn routes_a_path_that_does_not_exist_so_the_error_is_ours() {
        assert_eq!(
            rewrite(argv(&["tql", "nope.sqlite"])),
            argv(&["tql", "open", "nope.sqlite"])
        );
        assert_eq!(
            rewrite(argv(&["tql", "./data/nope"])),
            argv(&["tql", "open", "./data/nope"])
        );
    }

    #[test]
    fn does_not_mistake_a_command_name_for_a_path() {
        for command in [
            "browse", "open", "export", "list", "help", "sql", "history", "new",
        ] {
            assert_eq!(rewrite(argv(&["tql", command])), argv(&["tql", command]));
        }
    }

    #[test]
    fn connect_keeps_a_connection_name_and_hands_a_connection_string_or_file_to_open() {
        assert_eq!(
            rewrite(argv(&["tql", "connect", "prod", "--db=analytics"])),
            argv(&["tql", "connect", "prod", "--db=analytics"])
        );
        assert_eq!(
            rewrite(argv(&["tql", "connect", "shop.sqlite"])),
            argv(&["tql", "open", "shop.sqlite"])
        );
        assert_eq!(
            rewrite(argv(&[
                "tql",
                "connect",
                "--name=Shop",
                "mysql://root@127.0.0.1/shop"
            ])),
            argv(&["tql", "open", "--name=Shop", "mysql://root@127.0.0.1/shop"])
        );
        assert_eq!(
            rewrite(argv(&["tql", "connect", "Cargo.toml"])),
            argv(&["tql", "open", "Cargo.toml"])
        );
        assert_eq!(
            rewrite(argv(&["tql", "connect", "./missing.sqlite"])),
            argv(&["tql", "open", "./missing.sqlite"])
        );
        assert_eq!(
            rewrite(argv(&["tql", "connect", "prod", "--db=a/b"])),
            argv(&["tql", "connect", "prod", "--db=a/b"])
        );
    }

    #[test]
    fn routes_a_connection_string_to_open() {
        assert_eq!(
            rewrite(argv(&["tql", "mysql://user:pw@host/db"])),
            argv(&["tql", "open", "mysql://user:pw@host/db"])
        );
    }
}
