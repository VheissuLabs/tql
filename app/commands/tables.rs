use super::answers;
use crate::console::prelude::*;

pub struct Tables;

impl Command for Tables {
    fn signature(&self) -> &str {
        "tables {connection : The connection, by name} {--database= : Another database on the same server} {--json : JSON, which is what you get when the output is piped} {--table : A table, which is what you get in a terminal}"
    }

    fn description(&self) -> &str {
        "List the tables in a connection, for a script or an agent"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let listed = answers::agent().tables(cmd.argument("connection"), cmd.option("database"));
        answers::answer(cmd, listed, |cmd, answer| {
            for table in answer["tables"].as_array().into_iter().flatten() {
                cmd.line(answers::text(table));
            }
        })
    }
}
