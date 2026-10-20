use super::*;

impl Picker {
    pub(super) fn create(&mut self) {
        self.form = Some(ConnectionForm::creating(self.places.clone()).with_groups(self.groups()));
        self.status = None;
    }

    pub(super) fn edit(&mut self) {
        if let Some(Entry::Header { group, .. }) = self.entries(false).get(self.index) {
            self.renaming = Some(GroupRename::new(group));
            self.status = None;
            return;
        }
        let Some(connection) = self.current() else {
            return;
        };
        let encrypter = self
            .store
            .as_ref()
            .and_then(|(_, key)| Encrypter::from_file(key).ok());
        self.form = Some(
            ConnectionForm::editing(connection, encrypter.as_ref(), self.places.clone())
                .with_groups(self.groups()),
        );
        self.status = None;
    }

    pub(super) fn mark(&mut self) {
        let Some(id) = self.current().map(|connection| connection.id) else {
            return;
        };
        match self.pending_deletes.iter().position(|marked| *marked == id) {
            Some(position) => {
                self.pending_deletes.remove(position);
            }
            None => self.pending_deletes.push(id),
        }
        self.move_by(1);
        let count = self.pending_deletes.len();
        self.say(match count {
            0 => "marks cleared".to_string(),
            count => format!("{count} marked for deletion · :w writes · u clears"),
        });
    }

    pub(super) fn unmark_all(&mut self) {
        self.pending_deletes.clear();
        self.say("marks cleared");
    }

    pub(super) fn sign_out(&mut self) {
        let Some(connection) = self.current().cloned() else {
            self.say("highlight a connection to sign out of");
            return;
        };
        if connection.sign_in.is_none() {
            self.say(format!("{} has no browser sign-in saved", connection.name));
            return;
        }
        let forgotten = self
            .writer()
            .and_then(|writer| {
                writer.update(
                    connection.id,
                    &[("sign_in", crate::store_writer::Field::Null)],
                )
            })
            .and_then(|_| self.reload());
        if let Err(error) = forgotten {
            self.say(format!("{error:#}"));
            return;
        }
        crate::database::snowflake_oauth::forget_all();
        self.say(format!(
            "signed out of {}; it signs in through the browser when it next opens",
            connection.name
        ));
    }

    pub(super) fn write_pending(&mut self) {
        if self.pending_deletes.is_empty() {
            self.say("nothing to write");
            return;
        }
        let count = self.pending_deletes.len();
        let deleted = self
            .writer()
            .and_then(|writer| writer.delete(&self.pending_deletes))
            .and_then(|_| self.reload());
        if let Err(error) = deleted {
            self.say(format!("{error:#}"));
            return;
        }
        self.pending_deletes.clear();
        self.index = self.index.min(self.entries(false).len().saturating_sub(1));
        self.say(format!(
            "deleted {count} connection{}",
            if count == 1 { "" } else { "s" }
        ));
    }

    pub(super) fn on_form_key(&mut self, key: Key) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match form.on_key(key) {
            Step::Stay => {}
            Step::Cancel => self.close_form("nothing changed".to_string()),
            Step::Save => self.save_form(),
        }
    }

    pub(super) fn save_form(&mut self) {
        let writer = self.writer();
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let saved = writer.and_then(|writer| form.save(&writer));
        let refusal = match saved {
            Ok(refusal) => refusal,
            Err(error) => Some(format!("{error:#}")),
        };
        if let Some(refusal) = refusal {
            form.error = Some(refusal);
            return;
        }

        let creating = form.creating;
        let id = form.id;
        let name = form.name().to_string();
        let group = form.value("group_name").trim().to_string();
        if let Err(error) = self.reload() {
            self.say(format!("{error:#}"));
        }
        if !group.is_empty() && self.is_collapsed(&group) {
            self.collapsed.retain(|collapsed| *collapsed != group);
            self.remember_collapsed();
        }
        self.index = self.index_of(id).unwrap_or(0);
        self.close_form(format!(
            "{} {name}",
            if creating { "added" } else { "saved" }
        ));
    }

    pub(super) fn on_rename_key(&mut self, key: Key) {
        let Some(renaming) = self.renaming.as_mut() else {
            return;
        };
        match renaming.on_key(key) {
            group_rename::Step::Stay => {}
            group_rename::Step::Cancel => {
                self.renaming = None;
                self.say("nothing changed");
            }
            group_rename::Step::Save => self.rename_group(),
        }
    }

    pub(super) fn rename_group(&mut self) {
        let writer = self.writer();
        let groups = self.groups();
        let Some(renaming) = self.renaming.as_mut() else {
            return;
        };
        let (from, to) = (renaming.from.clone(), renaming.name());
        if to.is_empty() {
            renaming.error = Some("a group needs a name".to_string());
            return;
        }
        if to == from {
            self.renaming = None;
            self.say("nothing changed");
            return;
        }
        let merging = groups.contains(&to);
        if let Err(error) = writer.and_then(|writer| writer.rename_group(&from, &to)) {
            renaming.error = Some(format!("{error:#}"));
            return;
        }
        self.renaming = None;
        if let Err(error) = self.reload() {
            self.say(format!("{error:#}"));
            return;
        }
        let was_collapsed = self.is_collapsed(&from);
        self.collapsed.retain(|collapsed| *collapsed != from);
        if was_collapsed && !self.is_collapsed(&to) && !merging {
            self.collapsed.push(to.clone());
        }
        self.remember_collapsed();
        self.index = self.header_index(&to);
        self.say(match merging {
            true => format!("moved {from} into {to}"),
            false => format!("renamed {from} to {to}"),
        });
    }

    pub(super) fn close_form(&mut self, status: String) {
        self.form = None;
        self.say(status);
    }
}
