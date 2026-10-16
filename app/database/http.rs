use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, TcpConnector,
    Transport, TransportAdapter,
};

pub struct Reply {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub broken: Option<String>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

pub fn post(
    url: &str,
    headers: &[(&str, String)],
    body: String,
    timeout: Duration,
) -> anyhow::Result<Reply> {
    let reply = send(url, headers, body, timeout, None)?;
    if let Some(broken) = &reply.broken {
        bail!("could not read the answer from {}: {broken}", site(url));
    }
    Ok(reply)
}

pub fn send(
    url: &str,
    headers: &[(&str, String)],
    body: String,
    timeout: Duration,
    tls: Option<Arc<ClientConfig>>,
) -> anyhow::Result<Reply> {
    let mut pending = agent(timeout, tls).post(url);
    for (name, value) in headers {
        pending = pending.header(*name, value);
    }
    replied(
        pending
            .send(body)
            .map_err(|error| anyhow!("could not reach {}: {error}", site(url)))?,
    )
}

pub struct Client {
    agent: ureq::Agent,
}

impl Client {
    pub fn new(timeout: Duration) -> Client {
        Client {
            agent: agent(timeout, None),
        }
    }

    pub fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: String,
    ) -> anyhow::Result<Reply> {
        let mut pending = self.agent.post(url);
        for (name, value) in headers {
            pending = pending.header(*name, value);
        }
        whole(
            url,
            pending
                .send(body)
                .map_err(|error| anyhow!("could not reach {}: {error}", site(url)))?,
        )
    }

    pub fn get(&self, url: &str, headers: &[(&str, String)]) -> anyhow::Result<Reply> {
        let mut pending = self.agent.get(url);
        for (name, value) in headers {
            pending = pending.header(*name, value);
        }
        whole(
            url,
            pending
                .call()
                .map_err(|error| anyhow!("could not reach {}: {error}", site(url)))?,
        )
    }
}

fn whole(url: &str, response: ureq::http::Response<ureq::Body>) -> anyhow::Result<Reply> {
    let reply = replied(response)?;
    if let Some(broken) = &reply.broken {
        bail!("could not read the answer from {}: {broken}", site(url));
    }
    Ok(reply)
}

fn agent(timeout: Duration, tls: Option<Arc<ClientConfig>>) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .user_agent(concat!("tql/", env!("CARGO_PKG_VERSION")))
        .build();
    match tls {
        None => config.new_agent(),
        Some(tls) => ureq::Agent::with_parts(
            config,
            TcpConnector::default().chain(Secure(tls)),
            DefaultResolver::default(),
        ),
    }
}

fn replied(response: ureq::http::Response<ureq::Body>) -> anyhow::Result<Reply> {
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            Some((
                name.as_str().to_ascii_lowercase(),
                value.to_str().ok()?.to_string(),
            ))
        })
        .collect();
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    let mut chunk = [0; 64 * 1024];
    let broken = loop {
        match reader.read(&mut chunk) {
            Ok(0) => break None,
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => break Some(error.to_string()),
        }
    };
    Ok(Reply {
        status,
        headers,
        body: String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()),
        broken,
    })
}

fn site(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

#[derive(Debug)]
struct Secure(Arc<ClientConfig>);

impl<In: Transport> Connector<In> for Secure {
    type Out = Either<In, Secured>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Ok(None);
        };
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }
        let host = details
            .uri
            .host()
            .ok_or(ureq::Error::Tls("the address has no host"))?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let name = ServerName::try_from(host)
            .map_err(|_| ureq::Error::Tls("the host is not a name TLS can check"))?;
        let mut connection = ClientConnection::new(self.0.clone(), name)?;
        let mut socket = TransportAdapter::new(transport.boxed());
        socket.set_timeout(details.timeout);
        connection.complete_io(&mut socket)?;
        Ok(Some(Either::B(Secured {
            buffers: LazyBuffers::new(
                details.config.input_buffer_size(),
                details.config.output_buffer_size(),
            ),
            stream: StreamOwned::new(connection, socket),
        })))
    }
}

struct Secured {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter>,
}

impl std::fmt::Debug for Secured {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Secured")
    }
}

impl Transport for Secured {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let read = self.stream.read(input)?;
        self.buffers.input_appended(read);
        Ok(read > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}
