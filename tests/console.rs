use app::console::prelude::*;

const APP_CONFIG: &str = r#"
name = "Example"
env = "${TQL_RUST_TEST_UNSET_ENV:-development}"

[deploy]
environments = ["staging", "production"]
"#;

struct Greet;

impl Command for Greet {
    fn signature(&self) -> &str {
        "greet {name=World : Who should be greeted} {nickname? : What to call them} {--S|shout : Greet them loudly} {--times=1 : How many times}"
    }

    fn description(&self) -> &str {
        "Greet someone"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let name = cmd.argument("name").unwrap_or_default();
        let greeting = match cmd.argument("nickname") {
            Some(nickname) => format!("Hello, {name} \"{nickname}\"!"),
            None => format!("Hello, {name}!"),
        };
        let greeting = if cmd.flag("shout") {
            greeting.to_uppercase()
        } else {
            greeting
        };

        for _ in 0..cmd.option_as::<usize>("times")?.unwrap_or_default() {
            cmd.info(&greeting);
        }
        Ok(())
    }
}

struct About;

impl Command for About {
    fn signature(&self) -> &str {
        "about"
    }

    fn description(&self) -> &str {
        "Display application information"
    }

    fn help(&self) -> &str {
        "Shows the <fg=green>name</>, the version and\nthe <fg=yellow>environment</>."
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let config = cmd.config();
        let rows = [
            [
                "Application name",
                config.str("app.name").unwrap_or_default(),
            ],
            ["Version", cmd.app_version()],
            ["Environment", config.str("app.env").unwrap_or_default()],
        ]
        .map(|row| row.map(String::from));

        cmd.table(&["Key", "Value"], rows);
        Ok(())
    }
}

struct Fail;

impl Command for Fail {
    fn signature(&self) -> &str {
        "fail {code}"
    }

    fn description(&self) -> &str {
        "Exit with the given code"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        match cmd
            .argument("code")
            .unwrap_or_default()
            .parse()
            .unwrap_or(1)
        {
            0 => Ok(()),
            code => exit(code),
        }
    }
}

struct Deploy {
    steps: [&'static str; 2],
}

impl Command for Deploy {
    fn signature(&self) -> &str {
        "deploy {environment? : Where to deploy} {--force : Skip the production confirmation}"
    }

    fn description(&self) -> &str {
        "Deploy the application"
    }

    fn execute(&self, cmd: &mut Cmd) -> Result {
        let environments = cmd.config().strings("app.deploy.environments");

        let environment = match cmd.argument("environment") {
            Some(environment) => environment.to_string(),
            None => cmd.choice("Where should we deploy?", &environments, Some(0))?,
        };

        if !environments.contains(&environment) {
            bail!("unknown environment \"{environment}\"");
        }

        if environment == "production"
            && !cmd.flag("force")
            && !cmd.confirm("Really deploy to production?", false)?
        {
            cmd.warn("Deployment cancelled.");
            return exit(1);
        }

        for step in self.steps {
            cmd.task(step, || Ok(true))?;
        }
        cmd.info(format!("Deployed to {environment}."));
        Ok(())
    }
}

fn app() -> Application {
    Application::new("example", "1.2.3")
        .config_source("app", APP_CONFIG)
        .command(About)
        .command(Deploy {
            steps: ["Building assets", "Uploading release"],
        })
        .command(Fail)
        .command(Greet)
}

#[test]
fn listing_shows_config_name_version_and_commands() {
    let result = app().run_with(Vec::<String>::new(), "");

    assert_eq!(result.code, 0);
    assert!(
        result.stdout.starts_with("Example 1.2.3"),
        "{}",
        result.stdout
    );
    for command in ["about", "deploy", "fail", "greet"] {
        assert!(result.stdout.contains(command), "missing {command}");
    }
}

#[test]
fn defaults_fill_in_missing_arguments_and_options() {
    let result = app().run_with(["greet"], "");

    assert_eq!(result.code, 0);
    assert_eq!(result.stdout, "Hello, World!\n");
}

#[test]
fn optional_arguments_shortcuts_and_options_are_read() {
    let result = app().run_with(["greet", "Taylor", "Otwell", "-S", "--times=2"], "");

    assert_eq!(
        result.stdout,
        "HELLO, TAYLOR \"OTWELL\"!\nHELLO, TAYLOR \"OTWELL\"!\n"
    );
}

#[test]
fn invalid_typed_option_fails_with_an_error() {
    let result = app().run_with(["greet", "--times=lots"], "");

    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("invalid value \"lots\" for `times`"));
}

#[test]
fn exit_sets_the_exit_code() {
    assert_eq!(app().run_with(["fail", "3"], "").code, 3);
    assert_eq!(app().run_with(["fail", "0"], "").code, 0);
}

#[test]
fn unknown_command_suggests_a_match() {
    let result = app().run_with(["gret"], "");

    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("'greet'"));
}

#[test]
fn about_renders_config_in_a_table() {
    let result = app().run_with(["about"], "");

    assert!(result.stdout.contains("| Application name | Example     |"));
    assert!(result.stdout.contains("| Environment      | development |"));
}

#[test]
fn tasks_report_each_step() {
    let result = app().run_with(["deploy", "staging"], "");

    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("Building assets: ✔"));
    assert!(result.stdout.contains("Uploading release: ✔"));
}

#[test]
fn deploy_asks_for_environment_and_confirmation() {
    let result = app().run_with(["deploy"], "production\nyes\n");

    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("[1] production"));
    assert!(result.stdout.contains("Deployed to production."));
}

#[test]
fn declining_production_exits_with_code_one() {
    let result = app().run_with(["deploy", "production"], "n\n");

    assert_eq!(result.code, 1);
    assert!(result.stdout.contains("Deployment cancelled."));
}

#[test]
fn no_interaction_uses_prompt_defaults() {
    assert!(
        app()
            .run_with(["deploy", "-n"], "")
            .stdout
            .contains("Deployed to staging.")
    );
    assert_eq!(app().run_with(["deploy", "production", "-n"], "").code, 1);
}

#[test]
fn invalid_choice_is_asked_again() {
    let result = app().run_with(["deploy"], "mars\n0\n");

    assert!(result.stdout.contains("Value \"mars\" is invalid"));
    assert!(result.stdout.contains("Deployed to staging."));
}

#[test]
fn default_command_runs_when_no_command_is_given() {
    let result = app()
        .config_source("commands", "default = \"greet\"")
        .run_with(Vec::<String>::new(), "");

    assert_eq!(result.stdout, "Hello, World!\n");
}

#[test]
fn list_shows_the_commands_too() {
    let listed = app().run_with(["list"], "");

    assert_eq!(listed.code, 0);
    assert!(
        listed.stdout.starts_with("Example 1.2.3"),
        "{}",
        listed.stdout
    );
    for command in ["about", "deploy", "fail", "greet", "list"] {
        assert!(listed.stdout.contains(command), "missing {command}");
    }
}

#[test]
fn help_says_more_than_the_description_without_its_colour_tags() {
    let help = app().run_with(["about", "--help"], "");

    assert!(
        help.stdout
            .contains("Help:\n  Shows the name, the version and\n  the environment."),
        "{}",
        help.stdout
    );
}

#[test]
fn the_default_command_runs_when_only_global_flags_are_given() {
    for flag in ["-n", "--no-interaction", "--ansi", "--no-ansi"] {
        let result = app()
            .config_source("commands", "default = \"greet\"")
            .run_with([flag], "");

        assert!(
            result.stdout.contains("Hello, World!"),
            "{flag}: {}",
            result.stdout
        );
    }
}

#[test]
fn completion_prints_a_script_for_the_shell_without_being_listed() {
    let zsh = app().run_with(["completion", "zsh"], "");
    assert_eq!(zsh.code, 0, "{}", zsh.stderr);
    assert!(zsh.stdout.contains("#compdef"), "{}", zsh.stdout);
    assert!(zsh.stdout.contains("greet"));

    let bash = app().run_with(["completion", "bash"], "");
    assert!(bash.stdout.contains("complete -F"), "{}", bash.stdout);

    assert!(!app().run_with(["list"], "").stdout.contains("completion"));
}
