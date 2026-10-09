use serde_json::{Value, json};

use crate::agent::{self, Agent};
use crate::live;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    ListConnections,
    ListTables,
    DescribeTable,
    RunQuery,
    WriteSql,
}

impl Tool {
    pub const ALL: [Tool; 5] = [
        Tool::ListConnections,
        Tool::ListTables,
        Tool::DescribeTable,
        Tool::RunQuery,
        Tool::WriteSql,
    ];

    pub fn named(name: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|tool| tool.name() == name)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tool::ListConnections => "list-connections-tool",
            Tool::ListTables => "list-tables-tool",
            Tool::DescribeTable => "describe-table-tool",
            Tool::RunQuery => "run-query-tool",
            Tool::WriteSql => "write-sql-tool",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Tool::ListConnections => "List Connections Tool",
            Tool::ListTables => "List Tables Tool",
            Tool::DescribeTable => "Describe Table Tool",
            Tool::RunQuery => "Run Query Tool",
            Tool::WriteSql => "Write SQL Tool",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Tool::ListConnections => {
                "List every database connection tql knows about. Use the returned name with the other tql tools."
            }
            Tool::ListTables => "List the tables in a tql connection.",
            Tool::DescribeTable => {
                "Describe the columns of a table in a tql connection, including types and nullability."
            }
            Tool::RunQuery => {
                "Run a read-only SQL query against a tql connection. Only SELECT, SHOW, EXPLAIN, DESCRIBE and PRAGMA statements are permitted."
            }
            Tool::WriteSql => {
                "Put SQL in the SQL editor of the tql the user has open on a connection, in place of what is there, so the user can read it, change it and run it. Nothing runs. Use it for statements that change data, which the run query tool refuses, or for a query the user wants to see."
            }
        }
    }

    fn schema(&self) -> Value {
        let connection =
            |description: &str| json!({ "description": description, "type": "string" });
        match self {
            Tool::ListConnections => json!({ "type": "object", "properties": {} }),
            Tool::ListTables => json!({
                "properties": {
                    "connection": connection("The tql connection name, as returned by the list connections tool."),
                },
                "type": "object",
                "required": ["connection"],
            }),
            Tool::DescribeTable => json!({
                "properties": {
                    "connection": connection("The tql connection name."),
                    "table": { "description": "The table to describe.", "type": "string" },
                },
                "type": "object",
                "required": ["connection", "table"],
            }),
            Tool::RunQuery => json!({
                "properties": {
                    "connection": connection("The tql connection name."),
                    "query": { "description": "A read-only SQL statement.", "type": "string" },
                },
                "type": "object",
                "required": ["connection", "query"],
            }),
            Tool::WriteSql => json!({
                "properties": {
                    "connection": connection("The tql connection name of the tql the user has open."),
                    "sql": { "description": "The SQL to put in the editor.", "type": "string" },
                },
                "type": "object",
                "required": ["connection", "sql"],
            }),
        }
    }

    pub fn listed(&self) -> Value {
        json!({
            "name": self.name(),
            "title": self.title(),
            "description": self.description(),
            "inputSchema": self.schema(),
            "annotations": {},
        })
    }

    pub fn call(&self, agent: &Agent, arguments: &Value) -> Result<String, String> {
        let text = |name: &str| arguments[name].as_str();
        let pretty = |answer: Value| agent::pretty(&answer);
        match self {
            Tool::ListConnections => match agent.connections()? {
                Value::Array(listed) if listed.is_empty() => {
                    Ok("No connections are configured in tql yet.".to_string())
                }
                listed => Ok(pretty(listed)),
            },
            Tool::ListTables => agent.tables(text("connection"), None).map(pretty),
            Tool::DescribeTable => agent
                .describe(text("connection"), text("table").unwrap_or_default(), None)
                .map(pretty),
            Tool::RunQuery => agent
                .query(
                    text("connection"),
                    text("query").unwrap_or_default(),
                    "mcp",
                    None,
                    agent::LIMIT,
                )
                .map(pretty),
            Tool::WriteSql => live::deliver(
                &live::directory(),
                text("connection").unwrap_or_default(),
                text("sql").unwrap_or_default(),
            ),
        }
    }
}
