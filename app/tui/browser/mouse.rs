use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Area {
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
}

impl Area {
    pub(super) fn of(island: &Island) -> Self {
        Area {
            x: island.x,
            y: island.y,
            width: island.width,
            height: island.height,
        }
    }

    pub(super) fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.x
            && column < self.x + self.width
            && row >= self.y
            && row < self.y + self.height
    }

    pub(super) fn local_row(&self, row: u16) -> isize {
        row as isize - self.y as isize - 1
    }

    pub(super) fn local_column(&self, column: u16) -> isize {
        column as isize - self.x as isize - 1
    }

    pub(super) fn contains_content(&self, column: u16, row: u16) -> bool {
        let (local_row, local_column) = (self.local_row(row), self.local_column(column));
        self.contains(column, row)
            && local_row >= 0
            && local_row < self.height as isize - 2
            && local_column >= 0
            && local_column < self.width as isize - 2
    }
}

#[derive(Debug, Default)]
pub(super) struct Hits {
    pub(super) sidebar: Option<Area>,
    pub(super) sidebar_start: usize,
    pub(super) table: Option<Area>,
    pub(super) row_start: usize,
    pub(super) widths: Vec<usize>,
    pub(super) editor: Option<Area>,
}

#[derive(Debug, Clone)]
pub(super) enum Drag {
    Tables,
    Column { name: String, index: usize },
}

impl Browser {
    pub(super) fn press(&mut self, column: u16, row: u16) {
        if self
            .hits
            .sidebar
            .is_some_and(|area| area.contains(column, row))
        {
            self.focus = Focus::Sidebar;
        } else if self
            .hits
            .table
            .is_some_and(|area| area.contains(column, row))
        {
            self.focus = Focus::Grid;
        }

        if let Some(sidebar) = self.hits.sidebar
            && column == sidebar.x + sidebar.width - 1
            && row >= sidebar.y
            && row < sidebar.y + sidebar.height
        {
            self.drag = Some(Drag::Tables);
            return;
        }

        if let Some(table) = self.hits.table
            && row == table.y + 1
            && column >= table.x
            && column < table.x + table.width
        {
            if let Some(index) = self.handle_near(column)
                && let Some(name) = self.headers.get(index).cloned()
            {
                self.drag = Some(Drag::Column { name, index });
                self.column_index = index;
                return;
            }
            if let Some(index) = self.column_index_for(table.local_column(column))
                && let Some(name) = self.headers.get(index).cloned()
            {
                self.sort_by(name);
            }
            return;
        }

        self.click(column, row);
    }

    pub(super) fn click(&mut self, column: u16, row: u16) {
        if let Some(sidebar) = self.hits.sidebar
            && sidebar.contains_content(column, row)
        {
            let target = self.hits.sidebar_start + sidebar.local_row(row) as usize;
            if target < self.visible_tables().len() {
                self.select_table(target);
            }
            return;
        }

        if let Some(table) = self.hits.table
            && table.contains_content(column, row)
        {
            let target = self.hits.row_start as isize + table.local_row(row) - 2;
            if target < 0 || target as usize >= self.rows.len() {
                return;
            }
            self.focus = Focus::Grid;
            self.row_index = target as usize;
            if let Some(index) = self.column_index_for(table.local_column(column)) {
                self.column_index = index;
            }
            if self.is_double_click() {
                self.say("editing is not ported yet");
            }
            return;
        }

        if self
            .hits
            .editor
            .is_some_and(|editor| editor.contains(column, row))
        {
            self.say("the SQL editor is not ported yet");
        }
    }

    pub(super) fn is_double_click(&mut self) -> bool {
        let window = Duration::from_millis(settings::with(|settings| settings.double_click_ms));
        let here = (self.row_index, self.column_index);
        let double = self
            .last_click
            .is_some_and(|(row, column, at)| (row, column) == here && at.elapsed() <= window);
        self.last_click = if double {
            None
        } else {
            Some((here.0, here.1, Instant::now()))
        };
        double
    }

    pub(super) fn column_index_for(&self, local_column: isize) -> Option<usize> {
        let local = local_column - grid::GUTTER as isize;
        let mut offset = 0isize;
        for (position, width) in self.hits.widths.iter().enumerate() {
            let width = *width as isize;
            if local >= offset && local < offset + width + 2 {
                return Some(self.column_offset + position);
            }
            offset += width + 3;
        }
        None
    }

    pub(super) fn handles(&self) -> Vec<(usize, u16)> {
        let Some(table) = self.hits.table else {
            return Vec::new();
        };
        let mut x = table.x + 1 + grid::GUTTER as u16;
        let mut handles = Vec::new();
        for (position, width) in self.hits.widths.iter().enumerate() {
            x += *width as u16 + 2;
            handles.push((self.column_offset + position, x));
            x += 1;
        }
        handles
    }

    pub(super) fn handle_near(&self, column: u16) -> Option<usize> {
        self.handles()
            .into_iter()
            .find(|(_, x)| column.abs_diff(*x) <= 1)
            .map(|(index, _)| index)
    }

    pub(super) fn cell_start(&self, index: usize) -> Option<u16> {
        let table = self.hits.table?;
        let mut x = table.x + 1 + grid::GUTTER as u16;
        for (position, width) in self.hits.widths.iter().enumerate() {
            if self.column_offset + position == index {
                return Some(x);
            }
            x += *width as u16 + 3;
        }
        None
    }

    pub(super) fn drag_to(&mut self, column: u16) {
        match self.drag.clone() {
            Some(Drag::Tables) => {
                let width = (column as usize).saturating_sub(2).clamp(8, 80);
                self.sidebar_width = Some(width);
                self.say(format!("table list width {width}"));
            }
            Some(Drag::Column { name, index }) => {
                let Some(start) = self.cell_start(index) else {
                    return;
                };
                let width = (column as isize - start as isize - 2).clamp(3, 200) as usize;
                self.width_overrides.insert(name.clone(), width);
                self.say(format!("{name} width {width}"));
            }
            None => {}
        }
    }
}
