use super::*;

impl Browser {
    pub(super) fn offers(&self) -> Vec<(String, &'static str)> {
        let key = |action, which| keymap::key(action, which);
        if self.problem.is_some() {
            return vec![("esc".into(), "Close")];
        }
        if self.databases.is_some() {
            return vec![
                ("↑↓".into(), "Move"),
                ("↵".into(), "Choose"),
                ("esc".into(), "Cancel"),
            ];
        }
        if self.command.is_some() {
            return vec![("↵".into(), "Run"), ("esc".into(), "Cancel")];
        }
        if self.filtering {
            return vec![("↵".into(), "Keep"), ("esc".into(), "Clear")];
        }
        if self.mode == Mode::Help {
            return vec![("j k".into(), "Scroll"), ("esc".into(), "Close")];
        }
        let database_offer = if self.connection.driver == "sqlite" {
            String::new()
        } else {
            key(Action::Databases, 0)
        };
        if self.focus == Focus::Sidebar {
            return vec![
                ("↵".into(), "Open"),
                (key(Action::FilterTables, 0), "Filter"),
                (key(Action::ToggleTables, 0), "Hide"),
                (
                    format!("{}{}", key(Action::Narrow, 1), key(Action::Widen, 1)),
                    "Width",
                ),
                (database_offer, "Database"),
                (key(Action::FocusRows, 0), "Rows"),
                (key(Action::Help, 0), "Help"),
            ];
        }

        let mut offers = Vec::new();
        if self.has_more || self.offset > 0 {
            offers.push((
                format!(
                    "{}/{}",
                    key(Action::NextPage, 0),
                    key(Action::PreviousPage, 0)
                ),
                "Page",
            ));
        }
        offers.push((key(Action::SortColumn, 0), "Sort"));
        offers.push((key(Action::Help, 0), "Help"));
        offers
    }

    pub(super) fn status_line(&self, width: usize) -> Line<'static> {
        if let Some(command) = &self.command {
            let hint = if command == "r" {
                "  reloads the table"
            } else {
                ""
            };
            return Line::from(vec![
                Span::raw(format!(" :{command}")),
                Span::raw("█"),
                Span::styled(hint, theme::dim()),
            ]);
        }
        if self.filtering {
            return Line::from(vec![
                Span::raw(format!(" /{}", self.filter)),
                Span::raw("█"),
            ]);
        }
        if self.mode == Mode::Help {
            return Line::from(vec![
                Span::raw(" "),
                Span::styled("help", theme::bold()),
                Span::styled("   ? or esc closes", theme::dim()),
            ]);
        }

        let name = match self.connection.tag_color().and_then(theme::named) {
            Some(color) => Span::styled(self.connection.name.clone(), theme::bold().fg(color)),
            None => Span::styled(self.connection.name.clone(), theme::dim()),
        };
        let tag = self
            .connection
            .tag
            .as_ref()
            .map(|tag| Span::styled(format!(" [{tag}]"), theme::dim()))
            .unwrap_or_default();
        let position = if self.headers.is_empty() {
            String::new()
        } else {
            format!(" · col {}/{}", self.column_index + 1, self.headers.len())
        };
        let left = Line::from(vec![
            Span::raw(" "),
            name,
            tag,
            Span::styled(position, theme::dim()),
        ]);

        let Some(said) = self.status.as_deref().filter(|said| !said.is_empty()) else {
            return left;
        };
        let room = width.saturating_sub(1);
        let said = if text::width(said) > room.saturating_sub(2) {
            truncate(said, room.saturating_sub(2))
        } else {
            said.to_string()
        };
        let style = if self.status_fresh {
            theme::fresh()
        } else {
            theme::dim()
        };
        let space = width.saturating_sub(text::width(&said) + 1);
        let left_text = left.to_string();
        let left = if text::width(&left_text) > space.saturating_sub(2) {
            Line::raw(truncate(&left_text, space.saturating_sub(2)))
        } else {
            left
        };
        let gap = space.saturating_sub(left.width()).max(1);

        let mut spans = left.spans;
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(said, style));
        spans.push(Span::raw(" "));
        Line::from(spans)
    }

    pub(super) fn sidebar(&self, tables_width: usize, frame_height: u16) -> Island {
        let mut island = Island::placed(1, 1, tables_width as u16 + 2, frame_height);
        island.focused = self.focus == Focus::Sidebar;

        let name = match self.connection.driver.as_str() {
            "sqlite" => Path::new(self.connection.database.as_deref().unwrap_or_default())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            _ => self.active_database.clone().unwrap_or_default(),
        };
        let name = if name.trim().is_empty() {
            "TABLES".to_string()
        } else {
            name
        };
        let filter = if self.filter.is_empty() {
            String::new()
        } else {
            format!(" /{}", self.filter)
        };
        let pane = "[1] ";
        let room = tables_width.saturating_sub(4 + pane.len());
        island.title = format!(
            "{pane}{}{filter}",
            truncate(&name, room.saturating_sub(text::width(&filter)).max(4))
        );

        let inner_width = island.inner_width();
        let inner_height = island.inner_height();
        let tables = self.visible_tables();
        island.content = if tables.is_empty() {
            vec![Line::styled(" nothing matches", theme::dim())]
        } else {
            let start = window(self.table_index, tables.len(), inner_height);
            tables
                .iter()
                .enumerate()
                .skip(start)
                .take(inner_height)
                .map(|(position, table)| {
                    let label = format!(" {}", truncate(table, inner_width.saturating_sub(2)));
                    if position == self.table_index {
                        Line::styled(pad(&label, inner_width), theme::selection())
                    } else {
                        Line::styled(label, theme::dim())
                    }
                })
                .collect()
        };
        island
    }

    pub(super) fn grid(&mut self, x: u16, y: u16, width: u16, height: u16) -> Island {
        let mut island = Island::placed(x, y, width, height);
        island.focused = self.focus == Focus::Grid;
        island.title = format!(
            "[2] {}",
            self.current_table().unwrap_or_else(|| "ROWS".to_string())
        );

        if self.pending.is_some() {
            island.content = vec![Line::styled(" loading…", theme::dim())];
            return island;
        }

        let grid = Grid {
            headers: &self.headers,
            rows: &self.rows,
            row_index: self.row_index,
            column_index: self.column_index,
            column_offset: self.column_offset,
            sort: self.sort.as_ref(),
            focused: island.focused,
            overrides: &self.width_overrides,
        };
        let (offset, widths, content) = grid.lines(island.inner_width(), island.inner_height());
        self.column_offset = offset;
        self.hits.widths = widths.clone();
        self.hits.row_start = window(
            self.row_index,
            self.rows.len(),
            island.inner_height().saturating_sub(2).max(1),
        );
        if !self.headers.is_empty() {
            island.joins = grid::joins(&widths);
            island.rules = vec![1];
        }
        island.content = content;
        island
    }

    pub(super) fn draw(&mut self, frame: &mut Frame) {
        if chrome::too_small(frame) {
            return;
        }

        let area = frame.area();
        let (hotkey_bar, sql_always, sql_at_bottom) = settings::with(|settings| {
            (
                settings.hotkey_bar,
                settings.sql_always,
                settings.sql_at_bottom,
            )
        });
        let chrome_lines = if hotkey_bar { 2 } else { 1 };
        let frame_height = area
            .height
            .saturating_sub(chrome_lines + BOTTOM_MARGIN)
            .max(6);
        let tables_width = self
            .tables_width()
            .min((area.width as usize).saturating_sub(30).max(8));

        let sidebar = self.sidebar(tables_width, frame_height);
        self.hits = Hits {
            sidebar: (!self.tables_hidden).then(|| Area::of(&sidebar)),
            sidebar_start: window(
                self.table_index,
                self.visible_tables().len(),
                sidebar.inner_height(),
            ),
            ..Hits::default()
        };
        let right_x = if self.tables_hidden {
            1
        } else {
            sidebar.x + sidebar.width + 1
        };
        let right_width = area.width.saturating_sub(right_x).max(20);

        let editor_height = if sql_always {
            settings::with(|settings| settings.sql_height(frame_height as usize)) as u16
        } else {
            0
        };
        let table_height = frame_height - editor_height;
        let (table_y, editor_y) = if sql_at_bottom {
            (1, 1 + table_height)
        } else {
            (1 + editor_height, 1)
        };
        let grid = self.grid(right_x, table_y, right_width, table_height.max(5));

        if !self.tables_hidden {
            sidebar.draw(frame.buffer_mut());
        }
        grid.draw(frame.buffer_mut());

        self.hits.table = Some(Area::of(&grid));
        if sql_always {
            let mut editor = Island::placed(right_x, editor_y, right_width, editor_height);
            self.hits.editor = Some(Area::of(&editor));
            editor.title = "[3] SQL".to_string();
            editor.content = sql::lines(
                self.last_statement.as_deref().unwrap_or_default(),
                editor.inner_width(),
                editor.inner_height(),
            );
            editor.draw(frame.buffer_mut());
        }

        if self.mode == Mode::Help {
            let help_width = help::width_for(area.width.saturating_sub(4) as usize) as u16;
            let height = frame_height
                .saturating_sub(2)
                .min(help::natural_height(help_width.saturating_sub(2) as usize) as u16 + 2);
            let (content, hidden) = help::content(
                help_width.saturating_sub(2) as usize,
                height.saturating_sub(2) as usize,
                self.help_offset,
            );
            let island = Island {
                title: "HELP".to_string(),
                content,
                ..Island::default()
            };
            self.help_hidden = hidden;
            Modal::new(area.width, 1, frame_height, help_width).draw(
                frame.buffer_mut(),
                island,
                height,
            );
        }

        if let Some(choice) = &self.databases {
            let width = area.width.saturating_sub(4).min(choice::WIDTH);
            let height = choice.rows();
            let island = Island {
                title: choice.title.clone(),
                content: choice.content(
                    width.saturating_sub(2) as usize,
                    height.saturating_sub(2) as usize,
                ),
                ..Island::default()
            };
            Modal::new(area.width, 1, frame_height, width).draw(frame.buffer_mut(), island, height);
        }

        if let Some(problem) = &self.problem {
            let island = Island {
                title: "ERROR".to_string(),
                content: vec![Line::styled(format!(" {problem}"), theme::problem())],
                ..Island::default()
            };
            Modal::new(
                area.width,
                1,
                frame_height,
                area.width.saturating_sub(4).min(70),
            )
            .draw(frame.buffer_mut(), island, 3);
        }

        if hotkey_bar {
            chrome::line_at(
                frame,
                frame_height,
                chrome::hotkeys(&self.offers(), area.width as usize),
            );
        }
        chrome::line_at(
            frame,
            frame_height + chrome_lines - 1,
            self.status_line(area.width as usize),
        );
    }
}
