use std::cell::RefCell;

use super::http::{self, Fetched};
use super::version;

pub const REPOSITORY: &str = "VheissuLabs/tql";

pub const REPOSITORY_VARIABLE: &str = "TQL_UPDATE_REPOSITORY";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Releases {
    pub repository: String,
    latest: String,
    downloads: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    NoReleases,
    Unreachable,
}

thread_local! {
    static SOURCE: RefCell<Option<Releases>> = const { RefCell::new(None) };
}

pub fn use_source(releases: Releases) {
    SOURCE.with(|source| *source.borrow_mut() = Some(releases));
}

impl Releases {
    pub fn configured() -> Self {
        if let Some(chosen) = SOURCE.with(|source| source.borrow().clone()) {
            return chosen;
        }
        let repository = std::env::var(REPOSITORY_VARIABLE)
            .ok()
            .filter(|repository| is_repository(repository))
            .unwrap_or_else(|| REPOSITORY.to_string());
        Releases::github(&repository)
    }

    pub fn github(repository: &str) -> Self {
        Releases::at(
            repository,
            &format!("https://api.github.com/repos/{repository}/releases/latest"),
            &format!("https://github.com/{repository}/releases/download"),
        )
    }

    pub fn at(repository: &str, latest: &str, downloads: &str) -> Self {
        Releases {
            repository: repository.to_string(),
            latest: latest.to_string(),
            downloads: downloads.trim_end_matches('/').to_string(),
        }
    }

    pub fn page(&self) -> String {
        format!("github.com/{}/releases", self.repository)
    }

    pub fn download(&self, version: &str) -> String {
        format!("{}/v{version}", self.downloads)
    }

    pub fn latest(&self) -> Result<String, Unavailable> {
        let body = match http::fetch(&self.latest, 3, "application/vnd.github+json") {
            Fetched::Body(body) => body,
            Fetched::Missing => return Err(Unavailable::NoReleases),
            Fetched::Unreachable => return Err(Unavailable::Unreachable),
        };
        let release: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| Unavailable::Unreachable)?;
        release["tag_name"]
            .as_str()
            .and_then(version::of)
            .ok_or(Unavailable::NoReleases)
    }
}

fn is_repository(candidate: &str) -> bool {
    let mut halves = candidate.split('/');
    let named = |half: Option<&str>| {
        half.is_some_and(|half| {
            !half.is_empty()
                && half
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        })
    };
    named(halves.next()) && named(halves.next()) && halves.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_github_about_the_tql_repository_by_default() {
        let releases = Releases::github(REPOSITORY);

        assert_eq!(
            releases.latest,
            "https://api.github.com/repos/VheissuLabs/tql/releases/latest"
        );
        assert_eq!(
            releases.download("0.8.0"),
            "https://github.com/VheissuLabs/tql/releases/download/v0.8.0"
        );
        assert_eq!(releases.page(), "github.com/VheissuLabs/tql/releases");
    }

    #[test]
    fn takes_another_repository_only_when_it_looks_like_one() {
        assert!(is_repository("someone/tql"));
        assert!(is_repository("some.one/tql_rust"));
        assert!(!is_repository("someone"));
        assert!(!is_repository("someone/tql/extra"));
        assert!(!is_repository("someone/"));
        assert!(!is_repository("https://example.com/x"));
    }
}
