use std::fs;
use std::path::Path;

use aes::Aes256;
use anyhow::{Context, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
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

fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
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
    fn rejects_keys_of_the_wrong_length() {
        assert!(Encrypter::new("base64:c2hvcnQ=").is_err());
    }
}
