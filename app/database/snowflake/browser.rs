use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use serde_json::{Value, json};

use super::super::http;
use crate::dsn;

const WAIT: Duration = Duration::from_secs(180);
const SIGNED_IN: &str = r#"<!doctype html>
<meta charset="utf-8">
<title>tql</title>
<body style="font:16px system-ui;margin:3em">
<p>tql is signed in to Snowflake.</p>
<p id="closing">This tab closes in <span id="seconds">10</span> seconds.</p>
<script>
let seconds = 10;
const tick = setInterval(() => {
  seconds -= 1;
  document.getElementById("seconds").textContent = seconds;
  if (seconds > 0) return;
  clearInterval(tick);
  window.close();
  setTimeout(() => {
    document.getElementById("closing").textContent = "You can close this tab.";
  }, 250);
}, 1000);
</script>
</body>"#;

pub type Opener = fn(&str) -> anyhow::Result<()>;

static OPENER: Mutex<Opener> = Mutex::new(system);

pub fn open_with(opener: Opener) {
    *OPENER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = opener;
}

pub struct Account<'a> {
    pub base: &'a str,
    pub name: &'a str,
    pub user: &'a str,
}

pub fn sign_in(
    client: &http::Client,
    account: &Account,
    context: &[(&str, &Option<String>)],
) -> anyhow::Result<Value> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| anyhow!("could not listen for the Snowflake sign-in: {error}"))?;
    let port = listener.local_addr()?.port();
    let mut data = identity(account);
    data.insert("BROWSER_MODE_REDIRECT_PORT".into(), json!(port.to_string()));
    let asked = answered(client.post(
        &format!("{}/session/authenticator-request", account.base),
        &headers(),
        json!({ "data": data }).to_string(),
    )?)?;
    let (Some(address), Some(proof)) = (
        asked["data"]["ssoUrl"].as_str(),
        asked["data"]["proofKey"].as_str(),
    ) else {
        bail!("Snowflake did not say where to sign in; is SSO set up for this account?");
    };
    let opener = *OPENER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    opener(address).map_err(|error| {
        anyhow!("could not open a browser to sign in to Snowflake ({error}); open {address}")
    })?;
    let token = caught(&listener)?;
    let mut data = identity(account);
    data.insert("TOKEN".into(), json!(token));
    data.insert("PROOF_KEY".into(), json!(proof));
    let query: Vec<String> = context
        .iter()
        .filter_map(|(name, value)| Some(format!("{name}={}", dsn::encode(value.as_deref()?))))
        .collect();
    let logged = answered(client.post(
        &format!(
            "{}/session/v1/login-request?{}",
            account.base,
            query.join("&")
        ),
        &headers(),
        json!({ "data": data }).to_string(),
    )?)?;
    Ok(logged["data"].clone())
}

pub fn headers() -> Vec<(&'static str, String)> {
    vec![
        ("Content-Type", "application/json".to_string()),
        ("Accept", "application/json".to_string()),
    ]
}

pub fn answered(reply: http::Reply) -> anyhow::Result<Value> {
    let body: Value = serde_json::from_str(&reply.body).map_err(|_| {
        anyhow!(
            "Snowflake answered {} to the sign-in: {}",
            reply.status,
            reply.body.trim()
        )
    })?;
    if body["success"].as_bool() != Some(true) {
        bail!(
            "Snowflake refused the sign-in: {}",
            body["message"]
                .as_str()
                .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| format!("it answered {}", reply.status))
        );
    }
    Ok(body)
}

fn identity(account: &Account) -> serde_json::Map<String, Value> {
    let mut data = serde_json::Map::new();
    data.insert("CLIENT_APP_ID".into(), json!("tql"));
    data.insert(
        "CLIENT_APP_VERSION".into(),
        json!(env!("CARGO_PKG_VERSION")),
    );
    data.insert("ACCOUNT_NAME".into(), json!(account.name));
    data.insert("LOGIN_NAME".into(), json!(account.user));
    data.insert("AUTHENTICATOR".into(), json!("EXTERNALBROWSER"));
    data
}

fn caught(listener: &TcpListener) -> anyhow::Result<String> {
    listener.set_nonblocking(true)?;
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(token) = token_from(stream) {
                    return Ok(token);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > WAIT {
                    bail!(
                        "the Snowflake sign-in was not finished in the browser within {} seconds",
                        WAIT.as_secs()
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => bail!("the Snowflake sign-in could not be received: {error}"),
        }
    }
}

fn token_from(mut stream: TcpStream) -> Option<String> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length.min(64 * 1024)];
    reader.read_exact(&mut body).ok()?;
    let target = first.split_whitespace().nth(1).unwrap_or_default();
    let query = target.split_once('?').map(|(_, query)| query).unwrap_or("");
    let token = [query, &String::from_utf8_lossy(&body)]
        .into_iter()
        .flat_map(|pairs| pairs.split('&'))
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == "token")
        .map(|(_, value)| dsn::decode(value, true))
        .filter(|token| !token.is_empty());
    let (status, page) = match token {
        Some(_) => ("200 OK", SIGNED_IN),
        None => ("404 Not Found", ""),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
        page.len()
    );
    token
}

fn system(address: &str) -> anyhow::Result<()> {
    let mut command = match std::env::consts::OS {
        "macos" => Command::new("open"),
        "windows" => {
            let mut command = Command::new("rundll32");
            command.arg("url.dll,FileProtocolHandler");
            command
        }
        _ => Command::new("xdg-open"),
    };
    let status = command
        .arg(address)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    match status.success() {
        true => Ok(()),
        false => bail!("the browser opener exited with {status}"),
    }
}
