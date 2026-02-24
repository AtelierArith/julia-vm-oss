use serde::{Deserialize, Serialize};

/// Source code span with byte offsets and 1-indexed line/column positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
}

impl Span {
    pub fn new(
        start: usize,
        end: usize,
        start_line: usize,
        end_line: usize,
        start_column: usize,
        end_column: usize,
    ) -> Self {
        Self {
            start,
            end,
            start_line,
            end_line,
            start_column,
            end_column,
        }
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }

    /// Create a Span from subset_julia_vm_parser's Span type.
    pub fn from_parser_span(span: &subset_julia_vm_parser::Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
            start_line: span.start_line,
            end_line: span.end_line,
            start_column: span.start_column,
            end_column: span.end_column,
        }
    }
}
