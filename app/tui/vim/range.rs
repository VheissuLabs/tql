#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: usize,
    pub end: usize,
    pub linewise: bool,
}

impl Range {
    pub fn new(start: usize, end: usize) -> Self {
        Range {
            start,
            end,
            linewise: false,
        }
    }

    pub fn lines(start: usize, end: usize) -> Self {
        Range {
            start,
            end,
            linewise: true,
        }
    }

    pub fn between(from: usize, to: usize, linewise: bool) -> Self {
        Range {
            start: from.min(to),
            end: from.max(to),
            linewise,
        }
    }
}
