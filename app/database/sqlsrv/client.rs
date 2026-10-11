use super::*;

pub(super) fn configured(
    connection: &Connection,
    password: Option<&str>,
    database: Option<&str>,
) -> Config {
    let mut config = Config::new();
    config.host(connection.host.as_deref().unwrap_or("127.0.0.1"));
    config.port(connection.port.unwrap_or(1433));
    config.application_name("tql");
    config.authentication(AuthMethod::sql_server(
        connection.username.as_deref().unwrap_or_default(),
        password.unwrap_or_default(),
    ));
    if let Some(database) = database.filter(|database| !database.trim().is_empty()) {
        config.database(database);
    }

    let chosen = encryption(connection.ssl_mode.as_deref());
    config.encryption(if chosen.encrypt {
        EncryptionLevel::Required
    } else {
        EncryptionLevel::Off
    });
    if chosen.trust_server_certificate {
        config.trust_cert();
    } else if let Some(authority) = connection
        .ssl_ca
        .as_deref()
        .map(str::trim)
        .filter(|authority| !authority.is_empty())
    {
        config.trust_cert_ca(crate::paths::expand(authority));
    }
    config
}

pub(super) async fn reach(config: Config) -> tiberius::Result<Wire> {
    let stream = TcpStream::connect(config.get_addr()).await?;
    stream.set_nodelay(true)?;
    match Client::connect(config.clone(), stream.compat_write()).await {
        Err(tiberius::error::Error::Routing { host, port }) => {
            let mut routed = config;
            routed.host(&host);
            routed.port(port);
            let stream = TcpStream::connect(routed.get_addr()).await?;
            stream.set_nodelay(true)?;
            Client::connect(routed, stream.compat_write()).await
        }
        other => other,
    }
}

pub(super) fn dial(runtime: &Runtime, config: Config) -> anyhow::Result<Wire> {
    runtime.block_on(async {
        tokio::time::timeout(CONNECT_TIMEOUT, reach(config))
            .await
            .map_err(|_| anyhow!("timed out"))?
            .map_err(said)
    })
}

pub(super) fn said(error: tiberius::error::Error) -> anyhow::Error {
    match error {
        tiberius::error::Error::Server(token) => anyhow!(token.message().to_string()),
        other => anyhow!(other),
    }
}

impl SqlServer {
    pub fn through(
        connection: &Connection,
        password: Option<String>,
        tunnel: Option<Arc<Tunnel>>,
    ) -> anyhow::Result<Self> {
        let Some(tunnel) = tunnel else {
            return Self::open(connection, password);
        };
        let routed = Connection {
            host: Some("127.0.0.1".to_string()),
            port: Some(tunnel.port),
            ..connection.clone()
        };
        Ok(SqlServer {
            _tunnel: Some(tunnel),
            ..Self::open(&routed, password)?
        })
    }

    pub fn open(connection: &Connection, password: Option<String>) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let client = dial(
            &runtime,
            configured(
                connection,
                password.as_deref(),
                connection.database.as_deref(),
            ),
        )
        .with_context(|| format!("cannot connect to {}", connection.describe()))?;
        Ok(SqlServer {
            runtime,
            client,
            connection: connection.clone(),
            password,
            _tunnel: None,
        })
    }

    pub(super) fn reconnect(&self, database: &str) -> anyhow::Result<Wire> {
        dial(
            &self.runtime,
            configured(&self.connection, self.password.as_deref(), Some(database)),
        )
    }

    pub(super) fn query(
        &mut self,
        statement: &str,
        values: &[Option<&str>],
    ) -> anyhow::Result<Page> {
        let (headers, rows) = self.fetch(statement, values, text)?;
        Ok(Page {
            statement: statement.to_string(),
            headers,
            kinds: all_text(&rows),
            rows,
        })
    }

    pub(super) fn fetch<Cell>(
        &mut self,
        statement: &str,
        values: &[Option<&str>],
        convert: fn(ColumnData<'static>, ColumnType) -> Cell,
    ) -> anyhow::Result<(Vec<String>, Vec<Vec<Cell>>)> {
        let client = &mut self.client;
        self.runtime.block_on(async {
            let stream = if values.is_empty() {
                client.simple_query(statement).await.map_err(said)?
            } else {
                let mut query = Query::new(numbered_parameters(statement));
                for value in values {
                    query.bind(*value);
                }
                query.query(client).await.map_err(said)?
            };
            first_result(stream, convert).await
        })
    }

    pub(super) fn execute(
        &mut self,
        statement: &str,
        values: &[Option<&str>],
    ) -> anyhow::Result<u64> {
        let client = &mut self.client;
        self.runtime.block_on(async {
            let mut query = Query::new(numbered_parameters(statement));
            for value in values {
                query.bind(*value);
            }
            Ok(query.execute(client).await.map_err(said)?.total())
        })
    }

    pub(super) fn column(&mut self, statement: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .query(statement, &[])?
            .rows
            .into_iter()
            .filter_map(|mut row| row.swap_remove(0))
            .collect())
    }

    pub(super) fn write(
        &mut self,
        statement: &str,
        values: &[Option<&str>],
        shown: &str,
    ) -> anyhow::Result<u64> {
        self.execute(statement, values)
            .map_err(|error| refused(error, shown))
    }
}

pub(super) async fn first_result<Cell>(
    mut stream: QueryStream<'_>,
    convert: fn(ColumnData<'static>, ColumnType) -> Cell,
) -> anyhow::Result<(Vec<String>, Vec<Vec<Cell>>)> {
    let mut headers = Vec::new();
    let mut types = Vec::new();
    let mut rows = Vec::new();
    while let Some(item) = stream.try_next().await.map_err(said)? {
        match item {
            QueryItem::Metadata(metadata) if metadata.result_index() == 0 => {
                headers = metadata
                    .columns()
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect();
                types = metadata
                    .columns()
                    .iter()
                    .map(|column| column.column_type())
                    .collect();
            }
            QueryItem::Row(row) if row.result_index() == 0 => {
                rows.push(
                    row.into_iter()
                        .zip(&types)
                        .map(|(value, column_type)| convert(value, *column_type))
                        .collect(),
                );
            }
            _ => {}
        }
    }
    Ok((headers, rows))
}
