use std::time::Duration;

use anyhow::anyhow;

pub struct Reply {
    pub status: u16,
    pub body: String,
}

pub fn post(
    url: &str,
    headers: &[(&str, String)],
    body: String,
    timeout: Duration,
) -> anyhow::Result<Reply> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .user_agent(concat!("tql/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent();
    let mut pending = agent.post(url);
    for (name, value) in headers {
        pending = pending.header(*name, value);
    }
    let response = pending
        .send(body)
        .map_err(|error| anyhow!("could not reach {url}: {error}"))?;
    let status = response.status().as_u16();
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|error| anyhow!("could not read the answer from {url}: {error}"))?;
    Ok(Reply { status, body })
}
