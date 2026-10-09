use std::ffi::OsString;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Arg, ArgAction};

use crate::console::command::Exit;
use crate::console::config::Config;
use crate::console::input::{self, Input};
use crate::console::signature::Signature;
use crate::console::{Cmd, Command};

const RESERVED_NAMES: [&str; 5] = ["help", "version", "ansi", "no-ansi", "no-interaction"];
const RESERVED_SHORTCUTS: [char; 3] = ['h', 'V', 'n'];

pub struct Application {
    name: String,
    version: String,
    commands: Vec<Box<dyn Command>>,
    config_sources: Vec<(String, String)>,
    config_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

struct Streams<'a> {
    reader: &'a mut dyn BufRead,
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
    ansi: bool,
}

impl Application {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Application {
            name: name.into(),
            version: version.into(),
            commands: Vec::new(),
            config_sources: Vec::new(),
            config_dirs: Vec::new(),
        }
    }

    pub fn command(mut self, command: impl Command + 'static) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    pub fn config_source(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.config_sources.push((name.into(), source.into()));
        self
    }

    pub fn config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dirs.push(dir.into());
        self
    }

    pub fn run(&self) -> ExitCode {
        self.run_from(std::env::args_os())
    }

    pub fn run_from<Args, Arg>(&self, argv: Args) -> ExitCode
    where
        Args: IntoIterator<Item = Arg>,
        Arg: Into<OsString>,
    {
        let _ = dotenvy::dotenv();

        let stdout = io::stdout();
        let ansi = stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let code = self.execute(
            argv.into_iter().map(Into::into),
            Streams {
                reader: &mut io::stdin().lock(),
                out: &mut stdout.lock(),
                err: &mut io::stderr().lock(),
                ansi,
            },
        );

        ExitCode::from(code.clamp(0, 255) as u8)
    }

    pub fn run_with<Args, Arg>(&self, args: Args, stdin: &str) -> RunResult
    where
        Args: IntoIterator<Item = Arg>,
        Arg: Into<OsString>,
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let argv =
            std::iter::once(OsString::from(&self.name)).chain(args.into_iter().map(Into::into));

        let code = self.execute(
            argv,
            Streams {
                reader: &mut io::Cursor::new(stdin.as_bytes()),
                out: &mut out,
                err: &mut err,
                ansi: false,
            },
        );

        RunResult {
            code,
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        }
    }

    fn execute(&self, argv: impl IntoIterator<Item = OsString>, streams: Streams) -> i32 {
        let Streams {
            reader,
            out,
            err,
            ansi,
        } = streams;

        let signatures = match self.signatures() {
            Ok(signatures) => signatures,
            Err(message) => {
                let _ = writeln!(err, "{message}");
                return 1;
            }
        };

        let config = match self.load_config() {
            Ok(config) => config,
            Err(error) => {
                report(err, &error, ansi);
                return 1;
            }
        };

        let mut argv: Vec<OsString> = argv.into_iter().collect();
        if argv.len() == 1
            && let Some(default) = config.str("commands.default")
        {
            argv.push(default.into());
        }
        let binary = argv
            .first()
            .and_then(|program| std::path::Path::new(program).file_name())
            .map(|file_name| file_name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone());

        let mut cli = self
            .cli(&signatures, config.str("app.name").unwrap_or(&self.name))
            .bin_name(binary);
        let matches = match cli.try_get_matches_from_mut(argv) {
            Ok(matches) => matches,
            Err(error) => {
                let rendered = error.render();
                let rendered = if ansi {
                    rendered.ansi().to_string()
                } else {
                    rendered.to_string()
                };
                let target: &mut dyn Write = if error.use_stderr() { err } else { out };
                let _ = write!(target, "{rendered}");
                return error.exit_code();
            }
        };

        let Some((name, sub_matches)) = matches.subcommand() else {
            let help = cli.render_help();
            let help = if ansi {
                help.ansi().to_string()
            } else {
                help.to_string()
            };
            let _ = write!(out, "{help}");
            return 0;
        };

        let global_flag = |flag: &str| matches.get_flag(flag) || sub_matches.get_flag(flag);
        let ansi = !global_flag("no-ansi") && (ansi || global_flag("ansi"));

        let Some(index) = signatures
            .iter()
            .position(|signature| signature.name == name)
        else {
            return 1;
        };

        let mut cmd = Cmd {
            input: Input::from_matches(&signatures[index], sub_matches),
            config: &config,
            out,
            reader,
            ansi,
            interactive: !global_flag("no-interaction"),
            app_name: &self.name,
            app_version: &self.version,
        };

        let Err(error) = self.commands[index].execute(&mut cmd) else {
            return 0;
        };
        if let Some(Exit(code)) = error.downcast_ref::<Exit>() {
            return *code;
        }
        report(err, &error, ansi);
        1
    }

    fn signatures(&self) -> Result<Vec<Signature>, String> {
        let mut signatures: Vec<Signature> = Vec::new();

        for command in &self.commands {
            let signature = Signature::parse(command.signature())
                .map_err(|error| format!("Invalid command signature: {error}"))?;

            if signatures
                .iter()
                .any(|existing| existing.name == signature.name)
            {
                return Err(format!("Command `{}` is registered twice", signature.name));
            }

            let reserved_option = signature.options.iter().find(|option| {
                RESERVED_NAMES.contains(&option.name.as_str())
                    || option
                        .shortcut
                        .is_some_and(|shortcut| RESERVED_SHORTCUTS.contains(&shortcut))
            });
            if let Some(option) = reserved_option {
                return Err(format!(
                    "Command `{}` uses the reserved option `{}`",
                    signature.name, option.name
                ));
            }

            signatures.push(signature);
        }

        Ok(signatures)
    }

    fn cli(&self, signatures: &[Signature], display_name: &str) -> clap::Command {
        let global = |name: &'static str, help: &'static str| {
            Arg::new(name)
                .long(name)
                .global(true)
                .action(ArgAction::SetTrue)
                .help(help)
                .help_heading("Global Options")
        };

        let root = clap::Command::new(self.name.clone())
            .version(self.version.clone())
            .help_template(format!(
                "{display_name} {{version}}\n\n{{usage-heading}} {{usage}}\n\n{{all-args}}"
            ))
            .arg(global("no-interaction", "Do not ask any interactive question").short('n'))
            .arg(global("ansi", "Force ANSI output"))
            .arg(global("no-ansi", "Disable ANSI output"));

        let mut commands: Vec<_> = signatures.iter().zip(&self.commands).collect();
        commands.sort_by(|left, right| left.0.name.cmp(&right.0.name));

        commands
            .into_iter()
            .fold(root, |root, (signature, command)| {
                root.subcommand(input::to_clap(signature, command.description()))
            })
    }

    fn load_config(&self) -> anyhow::Result<Config> {
        let mut config = Config::default();
        for (name, source) in &self.config_sources {
            config.merge_source(name, source)?;
        }
        for dir in &self.config_dirs {
            config.load_dir(dir)?;
        }
        Ok(config)
    }
}

fn report(err: &mut dyn Write, error: &anyhow::Error, ansi: bool) {
    let badge = if ansi {
        "\x1b[37;41m ERROR \x1b[0m"
    } else {
        " ERROR "
    };
    let _ = writeln!(err, "\n {badge} {error}");
    for cause in error.chain().skip(1) {
        let _ = writeln!(err, "         caused by: {cause}");
    }
    let _ = writeln!(err);
}
