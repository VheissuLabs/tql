use super::*;
use crate::tui::grid::Tone;

pub type Overlay = (Vec<String>, Vec<Vec<String>>, Vec<Option<Tone>>);

pub struct Prompt {
    pub title: String,
    pub labels: Vec<&'static str>,
    pub values: Vec<String>,
    pub editor: LineEditor,
    purpose: Purpose,
}

enum Purpose {
    String {
        key: String,
    },
    Field {
        key: String,
        field: String,
    },
    Item {
        key: String,
        index: i64,
        was: String,
    },
    Member {
        key: String,
        was: String,
    },
    Score {
        key: String,
        member: String,
    },
    AddField {
        key: String,
    },
    PushItem {
        key: String,
    },
    AddMember {
        key: String,
    },
    AddScored {
        key: String,
    },
    NewKeyName,
    NewKeyFirst {
        key: String,
        kind: KeyKind,
    },
    Ttl {
        key: String,
    },
    FieldTtl {
        key: String,
        field: String,
    },
    Rename {
        key: String,
    },
}

impl Prompt {
    fn new(title: String, labels: Vec<&'static str>, start: &str, purpose: Purpose) -> Prompt {
        Prompt {
            title,
            labels,
            values: Vec::new(),
            editor: LineEditor::selecting(start),
            purpose,
        }
    }

    pub fn label(&self) -> &str {
        self.labels
            .get(self.values.len())
            .copied()
            .unwrap_or_default()
    }
}

impl KeyBrowser {
    fn refuses(&mut self) -> bool {
        if self.connection.read_only {
            self.fail(READ_ONLY);
            return true;
        }
        false
    }

    fn ask(&mut self, title: String, labels: Vec<&'static str>, start: &str, purpose: Purpose) {
        self.prompt = Some(Prompt::new(title, labels, start, purpose));
    }

    pub(super) fn type_into_prompt(&mut self, key: Key) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.prompt = None,
            Key::Enter => {
                prompt.values.push(prompt.editor.text());
                if prompt.values.len() < prompt.labels.len() {
                    prompt.editor = LineEditor::default();
                    return;
                }
                if let Some(prompt) = self.prompt.take() {
                    self.finish(prompt.purpose, prompt.values);
                }
            }
            other => prompt.editor.handle(other),
        }
    }

    fn finish(&mut self, purpose: Purpose, mut values: Vec<String>) {
        let mut next = || values.remove(0);
        let change = match purpose {
            Purpose::String { key } => Change::SetString { key, value: next() },
            Purpose::Field { key, field } => Change::SetField {
                key,
                field,
                value: next(),
            },
            Purpose::Item { key, index, was } => Change::SetItem {
                key,
                index,
                was,
                value: next(),
            },
            Purpose::Member { key, was } => Change::ReplaceMember {
                key,
                was,
                value: next(),
            },
            Purpose::Score { key, member } => {
                let score = next();
                if score.trim().parse::<f64>().is_err() {
                    return self.fail(format!("{score} is not a score; a score is a number"));
                }
                Change::SetScore { key, member, score }
            }
            Purpose::AddField { key } => {
                let field = next();
                Change::SetField {
                    key,
                    field,
                    value: next(),
                }
            }
            Purpose::PushItem { key } => Change::PushItem { key, value: next() },
            Purpose::AddMember { key } => Change::AddMember { key, value: next() },
            Purpose::AddScored { key } => {
                let member = next();
                let score = next();
                if score.trim().parse::<f64>().is_err() {
                    return self.fail(format!("{score} is not a score; a score is a number"));
                }
                Change::AddScored { key, member, score }
            }
            Purpose::NewKeyName => {
                let name = next();
                if name.trim().is_empty() {
                    return self.say("a key needs a name");
                }
                if self.keys.contains(&name) {
                    return self.fail(format!("{name} already exists"));
                }
                let choice = Choice::new("TYPE", kinds(), "string", false);
                self.kinds_choice = Some((name, choice));
                return;
            }
            Purpose::NewKeyFirst { key, kind } => {
                let mut first: Vec<String> = std::mem::take(&mut values);
                if kind == KeyKind::Zset
                    && first
                        .get(1)
                        .is_some_and(|score| score.trim().parse::<f64>().is_err())
                {
                    return self.fail("a score is a number");
                }
                if kind == KeyKind::Zset {
                    first.truncate(2);
                }
                self.pending.push(Change::NewKey {
                    key: key.clone(),
                    kind,
                    first,
                });
                self.select_key(&key);
                return self.say(format!("{key} will be added on :w"));
            }
            Purpose::Ttl { key } => {
                let typed = next();
                let seconds = match typed.trim() {
                    "" => None,
                    typed => match typed.parse::<i64>() {
                        Ok(seconds) if seconds > 0 => Some(seconds),
                        _ => return self.fail("a TTL is a whole number of seconds"),
                    },
                };
                match self.store.expire(&key, seconds) {
                    Ok(()) => {
                        if self.shown.as_ref().is_some_and(|shown| shown.key == key) {
                            self.reload_shown();
                        }
                        self.say(match seconds {
                            Some(seconds) => format!("{key} expires in {seconds}s"),
                            None => format!("{key} no longer expires"),
                        });
                    }
                    Err(error) => self.fail(format!("{error:#}")),
                }
                return;
            }
            Purpose::FieldTtl { key, field } => {
                let typed = next();
                let seconds = match typed.trim() {
                    "" => None,
                    typed => match typed.trim_end_matches('s').parse::<i64>() {
                        Ok(seconds) if seconds > 0 => Some(seconds),
                        _ => return self.fail("a TTL is a whole number of seconds"),
                    },
                };
                match self.store.expire_field(&key, &field, seconds) {
                    Ok(()) => {
                        self.reload_shown();
                        self.say(match seconds {
                            Some(seconds) => format!("{field} expires in {seconds}s"),
                            None => format!("{field} no longer expires"),
                        });
                    }
                    Err(error) => self.fail(format!("{error:#}")),
                }
                return;
            }
            Purpose::Rename { key } => {
                let to = next();
                if to.trim().is_empty() || to == key {
                    return;
                }
                match self.store.rename(&key, &to) {
                    Ok(()) => {
                        self.shown = None;
                        self.rescan();
                        self.say(format!("renamed {key} to {to}"));
                    }
                    Err(error) => self.fail(format!("{error:#}")),
                }
                return;
            }
        };
        self.pending.push(change);
        self.say(format!(
            "{} pending · :w writes, u drops",
            plural(self.pending.len(), "change")
        ));
    }

    fn here(&self) -> Option<(&Shown, Vec<String>)> {
        let shown = self.shown.as_ref()?;
        let (_, rows, _) = self.overlaid()?;
        let row = rows.get(shown.row)?.clone();
        Some((shown, row))
    }

    fn original_row(&self) -> Option<Vec<String>> {
        let shown = self.shown.as_ref()?;
        match &shown.contents {
            Contents::Rows { rows, .. } => rows.get(shown.row).cloned(),
            _ => None,
        }
    }

    pub(super) fn edit_here(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let key = shown.key.clone();
        if let Contents::Text(bytes) = &shown.contents {
            let current = self
                .pending
                .iter()
                .rev()
                .find_map(|change| match change {
                    Change::SetString { key: here, value } if *here == key => Some(value.clone()),
                    _ => None,
                })
                .or_else(|| String::from_utf8(bytes.clone()).ok());
            match current {
                Some(text) => self.ask(
                    format!("{key} · string"),
                    vec!["value"],
                    &text,
                    Purpose::String { key },
                ),
                None => self.say("a binary value is shown, not edited"),
            }
            return;
        }
        let kind = shown.kind.clone();
        let Some((_, row)) = self.here() else {
            return;
        };
        let original = self.original_row();
        let purpose = match kind {
            KeyKind::Hash => Purpose::Field {
                key: key.clone(),
                field: row[0].clone(),
            },
            KeyKind::List => match (original, row[0].parse::<i64>()) {
                (Some(original), Ok(index)) => Purpose::Item {
                    key: key.clone(),
                    index,
                    was: original[1].clone(),
                },
                _ => return self.say("an added item is written as typed; d drops it"),
            },
            KeyKind::Set => Purpose::Member {
                key: key.clone(),
                was: row[0].clone(),
            },
            KeyKind::Zset => Purpose::Score {
                key: key.clone(),
                member: row[0].clone(),
            },
            other => return self.say(format!("{} values are shown, not edited", other.name())),
        };
        let (label, start) = match kind {
            KeyKind::Zset => ("score", row[1].clone()),
            KeyKind::Set => ("member", row[0].clone()),
            _ => ("value", row[1].clone()),
        };
        self.ask(
            format!("{key} · {}", kind.name()),
            vec![label],
            &start,
            purpose,
        );
    }

    pub(super) fn add_here(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let key = shown.key.clone();
        let (labels, purpose) = match shown.kind {
            KeyKind::Hash => (
                vec!["field", "value"],
                Purpose::AddField { key: key.clone() },
            ),
            KeyKind::List => (vec!["item"], Purpose::PushItem { key: key.clone() }),
            KeyKind::Set => (vec!["member"], Purpose::AddMember { key: key.clone() }),
            KeyKind::Zset => (
                vec!["member", "score"],
                Purpose::AddScored { key: key.clone() },
            ),
            KeyKind::String => return self.say("a string has one value; e edits it"),
            ref other => return self.say(format!("{} values are shown, not edited", other.name())),
        };
        self.ask(format!("{key} · add"), labels, "", purpose);
    }

    pub(super) fn mark_here(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let key = shown.key.clone();
        let kind = shown.kind.clone();
        let Some(row) = self.original_row() else {
            return self.say("nothing to mark here");
        };
        let change = match kind {
            KeyKind::Hash => Change::DeleteField {
                key,
                field: row[0].clone(),
            },
            KeyKind::List => Change::RemoveItem {
                key,
                index: row[0].parse().unwrap_or_default(),
                was: row[1].clone(),
            },
            KeyKind::Set => Change::RemoveMember {
                key,
                value: row[0].clone(),
            },
            KeyKind::Zset => Change::RemoveScored {
                key,
                member: row[0].clone(),
            },
            KeyKind::String => return self.say("d on the key list deletes the whole key"),
            other => return self.say(format!("{} values are shown, not edited", other.name())),
        };
        self.toggle(change);
    }

    pub(super) fn mark_key(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(key) = self.current_key() else {
            return self.say("d deletes a key; move onto one");
        };
        self.toggle(Change::DeleteKey { key });
    }

    fn toggle(&mut self, change: Change) {
        match self.pending.iter().position(|pending| *pending == change) {
            Some(at) => {
                self.pending.remove(at);
            }
            None => self.pending.push(change),
        }
        self.say(match self.pending.len() {
            0 => "no changes pending".to_string(),
            count => format!("{} pending · :w writes, u drops", plural(count, "change")),
        });
    }

    pub(super) fn new_key(&mut self) {
        if self.refuses() {
            return;
        }
        let start = match self.current() {
            Some(Row::Space { path, .. }) => format!("{path}:"),
            Some(Row::Key { key, .. }) => tree::spaces_of(&key)
                .pop()
                .map(|space| format!("{space}:"))
                .unwrap_or_default(),
            None => String::new(),
        };
        self.prompt = Some(Prompt {
            title: "new key".to_string(),
            labels: vec!["name"],
            values: Vec::new(),
            editor: LineEditor::holding(&start),
            purpose: Purpose::NewKeyName,
        });
    }

    pub(super) fn ask_first_values(&mut self, key: String, kind: KeyKind) {
        let labels = match kind {
            KeyKind::String => vec!["value"],
            KeyKind::Hash => vec!["field", "value"],
            KeyKind::List => vec!["item"],
            KeyKind::Set => vec!["member"],
            KeyKind::Zset => vec!["member", "score"],
            _ => return,
        };
        self.ask(
            format!("{key} · {}", kind.name()),
            labels,
            "",
            Purpose::NewKeyFirst { key, kind },
        );
    }

    pub(super) fn ask_ttl(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(key) = self.current_key() else {
            return self.say("T sets a key's TTL; move onto one");
        };
        let now = self.store.ttl(&key).ok().flatten();
        self.ask(
            format!("{key} · TTL"),
            vec!["seconds, empty to keep it forever"],
            &now.map(|seconds| seconds.to_string()).unwrap_or_default(),
            Purpose::Ttl { key },
        );
    }

    pub(super) fn ask_field_ttl(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        if shown.kind != KeyKind::Hash {
            return self.say("T on a hash field sets its TTL; T on the key list sets the key's");
        }
        let key = shown.key.clone();
        if !self.store.supports("hexpire") {
            return self.say("this server has no expiry on hash fields; Redis 7.4 and Valkey 9 do");
        }
        let Some(row) = self.original_row() else {
            return;
        };
        let start = row
            .get(2)
            .filter(|ttl| *ttl != "none")
            .map(|ttl| ttl.trim_end_matches('s').to_string())
            .unwrap_or_default();
        self.ask(
            format!("{key} · {} · TTL", row[0]),
            vec!["seconds, empty to keep it forever"],
            &start,
            Purpose::FieldTtl {
                key,
                field: row[0].clone(),
            },
        );
    }

    pub(super) fn show_log(&mut self) {
        match self.store.log() {
            Ok(contents) => {
                self.shown = Some(Shown {
                    key: "command log".to_string(),
                    kind: KeyKind::Other("log".to_string()),
                    ttl: None,
                    contents,
                    row: 0,
                    offset: 0,
                });
                self.focus = Focus::Value;
            }
            Err(error) => self.fail(format!("{error:#}")),
        }
    }

    pub(super) fn ask_rename(&mut self) {
        if self.refuses() {
            return;
        }
        let Some(key) = self.current_key() else {
            return self.say("R renames a key; move onto one");
        };
        self.ask(
            format!("rename {key}"),
            vec!["new name"],
            &key.clone(),
            Purpose::Rename { key },
        );
    }

    pub(super) fn drop_pending(&mut self) {
        let count = self.pending.len();
        self.pending.clear();
        self.say(match count {
            0 => "no changes pending".to_string(),
            count => format!("dropped {}", plural(count, "change")),
        });
    }

    pub(super) fn write(&mut self) {
        if self.pending.is_empty() {
            return self.say("nothing to write");
        }
        if self.refuses() {
            return;
        }
        match self.store.write(&self.pending) {
            Ok(()) => {
                let count = self.pending.len();
                let keys_changed = self.pending.iter().any(|change| {
                    matches!(change, Change::NewKey { .. } | Change::DeleteKey { .. })
                });
                self.pending.clear();
                if keys_changed {
                    let shown = self.shown.as_ref().map(|shown| shown.key.clone());
                    self.rescan();
                    self.shown = None;
                    if let Some(key) = shown {
                        self.load(&key, 0);
                    }
                } else {
                    self.reload_shown();
                }
                self.say(format!("wrote {}", plural(count, "change")));
            }
            Err(error) => self.fail(format!("{error:#} · the changes are still pending")),
        }
    }

    pub fn overlaid(&self) -> Option<Overlay> {
        let shown = self.shown.as_ref()?;
        let Contents::Rows { headers, rows, .. } = &shown.contents else {
            return None;
        };
        let mut rows = rows.clone();
        let mut tones: Vec<Option<Tone>> = vec![None; rows.len()];
        let key = &shown.key;
        for change in self.pending.iter().filter(|change| change.key() == key) {
            let find = |rows: &Vec<Vec<String>>, column: usize, value: &str| {
                rows.iter()
                    .position(|row| row.get(column).map(String::as_str) == Some(value))
            };
            match change {
                Change::SetField { field, value, .. } => match find(&rows, 0, field) {
                    Some(at) => {
                        rows[at][1] = value.clone();
                        tones[at] = Some(Tone::Edited);
                    }
                    None => {
                        rows.push(vec![field.clone(), value.clone()]);
                        tones.push(Some(Tone::Added));
                    }
                },
                Change::DeleteField { field, .. } => {
                    if let Some(at) = find(&rows, 0, field) {
                        tones[at] = Some(Tone::Marked);
                    }
                }
                Change::SetItem { index, value, .. } => {
                    if let Some(at) = find(&rows, 0, &index.to_string()) {
                        rows[at][1] = value.clone();
                        tones[at] = Some(Tone::Edited);
                    }
                }
                Change::RemoveItem { index, .. } => {
                    if let Some(at) = find(&rows, 0, &index.to_string()) {
                        tones[at] = Some(Tone::Marked);
                    }
                }
                Change::PushItem { value, .. } => {
                    rows.push(vec!["+".to_string(), value.clone()]);
                    tones.push(Some(Tone::Added));
                }
                Change::ReplaceMember { was, value, .. } => {
                    if let Some(at) = find(&rows, 0, was) {
                        rows[at][0] = value.clone();
                        tones[at] = Some(Tone::Edited);
                    }
                }
                Change::AddMember { value, .. } => {
                    rows.push(vec![value.clone()]);
                    tones.push(Some(Tone::Added));
                }
                Change::RemoveMember { value, .. } => {
                    if let Some(at) = find(&rows, 0, value) {
                        tones[at] = Some(Tone::Marked);
                    }
                }
                Change::SetScore { member, score, .. } => {
                    if let Some(at) = find(&rows, 0, member) {
                        rows[at][1] = score.clone();
                        tones[at] = Some(Tone::Edited);
                    }
                }
                Change::AddScored { member, score, .. } => {
                    rows.push(vec![member.clone(), score.clone()]);
                    tones.push(Some(Tone::Added));
                }
                Change::RemoveScored { member, .. } => {
                    if let Some(at) = find(&rows, 0, member) {
                        tones[at] = Some(Tone::Marked);
                    }
                }
                Change::DeleteKey { .. } => {
                    tones.iter_mut().for_each(|tone| *tone = Some(Tone::Marked))
                }
                _ => {}
            }
        }
        Some((headers.clone(), rows, tones))
    }

    pub fn tone_of_key(&self, key: &str) -> Option<Tone> {
        self.pending.iter().rev().find_map(|change| match change {
            Change::DeleteKey { key: here } if here == key => Some(Tone::Marked),
            Change::NewKey { key: here, .. } if here == key => Some(Tone::Added),
            change if change.key() == key => Some(Tone::Edited),
            _ => None,
        })
    }
}

pub fn plural(count: usize, word: &str) -> String {
    match count {
        1 => format!("1 {word}"),
        count => format!("{count} {word}s"),
    }
}
