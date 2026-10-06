use std::fmt;

pub mod check;
pub mod http;
pub mod installation;
pub mod releases;
pub mod remembered;
pub mod updater;
pub mod version;
pub mod versions;

pub use installation::{How, Installation};
pub use releases::{Releases, Unavailable};
pub use remembered::Remembered;
pub use updater::Updater;
pub use versions::Versions;

pub const BINARY: &str = "tql-rust";

pub const TARGET: &str = env!("TQL_RUST_TARGET");

pub fn current_version() -> String {
    version::of(env!("CARGO_PKG_VERSION")).unwrap_or_else(|| "0.0.0".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFailed(pub String);

impl fmt::Display for UpdateFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UpdateFailed {}

pub fn failed(message: impl Into<String>) -> UpdateFailed {
    UpdateFailed(message.into())
}
