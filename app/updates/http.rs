use std::time::Duration;

const LARGEST_DOWNLOAD: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    Body(Vec<u8>),
    Missing,
    Unreachable,
}

impl Fetched {
    pub fn body(self) -> Option<Vec<u8>> {
        match self {
            Fetched::Body(body) => Some(body),
            Fetched::Missing | Fetched::Unreachable => None,
        }
    }
}

pub fn fetch(url: &str, seconds: u64, accept: &str) -> Fetched {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .user_agent(concat!("tql/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent();
    match agent.get(url).header("Accept", accept).call() {
        Ok(response) => response
            .into_body()
            .with_config()
            .limit(LARGEST_DOWNLOAD)
            .read_to_vec()
            .map(Fetched::Body)
            .unwrap_or(Fetched::Unreachable),
        Err(ureq::Error::StatusCode(404 | 410)) => Fetched::Missing,
        Err(_) => Fetched::Unreachable,
    }
}
