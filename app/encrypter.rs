use std::fs;
use std::path::Path;

use aes::Aes256;
use anyhow::{Context, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct Encrypter {
    key: Vec<u8>,
}

impl Encrypter {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("cannot read the tql key at {}", path.display()))?;
        Self::new(contents.trim())
    }

    pub fn ensure(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            write_new_key(path)?;
        }
        Self::from_file(path)
    }

    pub fn throwaway() -> anyhow::Result<Self> {
        Ok(Encrypter {
            key: random_bytes(32)?,
        })
    }

    pub fn new(key: &str) -> anyhow::Result<Self> {
        let key = match key.strip_prefix("base64:") {
            Some(encoded) => STANDARD
                .decode(encoded)
                .context("the tql key is not valid base64")?,
            None => key.as_bytes().to_vec(),
        };
        if key.len() != 32 {
            bail!(
                "the tql key must be 32 bytes for AES-256-CBC, found {}",
                key.len()
            );
        }
        Ok(Encrypter { key })
    }

    pub fn encrypt_string(&self, value: &str) -> anyhow::Result<String> {
        let iv = random_bytes(16)?;
        let length = value.len();
        let mut buffer = value.as_bytes().to_vec();
        buffer.resize(length + 16 - length % 16, 0);
        let ciphertext = cbc::Encryptor::<Aes256>::new_from_slices(&self.key, &iv)
            .map_err(|_| anyhow!("the tql key is the wrong length"))?
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, length)
            .map_err(|_| anyhow!("the value could not be encrypted"))?
            .to_vec();

        let iv = STANDARD.encode(iv);
        let value = STANDARD.encode(ciphertext);
        let mut signer = Hmac::<Sha256>::new_from_slice(&self.key)?;
        signer.update(iv.as_bytes());
        signer.update(value.as_bytes());
        let mac = encode_hex(&signer.finalize().into_bytes());

        let payload = serde_json::json!({ "iv": iv, "value": value, "mac": mac, "tag": "" });
        Ok(STANDARD.encode(serde_json::to_string(&payload)?))
    }

    pub fn decrypt_string(&self, payload: &str) -> anyhow::Result<String> {
        let json: serde_json::Value = serde_json::from_slice(
            &STANDARD
                .decode(payload.trim())
                .context("the payload is not valid base64")?,
        )
        .context("the payload is not valid JSON")?;

        let field = |name: &str| {
            json[name]
                .as_str()
                .ok_or_else(|| anyhow!("the payload has no `{name}`"))
        };
        let (iv, value, mac) = (field("iv")?, field("value")?, field("mac")?);

        let mut signer = Hmac::<Sha256>::new_from_slice(&self.key)?;
        signer.update(iv.as_bytes());
        signer.update(value.as_bytes());
        signer
            .verify_slice(&decode_hex(mac)?)
            .map_err(|_| anyhow!("the MAC is invalid; was this encrypted with another key?"))?;

        let iv = STANDARD.decode(iv)?;
        let mut ciphertext = STANDARD.decode(value)?;
        let plaintext = cbc::Decryptor::<Aes256>::new_from_slices(&self.key, &iv)
            .map_err(|_| anyhow!("the IV is the wrong length"))?
            .decrypt_padded_mut::<Pkcs7>(&mut ciphertext)
            .map_err(|_| anyhow!("the payload could not be decrypted"))?;

        Ok(String::from_utf8(plaintext.to_vec())?)
    }
}

fn random_bytes(count: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = vec![0; count];
    getrandom::fill(&mut bytes).map_err(|error| anyhow!("no randomness available: {error}"))?;
    Ok(bytes)
}

fn write_new_key(path: &Path) -> anyhow::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    fs::write(
        path,
        format!("base64:{}", STANDARD.encode(random_bytes(32)?)),
    )
    .with_context(|| format!("cannot write a tql key to {}", path.display()))?;
    restrict_to_owner(path)
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::set_permissions(
        path,
        fs::Permissions::from_mode(0o600),
    )?)
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    if !hex.is_ascii() || !hex.len().is_multiple_of(2) {
        bail!("the MAC is not valid hex");
    }
    (0..hex.len())
        .step_by(2)
        .map(|start| {
            u8::from_str_radix(&hex[start..start + 2], 16)
                .map_err(|_| anyhow!("the MAC is not valid hex"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Encrypter;

    const TEST_KEY: &str = "base64:a2tra2tra2tra2tra2tra2tra2tra2tra2tra2tra2s=";
    const LARAVEL_PAYLOAD: &str = "eyJpdiI6Inc4Wms3U2xJSzBSMTZKZDlBVmFIQnc9PSIsInZhbHVlIjoiZ2l1U3I0TFVzYzJIWlk2Tlo2UUI1NFFFazN4L3NKaGRJRUxkY1NUSEtXcz0iLCJtYWMiOiJlNWYzZGJiYTZkMGMwN2M0YTgxNDQ4MDIxZTY4MDBhZDkxMmI4OTcwMDQwODdlMjE0NDI4MDlkY2U2OWVhOTkyIiwidGFnIjoiIn0=";

    #[test]
    fn decrypts_what_laravels_encrypter_wrote() {
        let encrypter = Encrypter::new(TEST_KEY).unwrap();

        assert_eq!(
            encrypter.decrypt_string(LARAVEL_PAYLOAD).unwrap(),
            "s3cret/pässword"
        );
    }

    #[test]
    fn rejects_a_payload_from_another_key() {
        let other = Encrypter::new("base64:bGxsbGxsbGxsbGxsbGxsbGxsbGxsbGxsbGxsbGxsbGw=").unwrap();

        let error = other.decrypt_string(LARAVEL_PAYLOAD).unwrap_err();
        assert!(error.to_string().contains("MAC is invalid"));
    }

    #[test]
    fn decrypts_what_it_encrypted_with_a_fresh_iv_each_time() {
        let encrypter = Encrypter::new(TEST_KEY).unwrap();

        let first = encrypter.encrypt_string("s3cret/pässword").unwrap();
        let second = encrypter.encrypt_string("s3cret/pässword").unwrap();

        assert_ne!(first, second);
        assert_eq!(encrypter.decrypt_string(&first).unwrap(), "s3cret/pässword");
    }

    #[test]
    fn laravels_encrypter_reads_what_it_wrote() {
        let tql =
            std::path::PathBuf::from(std::env::var("TQL_PATH").unwrap_or_else(|_| {
                format!("{}/Code/tql", std::env::var("HOME").unwrap_or_default())
            }));
        let autoload = tql.join("vendor/autoload.php");
        if !autoload.exists() {
            eprintln!("skipped: no tql checkout at {}", tql.display());
            return;
        }

        let payload = Encrypter::new(TEST_KEY)
            .unwrap()
            .encrypt_string("s3cret/pässword")
            .unwrap();
        let script = format!(
            "require '{}'; $encrypter = new Illuminate\\Encryption\\Encrypter(base64_decode('{}'), 'AES-256-CBC'); echo $encrypter->decryptString('{payload}');",
            autoload.display(),
            TEST_KEY.trim_start_matches("base64:"),
        );
        let Ok(output) = std::process::Command::new("php")
            .args(["-r", &script])
            .output()
        else {
            eprintln!("skipped: php is not installed");
            return;
        };

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "s3cret/pässword");
    }

    #[test]
    fn writes_a_key_where_there_is_none_and_reads_it_back_after() {
        let directory = std::env::temp_dir().join(format!("tql-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let path = directory.join("tql/key");

        let payload = Encrypter::ensure(&path)
            .unwrap()
            .encrypt_string("pw")
            .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        let reread = Encrypter::ensure(&path).unwrap().decrypt_string(&payload);
        std::fs::remove_dir_all(&directory).unwrap();

        assert!(contents.starts_with("base64:"));
        assert_eq!(reread.unwrap(), "pw");
    }

    #[test]
    fn rejects_keys_of_the_wrong_length() {
        assert!(Encrypter::new("base64:c2hvcnQ=").is_err());
    }

    #[test]
    fn a_mac_that_is_not_ascii_is_refused_rather_than_sliced() {
        use base64::Engine;
        let encrypter = Encrypter::new(TEST_KEY).unwrap();
        let payload = base64::engine::general_purpose::STANDARD
            .encode(r#"{"iv":"AAAA","value":"AAAA","mac":"aéa"}"#);
        assert!(encrypter.decrypt_string(&payload).is_err());
    }
}
