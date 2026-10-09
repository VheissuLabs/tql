mod application;
mod cmd;
mod command;
mod config;
mod input;
mod make;
mod prompt;
mod signature;

pub use anyhow::{anyhow, bail};
pub use application::{Application, RunResult};
pub use cmd::Cmd;
pub use command::{Command, Exit, exit};
pub use config::Config;
pub use make::MakeCommand;
pub use signature::{Argument, Opt, OptionKind, Signature, SignatureError};

pub type Result<T = ()> = anyhow::Result<T>;

pub mod prelude {
    pub use super::{Application, Cmd, Command, Result, anyhow, bail, exit};
}
