use serde_json::Value;

use super::*;
use crate::tui::document;

impl Browser {
    pub(super) fn yank_value(&mut self) {
        if self.raw.is_empty() {
            return;
        }
        let value = self
            .raw
            .get(self.row_index)
            .and_then(|row| row.get(self.column_index))
            .cloned()
            .flatten()
            .unwrap_or_else(|| "NULL".to_string());
        let column = self
            .headers
            .get(self.column_index)
            .cloned()
            .unwrap_or_default();
        self.yanked(&value, &column);
    }

    pub(super) fn yank_row(&mut self) {
        let Some(values) = self.raw.get(self.row_index).cloned() else {
            return;
        };
        let types: HashMap<String, String> = self
            .current_table()
            .map(|table| {
                self.schema
                    .columns(self.database.as_mut(), &table)
                    .into_iter()
                    .map(|column| (column.name, column.type_name.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        let object: serde_json::Map<String, Value> = self
            .headers
            .iter()
            .zip(values)
            .map(|(column, value)| {
                let kind = types.get(column).map(String::as_str).unwrap_or_default();
                (column.clone(), json_value(value, kind))
            })
            .collect();
        self.yanked(&pretty(&Value::Object(object)), "row");
    }

    fn yanked(&mut self, text: &str, what: &str) {
        let reached = self.clipboard.copy(text);
        self.say(format!("yanked {what} to the {} clipboard", reached.name()));
    }
}

fn json_value(value: Option<String>, kind: &str) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if let Some(decoded) = document::json(&value) {
        return decoded;
    }
    let whole = ["int", "serial"].iter().any(|name| kind.contains(name));
    let fractional = ["real", "float", "double"]
        .iter()
        .any(|name| kind.contains(name));
    let number = match (whole, fractional) {
        (true, _) => value.parse::<i64>().ok().map(Value::from),
        (_, true) => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
        _ => None,
    };
    number.unwrap_or(Value::String(value))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let content = line.trim_start();
            let indent = line.len() - content.len();
            format!("{}{content}", " ".repeat(indent * 2))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
