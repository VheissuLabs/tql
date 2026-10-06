use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

pub fn ago(timestamp: Option<&str>, now: i64) -> String {
    let Some(then) = timestamp.and_then(parse) else {
        return "never".to_string();
    };

    let (count, unit) = span(now - then);
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

pub fn short(timestamp: &str, now: i64) -> String {
    let Some(then) = parse(timestamp) else {
        return String::new();
    };

    let (count, unit) = span(now - then);
    let abbreviation = match unit {
        "second" => "s",
        "minute" => "m",
        "hour" => "h",
        "day" => "d",
        "week" => "w",
        "month" => "mo",
        _ => "yr",
    };
    let plural = if count != 1 && abbreviation.len() > 1 {
        "s"
    } else {
        ""
    };
    format!("{count}{abbreviation}{plural} ago")
}

fn span(seconds: i64) -> (i64, &'static str) {
    let seconds = seconds.max(0);
    let days = seconds / 86_400;
    match seconds {
        0..60 => (seconds, "second"),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        _ if days < 7 => (days, "day"),
        _ if days < 30 => (days / 7, "week"),
        _ if days < 365 => (days / 30, "month"),
        _ => (days / 365, "year"),
    }
}

fn parse(timestamp: &str) -> Option<i64> {
    let digits: Vec<i64> = timestamp
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(6)
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let [year, month, day, hour, minute, second] = digits[..] else {
        return None;
    };

    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_like_carbons_diff_for_humans() {
        let now = parse("2026-10-05 23:00:00").unwrap();

        assert_eq!(ago(Some("2026-10-05 21:00:00"), now), "2 hours ago");
        assert_eq!(ago(Some("2026-10-05 22:59:00"), now), "1 minute ago");
        assert_eq!(ago(Some("2026-10-02 23:00:00"), now), "3 days ago");
        assert_eq!(ago(Some("2026-09-27 23:00:00"), now), "1 week ago");
        assert_eq!(ago(Some("2026-07-01 00:00:00"), now), "3 months ago");
        assert_eq!(ago(None, now), "never");
    }

    #[test]
    fn shortens_like_carbons_short_diff() {
        let now = parse("2026-10-05 23:00:00").unwrap();

        assert_eq!(short("2026-10-05 23:00:00", now), "0s ago");
        assert_eq!(short("2026-10-05 22:58:00", now), "2m ago");
        assert_eq!(short("2026-08-01 23:00:00", now), "2mos ago");
        assert_eq!(short("2024-08-01 23:00:00", now), "2yrs ago");
    }

    #[test]
    fn parses_utc_timestamps_to_unix_time() {
        assert_eq!(parse("1970-01-01 00:00:00"), Some(0));
        assert_eq!(parse("2000-03-01 00:00:00"), Some(951_868_800));
    }
}
