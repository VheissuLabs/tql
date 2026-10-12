use crate::terminal::prelude::*;

use super::chrome::theme;
use super::text::{pad, truncate, width};
use crate::database::{Column, Index, Link};

pub const WIDTH: usize = 78;

pub struct Structure<'a> {
    pub columns: &'a [Column],
    pub links: &'a [(String, Link)],
    pub indexes: &'a [Index],
    pub primary_key: Option<&'a str>,
}

impl Structure<'_> {
    pub fn content(
        &self,
        inner_width: usize,
        inner_height: usize,
        offset: usize,
    ) -> (Vec<Line<'static>>, usize) {
        let lines = self.lines(inner_width.saturating_sub(4));
        let hidden = lines.len().saturating_sub(inner_height);
        let offset = offset.min(hidden);

        let mut visible: Vec<Line<'static>> =
            lines.into_iter().skip(offset).take(inner_height).collect();
        if hidden > 0 && offset < hidden {
            visible[inner_height - 1] = Line::from(vec![
                Span::raw("  "),
                Span::styled("j / ↓ for more", theme::dim()),
            ]);
        }
        (visible, hidden)
    }

    pub fn natural_width(&self) -> usize {
        let (name, kind) = (self.name_width(), self.type_width());
        let columns = self
            .columns
            .iter()
            .map(|column| name + kind + width(&self.notes(column)) + 4);
        let indexes = self.index_lines().into_iter().map(|line| width(&line) + 2);
        let longest = columns.chain(indexes).max().unwrap_or(0);
        WIDTH.max(longest + 6)
    }

    fn lines(&self, room: usize) -> Vec<Line<'static>> {
        let (name, kind) = (self.name_width(), self.type_width());
        let mut lines = vec![Line::raw("")];

        for column in self.columns {
            let notes_room = room.saturating_sub(name + kind + 4).max(4);
            lines.push(Line::from(vec![
                Span::raw(format!("  {}  ", pad(&truncate(&column.name, name), name))),
                Span::styled(
                    pad(&truncate(&column.type_name, kind), kind),
                    theme::sql("keyword"),
                ),
                Span::raw("  "),
                Span::styled(truncate(&self.notes(column), notes_room), theme::dim()),
            ]));
        }

        if self.indexes.is_empty() {
            return lines;
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("indexes", theme::bold()),
        ]));
        for line in self.index_lines() {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(truncate(&line, room.saturating_sub(2).max(4)), theme::dim()),
            ]));
        }
        lines
    }

    fn name_width(&self) -> usize {
        self.columns
            .iter()
            .map(|column| width(&column.name))
            .fold(2, usize::max)
            .min(28)
    }

    fn type_width(&self) -> usize {
        self.columns
            .iter()
            .map(|column| width(&column.type_name))
            .fold(12, usize::max)
            .min(24)
    }

    fn notes(&self, column: &Column) -> String {
        let mut notes = Vec::new();
        if self.primary_key == Some(column.name.as_str()) {
            notes.push("primary key".to_string());
        }
        if let Some((_, link)) = self.links.iter().find(|(name, _)| *name == column.name) {
            notes.push(format!("→ {}.{}", link.table, link.column));
        }
        if !column.nullable {
            notes.push("not null".to_string());
        }
        if column.auto_increment {
            notes.push("auto".to_string());
        }
        if let Some(default) = column.default.as_deref().filter(|value| !value.is_empty()) {
            notes.push(format!("default {default}"));
        }
        notes.join("  ·  ")
    }

    fn index_lines(&self) -> Vec<String> {
        self.indexes
            .iter()
            .map(|index| {
                let columns = index.columns.join(", ");
                let notes: Vec<&str> = [(index.primary, "primary"), (index.unique, "unique")]
                    .into_iter()
                    .filter_map(|(applies, note)| applies.then_some(note))
                    .collect();
                let name = if index.name.is_empty() {
                    &columns
                } else {
                    &index.name
                };
                let notes = if notes.is_empty() {
                    String::new()
                } else {
                    format!("  {}", notes.join(" "))
                };
                format!("{name}  ({columns}){notes}")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn albums() -> (Vec<Column>, Vec<(String, Link)>, Vec<Index>) {
        let column = |name: &str, type_name: &str, nullable: bool| Column {
            name: name.to_string(),
            type_name: type_name.to_string(),
            nullable,
            ..Column::default()
        };
        (
            vec![
                Column {
                    auto_increment: true,
                    ..column("id", "integer", true)
                },
                Column {
                    default: Some("'untitled'".to_string()),
                    ..column("title", "text", false)
                },
                column("artist_id", "integer", true),
            ],
            vec![(
                "artist_id".to_string(),
                Link {
                    table: "artists".into(),
                    column: "id".to_string(),
                },
            )],
            vec![Index {
                name: "albums_artist_id".to_string(),
                columns: vec!["artist_id".to_string()],
                ..Index::default()
            }],
        )
    }

    #[test]
    fn notes_keys_links_nullability_and_defaults() {
        let (columns, links, indexes) = albums();
        let structure = Structure {
            columns: &columns,
            links: &links,
            indexes: &indexes,
            primary_key: Some("id"),
        };

        let (lines, hidden) = structure.content(76, 20, 0);
        let text: Vec<String> = lines.iter().map(|line| line.to_string()).collect();

        assert_eq!(hidden, 0);
        assert_eq!(
            text[1].trim_end(),
            "  id         integer       primary key  ·  auto"
        );
        assert_eq!(
            text[2].trim_end(),
            "  title      text          not null  ·  default 'untitled'"
        );
        assert_eq!(
            text[3].trim_end(),
            "  artist_id  integer       → artists.id"
        );
        assert_eq!(text[6], "    albums_artist_id  (artist_id)");
    }

    #[test]
    fn says_there_is_more_when_the_list_runs_past_the_box() {
        let (columns, links, indexes) = albums();
        let structure = Structure {
            columns: &columns,
            links: &links,
            indexes: &indexes,
            primary_key: None,
        };

        let (lines, hidden) = structure.content(76, 4, 0);

        assert_eq!(hidden, 3);
        assert_eq!(lines[3].to_string(), "  j / ↓ for more");

        let (lines, _) = structure.content(76, 4, 9);
        assert_eq!(lines[3].to_string(), "    albums_artist_id  (artist_id)");
    }
}
