use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use crate::terminal::{Button, Mouse, Tui};

#[derive(Default)]
pub struct Margin {
    below: Option<Terminal<TestBackend>>,
}

impl Margin {
    pub fn draw<App: Tui>(&mut self, app: &mut App, frame: &mut Frame) {
        let area = frame.area();
        let rows = app.top_margin().min(area.height.saturating_sub(1));
        if rows == 0 {
            app.render(frame);
            return;
        }

        let room = Rect::new(0, 0, area.width, area.height - rows);
        let below = self.below.get_or_insert_with(|| {
            Terminal::new(TestBackend::new(room.width, room.height))
                .expect("test backend never fails")
        });
        if below.backend().buffer().area != room {
            below.backend_mut().resize(room.width, room.height);
        }
        below
            .draw(|inner| app.render(inner))
            .expect("test backend never fails");

        let backend = below.backend();
        let drawn = backend.buffer();
        let buffer = frame.buffer_mut();
        for y in 0..room.height {
            for x in 0..room.width {
                buffer[(x, y + rows)] = drawn[(x, y)].clone();
            }
        }
        if backend.cursor_visible() {
            let cursor = backend.cursor_position();
            frame.set_cursor_position((cursor.x, cursor.y + rows));
        }
    }
}

// A click in the blank rows above the frame lands on nothing, but it is still
// something done, so the app hears of it as a button that does nothing.
pub fn below(mouse: Mouse, rows: u16) -> Mouse {
    match mouse.row.checked_sub(rows) {
        Some(row) => Mouse { row, ..mouse },
        None => Mouse {
            button: Button::Other,
            row: 0,
            ..mouse
        },
    }
}
