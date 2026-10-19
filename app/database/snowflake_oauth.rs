use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::http;
use crate::dsn;

const CLIENT: &str = "LOCAL_APPLICATION";
const SIGN_IN_WAIT: Duration = Duration::from_secs(180);
const EARLY: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct Tokens {
    access: String,
    refresh: Option<String>,
    expires: Instant,
}

pub type Save = Box<dyn Fn(&str) + Send + Sync>;

pub struct Session {
    base: String,
    role: Option<String>,
    tokens: Mutex<Tokens>,
    saved: Mutex<Option<String>>,
    save: Option<Save>,
}

fn signed_in() -> &'static Mutex<HashMap<String, Tokens>> {
    static SIGNED_IN: OnceLock<Mutex<HashMap<String, Tokens>>> = OnceLock::new();
    SIGNED_IN.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Session {
    pub fn start(
        base: &str,
        role: Option<&str>,
        saved: Option<&str>,
        save: Option<Save>,
    ) -> anyhow::Result<Session> {
        let role = role.map(scoped_role);
        let key = cache_key(base, role.as_deref());
        let saved = saved
            .and_then(|saved| serde_json::from_str::<Value>(saved).ok())
            .filter(|saved| saved["for"].as_str() == Some(key.as_str()))
            .and_then(|saved| saved["refresh"].as_str().map(String::from));
        let remembered = signed_in()
            .lock()
            .ok()
            .and_then(|tokens| tokens.get(&key).cloned());
        let tokens = match remembered {
            Some(tokens) => tokens,
            None => match saved
                .as_deref()
                .and_then(|refresh| refreshed(base, refresh))
            {
                Some(tokens) => tokens,
                None => sign_in(base, role.as_deref())?,
            },
        };
        let session = Session {
            base: base.to_string(),
            role,
            tokens: Mutex::new(tokens.clone()),
            saved: Mutex::new(saved),
            save,
        };
        session.remember(&tokens);
        Ok(session)
    }

    pub fn access_token(&self) -> anyhow::Result<String> {
        let mut tokens = self.lock()?;
        if tokens.expires.saturating_duration_since(Instant::now()) < EARLY {
            *tokens = self.renewed(&tokens)?;
            self.remember(&tokens);
        }
        Ok(tokens.access.clone())
    }

    pub fn renew(&self, rejected: &str) -> anyhow::Result<()> {
        let mut tokens = self.lock()?;
        if tokens.access == rejected {
            *tokens = self.renewed(&tokens)?;
            self.remember(&tokens);
        }
        Ok(())
    }

    fn lock(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Tokens>> {
        self.tokens
            .lock()
            .map_err(|_| anyhow!("the Snowflake sign-in broke while renewing it"))
    }

    fn renewed(&self, tokens: &Tokens) -> anyhow::Result<Tokens> {
        match tokens
            .refresh
            .as_deref()
            .and_then(|refresh| refreshed(&self.base, refresh))
        {
            Some(tokens) => Ok(tokens),
            None => sign_in(&self.base, self.role.as_deref()),
        }
    }

    fn remember(&self, tokens: &Tokens) {
        let key = cache_key(&self.base, self.role.as_deref());
        if let Ok(mut remembered) = signed_in().lock() {
            remembered.insert(key.clone(), tokens.clone());
        }
        let (Some(save), Some(refresh), Ok(mut saved)) =
            (&self.save, &tokens.refresh, self.saved.lock())
        else {
            return;
        };
        if saved.as_ref() != Some(refresh) {
            save(&serde_json::json!({ "for": key, "refresh": refresh }).to_string());
            *saved = Some(refresh.clone());
        }
    }
}

fn refreshed(base: &str, refresh: &str) -> Option<Tokens> {
    let mut tokens = requested(
        base,
        &[("grant_type", "refresh_token"), ("refresh_token", refresh)],
    )
    .ok()?;
    tokens.refresh = tokens.refresh.or_else(|| Some(refresh.to_string()));
    Some(tokens)
}

fn cache_key(base: &str, role: Option<&str>) -> String {
    format!("{base} {}", role.unwrap_or_default())
}

fn scoped_role(role: &str) -> String {
    let plain = role.chars().enumerate().all(|(at, c)| {
        c == '_' || c.is_ascii_alphabetic() || (at > 0 && (c.is_ascii_digit() || c == '$'))
    });
    match plain {
        true => role.to_ascii_uppercase(),
        false => role.trim_matches('"').to_string(),
    }
}

fn sign_in(base: &str, role: Option<&str>) -> anyhow::Result<Tokens> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| anyhow!("could not listen for the Snowflake sign-in: {error}"))?;
    let redirect = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());
    let verifier = random(32);
    let state = random(16);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let scope = match role {
        Some(role) => format!("refresh_token session:role-encoded:{}", dsn::encode(role)),
        None => "refresh_token".to_string(),
    };
    let url = format!(
        "{base}/oauth/authorize?{}",
        form(&[
            ("response_type", "code"),
            ("client_id", CLIENT),
            ("redirect_uri", &redirect),
            ("state", &state),
            ("scope", &scope),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ])
    );
    open_browser(&url)?;
    let code = awaited(&listener, &state)?;
    requested(
        base,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
        ],
    )
}

fn random(bytes: usize) -> String {
    let mut buffer = vec![0; bytes];
    getrandom::fill(&mut buffer).expect("the system has no source of randomness");
    URL_SAFE_NO_PAD.encode(buffer)
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{key}={}", dsn::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn open_browser(url: &str) -> anyhow::Result<()> {
    let browser = std::env::var("BROWSER")
        .ok()
        .filter(|browser| !browser.trim().is_empty());
    let mut command = match &browser {
        Some(browser) => {
            let mut words = browser.split_whitespace();
            let mut command = Command::new(words.next().unwrap_or_default());
            command.args(words);
            command
        }
        None if cfg!(target_os = "macos") => Command::new("open"),
        None => Command::new("xdg-open"),
    };
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|error| anyhow!("could not open a browser to sign in to Snowflake: {error}"))
}

fn awaited(listener: &TcpListener, state: &str) -> anyhow::Result<String> {
    listener.set_nonblocking(true)?;
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(answer) = answered(stream, state) {
                    return answer;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > SIGN_IN_WAIT {
                    bail!(
                        "the Snowflake sign-in did not finish within {} minutes. If Snowflake showed an error, the account may not have OAuth for local applications (SNOWFLAKE$LOCAL_APPLICATION) yet; an access token saved as the connection's password works instead",
                        SIGN_IN_WAIT.as_secs() / 60
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => bail!("the Snowflake sign-in stopped listening: {error}"),
        }
    }
}

fn answered(mut stream: TcpStream, state: &str) -> Option<anyhow::Result<String>> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    let mut first = String::new();
    BufReader::new(stream.try_clone().ok()?)
        .read_line(&mut first)
        .ok()?;
    let target = first.split_whitespace().nth(1).unwrap_or_default();
    let query = target
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let value = |wanted: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == wanted)
            .map(|(_, value)| dsn::decode(value, true))
    };
    if value("state").as_deref() != Some(state) {
        let _ = reply(
            &mut stream,
            404,
            "Not the Snowflake sign-in tql is waiting for.",
        );
        return None;
    }
    if let Some(code) = value("code") {
        let _ = reply(
            &mut stream,
            200,
            "Signed in to Snowflake. You can close this tab and go back to tql.",
        );
        return Some(Ok(code));
    }
    let reason = value("error_description")
        .or_else(|| value("error"))
        .unwrap_or_else(|| "Snowflake sent no code".to_string());
    let _ = reply(
        &mut stream,
        400,
        &format!("The Snowflake sign-in failed: {reason}"),
    );
    Some(Err(anyhow!("the Snowflake sign-in failed: {reason}")))
}

fn reply(stream: &mut TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    let message = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>tql</title><p style=\"font:16px system-ui;margin:3em\">{message}</p>"
    );
    write!(
        stream,
        "HTTP/1.1 {status} {}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        if status == 200 { "OK" } else { "Error" },
        body.len()
    )
}

fn requested(base: &str, pairs: &[(&str, &str)]) -> anyhow::Result<Tokens> {
    let answer = http::post(
        &format!("{base}/oauth/token-request"),
        &[
            (
                "Authorization",
                format!("Basic {}", STANDARD.encode(format!("{CLIENT}:{CLIENT}"))),
            ),
            (
                "Content-Type",
                "application/x-www-form-urlencoded".to_string(),
            ),
            ("Accept", "application/json".to_string()),
        ],
        form(pairs),
        TIMEOUT,
    )?;
    let body: Value = serde_json::from_str(&answer.body).unwrap_or_default();
    let Some(access) = body["access_token"]
        .as_str()
        .filter(|_| answer.status < 400)
    else {
        let reason = ["message", "error_description", "error"]
            .iter()
            .find_map(|key| body[key].as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                format!(
                    "Snowflake answered {}: {}",
                    answer.status,
                    answer.body.trim()
                )
            });
        bail!("Snowflake would not sign tql in: {reason}");
    };
    Ok(Tokens {
        access: access.to_string(),
        refresh: body["refresh_token"].as_str().map(String::from),
        expires: Instant::now() + Duration::from_secs(body["expires_in"].as_u64().unwrap_or(600)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_a_plain_role_in_capitals_and_a_quoted_one_as_typed() {
        assert_eq!(scoped_role("analyst"), "ANALYST");
        assert_eq!(scoped_role("data_eng$2"), "DATA_ENG$2");
        assert_eq!(scoped_role("\"Mixed Role\""), "Mixed Role");
    }
}
