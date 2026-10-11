use std::path::Path;

pub use plist::Value;

pub fn read(path: &Path) -> anyhow::Result<Value> {
    Value::from_file(path)
        .map_err(|_| anyhow::anyhow!("{} is not a property list tql can read.", path.display()))
}

pub fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Integer(number)) => number.to_string(),
        Some(Value::Real(number)) => real(*number),
        Some(Value::Boolean(true)) => "1".to_string(),
        Some(Value::Date(date)) => date.to_xml_format(),
        Some(Value::Array(_) | Value::Dictionary(_)) => "Array".to_string(),
        _ => String::new(),
    }
}

pub fn int(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Integer(number)) => number
            .as_signed()
            .or_else(|| number.as_unsigned().map(|number| number as i64))
            .unwrap_or_default(),
        Some(Value::Real(number)) => *number as i64,
        Some(Value::Boolean(flag)) => i64::from(*flag),
        Some(Value::String(text)) => leading_integer(text),
        Some(Value::Array(items)) => i64::from(!items.is_empty()),
        Some(Value::Dictionary(entries)) => i64::from(!entries.is_empty()),
        _ => 0,
    }
}

pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Boolean(flag)) => *flag,
        Some(Value::Integer(_)) => int(value) != 0,
        Some(Value::Real(number)) => *number != 0.0,
        Some(Value::String(text)) => !text.is_empty() && text != "0",
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Dictionary(entries)) => !entries.is_empty(),
        Some(_) => true,
        None => false,
    }
}

fn real(number: f64) -> String {
    match number.fract() == 0.0 && number.abs() < 1e15 {
        true => format!("{}", number as i64),
        false => number.to_string(),
    }
}

fn leading_integer(text: &str) -> i64 {
    let trimmed = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    sign * digits[..end].parse::<i64>().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casts_like_php() {
        let port = Value::String(" 3306abc".into());
        assert_eq!(int(Some(&port)), 3306);
        assert_eq!(int(Some(&Value::String("".into()))), 0);
        assert_eq!(text(Some(&Value::Integer(5432.into()))), "5432");
        assert_eq!(text(Some(&Value::Boolean(false))), "");
        assert!(!truthy(Some(&Value::String("0".into()))));
        assert!(truthy(Some(&Value::Integer(1.into()))));
        assert!(!truthy(None));
    }
}
