use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use app::database::tls::Tls;
use app::store::Connection;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned};

struct Authority {
    issuer: Issuer<'static, KeyPair>,
    pem: String,
}

impl Authority {
    fn new() -> Authority {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let key = KeyPair::generate().unwrap();
        let pem = params.self_signed(&key).unwrap().pem();
        Authority {
            issuer: Issuer::new(params, key),
            pem,
        }
    }

    fn sign(&self, names: &[&str]) -> (String, String) {
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
        (certificate.pem(), key.serialize_pem())
    }
}

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!("tql-rust-tls-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Scratch { root }
    }

    fn write(&self, name: &str, contents: &str) -> String {
        let path = self.root.join(name);
        fs::write(&path, contents).unwrap();
        path.display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn certificates(pem: &str) -> Vec<CertificateDer<'static>> {
    CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<_, _>>()
        .unwrap()
}

fn serve(certificate: &str, key: &str, client_authority: Option<&str>) -> u16 {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap();
    let builder = match client_authority {
        None => builder.with_no_client_auth(),
        Some(authority) => {
            let mut roots = RootCertStore::empty();
            roots.add_parsable_certificates(certificates(authority));
            builder.with_client_cert_verifier(
                WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
                    .build()
                    .unwrap(),
            )
        }
    };
    let config = Arc::new(
        builder
            .with_single_cert(
                certificates(certificate),
                PrivateKeyDer::from_pem_slice(key.as_bytes()).unwrap(),
            )
            .unwrap(),
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        let mut stream = StreamOwned::new(ServerConnection::new(config).unwrap(), socket);
        while stream.conn.is_handshaking() {
            if stream.conn.complete_io(&mut stream.sock).is_err() {
                return;
            }
        }
    });
    port
}

fn handshake(settings: &Tls, port: u16, name: &str) -> Result<(), String> {
    let mut connection = ClientConnection::new(
        Arc::new(
            settings
                .client_config()
                .map_err(|error| format!("{error:#}"))?,
        ),
        ServerName::try_from(name.to_string()).unwrap(),
    )
    .unwrap();
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    while connection.is_handshaking() {
        connection
            .complete_io(&mut socket)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn settings(mode: &str, files: &[(&str, &str)]) -> Tls {
    let file = |field: &str| {
        files
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, path)| path.to_string())
    };
    Tls::from_home(
        &Connection {
            driver: "pgsql".to_string(),
            ssl_mode: Some(mode.to_string()),
            ssl_ca: file("ca"),
            ssl_cert: file("cert"),
            ssl_key: file("key"),
            ..Connection::default()
        },
        Path::new("/"),
    )
    .unwrap()
}

#[test]
fn verify_full_trusts_a_server_its_ca_signed_for_that_name() {
    let scratch = Scratch::new("full");
    let authority = Authority::new();
    let (certificate, key) = authority.sign(&["db.test"]);
    let ca = scratch.write("ca.pem", &authority.pem);

    let port = serve(&certificate, &key, None);

    assert_eq!(
        handshake(&settings("verify-full", &[("ca", &ca)]), port, "db.test"),
        Ok(())
    );
}

#[test]
fn verify_full_refuses_a_certificate_for_another_name() {
    let scratch = Scratch::new("wrong-name");
    let authority = Authority::new();
    let (certificate, key) = authority.sign(&["elsewhere.test"]);
    let ca = scratch.write("ca.pem", &authority.pem);

    let port = serve(&certificate, &key, None);

    assert!(
        handshake(&settings("verify-full", &[("ca", &ca)]), port, "db.test")
            .unwrap_err()
            .contains("not valid for name")
    );
}

#[test]
fn verify_ca_checks_the_ca_but_not_the_name() {
    let scratch = Scratch::new("ca-only");
    let authority = Authority::new();
    let (certificate, key) = authority.sign(&["elsewhere.test"]);
    let ca = scratch.write("ca.pem", &authority.pem);

    let port = serve(&certificate, &key, None);

    assert_eq!(
        handshake(&settings("verify-ca", &[("ca", &ca)]), port, "db.test"),
        Ok(())
    );
}

#[test]
fn verify_ca_refuses_a_server_another_ca_signed() {
    let scratch = Scratch::new("other-ca");
    let (certificate, key) = Authority::new().sign(&["db.test"]);
    let ca = scratch.write("ca.pem", &Authority::new().pem);

    let port = serve(&certificate, &key, None);

    assert!(
        handshake(&settings("verify-ca", &[("ca", &ca)]), port, "db.test")
            .unwrap_err()
            .contains("invalid peer certificate")
    );
}

#[test]
fn verify_full_without_a_ca_trusts_only_the_public_roots() {
    let (certificate, key) = Authority::new().sign(&["db.test"]);

    let port = serve(&certificate, &key, None);

    assert!(handshake(&settings("verify-full", &[]), port, "db.test").is_err());
}

#[test]
fn require_and_prefer_encrypt_without_checking_who_signed() {
    for mode in ["require", "prefer", ""] {
        let (certificate, key) = Authority::new().sign(&["elsewhere.test"]);

        let port = serve(&certificate, &key, None);

        assert_eq!(
            handshake(&settings(mode, &[]), port, "db.test"),
            Ok(()),
            "{mode}"
        );
    }
}

#[test]
fn presents_the_client_certificate_a_server_asks_for() {
    let scratch = Scratch::new("client");
    let authority = Authority::new();
    let (server_certificate, server_key) = authority.sign(&["db.test"]);
    let (client_certificate, client_key) = authority.sign(&["tql"]);
    let ca = scratch.write("ca.pem", &authority.pem);
    let cert = scratch.write("client.pem", &client_certificate);
    let key = scratch.write("client.key", &client_key);

    let port = serve(&server_certificate, &server_key, Some(&authority.pem));

    assert_eq!(
        handshake(
            &settings(
                "verify-full",
                &[("ca", &ca), ("cert", &cert), ("key", &key)]
            ),
            port,
            "db.test"
        ),
        Ok(())
    );
}

#[test]
fn says_when_a_key_file_holds_no_key() {
    let scratch = Scratch::new("bad-key");
    let authority = Authority::new();
    let (client_certificate, _) = authority.sign(&["tql"]);
    let cert = scratch.write("client.pem", &client_certificate);
    let key = scratch.write("client.key", "not a key");

    let error = settings("require", &[("cert", &cert), ("key", &key)])
        .client_config()
        .unwrap_err();

    assert!(format!("{error:#}").contains("cannot read a PEM private key from"));
}
