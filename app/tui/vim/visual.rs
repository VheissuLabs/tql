use super::*;

impl Vim {
    pub(super) fn press_in_visual(&mut self, editor: &mut QueryEditor, key: Key) -> Outcome {
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

    pub(super) fn visual_key(&mut self, editor: &mut QueryEditor, key: Key) -> Option<Outcome> {
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

    pub(super) fn switch_visual(&mut self, mode: VimMode) {
        if self.mode == mode {
            self.leave_visual();
            return;
        }
        self.mode = mode;
    }

    pub(super) fn operate_on_selection(&mut self, editor: &mut QueryEditor, operator: char) {
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

    pub(super) fn start_visual(&mut self, editor: &QueryEditor, mode: VimMode) {
        self.mode = mode;
        self.anchor = Some(editor.cursor());
    }
}
