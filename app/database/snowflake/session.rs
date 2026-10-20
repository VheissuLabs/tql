use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use serde_json::{Value, json};

use super::super::http;
use super::browser::{self, Account};
use crate::encrypter::Encrypter;
use crate::store::Store;
use crate::store_writer::{Field, StoreWriter};

const EARLY: i64 = 60;
const FIRST_WAIT: Duration = Duration::from_millis(250);
const LONGEST_WAIT: Duration = Duration::from_secs(2);
const RUNNING: [&str; 2] = ["333333", "333334"];
const EXPIRED: [&str; 2] = ["390111", "390112"];
const DML: std::ops::Range<u64> = 0x3000..0x4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    pub session: String,
    pub master: String,
    pub session_until: i64,
    pub master_until: i64,
}

impl Tokens {
    fn signed_in(data: &Value, now: i64) -> Option<Tokens> {
        Some(Tokens {
            session: data["token"].as_str()?.to_string(),
            master: data["masterToken"].as_str()?.to_string(),
            session_until: now + data["validityInSeconds"].as_i64().unwrap_or(3600),
            master_until: now + data["masterValidityInSeconds"].as_i64().unwrap_or(14400),
        })
    }

    fn renewed(&self, data: &Value, now: i64) -> Option<Tokens> {
        Some(Tokens {
            session: data["sessionToken"].as_str()?.to_string(),
            session_until: now + data["validityInSecondsST"].as_i64().unwrap_or(3600),
            master: data["masterToken"]
                .as_str()
                .map_or_else(|| self.master.clone(), String::from),
            master_until: data["validityInSecondsMT"]
                .as_i64()
                .map_or(self.master_until, |seconds| now + seconds),
        })
    }

    pub fn saved(&self, owner: &str) -> String {
        json!({
            "for": owner,
            "session": self.session,
            "master": self.master,
            "session_until": self.session_until,
            "master_until": self.master_until,
        })
        .to_string()
    }

    pub fn restored(saved: &str, owner: &str) -> Option<Tokens> {
        let saved: Value = serde_json::from_str(saved).ok()?;
        if saved["for"].as_str() != Some(owner) {
            return None;
        }
        Some(Tokens {
            session: saved["session"].as_str()?.to_string(),
            master: saved["master"].as_str()?.to_string(),
            session_until: saved["session_until"].as_i64()?,
            master_until: saved["master_until"].as_i64()?,
        })
    }

    fn fresh(&self, now: i64) -> bool {
        self.session_until > now + EARLY
    }

    fn renewable(&self, now: i64) -> bool {
        self.master_until > now + EARLY
    }
}

pub struct Keeper {
    pub store: PathBuf,
    pub key: PathBuf,
    pub id: i64,
}

impl Keeper {
    fn load(&self, owner: &str) -> Option<Tokens> {
        let payload = Store::open(&self.store)
            .ok()?
            .connection(self.id)
            .ok()??
            .sign_in?;
        let saved = Encrypter::from_file(&self.key)
            .ok()?
            .decrypt_string(&payload)
            .ok()?;
        Tokens::restored(&saved, owner)
    }

    fn save(&self, tokens: &Tokens, owner: &str) {
        let _ = StoreWriter::open(&self.store, &self.key).and_then(|writer| {
            writer.update(self.id, &[("sign_in", Field::Secret(tokens.saved(owner)))])
        });
    }
}

#[derive(Debug, Default, Clone)]
pub struct Place {
    pub role: Option<String>,
    pub warehouse: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
}

impl Place {
    fn noted(&mut self, data: &Value) {
        for (name, value) in [
            ("finalRoleName", &mut self.role),
            ("finalWarehouseName", &mut self.warehouse),
            ("finalDatabaseName", &mut self.database),
            ("finalSchemaName", &mut self.schema),
        ] {
            if let Some(reported) = data.get(name) {
                *value = reported.as_str().map(String::from);
            }
        }
    }

    fn from_session(info: &Value) -> Place {
        let text = |name: &str| info[name].as_str().map(String::from);
        Place {
            role: text("roleName"),
            warehouse: text("warehouseName"),
            database: text("databaseName"),
            schema: text("schemaName"),
        }
    }
}

pub struct Sent {
    pub data: Value,
    pub chunks: Vec<Value>,
}

impl Sent {
    pub fn written(&self) -> Option<usize> {
        let kind = self.data["statementTypeId"].as_u64()?;
        if !DML.contains(&kind) {
            return None;
        }
        Some(
            self.data["rowset"][0]
                .as_array()?
                .iter()
                .filter_map(|count| count.as_str()?.parse::<usize>().ok())
                .fold(0, usize::saturating_add),
        )
    }
}

struct State {
    tokens: Tokens,
    place: Place,
}

pub struct Session {
    client: http::Client,
    base: String,
    account: String,
    user: String,
    timeout: Duration,
    keeper: Option<Keeper>,
    state: Mutex<State>,
}

impl Session {
    pub fn open(
        base: String,
        account: String,
        user: String,
        wanted: &Place,
        keeper: Option<Keeper>,
        cached: Option<String>,
        timeout: Duration,
    ) -> anyhow::Result<Session> {
        let client = http::Client::new(timeout);
        let owner = owner(&account, &user);
        let now = now();
        let restored = cached
            .and_then(|saved| Tokens::restored(&saved, &owner))
            .filter(|tokens| tokens.fresh(now) || tokens.renewable(now));
        let (tokens, place) = match restored {
            Some(tokens) => (tokens, Place::default()),
            None => {
                let data = sign_in(&client, &base, &account, &user, wanted)?;
                let tokens = Tokens::signed_in(&data, now)
                    .ok_or_else(|| anyhow!("Snowflake signed in without handing over a session"))?;
                if let Some(keeper) = &keeper {
                    keeper.save(&tokens, &owner);
                }
                (tokens, Place::from_session(&data["sessionInfo"]))
            }
        };
        Ok(Session {
            client,
            base,
            account,
            user,
            timeout,
            keeper,
            state: Mutex::new(State { tokens, place }),
        })
    }

    pub fn ask(&self, sql: &str, wanted: &Place) -> anyhow::Result<Sent> {
        self.align(wanted)?;
        self.query(sql)
    }

    fn align(&self, wanted: &Place) -> anyhow::Result<()> {
        let mut database_changed = false;
        for kind in ["role", "warehouse", "database", "schema"] {
            let desired = match kind {
                "role" => &wanted.role,
                "warehouse" => &wanted.warehouse,
                "database" => &wanted.database,
                _ => &wanted.schema,
            };
            let Some(desired) = desired else {
                continue;
            };
            let current = {
                let state = self.lock();
                match kind {
                    "role" => state.place.role.clone(),
                    "warehouse" => state.place.warehouse.clone(),
                    "database" => state.place.database.clone(),
                    _ => state.place.schema.clone(),
                }
            };
            if !database_changed && current.is_some_and(|current| same(&current, desired)) {
                continue;
            }
            self.query(&format!("use {kind} {}", identifier(desired)))?;
            database_changed |= kind == "database";
        }
        Ok(())
    }

    fn query(&self, sql: &str) -> anyhow::Result<Sent> {
        let mut retried = false;
        loop {
            let token = self.token()?;
            let body = json!({
                "sqlText": sql,
                "asyncExec": false,
                "sequenceId": 1,
                "querySubmissionTime": jiff::Timestamp::now().as_millisecond(),
            });
            let reply = self.client.post(
                &format!(
                    "{}/queries/v1/query-request?requestId={}",
                    self.base,
                    request_id()
                ),
                &authorised(&token),
                body.to_string(),
            )?;
            let mut answer = read(&reply)?;
            if code(&answer).is_some_and(|code| EXPIRED.contains(&code)) && !retried {
                self.refresh(&token)?;
                retried = true;
                continue;
            }
            answer = self.finished(answer, &token)?;
            if answer["success"].as_bool() != Some(true) {
                bail!("{}", said(&answer));
            }
            let data = answer["data"].take();
            self.lock().place.noted(&data);
            let chunks = self.chunks(&data)?;
            return Ok(Sent { data, chunks });
        }
    }

    fn finished(&self, mut answer: Value, token: &str) -> anyhow::Result<Value> {
        let started = Instant::now();
        let mut wait = FIRST_WAIT;
        while code(&answer).is_some_and(|code| RUNNING.contains(&code)) {
            let Some(place) = answer["data"]["getResultUrl"].as_str() else {
                bail!(
                    "Snowflake is still running the statement and did not say where its answer will be"
                );
            };
            if started.elapsed() > self.timeout {
                bail!(
                    "Snowflake was still running the statement after {} seconds",
                    self.timeout.as_secs()
                );
            }
            std::thread::sleep(wait);
            wait = (wait * 2).min(LONGEST_WAIT);
            answer = read(
                &self
                    .client
                    .get(&format!("{}{place}", self.base), &authorised(token))?,
            )?;
        }
        Ok(answer)
    }

    fn chunks(&self, data: &Value) -> anyhow::Result<Vec<Value>> {
        let Some(chunks) = data["chunks"]
            .as_array()
            .filter(|chunks| !chunks.is_empty())
        else {
            return Ok(Vec::new());
        };
        let headers: Vec<(&str, String)> = match data["chunkHeaders"].as_object() {
            Some(headers) => headers
                .iter()
                .filter_map(|(name, value)| Some((name.as_str(), value.as_str()?.to_string())))
                .collect(),
            None => data["qrmk"]
                .as_str()
                .map(|key| {
                    vec![
                        (
                            "x-amz-server-side-encryption-customer-algorithm",
                            "AES256".to_string(),
                        ),
                        ("x-amz-server-side-encryption-customer-key", key.to_string()),
                    ]
                })
                .unwrap_or_default(),
        };
        std::thread::scope(|scope| {
            let fetches: Vec<_> = chunks
                .iter()
                .map(|chunk| {
                    let headers = &headers;
                    scope.spawn(move || -> anyhow::Result<Value> {
                        let Some(address) = chunk["url"].as_str() else {
                            bail!("Snowflake sent part of an answer with no address");
                        };
                        let reply = self.client.get(address, headers)?;
                        if reply.status >= 400 {
                            bail!(
                                "could not fetch part of the answer from Snowflake's storage: it answered {}",
                                reply.status
                            );
                        }
                        serde_json::from_str(&format!("[{}]", reply.body)).map_err(|_| {
                            anyhow!("Snowflake's storage sent part of an answer tql could not read")
                        })
                    })
                })
                .collect();
            fetches
                .into_iter()
                .map(|fetch| {
                    fetch
                        .join()
                        .unwrap_or_else(|_| Err(anyhow!("fetching part of the answer failed")))
                })
                .collect()
        })
    }

    fn token(&self) -> anyhow::Result<String> {
        let current = self.lock().tokens.clone();
        if !current.fresh(now()) {
            self.refresh(&current.session)?;
        }
        Ok(self.lock().tokens.session.clone())
    }

    fn refresh(&self, stale: &str) -> anyhow::Result<()> {
        let mut state = self.lock();
        if state.tokens.session != stale {
            return Ok(());
        }
        let owner = owner(&self.account, &self.user);
        let now = now();
        if let Some(saved) = self.keeper.as_ref().and_then(|keeper| keeper.load(&owner))
            && saved.session != stale
            && saved.fresh(now)
        {
            state.tokens = saved;
            state.place = Place::default();
            return Ok(());
        }
        let tokens = match state.tokens.renewable(now) {
            true => self.renewed(&state.tokens).ok(),
            false => None,
        };
        let tokens = match tokens {
            Some(tokens) => tokens,
            None => {
                let wanted = state.place.clone();
                let data = sign_in(&self.client, &self.base, &self.account, &self.user, &wanted)?;
                state.place = Place::from_session(&data["sessionInfo"]);
                Tokens::signed_in(&data, now)
                    .ok_or_else(|| anyhow!("Snowflake signed in without handing over a session"))?
            }
        };
        if let Some(keeper) = &self.keeper {
            keeper.save(&tokens, &owner);
        }
        state.tokens = tokens;
        Ok(())
    }

    fn renewed(&self, tokens: &Tokens) -> anyhow::Result<Tokens> {
        let answer = browser::answered(self.client.post(
            &format!(
                "{}/session/token-request?requestId={}",
                self.base,
                request_id()
            ),
            &authorised(&tokens.master),
            json!({"oldSessionToken": tokens.session, "requestType": "RENEW"}).to_string(),
        )?)?;
        tokens
            .renewed(&answer["data"], now())
            .ok_or_else(|| anyhow!("Snowflake renewed the session without handing it over"))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn sign_in(
    client: &http::Client,
    base: &str,
    account: &str,
    user: &str,
    wanted: &Place,
) -> anyhow::Result<Value> {
    browser::sign_in(
        client,
        &Account {
            base,
            name: account,
            user,
        },
        &[
            ("roleName", &wanted.role),
            ("warehouse", &wanted.warehouse),
            ("databaseName", &wanted.database),
            ("schemaName", &wanted.schema),
        ],
    )
}

pub fn owner(account: &str, user: &str) -> String {
    format!("{account}.{}", user.trim().to_uppercase())
}

fn authorised(token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", format!("Snowflake Token=\"{token}\"")),
        ("Content-Type", "application/json".to_string()),
        ("Accept", "application/snowflake".to_string()),
    ]
}

fn read(reply: &http::Reply) -> anyhow::Result<Value> {
    serde_json::from_str(&reply.body).map_err(|_| match reply.status {
        401 | 403 => anyhow!("Snowflake refused the session ({})", reply.status),
        status => anyhow!(
            "Snowflake answered {status} with something tql could not read: {}",
            reply.body.trim()
        ),
    })
}

fn code(answer: &Value) -> Option<&str> {
    answer["code"].as_str()
}

fn said(answer: &Value) -> String {
    answer["message"]
        .as_str()
        .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| "Snowflake could not run the statement".to_string())
}

fn same(current: &str, desired: &str) -> bool {
    match simple(desired) {
        true => current.eq_ignore_ascii_case(desired),
        false => current == desired,
    }
}

fn simple(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_' || rest == '$')
}

fn identifier(name: &str) -> String {
    match simple(name) {
        true => name.to_string(),
        false => super::quote_double(name),
    }
}

fn request_id() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_name_snowflake_would_fold_unquoted_and_quotes_the_rest() {
        assert_eq!(identifier("analytics"), "analytics");
        assert_eq!(identifier("COMPUTE_WH"), "COMPUTE_WH");
        assert_eq!(identifier("Mixed Case"), "\"Mixed Case\"");
        assert!(same("ANALYTICS", "analytics"));
        assert!(!same("MIXED CASE", "Mixed Case"));
    }

    #[test]
    fn counts_the_rows_a_dml_statement_reports() {
        let sent = Sent {
            data: json!({"statementTypeId": 12544, "rowset": [["3", "1"]]}),
            chunks: Vec::new(),
        };
        assert_eq!(sent.written(), Some(4));
        let select = Sent {
            data: json!({"statementTypeId": 4096, "rowset": [["3"]]}),
            chunks: Vec::new(),
        };
        assert_eq!(select.written(), None);
    }

    #[test]
    fn restores_only_tokens_saved_for_the_same_account_and_user() {
        let tokens = Tokens {
            session: "s".into(),
            master: "m".into(),
            session_until: 10,
            master_until: 20,
        };
        let saved = tokens.saved(&owner("ACME-MAIN", "me@acme.com"));
        assert_eq!(
            Tokens::restored(&saved, &owner("ACME-MAIN", "ME@ACME.COM")),
            Some(tokens)
        );
        assert_eq!(Tokens::restored(&saved, &owner("ACME-MAIN", "you")), None);
    }
}
