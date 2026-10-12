use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Space {
        path: String,
        name: String,
        depth: usize,
        count: usize,
        open: bool,
    },
    Key {
        key: String,
        name: String,
        depth: usize,
    },
}

#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    key: Option<String>,
    count: usize,
}

pub struct Folds<'a> {
    pub opened: &'a HashSet<String>,
    pub closed: &'a HashSet<String>,
    pub open_by_default: bool,
}

impl Folds<'_> {
    fn is_open(&self, path: &str) -> bool {
        if self.closed.contains(path) {
            return false;
        }
        self.opened.contains(path) || self.open_by_default
    }
}

pub fn rows(keys: &[String], folds: &Folds) -> Vec<Row> {
    let mut root = Node::default();
    for key in keys {
        let parts: Vec<&str> = key.split(':').collect();
        let mut node = &mut root;
        node.count += 1;
        for (at, part) in parts.iter().enumerate() {
            let last = at + 1 == parts.len();
            let name = if last {
                format!("\u{0}{part}")
            } else {
                part.to_string()
            };
            node = node.children.entry(name).or_default();
            if last {
                node.key = Some(key.clone());
            } else {
                node.count += 1;
            }
        }
    }
    let mut rows = Vec::new();
    walk(&root, "", 0, folds, &mut rows);
    rows
}

fn walk(node: &Node, path: &str, depth: usize, folds: &Folds, rows: &mut Vec<Row>) {
    let mut ordered: Vec<(&String, &Node)> = node.children.iter().collect();
    ordered.sort_by(|(left, _), (right, _)| {
        left.trim_start_matches('\u{0}')
            .to_lowercase()
            .cmp(&right.trim_start_matches('\u{0}').to_lowercase())
            .then(right.starts_with('\u{0}').cmp(&left.starts_with('\u{0}')))
    });
    for (name, child) in ordered {
        match &child.key {
            Some(key) => rows.push(Row::Key {
                key: key.clone(),
                name: name.trim_start_matches('\u{0}').to_string(),
                depth,
            }),
            None => {
                let here = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}:{name}")
                };
                let open = folds.is_open(&here);
                rows.push(Row::Space {
                    path: here.clone(),
                    name: name.clone(),
                    depth,
                    count: child.count,
                    open,
                });
                if open {
                    walk(child, &here, depth + 1, folds, rows);
                }
            }
        }
    }
}

pub fn spaces_of(key: &str) -> Vec<String> {
    let parts: Vec<&str> = key.split(':').collect();
    (1..parts.len()).map(|end| parts[..end].join(":")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn shown(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Space {
                    name,
                    depth,
                    count,
                    open,
                    ..
                } => format!(
                    "{}{} {name} ({count})",
                    "  ".repeat(*depth),
                    if *open { "▾" } else { "▸" }
                ),
                Row::Key { name, depth, .. } => format!("{}{name}", "  ".repeat(*depth)),
            })
            .collect()
    }

    #[test]
    fn nests_keys_under_their_namespaces_with_counts() {
        let none = HashSet::new();
        let folds = Folds {
            opened: &none,
            closed: &none,
            open_by_default: true,
        };
        assert_eq!(
            shown(&rows(
                &keys(&["user:1", "user:2", "user:sessions:a", "config", "user"]),
                &folds
            )),
            [
                "config",
                "user",
                "▾ user (3)",
                "  1",
                "  2",
                "  ▾ sessions (1)",
                "    a",
            ]
        );
    }

    #[test]
    fn folded_namespaces_hide_their_keys() {
        let none = HashSet::new();
        let opened: HashSet<String> = ["user".to_string()].into();
        let folds = Folds {
            opened: &opened,
            closed: &none,
            open_by_default: false,
        };
        assert_eq!(
            shown(&rows(&keys(&["user:1", "cache:a", "cache:b"]), &folds)),
            ["▸ cache (2)", "▾ user (1)", "  1"]
        );
        assert_eq!(spaces_of("a:b:c"), ["a", "a:b"]);
    }
}
