use jiff::Zoned;
use jiff::tz::TimeZone;

use crate::settings;

pub const LITERAL: &str = "now()";

pub fn asked(value: &str) -> bool {
    value.trim().to_lowercase() == LITERAL
}

pub fn for_type(column_type: Option<&str>) -> String {
    let column_type = column_type.unwrap_or_default().trim().to_lowercase();
    let format = match column_type.as_str() {
        "date" => "%Y-%m-%d",
        "year" => "%Y",
        time if time.starts_with("time") && !time.starts_with("timestamp") => "%H:%M:%S",
        _ => "%Y-%m-%d %H:%M:%S",
    };
    moment().strftime(format).to_string()
}

pub fn label() -> String {
    moment().strftime("%Z").to_string()
}

pub fn suits(column_type: Option<&str>) -> bool {
    let column_type = column_type.unwrap_or_default().trim().to_lowercase();
    !column_type.is_empty()
        && ["date", "time", "year"]
            .iter()
            .any(|shape| column_type.contains(shape))
}

fn moment() -> Zoned {
    let zone = settings::with(|settings| settings.time_zone.trim().to_string());
    let zone = TimeZone::get(&zone)
        .ok()
        .or_else(|| offset(&zone).map(TimeZone::fixed))
        .unwrap_or(TimeZone::UTC);
    Zoned::now().with_time_zone(zone)
}

fn offset(zone: &str) -> Option<jiff::tz::Offset> {
    let (sign, digits) = match zone.split_at_checked(1)? {
        ("+", digits) => (1, digits),
        ("-", digits) => (-1, digits),
        _ => return None,
    };
    let digits = digits.replace(':', "");
    if !digits.chars().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    let (hours, minutes) = match digits.len() {
        1 | 2 => (digits.parse::<i32>().ok()?, 0),
        4 => (digits[..2].parse().ok()?, digits[2..].parse().ok()?),
        _ => return None,
    };
    if hours > 14 || minutes > 59 {
        return None;
    }
    jiff::tz::Offset::from_seconds(sign * (hours * 3600 + minutes * 60)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn in_zone(zone: &str) {
        let mut settings = Settings::default();
        settings.time_zone = zone.to_string();
        settings::install(settings);
    }

    #[test]
    fn takes_an_offset_as_well_as_a_zone_name() {
        for (zone, seconds) in [("+05:30", 19_800), ("+0530", 19_800), ("-03", -10_800)] {
            in_zone(zone);
            assert_eq!(moment().offset().seconds(), seconds, "{zone}");
        }
        in_zone("nowhere");
        assert_eq!(moment().offset().seconds(), 0);
        in_zone("UTC");
    }

    fn utc(format: &str) -> String {
        Zoned::now()
            .with_time_zone(TimeZone::UTC)
            .strftime(format)
            .to_string()
    }

    #[test]
    fn writes_the_time_the_way_the_column_takes_it() {
        in_zone("UTC");

        assert_eq!(for_type(Some("date")), utc("%Y-%m-%d"));
        assert_eq!(for_type(Some("time")), utc("%H:%M:%S"));
        assert_eq!(for_type(Some("year")), utc("%Y"));
        assert_eq!(for_type(Some("timestamp")), utc("%Y-%m-%d %H:%M:%S"));
        assert_eq!(for_type(Some("text")), utc("%Y-%m-%d %H:%M:%S"));
    }

    #[test]
    fn knows_which_columns_hold_a_moment() {
        assert!(suits(Some("datetime")));
        assert!(suits(Some("timestamp without time zone")));
        assert!(suits(Some("DATE")));
        assert!(suits(Some("year")));
        assert!(!suits(Some("varchar")));
        assert!(!suits(None));
    }

    #[test]
    fn now_is_asked_for_however_it_is_typed() {
        assert!(asked("now()"));
        assert!(asked(" NOW() "));
        assert!(!asked("now"));
    }

    #[test]
    fn writes_utc_unless_the_config_says_otherwise() {
        in_zone("UTC");
        assert_eq!(label(), "UTC");

        in_zone("Asia/Kolkata");
        assert_eq!(label(), "IST");
        assert_ne!(for_type(None), utc("%Y-%m-%d %H:%M:%S"));

        in_zone("Nowhere/Special");
        assert_eq!(label(), "UTC");

        in_zone("UTC");
    }
}
