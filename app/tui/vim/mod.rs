mod insert;
mod keys;
mod motions;
mod normal;
mod operators;
mod range;
mod resolve;
mod text;
mod text_objects;
mod visual;

use crate::terminal::Key;

use super::editor::QueryEditor;
use motions::kind;
pub use range::Range;
use text::Text;

use insert::*;
use keys::*;

const MOST_REPEATS: usize = 9999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl VimMode {
    pub fn label(self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
            VimMode::VisualLine => "VISUAL LINE",
        }
    }

    pub fn is_visual(self) -> bool {
        matches!(self, VimMode::Visual | VimMode::VisualLine)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Leave,
    NextPane,
    PreviousPane,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Down,
    Up,
    NextLine,
    LineStart,
    FirstNonBlank,
    LineEnd,
    WordForward(bool),
    WordBackward(bool),
    WordEnd(bool),
    LastLine,
    FirstLine,
    Find(char, Key),
    RepeatFind(bool),
    Bracket,
    ParagraphForward,
    ParagraphBackward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Exclusive,
    Inclusive,
    Linewise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Object {
    around: bool,
    kind: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Motion(Motion),
    Object(Object),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operand {
    Lines,
    On(Target),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    Go(Target),
    Operate(char, Operand),
    Act(Key, Option<Key>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Command {
    order: Order,
    count: usize,
    counted: bool,
    body: Vec<Key>,
}

enum Parsed<Found> {
    Waiting,
    Invalid,
    Ready(Found),
}

#[derive(Debug, Clone, Default)]
pub struct Vim {
    pub mode: VimMode,
    pub register: String,
    pub register_is_lines: bool,
    pub message: Option<String>,
    copied: Option<String>,
    keys: Vec<Key>,
    anchor: Option<usize>,
    column: Option<usize>,
    last_find: Option<(char, Key)>,
    last_change: Option<Vec<Key>>,
    recording: Option<Vec<Key>>,
    replaying: bool,
}

impl Vim {
    pub fn reset(&mut self) {
        self.mode = VimMode::Normal;
        self.keys.clear();
        self.anchor = None;
        self.recording = None;
    }

    pub fn rest(&self, editor: &mut QueryEditor) {
        if self.mode != VimMode::Insert {
            keep_on_character(editor);
        }
    }

    pub fn take_copied(&mut self) -> Option<String> {
        self.copied.take()
    }

    pub fn press(&mut self, editor: &mut QueryEditor, key: Key) -> Outcome {
        self.message = None;
        if self.mode == VimMode::Insert {
            self.press_in_insert(editor, key);
            return Outcome::Nothing;
        }

        let outcome = if self.mode == VimMode::Normal {
            self.press_in_normal(editor, key)
        } else {
            self.press_in_visual(editor, key)
        };
        if self.mode != VimMode::Insert {
            keep_on_character(editor);
        }
        outcome
    }

    pub fn selection(&self, editor: &QueryEditor) -> Option<Range> {
        let anchor = self.anchor?;
        let text = Text::of(&editor.buffer());
        let from = anchor.min(editor.cursor());
        let to = anchor.max(editor.cursor());
        if self.mode == VimMode::VisualLine {
            return Some(Range::lines(text.line_start(from), text.line_end(to)));
        }
        Some(Range::new(from, text.length().min(to + 1)))
    }

    pub fn selected_text(&self, editor: &QueryEditor) -> Option<String> {
        let range = self.selection(editor)?;
        Some(Text::of(&editor.buffer()).slice(range.start, range.end))
    }

    pub fn leave_visual(&mut self) {
        if self.anchor.take().is_some() {
            self.mode = VimMode::Normal;
        }
    }

    fn store(&mut self, taken: &str, lines: bool) {
        self.register = taken.to_string();
        self.register_is_lines = lines;
    }

    fn yanked(&mut self, taken: &str) {
        self.copied = Some(taken.to_string());
        let lines = taken.trim_end_matches('\n').matches('\n').count() + 1;
        let characters = taken.chars().count();
        self.message = Some(if self.register_is_lines {
            format!(
                "yanked {lines} {}",
                if lines == 1 { "line" } else { "lines" }
            )
        } else {
            format!(
                "yanked {characters} {}",
                if characters == 1 {
                    "character"
                } else {
                    "characters"
                }
            )
        });
    }
}
