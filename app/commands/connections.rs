use serde_json::Value;

use super::answers;
use crate::console::prelude::*;

pub struct Connections;

impl Command for Connections {
    fn signature(&self) -> &str {
        "connections {--json : JSON, which is what you get when the output is piped} {--table : A table, which is what you get in a terminal}"
    }

    fn description(&self) -> &str {
        "List the saved connections, for a script or an agent"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let listed = answers::agent().connections();
        answers::answer(cmd, listed, draw)
    }
}

fn draw(cmd: &mut Cmd, answer: &Value) {
    let rows: Vec<Vec<String>> = answer
        .as_array()
        .into_iter()
        .flatten()
        .map(|connection| {
            vec![
                answers::text(&connection["name"]),
                answers::text(&connection["driver"]),
                answers::text(&connection["target"]),
                match connection["read_only"].as_bool() {
                    Some(true) => "yes".to_string(),
                    _ => String::new(),
                },
                match &connection["last_used_at"] {
                    Value::Null => "never".to_string(),
                    at => answers::text(at),
                },
            ]
        })
        .collect();
    cmd.table(&["Name", "Driver", "Where", "Read only", "Last used"], rows);
}
