use super::*;
use crate::driver::Driver;
use crate::registry::{self, Screen};
use crate::store::Store;
use crate::tui::ago;
use crate::tui::history;
use crate::tui::palette::{Item, Kind, Palette};
use crate::tui::text::sentence;

const SKIPPED: [Action; 8] = [
    Action::MoveUp,
    Action::MoveDown,
    Action::MoveLeft,
    Action::MoveRight,
    Action::Activate,
    Action::Escape,
    Action::Command,
    Action::Palette,
];

fn keys_of(action: Action) -> String {
    let mut glyphs: Vec<String> = Vec::new();
    for key in keymap::binding(action).keys {
        let glyph = keymap::glyph(key);
        if !glyphs.contains(&glyph) {
            glyphs.push(glyph);
        }
    }
    glyphs.join(" / ")
}

impl Browser {
    pub(super) fn open_palette(&mut self) {
        let actions = keymap::bindings()
            .into_iter()
            .filter(|binding| !SKIPPED.contains(&binding.action))
            .map(|binding| {
                Item::new(
                    Kind::Action(binding.action),
                    sentence(binding.description),
                    keys_of(binding.action),
                    binding.description,
                )
            });
        let commands = registry::on(Screen::Browser)
            .filter(|entry| entry.palette)
            .map(|entry| {
                Item::new(
                    Kind::Command(entry.name),
                    sentence(entry.description),
                    format!(":{}", entry.word()),
                    entry.word(),
                )
            });
        let tables = self.tables.iter().map(|table| {
            Item::new(
                Kind::Table,
                table,
                table.schema.as_deref().unwrap_or("table"),
                table,
            )
        });

        let mut items: Vec<Item> = actions.chain(commands).chain(tables).collect();
        items.extend(self.other_databases());
        items.extend(self.other_connections());

        self.palette = Some(Palette::new("COMMANDS", items));
        self.hush();
    }

    fn other_databases(&mut self) -> Vec<Item> {
        if self.connection.driver == Driver::Sqlite {
            return Vec::new();
        }
        let active = self.active_database.clone().unwrap_or_default();
        self.database
            .databases()
            .unwrap_or_default()
            .into_iter()
            .filter(|database| *database != active)
            .map(|database| Item::new(Kind::Database, database.clone(), "database", database))
            .collect()
    }

    fn other_connections(&self) -> Vec<Item> {
        let Some(connections) = self
            .store
            .as_ref()
            .and_then(|store| Store::open(store).ok())
            .and_then(|store| store.connections().ok())
        else {
            return Vec::new();
        };
        let mut others: Vec<_> = connections
            .into_iter()
            .filter(|other| other.id != self.connection.id)
            .collect();
        others.sort_by(|first, second| first.name.cmp(&second.name));
        others
            .into_iter()
            .map(|other| {
                Item::new(
                    Kind::Connection,
                    other.name,
                    "connection",
                    other.id.to_string(),
                )
            })
            .collect()
    }

    pub(super) fn open_history(&mut self) {
        self.command = None;
        let now = ago::now();
        let items: Vec<Item> = self
            .store
            .as_ref()
            .map(|store| history::of(store, self.connection.id))
            .unwrap_or_default()
            .iter()
            .map(|run| {
                Item::new(
                    Kind::History,
                    history::one_line(&run.statement),
                    history::hint(run, now),
                    run.statement.clone(),
                )
            })
            .collect();

        if items.is_empty() {
            self.say("no history yet · statements you run from the SQL editor land here");
            return;
        }
        self.palette = Some(Palette::new("HISTORY", items));
        self.hush();
    }

    pub(super) fn type_into_palette(&mut self, key: Key) -> Flow<Exit> {
        let Some(palette) = self.palette.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => self.palette = None,
            Key::Enter => {
                let chosen = palette.selected();
                self.palette = None;
                if let Some(item) = chosen {
                    return self.run_palette_item(item);
                }
            }
            Key::Up | Key::Ctrl('p') => palette.move_by(false),
            Key::Down | Key::Ctrl('n') => palette.move_by(true),
            other => palette.type_key(other),
        }
        Flow::Continue
    }

    fn run_palette_item(&mut self, item: Item) -> Flow<Exit> {
        let opens_the_editor = matches!(
            item.kind,
            Kind::History | Kind::Action(Action::Sql | Action::ToggleSql)
        );
        if self.mode == Mode::Query && !opens_the_editor {
            self.mode = Mode::Browse;
        }

        match item.kind {
            Kind::Action(action) => return self.run(action),
            Kind::Command(name) => return self.run_named(name, ""),
            Kind::Table => {
                let default_schema = self.database.default_schema();
                if let Some(table) =
                    Table::resolve(&item.target, &self.tables, default_schema.as_deref())
                {
                    self.go_to_table(&table);
                }
            }
            Kind::Database => self.use_database(Some(item.target)),
            Kind::Connection => {
                if let Ok(id) = item.target.parse() {
                    return self.leave(Exit::Open(id));
                }
            }
            Kind::History => self.use_statement(&item.target),
        }
        Flow::Continue
    }

    fn go_to_table(&mut self, table: &Table) {
        self.filter.clear();
        self.query.results = false;
        let Some(index) = self.reveal(table) else {
            return;
        };
        if index == self.table_index {
            self.load(false);
        } else {
            self.select_table(index);
        }
        self.focus = Focus::Grid;
    }
}
