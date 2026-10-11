use super::answers;
use crate::console::prelude::*;
use crate::mcp::Server;

pub struct McpStart;

impl Command for McpStart {
    fn signature(&self) -> &str {
        "mcp:start {handle : The handle of the MCP server to start}"
    }

    fn description(&self) -> &str {
        "Start the MCP server that gives an agent tql's connections"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let handle = cmd.argument("handle").unwrap_or_default().to_string();
        if handle != "tql" {
            return answers::refuse(
                cmd,
                format!("There is no MCP server named [{handle}]. tql has one, named [tql]."),
            );
        }
        let server = Server::new(answers::agent(), cmd.app_version());
        while let Some(line) = cmd.read_line()? {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = server.handle(&line) {
                cmd.line(reply);
                cmd.flush();
            }
        }
        Ok(())
    }
}
