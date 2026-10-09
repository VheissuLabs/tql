use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

static NUMBERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+\.\d+\.\d+)").unwrap());

pub fn of(raw: &str) -> Option<String> {
    NUMBERED
        .captures(raw)
        .map(|captured| captured[1].to_string())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare(candidate, current) == Ordering::Greater
}

pub fn compare(first: &str, second: &str) -> Ordering {
    parts(first).cmp(&parts(second))
}

fn parts(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or_default())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_version_out_of_what_tql_and_github_call_it() {
        for (raw, version) in [
            ("0.7.0", Some("0.7.0")),
            ("v0.7.0", Some("0.7.0")),
            ("Tql v0.7.0", Some("0.7.0")),
            ("v0.7.0-3-gabc1234", Some("0.7.0")),
            ("dev", None),
        ] {
            assert_eq!(of(raw).as_deref(), version, "{raw}");
        }
    }

    #[test]
    fn compares_each_part_as_a_number() {
        assert!(is_newer("0.8.0", "0.7.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.7.0", "0.7.0"));
        assert!(!is_newer("0.6.10", "0.7.0"));
    }
}
