use super::*;

impl Picker {
    pub(super) fn entries(&self, unfolded: bool) -> Vec<Entry> {
        let mut ungrouped = Vec::new();
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (index, connection) in self.connections.iter().enumerate() {
            let group = connection.group_name.clone().unwrap_or_default();
            if group.is_empty() {
                ungrouped.push(Entry::Connection {
                    index,
                    group: String::new(),
                });
                continue;
            }
            match groups.iter_mut().find(|(name, _)| *name == group) {
                Some((_, members)) => members.push(index),
                None => groups.push((group, vec![index])),
            }
        }
        groups.sort_by_key(|(name, _)| name.to_lowercase());

        let mut entries = ungrouped;
        for (group, members) in groups {
            let collapsed = !unfolded && self.is_collapsed(&group);
            entries.push(Entry::Header {
                group: group.clone(),
                count: members.len(),
                collapsed,
            });
            if !collapsed {
                entries.extend(members.into_iter().map(|index| Entry::Connection {
                    index,
                    group: group.clone(),
                }));
            }
        }
        entries
    }

    pub fn is_collapsed(&self, group: &str) -> bool {
        self.collapsed.iter().any(|collapsed| collapsed == group)
    }

    pub(super) fn current(&self) -> Option<&Connection> {
        match self.entries(false).get(self.index)? {
            Entry::Connection { index, .. } => self.connections.get(*index),
            Entry::Header { .. } => None,
        }
    }

    pub(super) fn index_of(&self, id: Option<i64>) -> Option<usize> {
        self.entries(false).iter().position(|entry| {
            matches!(entry, Entry::Connection { index, .. } if Some(self.connections[*index].id) == id)
        })
    }

    pub(super) fn header_index(&self, wanted: &str) -> usize {
        self.entries(false)
            .iter()
            .position(|entry| matches!(entry, Entry::Header { group, .. } if group == wanted))
            .unwrap_or(self.index)
    }

    pub(super) fn groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = Vec::new();
        for group in self
            .connections
            .iter()
            .filter_map(|connection| connection.group_name.clone())
        {
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        groups.sort_by_key(|group| group.to_lowercase());
        groups
    }

    pub(super) fn remember_collapsed(&self) {
        if let Some((store, _)) = &self.store {
            Remembered::new(store).write(
                COLLAPSED,
                &serde_json::to_string(&self.collapsed).unwrap_or_default(),
            );
        }
    }

    pub(super) fn fold(&mut self, shut: Option<bool>) {
        let (group, header) = match self.entries(false).get(self.index) {
            Some(Entry::Header { group, .. }) => (group.clone(), true),
            Some(Entry::Connection { group, .. }) => (group.clone(), false),
            None => return,
        };
        if group.is_empty() || (!header && shut != Some(true)) {
            return;
        }

        let shut = shut.unwrap_or(!self.is_collapsed(&group));
        self.collapsed.retain(|collapsed| *collapsed != group);
        if shut {
            self.collapsed.push(group.clone());
        }
        self.remember_collapsed();
        self.index = self.header_index(&group);
    }
}
