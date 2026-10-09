use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::dsn;
use crate::paths;
use crate::store::Connection;

pub const DEFAULT_PORT: u16 = 22;

const DID_NOT_COME_UP: &str = "the ssh tunnel did not come up";

const WAIT: Duration = Duration::from_secs(10);

// ssh that answers on the port must still be running a moment later, so a
// port someone else holds is not taken for the tunnel while ssh fails.
const SETTLE: Duration = Duration::from_millis(150);

static OPEN: LazyLock<Mutex<HashMap<String, Arc<Tunnel>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct Tunnel {
    pub port: u16,
    process: Mutex<Child>,
}

pub fn for_connection(
    connection: &Connection,
    password: Option<String>,
) -> anyhow::Result<Arc<Tunnel>> {
    let key = key_for(connection);
    if let Some(tunnel) = open().get(&key).filter(|tunnel| tunnel.running()).cloned() {
        return Ok(tunnel);
    }

    let port = free_port();
    let tunnel = Arc::new(Tunnel::start(
        &command(connection, port, &paths::home()),
        password.as_deref(),
        port,
        WAIT,
    )?);
    keep(key, Arc::clone(&tunnel));
    Ok(tunnel)
}

fn open() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Tunnel>>> {
    OPEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn keep(key: String, tunnel: Arc<Tunnel>) {
    open().insert(key, tunnel);
}

pub fn close_all() {
    let tunnels: Vec<Arc<Tunnel>> = open().drain().map(|(_, tunnel)| tunnel).collect();
    tunnels.iter().for_each(|tunnel| tunnel.close());
}

pub struct CloseAtExit;

impl Drop for CloseAtExit {
    fn drop(&mut self) {
        close_all();
    }
}

pub fn key_for(connection: &Connection) -> String {
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    let number = |value: Option<u16>| value.map(|port| port.to_string()).unwrap_or_default();
    [
        text(&connection.ssh_user),
        text(&connection.ssh_host),
        number(connection.ssh_port),
        text(&connection.host),
        number(connection.port),
    ]
    .join("|")
}

pub fn command(connection: &Connection, port: u16, home: &Path) -> Vec<String> {
    let destination = format!(
        "{port}:{}:{}",
        connection
            .host
            .as_deref()
            .filter(|host| !host.trim().is_empty())
            .unwrap_or("127.0.0.1"),
        connection
            .port
            .unwrap_or_else(|| dsn::default_port(&connection.driver))
    );
    let mut command: Vec<String> = [
        "ssh",
        "-N",
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ServerAliveInterval=30",
        "-L",
        &destination,
    ]
    .map(String::from)
    .into();

    if let Some(key) = connection
        .ssh_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        command.extend(["-i".to_string(), paths::expand_from(key, home)]);
    }

    if let Some(ssh_port) = connection
        .ssh_port
        .filter(|ssh_port| *ssh_port != DEFAULT_PORT)
    {
        command.extend(["-p".to_string(), ssh_port.to_string()]);
    }

    let host = connection.ssh_host.clone().unwrap_or_default();
    command.push(
        match connection
            .ssh_user
            .as_deref()
            .filter(|user| !user.is_empty())
        {
            Some(user) => format!("{user}@{host}"),
            None => host,
        },
    );
    command
}

pub fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .unwrap_or_else(|_| 30000 + (std::process::id() % 30000) as u16)
}

impl Tunnel {
    pub fn start(
        command: &[String],
        password: Option<&str>,
        port: u16,
        wait: Duration,
    ) -> anyhow::Result<Tunnel> {
        let askpass = password
            .filter(|password| !password.is_empty())
            .map(Askpass::write)
            .transpose()?;

        let mut process = Command::new(&command[0]);
        process
            .args(&command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(askpass) = &askpass {
            process.envs(askpass.environment());
        }
        let mut child = process
            .spawn()
            .map_err(|error| anyhow::anyhow!("ssh: {error}"))?;

        let said = collect(child.stderr.take());
        let tunnel = Tunnel {
            port,
            process: Mutex::new(child),
        };
        let listening = tunnel.wait_until_listening(wait);
        drop(askpass);
        if listening {
            return Ok(tunnel);
        }

        tunnel.close();
        match said
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_default()
            .trim()
        {
            "" => anyhow::bail!(DID_NOT_COME_UP),
            error => anyhow::bail!("ssh: {error}"),
        }
    }

    pub fn running(&self) -> bool {
        self.process
            .lock()
            .map(|mut child| matches!(child.try_wait(), Ok(None)))
            .unwrap_or(false)
    }

    fn close(&self) {
        let Ok(mut child) = self.process.lock() else {
            return;
        };
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
    }

    fn wait_until_listening(&self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        while Instant::now() < deadline {
            if !self.running() {
                return false;
            }
            if TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok() {
                thread::sleep(SETTLE);
                return self.running();
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.close();
    }
}

fn collect(stream: Option<impl Read + Send + 'static>) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    let Some(mut stream) = stream else {
        let _ = sender.send(String::new());
        return receiver;
    };
    thread::spawn(move || {
        let mut said = String::new();
        let _ = stream.read_to_string(&mut said);
        let _ = sender.send(said);
    });
    receiver
}

pub struct Askpass {
    directory: PathBuf,
}

impl Askpass {
    pub fn write(password: &str) -> io::Result<Askpass> {
        let mut random = [0u8; 6];
        getrandom::fill(&mut random).map_err(io::Error::other)?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let directory = env::temp_dir().join(format!("tql-askpass-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;

        let askpass = Askpass { directory };
        let secret = askpass.secret();
        fs::write(&secret, password)?;
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600))?;

        let quoted = secret.display().to_string().replace('\'', "'\\''");
        fs::write(askpass.script(), format!("#!/bin/sh\ncat '{quoted}'\n"))?;
        fs::set_permissions(askpass.script(), fs::Permissions::from_mode(0o700))?;
        Ok(askpass)
    }

    pub fn script(&self) -> PathBuf {
        self.directory.join("askpass")
    }

    pub fn secret(&self) -> PathBuf {
        self.directory.join("secret")
    }

    pub fn environment(&self) -> Vec<(String, String)> {
        vec![
            (
                "SSH_ASKPASS".to_string(),
                self.script().display().to_string(),
            ),
            ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
            (
                "DISPLAY".to_string(),
                env::var("DISPLAY")
                    .ok()
                    .filter(|display| !display.is_empty())
                    .unwrap_or_else(|| ":0".to_string()),
            ),
        ]
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.secret());
        let _ = fs::remove_file(self.script());
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tunnelled() -> Connection {
        Connection {
            name: "prod".to_string(),
            driver: "mysql".to_string(),
            host: Some("db.internal".to_string()),
            port: Some(3306),
            database: Some("shop".to_string()),
            username: Some("alice".to_string()),
            ssh_host: Some("bastion.example.com".to_string()),
            ssh_user: Some("karl".to_string()),
            ..Connection::default()
        }
    }

    fn home() -> PathBuf {
        PathBuf::from("/home/karl")
    }

    fn scratch(name: &str) -> PathBuf {
        let directory =
            env::temp_dir().join(format!("tql-rust-tunnel-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    // These tests pick a free port and bind it a moment later, so two of them
    // at once can pick the same one.
    static PORTS: Mutex<()> = Mutex::new(());

    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        PORTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn fake_ssh(directory: &Path, body: &str) -> String {
        let script = directory.join("ssh");
        fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        script.display().to_string()
    }

    #[test]
    fn builds_an_ssh_command_that_forwards_the_database_port() {
        let command = command(&tunnelled(), 54321, &home());

        assert_eq!(
            command,
            [
                "ssh",
                "-N",
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "ServerAliveInterval=30",
                "-L",
                "54321:db.internal:3306",
                "karl@bastion.example.com",
            ]
        );
    }

    #[test]
    fn passes_a_key_file_and_a_port_when_they_are_set() {
        let connection = Connection {
            ssh_key: Some("~/.ssh/id_prod".to_string()),
            ssh_port: Some(2222),
            ..tunnelled()
        };

        let command = command(&connection, 54321, &home());

        assert!(
            command
                .windows(2)
                .any(|pair| pair == ["-i", "/home/karl/.ssh/id_prod"])
        );
        assert!(command.windows(2).any(|pair| pair == ["-p", "2222"]));
        assert!(!command.contains(&"~/.ssh/id_prod".to_string()));
    }

    #[test]
    fn leaves_out_the_port_when_it_is_the_default_and_the_user_when_there_is_none() {
        let connection = Connection {
            ssh_port: Some(DEFAULT_PORT),
            ssh_user: None,
            ..tunnelled()
        };

        let command = command(&connection, 1234, &home());

        assert!(!command.contains(&"-p".to_string()));
        assert!(!command.contains(&"-i".to_string()));
        assert_eq!(command.last().unwrap(), "bastion.example.com");
    }

    #[test]
    fn forwards_to_the_drivers_own_port_and_localhost_when_they_are_not_set() {
        let connection = Connection {
            driver: "pgsql".to_string(),
            host: None,
            port: None,
            ..tunnelled()
        };

        assert!(command(&connection, 1234, &home()).contains(&"1234:127.0.0.1:5432".to_string()));
    }

    #[test]
    fn asks_the_system_for_a_port_that_is_free() {
        let _ports = one_at_a_time();
        let port = free_port();

        assert!(port > 1024);
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok());
    }

    #[test]
    fn reuses_one_tunnel_per_destination() {
        let second = Connection {
            name: "second".to_string(),
            database: Some("other".to_string()),
            ..tunnelled()
        };
        let elsewhere = Connection {
            host: Some("other.internal".to_string()),
            ..tunnelled()
        };

        assert_eq!(key_for(&tunnelled()), key_for(&second));
        assert_ne!(key_for(&tunnelled()), key_for(&elsewhere));
    }

    #[test]
    fn writes_an_askpass_helper_the_user_alone_can_read_and_forgets_it() {
        let askpass = Askpass::write("hunter2").unwrap();
        let environment: HashMap<String, String> = askpass.environment().into_iter().collect();
        let secret = askpass.secret();

        assert_eq!(
            environment["SSH_ASKPASS"],
            askpass.script().display().to_string()
        );
        assert_eq!(environment["SSH_ASKPASS_REQUIRE"], "force");
        assert_eq!(fs::read_to_string(&secret).unwrap(), "hunter2");
        assert_eq!(
            fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::process::Command::new(askpass.script())
                .output()
                .unwrap()
                .stdout,
            b"hunter2"
        );

        drop(askpass);

        assert!(!secret.exists());
        assert!(!secret.parent().unwrap().exists());
    }

    #[test]
    fn hands_the_password_to_ssh_through_askpass_only_when_there_is_one() {
        let _ports = one_at_a_time();
        let directory = scratch("askpass");
        let seen = directory.join("seen");
        let program = fake_ssh(
            &directory,
            &format!(
                "echo \"${{SSH_ASKPASS_REQUIRE:-none}} $(\"${{SSH_ASKPASS:-true}}\")\" > '{}'; exit 1",
                seen.display()
            ),
        );

        let _ = Tunnel::start(
            std::slice::from_ref(&program),
            Some("hunter2"),
            free_port(),
            WAIT,
        );
        assert_eq!(fs::read_to_string(&seen).unwrap(), "force hunter2\n");

        let _ = Tunnel::start(&[program], None, free_port(), WAIT);
        assert_eq!(fs::read_to_string(&seen).unwrap(), "none \n");

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn says_what_ssh_said_when_it_gives_up() {
        let _ports = one_at_a_time();
        let directory = scratch("refused");
        let program = fake_ssh(
            &directory,
            "echo 'karl@bastion.example.com: Permission denied (publickey).' >&2; exit 255",
        );

        let error = Tunnel::start(&[program], None, free_port(), WAIT)
            .err()
            .unwrap();

        assert_eq!(
            error.to_string(),
            "ssh: karl@bastion.example.com: Permission denied (publickey)."
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn says_the_tunnel_did_not_come_up_when_ssh_is_silent() {
        let _ports = one_at_a_time();
        let directory = scratch("silent");
        let program = fake_ssh(&directory, "exit 1");

        let error = Tunnel::start(&[program], None, free_port(), WAIT)
            .err()
            .unwrap();

        assert_eq!(error.to_string(), DID_NOT_COME_UP);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn gives_up_on_a_tunnel_that_never_listens_and_stops_it() {
        let _ports = one_at_a_time();
        let directory = scratch("never");
        let pid = directory.join("pid");
        let program = fake_ssh(
            &directory,
            &format!("echo $$ > '{}'; exec sleep 30", pid.display()),
        );

        let started = Instant::now();
        let error = Tunnel::start(&[program], None, free_port(), Duration::from_millis(600))
            .err()
            .unwrap();

        assert_eq!(error.to_string(), DID_NOT_COME_UP);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!alive(fs::read_to_string(&pid).unwrap().trim()));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn waits_for_the_forwarded_port_and_stops_ssh_when_dropped() {
        let _ports = one_at_a_time();
        let directory = scratch("listening");
        let port = free_port();
        let program = fake_ssh(
            &directory,
            &format!(
                "sleep 0.3; exec python3 -c \"import socket,time; listener=socket.socket(); listener.bind(('127.0.0.1', {port})); listener.listen(); time.sleep(30)\""
            ),
        );

        let tunnel = Tunnel::start(&[program], None, port, WAIT).unwrap();

        assert!(tunnel.running());
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok());

        drop(tunnel);

        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn keeps_a_tunnel_open_until_tql_closes_them_all() {
        let _ports = one_at_a_time();
        let directory = scratch("kept");
        let port = free_port();
        let program = fake_ssh(
            &directory,
            &format!(
                "exec python3 -c \"import socket,time; listener=socket.socket(); listener.bind(('127.0.0.1', {port})); listener.listen(); time.sleep(30)\""
            ),
        );
        let tunnel = Arc::new(Tunnel::start(&[program], None, port, WAIT).unwrap());

        keep(format!("kept-{port}"), Arc::clone(&tunnel));
        drop(tunnel);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok());

        drop(CloseAtExit);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}
