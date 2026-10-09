use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::dsn;

pub const COMMANDS: [&str; 17] = [
    "browse",
    "open",
    "connect",
    "export",
    "config",
    "update",
    "connections",
    "tables",
    "describe",
    "query",
    "list",
    "help",
    "mcp:start",
    "dump",
    "load",
    "import",
    "sync",
];

static DATABASE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.(sqlite3?|db)$").unwrap());

pub fn rewrite(mut argv: Vec<String>) -> Vec<String> {
    let Some(first) = argv.get(1) else {
        return argv;
    };
    if first.starts_with('-') || COMMANDS.contains(&first.as_str()) || !is_path(first) {
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
        for command in ["browse", "open", "export", "list", "help"] {
            assert_eq!(rewrite(argv(&["tql", command])), argv(&["tql", command]));
        }
    }

    #[test]
    fn routes_a_connection_string_to_open() {
        assert_eq!(
            rewrite(argv(&["tql", "mysql://user:pw@host/db"])),
            argv(&["tql", "open", "mysql://user:pw@host/db"])
        );
    }
}
