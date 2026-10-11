use super::motions::{self, kind};
use super::range::Range;
use super::text::Text;

pub fn word(text: &Text, at: usize, around: bool, big: bool) -> Range {
    let line_start = text.line_start(at);
    let line_end = text.line_end(at);
    if line_start == line_end {
        return Range::new(at, at);
    }

    let at = at.min(line_end - 1);
    let under = kind(text.at(at), big);
    let start = extend_back(text, at, line_start, under, big);
    let end = extend_forward(text, at + 1, line_end, under, big);
    if !around {
        return Range::new(start, end);
    }
    if under == 0 {
        let following = kind(text.at(end), big);
        return Range::new(start, extend_forward(text, end, line_end, following, big));
    }

    let trailing = extend_forward(text, end, line_end, 0, big);
    if trailing > end {
        return Range::new(start, trailing);
    }
    Range::new(extend_back(text, start, line_start, 0, big), end)
}

pub fn bracket(text: &Text, at: usize, open: char, around: bool) -> Option<Range> {
    let close = match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    };
    let opener = if text.at(at) == Some(open) {
        at
    } else {
        motions::opener_for(text, at, open, close)?
    };
    let closer = motions::closer_for(text, opener, open, close)?;
    Some(if around {
        Range::new(opener, closer + 1)
    } else {
        Range::new(opener + 1, closer)
    })
}

pub fn quote(text: &Text, at: usize, mark: char, around: bool) -> Option<Range> {
    let line_start = text.line_start(at);
    let line_end = text.line_end(at);
    let marks: Vec<usize> = (line_start..line_end)
        .filter(|offset| text.at(*offset) == Some(mark))
        .collect();
    let (opener, closer) = quote_pair_for(&marks, at)?;
    if !around {
        return Some(Range::new(opener + 1, closer));
    }

    let is_space = |offset: usize| matches!(text.at(offset), Some(' ' | '\t'));
    let mut end = closer + 1;
    while end < line_end && is_space(end) {
        end += 1;
    }
    if end > closer + 1 {
        return Some(Range::new(opener, end));
    }

    let mut start = opener;
    while start > line_start && is_space(start - 1) {
        start -= 1;
    }
    Some(Range::new(start, end))
}

pub fn paragraph(text: &Text, at: usize, around: bool) -> Range {
    let line = text.line_of(at);
    let last = text.line_count() - 1;
    let blank = motions::blank(text, line);

    let mut first = line;
    let mut finale = line;
    while first > 0 && motions::blank(text, first - 1) == blank {
        first -= 1;
    }
    while finale < last && motions::blank(text, finale + 1) == blank {
        finale += 1;
    }

    if around {
        let before = finale;
        while finale < last && motions::blank(text, finale + 1) != blank {
            finale += 1;
        }
        while finale == before && first > 0 && motions::blank(text, first - 1) != blank {
            first -= 1;
        }
    }

    Range::lines(
        text.start_of_line(first),
        text.line_end(text.start_of_line(finale)),
    )
}

fn extend_back(text: &Text, at: usize, limit: usize, wanted: u8, big: bool) -> usize {
    let mut at = at;
    while at > limit && kind(text.at(at - 1), big) == wanted {
        at -= 1;
    }
    at
}

fn extend_forward(text: &Text, at: usize, limit: usize, wanted: u8, big: bool) -> usize {
    let mut at = at;
    while at < limit && kind(text.at(at), big) == wanted {
        at += 1;
    }
    at
}

fn quote_pair_for(marks: &[usize], at: usize) -> Option<(usize, usize)> {
    let pairs: Vec<(usize, usize)> = marks
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| (pair[0], pair[1]))
        .collect();
    pairs
        .iter()
        .find(|(opener, closer)| at >= *opener && at <= *closer)
        .or_else(|| pairs.iter().find(|(opener, _)| *opener > at))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_at(marked: &str) -> (Text, usize) {
        let at = marked
            .chars()
            .position(|character| character == '|')
            .unwrap();
        (Text::of(&marked.replace('|', "")), at)
    }

    fn covered(text: &Text, range: Option<Range>) -> Option<String> {
        range.map(|range| text.slice(range.start, range.end))
    }

    #[test]
    fn selects_a_word_inside_or_around() {
        for (from, around, big, expected) in [
            ("select na|me from", false, false, "name"),
            ("select na|me from", true, false, "name "),
            ("select name fr|om", true, false, " from"),
            ("fruit.na|me x", false, false, "name"),
            ("fruit.na|me x", false, true, "fruit.name"),
            ("select |  name", false, false, "   "),
            ("select |  name", true, false, "   name"),
        ] {
            let (text, at) = text_at(from);
            let selected = covered(&text, Some(word(&text, at, around, big)));
            assert_eq!(selected.as_deref(), Some(expected), "{from}");
        }
    }

    #[test]
    fn selects_inside_or_around_brackets_across_lines() {
        for (from, open, around, expected) in [
            ("count(i|d)", '(', false, Some("id")),
            ("count(i|d)", '(', true, Some("(id)")),
            ("coalesce(a, max(|b))", '(', false, Some("b")),
            ("coalesce(a, |max(b))", '(', false, Some("a, max(b)")),
            ("in (\n  sel|ect 1\n)", '(', false, Some("\n  select 1\n")),
            ("count|(id)", '(', false, Some("id")),
            ("count(id|)", '(', false, Some("id")),
            ("tags[|0]", '[', false, Some("0")),
            ("{\"a\": |1}", '{', true, Some("{\"a\": 1}")),
            ("sel|ect 1", '(', false, None),
        ] {
            let (text, at) = text_at(from);
            let selected = covered(&text, bracket(&text, at, open, around));
            assert_eq!(selected.as_deref(), expected, "{from}");
        }
    }

    #[test]
    fn selects_inside_or_around_quotes_on_the_line() {
        for (from, mark, around, expected) in [
            ("where name = 'ch|erry'", '\'', false, Some("cherry")),
            ("where name = 'ch|erry' and", '\'', true, Some("'cherry' ")),
            ("wh|ere name = 'cherry'", '\'', false, Some("cherry")),
            ("where a = 'x' and b = 'y|y'", '\'', false, Some("yy")),
            ("where a = |'x'", '\'', false, Some("x")),
            ("select \"na|me\"", '"', false, Some("name")),
            ("where name = 'ch|erry", '\'', false, None),
        ] {
            let (text, at) = text_at(from);
            let selected = covered(&text, quote(&text, at, mark, around));
            assert_eq!(selected.as_deref(), expected, "{from}");
        }
    }

    #[test]
    fn selects_a_paragraph_line_wise() {
        for (from, around, expected) in [
            ("select\nfr|om\n\nwhere", false, "select\nfrom"),
            ("select\nfr|om\n\n\nwhere", true, "select\nfrom\n\n"),
            ("select\n\nwh|ere\nand", true, "\nwhere\nand"),
        ] {
            let (text, at) = text_at(from);
            let range = paragraph(&text, at, around);
            assert!(range.linewise);
            assert_eq!(text.slice(range.start, range.end), expected, "{from}");
        }
    }
}
