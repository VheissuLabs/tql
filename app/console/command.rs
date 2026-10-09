use crate::console::Cmd;

pub trait Command {
    fn signature(&self) -> &str;

    fn description(&self) -> &str {
        ""
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn help(&self) -> &str {
        ""
    }

    fn execute(&self, cmd: &mut Cmd) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit(pub i32);

impl std::fmt::Display for Exit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "exited with code {}", self.0)
    }
}

impl std::error::Error for Exit {}

pub fn exit(code: i32) -> anyhow::Result<()> {
    Err(Exit(code).into())
}
