//! Span information for source locations
//!
//! Provides precise source location tracking for error reporting and IDE integration.

use serde::{Deserialize, Serialize};

/// Represents a span in the source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Span {
    /// Byte offset start (0-indexed)
    pub start: usize,
    /// Byte offset end (exclusive)
    pub end: usize,
    /// Line number (1-indexed)
    pub start_line: usize,
    /// Line number (1-indexed)
    pub end_line: usize,
    /// Column (1-indexed, in bytes)
    pub start_column: usize,
    /// Column (1-indexed, in bytes)
    pub end_column: usize,
}

impl Span {
    /// Create a new span
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

    /// Create an empty span at position 0
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a span from byte offsets only (line/column will be computed later)
    pub fn from_offsets(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            start_line: 0,
            end_line: 0,
            start_column: 0,
            end_column: 0,
        }
    }

    /// Merge two spans into one that covers both
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            start_line: self.start_line.min(other.start_line),
            end_line: self.end_line.max(other.end_line),
            start_column: if self.start <= other.start {
                self.start_column
            } else {
                other.start_column
            },
            end_column: if self.end >= other.end {
                self.end_column
            } else {
                other.end_column
            },
        }
    }

    /// Get the length of the span in bytes
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Check if the span is empty
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Check if a byte offset is within this span
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// Helper struct to track line and column positions while lexing
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Line start offsets (byte positions where each line starts)
    line_starts: Vec<usize>,
}

impl SourceMap {
    /// Create a new source map from source code
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Get line and column for a byte offset
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        // Binary search for the line
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let column = offset - line_start + 1; // 1-indexed
        (line + 1, column) // 1-indexed line
    }

    /// Create a span with line/column information
    pub fn span(&self, start: usize, end: usize) -> Span {
        let (start_line, start_column) = self.line_col(start);
        let (end_line, end_column) = self.line_col(end);
        Span {
            start,
            end,
            start_line,
            end_line,
            start_column,
            end_column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_map() {
        let source = "hello\nworld\n";
        let map = SourceMap::new(source);

        assert_eq!(map.line_col(0), (1, 1)); // 'h'
        assert_eq!(map.line_col(5), (1, 6)); // '\n'
        assert_eq!(map.line_col(6), (2, 1)); // 'w'
        assert_eq!(map.line_col(11), (2, 6)); // '\n'
    }

    #[test]
    fn test_span_merge() {
        let span1 = Span::new(0, 5, 1, 1, 1, 6);
        let span2 = Span::new(10, 15, 2, 2, 5, 10);
        let merged = span1.merge(&span2);

        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 15);
        assert_eq!(merged.start_line, 1);
        assert_eq!(merged.end_line, 2);
    }
}
