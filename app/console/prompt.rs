use std::io::{self, Write};

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::style::Stylize;
use ratatui::crossterm::terminal::{self, ClearType};
use ratatui::crossterm::{cursor, execute, queue};

use crate::terminal::Key;

const ROWS: usize = 10;

pub struct Menu {
    options: Vec<String>,
    pinned: usize,
    searching: bool,
    placeholder: String,
    query: String,
    index: usize,
    start: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Stay,
    Pick(usize),
    Cancel,
}

impl Menu {
    pub fn select(options: Vec<String>, default: usize) -> Menu {
        Menu {
            index: default.min(options.len().saturating_sub(1)),
            options,
            pinned: 0,
            searching: false,
            placeholder: String::new(),
            query: String::new(),
            start: 0,
        }
    }

    pub fn search(options: Vec<String>, pinned: usize, placeholder: &str) -> Menu {
        Menu {
            searching: true,
            pinned,
            placeholder: placeholder.to_string(),
            ..Menu::select(options, 0)
        }
    }

    pub fn matches(&self) -> Vec<usize> {
        let needle = self.query.to_lowercase();
        (0..self.options.len())
            .filter(|&at| {
                at < self.pinned
                    || needle.is_empty()
                    || self.options[at].to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn press(&mut self, key: Key) -> Step {
        let count = self.matches().len();
        match key {
            Key::Ctrl('c') | Key::Escape => return Step::Cancel,
            Key::Enter => {
                return match self.matches().get(self.index) {
                    Some(&chosen) => Step::Pick(chosen),
                    None => Step::Stay,
                };
            }
            Key::Up | Key::Ctrl('p') => self.up(count),
            Key::Down | Key::Ctrl('n') | Key::Tab => self.down(count),
            Key::Char('k') if !self.searching => self.up(count),
            Key::Char('j') if !self.searching => self.down(count),
            Key::Backspace | Key::Ctrl('h') if self.searching => {
                self.query.pop();
                self.index = 0;
            }
            Key::Char(character) if self.searching => {
                self.query.push(character);
                self.index = 0;
            }
            _ => {}
        }
        if self.index < self.start {
            self.start = self.index;
        } else if self.index >= self.start + ROWS {
            self.start = self.index + 1 - ROWS;
        }
        Step::Stay
    }

    fn up(&mut self, count: usize) {
        self.index = self.index.checked_sub(1).unwrap_or(count.saturating_sub(1));
    }

    fn down(&mut self, count: usize) {
        self.index = if self.index + 1 >= count {
            0
        } else {
            self.index + 1
        };
    }

    pub fn lines(&self, label: &str) -> Vec<String> {
        let mut lines = vec![format!(" {}", label.green())];
        if self.searching {
            let typed = match self.query.is_empty() {
                true => self.placeholder.as_str().dark_grey().to_string(),
                false => self.query.clone(),
            };
            lines.push(format!(" › {typed}"));
        }
        let matches = self.matches();
        if matches.is_empty() {
            lines.push("   no matches".dark_grey().to_string());
        }
        for (shown, &at) in matches.iter().enumerate().skip(self.start).take(ROWS) {
            let option = &self.options[at];
            lines.push(match shown == self.index {
                true => format!(" {} {}", "›".cyan(), option.as_str().cyan()),
                false => format!("   {option}"),
            });
        }
        lines
    }
}

pub fn ask(label: &str, mut menu: Menu) -> io::Result<Option<usize>> {
    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    let mut drawn = 0;
    let picked = loop {
        drawn = draw(&mut out, &menu.lines(label), drawn)?;
        let Event::Key(pressed) = event::read()? else {
            continue;
        };
        if pressed.kind == KeyEventKind::Release {
            continue;
        }
        let Some(key) = Key::from_event(pressed) else {
            continue;
        };
        match menu.press(key) {
            Step::Stay => {}
            Step::Pick(chosen) => break Some(chosen),
            Step::Cancel => break None,
        }
    };
    let chosen = picked
        .map(|at| menu.options[at].clone())
        .unwrap_or_default();
    draw(
        &mut out,
        &[format!(" {}", label.green()), format!("   {chosen}")],
        drawn,
    )?;
    terminal::disable_raw_mode()?;
    Ok(picked)
}

fn draw(out: &mut impl Write, lines: &[String], drawn: usize) -> io::Result<usize> {
    if drawn > 0 {
        queue!(out, cursor::MoveUp(drawn as u16))?;
    }
    queue!(
        out,
        cursor::MoveToColumn(0),
        terminal::Clear(ClearType::FromCursorDown)
    )?;
    for line in lines {
        write!(out, "{line}\r\n")?;
    }
    execute!(out)?;
    Ok(lines.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn moves_with_arrows_and_vim_keys_and_picks_on_enter() {
        let mut menu = Menu::select(options(&["a", "b", "c"]), 0);

        menu.press(Key::Down);
        menu.press(Key::Char('j'));
        assert_eq!(menu.press(Key::Enter), Step::Pick(2));

        menu.press(Key::Char('j'));
        assert_eq!(menu.press(Key::Enter), Step::Pick(0));
        menu.press(Key::Up);
        assert_eq!(menu.press(Key::Enter), Step::Pick(2));
        assert_eq!(menu.press(Key::Escape), Step::Cancel);
    }

    #[test]
    fn a_search_filters_as_you_type_and_keeps_the_pinned_option_first() {
        let mut menu = Menu::search(
            options(&["every table", "orders", "order_items", "users"]),
            1,
            "type",
        );

        for character in "ORD".chars() {
            menu.press(Key::Char(character));
        }
        assert_eq!(menu.matches(), [0, 1, 2]);

        menu.press(Key::Down);
        menu.press(Key::Down);
        assert_eq!(menu.press(Key::Enter), Step::Pick(2));

        menu.press(Key::Backspace);
        menu.press(Key::Backspace);
        menu.press(Key::Backspace);
        assert_eq!(menu.matches(), [0, 1, 2, 3]);
        assert_eq!(menu.press(Key::Enter), Step::Pick(0));
    }

    #[test]
    fn a_search_takes_j_and_k_as_letters() {
        let mut menu = Menu::search(options(&["jobs", "kits"]), 0, "");

        menu.press(Key::Char('k'));

        assert_eq!(menu.matches(), [1]);
    }
}
