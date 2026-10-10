mod fake;
mod key;
mod margin;
mod mouse;

use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind,
};
use ratatui::crossterm::execute;

pub use fake::Fake;
pub use key::Key;
pub use mouse::{Button, Input, Mouse};

const TICK: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Default,
    Block,
    Bar,
    Underline,
}

impl CursorShape {
    fn style(self) -> SetCursorStyle {
        match self {
            CursorShape::Default => SetCursorStyle::DefaultUserShape,
            CursorShape::Block => SetCursorStyle::SteadyBlock,
            CursorShape::Bar => SetCursorStyle::SteadyBar,
            CursorShape::Underline => SetCursorStyle::SteadyUnderScore,
        }
    }
}

pub enum Flow<Output> {
    Continue,
    Exit(Output),
}

pub trait Tui {
    type Output;

    fn on_key(&mut self, key: Key) -> Flow<Self::Output>;

    fn render(&mut self, frame: &mut Frame);

    fn on_mouse(&mut self, _mouse: Mouse) -> Flow<Self::Output> {
        Flow::Continue
    }

    fn wants_mouse(&self) -> bool {
        false
    }

    fn cursor_shape(&self) -> CursorShape {
        CursorShape::Default
    }

    fn top_margin(&self) -> u16 {
        0
    }

    fn take_repaint(&mut self) -> bool {
        false
    }

    fn on_paste(&mut self, text: &str) -> Flow<Self::Output> {
        for character in text.chars() {
            let key = match character {
                '\n' | '\r' => Key::Enter,
                '\t' => Key::Tab,
                other => Key::Char(other),
            };
            if let Flow::Exit(output) = self.on_key(key) {
                return Flow::Exit(output);
            }
        }
        Flow::Continue
    }

    fn idle(&mut self) -> bool {
        false
    }
}

pub fn run<App: Tui>(app: &mut App) -> anyhow::Result<App::Output> {
    Session::start()?.run(app)
}

pub struct Session {
    terminal: ratatui::DefaultTerminal,
}

impl Session {
    pub fn start() -> anyhow::Result<Session> {
        let terminal = ratatui::init();
        let session = Session { terminal };
        execute!(std::io::stdout(), EnableBracketedPaste)?;
        Ok(session)
    }

    pub fn run<App: Tui>(&mut self, app: &mut App) -> anyhow::Result<App::Output> {
        let mouse = app.wants_mouse();
        if mouse {
            execute!(std::io::stdout(), EnableMouseCapture)?;
        }
        let outcome = event_loop(&mut self.terminal, app);
        if mouse {
            let _ = execute!(std::io::stdout(), DisableMouseCapture);
        }
        let _ = execute!(std::io::stdout(), CursorShape::Default.style());
        outcome
    }

    pub fn show<App: Tui>(&mut self, app: &mut App) -> anyhow::Result<()> {
        let mut margin = margin::Margin::default();
        self.terminal.draw(|frame| margin.draw(app, frame))?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), DisableBracketedPaste);
        ratatui::restore();
    }
}

fn event_loop<App: Tui>(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
) -> anyhow::Result<App::Output> {
    let mut dirty = true;
    let mut shape = CursorShape::Default;
    let mut margin = margin::Margin::default();

    loop {
        while event::poll(Duration::ZERO)? {
            dirty = true;
            let mut inputs = vec![input(event::read()?)];
            if matches!(inputs[0], Some(Input::Key(Key::Escape))) && event::poll(SPLIT_ESCAPE)? {
                let next = input(event::read()?);
                inputs = match joined(&next) {
                    Some(key) => vec![Some(Input::Key(key))],
                    None => vec![inputs.remove(0), next],
                };
            }
            for input in inputs.into_iter().flatten() {
                if let Flow::Exit(output) = dispatch(app, input) {
                    return Ok(output);
                }
            }
        }

        if dirty {
            if app.take_repaint() {
                terminal.clear()?;
            }
            terminal.draw(|frame| margin.draw(app, frame))?;
            dirty = false;
            if app.cursor_shape() != shape {
                shape = app.cursor_shape();
                execute!(std::io::stdout(), shape.style())?;
            }
            continue;
        }

        if app.idle() {
            dirty = true;
            continue;
        }

        event::poll(TICK)?;
    }
}

// A terminal multiplexer can split alt+h into escape and h a moment apart;
// a key that follows escape this closely was meant with it.
const SPLIT_ESCAPE: Duration = Duration::from_millis(30);

fn joined(next: &Option<Input>) -> Option<Key> {
    match next {
        Some(Input::Key(Key::Char(character))) => Some(Key::Alt(*character)),
        Some(Input::Key(Key::Enter)) => Some(Key::ShiftEnter),
        _ => None,
    }
}

fn dispatch<App: Tui>(app: &mut App, input: Input) -> Flow<App::Output> {
    match input {
        Input::Key(key) => app.on_key(key),
        Input::Mouse(mouse) => app.on_mouse(margin::below(mouse, app.top_margin())),
        Input::Paste(text) => app.on_paste(&text),
    }
}

fn input(event: Event) -> Option<Input> {
    match event {
        Event::Key(pressed) if pressed.kind != KeyEventKind::Release => {
            Key::from_event(pressed).map(Input::Key)
        }
        Event::Mouse(mouse) => Some(Input::Mouse(Mouse::from_event(mouse))),
        Event::Paste(text) => Some(Input::Paste(text)),
        _ => None,
    }
}

pub trait CmdTui {
    fn tui<App: Tui>(&mut self, app: &mut App) -> anyhow::Result<App::Output>;
}

impl CmdTui for crate::console::Cmd<'_> {
    fn tui<App: Tui>(&mut self, app: &mut App) -> anyhow::Result<App::Output> {
        run(app)
    }
}

pub mod prelude {
    pub use super::{Button, CmdTui, CursorShape, Flow, Key, Mouse, Session, Tui};
    pub use ratatui::Frame;
    pub use ratatui::layout::Rect;
    pub use ratatui::style::{Color, Modifier, Style, Stylize};
    pub use ratatui::text::{Line, Span, Text};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_close_behind_escape_is_its_alt_key() {
        assert_eq!(
            joined(&Some(Input::Key(Key::Char('h')))),
            Some(Key::Alt('h'))
        );
        assert_eq!(joined(&Some(Input::Key(Key::Enter))), Some(Key::ShiftEnter));
        assert_eq!(joined(&Some(Input::Key(Key::Up))), None);
        assert_eq!(joined(&None), None);
    }
}
