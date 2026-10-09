use serde_json::Value;

use crate::agent::{self, Agent, Answer};
use crate::console::prelude::*;
use crate::paths;

pub fn agent() -> Agent {
    Agent::new(paths::store(), paths::key())
}

pub fn answer(cmd: &mut Cmd, produced: Answer, draw: impl FnOnce(&mut Cmd, &Value)) -> Result {
    let answer = match produced {
        Ok(answer) => answer,
        Err(refusal) => return refuse(cmd, refusal),
    };
    match wants_json(cmd) {
        true => cmd.line(agent::pretty(&answer)),
        false => draw(cmd, &answer),
    }
    Ok(())
}

pub fn refuse(cmd: &mut Cmd, refusal: impl std::fmt::Display) -> Result {
    cmd.refuse(refusal);
    exit(1)
}

fn wants_json(cmd: &Cmd) -> bool {
    cmd.flag("json") || (!cmd.flag("table") && cmd.is_piped())
}

pub fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
