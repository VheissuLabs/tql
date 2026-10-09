use std::fs;
use std::path::Path;

use anyhow::bail;

use crate::console::{Cmd, Command};

const STUB: &str = r#"use crate::console::prelude::*;

pub struct {{struct}};

impl Command for {{struct}} {
    fn signature(&self) -> &str {
        "{{signature}}"
    }

    fn description(&self) -> &str {
        "Command description"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        cmd.info("{{signature}} ran successfully.");
        Ok(())
    }
}
"#;

const COMMANDS: &str = "app/commands";
const APPLICATION: &str = "app/mod.rs";

pub struct MakeCommand;

impl Command for MakeCommand {
    fn signature(&self) -> &str {
        "make:command {name : The name of the command, e.g. SendEmails or users:prune}"
    }

    fn description(&self) -> &str {
        "Create a new command"
    }

    fn execute(&self, cmd: &mut Cmd) -> anyhow::Result<()> {
        let name = cmd.argument("name").unwrap_or_default().to_string();
        let segments: Vec<Vec<String>> = name.split(':').map(words).collect();

        if segments.iter().any(Vec::is_empty)
            || !name.starts_with(|first: char| first.is_ascii_alphabetic())
        {
            bail!("\"{name}\" is not a valid command name");
        }

        let all_words: Vec<&String> = segments.iter().flatten().collect();
        let module = all_words
            .iter()
            .map(|word| word.as_str())
            .collect::<Vec<_>>()
            .join("_");
        let name = all_words
            .iter()
            .map(|word| capitalised(word))
            .collect::<String>();
        let signature = segments
            .iter()
            .map(|segment| segment.join("-"))
            .collect::<Vec<_>>()
            .join(":");

        let file = Path::new(COMMANDS).join(format!("{module}.rs"));
        if file.exists() {
            bail!("{} already exists", file.display());
        }

        let modules = Path::new(COMMANDS).join("mod.rs");
        let registered = register(
            &fs::read_to_string(APPLICATION)?,
            &format!(".command(commands::{module}::{name})"),
        )?;

        fs::write(
            &file,
            STUB.replace("{{struct}}", &name)
                .replace("{{signature}}", &signature),
        )?;
        fs::write(
            &modules,
            declare(
                &fs::read_to_string(&modules)?,
                &format!("pub mod {module};"),
            ),
        )?;
        fs::write(APPLICATION, registered)?;

        cmd.info(format!(
            "Command [{}] created successfully.",
            file.display()
        ));
        Ok(())
    }
}

fn declare(modules: &str, declaration: &str) -> String {
    let mut lines: Vec<&str> = modules.lines().filter(|line| !line.is_empty()).collect();
    lines.push(declaration);
    lines.sort();
    lines.join("\n") + "\n"
}

fn register(application: &str, registration: &str) -> anyhow::Result<String> {
    let lines: Vec<&str> = application.lines().collect();
    let registered = |line: &str| line.trim_start().starts_with(".command(commands::");
    let Some(first) = lines.iter().position(|line| registered(line)) else {
        bail!("{APPLICATION} registers no commands to add this one beside");
    };
    let last = first
        + lines[first..]
            .iter()
            .take_while(|line| registered(line))
            .count()
        - 1;
    let indent = &lines[first][..lines[first].len() - lines[first].trim_start().len()];

    let mut commands: Vec<String> = lines[first..=last]
        .iter()
        .map(|line| line.trim().trim_end_matches(';').to_string())
        .collect();
    commands.push(registration.to_string());
    commands.sort();

    let block = commands
        .iter()
        .map(|command| format!("{indent}{command}"))
        .collect::<Vec<_>>()
        .join("\n");
    let ending = if lines[last].trim_end().ends_with(';') {
        ";"
    } else {
        ""
    };

    let mut rewritten: Vec<String> = lines[..first].iter().map(|line| line.to_string()).collect();
    rewritten.push(format!("{block}{ending}"));
    rewritten.extend(lines[last + 1..].iter().map(|line| line.to_string()));
    Ok(rewritten.join("\n") + "\n")
}

fn capitalised(word: &str) -> String {
    let mut characters = word.chars();
    characters
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
        .unwrap_or_default()
}

fn words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_was_lower = false;

    for character in name.chars() {
        if matches!(character, '-' | '_' | ' ') {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_was_lower = false;
            continue;
        }
        if !character.is_ascii_alphanumeric() {
            return Vec::new();
        }
        if character.is_uppercase() && previous_was_lower {
            words.push(std::mem::take(&mut current));
        }
        current.push(character.to_ascii_lowercase());
        previous_was_lower = character.is_lowercase() || character.is_ascii_digit();
    }

    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::{declare, register, words};

    #[test]
    fn splits_names_into_words() {
        assert_eq!(words("SendEmails"), ["send", "emails"]);
        assert_eq!(words("send-emails"), ["send", "emails"]);
        assert_eq!(words("send_emails2"), ["send", "emails2"]);
        assert_eq!(words("HTTPPing"), ["httpping"]);
        assert!(words("bad!").is_empty());
    }

    #[test]
    fn declares_the_module_in_order() {
        assert_eq!(
            declare("pub mod browse;\npub mod open;\n", "pub mod export;"),
            "pub mod browse;\npub mod export;\npub mod open;\n"
        );
    }

    #[test]
    fn registers_the_command_in_order_and_keeps_the_semicolon_last() {
        let application = "fn application() {\n    app\n        .command(commands::browse::Browse)\n        .command(commands::open::Open);\n\n    tail\n}\n";

        assert_eq!(
            register(application, ".command(commands::users_prune::UsersPrune)").unwrap(),
            "fn application() {\n    app\n        .command(commands::browse::Browse)\n        .command(commands::open::Open)\n        .command(commands::users_prune::UsersPrune);\n\n    tail\n}\n"
        );
        assert_eq!(
            register(application, ".command(commands::export::Export)").unwrap(),
            "fn application() {\n    app\n        .command(commands::browse::Browse)\n        .command(commands::export::Export)\n        .command(commands::open::Open);\n\n    tail\n}\n"
        );
    }
}
