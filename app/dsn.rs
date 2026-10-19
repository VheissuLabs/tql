use std::sync::LazyLock;

use regex::Regex;

use crate::driver::Driver;

static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*://").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dsn {
    pub name: String,
    pub driver: Driver,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tag: Option<String>,
    pub ssl_mode: Option<String>,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_user: Option<String>,
    pub ssh_password: Option<String>,
    pub warehouse: Option<String>,
    pub role: Option<String>,
}

pub fn looks_like_one(value: &str) -> bool {
    SCHEME.is_match(value)
}

pub fn parse(dsn: &str) -> Option<Dsn> {
    if !looks_like_one(dsn) {
        return None;
    }

    let (scheme, rest) = dsn.split_once("://")?;
    if scheme.to_lowercase().ends_with("+ssh") {
        return over_ssh(&scheme[..scheme.len() - 4], rest);
    }
    let lowered = scheme.to_lowercase();
    if lowered == "libsql"
        || matches!(lowered.as_str(), "http" | "https") && rest.contains("authToken=")
    {
        return Some(libsql(&lowered, rest));
    }
    let tls = matches!(lowered.as_str(), "clickhouses" | "rediss" | "valkeys");
    let driver = match lowered.as_str() {
        "clickhouses" => Driver::Clickhouse,
        "rediss" | "valkeys" => Driver::Redis,
        other => driver_for(other)?,
    };

    if driver.is_file() {
        return Some(file(rest, driver));
    }
    if driver == Driver::Snowflake {
        return snowflake(rest);
    }

    let (before_fragment, _) = rest.split_once('#').unwrap_or((rest, ""));
    let (location, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    let (authority, path) = match location.find('/') {
        Some(slash) => location.split_at(slash),
        None => (location, ""),
    };
    let (user, pass, host, port) = authority_parts(authority)?;

    let database = path.trim_start_matches('/');
    let host = (!host.is_empty()).then(|| host.to_string());
    let name = match query_value(query, "name").filter(|name| !name.is_empty()) {
        Some(name) => name,
        None if driver == Driver::Redis => {
            let place = host.clone().unwrap_or_else(|| "localhost".to_string());
            match database {
                "" | "0" => place,
                number => format!("{place} db{number}"),
            }
        }
        None => name_for(host.as_deref(), database),
    };

    Some(Dsn {
        name,
        driver,
        host: Some(host.unwrap_or_else(|| "127.0.0.1".to_string())),
        port: port.or(match (tls, driver) {
            (true, Driver::Clickhouse) => Some(8443),
            _ => driver.default_port(),
        }),
        ssl_mode: tls.then(|| "require".to_string()),
        database: (!database.is_empty()).then(|| decode(database, false)),
        username: user.map(|user| decode(user, false)),
        password: pass.map(|pass| decode(pass, false)),
        tag: query_value(query, "env").and_then(|env| tag_for(&env)),
        ..Dsn::default()
    })
}

fn over_ssh(scheme: &str, rest: &str) -> Option<Dsn> {
    let (tunnel, database) = rest.split_once('/')?;
    if database.is_empty() {
        return None;
    }
    let (user, pass, host, port) = authority_parts(tunnel)?;
    if host.is_empty() {
        return None;
    }

    let mut target = parse(&format!("{scheme}://{database}"))?;
    if target.driver.is_file() {
        return None;
    }
    let (_, query) = database.split_once('?').unwrap_or((database, ""));
    if query_value(query, "name").is_none_or(|name| name.is_empty()) {
        target.name = name_for(Some(host), target.database.as_deref().unwrap_or_default());
    }
    target.ssh_host = Some(host.to_string());
    target.ssh_port = port;
    target.ssh_user = user.map(|user| decode(user, false));
    target.ssh_password = pass.map(|pass| decode(pass, false));
    Some(target)
}

type Authority<'a> = (Option<&'a str>, Option<&'a str>, &'a str, Option<u16>);

fn authority_parts(authority: &str) -> Option<Authority<'_>> {
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
    let (host, port) = match address
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
    {
        Some((host, "")) => (host, None),
        Some((host, after)) => (host, Some(after.strip_prefix(':')?.parse::<u16>().ok()?)),
        None => match address.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() => (host, Some(port.parse::<u16>().ok()?)),
            Some((host, _)) => (host, None),
            None => (address, None),
        },
    };
    Some((user, pass, host, port))
}

fn name_for(host: Option<&str>, database: &str) -> String {
    let place = host.unwrap_or("localhost");
    match database {
        "" => place.to_string(),
        database => format!("{database} on {place}"),
    }
}

fn tag_for(environment: &str) -> Option<String> {
    let tag = match environment.trim().to_lowercase().as_str() {
        "production" | "prod" => "production",
        "staging" | "stage" => "staging",
        "development" | "dev" | "testing" | "test" => "dev",
        "local" => "local",
        _ => return None,
    };
    Some(tag.to_string())
}

fn driver_for(scheme: &str) -> Option<Driver> {
    match scheme {
        "postgres" | "postgresql" => Some(Driver::Pgsql),
        "mssql" => Some(Driver::Sqlsrv),
        other => Driver::parse(other),
    }
}

fn libsql(scheme: &str, rest: &str) -> Dsn {
    let (location, query) = rest.split_once('?').unwrap_or((rest, ""));
    let location = location.trim_end_matches('/');
    let place = location.rsplit('@').next().unwrap_or(location).to_string();
    Dsn {
        name: query_value(query, "name")
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| place.clone()),
        driver: Driver::Libsql,
        host: Some(format!("{scheme}://{place}")),
        password: query_value(query, "authToken"),
        tag: query_value(query, "env").and_then(|env| tag_for(&env)),
        ..Dsn::default()
    }
}

fn snowflake(rest: &str) -> Option<Dsn> {
    let (location, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = location.split_once('/').unwrap_or((location, ""));
    let (user, token, account, _) = authority_parts(authority)?;
    if account.is_empty() {
        return None;
    }
    let database = path.split('/').next().unwrap_or_default();
    let present = |value: Option<String>| value.filter(|value| !value.is_empty());
    Some(Dsn {
        name: present(query_value(query, "name"))
            .unwrap_or_else(|| name_for(Some(account), &decode(database, false))),
        driver: Driver::Snowflake,
        host: Some(account.to_string()),
        database: (!database.is_empty()).then(|| decode(database, false)),
        username: user.map(|user| decode(user, false)),
        password: token.map(|token| decode(token, false)),
        tag: query_value(query, "env").and_then(|env| tag_for(&env)),
        warehouse: present(query_value(query, "warehouse")),
        role: present(query_value(query, "role")),
        ..Dsn::default()
    })
}

fn file(rest: &str, driver: Driver) -> Dsn {
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let path = decode(path, false);
    let base = path.rsplit('/').next().unwrap_or_default().to_string();
    Dsn {
        name: query_value(query, "name").unwrap_or(base),
        driver,
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

pub fn decode(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        let byte = bytes[position];
        let escaped = (byte == b'%')
            .then(|| text.get(position + 1..position + 3))
            .flatten()
            .filter(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
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
    fn reads_a_turso_url_with_its_token() {
        let parsed = parse("libsql://shop-karl.turso.io?authToken=secret").unwrap();
        assert_eq!(parsed.driver, Driver::Libsql);
        assert_eq!(parsed.host.as_deref(), Some("libsql://shop-karl.turso.io"));
        assert_eq!(parsed.password.as_deref(), Some("secret"));
        assert_eq!(parsed.name, "shop-karl.turso.io");

        let secure = parse("https://shop.turso.io?authToken=t&name=Shop").unwrap();
        assert_eq!(secure.driver, Driver::Libsql);
        assert_eq!(secure.host.as_deref(), Some("https://shop.turso.io"));
        assert_eq!(secure.name, "Shop");

        assert_eq!(parse("https://example.com/page"), None);
    }

    #[test]
    fn reads_redis_and_valkey_urls_with_their_database_number() {
        let plain = parse("redis://:secret@cache.local:6380/2").unwrap();
        assert_eq!(plain.driver, Driver::Redis);
        assert_eq!(plain.port, Some(6380));
        assert_eq!(plain.database.as_deref(), Some("2"));
        assert_eq!(plain.password.as_deref(), Some("secret"));
        assert_eq!(plain.name, "cache.local db2");
        assert_eq!(parse("redis://localhost/0").unwrap().name, "localhost");

        let secure = parse("rediss://default:pw@redis.cloud").unwrap();
        assert_eq!(secure.driver, Driver::Redis);
        assert_eq!(secure.port, Some(6379));
        assert_eq!(secure.ssl_mode.as_deref(), Some("require"));

        assert_eq!(parse("valkey://localhost").unwrap().driver, Driver::Redis);
        assert_eq!(
            parse("valkeys://localhost").unwrap().ssl_mode.as_deref(),
            Some("require")
        );
    }

    #[test]
    fn reads_a_clickhouse_url_and_its_tls_form() {
        let plain = parse("clickhouse://default:pw@ch.local:8123/events").unwrap();
        assert_eq!(plain.driver, Driver::Clickhouse);
        assert_eq!(plain.port, Some(8123));
        assert_eq!(plain.database.as_deref(), Some("events"));
        assert_eq!(plain.ssl_mode, None);

        let secure = parse("clickhouses://default:pw@ch.cloud/events").unwrap();
        assert_eq!(secure.driver, Driver::Clickhouse);
        assert_eq!(secure.port, Some(8443));
        assert_eq!(secure.ssl_mode.as_deref(), Some("require"));
    }

    #[test]
    fn parses_a_mysql_connection_string() {
        assert_eq!(
            parse("mysql://alice:s3cret@db.example.com:3307/shop"),
            Some(Dsn {
                name: "shop on db.example.com".into(),
                driver: Driver::Mysql,
                host: Some("db.example.com".into()),
                port: Some(3307),
                database: Some("shop".into()),
                username: Some("alice".into()),
                password: Some("s3cret".into()),
                ..Dsn::default()
            })
        );
    }

    #[test]
    fn reads_a_snowflake_url_with_its_token_warehouse_and_role() {
        let parsed = parse(
            "snowflake://karl:pat%2Btoken@myorg-myaccount/ANALYTICS/PUBLIC?warehouse=COMPUTE_WH&role=ANALYST&env=prod",
        )
        .unwrap();
        assert_eq!(parsed.driver, Driver::Snowflake);
        assert_eq!(parsed.host.as_deref(), Some("myorg-myaccount"));
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.database.as_deref(), Some("ANALYTICS"));
        assert_eq!(parsed.username.as_deref(), Some("karl"));
        assert_eq!(parsed.password.as_deref(), Some("pat+token"));
        assert_eq!(parsed.warehouse.as_deref(), Some("COMPUTE_WH"));
        assert_eq!(parsed.role.as_deref(), Some("ANALYST"));
        assert_eq!(parsed.tag.as_deref(), Some("production"));
        assert_eq!(parsed.name, "ANALYTICS on myorg-myaccount");
    }

    #[test]
    fn reads_a_tableplus_url_that_goes_through_ssh() {
        assert_eq!(
            parse(
                "mysql+ssh://deploy@bastion.example.com/dbuser:pw@127.0.0.1?name=Prod&env=production"
            ),
            Some(Dsn {
                name: "Prod".into(),
                driver: Driver::Mysql,
                host: Some("127.0.0.1".into()),
                port: Some(3306),
                database: None,
                username: Some("dbuser".into()),
                password: Some("pw".into()),
                tag: Some("production".into()),
                ssl_mode: None,
                ssh_host: Some("bastion.example.com".into()),
                ssh_port: None,
                ssh_user: Some("deploy".into()),
                ssh_password: None,
                warehouse: None,
                role: None,
            })
        );
    }

    #[test]
    fn names_a_connection_through_ssh_after_the_ssh_host() {
        let parsed = parse("pgsql+ssh://me:ssh%20pw@jump:2222/app:pw@10.0.0.5:5433/shop").unwrap();

        assert_eq!(parsed.name, "shop on jump");
        assert_eq!(parsed.driver, Driver::Pgsql);
        assert_eq!(
            (parsed.host.as_deref(), parsed.port),
            (Some("10.0.0.5"), Some(5433))
        );
        assert_eq!(parsed.ssh_port, Some(2222));
        assert_eq!(parsed.ssh_user.as_deref(), Some("me"));
        assert_eq!(parsed.ssh_password.as_deref(), Some("ssh pw"));
    }

    #[test]
    fn will_not_tunnel_to_sqlite_or_to_nowhere() {
        assert_eq!(parse("sqlite+ssh://me@jump//tmp/a.sqlite"), None);
        assert_eq!(parse("mysql+ssh://me@jump"), None);
        assert_eq!(parse("mysql+ssh://me@jump/"), None);
    }

    #[test]
    fn turns_the_environment_into_a_tag() {
        let tag = |env: &str| parse(&format!("mysql://u@h/db?env={env}")).unwrap().tag;

        assert_eq!(tag("prod").as_deref(), Some("production"));
        assert_eq!(tag("Stage").as_deref(), Some("staging"));
        assert_eq!(tag("testing").as_deref(), Some("dev"));
        assert_eq!(tag("development").as_deref(), Some("dev"));
        assert_eq!(tag("local").as_deref(), Some("local"));
        assert_eq!(tag("qa"), None);
        assert_eq!(parse("mysql://u@h/db").unwrap().tag, None);
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

            assert_eq!(parsed.driver.name(), driver);
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
                driver: Driver::Sqlite,
                database: Some("/tmp/app.sqlite".into()),
                ..Dsn::default()
            })
        );
    }

    #[test]
    fn says_no_to_a_scheme_it_does_not_know() {
        for dsn in [
            "mongodb://localhost",
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

    #[test]
    fn decodes_only_a_percent_followed_by_two_hex_digits() {
        let parsed = parse("mysql://a%+4b@h/db").unwrap();
        assert_eq!(parsed.username.as_deref(), Some("a%+4b"));
    }

    #[test]
    fn reads_a_bracketed_ipv6_host_with_or_without_a_port() {
        let parsed = parse("mysql://u@[::1]/db").unwrap();
        assert_eq!(parsed.host.as_deref(), Some("::1"));
        assert_eq!(parsed.port, Some(3306));
        let parsed = parse("mysql://u@[::1]:3307/db").unwrap();
        assert_eq!(parsed.host.as_deref(), Some("::1"));
        assert_eq!(parsed.port, Some(3307));
    }
}
