use super::*;

impl Browser {
    pub(in crate::tui::browser) fn pending_offers(&self) -> Vec<(String, &'static str)> {
        if !self.edits.pending() {
            return Vec::new();
        }
        vec![
            (":w".into(), "Write"),
            (keymap::key(Action::ClearMarks, 0), "Undo"),
        ]
    }

    pub(in crate::tui::browser) fn edit_offers(&self) -> Vec<(String, &'static str)> {
        vec![
            (keymap::key(Action::EditValue, 0), "Edit"),
            (keymap::key(Action::NewRow, 0), "Add"),
            (keymap::key(Action::MarkDelete, 0), "Mark"),
        ]
    }

    pub(in crate::tui::browser) fn editing_offers(&self) -> Option<Vec<(String, &'static str)>> {
        let offers: &[(&str, &'static str)] = match (&self.edits.form, &self.edits.value) {
            (Some(form), _) if form.editor.is_some() => {
                &[("↵", "Keep"), ("tab", "Next"), ("esc", "Put back")]
            }
            (Some(_), _) => &[
                ("↑↓", "Field"),
                ("↵", "Edit"),
                ("ctrl+s", "Keep row"),
                ("esc", "Cancel"),
            ],
            (None, Some(value)) if value.editable() => {
                &[("↵", "Keep"), ("⇧↵", "New line"), ("esc", "Cancel")]
            }
            (None, Some(_)) => &[
                ("V", "Select"),
                ("y", "Yank"),
                ("e", "Edit"),
                ("esc", "Close"),
            ],
            (None, None) => return None,
        };
        Some(
            offers
                .iter()
                .map(|(key, label)| (key.to_string(), *label))
                .collect(),
        )
    }

    pub(in crate::tui::browser) fn editing_status(&self, width: usize) -> Option<Line<'static>> {
        let (heading, hint) = match (&self.edits.form, &self.edits.value) {
            (Some(form), _) => (
                if form.adds() {
                    "new row"
                } else {
                    "editing row"
                }
                .to_string(),
                format!(
                    "   ctrl+s keeps it pending    :w writes it{}",
                    if form.expanded {
                        "    ⇧↵ adds a line"
                    } else {
                        ""
                    }
                ),
            ),
            (None, Some(value)) => self.value_status(value),
            (None, None) => return None,
        };
        let plain = format!(" {heading}{hint}");
        if text::width(&plain) > width {
            return Some(Line::raw(truncate(&plain, width)));
        }
        Some(Line::from(vec![
            Span::raw(" "),
            Span::styled(heading, theme::bold()),
            Span::styled(hint, theme::dim()),
        ]))
    }

    pub(super) fn value_status(&self, value: &ValueEditor) -> (String, String) {
        let column = self
            .headers
            .get(self.column_index)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        if !value.editable() && value.anchor.is_some() {
            let (from, to) = value.selected_lines();
            return (
                "visual".to_string(),
                format!("   lines {}-{}    y yanks    esc clears", from + 1, to + 1),
            );
        }
        if !value.editable() {
            return (
                format!("viewing {column}"),
                "   j/k moves    V selects    y yanks    g/G top/bottom    esc closes".to_string(),
            );
        }
        let time = if now::suits(value.column_type.as_deref()) {
            format!("    ctrl+t now {}", now::label())
        } else {
            String::new()
        };
        let json = if value.json {
            "    json is validated"
        } else {
            ""
        };
        (
            format!("editing {column}"),
            format!("   ↵ keeps it{time}    ⇧↵ adds a line    esc cancels{json}"),
        )
    }

    pub(in crate::tui::browser) fn pending_callout(&self) -> String {
        if self.edits.pending() {
            self.edits.status()
        } else {
            String::new()
        }
    }

    pub(in crate::tui::browser) fn pending_style(&self) -> Option<Style> {
        let (key, fallback) = match (self.edits.deletes.is_empty(), self.edits.updates.is_empty()) {
            (false, _) => ("deleted", "red"),
            (true, false) => ("edited", "yellow"),
            (true, true) => return None,
        };
        Some(theme::paint(&settings::with(|settings| {
            settings.color(key, fallback)
        })))
    }

    pub(in crate::tui::browser) fn callout_style(&self) -> Style {
        let (key, fallback) = if self.edits.deletes.is_empty() {
            ("edited", "yellow")
        } else {
            ("deleted", "red")
        };
        theme::bold().patch(theme::paint(&settings::with(|settings| {
            settings.color(key, fallback)
        })))
    }

    pub(in crate::tui::browser) fn draw_edits(&mut self, frame: &mut Frame, frame_height: u16) {
        let frame_width = frame.area().width;
        let room = frame_width.saturating_sub(4) as usize;
        self.edits.viewer = None;

        if let Some(value) = &self.edits.value {
            let title = if value.editable() {
                value.column.clone()
            } else {
                format!("{}  ·  read-only", value.column)
            };
            let view = ValueView {
                title,
                buffer: &value.buffer,
                json: value.json,
                show_cursor: value.editable(),
                selection: value.anchor.map(|_| value.selected_lines()),
            };
            let area = draw_view(frame, &view, room, frame_height);
            self.edits.viewer = Some((area, view.first_line(area.height as usize - 2)));
        }

        if let Some(form) = self.edits.form.as_mut() {
            let width = room.min(form.natural_width()) as u16;
            let height = frame_height.saturating_sub(2).min(form.rows() as u16);
            let island = Island {
                title: form.title(),
                content: form.content(width as usize - 2, height as usize - 2),
                ..Island::default()
            };
            let editing_expanded = form.expanded && form.editor.is_some();
            Modal::new(frame_width, 1, frame_height, width).draw_focused(
                frame.buffer_mut(),
                island,
                height,
                !editing_expanded,
            );

            if form.expanded
                && let Some(editor) = &form.editor
            {
                let view = ValueView {
                    title: form.current().name.clone(),
                    buffer: editor,
                    json: form.json,
                    show_cursor: true,
                    selection: None,
                };
                draw_view(frame, &view, room, frame_height);
            }
        }
    }

    pub(in crate::tui::browser) fn mouse_in_edits(&mut self, mouse: Mouse, row: u16) {
        let viewer = self.edits.viewer;
        let Some(value) = self.edits.value.as_mut().filter(|value| !value.editable()) else {
            return;
        };
        let line = viewer.and_then(|(area, first)| {
            let local = area.local_row(row);
            (local >= 0 && local < area.height as isize - 2)
                .then_some(first + local as usize)
                .filter(|line| *line < value.buffer.lines().len())
        });

        match (mouse.button, line) {
            (Button::WheelUp, _) => value.buffer.handle(Key::Up),
            (Button::WheelDown, _) => value.buffer.handle(Key::Down),
            (Button::Drag, Some(line)) => {
                value.anchor = value.anchor.or(Some(value.buffer.cursor_line()));
                value.buffer.move_to_line_column(line, 0);
            }
            (Button::Left, Some(line)) => {
                value.anchor = None;
                value.buffer.move_to_line_column(line, 0);
            }
            _ => {}
        }
    }
}

fn draw_view(frame: &mut Frame, view: &ValueView, room: usize, frame_height: u16) -> Area {
    let frame_width = frame.area().width;
    let width = room.min(view.natural_width()) as u16;
    let height = frame_height.saturating_sub(2).min(view.rows() as u16);
    let island = view.island(width as usize - 2, height as usize - 2);
    Modal::new(frame_width, 1, frame_height, width).draw(frame.buffer_mut(), island, height);
    Area {
        x: (frame_width.saturating_sub(width) / 2 + 1).max(1),
        y: 1 + frame_height.saturating_sub(height) / 2,
        width,
        height,
    }
}
