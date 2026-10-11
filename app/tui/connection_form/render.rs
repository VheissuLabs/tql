use super::*;

impl ConnectionForm {
    pub fn draw(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        match &self.picker {
            Some(picker) => draw_picker(picker, buffer, frame_width, frame_lines),
            None => self.draw_form(buffer, frame_width, frame_lines),
        }
    }

    fn draw_form(&self, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
        let lines = self.lines();
        let modal_width = lines.first().map(Line::width).unwrap_or_default() as u16;
        let left = centred(frame_width, modal_width);
        let top = frame_lines.saturating_sub(lines.len() as u16) / 2;
        let visible = (lines.len() as u16).min(frame_lines.saturating_sub(top));
        Island {
            bare: true,
            content: lines,
            ..Island::placed(left, top + 1, modal_width, visible)
        }
        .draw(buffer);
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let fields = self.fields();
        let label_width = fields
            .iter()
            .map(|(_, label)| label.chars().count())
            .max()
            .unwrap_or_default();
        let inner = label_width + VALUE_WIDTH + 5;
        let edge = theme::border(true, false);
        let row = |spans: Vec<Span<'static>>| -> Line<'static> {
            let used: usize = spans.iter().map(Span::width).sum();
            let mut all = vec![Span::styled("│", edge)];
            all.extend(spans);
            all.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
            all.push(Span::styled("│", edge));
            Line::from(all)
        };

        let title = if self.creating {
            " NEW CONNECTION "
        } else {
            " EDIT CONNECTION "
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled("╭─", edge),
                Span::styled(title, theme::title(true, false)),
                Span::styled(
                    format!("{}╮", "─".repeat(inner.saturating_sub(width(title) + 1))),
                    edge,
                ),
            ]),
            row(Vec::new()),
        ];

        let current = self.current_key();
        for (key, label) in fields {
            if self.starts_group(key) && !Self::group(key).is_empty() {
                lines.push(row(Vec::new()));
            }
            let focused = key == current;
            let label_style = if focused { theme::bold() } else { theme::dim() };
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(pad(label, label_width), label_style),
                Span::raw("  "),
            ];
            if key == "tag"
                && let Some(color) = tag::color_of(Some(self.value("tag")))
            {
                spans.push(Span::styled("●", theme::paint(color)));
                spans.push(Span::raw(" "));
            }
            if focused && self.editor.is_some() {
                spans.extend(typing(&self.buffer(), self.cursor(), VALUE_WIDTH));
            } else {
                let shown = truncate(&self.display(key), VALUE_WIDTH);
                let shown = if focused && self.choices(key).is_some() {
                    format!("← {shown} →")
                } else {
                    shown
                };
                let style = if self.is_placeholder(key) && !focused {
                    theme::dim()
                } else {
                    Style::new()
                };
                spans.push(Span::styled(shown, style));
            }
            lines.push(row(spans));
        }

        lines.push(row(Vec::new()));
        let hint = match &self.error {
            Some(error) => Span::styled(
                error.clone(),
                theme::paint("red").add_modifier(Modifier::DIM),
            ),
            None => Span::styled(
                if self.editor.is_some() {
                    "↵ keeps it    esc drops it"
                } else if self.choices(current).is_some() {
                    "← → changes it    ctrl+s save    esc cancel"
                } else {
                    "↵ change    ctrl+s save    esc cancel"
                },
                theme::dim(),
            ),
        };
        lines.push(row(vec![Span::raw("  "), hint]));
        lines.push(Line::styled(format!("╰{}╯", "─".repeat(inner)), edge));
        lines
    }
}

fn draw_picker(picker: &Choice, buffer: &mut Buffer, frame_width: u16, frame_lines: u16) {
    let box_width = frame_width.saturating_sub(6).min(PICKER_WIDTH);
    let box_height = picker.rows();
    let top = frame_lines.saturating_sub(box_height) / 2;
    let left = centred(frame_width, box_width);

    let first = top.saturating_sub(1);
    let last = (top + box_height).min(frame_lines.saturating_sub(1));
    Island {
        bare: true,
        ..Island::placed(
            left.saturating_sub(2).max(1),
            first + 1,
            box_width + 4,
            last.saturating_sub(first) + 1,
        )
    }
    .draw(buffer);

    let content = picker.content(
        box_width.saturating_sub(2) as usize,
        box_height.saturating_sub(2) as usize,
    );
    let visible = (content.len() as u16 + 2).min(frame_lines.saturating_sub(top));
    Island {
        title: picker.title.clone(),
        focused: true,
        modal: true,
        content,
        ..Island::placed(left, top + 1, box_width, visible)
    }
    .draw(buffer);
}

fn centred(frame_width: u16, box_width: u16) -> u16 {
    ((frame_width as i32 - box_width as i32) / 2 + 1).max(1) as u16
}

fn typing(text: &str, cursor: usize, room: usize) -> Vec<Span<'static>> {
    let characters: Vec<char> = text.chars().collect();
    let start = (cursor + 1).saturating_sub(room);
    let mut visible: Vec<char> = characters.iter().skip(start).take(room).copied().collect();
    if start > 0
        && let Some(first) = visible.first_mut()
    {
        *first = '…';
    }
    let at = cursor - start;
    let before: String = visible.iter().take(at).collect();
    let under = visible.get(at).map_or(" ".to_string(), char::to_string);
    let after: String = visible.iter().skip(at + 1).collect();
    vec![
        Span::raw(before),
        Span::styled(under, Style::new().add_modifier(Modifier::REVERSED)),
        Span::raw(after),
    ]
}
