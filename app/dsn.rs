use std::sync::LazyLock;

use regex::Regex;

static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*://").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dsn {
    pub name: String,
    pub driver: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn looks_like_one(value: &str) -> bool {
    SCHEME.is_match(value)
}

pub fn parse(dsn: &str) -> Option<Dsn> {
    if !looks_like_one(dsn) {
        return None;
    }

    let (scheme, rest) = dsn.split_once("://")?;
    let driver = driver_for(&scheme.to_lowercase())?;

    if driver == "sqlite" {
        return Some(sqlite(rest));
    }

    let (before_fragment, _) = rest.split_once('#').unwrap_or((rest, ""));
    let (location, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    let (authority, path) = match location.find('/') {
        Some(slash) => location.split_at(slash),
        None => (location, ""),
    };
    let (credentials, address) = match authority.rsplit_once('@') {
        Some((credentials, address)) => (Some(credentials), address),
        None => (None, authority),
    };
    let (user, pass) = match credentials {
        Some(credentials) => match credentials.split_once(':') {
            Some((user, pass)) => (Some(user), Some(pass)),
            None => (Some(credentials), None),
        },
        None => (None, None),
    };
    let (host, port) = match address.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() => (host, Some(port.parse::<u16>().ok()?)),
        Some((host, _)) => (host, None),
        None => (address, None),
    };

    let database = path.trim_start_matches('/');
    let host = (!host.is_empty()).then(|| host.to_string());
    let name = match query_value(query, "name").filter(|name| !name.is_empty()) {
        Some(name) => name,
        None => {
            let place = host.clone().unwrap_or_else(|| "localhost".to_string());
            match database {
                "" => place,
                database => format!("{database} on {place}"),
            }
        }
    };

    Some(Dsn {
        name,
        driver: driver.to_string(),
        host: Some(host.unwrap_or_else(|| "127.0.0.1".to_string())),
        port: Some(port.unwrap_or_else(|| default_port(driver))),
        database: (!database.is_empty()).then(|| decode(database, false)),
        username: user.map(|user| decode(user, false)),
        password: pass.map(|pass| decode(pass, false)),
    })
}

fn driver_for(scheme: &str) -> Option<&'static str> {
    match scheme {
        "mysql" | "mariadb" => Some("mysql"),
        "pgsql" | "postgres" | "postgresql" => Some("pgsql"),
        "sqlsrv" | "mssql" => Some("sqlsrv"),
        "sqlite" => Some("sqlite"),
        _ => None,
    }
}

pub fn default_port(driver: &str) -> u16 {
    match driver {
        "pgsql" => 5432,
        "sqlsrv" => 1433,
        _ => 3306,
    }
}

fn sqlite(rest: &str) -> Dsn {
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let path = decode(path, false);
    let base = path.rsplit('/').next().unwrap_or_default().to_string();
    Dsn {
        name: query_value(query, "name").unwrap_or(base),
        driver: "sqlite".to_string(),
        database: Some(path),
        ..Dsn::default()
    }
}

fn query_value(query: &str, wanted: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
        .find(|(key, _)| decode(key, true) == wanted)
        .map(|(_, value)| decode(value, true))
}

pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn decode(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        let byte = bytes[position];
        let escaped = (byte == b'%')
            .then(|| text.get(position + 1..position + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (escaped, byte) {
            (Some(value), _) => {
                decoded.push(value);
                position += 3;
                continue;
            }
            (None, b'+') if plus_is_space => decoded.push(b' '),
            (None, other) => decoded.push(other),
        }
        position += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_mysql_connection_string() {
        assert_eq!(
            parse("mysql://alice:s3cret@db.example.com:3307/shop"),
            Some(Dsn {
                name: "shop on db.example.com".into(),
                driver: "mysql".into(),
                host: Some("db.example.com".into()),
                port: Some(3307),
                database: Some("shop".into()),
                username: Some("alice".into()),
                password: Some("s3cret".into()),
            })
        );
    }

    #[test]
    fn takes_the_name_from_the_query_string() {
        let parsed = parse("mysql://alice:pw@host/shop?name=Cloud%20-%20lunar").unwrap();

        assert_eq!(parsed.name, "Cloud - lunar");
    }

    #[test]
    fn fills_in_the_default_port_for_the_driver() {
        for (scheme, driver, port) in [
            ("mysql", "mysql", 3306),
            ("mariadb", "mysql", 3306),
            ("postgres", "pgsql", 5432),
            ("postgresql", "pgsql", 5432),
            ("pgsql", "pgsql", 5432),
            ("sqlsrv", "sqlsrv", 1433),
            ("mssql", "sqlsrv", 1433),
        ] {
            let parsed = parse(&format!("{scheme}://user:pw@host/db")).unwrap();

            assert_eq!(parsed.driver, driver);
            assert_eq!(parsed.port, Some(port));
        }
    }

    #[test]
    fn copes_with_no_database_in_the_path() {
        let parsed = parse("mysql://user:pw@db.example.com?name=Cloud").unwrap();

        assert_eq!(parsed.database, None);
        assert_eq!(parsed.host.as_deref(), Some("db.example.com"));
        assert_eq!(parsed.name, "Cloud");
    }

    #[test]
    fn names_it_after_the_host_when_nothing_else_says() {
        assert_eq!(
            parse("mysql://user:pw@db.example.com").unwrap().name,
            "db.example.com"
        );
    }

    #[test]
    fn decodes_credentials_that_were_percent_encoded() {
        let parsed = parse("pgsql://a%40b:p%2Fw%3A1@host/db").unwrap();

        assert_eq!(parsed.username.as_deref(), Some("a@b"));
        assert_eq!(parsed.password.as_deref(), Some("p/w:1"));
    }

    #[test]
    fn handles_a_sqlite_url() {
        assert_eq!(
            parse("sqlite:///tmp/app.sqlite"),
            Some(Dsn {
                name: "app.sqlite".into(),
                driver: "sqlite".into(),
                database: Some("/tmp/app.sqlite".into()),
                ..Dsn::default()
            })
        );
    }

    #[test]
    fn says_no_to_a_scheme_it_does_not_know() {
        for dsn in [
            "redis://localhost",
            "https://example.com/db",
            "mongodb://host/db",
        ] {
            assert_eq!(parse(dsn), None, "{dsn}");
        }
    }

    #[test]
    fn knows_a_connection_string_from_a_file_path() {
        assert!(looks_like_one("mysql://host/db"));
        assert!(!looks_like_one("./test.sqlite"));
        assert!(!looks_like_one("/var/db/app.db"));
    }
}
