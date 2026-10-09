use std::fmt::Display;
use std::io::{BufRead, Write};
use std::str::FromStr;

use anyhow::anyhow;
use unicode_width::UnicodeWidthStr;

use crate::console::config::Config;
use crate::console::input::Input;
use crate::console::{Exit, prompt};

const GREEN: &str = "32";
const YELLOW: &str = "33";
const DIM: &str = "2";
const RED: &str = "31";
const QUESTION: &str = "30;46";
const ERROR: &str = "37;41";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Silent,
    Quiet,
    #[default]
    Normal,
    Verbose,
    VeryVerbose,
    Debug,
}

pub struct Cmd<'a> {
    pub(crate) input: Input,
    pub(crate) config: &'a Config,
    pub(crate) out: &'a mut dyn Write,
    pub(crate) err: &'a mut dyn Write,
    pub(crate) reader: &'a mut dyn BufRead,
    pub(crate) ansi: bool,
    pub(crate) interactive: bool,
    pub(crate) terminal: bool,
    pub(crate) piped: bool,
    pub(crate) verbosity: Verbosity,
    pub(crate) app_name: &'a str,
    pub(crate) app_version: &'a str,
}

impl Cmd<'_> {
    pub fn argument(&self, name: &str) -> Option<&str> {
        self.input.argument(name)
    }

    pub fn arguments(&self, name: &str) -> &[String] {
        self.input.arguments(name)
    }

    pub fn argument_as<Target>(&self, name: &str) -> anyhow::Result<Option<Target>>
    where
        Target: FromStr,
        Target::Err: Display,
    {
        parse_value(name, self.argument(name))
    }

    pub fn option(&self, name: &str) -> Option<&str> {
        self.input.option(name)
    }

    pub fn options(&self, name: &str) -> &[String] {
        self.input.options(name)
    }

    pub fn option_as<Target>(&self, name: &str) -> anyhow::Result<Option<Target>>
    where
        Target: FromStr,
        Target::Err: Display,
    {
        parse_value(name, self.option(name))
    }

    pub fn flag(&self, name: &str) -> bool {
        self.input.flag(name)
    }

    pub fn config(&self) -> &Config {
        self.config
    }

    pub fn app_name(&self) -> &str {
        self.app_name
    }

    pub fn app_version(&self) -> &str {
        self.app_version
    }

    pub fn has_terminal(&self) -> bool {
        self.terminal
    }

    pub fn is_piped(&self) -> bool {
        self.piped
    }

    pub fn read_all(&mut self) -> anyhow::Result<String> {
        let mut read = String::new();
        self.reader.read_to_string(&mut read)?;
        Ok(read)
    }

    pub fn read_line(&mut self) -> anyhow::Result<Option<String>> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end_matches(['\r', '\n']).to_string()))
    }

    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }

    pub fn refuse(&mut self, text: impl Display) {
        if self.verbosity > Verbosity::Silent {
            let _ = writeln!(self.err, "{text}");
        }
    }

    pub fn verbosity(&self) -> Verbosity {
        self.verbosity
    }

    pub fn is_verbose(&self) -> bool {
        self.verbosity >= Verbosity::Verbose
    }

    pub fn is_quiet(&self) -> bool {
        self.verbosity < Verbosity::Normal
    }

    pub fn has_ansi(&self) -> bool {
        self.ansi
    }

    pub fn write(&mut self, text: impl Display) {
        if !self.is_quiet() {
            let _ = write!(self.out, "{text}");
            let _ = self.out.flush();
        }
    }

    pub fn marked(&mut self, text: &str) {
        let marked = self.markup(text);
        self.line(marked);
    }

    pub fn markup(&self, text: &str) -> String {
        let tags = regex::Regex::new(r"<fg=(\w+)>|</>").expect("the pattern is valid");
        let ansi = self.ansi;
        tags.replace_all(text, |tag: &regex::Captures| {
            if !ansi {
                return String::new();
            }
            match tag.get(1).map(|color| color.as_str()) {
                Some("green") => "\x1b[32m",
                Some("yellow") => "\x1b[33m",
                Some("cyan") => "\x1b[36m",
                Some("red") => "\x1b[31m",
                Some("gray") => "\x1b[90m",
                Some(_) => "",
                None => "\x1b[39m",
            }
            .to_string()
        })
        .into_owned()
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    pub fn line(&mut self, text: impl Display) {
        if !self.is_quiet() {
            let _ = writeln!(self.out, "{text}");
        }
    }

    pub fn new_line(&mut self) {
        if !self.is_quiet() {
            let _ = writeln!(self.out);
        }
    }

    pub fn info(&mut self, text: impl Display) {
        self.styled_line(text, GREEN);
    }

    pub fn comment(&mut self, text: impl Display) {
        self.styled_line(text, YELLOW);
    }

    pub fn warn(&mut self, text: impl Display) {
        self.styled_line(text, YELLOW);
    }

    pub fn question(&mut self, text: impl Display) {
        self.styled_line(text, QUESTION);
    }

    pub fn error(&mut self, text: impl Display) {
        if self.verbosity > Verbosity::Silent {
            let painted = self.paint(text, ERROR);
            let _ = writeln!(self.out, "{painted}");
        }
    }

    pub fn alert(&mut self, text: impl Display) {
        let text = text.to_string();
        let border = "*".repeat(text.chars().count() + 12);
        self.comment(&border);
        self.comment(format!("*     {text}     *"));
        self.comment(&border);
        self.new_line();
    }

    pub fn table<Header, Rows, Cell>(&mut self, headers: &[Header], rows: Rows)
    where
        Header: AsRef<str>,
        Rows: IntoIterator,
        Rows::Item: IntoIterator<Item = Cell>,
        Cell: Display,
    {
        let headers: Vec<String> = headers
            .iter()
            .map(|header| header.as_ref().to_string())
            .collect();
        let rows: Vec<Vec<String>> = rows
            .into_iter()
            .map(|row| row.into_iter().map(|cell| cell.to_string()).collect())
            .collect();

        let columns = rows
            .iter()
            .map(Vec::len)
            .chain([headers.len()])
            .max()
            .unwrap_or(0);
        let mut widths = vec![0; columns];
        for row in std::iter::once(&headers).chain(&rows) {
            for (column, cell) in row.iter().enumerate() {
                widths[column] = widths[column].max(cell.width());
            }
        }

        let border = format!(
            "+{}+",
            widths
                .iter()
                .map(|width| "-".repeat(width + 2))
                .collect::<Vec<_>>()
                .join("+")
        );
        let render = |row: &[String], style: Option<&str>| {
            let cells: Vec<String> = widths
                .iter()
                .enumerate()
                .map(|(column, width)| {
                    let cell = row.get(column).map(String::as_str).unwrap_or("");
                    let padding = " ".repeat(width - cell.width());
                    let cell = match style {
                        Some(style) => self.paint(cell, style),
                        None => cell.to_string(),
                    };
                    format!(" {cell}{padding} ")
                })
                .collect();
            format!("|{}|", cells.join("|"))
        };

        let mut lines = vec![border.clone()];
        if !headers.is_empty() {
            lines.push(render(&headers, Some(GREEN)));
            lines.push(border.clone());
        }
        lines.extend(rows.iter().map(|row| render(row, None)));
        lines.push(border);

        for line in lines {
            self.line(line);
        }
    }

    pub fn task<Task>(&mut self, title: impl Display, task: Task) -> anyhow::Result<bool>
    where
        Task: FnOnce() -> anyhow::Result<bool>,
    {
        if self.ansi {
            let loading = self.paint("loading...", YELLOW);
            let _ = write!(self.out, "{title}: {loading}");
            let _ = self.out.flush();
        }

        let result = task();

        let mark = match result {
            Ok(true) => self.paint("✔", GREEN),
            _ => self.paint("✘", RED),
        };
        let rewind = if self.ansi { "\r\x1b[2K" } else { "" };
        let _ = writeln!(self.out, "{rewind}{title}: {mark}");
        result
    }

    pub fn ask(&mut self, question: &str) -> anyhow::Result<String> {
        self.ask_inner(question, None)
    }

    pub fn ask_with_default(&mut self, question: &str, default: &str) -> anyhow::Result<String> {
        self.ask_inner(question, Some(default))
    }

    pub fn confirm(&mut self, question: &str, default: bool) -> anyhow::Result<bool> {
        if !self.interactive {
            return Ok(default);
        }

        let default_label = if default { "yes" } else { "no" };
        let prompt = format!(
            " {} (yes/no) [{}]:\n > ",
            self.paint(question, GREEN),
            self.paint(default_label, YELLOW)
        );

        loop {
            let Some(answer) = self.read_answer(&prompt)? else {
                return Ok(default);
            };
            match answer.trim().to_ascii_lowercase().chars().next() {
                None => return Ok(default),
                Some('y') => return Ok(true),
                Some('n') => return Ok(false),
                Some(_) => self.error("Please answer yes or no."),
            }
        }
    }

    pub fn confirm_box(&mut self, label: &str, default: bool) -> anyhow::Result<bool> {
        if !(self.terminal && self.interactive && !self.piped) {
            return Ok(default);
        }
        let _ = self.out.flush();
        Ok(prompt::confirm(label, default)?.unwrap_or(false))
    }

    pub fn select(
        &mut self,
        label: &str,
        options: &[String],
        default: Option<usize>,
    ) -> anyhow::Result<String> {
        if !(self.terminal && self.interactive) {
            return self.choice(label, options, default);
        }
        let menu = prompt::Menu::select(options.to_vec(), default.unwrap_or(0));
        self.picked(label, options, menu)
    }

    pub fn search(
        &mut self,
        label: &str,
        options: &[String],
        pinned: usize,
        placeholder: &str,
    ) -> anyhow::Result<String> {
        if !(self.terminal && self.interactive) {
            return self.choice(label, options, None);
        }
        let menu = prompt::Menu::search(options.to_vec(), pinned, placeholder);
        self.picked(label, options, menu)
    }

    fn picked(
        &mut self,
        label: &str,
        options: &[String],
        menu: prompt::Menu,
    ) -> anyhow::Result<String> {
        match prompt::ask(label, menu)? {
            Some(chosen) => Ok(options[chosen].clone()),
            None => Err(Exit(130).into()),
        }
    }

    pub fn choice<Choice>(
        &mut self,
        question: &str,
        choices: &[Choice],
        default: Option<usize>,
    ) -> anyhow::Result<String>
    where
        Choice: AsRef<str>,
    {
        let choices: Vec<&str> = choices.iter().map(AsRef::as_ref).collect();
        let default = default.and_then(|index| choices.get(index).copied());

        if !self.interactive {
            return default
                .map(String::from)
                .ok_or_else(|| anyhow!("cannot ask \"{question}\" without interaction"));
        }

        let hint = default
            .map(|value| format!(" [{}]", self.paint(value, YELLOW)))
            .unwrap_or_default();
        let listing: String = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| format!("  [{}] {choice}\n", self.paint(index, YELLOW)))
            .collect();
        let prompt = format!(" {}{hint}:\n{listing} > ", self.paint(question, GREEN));

        loop {
            let Some(answer) = self.read_answer(&prompt)? else {
                return default
                    .map(String::from)
                    .ok_or_else(|| anyhow!("no answer given for \"{question}\""));
            };
            let answer = answer.trim();

            if answer.is_empty()
                && let Some(default) = default
            {
                return Ok(default.to_string());
            }

            let picked = answer
                .parse::<usize>()
                .ok()
                .and_then(|index| choices.get(index).copied())
                .or_else(|| choices.iter().copied().find(|choice| *choice == answer));

            match picked {
                Some(choice) => return Ok(choice.to_string()),
                None => self.error(format!("Value \"{answer}\" is invalid")),
            }
        }
    }

    pub fn ask_with_hint(
        &mut self,
        question: &str,
        default: &str,
        hint: &str,
    ) -> anyhow::Result<String> {
        self.ask_hinted(question, Some(default), Some(hint))
    }

    fn ask_inner(&mut self, question: &str, default: Option<&str>) -> anyhow::Result<String> {
        self.ask_hinted(question, default, None)
    }

    fn ask_hinted(
        &mut self,
        question: &str,
        default: Option<&str>,
        hint: Option<&str>,
    ) -> anyhow::Result<String> {
        if !self.interactive {
            return default
                .map(String::from)
                .ok_or_else(|| anyhow!("cannot ask \"{question}\" without interaction"));
        }

        let offered = default
            .map(|value| format!(" [{}]", self.paint(value, YELLOW)))
            .unwrap_or_default();
        let hint = hint
            .map(|hint| format!("   {}\n", self.paint(hint, DIM)))
            .unwrap_or_default();
        let prompt = format!(" {}{offered}:\n{hint} > ", self.paint(question, GREEN));

        let answer = self.read_answer(&prompt)?.unwrap_or_default();
        if answer.trim().is_empty() {
            return Ok(default.unwrap_or_default().to_string());
        }
        Ok(answer)
    }

    fn read_answer(&mut self, prompt: &str) -> anyhow::Result<Option<String>> {
        let _ = write!(self.out, "{prompt}");
        let _ = self.out.flush();

        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            self.new_line();
            return Ok(None);
        }
        Ok(Some(line.trim_end_matches(['\r', '\n']).to_string()))
    }

    fn styled_line(&mut self, text: impl Display, style: &str) {
        let painted = self.paint(text, style);
        self.line(painted);
    }

    fn paint(&self, text: impl Display, style: &str) -> String {
        if self.ansi {
            format!("\x1b[{style}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

fn parse_value<Target>(name: &str, value: Option<&str>) -> anyhow::Result<Option<Target>>
where
    Target: FromStr,
    Target::Err: Display,
{
    value
        .map(|value| {
            value
                .parse()
                .map_err(|error| anyhow!("invalid value \"{value}\" for `{name}`: {error}"))
        })
        .transpose()
}
