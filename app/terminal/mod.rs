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
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableBracketedPaste)?;
    let mouse = app.wants_mouse();
    if mouse {
        execute!(std::io::stdout(), EnableMouseCapture)?;
    }
    let outcome = event_loop(&mut terminal, app);
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    let _ = execute!(
        std::io::stdout(),
        CursorShape::Default.style(),
        DisableBracketedPaste
    );
    ratatui::restore();
    outcome
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
            let flow = match input(event::read()?) {
                Some(Input::Key(key)) => app.on_key(key),
                Some(Input::Mouse(mouse)) => match margin::below(mouse, app.top_margin()) {
                    Some(mouse) => app.on_mouse(mouse),
                    None => Flow::Continue,
                },
                Some(Input::Paste(text)) => app.on_paste(&text),
                None => Flow::Continue,
            };
            if let Flow::Exit(output) = flow {
                return Ok(output);
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
    pub use super::{Button, CmdTui, CursorShape, Flow, Key, Mouse, Tui};
    pub use ratatui::Frame;
    pub use ratatui::layout::Rect;
    pub use ratatui::style::{Color, Modifier, Style, Stylize};
    pub use ratatui::text::{Line, Span, Text};
}
