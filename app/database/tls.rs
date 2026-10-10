use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use mysql::{ClientIdentity, SslOpts};
use postgres::config::SslMode;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, ring};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};

use crate::paths;
use crate::store::Connection;

pub fn choose_ring() {
    let _ = ring::default_provider().install_default();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Unset,
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl Mode {
    fn named(name: &str) -> anyhow::Result<Mode> {
        Ok(match name {
            "" => Mode::Unset,
            "disable" => Mode::Disable,
            "prefer" => Mode::Prefer,
            "require" => Mode::Require,
            "verify-ca" => Mode::VerifyCa,
            "verify-full" => Mode::VerifyFull,
            other => bail!(
                "invalid SSL mode \"{other}\"; use disable, prefer, require, verify-ca or verify-full"
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tls {
    pub mode: Mode,
    pub ca: Option<PathBuf>,
    pub identity: Option<(PathBuf, PathBuf)>,
}

impl Tls {
    pub fn of(connection: &Connection) -> anyhow::Result<Tls> {
        Self::from_home(connection, &paths::home())
    }

    pub fn from_home(connection: &Connection, home: &Path) -> anyhow::Result<Tls> {
        let mode = match connection.driver.as_str() {
            "sqlite" => Mode::Unset,
            _ => Mode::named(connection.ssl_mode.as_deref().unwrap_or_default().trim())?,
        };
        if matches!(mode, Mode::Unset | Mode::Disable) {
            return Ok(Tls {
                mode,
                ca: None,
                identity: None,
            });
        }

        let file = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(|path| PathBuf::from(paths::expand_from(path, home)))
        };
        let identity = match (file(&connection.ssl_cert), file(&connection.ssl_key)) {
            (Some(certificate), Some(key)) => Some((certificate, key)),
            (None, None) => None,
            (Some(_), None) => bail!("an SSL cert needs its SSL key; set both or neither"),
            (None, Some(_)) => bail!("an SSL key needs its SSL cert; set both or neither"),
        };
        let mut ca = file(&connection.ssl_ca);
        let mut identity = identity;
        if connection.driver == "pgsql" {
            let libpq = home.join(".postgresql");
            let existing = |name: &str| Some(libpq.join(name)).filter(|path| path.is_file());
            ca = ca.or_else(|| existing("root.crt"));
            identity =
                identity.or_else(|| existing("postgresql.crt").zip(existing("postgresql.key")));
        }
        Ok(Tls { mode, ca, identity })
    }

    pub fn checks_chain(&self) -> bool {
        match self.mode {
            Mode::VerifyCa | Mode::VerifyFull => true,
            Mode::Require => self.ca.is_some(),
            Mode::Unset | Mode::Disable | Mode::Prefer => false,
        }
    }

    pub fn checks_hostname(&self) -> bool {
        self.mode == Mode::VerifyFull
    }

    pub fn postgres_mode(&self) -> SslMode {
        match self.mode {
            Mode::Disable => SslMode::Disable,
            Mode::Unset | Mode::Prefer => SslMode::Prefer,
            Mode::Require | Mode::VerifyCa | Mode::VerifyFull => SslMode::Require,
        }
    }

    pub fn mysql_options(&self) -> Option<SslOpts> {
        if matches!(self.mode, Mode::Unset | Mode::Disable) {
            return None;
        }
        Some(
            SslOpts::default()
                .with_root_cert_path(self.ca.clone().filter(|_| self.checks_chain()))
                .with_client_identity(
                    self.identity
                        .clone()
                        .map(|(certificate, key)| ClientIdentity::new(certificate, key)),
                )
                .with_danger_accept_invalid_certs(!self.checks_chain())
                .with_danger_skip_domain_validation(!self.checks_hostname()),
        )
    }

    pub fn client_config(&self) -> anyhow::Result<ClientConfig> {
        let provider = Arc::new(ring::default_provider());
        let builder = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .context("the TLS library refused its own defaults")?
            .dangerous()
            .with_custom_certificate_verifier(self.verifier(&provider)?);
        match &self.identity {
            None => Ok(builder.with_no_client_auth()),
            Some((certificate, key)) => builder
                .with_client_auth_cert(certificates_in(certificate)?, private_key_in(key)?)
                .context("the SSL cert and SSL key do not belong together"),
        }
    }

    fn verifier(
        &self,
        provider: &Arc<CryptoProvider>,
    ) -> anyhow::Result<Arc<dyn ServerCertVerifier>> {
        if !self.checks_chain() {
            return Ok(Arc::new(Unverified {
                algorithms: provider.signature_verification_algorithms,
            }));
        }

        let mut roots = RootCertStore::empty();
        match &self.ca {
            Some(ca) => {
                for certificate in certificates_in(ca)? {
                    roots.add(certificate).with_context(|| {
                        format!("the SSL CA file {} is not a certificate", ca.display())
                    })?;
                }
            }
            None => roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        }
        let chain = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
            .build()
            .context("the SSL CA file holds no usable certificate")?;
        Ok(Arc::new(Verified {
            chain,
            hostname: self.checks_hostname(),
        }))
    }
}

fn certificates_in(path: &Path) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    let unreadable = || format!("cannot read the certificate file {}", path.display());
    let certificates = CertificateDer::pem_file_iter(path)
        .with_context(unreadable)?
        .collect::<Result<Vec<_>, _>>()
        .with_context(unreadable)?;
    if certificates.is_empty() {
        bail!("{} holds no PEM certificate", path.display());
    }
    Ok(certificates)
}

fn private_key_in(path: &Path) -> anyhow::Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path)
        .with_context(|| format!("cannot read a PEM private key from {}", path.display()))
}

#[derive(Debug)]
struct Unverified {
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Unverified {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, certificate, signature, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, certificate, signature, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[derive(Debug)]
struct Verified {
    chain: Arc<WebPkiServerVerifier>,
    hostname: bool,
}

impl ServerCertVerifier for Verified {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let verdict = self.chain.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        match verdict {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) if !self.hostname => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.chain
            .verify_tls12_signature(message, certificate, signature)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.chain
            .verify_tls13_signature(message, certificate, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.chain.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_uses_libpqs_files_in_home_when_the_connection_names_none() {
        let home = std::env::temp_dir().join(format!("tql-pg-home-{}", std::process::id()));
        let folder = home.join(".postgresql");
        std::fs::create_dir_all(&folder).unwrap();
        for file in ["root.crt", "postgresql.crt", "postgresql.key"] {
            std::fs::write(folder.join(file), "").unwrap();
        }
        let postgres = Connection {
            driver: "pgsql".to_string(),
            ..server("require")
        };

        let found = Tls::from_home(&postgres, &home).unwrap();
        let mysql = Tls::from_home(&server("require"), &home).unwrap();
        std::fs::remove_dir_all(&home).unwrap();

        assert_eq!(found.ca, Some(folder.join("root.crt")));
        assert_eq!(
            found.identity,
            Some((folder.join("postgresql.crt"), folder.join("postgresql.key")))
        );
        assert!(found.checks_chain());
        assert_eq!(mysql.ca, None);
    }

    fn server(mode: &str) -> Connection {
        Connection {
            name: "cloud".to_string(),
            driver: "mysql".to_string(),
            host: Some("db.example.com".to_string()),
            ssl_mode: Some(mode.to_string()),
            ..Connection::default()
        }
    }

    fn with_files(mode: &str) -> Connection {
        Connection {
            ssl_ca: Some("~/certs/ca.pem".to_string()),
            ssl_cert: Some("~/certs/client.pem".to_string()),
            ssl_key: Some("/etc/ssl/client.key".to_string()),
            ..server(mode)
        }
    }

    fn tls(connection: &Connection) -> Tls {
        Tls::from_home(connection, Path::new("/home/karl")).unwrap()
    }

    #[test]
    fn reads_every_mode_tql_offers() {
        for (name, mode) in [
            ("", Mode::Unset),
            ("  ", Mode::Unset),
            ("disable", Mode::Disable),
            ("prefer", Mode::Prefer),
            ("require", Mode::Require),
            ("verify-ca", Mode::VerifyCa),
            ("verify-full", Mode::VerifyFull),
        ] {
            assert_eq!(tls(&server(name)).mode, mode, "{name}");
        }
        assert_eq!(
            tls(&Connection {
                ssl_mode: None,
                ..server("")
            })
            .mode,
            Mode::Unset
        );
    }

    #[test]
    fn refuses_a_mode_it_does_not_know() {
        let error = Tls::from_home(&server("allow-anything"), Path::new("/")).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("invalid SSL mode \"allow-anything\"")
        );
    }

    #[test]
    fn never_uses_tls_on_a_sqlite_file() {
        let file = Connection {
            driver: "sqlite".to_string(),
            ..with_files("verify-full")
        };

        assert_eq!(tls(&file).mode, Mode::Unset);
        assert_eq!(tls(&file).mysql_options(), None);
    }

    #[test]
    fn ignores_the_certificate_files_until_a_mode_asks_for_tls() {
        for mode in ["", "disable"] {
            let settings = tls(&with_files(mode));

            assert_eq!(settings.ca, None);
            assert_eq!(settings.identity, None);
            assert_eq!(settings.mysql_options(), None);
        }
    }

    #[test]
    fn expands_the_certificate_paths_from_home() {
        let settings = tls(&with_files("require"));

        assert_eq!(settings.ca, Some(PathBuf::from("/home/karl/certs/ca.pem")));
        assert_eq!(
            settings.identity,
            Some((
                PathBuf::from("/home/karl/certs/client.pem"),
                PathBuf::from("/etc/ssl/client.key")
            ))
        );
    }

    #[test]
    fn wants_a_cert_and_its_key_together() {
        let without_key = Connection {
            ssl_key: Some(" ".to_string()),
            ..with_files("require")
        };
        let without_cert = Connection {
            ssl_cert: None,
            ..with_files("require")
        };

        assert!(
            Tls::from_home(&without_key, Path::new("/"))
                .unwrap_err()
                .to_string()
                .contains("needs its SSL key")
        );
        assert!(
            Tls::from_home(&without_cert, Path::new("/"))
                .unwrap_err()
                .to_string()
                .contains("needs its SSL cert")
        );
    }

    #[test]
    fn only_verify_full_checks_the_hostname() {
        for (mode, chain, hostname) in [
            ("prefer", false, false),
            ("require", false, false),
            ("verify-ca", true, false),
            ("verify-full", true, true),
        ] {
            let settings = tls(&server(mode));

            assert_eq!(settings.checks_chain(), chain, "{mode}");
            assert_eq!(settings.checks_hostname(), hostname, "{mode}");
        }
    }

    #[test]
    fn require_with_a_ca_checks_the_certificate_against_it() {
        assert!(tls(&with_files("require")).checks_chain());
        assert!(!tls(&with_files("require")).checks_hostname());
        assert!(!tls(&with_files("prefer")).checks_chain());
    }

    #[test]
    fn asks_postgres_for_tls_the_way_libpq_would() {
        for (mode, wanted) in [
            ("", SslMode::Prefer),
            ("disable", SslMode::Disable),
            ("prefer", SslMode::Prefer),
            ("require", SslMode::Require),
            ("verify-ca", SslMode::Require),
            ("verify-full", SslMode::Require),
        ] {
            assert_eq!(
                tls(&Connection {
                    driver: "pgsql".to_string(),
                    ..server(mode)
                })
                .postgres_mode(),
                wanted,
                "{mode}"
            );
        }
    }

    #[test]
    fn tells_mysql_how_far_to_trust_the_server() {
        let require = tls(&server("require")).mysql_options().unwrap();
        assert!(require.accept_invalid_certs());
        assert!(require.skip_domain_validation());
        assert_eq!(require.root_cert_path(), None);

        let verify_ca = tls(&with_files("verify-ca")).mysql_options().unwrap();
        assert!(!verify_ca.accept_invalid_certs());
        assert!(verify_ca.skip_domain_validation());
        assert_eq!(
            verify_ca.root_cert_path(),
            Some(Path::new("/home/karl/certs/ca.pem"))
        );

        let verify_full = tls(&server("verify-full")).mysql_options().unwrap();
        assert!(!verify_full.accept_invalid_certs());
        assert!(!verify_full.skip_domain_validation());
        assert_eq!(verify_full.root_cert_path(), None);
    }

    #[test]
    fn hands_mysql_the_client_certificate() {
        let options = tls(&with_files("verify-full")).mysql_options().unwrap();
        let identity = options.client_identity().unwrap();

        assert_eq!(
            identity.cert_chain_path(),
            Path::new("/home/karl/certs/client.pem")
        );
        assert_eq!(identity.priv_key_path(), Path::new("/etc/ssl/client.key"));
    }

    #[test]
    fn says_which_certificate_file_it_could_not_read() {
        let error = tls(&Connection {
            ssl_ca: Some("/nowhere/ca.pem".to_string()),
            ..server("verify-ca")
        })
        .client_config()
        .unwrap_err();

        assert!(format!("{error:#}").contains("cannot read the certificate file /nowhere/ca.pem"));
    }

    #[test]
    fn trusts_the_public_roots_when_no_ca_is_given() {
        assert!(tls(&server("verify-full")).client_config().is_ok());
        assert!(tls(&server("require")).client_config().is_ok());
    }
}
