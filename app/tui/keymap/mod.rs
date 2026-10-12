use crate::terminal::Key;
use toml::Value;

use crate::settings;

mod defaults;
mod names;

pub use defaults::defaults;
use names::spelling;
pub use names::{glyph, keys, parse, spell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Activate,
    Escape,
    Command,
    NextPane,
    PreviousPane,
    FocusTables,
    FocusRows,
    FocusSql,
    ToggleTables,
    ToggleSql,
    InspectRow,
    ViewValue,
    EditValue,
    EditRow,
    SortColumn,
    MarkDelete,
    ClearMarks,
    NewRow,
    Ask,
    FilterRows,
    FilterTables,
    Structure,
    Databases,
    Sql,
    History,
    FollowLink,
    JumpBack,
    NextPage,
    PreviousPage,
    Reload,
    Connections,
    YankValue,
    YankRow,
    Narrow,
    Widen,
    ResetWidth,
    Palette,
    Help,
    Quit,
    Redraw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub action: Action,
    pub name: &'static str,
    pub keys: Vec<Key>,
    pub description: &'static str,
    pub rebindable: bool,
}

impl Binding {
    pub fn shown(&self) -> String {
        self.keys
            .iter()
            .map(|key| spell(*key))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

pub(super) fn ported(
    action: Action,
    name: &'static str,
    keys: &[Key],
    description: &'static str,
) -> Binding {
    Binding {
        action,
        name,
        keys: keys.to_vec(),
        description,
        rebindable: true,
    }
}

pub(super) fn fixed(
    action: Action,
    name: &'static str,
    keys: &[Key],
    description: &'static str,
) -> Binding {
    Binding {
        rebindable: false,
        ..ported(action, name, keys, description)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap {
            bindings: defaults(),
        }
    }
}

impl Keymap {
    pub fn rebind(&mut self, name: &str, value: &Value) -> bool {
        let Some(binding) = self
            .bindings
            .iter_mut()
            .find(|binding| binding.name == name)
        else {
            return false;
        };
        if !binding.rebindable {
            return true;
        }
        let spelled = match value {
            Value::Array(values) => values.iter().filter_map(spelling).collect(),
            other => spelling(other).into_iter().collect::<Vec<_>>(),
        };
        let keys: Vec<Key> = spelled.iter().filter_map(|name| parse(name)).collect();
        if !keys.is_empty() {
            binding.keys = keys;
        }
        true
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn binding(&self, action: Action) -> &Binding {
        self.bindings
            .iter()
            .find(|binding| binding.action == action)
            .expect("every action has a binding")
    }

    pub fn action(&self, key: Key) -> Option<Action> {
        self.bindings
            .iter()
            .find(|binding| binding.keys.contains(&key))
            .map(|binding| binding.action)
    }

    pub fn clashes(&self) -> Vec<String> {
        let mut seen: Vec<(Key, &str)> = Vec::new();
        let mut clashing = Vec::new();
        for binding in &self.bindings {
            for key in &binding.keys {
                if let Some((_, earlier)) = seen.iter().rev().find(|(seen_key, _)| seen_key == key)
                {
                    clashing.push(format!(
                        "{} is both {earlier} and {}",
                        spell(*key),
                        binding.name
                    ));
                }
                seen.push((*key, binding.name));
            }
        }
        clashing
    }
}

pub fn bindings() -> Vec<Binding> {
    settings::with(|settings| settings.keymap.bindings().to_vec())
}

pub fn binding(action: Action) -> Binding {
    settings::with(|settings| settings.keymap.binding(action).clone())
}

pub fn action(key: Key) -> Option<Action> {
    settings::with(|settings| settings.keymap.action(key))
}

pub fn clashes() -> Vec<String> {
    settings::with(|settings| settings.keymap.clashes())
}

pub fn key(action: Action, which: usize) -> String {
    let keys = binding(action).keys;
    keys.get(which)
        .or(keys.first())
        .map(|key| glyph(*key))
        .unwrap_or_default()
}

pub fn spelled(action: Action) -> String {
    binding(action)
        .keys
        .first()
        .map(|key| spell(*key))
        .unwrap_or_default()
}

pub fn pane_key(action: Action) -> String {
    match binding(action).keys.first() {
        Some(Key::Alt(digit)) if digit.is_ascii_digit() => format!("[{digit}] "),
        Some(key) => format!("[{}] ", glyph(*key)),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests;
