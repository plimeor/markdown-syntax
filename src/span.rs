use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn contains(self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub const fn is_valid(self) -> bool {
        self.start <= self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinePosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut starts = Vec::new();
        starts.push(0);

        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\r' => {
                    if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                        index += 2;
                    } else {
                        index += 1;
                    }
                    starts.push(index);
                }
                b'\n' => {
                    index += 1;
                    starts.push(index);
                }
                _ => index += 1,
            }
        }

        Self {
            line_starts: starts,
            len: source.len(),
        }
    }

    pub fn position(&self, offset: usize) -> LinePosition {
        let offset = offset.min(self.len);
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];

        LinePosition {
            line: line_index + 1,
            column: offset.saturating_sub(line_start) + 1,
        }
    }

    pub fn span(&self, span: Span) -> (LinePosition, LinePosition) {
        (self.position(span.start), self.position(span.end))
    }
}
