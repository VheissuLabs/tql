use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::http;
use super::{BINARY, Releases, TARGET, UpdateFailed, Versions, failed};

pub const CHECKSUMS: &str = "SHA256SUMS";

pub fn asset(version: &str) -> String {
    format!("{BINARY}-{version}-{TARGET}.tar.gz")
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
        let asset = asset(version);
        let expected = self.checksum_for(version, &asset)?;
        let archive = http::fetch(
            &format!("{}/{asset}", self.releases.download(version)),
            120,
            "application/octet-stream",
        )
        .body()
        .ok_or_else(|| failed(format!("could not download {asset} for v{version}")))?;

        if hex(&Sha256::digest(&archive)) != expected {
            return Err(failed(format!(
                "the download for v{version} does not match its checksum, so it was not installed"
            )));
        }

        let binary = unpack(&archive).ok_or_else(|| {
            failed(format!(
                "{asset} for v{version} holds no {BINARY} to install"
            ))
        })?;
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
                "v{version} publishes no checksums, so tql-rust will not install it unchecked"
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

pub fn unpack(archive: &[u8]) -> Option<Vec<u8>> {
    let mut entries = tar::Archive::new(GzDecoder::new(archive));
    entries.entries().ok()?.flatten().find_map(|mut entry| {
        let named = entry.path().ok()?.file_name()? == BINARY;
        let regular = entry.header().entry_type().is_file();
        if !named || !regular {
            return None;
        }
        let mut binary = Vec::new();
        entry.read_to_end(&mut binary).ok()?;
        Some(binary)
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_asset_for_this_version_and_target() {
        assert_eq!(asset("0.8.0"), format!("tql-rust-0.8.0-{TARGET}.tar.gz"));
        assert!(TARGET.contains('-'), "{TARGET}");
    }

    #[test]
    fn reads_the_checksum_for_its_own_asset_only() {
        let sum = "a".repeat(64);
        let other = "b".repeat(64);
        let sums =
            format!("{other}  tql-rust-0.8.0-other.tar.gz\n{sum} *tql-rust-0.8.0-mine.tar.gz\n");

        assert_eq!(checksum_in(&sums, "tql-rust-0.8.0-mine.tar.gz"), Some(sum));
        assert_eq!(checksum_in(&sums, "tql-rust-0.8.0-mine"), None);
        assert_eq!(
            checksum_in(
                "nonsense  tql-rust-0.8.0-mine.tar.gz",
                "tql-rust-0.8.0-mine.tar.gz"
            ),
            None
        );
    }
}
