use crate::encrypter::Encrypter;
use crate::store::Connection;

mod redis;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyKind {
    String,
    Hash,
    List,
    Set,
    Zset,
    Stream,
    Other(String),
}

impl KeyKind {
    pub fn of(name: &str) -> KeyKind {
        match name {
            "string" => KeyKind::String,
            "hash" => KeyKind::Hash,
            "list" => KeyKind::List,
            "set" => KeyKind::Set,
            "zset" => KeyKind::Zset,
            "stream" => KeyKind::Stream,
            other => KeyKind::Other(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            KeyKind::String => "string",
            KeyKind::Hash => "hash",
            KeyKind::List => "list",
            KeyKind::Set => "set",
            KeyKind::Zset => "zset",
            KeyKind::Stream => "stream",
            KeyKind::Other(name) => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scanned {
    pub cursor: u64,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contents {
    Text(Vec<u8>),
    Long {
        head: Vec<u8>,
        size: u64,
    },
    Rows {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        binary: Vec<bool>,
        more: bool,
    },
    Missing,
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    SetString {
        key: String,
        value: String,
    },
    SetField {
        key: String,
        field: String,
        value: String,
    },
    DeleteField {
        key: String,
        field: String,
    },
    SetItem {
        key: String,
        index: i64,
        was: String,
        value: String,
    },
    PushItem {
        key: String,
        value: String,
    },
    RemoveItem {
        key: String,
        index: i64,
        was: String,
    },
    ReplaceMember {
        key: String,
        was: String,
        value: String,
    },
    AddMember {
        key: String,
        value: String,
    },
    RemoveMember {
        key: String,
        value: String,
    },
    SetScore {
        key: String,
        member: String,
        score: String,
    },
    AddScored {
        key: String,
        member: String,
        score: String,
    },
    RemoveScored {
        key: String,
        member: String,
    },
    NewKey {
        key: String,
        kind: KeyKind,
        first: Vec<String>,
    },
    DeleteKey {
        key: String,
    },
}

impl Change {
    pub fn key(&self) -> &str {
        match self {
            Change::SetString { key, .. }
            | Change::SetField { key, .. }
            | Change::DeleteField { key, .. }
            | Change::SetItem { key, .. }
            | Change::PushItem { key, .. }
            | Change::RemoveItem { key, .. }
            | Change::ReplaceMember { key, .. }
            | Change::AddMember { key, .. }
            | Change::RemoveMember { key, .. }
            | Change::SetScore { key, .. }
            | Change::AddScored { key, .. }
            | Change::RemoveScored { key, .. }
            | Change::NewKey { key, .. }
            | Change::DeleteKey { key } => key,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Nil,
    Text(String),
    Number(i64),
    List(Vec<Reply>),
    Map(Vec<(Reply, Reply)>),
    Error(String),
}

#[derive(Debug)]
pub struct PartlyWritten(pub String);

impl std::fmt::Display for PartlyWritten {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "{} · Redis may have written some of the changes, so tql dropped them and shows what is there now",
            self.0
        )
    }
}

impl std::error::Error for PartlyWritten {}

pub fn score(typed: &str) -> Option<String> {
    let trimmed = typed.trim();
    if matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "inf" | "+inf" | "-inf"
    ) {
        return Some(trimmed.to_string());
    }
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
        .map(|_| trimmed.to_string())
}

pub trait KeyStore: Send {
    fn scan(&mut self, cursor: u64, pattern: &str, count: usize) -> anyhow::Result<Scanned>;
    fn size(&mut self) -> anyhow::Result<u64>;
    fn kinds(&mut self, keys: &[String]) -> anyhow::Result<Vec<KeyKind>>;
    fn ttl(&mut self, key: &str) -> anyhow::Result<Option<i64>>;
    fn read(
        &mut self,
        key: &str,
        kind: &KeyKind,
        offset: usize,
        limit: usize,
    ) -> anyhow::Result<Contents>;
    fn write(&mut self, changes: &[Change]) -> anyhow::Result<()>;
    fn expire(&mut self, key: &str, seconds: Option<i64>) -> anyhow::Result<()>;
    fn rename(&mut self, from: &str, to: &str) -> anyhow::Result<()>;
    fn databases(&mut self) -> anyhow::Result<Vec<(u32, u64)>>;
    fn select(&mut self, database: u32) -> anyhow::Result<()>;
    fn run(&mut self, line: &str) -> anyhow::Result<Reply>;
    fn reads_only(&mut self, command: &str, subcommand: Option<&str>) -> anyhow::Result<bool>;
    fn server(&mut self) -> String;
    fn fork(&mut self) -> anyhow::Result<Box<dyn KeyStore>>;

    fn supports(&mut self, _command: &str) -> bool {
        false
    }

    fn expire_field(
        &mut self,
        _key: &str,
        _field: &str,
        _seconds: Option<i64>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("this server has no expiry on hash fields; Redis 7.4 and Valkey 9 do")
    }

    fn log(&mut self) -> anyhow::Result<Contents> {
        anyhow::bail!("this server keeps no command log")
    }
}

pub const BLOCKING: [&str; 15] = [
    "subscribe",
    "psubscribe",
    "ssubscribe",
    "monitor",
    "sync",
    "psync",
    "blpop",
    "brpop",
    "blmove",
    "brpoplpush",
    "bzpopmin",
    "bzpopmax",
    "bzmpop",
    "blmpop",
    "wait",
];

pub fn blocks(words: &[String]) -> bool {
    let Some(first) = words.first().map(|word| word.to_lowercase()) else {
        return false;
    };
    if BLOCKING.contains(&first.as_str()) || first == "waitaof" {
        return true;
    }
    matches!(first.as_str(), "xread" | "xreadgroup")
        && words.iter().any(|word| word.eq_ignore_ascii_case("block"))
}

pub fn redacted(line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    let lowered: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();
    let keep = |count: usize| format!("{} •••", words[..count.min(words.len())].join(" "));
    match lowered
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["auth", ..] => keep(1),
        ["acl", "setuser", ..] => keep(3),
        ["config", "set", setting, ..] if setting.contains("pass") || setting.contains("auth") => {
            keep(3)
        }
        _ => match lowered
            .iter()
            .position(|word| word == "auth" || word == "auth2")
        {
            Some(at) if at > 0 => keep(at + 1),
            _ => line.to_string(),
        },
    }
}

pub const CONNECTION_STATE: [&str; 10] = [
    "multi", "exec", "discard", "watch", "unwatch", "reset", "hello", "quit", "client", "auth",
];

pub fn words(line: &str) -> anyhow::Result<Vec<String>> {
    Ok(raw_words(line)?
        .into_iter()
        .map(|word| String::from_utf8_lossy(&word).into_owned())
        .collect())
}

pub(crate) fn raw_words(line: &str) -> anyhow::Result<Vec<Vec<u8>>> {
    let mut words = Vec::new();
    let mut current: Option<Vec<u8>> = None;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' | '\'' => {
                let quote = character;
                let word = current.get_or_insert_with(Vec::new);
                let mut closed = false;
                while let Some(inner) = characters.next() {
                    if inner == quote {
                        closed = true;
                        break;
                    }
                    if inner == '\\' && quote == '\'' && characters.peek() == Some(&'\'') {
                        characters.next();
                        word.push(b'\'');
                        continue;
                    }
                    if inner == '\\' && quote == '"' {
                        match characters.next() {
                            Some('n') => word.push(b'\n'),
                            Some('t') => word.push(b'\t'),
                            Some('r') => word.push(b'\r'),
                            Some('x') => {
                                let hex: String = [characters.next(), characters.next()]
                                    .into_iter()
                                    .flatten()
                                    .collect();
                                word.push(
                                    u8::from_str_radix(&hex, 16)
                                        .map_err(|_| anyhow::anyhow!("\\x needs two hex digits"))?,
                                );
                            }
                            Some(other) => {
                                let mut buffer = [0; 4];
                                word.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
                            }
                            None => break,
                        }
                        continue;
                    }
                    let mut buffer = [0; 4];
                    word.extend_from_slice(inner.encode_utf8(&mut buffer).as_bytes());
                }
                if !closed {
                    anyhow::bail!("a quote is not closed");
                }
            }
            whitespace if whitespace.is_whitespace() => {
                if let Some(word) = current.take() {
                    words.push(word);
                }
            }
            other => {
                let mut buffer = [0; 4];
                current
                    .get_or_insert_with(Vec::new)
                    .extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    if let Some(word) = current {
        words.push(word);
    }
    Ok(words)
}

pub fn connect(
    connection: &Connection,
    encrypter: Option<&Encrypter>,
) -> anyhow::Result<Box<dyn KeyStore>> {
    Ok(Box::new(redis::Redis::open(connection, encrypter)?))
}

pub fn shown(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text)
            if !text.chars().any(|character| {
                character.is_control() && !matches!(character, '\n' | '\t' | '\r')
            }) =>
        {
            text.to_string()
        }
        _ => format!(
            "0x{}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_a_score_redis_takes() {
        assert_eq!(score(" 5 ").as_deref(), Some("5"));
        assert_eq!(score("-inf").as_deref(), Some("-inf"));
        assert_eq!(score("2.5e3").as_deref(), Some("2.5e3"));
        assert_eq!(score("NaN"), None);
        assert_eq!(score("1e400"), None);
        assert_eq!(score("five"), None);
    }

    #[test]
    fn splits_a_command_line_the_way_redis_cli_does() {
        assert_eq!(
            words(r#"SET "a key" 'it\'s' plain"#).unwrap(),
            ["SET", "a key", "it's", "plain"]
        );
        assert_eq!(raw_words(r#"SET k "\xff\x00""#).unwrap()[2], [0xff, 0x00]);
        assert!(words(r#"GET "open"#).is_err());
    }

    #[test]
    fn knows_which_commands_block() {
        let line = |text: &str| words(text).unwrap();
        assert!(blocks(&line("BLPOP q 0")));
        assert!(blocks(&line("subscribe news")));
        assert!(blocks(&line("XREAD BLOCK 0 STREAMS s $")));
        assert!(!blocks(&line("XREAD COUNT 1 STREAMS s 0")));
        assert!(!blocks(&line("GET k")));
    }

    #[test]
    fn keeps_passwords_out_of_what_it_remembers() {
        assert_eq!(redacted("AUTH alice s3cret"), "AUTH •••");
        assert_eq!(redacted("auth s3cret"), "auth •••");
        assert_eq!(redacted("HELLO 3 AUTH alice s3cret"), "HELLO 3 AUTH •••");
        assert_eq!(
            redacted("ACL SETUSER bob on >pw ~* +@all"),
            "ACL SETUSER bob •••"
        );
        assert_eq!(
            redacted("CONFIG SET requirepass hunter2"),
            "CONFIG SET requirepass •••"
        );
        assert_eq!(
            redacted("MIGRATE h 6379 k 0 5000 AUTH pw"),
            "MIGRATE h 6379 k 0 5000 AUTH •••"
        );
        assert_eq!(redacted("GET user:1"), "GET user:1");
    }

    #[test]
    fn shows_binary_as_hex_and_text_as_text() {
        assert_eq!(shown(b"hello"), "hello");
        assert_eq!(shown(&[0xff, 0x00]), "0xff00");
        assert_eq!(shown(b"a\x00b"), "0x610062");
    }
}
