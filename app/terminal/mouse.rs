use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::terminal::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Drag,
    Release,
    WheelUp,
    WheelDown,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    pub button: Button,
    pub column: u16,
    pub row: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Key(Key),
    Mouse(Mouse),
    Paste(String),
}

impl Mouse {
    pub fn from_event(event: MouseEvent) -> Self {
        let button = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => Button::Left,
            MouseEventKind::Drag(MouseButton::Left) => Button::Drag,
            MouseEventKind::Up(_) => Button::Release,
            MouseEventKind::ScrollUp => Button::WheelUp,
            MouseEventKind::ScrollDown => Button::WheelDown,
            _ => Button::Other,
        };
        Mouse {
            button,
            column: event.column + 1,
            row: event.row + 1,
        }
    }

    fn from_sgr(code: u16, column: u16, row: u16, pressed: bool) -> Self {
        let button = match (code, pressed) {
            (_, false) => Button::Release,
            (0, true) => Button::Left,
            (32, true) => Button::Drag,
            (64, true) => Button::WheelUp,
            (65, true) => Button::WheelDown,
            _ => Button::Other,
        };
        Mouse {
            button,
            column,
            row,
        }
    }
}

impl Input {
    pub fn parse(bytes: &str) -> Vec<Input> {
        let mut inputs = Vec::new();
        let mut rest = bytes;

        while !rest.is_empty() {
            if let Some(body) = rest.strip_prefix(PASTE_START) {
                let end = body.find(PASTE_END).unwrap_or(body.len());
                inputs.push(Input::Paste(body[..end].to_string()));
                rest = body.get(end + PASTE_END.len()..).unwrap_or_default();
                continue;
            }
            if let Some((mouse, length)) = sgr(rest) {
                inputs.push(Input::Mouse(mouse));
                rest = &rest[length..];
                continue;
            }
            let next = ["\x1b[<", PASTE_START]
                .iter()
                .filter_map(|marker| rest[1..].find(marker).map(|at| at + 1))
                .min()
                .unwrap_or(rest.len());
            inputs.extend(Key::parse(&rest[..next]).into_iter().map(Input::Key));
            rest = &rest[next..];
        }
        inputs
    }
}

const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

fn sgr(bytes: &str) -> Option<(Mouse, usize)> {
    let body = bytes.strip_prefix("\x1b[<")?;
    let end = body.find(['M', 'm'])?;
    let numbers: Vec<u16> = body[..end]
        .split(';')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let [code, column, row] = numbers[..] else {
        return None;
    };
    let pressed = body.as_bytes()[end] == b'M';
    Some((Mouse::from_sgr(code, column, row, pressed), 3 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bracketed_paste_arrives_whole() {
        assert_eq!(
            Input::parse("a\x1b[200~select *\nfrom t\x1b[201~b"),
            [
                Input::Key(Key::Char('a')),
                Input::Paste("select *\nfrom t".to_string()),
                Input::Key(Key::Char('b')),
            ]
        );
    }

    #[test]
    fn parses_sgr_mouse_sequences_between_keys() {
        assert_eq!(
            Input::parse("j\x1b[<0;10;5M\x1b[<0;10;5mk\x1b[<65;3;4M"),
            [
                Input::Key(Key::Char('j')),
                Input::Mouse(Mouse {
                    button: Button::Left,
                    column: 10,
                    row: 5
                }),
                Input::Mouse(Mouse {
                    button: Button::Release,
                    column: 10,
                    row: 5
                }),
                Input::Key(Key::Char('k')),
                Input::Mouse(Mouse {
                    button: Button::WheelDown,
                    column: 3,
                    row: 4
                }),
            ]
        );
    }
}
