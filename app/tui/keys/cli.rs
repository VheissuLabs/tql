use super::*;
use crate::keystore::{CONNECTION_STATE, blocks, redacted, words};
use crate::tui::history::{self, Execution};

pub struct Cli {
    pub editor: QueryEditor,
    pub reply: Option<Reply>,
    pub ran: Option<String>,
}

impl KeyBrowser {
    pub(super) fn open_cli(&mut self) {
        if self.cli.is_none() {
            self.cli = Some(Cli {
                editor: fresh_editor(),
                reply: None,
                ran: None,
            });
        }
        self.say(format!(
            "{} · ctrl+r runs the line · esc closes",
            self.server
        ));
    }

    pub(super) fn type_into_cli(&mut self, key: Key) -> Flow<Exit> {
        let Some(cli) = self.cli.as_mut() else {
            return Flow::Continue;
        };
        match key {
            Key::Escape => {
                self.cli = None;
                self.hush();
            }
            Key::Ctrl('r') => self.run_cli_line(),
            Key::Char(':') if cli.editor.is_empty() => self.command = Some(String::new()),
            other => cli.editor.handle(other),
        }
        Flow::Continue
    }

    fn hush(&mut self) {
        self.status = None;
    }

    fn run_cli_line(&mut self) {
        let Some(cli) = self.cli.as_ref() else {
            return;
        };
        let lines = cli.editor.lines();
        let line = lines
            .get(cli.editor.cursor_line())
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string();
        if line.is_empty() {
            return self.say("type a command, such as HGETALL user:1");
        }
        let typed = match words(&line) {
            Ok(typed) => typed,
            Err(error) => return self.fail(format!("{error:#}")),
        };
        let first = typed[0].to_lowercase();
        if first == "select" {
            return self.fail("b switches database here, so the key list follows it");
        }
        let replies_off = first == "client"
            && typed
                .get(1)
                .is_some_and(|word| word.eq_ignore_ascii_case("reply"));
        if (CONNECTION_STATE.contains(&first.as_str()) && first != "client") || replies_off {
            return self.fail(format!(
                "{} changes the connection tql reads keys through, so tql does not run it here",
                typed[0].to_uppercase()
            ));
        }
        if blocks(&typed) {
            return self.fail(format!(
                "{} waits for the server forever, so tql does not run it",
                typed[0].to_uppercase()
            ));
        }
        if self.connection.read_only {
            match self.store.reads_only(&typed[0]) {
                Ok(true) => {}
                Ok(false) => {
                    return self.fail(format!(
                        "this connection only reads, and {} writes, so tql did not run it",
                        typed[0].to_uppercase()
                    ));
                }
                Err(error) => return self.fail(format!("{error:#}")),
            }
        }
        let started = Instant::now();
        let ran = self.store.run(&line);
        let elapsed = started.elapsed().as_millis();
        let (error, rows) = match &ran {
            Ok(Reply::Error(said)) => (Some(said.clone()), None),
            Ok(reply) => (None, Some(reply_rows(reply).1.len())),
            Err(error) => (Some(format!("{error:#}")), None),
        };
        if let Some(store) = &self.store_path {
            history::record(
                store,
                "redis",
                Execution {
                    connection_id: self.connection.id,
                    statement: &redacted(&line),
                    error: error.as_deref(),
                    rows,
                    duration_ms: elapsed,
                },
            );
        }
        match ran {
            Ok(reply) => {
                let failed = matches!(reply, Reply::Error(_));
                if let Some(cli) = self.cli.as_mut() {
                    cli.reply = Some(reply);
                    cli.ran = Some(line);
                }
                match failed {
                    true => self.fail(error.unwrap_or_default()),
                    false => self.say(format!("ran in {elapsed}ms")),
                }
                if !typed[0].eq_ignore_ascii_case("get")
                    && !self.store.reads_only(&typed[0]).unwrap_or(true)
                {
                    self.reload_shown();
                }
            }
            Err(error) => self.fail(format!("{error:#}")),
        }
    }
}
