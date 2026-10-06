pub const NONE: &str = "none";

const TAGS: [(&str, &str); 4] = [
    ("production", "red"),
    ("staging", "yellow"),
    ("dev", "blue"),
    ("local", "green"),
];

pub fn parse(tag: &str) -> Option<&'static str> {
    let wanted = tag.trim().to_lowercase();
    TAGS.iter()
        .find(|(name, _)| *name == wanted)
        .map(|(name, _)| *name)
}

pub fn color_of(tag: Option<&str>) -> Option<&'static str> {
    let wanted = parse(tag?)?;
    TAGS.iter()
        .find(|(name, _)| *name == wanted)
        .map(|(_, color)| *color)
}

pub fn names() -> Vec<&'static str> {
    TAGS.iter().map(|(name, _)| *name).collect()
}

pub fn choices() -> Vec<String> {
    std::iter::once(NONE)
        .chain(names())
        .map(String::from)
        .collect()
}

pub fn colors() -> Vec<(String, String)> {
    TAGS.iter()
        .map(|(name, color)| (name.to_string(), color.to_string()))
        .collect()
}

pub fn value(choice: &str) -> String {
    if choice == NONE {
        return String::new();
    }
    choice.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_fixed_set_whatever_the_case() {
        assert_eq!(parse(" Production "), Some("production"));
        assert_eq!(parse("prodction"), None);
        assert_eq!(color_of(Some("local")), Some("green"));
        assert_eq!(color_of(Some("")), None);
        assert_eq!(color_of(None), None);
    }

    #[test]
    fn none_is_a_choice_that_clears_the_tag() {
        assert_eq!(choices(), ["none", "production", "staging", "dev", "local"]);
        assert_eq!(value("none"), "");
        assert_eq!(value("staging"), "staging");
    }
}
