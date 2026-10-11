use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub arguments: Vec<Argument>,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argument {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub array: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Flag,
    Value,
    Array,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub name: String,
    pub shortcut: Option<char>,
    pub description: String,
    pub kind: OptionKind,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureError(pub String);

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SignatureError {}

fn error<T>(message: impl Into<String>) -> Result<T, SignatureError> {
    Err(SignatureError(message.into()))
}

impl Signature {
    pub fn parse(signature: &str) -> Result<Self, SignatureError> {
        let signature = signature.trim();
        let (name, mut rest) = match signature.find('{') {
            Some(brace) => (signature[..brace].trim(), &signature[brace..]),
            None => (signature, ""),
        };

        if name.is_empty() || name.contains(char::is_whitespace) {
            return error(format!(
                "signature `{signature}` must start with a command name"
            ));
        }

        let mut parsed = Signature {
            name: name.to_string(),
            arguments: Vec::new(),
            options: Vec::new(),
        };

        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            if !rest.starts_with('{') {
                return error(format!("unexpected `{rest}` in signature `{signature}`"));
            }
            let Some(end) = rest.find('}') else {
                return error(format!("unclosed `{{` in signature `{signature}`"));
            };
            let token = &rest[1..end];
            rest = &rest[end + 1..];

            let (token, description) = match token.split_once(" : ") {
                Some((token, description)) => (token.trim(), description.trim()),
                None => (token.trim(), ""),
            };

            match token.strip_prefix("--") {
                Some(option) => {
                    let option = parse_option(option, description)?;
                    parsed.ensure_unique(&option.name, true)?;
                    parsed.options.push(option);
                }
                None => parsed.push_argument(parse_argument(token, description)?)?,
            }
        }

        Ok(parsed)
    }

    fn ensure_unique(&self, name: &str, option: bool) -> Result<(), SignatureError> {
        let taken = match option {
            true => self.options.iter().any(|option| option.name == name),
            false => self.arguments.iter().any(|argument| argument.name == name),
        };
        if taken {
            return error(format!("`{name}` is defined twice in `{}`", self.name));
        }
        Ok(())
    }

    fn push_argument(&mut self, argument: Argument) -> Result<(), SignatureError> {
        self.ensure_unique(&argument.name, false)?;
        if let Some(last) = self.arguments.last() {
            if last.array {
                return error(format!(
                    "argument `{}` cannot follow array argument `{}`",
                    argument.name, last.name
                ));
            }
            if !last.required && argument.required {
                return error(format!(
                    "required argument `{}` cannot follow optional argument `{}`",
                    argument.name, last.name
                ));
            }
        }
        self.arguments.push(argument);
        Ok(())
    }
}

fn parse_argument(token: &str, description: &str) -> Result<Argument, SignatureError> {
    let (name, required, array, default) = if let Some(name) = token.strip_suffix("?*") {
        (name, false, true, None)
    } else if let Some(name) = token.strip_suffix('*') {
        (name, true, true, None)
    } else if let Some(name) = token.strip_suffix('?') {
        (name, false, false, None)
    } else if let Some((name, default)) = token.split_once('=') {
        (name, false, false, non_empty(default))
    } else {
        (token, true, false, None)
    };

    validate_name(name)?;

    Ok(Argument {
        name: name.to_string(),
        description: description.to_string(),
        required,
        array,
        default,
    })
}

fn parse_option(token: &str, description: &str) -> Result<Opt, SignatureError> {
    let (shortcut, token) = match token.split_once('|') {
        Some((shortcut, rest)) => {
            let mut characters = shortcut.chars();
            match (characters.next(), characters.next()) {
                (Some(letter), None) if letter.is_ascii_alphanumeric() => (Some(letter), rest),
                _ => {
                    return error(format!(
                        "option shortcut `{shortcut}` must be a single character"
                    ));
                }
            }
        }
        None => (None, token),
    };

    let (name, kind, default) = if let Some(name) = token.strip_suffix("=*") {
        (name, OptionKind::Array, None)
    } else if let Some((name, default)) = token.split_once('=') {
        (name, OptionKind::Value, non_empty(default))
    } else {
        (token, OptionKind::Flag, None)
    };

    validate_name(name)?;

    Ok(Opt {
        name: name.to_string(),
        shortcut,
        description: description.to_string(),
        kind,
        default,
    })
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn validate_name(name: &str) -> Result<(), SignatureError> {
    let valid = !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
    if !valid {
        return error(format!("`{name}` is not a valid argument or option name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_only() {
        let sig = Signature::parse("inspire").unwrap();
        assert_eq!(sig.name, "inspire");
        assert!(sig.arguments.is_empty());
        assert!(sig.options.is_empty());
    }

    #[test]
    fn parses_arguments() {
        let sig =
            Signature::parse("mail:send {user : The user} {queue?} {tag=news} {ids?*}").unwrap();
        assert_eq!(sig.name, "mail:send");

        let [user, queue, tag, ids] = sig.arguments.as_slice() else {
            panic!("expected four arguments");
        };
        assert!(user.required && !user.array);
        assert_eq!(user.description, "The user");
        assert!(!queue.required);
        assert_eq!(tag.default.as_deref(), Some("news"));
        assert!(!tag.required);
        assert!(ids.array && !ids.required);
    }

    #[test]
    fn parses_options() {
        let sig =
            Signature::parse("x {--force} {--Q|queue=} {--times=3 : Repeat} {--id=*}").unwrap();
        let [force, queue, times, id] = sig.options.as_slice() else {
            panic!("expected four options");
        };
        assert_eq!(force.kind, OptionKind::Flag);
        assert_eq!(queue.kind, OptionKind::Value);
        assert_eq!(queue.shortcut, Some('Q'));
        assert_eq!(queue.default, None);
        assert_eq!(times.default.as_deref(), Some("3"));
        assert_eq!(times.description, "Repeat");
        assert_eq!(id.kind, OptionKind::Array);
    }

    #[test]
    fn rejects_bad_signatures() {
        for bad in [
            "",
            "{name}",
            "x {name",
            "x name",
            "x {a?} {b}",
            "x {a*} {b?}",
            "x {a} {a}",
            "x {--a} {--a}",
            "x {--ab|long}",
            "x {bad name}",
        ] {
            assert!(Signature::parse(bad).is_err(), "`{bad}` should be rejected");
        }
    }

    #[test]
    fn an_argument_and_an_option_may_share_a_name() {
        let parsed = Signature::parse("describe {table} {--table}").unwrap();

        assert_eq!(parsed.arguments[0].name, "table");
        assert_eq!(parsed.options[0].name, "table");
    }
}
