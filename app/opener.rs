use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{Context, anyhow, bail};

use crate::driver::Driver;
use crate::dsn::{self, Dsn};
use crate::encrypter::Encrypter;
use crate::store::{Connection, Store};
use crate::store_writer::{Field, StoreWriter};
use crate::tag;

pub struct Opened {
    pub connection: Connection,
    pub encrypter: Option<Encrypter>,
}

pub fn refuse_tag(tag: Option<&str>) -> Option<String> {
    let tag = tag?;
    if tag::parse(tag).is_some() {
        return None;
    }
    Some(format!(
        "[{tag}] is not a tag. Try {}.",
        tag::names().join(", ")
    ))
}

pub fn target(path: &str) -> anyhow::Result<Dsn> {
    if dsn::looks_like_one(path) {
        return dsn::parse(path)
            .ok_or_else(|| anyhow!("That connection string is not one I understand."));
    }

    let file = Path::new(path);
    if !file.is_file() {
        bail!("No such file: {path}");
    }
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let driver = if looks_like_sqlite(file) {
        Driver::Sqlite
    } else if looks_like_duckdb(file) {
        Driver::Duckdb
    } else {
        bail!("{name} is not a SQLite or DuckDB database.");
    };
    Ok(Dsn {
        name,
        driver,
        database: Some(
            fs::canonicalize(file)
                .unwrap_or_else(|_| file.to_path_buf())
                .display()
                .to_string(),
        ),
        ..Dsn::default()
    })
}

pub fn saved(typed: &str, store: &Path, key: &Path) -> anyhow::Result<Connection> {
    let target = target(&crate::paths::expand(typed.trim()))?;
    Ok(connection_for(store, key, target, None, None, false)?.connection)
}

pub fn looks_like_duckdb(path: &Path) -> bool {
    let mut header = [0; 12];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|_| &header[8..12] == b"DUCK")
}

pub fn looks_like_sqlite(path: &Path) -> bool {
    let mut header = [0; 16];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|_| &header == b"SQLite format 3\0")
}

pub fn connection_for(
    store: &Path,
    key: &Path,
    target: Dsn,
    called: Option<&str>,
    tag: Option<&str>,
    peek: bool,
) -> anyhow::Result<Opened> {
    let given = tag.map(tag::value);
    let writer = StoreWriter::open(store, key)?;

    if let Some(id) = writer.matching(&identity(&target))? {
        let existing = Store::open(store)?
            .connection(id)?
            .context("the matching connection has gone")?;
        if let Some(called) = called.filter(|called| !called.is_empty() && *called != existing.name)
        {
            writer.update(id, &[("name", Field::Text(writer.free_name(called)?))])?;
        }
        if let Some(tag) = given {
            writer.update(id, &[("tag", Field::Text(tag))])?;
        }
        return Ok(Opened {
            connection: Store::open(store)?
                .connection(id)?
                .context("the matching connection has gone")?,
            encrypter: Encrypter::from_file(key).ok(),
        });
    }

    let tag = given.or(target.tag.clone());
    let name = called
        .filter(|called| !called.is_empty())
        .map(String::from)
        .unwrap_or(target.name.clone());
    if peek {
        let encrypter = Encrypter::throwaway()?;
        let secret = |plain: &Option<String>| {
            plain
                .as_deref()
                .map(|plain| encrypter.encrypt_string(plain))
                .transpose()
        };
        let password = secret(&target.password)?;
        let ssh_password = secret(&target.ssh_password)?;
        return Ok(Opened {
            connection: Connection {
                name,
                driver: target.driver,
                host: target.host,
                port: target.port,
                database: target.database,
                username: target.username,
                password,
                ssl_mode: target.ssl_mode.clone(),
                tag: tag.filter(|tag| !tag.is_empty()),
                ssh_host: target.ssh_host,
                ssh_port: target.ssh_port,
                ssh_user: target.ssh_user,
                ssh_password,
                warehouse: target.warehouse,
                role: target.role,
                ..Connection::default()
            },
            encrypter: Some(encrypter),
        });
    }

    let text = |value: Option<String>| value.map_or(Field::Null, Field::Text);
    let mut fields = vec![
        ("name", Field::Text(writer.free_name(&name)?)),
        ("driver", Field::Text(target.driver.name().to_string())),
        ("database", text(target.database.clone())),
    ];
    if !target.driver.is_file() {
        fields.extend([
            ("host", text(target.host.clone())),
            (
                "port",
                target
                    .port
                    .map_or(Field::Null, |port| Field::Number(port.into())),
            ),
            ("username", text(target.username.clone())),
            (
                "password",
                target.password.clone().map_or(Field::Null, Field::Secret),
            ),
        ]);
        for (column, value) in [("warehouse", &target.warehouse), ("role", &target.role)] {
            if let Some(value) = value {
                fields.push((column, Field::Text(value.clone())));
            }
        }
        if let Some(ssl_mode) = &target.ssl_mode {
            fields.push(("ssl_mode", Field::Text(ssl_mode.clone())));
        }
        if target.ssh_host.is_some() {
            fields.extend([
                ("ssh_host", text(target.ssh_host.clone())),
                (
                    "ssh_port",
                    target
                        .ssh_port
                        .map_or(Field::Null, |port| Field::Number(port.into())),
                ),
                ("ssh_user", text(target.ssh_user.clone())),
                (
                    "ssh_password",
                    target
                        .ssh_password
                        .clone()
                        .map_or(Field::Null, Field::Secret),
                ),
            ]);
        }
    }
    if let Some(tag) = tag {
        fields.push(("tag", Field::Text(tag)));
    }
    let id = writer.create(&fields)?;
    Ok(Opened {
        connection: Store::open(store)?
            .connection(id)?
            .context("the new connection did not save")?,
        encrypter: Encrypter::from_file(key).ok(),
    })
}

fn identity(target: &Dsn) -> Vec<(&'static str, Field)> {
    let text = |value: &Option<String>| value.clone().map_or(Field::Null, Field::Text);
    vec![
        ("driver", Field::Text(target.driver.name().to_string())),
        ("host", text(&target.host)),
        (
            "port",
            target
                .port
                .map_or(Field::Null, |port| Field::Number(port.into())),
        ),
        ("database", text(&target.database)),
        ("username", text(&target.username)),
        ("ssh_host", text(&target.ssh_host)),
    ]
}
