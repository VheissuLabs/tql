use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use pkcs8::der::asn1::BitStringRef;
use pkcs8::der::{AnyRef, Encode};
use pkcs8::spki::{AlgorithmIdentifierRef, SubjectPublicKeyInfoRef};
use pkcs8::{EncryptedPrivateKeyInfo, ObjectIdentifier};
use ring::rand::SystemRandom;
use ring::signature::{KeyPair, RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::paths;

const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
pub const LIFETIME: Duration = Duration::from_secs(59 * 60);

pub struct Signer {
    key: RsaKeyPair,
    issuer: String,
    subject: String,
}

impl Signer {
    pub fn load(
        path: &str,
        passphrase: Option<&str>,
        account: &str,
        user: &str,
    ) -> anyhow::Result<Signer> {
        let expanded = paths::expand(path.trim());
        let pem = std::fs::read_to_string(&expanded)
            .with_context(|| format!("cannot read the private key at {expanded}"))?;
        let key = private_key(&pem, passphrase)
            .with_context(|| format!("cannot use the private key at {expanded}"))?;
        let subject = format!("{account}.{}", user.trim().to_uppercase());
        Ok(Signer {
            issuer: format!("{subject}.{}", fingerprint(key.public_key().as_ref())?),
            subject,
            key,
        })
    }

    pub fn token(&self, now: u64) -> anyhow::Result<String> {
        let header = URL_SAFE_NO_PAD.encode(json!({"alg": "RS256", "typ": "JWT"}).to_string());
        let claims = URL_SAFE_NO_PAD.encode(
            json!({
                "iss": self.issuer,
                "sub": self.subject,
                "iat": now,
                "exp": now + LIFETIME.as_secs(),
            })
            .to_string(),
        );
        let signed = format!("{header}.{claims}");
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                signed.as_bytes(),
                &mut signature,
            )
            .map_err(|_| anyhow!("could not sign the Snowflake login with the private key"))?;
        Ok(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)))
    }
}

fn private_key(pem: &str, passphrase: Option<&str>) -> anyhow::Result<RsaKeyPair> {
    if pem.contains("Proc-Type: 4,ENCRYPTED") {
        bail!(
            "it is an encrypted PKCS#1 key, which tql cannot read; convert it with openssl pkcs8 -topk8 -v2 aes256 -in old.pem -out new.p8"
        );
    }
    let (label, der) = pkcs8::der::pem::decode_vec(pem.trim().as_bytes())
        .map_err(|error| anyhow!("it is not a PEM private key ({error})"))?;
    let rejected = |error: ring::error::KeyRejected| {
        anyhow!("it is not an RSA key tql can sign with ({error})")
    };
    match label {
        "PRIVATE KEY" => RsaKeyPair::from_pkcs8(&der).map_err(rejected),
        "RSA PRIVATE KEY" => RsaKeyPair::from_der(&der).map_err(rejected),
        "ENCRYPTED PRIVATE KEY" => {
            let Some(passphrase) = passphrase.filter(|passphrase| !passphrase.is_empty()) else {
                bail!("it is encrypted; save its passphrase as the connection's password");
            };
            let decrypted = EncryptedPrivateKeyInfo::try_from(der.as_slice())
                .and_then(|info| info.decrypt(passphrase))
                .map_err(|_| anyhow!("the passphrase does not decrypt it"))?;
            RsaKeyPair::from_pkcs8(decrypted.as_bytes()).map_err(rejected)
        }
        other => bail!("it holds a {other}, not a private key"),
    }
}

fn fingerprint(public_key: &[u8]) -> anyhow::Result<String> {
    let info = SubjectPublicKeyInfoRef {
        algorithm: AlgorithmIdentifierRef {
            oid: RSA_ENCRYPTION,
            parameters: Some(AnyRef::NULL),
        },
        subject_public_key: BitStringRef::from_bytes(public_key)?,
    };
    Ok(format!(
        "SHA256:{}",
        STANDARD.encode(Sha256::digest(info.to_der()?))
    ))
}
