use std::path::Path;

use sha2::{Digest, Sha256};

use super::http;
use super::{BINARY, Releases, UpdateFailed, Versions, failed};

pub const CHECKSUMS: &str = "SHA256SUMS";

pub fn asset() -> String {
    let system = match cfg!(target_os = "macos") {
        true => "macos",
        false => "linux",
    };
    let machine = match cfg!(target_arch = "aarch64") {
        true => "aarch64",
        false => "x86_64",
    };
    format!("{BINARY}-{system}-{machine}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updater {
    pub releases: Releases,
}

impl Updater {
    pub fn new(releases: Releases) -> Self {
        Updater { releases }
    }

    pub fn install(&self, version: &str, target: &Path) -> Result<(), UpdateFailed> {
        let asset = asset();
        let expected = self.checksum_for(version, &asset)?;
        let binary = http::fetch(
            &format!("{}/{asset}", self.releases.download(version)),
            120,
            "application/octet-stream",
        )
        .body()
        .ok_or_else(|| failed(format!("could not download {asset} for v{version}")))?;

        if hex(&Sha256::digest(&binary)) != expected {
            return Err(failed(format!(
                "the download for v{version} does not match its checksum, so it was not installed"
            )));
        }

        Versions::for_installed(target).stage(version, &binary)
    }

    fn checksum_for(&self, version: &str, asset: &str) -> Result<String, UpdateFailed> {
        let sums = http::fetch(
            &format!("{}/{CHECKSUMS}", self.releases.download(version)),
            30,
            "text/plain",
        )
        .body()
        .ok_or_else(|| {
            failed(format!(
                "v{version} publishes no checksums, so tql will not install it unchecked"
            ))
        })?;
        checksum_in(&String::from_utf8_lossy(&sums), asset)
            .ok_or_else(|| failed(format!("v{version} has no checksum for {asset}")))
    }
}

pub fn checksum_in(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, name) = line.trim_end().split_once(char::is_whitespace)?;
        let named = name.trim_start().trim_start_matches('*') == asset;
        let well_formed =
            sum.len() == 64 && sum.chars().all(|character| character.is_ascii_hexdigit());
        (named && well_formed).then(|| sum.to_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_asset_the_way_every_tql_release_has() {
        let named = asset();
        assert!(
            [
                "tql-macos-aarch64",
                "tql-macos-x86_64",
                "tql-linux-aarch64",
                "tql-linux-x86_64"
            ]
            .contains(&named.as_str()),
            "{named}"
        );
    }

    #[test]
    fn reads_the_checksum_for_its_own_asset_only() {
        let sum = "a".repeat(64);
        let other = "b".repeat(64);
        let sums = format!("{other}  tql-linux-x86_64\n{sum} *tql-macos-aarch64\n");

        assert_eq!(checksum_in(&sums, "tql-macos-aarch64"), Some(sum));
        assert_eq!(checksum_in(&sums, "tql-macos"), None);
        assert_eq!(
            checksum_in("nonsense  tql-macos-aarch64", "tql-macos-aarch64"),
            None
        );
    }
}
