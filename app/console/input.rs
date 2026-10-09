use std::collections::HashMap;
use std::slice;

use clap::{Arg, ArgAction, ArgMatches};

use crate::console::signature::{OptionKind, Signature};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    One(Option<String>),
    Many(Vec<String>),
    Flag(bool),
}

#[derive(Debug, Default)]
pub(crate) struct Input {
    arguments: HashMap<String, Value>,
    options: HashMap<String, Value>,
}

impl Input {
    pub(crate) fn from_matches(signature: &Signature, matches: &ArgMatches) -> Self {
        let many = |name: &str| {
            Value::Many(
                matches
                    .get_many::<String>(name)
                    .map(|values| values.cloned().collect())
                    .unwrap_or_default(),
            )
        };
        let one = |name: &str| Value::One(matches.get_one::<String>(name).cloned());

        let arguments = signature
            .arguments
            .iter()
            .map(|argument| {
                let value = if argument.array {
                    many(&argument.name)
                } else {
                    one(&argument.name)
                };
                (argument.name.clone(), value)
            })
            .collect();

        let options = signature
            .options
            .iter()
            .map(|option| {
                let value = match option.kind {
                    OptionKind::Flag => Value::Flag(matches.get_flag(&option.name)),
                    OptionKind::Value => one(&option.name),
                    OptionKind::Array => many(&option.name),
                };
                (option.name.clone(), value)
            })
            .collect();

        Input { arguments, options }
    }

    pub(crate) fn argument(&self, name: &str) -> Option<&str> {
        single(lookup(&self.arguments, "argument", name), name)
    }

    pub(crate) fn arguments(&self, name: &str) -> &[String] {
        list(lookup(&self.arguments, "argument", name), name)
    }

    pub(crate) fn option(&self, name: &str) -> Option<&str> {
        single(lookup(&self.options, "option", name), name)
    }

    pub(crate) fn options(&self, name: &str) -> &[String] {
        list(lookup(&self.options, "option", name), name)
    }

    pub(crate) fn flag(&self, name: &str) -> bool {
        match lookup(&self.options, "option", name) {
            Value::Flag(enabled) => *enabled,
            Value::One(value) => value.is_some(),
            Value::Many(values) => !values.is_empty(),
        }
    }
}

fn lookup<'a>(values: &'a HashMap<String, Value>, kind: &str, name: &str) -> &'a Value {
    values
        .get(name)
        .unwrap_or_else(|| panic!("the command signature has no {kind} named `{name}`"))
}

fn single<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    match value {
        Value::One(value) => value.as_deref(),
        Value::Many(values) => values.first().map(String::as_str),
        Value::Flag(_) => panic!("`{name}` is a flag; read it with `flag()`"),
    }
}

fn list<'a>(value: &'a Value, name: &str) -> &'a [String] {
    match value {
        Value::Many(values) => values,
        Value::One(Some(value)) => slice::from_ref(value),
        Value::One(None) => &[],
        Value::Flag(_) => panic!("`{name}` is a flag; read it with `flag()`"),
    }
}

pub(crate) fn to_clap(signature: &Signature, about: &str) -> clap::Command {
    let command = clap::Command::new(signature.name.clone()).about(about.to_string());

    let command = signature
        .arguments
        .iter()
        .fold(command, |command, argument| {
            let mut arg = Arg::new(argument.name.clone())
                .value_name(argument.name.to_uppercase())
                .help(argument.description.clone())
                .required(argument.required);
            if argument.array {
                arg = arg.action(ArgAction::Append).num_args(1..);
            }
            if let Some(default) = &argument.default {
                arg = arg.default_value(default.clone());
            }
            command.arg(arg)
        });

    signature.options.iter().fold(command, |command, option| {
        let mut arg = Arg::new(option.name.clone())
            .long(option.name.clone())
            .help(option.description.clone());
        if let Some(shortcut) = option.shortcut {
            arg = arg.short(shortcut);
        }
        arg = match option.kind {
            OptionKind::Flag => arg.action(ArgAction::SetTrue),
            OptionKind::Value => arg
                .action(ArgAction::Set)
                .num_args(1)
                .value_name(option.name.to_uppercase()),
            OptionKind::Array => arg
                .action(ArgAction::Append)
                .num_args(1)
                .value_name(option.name.to_uppercase()),
        };
        if let Some(default) = &option.default {
            arg = arg.default_value(default.clone());
        }
        command.arg(arg)
    })
}
