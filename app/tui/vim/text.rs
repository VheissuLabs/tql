pub struct Text {
    pub characters: Vec<char>,
}

impl Text {
    pub fn of(text: &str) -> Self {
        Text {
            characters: text.chars().collect(),
        }
    }

    pub fn length(&self) -> usize {
        self.characters.len()
    }

    pub fn at(&self, offset: usize) -> Option<char> {
        self.characters.get(offset).copied()
    }

    pub fn line_start(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.length());
        while offset > 0 && self.characters[offset - 1] != '\n' {
            offset -= 1;
        }
        offset
    }

    pub fn line_end(&self, offset: usize) -> usize {
        let mut offset = offset;
        while offset < self.length() && self.characters[offset] != '\n' {
            offset += 1;
        }
        offset
    }

    pub fn line_of(&self, offset: usize) -> usize {
        self.characters[..offset.min(self.length())]
            .iter()
            .filter(|character| **character == '\n')
            .count()
    }

    pub fn line_count(&self) -> usize {
        self.line_of(self.length()) + 1
    }

    pub fn start_of_line(&self, line: usize) -> usize {
        let mut offset = 0;
        for _ in 0..line {
            offset = self.line_end(offset) + 1;
        }
        offset.min(self.length())
    }

    pub fn is_blank_line(&self, offset: usize) -> bool {
        self.line_start(offset) == self.line_end(offset)
    }

    pub fn slice(&self, start: usize, end: usize) -> String {
        let end = end.min(self.length());
        if start >= end {
            return String::new();
        }
        self.characters[start..end].iter().collect()
    }
}
