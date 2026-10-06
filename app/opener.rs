use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::Context;

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

pub fn target(path: &str) -> Result<Dsn, String> {
    if dsn::looks_like_one(path) {
        let parsed = dsn::parse(path).ok_or("That connection string is not one I understand.")?;
        if parsed.driver == "sqlsrv" {
            return Err(
                "SQL Server is not supported by tql-rust yet; open it with tql.".to_string(),
            );
        }
        return Ok(parsed);
    }

    let file = Path::new(path);
    if !file.is_file() {
        return Err(format!("No such file: {path}"));
    }
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !looks_like_sqlite(file) {
        return Err(format!("{name} is not a SQLite database."));
    }
    Ok(Dsn {
        name,
        driver: "sqlite".to_string(),
        database: Some(
            fs::canonicalize(file)
                .unwrap_or_else(|_| file.to_path_buf())
                .display()
                .to_string(),
        ),
        ..Dsn::default()
    })
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
    let tag = tag.map(|tag| tag::parse(tag).unwrap_or(tag).to_string());
    let writer = StoreWriter::open(store, key)?;

    if let Some(id) = writer.matching(&identity(&target))? {
        let existing = Store::open(store)?
            .connection(id)?
            .context("the matching connection has gone")?;
        if let Some(called) = called.filter(|called| *called != existing.name) {
            writer.update(id, &[("name", Field::Text(writer.free_name(called)?))])?;
        }
        if let Some(tag) = tag {
            writer.update(id, &[("tag", Field::Text(tag))])?;
        }
        return Ok(Opened {
            connection: Store::open(store)?
                .connection(id)?
                .context("the matching connection has gone")?,
            encrypter: Encrypter::from_file(key).ok(),
        });
    }

    let name = called.map(String::from).unwrap_or(target.name.clone());
    if peek {
        let encrypter = Encrypter::throwaway()?;
        let password = target
            .password
            .as_deref()
            .map(|password| encrypter.encrypt_string(password))
            .transpose()?;
        return Ok(Opened {
            connection: Connection {
                name,
                driver: target.driver,
                host: target.host,
                port: target.port,
                database: target.database,
                username: target.username,
                password,
                tag: tag.filter(|tag| !tag.is_empty()),
                ..Connection::default()
            },
            encrypter: Some(encrypter),
        });
    }

    let text = |value: Option<String>| value.map_or(Field::Null, Field::Text);
    let mut fields = vec![
        ("name", Field::Text(writer.free_name(&name)?)),
        ("driver", Field::Text(target.driver.clone())),
        ("database", text(target.database.clone())),
    ];
    if target.driver != "sqlite" {
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
        ("driver", Field::Text(target.driver.clone())),
        ("host", text(&target.host)),
        (
            "port",
            target
                .port
                .map_or(Field::Null, |port| Field::Number(port.into())),
        ),
        ("database", text(&target.database)),
        ("username", text(&target.username)),
    ]
}
