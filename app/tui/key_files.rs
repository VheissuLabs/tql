use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::paths;

const CERT_DIRECTORIES: [&str; 5] = [".ssh", ".postgresql", ".mysql", "certs", "Downloads"];
const CERT_EXTENSIONS: [&str; 5] = ["pem", "crt", "cer", "key", "ca"];
const SNOWFLAKE_DIRECTORIES: [&str; 2] = [".snowflake", ".ssh"];
const NOT_KEYS: [&str; 5] = [
    "known_hosts",
    "known_hosts.old",
    "config",
    "authorized_keys",
    "environment",
];

pub fn ssh_keys(home: &Path) -> Vec<String> {
    let found = files_in(&home.join(".ssh"))
        .into_iter()
        .filter(|path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            !NOT_KEYS.contains(&name.as_str()) && !name.ends_with(".pub")
        })
        .filter(|path| looks_private(path))
        .collect();
    tidy(found, home)
}

pub fn certificates(home: &Path, cwd: &Path) -> Vec<String> {
    let directories = CERT_DIRECTORIES
        .iter()
        .map(|directory| home.join(directory))
        .chain(std::iter::once(cwd.to_path_buf()));
    let found = directories
        .flat_map(|directory| files_in(&directory))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| CERT_EXTENSIONS.contains(&&*extension.to_string_lossy()))
        })
        .collect();
    tidy(found, home)
}

pub fn snowflake_keys(home: &Path, cwd: &Path) -> Vec<String> {
    let found = SNOWFLAKE_DIRECTORIES
        .iter()
        .map(|directory| home.join(directory))
        .chain(std::iter::once(cwd.to_path_buf()))
        .flat_map(|directory| files_in(&directory))
        .filter(|path| {
            first_line(path)
                .is_some_and(|line| line.contains("PRIVATE KEY") && !line.contains("OPENSSH"))
        })
        .collect();
    tidy(found, home)
}

fn files_in(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
        })
        .filter(|path| path.is_file())
        .collect()
}

fn looks_private(path: &Path) -> bool {
    first_line(path).is_some_and(|line| line.contains("PRIVATE KEY"))
}

fn first_line(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut first = Vec::new();
    let _ = BufReader::new(file.take(63)).read_until(b'\n', &mut first);
    Some(String::from_utf8_lossy(&first).into_owned())
}

fn tidy(found: Vec<PathBuf>, home: &Path) -> Vec<String> {
    let mut spelled: Vec<String> = found
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    spelled.sort();
    spelled.dedup();
    spelled
        .iter()
        .map(|path| paths::shorten_from(path, home))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Home {
        root: PathBuf,
    }

    impl Home {
        fn new(label: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("tql-keys-{label}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join(".ssh")).unwrap();
            fs::create_dir_all(root.join("certs")).unwrap();
            fs::create_dir_all(root.join("project")).unwrap();
            Home { root }
        }

        fn write(&self, path: &str, contents: &str) {
            fs::write(self.root.join(path), contents).unwrap();
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn offers_the_private_keys_in_ssh_and_nothing_else_there() {
        let home = Home::new("ssh");
        home.write(
            ".ssh/id_ed25519",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n",
        );
        home.write(".ssh/id_ed25519.pub", "ssh-ed25519 AAAA");
        home.write(".ssh/config", "Host *");
        home.write(".ssh/known_hosts", "-----BEGIN OPENSSH PRIVATE KEY-----");
        home.write(".ssh/notes.txt", "just notes");
        home.write(".ssh/deploy", "-----BEGIN RSA PRIVATE KEY-----\n");

        assert_eq!(ssh_keys(&home.root), ["~/.ssh/deploy", "~/.ssh/id_ed25519"]);
    }

    #[test]
    fn offers_certificates_from_the_usual_places_and_where_tql_started() {
        let home = Home::new("certs");
        home.write("certs/ca.pem", "");
        home.write("certs/readme.md", "");
        home.write(".ssh/client.key", "");
        home.write("project/server.crt", "");

        assert_eq!(
            certificates(&home.root, &home.root.join("project")),
            [
                "~/.ssh/client.key",
                "~/certs/ca.pem",
                "~/project/server.crt"
            ]
        );
    }

    #[test]
    fn offers_the_pem_private_keys_snowflake_can_sign_with() {
        let home = Home::new("snowflake");
        fs::create_dir_all(home.root.join(".snowflake")).unwrap();
        home.write(".snowflake/rsa_key.p8", "-----BEGIN PRIVATE KEY-----\n");
        home.write(".snowflake/rsa_key.pub", "-----BEGIN PUBLIC KEY-----\n");
        home.write(".snowflake/config.toml", "[connections]");
        home.write(".ssh/id_ed25519", "-----BEGIN OPENSSH PRIVATE KEY-----\n");
        home.write("project/sf.pem", "-----BEGIN ENCRYPTED PRIVATE KEY-----\n");

        assert_eq!(
            snowflake_keys(&home.root, &home.root.join("project")),
            ["~/.snowflake/rsa_key.p8", "~/project/sf.pem"]
        );
    }

    #[test]
    fn finds_nothing_in_a_home_without_those_directories() {
        let empty = std::env::temp_dir().join("tql-keys-nowhere-at-all");

        assert!(ssh_keys(&empty).is_empty());
        assert!(certificates(&empty, &empty).is_empty());
    }
}
