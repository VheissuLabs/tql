mod motions;
mod operators;
mod range;
mod text;
mod text_objects;

use crate::terminal::Key;

use super::editor::QueryEditor;
use motions::kind;
pub use range::Range;
use text::Text;

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

fn typed(key: Key) -> Option<char> {
    match key {
        Key::Char(character) => Some(character),
        Key::Tab => Some('\t'),
        _ => None,
    }
}

fn operator_of(key: Key) -> Option<char> {
    match key {
        Key::Char(operator @ ('d' | 'c' | 'y' | '>' | '<')) => Some(operator),
        _ => None,
    }
}

fn motion_of(key: Key) -> Option<Motion> {
    Some(match key {
        Key::Char('h') | Key::Left | Key::Backspace => Motion::Left,
        Key::Char('l' | ' ') | Key::Right => Motion::Right,
        Key::Char('j') | Key::Down => Motion::Down,
        Key::Char('k') | Key::Up => Motion::Up,
        Key::Enter => Motion::NextLine,
        Key::Char('0') => Motion::LineStart,
        Key::Char('^') => Motion::FirstNonBlank,
        Key::Char('$') => Motion::LineEnd,
        Key::Char('w') => Motion::WordForward(false),
        Key::Char('W') => Motion::WordForward(true),
        Key::Char('b') => Motion::WordBackward(false),
        Key::Char('B') => Motion::WordBackward(true),
        Key::Char('e') => Motion::WordEnd(false),
        Key::Char('E') => Motion::WordEnd(true),
        Key::Char('G') => Motion::LastLine,
        Key::Char(';') => Motion::RepeatFind(false),
        Key::Char(',') => Motion::RepeatFind(true),
        Key::Char('%') => Motion::Bracket,
        Key::Char('}') => Motion::ParagraphForward,
        Key::Char('{') => Motion::ParagraphBackward,
        _ => return None,
    })
}

fn is_change(key: Key) -> bool {
    matches!(
        key,
        Key::Char(
            'x' | 'X'
                | 'D'
                | 'C'
                | 's'
                | 'S'
                | 'r'
                | 'J'
                | '~'
                | 'p'
                | 'P'
                | 'i'
                | 'a'
                | 'I'
                | 'A'
                | 'o'
                | 'O'
        )
    )
}

fn digits_of(count: usize) -> Vec<Key> {
    count.to_string().chars().map(Key::Char).collect()
}

fn is_digit(key: &Key) -> bool {
    matches!(key, Key::Char(digit) if digit.is_ascii_digit())
}

fn read_count(keys: &[Key], at: &mut usize) -> Option<usize> {
    let mut count: Option<usize> = None;
    while let Some(Key::Char(digit)) = keys.get(*at) {
        let Some(value) = digit.to_digit(10) else {
            break;
        };
        if count.is_none() && value == 0 {
            break;
        }
        count = Some((count.unwrap_or_default() * 10 + value as usize).min(MOST_REPEATS));
        *at += 1;
    }
    count
}

fn parse_target(keys: &[Key], at: usize, allow_objects: bool) -> Parsed<Target> {
    let next = keys.get(at + 1).copied();
    match keys[at] {
        Key::Char(find @ ('f' | 't' | 'F' | 'T')) => match next {
            None => Parsed::Waiting,
            Some(wanted) => Parsed::Ready(Target::Motion(Motion::Find(find, wanted))),
        },
        Key::Char('g') => match next {
            None => Parsed::Waiting,
            Some(Key::Char('g')) => Parsed::Ready(Target::Motion(Motion::FirstLine)),
            Some(_) => Parsed::Invalid,
        },
        Key::Char(side @ ('i' | 'a')) if allow_objects => match next {
            None => Parsed::Waiting,
            Some(Key::Char(
                object @ ('w' | 'W' | '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '"' | '\''
                | '`' | 'p'),
            )) => Parsed::Ready(Target::Object(Object {
                around: side == 'a',
                kind: object,
            })),
            Some(_) => Parsed::Invalid,
        },
        key => motion_of(key).map_or(Parsed::Invalid, |motion| {
            Parsed::Ready(Target::Motion(motion))
        }),
    }
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

    fn enter_insert(&mut self, editor: &mut QueryEditor) {
        editor.checkpoint();
        self.mode = VimMode::Insert;
        self.anchor = None;
    }

    fn press_in_insert(&mut self, editor: &mut QueryEditor, key: Key) {
        if let Some(recording) = self.recording.as_mut() {
            recording.push(key);
        }
        match key {
            Key::Escape => self.leave_insert(editor),
            Key::Left | Key::Right | Key::Up | Key::Down => {
                editor.commit();
                editor.handle(key);
                editor.checkpoint();
            }
            Key::Ctrl('w') => delete_word_before(editor),
            Key::Ctrl('u') => {
                let cursor = editor.cursor();
                let start = Text::of(&editor.buffer()).line_start(cursor);
                editor.replace(start, cursor, "");
            }
            Key::Ctrl('r') => {}
            Key::ShiftEnter => editor.handle(Key::Enter),
            other => editor.handle(other),
        }
    }

    fn leave_insert(&mut self, editor: &mut QueryEditor) {
        editor.commit();
        self.mode = VimMode::Normal;
        let cursor = editor.cursor();
        if cursor > Text::of(&editor.buffer()).line_start(cursor) {
            editor.move_to(cursor - 1);
        }
        let recording = self.recording.take();
        if !self.replaying && recording.is_some() {
            self.last_change = recording;
        }
    }

    fn press_in_normal(&mut self, editor: &mut QueryEditor, key: Key) -> Outcome {
        if self.keys.is_empty() {
            match key {
                Key::Escape => return Outcome::Leave,
                Key::Tab => return Outcome::NextPane,
                Key::BackTab => return Outcome::PreviousPane,
                Key::Char(':') => return Outcome::Command,
                _ => {}
            }
        }
        if key == Key::Escape {
            self.keys.clear();
            return Outcome::Nothing;
        }

        self.keys.push(key);
        let keys = self.keys.clone();
        match self.parse(&keys, true, false) {
            Parsed::Waiting => {}
            Parsed::Invalid => self.keys.clear(),
            Parsed::Ready(command) => {
                self.keys.clear();
                self.run(editor, command);
            }
        }
        Outcome::Nothing
    }

    fn press_in_visual(&mut self, editor: &mut QueryEditor, key: Key) -> Outcome {
        if self.keys.is_empty()
            && let Some(outcome) = self.visual_key(editor, key)
        {
            return outcome;
        }
        if key == Key::Escape {
            self.keys.clear();
            return Outcome::Nothing;
        }

        self.keys.push(key);
        let keys = self.keys.clone();
        let command = match self.parse(&keys, false, true) {
            Parsed::Waiting => return Outcome::Nothing,
            Parsed::Invalid => {
                self.keys.clear();
                return Outcome::Nothing;
            }
            Parsed::Ready(command) => command,
        };
        self.keys.clear();

        match command.order {
            Order::Go(Target::Object(object)) => {
                let range = self
                    .resolve_object(editor, object)
                    .filter(|range| range.end > range.start);
                if let Some(range) = range {
                    self.anchor = Some(range.start);
                    editor.move_to(range.end - 1);
                }
            }
            Order::Go(Target::Motion(motion)) => {
                self.move_cursor(editor, motion, command.count, command.counted)
            }
            _ => {}
        }
        Outcome::Nothing
    }

    fn visual_key(&mut self, editor: &mut QueryEditor, key: Key) -> Option<Outcome> {
        match key {
            Key::Escape => self.leave_visual(),
            Key::Char(':') => return Some(Outcome::Command),
            Key::Char('o') => {
                let cursor = editor.cursor();
                editor.move_to(self.anchor.unwrap_or(cursor));
                self.anchor = Some(cursor);
            }
            Key::Char('v') => self.switch_visual(VimMode::Visual),
            Key::Char('V') => self.switch_visual(VimMode::VisualLine),
            Key::Char(operator @ ('d' | 'x' | 'y' | 'c' | '>' | '<' | '~' | 'J')) => {
                let operator = if operator == 'x' { 'd' } else { operator };
                self.operate_on_selection(editor, operator);
            }
            _ => return None,
        }
        Some(Outcome::Nothing)
    }

    fn switch_visual(&mut self, mode: VimMode) {
        if self.mode == mode {
            self.leave_visual();
            return;
        }
        self.mode = mode;
    }

    fn operate_on_selection(&mut self, editor: &mut QueryEditor, operator: char) {
        let selection = self.selection(editor);
        self.anchor = None;
        self.mode = VimMode::Normal;
        let Some(selection) = selection else {
            return;
        };
        editor.checkpoint();
        self.apply(editor, operator, selection);
        if self.mode != VimMode::Insert {
            editor.commit();
        }
    }

    fn parse(&self, keys: &[Key], allow_operators: bool, allow_objects: bool) -> Parsed<Command> {
        let mut at = 0;
        let first = read_count(keys, &mut at);
        let Some(&key) = keys.get(at) else {
            return Parsed::Waiting;
        };

        if allow_operators && let Some(operator) = operator_of(key) {
            return parse_operator(keys, at + 1, operator, first);
        }

        match parse_target(keys, at, allow_objects) {
            Parsed::Waiting => return Parsed::Waiting,
            Parsed::Ready(target) => {
                return Parsed::Ready(Command {
                    order: Order::Go(target),
                    count: first.unwrap_or(1),
                    counted: first.is_some(),
                    body: keys[at..].to_vec(),
                });
            }
            Parsed::Invalid => {}
        }

        if !allow_operators {
            return Parsed::Invalid;
        }
        let character = keys.get(at + 1).copied();
        if key == Key::Char('r') && character.is_none() {
            return Parsed::Waiting;
        }
        Parsed::Ready(Command {
            order: Order::Act(key, character),
            count: first.unwrap_or(1),
            counted: first.is_some(),
            body: keys[at..].to_vec(),
        })
    }

    fn run(&mut self, editor: &mut QueryEditor, command: Command) {
        match command.order {
            Order::Operate(operator, operand) => {
                self.operate(editor, operator, operand, command.count, command.counted);
                self.remember(&command);
            }
            Order::Go(Target::Motion(motion)) => {
                self.move_cursor(editor, motion, command.count, command.counted)
            }
            Order::Go(Target::Object(_)) => {}
            Order::Act(key, character) => {
                self.act(editor, key, character, command.count, command.counted);
                if is_change(key) {
                    self.remember(&command);
                }
            }
        }
    }

    fn remember(&mut self, command: &Command) {
        if self.replaying || matches!(command.order, Order::Operate('y', _)) {
            return;
        }
        let mut keys = if command.counted {
            digits_of(command.count)
        } else {
            Vec::new()
        };
        keys.extend(command.body.iter().copied());
        if self.mode == VimMode::Insert {
            self.recording = Some(keys);
            return;
        }
        self.last_change = Some(keys);
    }

    fn move_cursor(
        &mut self,
        editor: &mut QueryEditor,
        motion: Motion,
        count: usize,
        counted: bool,
    ) {
        let text = Text::of(&editor.buffer());
        if let Some((target, _)) =
            self.resolve_motion(&text, editor.cursor(), motion, count, counted)
        {
            editor.move_to(target);
        }
    }

    fn operate(
        &mut self,
        editor: &mut QueryEditor,
        operator: char,
        operand: Operand,
        count: usize,
        counted: bool,
    ) {
        let text = Text::of(&editor.buffer());
        let cursor = editor.cursor();
        let range = match operand {
            Operand::Lines => Some(current_lines(&text, cursor, count)),
            Operand::On(Target::Object(object)) => self.resolve_object(editor, object),
            Operand::On(Target::Motion(motion)) => {
                self.motion_range(&text, cursor, operator, motion, count, counted)
            }
        };
        let Some(range) = range else {
            return;
        };
        editor.checkpoint();
        self.apply(editor, operator, range);
        if self.mode != VimMode::Insert {
            editor.commit();
        }
    }

    fn motion_range(
        &mut self,
        text: &Text,
        cursor: usize,
        operator: char,
        motion: Motion,
        count: usize,
        counted: bool,
    ) -> Option<Range> {
        let motion = match motion {
            Motion::WordForward(big) if operator == 'c' && kind(text.at(cursor), false) != 0 => {
                Motion::WordEnd(big)
            }
            other => other,
        };
        let (target, reach) = self.resolve_motion(text, cursor, motion, count, counted)?;
        Some(match reach {
            Reach::Linewise => Range::between(cursor, target, true),
            Reach::Inclusive => {
                let last = cursor.max(target);
                let end = if text.at(last) == Some('\n') || last >= text.length() {
                    last
                } else {
                    last + 1
                };
                Range::new(cursor.min(target), end)
            }
            Reach::Exclusive if matches!(motion, Motion::WordForward(_)) && target > cursor => {
                Range::between(cursor, end_of_last_word(text, cursor, target), false)
            }
            Reach::Exclusive => Range::between(cursor, target, false),
        })
    }

    fn resolve_motion(
        &mut self,
        text: &Text,
        cursor: usize,
        motion: Motion,
        count: usize,
        counted: bool,
    ) -> Option<(usize, Reach)> {
        if !matches!(motion, Motion::Down | Motion::Up) {
            self.column = None;
        }
        let column = self.column.unwrap_or(cursor - text.line_start(cursor));
        let exclusive = |target| Some((target, Reach::Exclusive));
        let inclusive = |target| Some((target, Reach::Inclusive));
        let linewise = |target| Some((target, Reach::Linewise));

        let resolved = match motion {
            Motion::Left => exclusive(motions::left(text, cursor, count)),
            Motion::Right => exclusive(motions::right(text, cursor, count)),
            Motion::Down => self.vertical(text, cursor, count as isize, column),
            Motion::Up => self.vertical(text, cursor, -(count as isize), column),
            Motion::NextLine => motions::vertical(text, cursor, count as isize, 0)
                .and_then(|target| linewise(motions::first_non_blank(text, target))),
            Motion::LineStart => exclusive(motions::line_start(text, cursor)),
            Motion::FirstNonBlank => exclusive(motions::first_non_blank(text, cursor)),
            Motion::LineEnd => inclusive(motions::last_character(text, cursor, count)),
            Motion::WordForward(big) => exclusive(motions::word_forward(text, cursor, count, big)),
            Motion::WordBackward(big) => {
                exclusive(motions::word_backward(text, cursor, count, big))
            }
            Motion::WordEnd(big) => inclusive(motions::word_end(text, cursor, count, big)),
            Motion::LastLine => linewise(motions::line(
                text,
                if counted {
                    count - 1
                } else {
                    text.line_count() - 1
                },
            )),
            Motion::FirstLine => linewise(motions::line(text, if counted { count - 1 } else { 0 })),
            Motion::Find(find, wanted) => {
                self.last_find = Some((find, wanted));
                find_with(text, cursor, find, wanted, count)
            }
            Motion::RepeatFind(reverse) => self.last_find.and_then(|(find, wanted)| {
                let find = match (reverse, find) {
                    (false, find) => find,
                    (true, 'f') => 'F',
                    (true, 'F') => 'f',
                    (true, 't') => 'T',
                    (true, _) => 't',
                };
                find_with(text, cursor, find, wanted, count)
            }),
            Motion::Bracket => motions::matching_bracket(text, cursor).and_then(inclusive),
            Motion::ParagraphForward => exclusive(motions::paragraph_forward(text, cursor, count)),
            Motion::ParagraphBackward => {
                exclusive(motions::paragraph_backward(text, cursor, count))
            }
        };

        if motion == Motion::LineEnd {
            self.column = Some(usize::MAX);
        }
        resolved
    }

    fn vertical(
        &mut self,
        text: &Text,
        cursor: usize,
        by: isize,
        column: usize,
    ) -> Option<(usize, Reach)> {
        self.column = Some(column);
        motions::vertical(text, cursor, by, column).map(|target| (target, Reach::Linewise))
    }

    fn resolve_object(&self, editor: &QueryEditor, object: Object) -> Option<Range> {
        let text = Text::of(&editor.buffer());
        let cursor = editor.cursor();
        let around = object.around;
        match object.kind {
            'w' | 'W' => Some(text_objects::word(
                &text,
                cursor,
                around,
                object.kind == 'W',
            )),
            '(' | ')' | 'b' => text_objects::bracket(&text, cursor, '(', around),
            '[' | ']' => text_objects::bracket(&text, cursor, '[', around),
            '{' | '}' | 'B' => text_objects::bracket(&text, cursor, '{', around),
            '"' | '\'' | '`' => text_objects::quote(&text, cursor, object.kind, around),
            'p' => Some(text_objects::paragraph(&text, cursor, around)),
            _ => None,
        }
    }

    fn act(
        &mut self,
        editor: &mut QueryEditor,
        key: Key,
        character: Option<Key>,
        count: usize,
        counted: bool,
    ) {
        match key {
            Key::Char('x') => self.operate_with(editor, 'd', Motion::Right, count),
            Key::Char('X') => self.operate_with(editor, 'd', Motion::Left, count),
            Key::Char('D') => self.operate_with(editor, 'd', Motion::LineEnd, count),
            Key::Char('C') => self.operate_with(editor, 'c', Motion::LineEnd, count),
            Key::Char('s') => self.substitute(editor, count),
            Key::Char('S') => self.operate(editor, 'c', Operand::Lines, count, false),
            Key::Char('Y') => self.operate(editor, 'y', Operand::Lines, count, false),
            Key::Char('r') => replace_characters(editor, character, count),
            Key::Char('J') => self.join(editor, count),
            Key::Char('~') => self.swap_case_under(editor, count),
            Key::Char(side @ ('p' | 'P')) => self.put(editor, side == 'p', count),
            Key::Char('u') => self.undo(editor, count),
            Key::Ctrl('r') => self.redo(editor, count),
            Key::Char('.') => self.repeat(editor, counted.then_some(count)),
            Key::Char(entry @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')) => self.insert(editor, entry),
            Key::Char('v') => self.start_visual(editor, VimMode::Visual),
            Key::Char('V') => self.start_visual(editor, VimMode::VisualLine),
            _ => {}
        }
    }

    fn operate_with(
        &mut self,
        editor: &mut QueryEditor,
        operator: char,
        motion: Motion,
        count: usize,
    ) {
        if Text::of(&editor.buffer()).is_blank_line(editor.cursor()) && motion != Motion::LineEnd {
            return;
        }
        self.operate(
            editor,
            operator,
            Operand::On(Target::Motion(motion)),
            count,
            true,
        );
    }

    fn substitute(&mut self, editor: &mut QueryEditor, count: usize) {
        if Text::of(&editor.buffer()).is_blank_line(editor.cursor()) {
            self.enter_insert(editor);
            return;
        }
        self.operate_with(editor, 'c', Motion::Right, count);
    }

    fn join(&mut self, editor: &mut QueryEditor, count: usize) {
        let line = Text::of(&editor.buffer()).line_of(editor.cursor());
        editor.checkpoint();
        operators::join_lines(editor, line, line + count.saturating_sub(1).max(1));
        editor.commit();
    }

    fn swap_case_under(&mut self, editor: &mut QueryEditor, count: usize) {
        let cursor = editor.cursor();
        let end = Text::of(&editor.buffer())
            .line_end(cursor)
            .min(cursor + count);
        if end <= cursor {
            return;
        }
        editor.checkpoint();
        self.apply(editor, '~', Range::new(cursor, end));
        editor.move_to(end);
        editor.commit();
    }

    fn put(&mut self, editor: &mut QueryEditor, after: bool, count: usize) {
        if self.register.is_empty() {
            return;
        }
        let text = Text::of(&editor.buffer());
        let cursor = editor.cursor();
        editor.checkpoint();

        if self.register_is_lines {
            let lines = self.register.repeat(count);
            if after {
                let at = text.line_end(cursor);
                editor.replace(at, at, &format!("\n{}", lines.trim_end_matches('\n')));
                editor.move_to(motions::first_non_blank(
                    &Text::of(&editor.buffer()),
                    at + 1,
                ));
            } else {
                let at = text.line_start(cursor);
                editor.replace(at, at, &lines);
                editor.move_to(motions::first_non_blank(&Text::of(&editor.buffer()), at));
            }
            editor.commit();
            return;
        }

        let at = if after && !text.is_blank_line(cursor) {
            (cursor + 1).min(text.line_end(cursor))
        } else {
            cursor
        };
        let pasted = self.register.repeat(count);
        editor.replace(at, at, &pasted);
        editor.move_to((at + pasted.chars().count()).saturating_sub(1));
        editor.commit();
    }

    fn undo(&mut self, editor: &mut QueryEditor, count: usize) {
        if (0..count).any(|_| !editor.undo()) {
            self.message = Some("already at oldest change".to_string());
        }
    }

    fn redo(&mut self, editor: &mut QueryEditor, count: usize) {
        if (0..count).any(|_| !editor.redo()) {
            self.message = Some("already at newest change".to_string());
        }
    }

    fn repeat(&mut self, editor: &mut QueryEditor, count: Option<usize>) {
        let Some(mut keys) = self.last_change.clone() else {
            return;
        };
        if let Some(count) = count {
            let uncounted: Vec<Key> = keys.into_iter().skip_while(is_digit).collect();
            keys = digits_of(count);
            keys.extend(uncounted);
        }
        self.replaying = true;
        for key in keys {
            self.press(editor, key);
        }
        self.replaying = false;
    }

    fn insert(&mut self, editor: &mut QueryEditor, entry: char) {
        let text = Text::of(&editor.buffer());
        let cursor = editor.cursor();
        self.enter_insert(editor);
        match entry {
            'a' => editor.move_to((cursor + 1).min(text.line_end(cursor))),
            'I' => editor.move_to(motions::first_non_blank(&text, cursor)),
            'A' => editor.move_to(text.line_end(cursor)),
            'o' => editor.open_line(false),
            'O' => editor.open_line(true),
            _ => {}
        }
    }

    fn start_visual(&mut self, editor: &QueryEditor, mode: VimMode) {
        self.mode = mode;
        self.anchor = Some(editor.cursor());
    }
}

fn parse_operator(
    keys: &[Key],
    at: usize,
    operator: char,
    first: Option<usize>,
) -> Parsed<Command> {
    let Some(&next) = keys.get(at) else {
        return Parsed::Waiting;
    };
    if next == Key::Char(operator) {
        return Parsed::Ready(Command {
            order: Order::Operate(operator, Operand::Lines),
            count: first.unwrap_or(1),
            counted: first.is_some(),
            body: vec![next, next],
        });
    }

    let mut at = at;
    let second = read_count(keys, &mut at);
    if keys.get(at).is_none() {
        return Parsed::Waiting;
    }
    let target = match parse_target(keys, at, true) {
        Parsed::Waiting => return Parsed::Waiting,
        Parsed::Invalid => return Parsed::Invalid,
        Parsed::Ready(target) => target,
    };
    let mut body = vec![Key::Char(operator)];
    body.extend_from_slice(&keys[at..]);
    Parsed::Ready(Command {
        order: Order::Operate(operator, Operand::On(target)),
        count: (first.unwrap_or(1) * second.unwrap_or(1)).min(MOST_REPEATS),
        counted: first.is_some() || second.is_some(),
        body,
    })
}

fn current_lines(text: &Text, cursor: usize, count: usize) -> Range {
    let line = text.line_of(cursor);
    Range::lines(
        text.start_of_line(line),
        text.start_of_line((text.line_count() - 1).min(line + count - 1)),
    )
}

fn end_of_last_word(text: &Text, cursor: usize, target: usize) -> usize {
    if text.line_of(target) == text.line_of(cursor) || target >= text.length() {
        return target;
    }
    let mut end = target;
    while end > cursor && kind(text.at(end - 1), false) == 0 {
        end -= 1;
    }
    end
}

fn find_with(
    text: &Text,
    cursor: usize,
    find: char,
    wanted: Key,
    count: usize,
) -> Option<(usize, Reach)> {
    let forward = matches!(find, 'f' | 't');
    let till = matches!(find, 't' | 'T');
    let target = motions::find_in_line(text, cursor, typed(wanted)?, count, forward, till)?;
    Some((
        target,
        if forward {
            Reach::Inclusive
        } else {
            Reach::Exclusive
        },
    ))
}

fn replace_characters(editor: &mut QueryEditor, character: Option<Key>, count: usize) {
    let Some(Key::Char(replacement)) =
        character.filter(|key| matches!(key, Key::Char(character) if !character.is_control()))
    else {
        return;
    };
    let cursor = editor.cursor();
    if cursor + count > Text::of(&editor.buffer()).line_end(cursor) {
        return;
    }
    editor.checkpoint();
    editor.replace(
        cursor,
        cursor + count,
        &replacement.to_string().repeat(count),
    );
    editor.move_to(cursor + count - 1);
    editor.commit();
}

fn delete_word_before(editor: &mut QueryEditor) {
    let text = Text::of(&editor.buffer());
    let cursor = editor.cursor();
    let line_start = text.line_start(cursor);
    if cursor == line_start {
        editor.handle(Key::Backspace);
        return;
    }

    let before = |offset: usize| offset.checked_sub(1).and_then(|previous| text.at(previous));
    let mut start = cursor;
    while start > line_start && kind(before(start), false) == 0 {
        start -= 1;
    }
    let word = kind(before(start), false);
    while start > line_start && word != 0 && kind(before(start), false) == word {
        start -= 1;
    }
    editor.replace(start, cursor, "");
}

fn keep_on_character(editor: &mut QueryEditor) {
    let text = Text::of(&editor.buffer());
    let cursor = editor.cursor();
    let end = text.line_end(cursor);
    if cursor >= end && end > text.line_start(cursor) {
        editor.move_to(end - 1);
    }
}
