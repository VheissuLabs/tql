use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::anyhow;

const QUOTES: &[u8] = b"'\"`";
const CHUNK: usize = 1 << 16;

pub struct StatementReader {
    backslash_escapes: bool,
}

pub struct Statements<Source: Read> {
    source: Source,
    chunk: Vec<u8>,
    at: usize,
    finished: bool,
    failed: Option<std::io::Error>,
    backslash_escapes: bool,
    statement: Vec<u8>,
    quote: Option<u8>,
    escaped: bool,
    in_comment: bool,
    previous: Option<u8>,
}

impl StatementReader {
    pub fn new(backslash_escapes: bool) -> Self {
        StatementReader { backslash_escapes }
    }

    pub fn read(&self, path: &Path) -> anyhow::Result<Statements<BufReader<File>>> {
        let file = File::open(path).map_err(|_| anyhow!("Could not read [{}].", path.display()))?;
        Ok(self.statements(BufReader::new(file)))
    }

    pub fn split(&self, sql: &str) -> Vec<String> {
        self.statements(sql.as_bytes())
            .collect::<anyhow::Result<Vec<_>>>()
            .unwrap_or_default()
    }

    fn statements<Source: Read>(&self, source: Source) -> Statements<Source> {
        Statements {
            source,
            chunk: Vec::new(),
            at: 0,
            finished: false,
            failed: None,
            backslash_escapes: self.backslash_escapes,
            statement: Vec::new(),
            quote: None,
            escaped: false,
            in_comment: false,
            previous: None,
        }
    }
}

impl<Source: Read> Statements<Source> {
    fn next_byte(&mut self) -> Option<u8> {
        if self.at >= self.chunk.len() {
            if self.finished {
                return None;
            }
            self.chunk.resize(CHUNK, 0);
            match self.source.read(&mut self.chunk) {
                Ok(0) => {
                    self.finished = true;
                    self.chunk.clear();
                    return None;
                }
                Ok(read) => self.chunk.truncate(read),
                Err(error) => {
                    self.finished = true;
                    self.chunk.clear();
                    self.failed = Some(error);
                    return None;
                }
            }
            self.at = 0;
        }
        self.at += 1;
        Some(self.chunk[self.at - 1])
    }

    fn take(&mut self) -> Option<String> {
        let statement = String::from_utf8_lossy(&std::mem::take(&mut self.statement))
            .trim()
            .to_string();
        (!statement.is_empty()).then_some(statement)
    }
}

impl<Source: Read> Iterator for Statements<Source> {
    type Item = anyhow::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(byte) = self.next_byte() {
            if self.in_comment {
                self.in_comment = byte != b'\n';
                self.previous = Some(byte);
                continue;
            }

            if let Some(quote) = self.quote {
                self.statement.push(byte);
                if !self.escaped && byte == quote {
                    self.quote = None;
                }
                self.escaped = !self.escaped && self.backslash_escapes && byte == b'\\';
                self.previous = Some(byte);
                continue;
            }

            if byte == b'-' && self.previous == Some(b'-') && self.statement.ends_with(b"-") {
                self.statement.pop();
                self.in_comment = true;
                self.previous = None;
                continue;
            }

            if byte == b';' {
                self.previous = Some(byte);
                if let Some(statement) = self.take() {
                    return Some(Ok(statement));
                }
                continue;
            }

            if QUOTES.contains(&byte) {
                self.quote = Some(byte);
            }
            self.statement.push(byte);
            self.previous = Some(byte);
        }

        if let Some(error) = self.failed.take() {
            return Some(Err(error.into()));
        }
        self.take().map(Ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_semicolons_outside_quotes_and_drops_comments() {
        let sql = "-- tql export\n-- table: a; b\n\ninsert into t values ('a;b', \"c;d\", `e;f`);\n\ninsert into t values (1);";

        assert_eq!(
            StatementReader::new(false).split(sql),
            [
                "insert into t values ('a;b', \"c;d\", `e;f`)",
                "insert into t values (1)"
            ]
        );
    }

    #[test]
    fn reads_a_doubled_quote_as_part_of_the_string() {
        assert_eq!(
            StatementReader::new(false).split("insert into t values ('it''s; fine');select 1"),
            ["insert into t values ('it''s; fine')", "select 1"]
        );
    }

    #[test]
    fn honours_backslash_escapes_only_where_the_database_does() {
        let sql = "insert into t values ('a\\'; b');select 2";

        assert_eq!(
            StatementReader::new(true).split(sql),
            ["insert into t values ('a\\'; b')", "select 2"]
        );
        assert_eq!(
            StatementReader::new(false).split(sql),
            ["insert into t values ('a\\'", "b');select 2"]
        );
    }

    #[test]
    fn keeps_a_double_dash_inside_a_string() {
        assert_eq!(
            StatementReader::new(false).split("insert into t values ('a -- b');"),
            ["insert into t values ('a -- b')"]
        );
    }

    #[test]
    fn says_which_file_it_could_not_read() {
        let missing = Path::new("/nonexistent/tql.sql");

        assert_eq!(
            StatementReader::new(false)
                .read(missing)
                .err()
                .unwrap()
                .to_string(),
            "Could not read [/nonexistent/tql.sql]."
        );
    }

    #[test]
    fn reads_statements_that_cross_a_chunk_boundary() {
        let long = format!(
            "insert into t values ('{}');select 1",
            "x".repeat(CHUNK + 10)
        );
        let path = std::env::temp_dir().join(format!("tql-statements-{}.sql", std::process::id()));
        std::fs::write(&path, &long).unwrap();

        let read: Vec<String> = StatementReader::new(false)
            .read(&path)
            .unwrap()
            .collect::<anyhow::Result<_>>()
            .unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(read.len(), 2);
        assert_eq!(read[0].len(), CHUNK + 10 + 25);
    }
}
