use std::cell::RefCell;
use std::io::Write;
use std::process::{Command, Stdio};
use std::rc::Rc;

use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reached {
    System,
    Terminal,
}

impl Reached {
    pub fn name(self) -> &'static str {
        match self {
            Reached::System => "system",
            Reached::Terminal => "terminal",
        }
    }
}

pub trait Clipboard {
    fn copy(&mut self, text: &str) -> Reached;
}

pub struct SystemClipboard;

const COMMANDS: [&[&str]; 4] = [
    &["pbcopy"],
    &["wl-copy"],
    &["xclip", "-selection", "clipboard"],
    &["xsel", "--clipboard", "--input"],
];

impl Clipboard for SystemClipboard {
    fn copy(&mut self, text: &str) -> Reached {
        if COMMANDS
            .iter()
            .any(|command| pipe_into(command, text).unwrap_or(false))
        {
            return Reached::System;
        }

        let payload = base64::engine::general_purpose::STANDARD.encode(text);
        let mut terminal = std::io::stdout();
        let _ = write!(terminal, "\x1b]52;c;{payload}\x07");
        let _ = terminal.flush();
        Reached::Terminal
    }
}

fn pipe_into(command: &[&str], text: &str) -> std::io::Result<bool> {
    let mut child = Command::new(command[0])
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes())?;
    Ok(child.wait()?.success())
}

#[derive(Clone, Default)]
pub struct Recorded {
    copies: Rc<RefCell<Vec<String>>>,
    reaches: Option<Reached>,
}

impl Recorded {
    pub fn reaching(reached: Reached) -> Self {
        Recorded {
            reaches: Some(reached),
            ..Recorded::default()
        }
    }

    pub fn copies(&self) -> Vec<String> {
        self.copies.borrow().clone()
    }

    pub fn last(&self) -> Option<String> {
        self.copies.borrow().last().cloned()
    }
}

impl Clipboard for Recorded {
    fn copy(&mut self, text: &str) -> Reached {
        self.copies.borrow_mut().push(text.to_string());
        self.reaches.unwrap_or(Reached::System)
    }
}
