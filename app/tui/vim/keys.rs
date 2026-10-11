use super::*;

impl Vim {
    pub(super) fn parse(
        &self,
        keys: &[Key],
        allow_operators: bool,
        allow_objects: bool,
    ) -> Parsed<Command> {
        let mut at = 0;
        let first = read_count(keys, &mut at);
        let Some(&key) = keys.get(at) else {
            return Parsed::Waiting;
        };

        if allow_operators && let Some(operator) = operator_of(key) {
            return parse_operator(keys, at + 1, operator, first);
        }

        match parse_target(keys, at, allow_objects) {
            Parsed::Waiting => return Parsed::Waiting,
            Parsed::Ready(target) => {
                return Parsed::Ready(Command {
                    order: Order::Go(target),
                    count: first.unwrap_or(1),
                    counted: first.is_some(),
                    body: keys[at..].to_vec(),
                });
            }
            Parsed::Invalid => {}
        }

        if !allow_operators {
            return Parsed::Invalid;
        }
        let character = keys.get(at + 1).copied();
        if key == Key::Char('r') && character.is_none() {
            return Parsed::Waiting;
        }
        Parsed::Ready(Command {
            order: Order::Act(key, character),
            count: first.unwrap_or(1),
            counted: first.is_some(),
            body: keys[at..].to_vec(),
        })
    }
}

pub(super) fn typed(key: Key) -> Option<char> {
    match key {
        Key::Char(character) => Some(character),
        Key::Tab => Some('\t'),
        _ => None,
    }
}
pub(super) fn operator_of(key: Key) -> Option<char> {
    match key {
        Key::Char(operator @ ('d' | 'c' | 'y' | '>' | '<')) => Some(operator),
        _ => None,
    }
}
pub(super) fn motion_of(key: Key) -> Option<Motion> {
    Some(match key {
        Key::Char('h') | Key::Left | Key::Backspace => Motion::Left,
        Key::Char('l' | ' ') | Key::Right => Motion::Right,
        Key::Char('j') | Key::Down => Motion::Down,
        Key::Char('k') | Key::Up => Motion::Up,
        Key::Enter => Motion::NextLine,
        Key::Char('0') => Motion::LineStart,
        Key::Char('^') => Motion::FirstNonBlank,
        Key::Char('$') => Motion::LineEnd,
        Key::Char('w') => Motion::WordForward(false),
        Key::Char('W') => Motion::WordForward(true),
        Key::Char('b') => Motion::WordBackward(false),
        Key::Char('B') => Motion::WordBackward(true),
        Key::Char('e') => Motion::WordEnd(false),
        Key::Char('E') => Motion::WordEnd(true),
        Key::Char('G') => Motion::LastLine,
        Key::Char(';') => Motion::RepeatFind(false),
        Key::Char(',') => Motion::RepeatFind(true),
        Key::Char('%') => Motion::Bracket,
        Key::Char('}') => Motion::ParagraphForward,
        Key::Char('{') => Motion::ParagraphBackward,
        _ => return None,
    })
}
pub(super) fn is_change(key: Key) -> bool {
    matches!(
        key,
        Key::Char(
            'x' | 'X'
                | 'D'
                | 'C'
                | 's'
                | 'S'
                | 'r'
                | 'J'
                | '~'
                | 'p'
                | 'P'
                | 'i'
                | 'a'
                | 'I'
                | 'A'
                | 'o'
                | 'O'
        )
    )
}
pub(super) fn digits_of(count: usize) -> Vec<Key> {
    count.to_string().chars().map(Key::Char).collect()
}
pub(super) fn is_digit(key: &Key) -> bool {
    matches!(key, Key::Char(digit) if digit.is_ascii_digit())
}
pub(super) fn read_count(keys: &[Key], at: &mut usize) -> Option<usize> {
    let mut count: Option<usize> = None;
    while let Some(Key::Char(digit)) = keys.get(*at) {
        let Some(value) = digit.to_digit(10) else {
            break;
        };
        if count.is_none() && value == 0 {
            break;
        }
        count = Some((count.unwrap_or_default() * 10 + value as usize).min(MOST_REPEATS));
        *at += 1;
    }
    count
}
pub(super) fn parse_target(keys: &[Key], at: usize, allow_objects: bool) -> Parsed<Target> {
    let next = keys.get(at + 1).copied();
    match keys[at] {
        Key::Char(find @ ('f' | 't' | 'F' | 'T')) => match next {
            None => Parsed::Waiting,
            Some(wanted) => Parsed::Ready(Target::Motion(Motion::Find(find, wanted))),
        },
        Key::Char('g') => match next {
            None => Parsed::Waiting,
            Some(Key::Char('g')) => Parsed::Ready(Target::Motion(Motion::FirstLine)),
            Some(_) => Parsed::Invalid,
        },
        Key::Char(side @ ('i' | 'a')) if allow_objects => match next {
            None => Parsed::Waiting,
            Some(Key::Char(
                object @ ('w' | 'W' | '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '"' | '\''
                | '`' | 'p'),
            )) => Parsed::Ready(Target::Object(Object {
                around: side == 'a',
                kind: object,
            })),
            Some(_) => Parsed::Invalid,
        },
        key => motion_of(key).map_or(Parsed::Invalid, |motion| {
            Parsed::Ready(Target::Motion(motion))
        }),
    }
}
pub(super) fn parse_operator(
    keys: &[Key],
    at: usize,
    operator: char,
    first: Option<usize>,
) -> Parsed<Command> {
    let Some(&next) = keys.get(at) else {
        return Parsed::Waiting;
    };
    if next == Key::Char(operator) {
        return Parsed::Ready(Command {
            order: Order::Operate(operator, Operand::Lines),
            count: first.unwrap_or(1),
            counted: first.is_some(),
            body: vec![next, next],
        });
    }

    let mut at = at;
    let second = read_count(keys, &mut at);
    if keys.get(at).is_none() {
        return Parsed::Waiting;
    }
    let target = match parse_target(keys, at, true) {
        Parsed::Waiting => return Parsed::Waiting,
        Parsed::Invalid => return Parsed::Invalid,
        Parsed::Ready(target) => target,
    };
    let mut body = vec![Key::Char(operator)];
    body.extend_from_slice(&keys[at..]);
    Parsed::Ready(Command {
        order: Order::Operate(operator, Operand::On(target)),
        count: (first.unwrap_or(1) * second.unwrap_or(1)).min(MOST_REPEATS),
        counted: first.is_some() || second.is_some(),
        body,
    })
}
