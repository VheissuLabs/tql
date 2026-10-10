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
        let left = app.left_margin().min(area.width.saturating_sub(1));
        let right = app.right_margin().min(area.width.saturating_sub(left + 1));
        let (columns, lines) = app.smallest();
        let too_small = area.width < columns || area.height < lines;
        if too_small || (rows == 0 && left == 0 && right == 0) {
            app.render(frame);
            return;
        }

        let room = Rect::new(0, 0, area.width - left - right, area.height - rows);
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
                buffer[(x + left, y + rows)] = drawn[(x, y)].clone();
            }
        }
        if backend.cursor_visible() {
            let cursor = backend.cursor_position();
            frame.set_cursor_position((cursor.x + left, cursor.y + rows));
        }
    }
}

// A click in the blank rows above the frame, or the blank columns beside it,
// lands on nothing, but it is still something done, so the app hears of it as
// a button that does nothing.
pub fn inside(mouse: Mouse, rows: u16, columns: u16) -> Mouse {
    match (
        mouse.row.checked_sub(rows),
        mouse.column.checked_sub(columns),
    ) {
        (Some(row), Some(column)) if column > 0 || columns == 0 => Mouse {
            row,
            column,
            ..mouse
        },
        _ => Mouse {
            button: Button::Other,
            row: 0,
            column: 0,
        },
    }
}
