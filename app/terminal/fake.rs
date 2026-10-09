use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

use crate::terminal::{Flow, Input, Key, Mouse, Tui};

pub struct Fake<App: Tui> {
    pub app: App,
    terminal: Terminal<TestBackend>,
    exit: Option<App::Output>,
}

impl<App: Tui> Fake<App> {
    pub fn new(app: App) -> Self {
        Self::sized(app, 100, 30)
    }

    pub fn sized(app: App, width: u16, height: u16) -> Self {
        Fake {
            app,
            terminal: Terminal::new(TestBackend::new(width, height))
                .expect("test backend never fails"),
            exit: None,
        }
    }

    pub fn press(&mut self, bytes: &str) -> &mut Self {
        for input in Input::parse(bytes) {
            self.input(input);
            self.settle();
        }
        self
    }

    pub fn burst(&mut self, bytes: &str) -> &mut Self {
        for input in Input::parse(bytes) {
            self.input(input);
        }
        self.settle()
    }

    pub fn click(&mut self, column: u16, row: u16) -> &mut Self {
        self.press(&format!("\x1b[<0;{column};{row}M\x1b[<0;{column};{row}m"))
    }

    pub fn drag(&mut self, from: (u16, u16), to: (u16, u16)) -> &mut Self {
        self.press(&format!(
            "\x1b[<0;{};{}M\x1b[<32;{};{}M\x1b[<0;{};{}m",
            from.0, from.1, to.0, to.1, to.0, to.1
        ))
    }

    pub fn scroll(&mut self, down: bool, column: u16, row: u16) -> &mut Self {
        let code = if down { 65 } else { 64 };
        self.press(&format!("\x1b[<{code};{column};{row}M"))
    }

    pub fn mouse(&mut self, mouse: Mouse) -> &mut Self {
        if self.exit.is_some() {
            return self;
        }
        self.render();
        if let Flow::Exit(output) = self.app.on_mouse(mouse) {
            self.exit = Some(output);
        }
        self
    }

    pub fn paste(&mut self, text: &str) -> &mut Self {
        if self.exit.is_some() {
            return self;
        }
        self.render();
        if let Flow::Exit(output) = self.app.on_paste(text) {
            self.exit = Some(output);
        }
        self.settle()
    }

    fn input(&mut self, input: Input) -> &mut Self {
        match input {
            Input::Key(key) => self.key(key),
            Input::Mouse(mouse) => self.mouse(mouse),
            Input::Paste(text) => self.paste(&text),
        }
    }

    pub fn key(&mut self, key: Key) -> &mut Self {
        if self.exit.is_some() {
            return self;
        }
        self.render();
        if let Flow::Exit(output) = self.app.on_key(key) {
            self.exit = Some(output);
        }
        self
    }

    pub fn resize(&mut self, width: u16, height: u16) -> &mut Self {
        self.terminal.backend_mut().resize(width, height);
        self
    }

    pub fn settle(&mut self) -> &mut Self {
        while self.app.idle() {}
        self
    }

    pub fn screen(&mut self) -> String {
        self.render();
        let buffer = self.terminal.backend().buffer();
        let width = buffer.area.width as usize;

        buffer
            .content
            .chunks(width)
            .map(|row| {
                let mut skip = 0;
                let line: String = row
                    .iter()
                    .filter_map(|cell| {
                        if skip > 0 {
                            skip -= 1;
                            return None;
                        }
                        let symbol = cell.symbol();
                        skip = unicode_extra_width(symbol);
                        Some(symbol)
                    })
                    .collect();
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn highlighted(&mut self) -> Vec<String> {
        self.render();
        let buffer = self.terminal.backend().buffer();
        let width = buffer.area.width as usize;
        let mut runs = Vec::new();

        for row in buffer.content.chunks(width) {
            let mut run = String::new();
            for cell in row {
                if cell.modifier.contains(Modifier::REVERSED) {
                    run.push_str(cell.symbol());
                } else if !run.is_empty() {
                    runs.push(run.trim().to_string());
                    run.clear();
                }
            }
            if !run.is_empty() {
                runs.push(run.trim().to_string());
            }
        }
        runs
    }

    pub fn exit(&self) -> Option<&App::Output> {
        self.exit.as_ref()
    }

    pub fn exited(&self) -> bool {
        self.exit.is_some()
    }

    fn render(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| app.render(frame))
            .expect("test backend never fails");
    }
}

fn unicode_extra_width(symbol: &str) -> usize {
    ratatui::text::Span::raw(symbol).width().saturating_sub(1)
}
