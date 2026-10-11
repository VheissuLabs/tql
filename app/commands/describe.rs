use super::answers;
use crate::console::prelude::*;

pub struct Describe;

impl Command for Describe {
    fn signature(&self) -> &str {
        "describe {connection : The connection, by name} {table : The table} {--database= : Another database on the same server} {--json : JSON, which is what you get when the output is piped} {--table : A table, which is what you get in a terminal}"
    }

    fn description(&self) -> &str {
        "Describe a table's columns, for a script or an agent"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let described = answers::agent().describe(
            cmd.argument("connection"),
            cmd.argument("table").unwrap_or_default(),
            cmd.option("database"),
        );
        answers::answer(cmd, described, |cmd, answer| {
            let primary_key = answer["primary_key"].as_str();
            let rows: Vec<Vec<String>> = answer["columns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|column| {
                    let name = answers::text(&column["name"]);
                    let shown_type = match answers::text(&column["type"]) {
                        full_type if full_type.is_empty() => answers::text(&column["type_name"]),
                        full_type => full_type,
                    };
                    let nullable = match column["nullable"].as_bool() {
                        Some(true) => "yes",
                        _ => "no",
                    };
                    let key = match Some(name.as_str()) == primary_key {
                        true => "primary",
                        false => "",
                    };
                    vec![
                        name.clone(),
                        shown_type,
                        nullable.to_string(),
                        answers::text(&column["default"]),
                        key.to_string(),
                    ]
                })
                .collect();
            cmd.table(&["Column", "Type", "Null", "Default", "Key"], rows);
        })
    }
}
