#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Name {
    Quit,
    Write,
    New,
    Connections,
    Tables,
    Rows,
    RunOrReload,
    Run,
    Reload,
    Sql,
    History,
    Export,
    CreateDatabase,
    DropDatabase,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Connections,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Nothing,
    Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Not,
    Own,
    Opens,
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub name: Name,
    pub words: &'static [&'static str],
    pub description: &'static str,
    pub screens: &'static [Screen],
    pub shell: Shell,
    pub needs: Needs,
    pub takes: Option<&'static str>,
    pub palette: bool,
}

impl Entry {
    pub fn word(&self) -> &'static str {
        self.words[0]
    }

    pub fn works_on(&self, screen: Screen) -> bool {
        self.screens.contains(&screen)
    }
}

const EVERYWHERE: &[Screen] = &[Screen::Connections, Screen::Browser];
const BROWSER: &[Screen] = &[Screen::Browser];
const CONNECTIONS: &[Screen] = &[Screen::Connections];

const fn entry(
    name: Name,
    words: &'static [&'static str],
    description: &'static str,
    screens: &'static [Screen],
) -> Entry {
    Entry {
        name,
        words,
        description,
        screens,
        shell: Shell::Not,
        needs: Needs::Nothing,
        takes: None,
        palette: true,
    }
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        shell: Shell::Opens,
        ..entry(
            Name::Settings,
            &["settings"],
            "change tql's settings",
            EVERYWHERE,
        )
    },
    Entry {
        palette: false,
        ..entry(Name::Quit, &["q", "q!", "quit"], "quit tql", EVERYWHERE)
    },
    entry(
        Name::Write,
        &["w", "write"],
        "write the pending changes",
        EVERYWHERE,
    ),
    Entry {
        shell: Shell::Opens,
        ..entry(Name::New, &["new"], "add a connection", CONNECTIONS)
    },
    Entry {
        shell: Shell::Own,
        palette: false,
        ..entry(
            Name::Connections,
            &["connections", "c"],
            "go back to the connection list",
            BROWSER,
        )
    },
    Entry {
        shell: Shell::Own,
        palette: false,
        ..entry(Name::Tables, &["tables"], "focus the table list", BROWSER)
    },
    Entry {
        palette: false,
        ..entry(Name::Rows, &["rows"], "focus the rows", BROWSER)
    },
    Entry {
        palette: false,
        ..entry(
            Name::RunOrReload,
            &["r"],
            "run the statement in the SQL editor, or reload the table",
            BROWSER,
        )
    },
    entry(
        Name::Run,
        &["run"],
        "run the statement in the SQL editor",
        BROWSER,
    ),
    Entry {
        palette: false,
        ..entry(Name::Reload, &["reload"], "reload the table", BROWSER)
    },
    Entry {
        shell: Shell::Opens,
        needs: Needs::Connection,
        palette: false,
        ..entry(Name::Sql, &["sql"], "open the SQL editor", BROWSER)
    },
    Entry {
        shell: Shell::Opens,
        needs: Needs::Connection,
        palette: false,
        ..entry(
            Name::History,
            &["history"],
            "browse the query history",
            BROWSER,
        )
    },
    Entry {
        shell: Shell::Own,
        ..entry(
            Name::Export,
            &["export", "export sql"],
            "export this table as SQL",
            BROWSER,
        )
    },
    Entry {
        takes: Some("name"),
        ..entry(
            Name::CreateDatabase,
            &["create database"],
            "create a database on this server",
            BROWSER,
        )
    },
    Entry {
        takes: Some("name"),
        ..entry(
            Name::DropDatabase,
            &["drop database"],
            "drop a database on this server",
            BROWSER,
        )
    },
];

pub fn entry_for(name: Name) -> &'static Entry {
    ENTRIES
        .iter()
        .find(|entry| entry.name == name)
        .expect("every name has an entry")
}

pub fn parse(line: &str) -> Option<(&'static Entry, String)> {
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let lowered = line.to_lowercase();
    ENTRIES
        .iter()
        .flat_map(|entry| entry.words.iter().map(move |word| (entry, *word)))
        .filter(|(entry, word)| {
            lowered == *word
                || entry.takes.is_some()
                    && lowered
                        .strip_prefix(word)
                        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
        })
        .max_by_key(|(_, word)| word.len())
        .map(|(entry, word)| (entry, line[word.len()..].trim().to_string()))
}

pub fn on(screen: Screen) -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter().filter(move |entry| entry.works_on(screen))
}

pub fn opening() -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter().filter(|entry| entry.shell == Shell::Opens)
}

pub fn unavailable(screen: Screen, line: &str) -> String {
    match parse(line) {
        Some((entry, _)) if !entry.works_on(screen) => match screen {
            Screen::Connections => format!(":{} works once a connection is open", entry.word()),
            Screen::Browser => format!(":{} works on the connection list", entry.word()),
        },
        _ => format!("unknown command: :{}", line.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_has_one_entry_and_no_word_is_used_twice() {
        let mut words: Vec<&str> = ENTRIES
            .iter()
            .flat_map(|entry| entry.words.iter().copied())
            .collect();
        let count = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), count);
        for entry in ENTRIES {
            assert_eq!(entry_for(entry.name).word(), entry.word());
        }
    }

    #[test]
    fn reads_a_command_its_aliases_and_what_follows_it() {
        assert_eq!(parse("q!").map(|(entry, _)| entry.name), Some(Name::Quit));
        assert_eq!(
            parse(" C ").map(|(entry, _)| entry.name),
            Some(Name::Connections)
        );
        assert_eq!(
            parse("r").map(|(entry, _)| entry.name),
            Some(Name::RunOrReload)
        );
        assert_eq!(
            parse("create  Database Shop").map(|(entry, rest)| (entry.name, rest)),
            Some((Name::CreateDatabase, "Shop".to_string()))
        );
        assert_eq!(
            parse("drop database").map(|(entry, rest)| (entry.name, rest)),
            Some((Name::DropDatabase, String::new()))
        );
        assert_eq!(parse("write it").map(|(entry, _)| entry.name), None);
        assert!(parse("nonsense").is_none());
    }

    #[test]
    fn says_where_a_command_works_when_it_is_typed_on_the_wrong_screen() {
        assert_eq!(
            unavailable(Screen::Connections, "sql"),
            ":sql works once a connection is open"
        );
        assert_eq!(
            unavailable(Screen::Browser, "new"),
            ":new works on the connection list"
        );
        assert_eq!(
            unavailable(Screen::Browser, "nope"),
            "unknown command: :nope"
        );
    }
}
