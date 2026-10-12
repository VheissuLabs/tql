use std::path::Path;

use anyhow::{Context, bail};
use rusqlite::OpenFlags;
use rusqlite::types::ValueRef;

use crate::driver::Driver;
use crate::{dsn, paths, tag};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Connection {
    pub id: i64,
    pub name: String,
    pub driver: Driver,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub read_only: bool,
    pub tag: Option<String>,
    pub group_name: Option<String>,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_user: Option<String>,
    pub ssh_key: Option<String>,
    pub ssh_password: Option<String>,
    pub ssl_mode: Option<String>,
    pub ssl_ca: Option<String>,
    pub ssl_cert: Option<String>,
    pub ssl_key: Option<String>,
    pub last_used_at: Option<String>,
}

impl Connection {
    pub fn sqlite(name: &str, path: &str) -> Self {
        Connection {
            name: name.to_string(),
            driver: Driver::Sqlite,
            database: Some(path.to_string()),
            ..Connection::default()
        }
    }

    pub fn tag_color(&self) -> Option<&'static str> {
        tag::color_of(self.tag.as_deref())
    }

    pub fn is_production(&self) -> bool {
        tag::parse(self.tag.as_deref().unwrap_or_default()) == Some("production")
    }

    pub fn uses_ssh(&self) -> bool {
        self.driver.is_server()
            && self
                .ssh_host
                .as_deref()
                .is_some_and(|host| !host.trim().is_empty())
    }

    pub fn connection_string(&self, password: Option<&str>) -> String {
        let database = self.database.clone().unwrap_or_default();
        if self.driver.is_file() {
            return format!("{}://{database}", self.driver);
        }
        let credentials = match (self.username.as_deref(), password) {
            (None | Some(""), _) => String::new(),
            (Some(username), None | Some("")) => format!("{}@", dsn::encode(username)),
            (Some(username), Some(password)) => {
                format!("{}:{}@", dsn::encode(username), dsn::encode(password))
            }
        };
        format!(
            "{}://{credentials}{}:{}/{}",
            self.driver,
            self.host.clone().unwrap_or_default(),
            self.port.map(|port| port.to_string()).unwrap_or_default(),
            dsn::encode(&database)
        )
    }

    pub fn describe(&self) -> String {
        self.describe_from(&paths::home())
    }

    pub fn describe_from(&self, home: &Path) -> String {
        if self.driver.is_file() {
            return format!(
                "{}:{}",
                self.driver,
                paths::shorten_from(self.database.as_deref().unwrap_or_default(), home)
            );
        }

        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let place = format!(
            "{}://{}@{}:{}/{}",
            self.driver,
            text(&self.username),
            text(&self.host),
            self.port.map(|port| port.to_string()).unwrap_or_default(),
            text(&self.database)
        );
        if !self.uses_ssh() {
            return place;
        }
        let user = self
            .ssh_user
            .as_deref()
            .filter(|user| !user.is_empty())
            .map(|user| format!("{user}@"))
            .unwrap_or_default();
        format!("{place}  ssh {user}{}", text(&self.ssh_host))
    }
}

pub struct Store {
    connection: rusqlite::Connection,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            bail!(
                "no tql store at {}; add a connection with tql first",
                path.display()
            );
        }

        let connection =
            rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .with_context(|| format!("cannot open {}", path.display()))?;
        Ok(Store { connection })
    }

    pub fn connections(&self) -> anyhow::Result<Vec<Connection>> {
        self.read(
            "select * from connections order by last_used_at desc, name",
            [],
        )
    }

    pub fn connection(&self, id: i64) -> anyhow::Result<Option<Connection>> {
        Ok(self
            .read("select * from connections where id = ?1", [id])?
            .into_iter()
            .next())
    }

    fn read(
        &self,
        sql: &str,
        parameters: impl rusqlite::Params,
    ) -> anyhow::Result<Vec<Connection>> {
        let mut statement = self.connection.prepare(sql)?;
        let columns: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();

        let connections = statement.query_map(parameters, |row| {
            let text = |name: &str| -> Option<String> {
                let index = columns.iter().position(|column| column == name)?;
                match row.get_ref(index).ok()? {
                    ValueRef::Null => None,
                    ValueRef::Integer(number) => Some(number.to_string()),
                    ValueRef::Real(number) => Some(number.to_string()),
                    ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                        Some(String::from_utf8_lossy(bytes).into_owned())
                    }
                }
            };

            let Some(driver) = text("driver").as_deref().and_then(Driver::parse) else {
                return Ok(None);
            };
            Ok(Some(Connection {
                id: text("id")
                    .and_then(|id| id.parse().ok())
                    .unwrap_or_default(),
                name: text("name").unwrap_or_default(),
                driver,
                host: text("host"),
                port: text("port").and_then(|port| port.parse().ok()),
                database: text("database"),
                username: text("username"),
                password: text("password").filter(|password| !password.is_empty()),
                read_only: text("read_only").is_some_and(|flag| flag == "1"),
                tag: text("tag").filter(|tag| !tag.trim().is_empty()),
                group_name: text("group_name")
                    .map(|group| group.trim().to_string())
                    .filter(|group| !group.is_empty()),
                ssh_host: text("ssh_host"),
                ssh_port: text("ssh_port").and_then(|port| port.parse().ok()),
                ssh_user: text("ssh_user"),
                ssh_key: text("ssh_key"),
                ssh_password: text("ssh_password").filter(|password| !password.is_empty()),
                ssl_mode: text("ssl_mode"),
                ssl_ca: text("ssl_ca"),
                ssl_cert: text("ssl_cert"),
                ssl_key: text("ssl_key"),
                last_used_at: text("last_used_at"),
            }))
        })?;

        Ok(connections
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_mariadb_connection_as_mysql_and_skips_a_driver_it_does_not_know() {
        let path =
            std::env::temp_dir().join(format!("tql-store-drivers-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "create table connections (id integer primary key, name text, driver text, last_used_at text);
                 insert into connections values (1, 'maria', 'mariadb', null);
                 insert into connections values (2, 'oracle', 'oci', null);",
            )
            .unwrap();

        let connections = Store::open(&path).unwrap().connections().unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "maria");
        assert_eq!(connections[0].driver, Driver::Mysql);
    }

    #[test]
    fn reads_connections_most_recently_used_first() {
        let path = std::env::temp_dir().join(format!("tql-store-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer
            .execute_batch(
                "create table connections (id integer primary key, name text, driver text, host text, port integer,
                    database text, username text, password text, read_only integer, tag text, last_used_at text);
                 insert into connections values (1, 'old', 'mysql', 'db.test', 3306, 'shop', 'root', null, 0, 'production', '2026-01-01');
                 insert into connections values (2, 'new', 'sqlite', null, null, '/tmp/a.sqlite', null, null, 1, '', '2026-06-01');
                 insert into connections values (3, 'never', 'pgsql', 'pg.test', 5432, 'app', 'me', 'secret', 0, null, null);",
            )
            .unwrap();

        let connections = Store::open(&path).unwrap().connections().unwrap();
        std::fs::remove_file(&path).unwrap();

        let names: Vec<_> = connections
            .iter()
            .map(|connection| connection.name.as_str())
            .collect();
        assert_eq!(names, ["new", "old", "never"]);
        assert_eq!(connections[1].port, Some(3306));
        assert_eq!(connections[1].tag_color(), Some("red"));
        assert_eq!(connections[1].describe(), "mysql://root@db.test:3306/shop");
        assert!(connections[0].read_only);
        assert_eq!(connections[0].tag, None);
    }

    fn tunnelled() -> Connection {
        Connection {
            name: "prod".to_string(),
            driver: Driver::Mysql,
            host: Some("db.internal".to_string()),
            port: Some(3306),
            database: Some("shop".to_string()),
            username: Some("alice".to_string()),
            ssh_host: Some("bastion.example.com".to_string()),
            ssh_user: Some("karl".to_string()),
            ..Connection::default()
        }
    }

    #[test]
    fn knows_when_a_connection_goes_through_ssh() {
        let without = |ssh_host: Option<&str>| Connection {
            ssh_host: ssh_host.map(String::from),
            ..tunnelled()
        };

        assert!(tunnelled().uses_ssh());
        assert!(!without(None).uses_ssh());
        assert!(!without(Some("  ")).uses_ssh());
    }

    #[test]
    fn never_tunnels_a_sqlite_file() {
        let file = Connection {
            ssh_host: Some("bastion.example.com".to_string()),
            ..Connection::sqlite("local", "/tmp/app.sqlite")
        };

        assert!(!file.uses_ssh());
    }

    #[test]
    fn says_in_the_connection_list_that_it_goes_through_ssh() {
        let anonymous = Connection {
            ssh_user: None,
            ..tunnelled()
        };

        assert!(
            tunnelled()
                .describe()
                .ends_with("  ssh karl@bastion.example.com")
        );
        assert!(anonymous.describe().ends_with("  ssh bastion.example.com"));
    }
}
