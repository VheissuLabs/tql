use super::text::Text;

const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

pub fn is_blank(character: Option<char>) -> bool {
    matches!(
        character,
        None | Some(' ' | '\t' | '\n' | '\r' | '\0' | '\x0B')
    )
}

pub fn kind(character: Option<char>, big: bool) -> u8 {
    let Some(character) = character.filter(|_| !is_blank(character)) else {
        return 0;
    };
    if big || character.is_alphanumeric() || character == '_' {
        return 1;
    }
    2
}

pub fn left(text: &Text, at: usize, count: usize) -> usize {
    text.line_start(at).max(at.saturating_sub(count))
}

pub fn right(text: &Text, at: usize, count: usize) -> usize {
    text.line_end(at).min(at + count)
}

pub fn vertical(text: &Text, at: usize, by: isize, column: usize) -> Option<usize> {
    let line = text.line_of(at);
    let target = (line as isize + by).clamp(0, text.line_count() as isize - 1) as usize;
    if target == line {
        return None;
    }
    let start = text.start_of_line(target);
    Some(start + column.min((text.line_end(start) - start).saturating_sub(1)))
}

pub fn line_start(text: &Text, at: usize) -> usize {
    text.line_start(at)
}

pub fn first_non_blank(text: &Text, at: usize) -> usize {
    let mut offset = text.line_start(at);
    let end = text.line_end(offset);
    while offset < end && matches!(text.at(offset), Some(' ' | '\t')) {
        offset += 1;
    }
    offset
}

pub fn last_character(text: &Text, at: usize, count: usize) -> usize {
    let line = (text.line_count() - 1).min(text.line_of(at) + count - 1);
    let start = text.start_of_line(line);
    let end = text.line_end(start);
    if end > start { end - 1 } else { start }
}

pub fn line(text: &Text, line: usize) -> usize {
    first_non_blank(text, text.start_of_line(line.min(text.line_count() - 1)))
}

pub fn word_forward(text: &Text, at: usize, count: usize, big: bool) -> usize {
    (0..count).fold(at, |offset, _| next_word_start(text, offset, big))
}

pub fn word_backward(text: &Text, at: usize, count: usize, big: bool) -> usize {
    (0..count).fold(at, |offset, _| previous_word_start(text, offset, big))
}

pub fn word_end(text: &Text, at: usize, count: usize, big: bool) -> usize {
    (0..count).fold(at, |offset, _| next_word_end(text, offset, big))
}

pub fn find_in_line(
    text: &Text,
    at: usize,
    wanted: char,
    count: usize,
    forward: bool,
    till: bool,
) -> Option<usize> {
    let mut found = 0;
    if forward {
        for offset in at + 1..text.line_end(at) {
            if text.at(offset) == Some(wanted) {
                found += 1;
                if found == count {
                    return Some(if till { offset - 1 } else { offset });
                }
            }
        }
        return None;
    }
    for offset in (text.line_start(at)..at).rev() {
        if text.at(offset) == Some(wanted) {
            found += 1;
            if found == count {
                return Some(if till { offset + 1 } else { offset });
            }
        }
    }
    None
}

pub fn matching_bracket(text: &Text, at: usize) -> Option<usize> {
    for offset in at..text.line_end(at) {
        let character = text.at(offset)?;
        if let Some((open, close)) = PAIRS.iter().find(|(open, _)| *open == character) {
            return closer_for(text, offset, *open, *close);
        }
        if let Some((open, close)) = PAIRS.iter().find(|(_, close)| *close == character) {
            return opener_for(text, offset, *open, *close);
        }
    }
    None
}

pub fn closer_for(text: &Text, opener: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for offset in opener + 1..text.length() {
        let character = text.at(offset);
        if character == Some(open) {
            depth += 1;
        }
        if character == Some(close) {
            if depth == 0 {
                return Some(offset);
            }
            depth -= 1;
        }
    }
    None
}

pub fn opener_for(text: &Text, closer: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for offset in (0..closer).rev() {
        let character = text.at(offset);
        if character == Some(close) {
            depth += 1;
        }
        if character == Some(open) {
            if depth == 0 {
                return Some(offset);
            }
            depth -= 1;
        }
    }
    None
}

pub fn paragraph_forward(text: &Text, at: usize, count: usize) -> usize {
    let mut line = text.line_of(at);
    let last = text.line_count() - 1;
    for _ in 0..count {
        while line < last && blank(text, line) {
            line += 1;
        }
        while line < last && !blank(text, line) {
            line += 1;
        }
    }
    if line == last && !blank(text, line) {
        return text.length().saturating_sub(1);
    }
    text.start_of_line(line)
}

pub fn paragraph_backward(text: &Text, at: usize, count: usize) -> usize {
    let mut line = text.line_of(at);
    for _ in 0..count {
        while line > 0 && blank(text, line) {
            line -= 1;
        }
        while line > 0 && !blank(text, line) {
            line -= 1;
        }
    }
    text.start_of_line(line)
}

pub fn blank(text: &Text, line: usize) -> bool {
    text.is_blank_line(text.start_of_line(line))
}

fn next_word_start(text: &Text, at: usize, big: bool) -> usize {
    if at >= text.length() {
        return text.length();
    }
    let mut at = at;
    let started_on = kind(text.at(at), big);
    if started_on != 0 {
        while at < text.length() && kind(text.at(at), big) == started_on {
            at += 1;
        }
    }
    while at < text.length() && kind(text.at(at), big) == 0 {
        if text.at(at) == Some('\n') && text.at(at + 1) == Some('\n') {
            return at + 1;
        }
        at += 1;
    }
    at
}

fn previous_word_start(text: &Text, at: usize, big: bool) -> usize {
    if at == 0 {
        return 0;
    }
    let mut at = at - 1;
    while at > 0 && kind(text.at(at), big) == 0 {
        if text.at(at) == Some('\n') && text.at(at - 1) == Some('\n') {
            return at;
        }
        at -= 1;
    }
    let landed_on = kind(text.at(at), big);
    while at > 0 && kind(text.at(at - 1), big) == landed_on {
        at -= 1;
    }
    at
}

fn next_word_end(text: &Text, at: usize, big: bool) -> usize {
    if at + 1 >= text.length() {
        return text.length().saturating_sub(1);
    }
    let mut at = at + 1;
    while at < text.length() - 1 && kind(text.at(at), big) == 0 {
        at += 1;
    }
    let landed_on = kind(text.at(at), big);
    while at + 1 < text.length() && kind(text.at(at + 1), big) == landed_on {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_in(marked: &str) -> (Text, usize) {
        let at = marked
            .chars()
            .position(|character| character == '|')
            .unwrap();
        (Text::of(&marked.replace('|', "")), at)
    }

    fn marked(text: &Text, at: Option<usize>) -> Option<String> {
        at.map(|at| format!("{}|{}", text.slice(0, at), text.slice(at, text.length())))
    }

    fn moved(from: &str, motion: impl Fn(&Text, usize) -> Option<usize>) -> Option<String> {
        let (text, at) = cursor_in(from);
        marked(&text, motion(&text, at))
    }

    #[test]
    fn moves_left_and_right_without_leaving_the_line() {
        for (from, leftwards, count, to) in [
            ("sel|ect\nfrom", true, 1, "se|lect\nfrom"),
            ("sel|ect\nfrom", true, 9, "|select\nfrom"),
            ("sel|ect\nfrom", false, 2, "selec|t\nfrom"),
            ("sel|ect\nfrom", false, 9, "select|\nfrom"),
        ] {
            let landed = moved(from, |text, at| {
                Some(if leftwards {
                    left(text, at, count)
                } else {
                    right(text, at, count)
                })
            });
            assert_eq!(landed.as_deref(), Some(to), "{from}");
        }
    }

    #[test]
    fn moves_down_and_up_to_the_column_it_can() {
        for (from, by, column, to) in [
            ("sel|ect *\nfrom fruit", 1, 3, Some("select *\nfro|m fruit")),
            (
                "select *\nfrom fr|uit",
                -1,
                7,
                Some("select |*\nfrom fruit"),
            ),
            (
                "select *\nab\nfrom fruit|",
                -1,
                10,
                Some("select *\na|b\nfrom fruit"),
            ),
            (
                "select *\n\nfrom fruit|",
                -1,
                10,
                Some("select *\n|\nfrom fruit"),
            ),
            ("a|\nb\nc", 5, 0, Some("a\nb\n|c")),
            ("a\nb\n|c", 1, 0, None),
            ("|a\nb", -1, 0, None),
        ] {
            let landed = moved(from, |text, at| vertical(text, at, by, column));
            assert_eq!(landed.as_deref(), to, "{from}");
        }
    }

    #[test]
    fn finds_the_line_start_first_non_blank_and_last_character() {
        let from = "select\n    from fr|uit\nwhere";
        let start = moved(from, |text, at| Some(line_start(text, at)));
        let first = moved(from, |text, at| Some(first_non_blank(text, at)));
        let last = moved(from, |text, at| Some(last_character(text, at, 1)));
        let next_last = moved(from, |text, at| Some(last_character(text, at, 2)));

        assert_eq!(start.as_deref(), Some("select\n|    from fruit\nwhere"));
        assert_eq!(first.as_deref(), Some("select\n    |from fruit\nwhere"));
        assert_eq!(last.as_deref(), Some("select\n    from frui|t\nwhere"));
        assert_eq!(next_last.as_deref(), Some("select\n    from fruit\nwher|e"));
    }

    #[test]
    fn goes_to_a_numbered_line_at_its_first_non_blank() {
        let from = "|select\n  from fruit\nwhere";
        let second = moved(from, |text, _| Some(line(text, 1)));
        let beyond = moved(from, |text, _| Some(line(text, 99)));

        assert_eq!(second.as_deref(), Some("select\n  |from fruit\nwhere"));
        assert_eq!(beyond.as_deref(), Some("select\n  from fruit\n|where"));
    }

    #[test]
    fn moves_forward_by_word() {
        for (from, count, big, to) in [
            (
                "|select name from fruit",
                1,
                false,
                "select |name from fruit",
            ),
            (
                "|select name from fruit",
                3,
                false,
                "select name from |fruit",
            ),
            ("|fruit.name, qty", 1, false, "fruit|.name, qty"),
            ("fruit|.name, qty", 1, false, "fruit.|name, qty"),
            ("|fruit.name, qty", 1, true, "fruit.name, |qty"),
            ("sel|ect\n  from", 1, false, "select\n  |from"),
            ("sel|ect\n\nfrom", 1, false, "select\n|\nfrom"),
            ("select fr|uit", 1, false, "select fruit|"),
        ] {
            let landed = moved(from, |text, at| Some(word_forward(text, at, count, big)));
            assert_eq!(landed.as_deref(), Some(to), "{from}");
        }
    }

    #[test]
    fn moves_back_by_word() {
        for (from, count, big, to) in [
            (
                "select name from fr|uit",
                1,
                false,
                "select name from |fruit",
            ),
            (
                "select name from |fruit",
                2,
                false,
                "select |name from fruit",
            ),
            ("fruit.name|, qty", 1, false, "fruit.|name, qty"),
            ("fruit.name, q|ty", 2, true, "|fruit.name, qty"),
            ("select\n  |from", 1, false, "|select\n  from"),
            ("select\n\n|from", 1, false, "select\n|\nfrom"),
            ("|select", 1, false, "|select"),
        ] {
            let landed = moved(from, |text, at| Some(word_backward(text, at, count, big)));
            assert_eq!(landed.as_deref(), Some(to), "{from}");
        }
    }

    #[test]
    fn moves_to_the_end_of_a_word() {
        for (from, count, big, to) in [
            ("|select name", 1, false, "selec|t name"),
            ("selec|t name", 1, false, "select nam|e"),
            ("|fruit.name", 1, false, "frui|t.name"),
            ("|fruit.name x", 1, true, "fruit.nam|e x"),
            ("|select name from", 3, false, "select name fro|m"),
        ] {
            let landed = moved(from, |text, at| Some(word_end(text, at, count, big)));
            assert_eq!(landed.as_deref(), Some(to), "{from}");
        }
    }

    #[test]
    fn finds_a_character_in_the_line() {
        for (from, wanted, count, forward, till, to) in [
            (
                "|select a, b, c",
                ',',
                1,
                true,
                false,
                Some("select a|, b, c"),
            ),
            (
                "|select a, b, c",
                ',',
                2,
                true,
                false,
                Some("select a, b|, c"),
            ),
            (
                "|select a, b, c",
                ',',
                1,
                true,
                true,
                Some("select |a, b, c"),
            ),
            (
                "select a, b, |c",
                ',',
                1,
                false,
                false,
                Some("select a, b|, c"),
            ),
            (
                "select a, b, |c",
                ',',
                1,
                false,
                true,
                Some("select a, b,| c"),
            ),
            ("|select\na, b", ',', 1, true, false, None),
            ("|select", 'z', 1, true, false, None),
        ] {
            let landed = moved(from, |text, at| {
                find_in_line(text, at, wanted, count, forward, till)
            });
            assert_eq!(landed.as_deref(), to, "{from}");
        }
    }

    #[test]
    fn jumps_to_the_matching_bracket() {
        for (from, to) in [
            ("count|(id)", Some("count(id|)")),
            ("count(id|)", Some("count|(id)")),
            ("|count(id)", Some("count(id|)")),
            (
                "|where id in (\n  select (1)\n)",
                Some("where id in (\n  select (1)\n|)"),
            ),
            ("in (a, [b]|)", Some("in |(a, [b])")),
            ("|select 1", None),
            ("|count(id", None),
        ] {
            let landed = moved(from, matching_bracket);
            assert_eq!(landed.as_deref(), to, "{from}");
        }
    }

    #[test]
    fn moves_by_paragraph() {
        let from = "sel|ect\nfrom\n\nwhere\nand\n\nlimit";
        let forward = |count| moved(from, |text, at| Some(paragraph_forward(text, at, count)));
        assert_eq!(
            forward(1).as_deref(),
            Some("select\nfrom\n|\nwhere\nand\n\nlimit")
        );
        assert_eq!(
            forward(2).as_deref(),
            Some("select\nfrom\n\nwhere\nand\n|\nlimit")
        );
        assert_eq!(
            forward(3).as_deref(),
            Some("select\nfrom\n\nwhere\nand\n\nlimi|t")
        );

        let from = "select\nfrom\n\nwhere\nan|d";
        let backward = |count| moved(from, |text, at| Some(paragraph_backward(text, at, count)));
        assert_eq!(backward(1).as_deref(), Some("select\nfrom\n|\nwhere\nand"));
        assert_eq!(backward(2).as_deref(), Some("|select\nfrom\n\nwhere\nand"));
    }
}
