use super::*;

impl Vim {
    pub(super) fn move_cursor(
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

    pub(super) fn operate(
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

    pub(super) fn motion_range(
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

    pub(super) fn resolve_motion(
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

    pub(super) fn vertical(
        &mut self,
        text: &Text,
        cursor: usize,
        by: isize,
        column: usize,
    ) -> Option<(usize, Reach)> {
        self.column = Some(column);
        motions::vertical(text, cursor, by, column).map(|target| (target, Reach::Linewise))
    }

    pub(super) fn resolve_object(&self, editor: &QueryEditor, object: Object) -> Option<Range> {
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
}

pub(super) fn current_lines(text: &Text, cursor: usize, count: usize) -> Range {
    let line = text.line_of(cursor);
    Range::lines(
        text.start_of_line(line),
        text.start_of_line((text.line_count() - 1).min(line + count - 1)),
    )
}
pub(super) fn end_of_last_word(text: &Text, cursor: usize, target: usize) -> usize {
    if text.line_of(target) == text.line_of(cursor) || target >= text.length() {
        return target;
    }
    let mut end = target;
    while end > cursor && kind(text.at(end - 1), false) == 0 {
        end -= 1;
    }
    end
}
pub(super) fn find_with(
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
