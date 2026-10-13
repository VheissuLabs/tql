use std::fs;
use std::net::{Ipv4Addr, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use app::database::{self, Database};
use app::driver::Driver;
use app::encrypter::Encrypter;
use app::store::Connection;
use app::tunnel;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};

struct Authority {
    issuer: Issuer<'static, KeyPair>,
    pem: String,
}

impl Authority {
    fn new(name: &str) -> Authority {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let key = KeyPair::generate().unwrap();
        let pem = params.self_signed(&key).unwrap().pem();
        Authority {
            issuer: Issuer::new(params, key),
            pem,
        }
    }

    fn sign_into(&self, directory: &Path, names: &[&str]) {
        let key = KeyPair::generate().unwrap();
        let certificate = CertificateParams::new(
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap()
        .signed_by(&key, &self.issuer)
        .unwrap();
        fs::write(directory.join("ca.pem"), &self.pem).unwrap();
        fs::write(directory.join("server.pem"), certificate.pem()).unwrap();
        fs::write(directory.join("server.key"), key.serialize_pem()).unwrap();
        for file in ["ca.pem", "server.pem", "server.key"] {
            fs::set_permissions(directory.join(file), fs::Permissions::from_mode(0o644)).unwrap();
        }
    }
}

struct Lab {
    prefix: String,
    directory: PathBuf,
    containers: Vec<String>,
    network: Option<String>,
}

impl Lab {
    fn new(name: &str) -> Lab {
        let prefix = format!("tql-rust-test-{name}-{}", std::process::id());
        let directory = std::env::temp_dir().join(&prefix);
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        Lab {
            prefix,
            directory,
            containers: Vec::new(),
            network: None,
        }
    }

    fn network(&mut self) -> String {
        let network = format!("{}-net", self.prefix);
        docker(&["network", "create", &network]);
        self.network = Some(network.clone());
        network
    }

    fn run(&mut self, role: &str, arguments: &[&str]) -> String {
        let name = format!("{}-{role}", self.prefix);
        let mut command = vec!["run", "-d", "--name", &name];
        let network = self.network.clone();
        if let Some(network) = &network {
            command.extend(["--network", network]);
        }
        command.extend(arguments);
        self.containers.push(name.clone());
        docker(&command);
        name
    }

    fn port(&self, container: &str, inside: u16) -> u16 {
        docker(&["port", container, &format!("{inside}/tcp")])
            .lines()
            .next()
            .and_then(|line| line.rsplit(':').next())
            .and_then(|port| port.trim().parse().ok())
            .unwrap()
    }

    fn path(&self, name: &str) -> String {
        self.directory.join(name).display().to_string()
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        for container in &self.containers {
            let _ = Command::new("docker")
                .args(["rm", "-f", "-v", container])
                .output();
        }
        if let Some(network) = &self.network {
            let _ = Command::new("docker")
                .args(["network", "rm", network])
                .output();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn docker(arguments: &[&str]) -> String {
    let output = Command::new("docker").args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "docker {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn eventually(connection: &Connection, encrypter: Option<&Encrypter>) -> Box<dyn Database> {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match database::connect(connection, encrypter) {
            Ok(database) => return database,
            Err(error) if Instant::now() > deadline => panic!("never came up: {error:#}"),
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn mysql(port: u16, mode: &str) -> Connection {
    Connection {
        name: "mysql".to_string(),
        driver: Driver::Mysql,
        host: Some("127.0.0.1".to_string()),
        port: Some(port),
        username: Some("root".to_string()),
        ssl_mode: Some(mode.to_string()),
        ..Connection::default()
    }
}

fn cipher(database: &mut Box<dyn Database>) -> String {
    database
        .select("show session status like 'Ssl_cipher'", &[])
        .unwrap()
        .rows[0][1]
        .clone()
        .unwrap_or_default()
}

fn connect_error(connection: &Connection) -> String {
    match database::connect(connection, None) {
        Ok(_) => String::from("connected"),
        Err(error) => format!("{error:#}"),
    }
}

#[test]
#[ignore = "starts throwaway mysql, postgres and sshd containers in docker"]
fn mysql_speaks_tls_as_far_as_the_mode_asks() {
    let mut lab = Lab::new("mysql-tls");
    Authority::new("tql test ca").sign_into(&lab.directory, &["127.0.0.1", "localhost"]);
    let certificates = format!("{}:/certs:ro", lab.directory.display());
    let container = lab.run(
        "mysql",
        &[
            "-p",
            "127.0.0.1::3306",
            "-e",
            "MYSQL_ALLOW_EMPTY_PASSWORD=yes",
            "-v",
            &certificates,
            "mysql:8.0",
            "--require-secure-transport=ON",
            "--ssl-ca=/certs/ca.pem",
            "--ssl-cert=/certs/server.pem",
            "--ssl-key=/certs/server.key",
        ],
    );
    let port = lab.port(&container, 3306);

    let mut required = eventually(&mysql(port, "require"), None);
    assert!(!cipher(&mut required).is_empty());

    let mut preferred = database::connect(&mysql(port, "prefer"), None).unwrap();
    assert!(!cipher(&mut preferred).is_empty());

    let full = Connection {
        ssl_ca: Some(lab.path("ca.pem")),
        ..mysql(port, "verify-full")
    };
    let mut verified = database::connect(&full, None).unwrap();
    assert!(!cipher(&mut verified).is_empty());

    assert!(connect_error(&mysql(port, "")).contains("insecure transport are prohibited"));
    assert!(connect_error(&mysql(port, "disable")).contains("insecure transport"));

    let stranger = Lab::new("mysql-stranger");
    Authority::new("someone else").sign_into(&stranger.directory, &["127.0.0.1"]);
    let untrusted = Connection {
        ssl_ca: Some(stranger.path("ca.pem")),
        ..mysql(port, "verify-ca")
    };
    assert_ne!(connect_error(&untrusted), "connected");
    assert_ne!(connect_error(&mysql(port, "verify-full")), "connected");
}

#[test]
#[ignore = "starts throwaway mysql, postgres and sshd containers in docker"]
fn mysql_prefer_falls_back_to_plain_when_the_server_has_no_tls() {
    let mut lab = Lab::new("mysql-plain");
    let container = lab.run(
        "mysql",
        &[
            "-p",
            "127.0.0.1::3306",
            "-e",
            "MYSQL_ALLOW_EMPTY_PASSWORD=yes",
            "mysql:8.0",
            "--skip-ssl",
        ],
    );
    let port = lab.port(&container, 3306);

    let mut preferred = eventually(&mysql(port, "prefer"), None);

    assert_eq!(cipher(&mut preferred), "");
    assert_ne!(connect_error(&mysql(port, "require")), "connected");
}

fn postgres_server(lab: &mut Lab, names: &[&str], extra: &[&str]) -> String {
    Authority::new("tql test ca").sign_into(&lab.directory, names);
    let certificates = format!("{}:/certs:ro", lab.directory.display());
    let mut arguments = vec!["-e", "POSTGRES_PASSWORD=secret", "-v", &certificates];
    arguments.extend(extra);
    arguments.extend([
        "postgres:16-alpine",
        "sh",
        "-c",
        "cp /certs/server.pem /certs/server.key /tmp/ && chown postgres /tmp/server.* && chmod 600 /tmp/server.key && exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/tmp/server.pem -c ssl_key_file=/tmp/server.key",
    ]);
    lab.run("postgres", &arguments)
}

fn postgres(host: &str, port: u16, mode: &str) -> Connection {
    Connection {
        name: "postgres".to_string(),
        driver: Driver::Pgsql,
        host: Some(host.to_string()),
        port: Some(port),
        username: Some("postgres".to_string()),
        password: None,
        ssl_mode: Some(mode.to_string()),
        ..Connection::default()
    }
}

fn with_password(connection: Connection, encrypter: &Encrypter) -> Connection {
    Connection {
        password: Some(encrypter.encrypt_string("secret").unwrap()),
        ..connection
    }
}

fn encrypted(database: &mut Box<dyn Database>) -> String {
    database
        .select(
            "select ssl::text from pg_stat_ssl where pid = pg_backend_pid()",
            &[],
        )
        .unwrap()
        .rows[0][0]
        .clone()
        .unwrap_or_default()
}

#[test]
#[ignore = "starts throwaway mysql, postgres and sshd containers in docker"]
fn postgres_speaks_tls_as_far_as_the_mode_asks() {
    let mut lab = Lab::new("pg-tls");
    let encrypter = Encrypter::throwaway().unwrap();
    let container = postgres_server(&mut lab, &["127.0.0.1"], &["-p", "127.0.0.1::5432"]);
    let port = lab.port(&container, 5432);
    let reach = |mode: &str| with_password(postgres("127.0.0.1", port, mode), &encrypter);

    let mut unset = eventually(&reach(""), Some(&encrypter));
    assert_eq!(encrypted(&mut unset), "true");

    for mode in ["prefer", "require"] {
        let mut database = database::connect(&reach(mode), Some(&encrypter)).unwrap();
        assert_eq!(encrypted(&mut database), "true", "{mode}");
    }

    let mut plain = database::connect(&reach("disable"), Some(&encrypter)).unwrap();
    assert_eq!(encrypted(&mut plain), "false");

    let full = Connection {
        ssl_ca: Some(lab.path("ca.pem")),
        ..reach("verify-full")
    };
    let mut verified = database::connect(&full, Some(&encrypter)).unwrap();
    assert_eq!(encrypted(&mut verified), "true");
    verified.use_database("template1").unwrap();
    assert_eq!(encrypted(&mut verified), "true");

    let by_name = Connection {
        host: Some("localhost".to_string()),
        ..full.clone()
    };
    assert_ne!(
        match database::connect(&by_name, Some(&encrypter)) {
            Ok(_) => "connected".to_string(),
            Err(error) => format!("{error:#}"),
        },
        "connected"
    );
    let mut name_unchecked = database::connect(
        &Connection {
            ssl_mode: Some("verify-ca".to_string()),
            ..by_name
        },
        Some(&encrypter),
    )
    .unwrap();
    assert_eq!(encrypted(&mut name_unchecked), "true");
}

struct SshClient {
    previous_path: String,
}

impl SshClient {
    fn isolated(lab: &Lab) -> SshClient {
        let bin = lab.directory.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let wrapper = bin.join("ssh");
        fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nexec /usr/bin/ssh -F /dev/null -o UserKnownHostsFile={known} -o IdentityAgent=none -o IdentitiesOnly=yes -o IdentityFile={nothing} \"$@\"\n",
                known = lab.path("known_hosts"),
                nothing = lab.path("no-default-key"),
            ),
        )
        .unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let previous_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{previous_path}", bin.display())) };
        SshClient { previous_path }
    }
}

impl Drop for SshClient {
    fn drop(&mut self) {
        unsafe { std::env::set_var("PATH", &self.previous_path) };
    }
}

#[test]
#[ignore = "starts throwaway mysql, postgres and sshd containers in docker"]
fn reaches_a_database_only_the_bastion_can_see() {
    let mut lab = Lab::new("ssh");
    let _client = SshClient::isolated(&lab);
    let encrypter = Encrypter::throwaway().unwrap();
    lab.network();
    let database_host = format!("{}-postgres", lab.prefix);
    postgres_server(&mut lab, &[&database_host], &[]);

    let generated = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "tql-test", "-f"])
        .arg(lab.path("id_test"))
        .status()
        .unwrap();
    assert!(generated.success());
    let authorized = lab.directory.join("authorized_keys");
    fs::copy(lab.directory.join("id_test.pub"), &authorized).unwrap();
    fs::set_permissions(&authorized, fs::Permissions::from_mode(0o644)).unwrap();
    let mounted = format!("{}:/keys:ro", authorized.display());

    let bastion = lab.run(
        "sshd",
        &[
            "-p",
            "127.0.0.1::22",
            "-v",
            &mounted,
            "alpine:latest",
            "sh",
            "-c",
            "apk add --no-cache openssh >/dev/null && ssh-keygen -A >/dev/null && adduser -D tunnel && echo tunnel:hunter2 | chpasswd && mkdir -p /home/tunnel/.ssh && cp /keys /home/tunnel/.ssh/authorized_keys && chown -R tunnel /home/tunnel/.ssh && chmod 600 /home/tunnel/.ssh/authorized_keys && exec /usr/sbin/sshd -D -e -o AllowTcpForwarding=yes -o PasswordAuthentication=yes",
        ],
    );
    let ssh_port = lab.port(&bastion, 22);

    let through = |mode: &str| Connection {
        ssh_host: Some("127.0.0.1".to_string()),
        ssh_port: Some(ssh_port),
        ssh_user: Some("tunnel".to_string()),
        ssh_key: Some(lab.path("id_test")),
        ssl_ca: Some(lab.path("ca.pem")),
        ..with_password(postgres(&database_host, 5432, mode), &encrypter)
    };

    let mut keyed = eventually(&through("verify-full"), Some(&encrypter));
    assert_eq!(encrypted(&mut keyed), "true");

    let shared_first = tunnel::for_connection(&through(""), None).unwrap();
    let shared_second = tunnel::for_connection(
        &Connection {
            name: "another name".to_string(),
            database: Some("template1".to_string()),
            ..through("disable")
        },
        None,
    )
    .unwrap();
    assert!(Arc::ptr_eq(&shared_first, &shared_second));
    let forwarded = shared_first.port;
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, forwarded)).is_ok());
    drop(keyed);
    drop(shared_first);
    drop(shared_second);
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, forwarded)).is_ok());
    tunnel::close_all();
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, forwarded)).is_err());

    let by_password = Connection {
        ssh_key: None,
        ssh_password: Some(encrypter.encrypt_string("hunter2").unwrap()),
        ..through("require")
    };
    let mut passworded = database::connect(&by_password, Some(&encrypter)).unwrap();
    assert_eq!(encrypted(&mut passworded), "true");
    drop(passworded);
    tunnel::close_all();

    let refused = Connection {
        ssh_password: Some(encrypter.encrypt_string("wrong").unwrap()),
        ..by_password
    };
    let error = match database::connect(&refused, Some(&encrypter)) {
        Ok(_) => "connected".to_string(),
        Err(error) => format!("{error:#}"),
    };
    assert!(error.starts_with("ssh: "), "{error}");
    assert!(error.contains("Permission denied"), "{error}");

    let leftovers: Vec<_> = fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("tql-askpass-")
        })
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
#[ignore = "starts throwaway mysql, postgres and sshd containers in docker"]
fn clickhouse_speaks_tls_as_far_as_the_mode_asks() {
    let mut lab = Lab::new("ch-tls");
    let encrypter = Encrypter::throwaway().unwrap();
    Authority::new("tql test ca").sign_into(&lab.directory, &["127.0.0.1"]);
    fs::write(
        lab.directory.join("tls.xml"),
        "<clickhouse><https_port>8443</https_port><openSSL><server>\
         <certificateFile>/certs/server.pem</certificateFile>\
         <privateKeyFile>/certs/server.key</privateKeyFile>\
         <verificationMode>none</verificationMode></server></openSSL></clickhouse>",
    )
    .unwrap();
    let certificates = format!("{}:/certs:ro", lab.directory.display());
    let config = format!(
        "{}:/etc/clickhouse-server/config.d/tls.xml:ro",
        lab.path("tls.xml")
    );
    let container = lab.run(
        "clickhouse",
        &[
            "-e",
            "CLICKHOUSE_USER=tql",
            "-e",
            "CLICKHOUSE_PASSWORD=secret",
            "-v",
            &certificates,
            "-v",
            &config,
            "-p",
            "127.0.0.1::8443",
            "clickhouse/clickhouse-server:latest",
        ],
    );
    let port = lab.port(&container, 8443);
    let reach = |host: &str, mode: &str, ca: bool| Connection {
        name: "clickhouse".to_string(),
        driver: Driver::Clickhouse,
        host: Some(host.to_string()),
        port: Some(port),
        username: Some("tql".to_string()),
        password: Some(encrypter.encrypt_string("secret").unwrap()),
        ssl_mode: Some(mode.to_string()),
        ssl_ca: ca.then(|| lab.path("ca.pem")),
        ..Connection::default()
    };
    let refused = |connection: &Connection| match database::connect(connection, Some(&encrypter)) {
        Ok(_) => "connected".to_string(),
        Err(error) => format!("{error:#}"),
    };

    let mut required = eventually(&reach("127.0.0.1", "require", false), Some(&encrypter));
    assert_eq!(
        required.run("select 1").unwrap().rows,
        [[Some("1".to_string())]]
    );
    assert!(database::connect(&reach("127.0.0.1", "verify-full", true), Some(&encrypter)).is_ok());
    assert!(database::connect(&reach("localhost", "verify-ca", true), Some(&encrypter)).is_ok());
    assert_ne!(
        refused(&reach("localhost", "verify-full", true)),
        "connected"
    );
    assert_ne!(
        refused(&reach("127.0.0.1", "verify-full", false)),
        "connected"
    );
}

#[test]
#[ignore = "starts throwaway mysql, postgres and sshd containers in docker"]
fn redis_speaks_tls_as_far_as_the_mode_asks() {
    let mut lab = Lab::new("redis-tls");
    Authority::new("tql test ca").sign_into(&lab.directory, &["127.0.0.1"]);
    let certificates = format!("{}:/certs:ro", lab.directory.display());
    let container = lab.run(
        "redis",
        &[
            "-v",
            &certificates,
            "-p",
            "127.0.0.1::6379",
            "redis:7",
            "redis-server",
            "--port",
            "0",
            "--tls-port",
            "6379",
            "--tls-cert-file",
            "/certs/server.pem",
            "--tls-key-file",
            "/certs/server.key",
            "--tls-ca-cert-file",
            "/certs/ca.pem",
            "--tls-auth-clients",
            "no",
        ],
    );
    let port = lab.port(&container, 6379);
    let reach = |host: &str, mode: &str, ca: bool| Connection {
        name: "redis".to_string(),
        driver: Driver::Redis,
        host: Some(host.to_string()),
        port: Some(port),
        ssl_mode: Some(mode.to_string()),
        ssl_ca: ca.then(|| lab.path("ca.pem")),
        ..Connection::default()
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut required = loop {
        match app::keystore::connect(&reach("127.0.0.1", "require", false), None) {
            Ok(store) => break store,
            Err(error) if Instant::now() > deadline => panic!("never came up: {error:#}"),
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    };
    assert_eq!(
        required.run("PING").unwrap(),
        app::keystore::Reply::Text("PONG".to_string())
    );
    assert!(app::keystore::connect(&reach("127.0.0.1", "verify-full", true), None).is_ok());
    assert!(app::keystore::connect(&reach("127.0.0.1", "verify-full", false), None).is_err());
    assert!(app::keystore::connect(&reach("127.0.0.1", "disable", false), None).is_err());
}
