use super::*;
use crate::tui::structure::Structure;

#[derive(Debug, Default)]
pub struct StructureView {
    pub offset: usize,
    hidden: usize,
}

impl Browser {
    pub(super) fn toggle_structure(&mut self) {
        if self.current_table().is_none() {
            return;
        }
        self.structure = match self.structure {
            Some(_) => None,
            None => Some(StructureView::default()),
        };
    }

    pub(super) fn type_into_structure(&mut self, key: Key) {
        let Some(view) = self.structure.as_mut() else {
            return;
        };
        match key {
            Key::Escape | Key::Char('q' | 't') => self.structure = None,
            Key::Down | Key::Char('j') => view.offset = (view.offset + 1).min(view.hidden),
            Key::Up | Key::Char('k') => view.offset = view.offset.saturating_sub(1),
            Key::Char('g') => view.offset = 0,
            Key::Char('G') => view.offset = view.hidden,
            _ => {}
        }
    }

    pub(super) fn draw_structure(&mut self, frame: &mut Frame, frame_height: u16) {
        let Some(offset) = self.structure.as_ref().map(|view| view.offset) else {
            return;
        };
        let table = self.current_table().unwrap_or_default();
        let database = self.database.as_mut();
        let columns = self.schema.columns(database, &table);
        let links = self.schema.links(database, &table);
        let indexes = self.schema.indexes(database, &table);
        let primary_key = self.schema.primary_key(database, &table);
        let structure = Structure {
            columns: &columns,
            links: &links,
            indexes: &indexes,
            primary_key: primary_key.as_deref(),
        };

        let area = frame.area();
        let width = (area.width as usize)
            .saturating_sub(4)
            .min(structure.natural_width());
        let height = frame_height
            .saturating_sub(2)
            .min(columns.len() as u16 + 9)
            .max(3);
        let (content, hidden) = structure.content(
            width.saturating_sub(2),
            height.saturating_sub(2) as usize,
            offset,
        );
        if let Some(view) = self.structure.as_mut() {
            view.hidden = hidden;
        }

        let island = Island {
            title: format!("STRUCTURE  ·  {table}"),
            content,
            ..Island::default()
        };
        Modal::new(area.width, 1, frame_height, width as u16).draw(
            frame.buffer_mut(),
            island,
            height,
        );
    }
}
