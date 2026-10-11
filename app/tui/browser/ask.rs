use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use super::*;
use crate::ai::{self, Answer, Environment};
use crate::tui::QueryEditor;

pub const WIDTH: u16 = 68;
const ROWS: usize = 5;
const HINT: &str = "↵ asks    ⇧↵ new line    esc cancels";
const WAITING: &str = "asking…    esc cancels";

enum Waiting {
    Preparing(String),
    Answering(Receiver<anyhow::Result<Answer>>),
}

pub struct Question {
    pub editor: QueryEditor,
    waiting: Option<Waiting>,
}

impl Question {
    fn new() -> Self {
        Question {
            editor: QueryEditor::new(false),
            waiting: None,
        }
    }

    pub fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }
}

pub fn environment() -> Arc<Environment> {
    Arc::new(|name: &str| std::env::var(name).ok())
}

impl Browser {
    pub fn with_ai_environment(
        mut self,
        environment: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.ai_environment = Arc::new(environment);
        self
    }

    pub(super) fn open_question(&mut self) {
        self.question = Some(Question::new());
        self.hush();
    }

    pub(super) fn type_into_question(&mut self, key: Key) {
        let Some(question) = self.question.as_mut() else {
            return;
        };
        if key == Key::Escape {
            self.question = None;
            self.hush();
            return;
        }
        if question.is_waiting() {
            return;
        }
        match key {
            Key::ShiftEnter => question.editor.handle(Key::Enter),
            Key::Enter | Key::Ctrl('s') => {
                let asked = question.editor.buffer().trim().to_string();
                if !asked.is_empty() {
                    question.waiting = Some(Waiting::Preparing(asked));
                }
            }
            other => question.editor.handle(other),
        }
    }

    pub(super) fn paste_into_question(&mut self, text: &str) {
        if let Some(question) = self
            .question
            .as_mut()
            .filter(|question| !question.is_waiting())
        {
            question.editor.paste(text);
        }
    }

    pub(super) fn wait_for_answer(&mut self) -> bool {
        let Some(question) = self.question.as_mut() else {
            return false;
        };
        match question.waiting.take() {
            Some(Waiting::Preparing(asked)) => {
                self.start_asking(&asked);
                true
            }
            Some(Waiting::Answering(receiver)) => match receiver.try_recv() {
                Ok(answer) => {
                    self.answered(answer);
                    true
                }
                Err(TryRecvError::Empty) => {
                    question.waiting = Some(Waiting::Answering(receiver));
                    false
                }
                Err(TryRecvError::Disconnected) => {
                    self.answered(Err(anyhow::anyhow!(
                        "the model did not answer with a query"
                    )));
                    true
                }
            },
            None => false,
        }
    }

    fn start_asking(&mut self, asked: &str) {
        let tables = self.tables.clone();
        let focus = self.current_table();
        let database = self.database.as_mut();
        let schema = &mut self.schema;
        let summary = ai::summary(&tables, focus.as_deref(), |table| {
            schema.columns(database, table)
        });
        let prepared = settings::with(|settings| {
            ai::prepare(
                &settings.ai,
                self.ai_environment.as_ref(),
                self.connection.driver.name(),
                &summary,
                asked,
            )
        });
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(missing) => return self.answered(Err(missing)),
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(ai::ask(&prepared));
        });
        if let Some(question) = self.question.as_mut() {
            question.waiting = Some(Waiting::Answering(receiver));
        }
    }

    fn answered(&mut self, answer: anyhow::Result<Answer>) {
        let answer = match answer {
            Ok(answer) => answer,
            Err(message) => {
                return self.say(
                    single_line(&format!("{message:#}"))
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        };
        if answer.query.is_empty() {
            let said = if answer.notes.is_empty() {
                "no query for that".to_string()
            } else {
                answer.notes.clone()
            };
            return self.say(said);
        }
        self.question = None;
        self.replace_query(&ai::annotate(&answer));
        self.query.editor.move_to_start();
        self.enter_query_mode();
        let run = if settings::with(|settings| settings.sql_vim) {
            ":r"
        } else {
            "ctrl+r"
        };
        self.say(format!("{run} runs it · read it first · esc returns"));
    }

    pub(super) fn question_offers(&self) -> Option<Vec<(String, &'static str)>> {
        let question = self.question.as_ref()?;
        if question.is_waiting() {
            return Some(vec![("esc".into(), "Cancel")]);
        }
        Some(vec![
            ("↵".into(), "Ask"),
            ("⇧↵".into(), "New line"),
            ("esc".into(), "Cancel"),
        ])
    }

    pub(super) fn draw_question(&self, frame: &mut Frame, frame_height: u16) {
        let Some(question) = &self.question else {
            return;
        };
        let area = frame.area();
        let width = area.width.saturating_sub(4).min(WIDTH);
        let inner_width = width.saturating_sub(2) as usize;
        let inner_height = (ROWS + 3).min(frame_height.saturating_sub(2) as usize);

        let mut content = vec![Line::raw("")];
        content.extend(question_lines(question, inner_width.saturating_sub(4)));
        while content.len() < ROWS + 1 {
            content.push(Line::raw(""));
        }
        content.push(Line::raw(""));
        let footer = if question.is_waiting() { WAITING } else { HINT };
        content.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(footer, theme::dim()),
        ]));
        content.truncate(inner_height);

        let island = Island {
            title: "ASK".to_string(),
            content,
            ..Island::default()
        };
        Modal::new(area.width, 1, frame_height, width).draw(
            frame.buffer_mut(),
            island,
            (ROWS + 5) as u16,
        );
    }
}

fn question_lines(question: &Question, width: usize) -> Vec<Line<'static>> {
    let text = question.editor.buffer();
    if question.is_waiting() {
        return ai::wordwrap(&text, width, true)
            .split('\n')
            .map(|line| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(line.to_string(), theme::dim()),
                ])
            })
            .collect();
    }
    let cursor = question.editor.cursor();
    let mut lines = Vec::new();
    let mut seen = 0;
    let mut placed = false;
    for line in text.split('\n') {
        let characters: Vec<char> = line.chars().collect();
        for (start, end) in ai::pieces(&characters, width, true) {
            let piece: String = characters[start..end].iter().collect();
            let at = seen + start;
            let here = !placed && cursor >= at && cursor <= seen + end;
            let mut spans = vec![Span::raw("  ")];
            if here {
                placed = true;
                spans.extend(with_cursor(&characters[start..end], cursor - at));
            } else {
                spans.push(Span::raw(piece));
            }
            lines.push(Line::from(spans));
        }
        seen += characters.len() + 1;
    }
    lines
}

fn with_cursor(characters: &[char], at: usize) -> Vec<Span<'static>> {
    let before: String = characters[..at.min(characters.len())].iter().collect();
    let Some(under) = characters.get(at) else {
        return vec![Span::raw(before), Span::styled(" ", theme::cursor())];
    };
    let after: String = characters[at + 1..].iter().collect();
    vec![
        Span::raw(before),
        Span::styled(under.to_string(), theme::cursor()),
        Span::raw(after),
    ]
}
