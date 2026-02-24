//! Regex value types for Julia's Regex and RegexMatch.
//!
//! This module provides:
//! - `RegexValue`: A compiled regex pattern (Julia's `Regex` type)
//! - `RegexMatchValue`: The result of a regex match (Julia's `RegexMatch` type)
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use regex::Regex;
use std::sync::Arc;

/// A compiled regular expression (Julia's `Regex` type).
///
/// In Julia, Regex is created via:
/// - `r"pattern"` (regex literal)
/// - `r"pattern"i` (case-insensitive)
/// - `r"pattern"imsx` (with multiple flags)
/// - `Regex("pattern")` (constructor)
#[derive(Debug, Clone)]
pub struct RegexValue {
    /// The compiled regex
    pub regex: Arc<Regex>,
    /// The original pattern string
    pub pattern: String,
    /// The flags used (i, m, s, x)
    pub flags: String,
}

impl RegexValue {
    /// Create a new RegexValue from a pattern and flags.
    ///
    /// Flags:
    /// - `i`: case-insensitive (PCRE2_CASELESS)
    /// - `m`: multiline (PCRE2_MULTILINE) - ^ and $ match line boundaries
    /// - `s`: dotall (PCRE2_DOTALL) - . matches newlines
    /// - `x`: extended (PCRE2_EXTENDED) - free-spacing mode
    pub fn new(pattern: &str, flags: &str) -> Result<Self, String> {
        // Build regex pattern with flags
        // Rust's regex crate uses inline flags: (?i), (?m), (?s), (?x)
        let mut prefix = String::new();

        for c in flags.chars() {
            match c {
                'i' => prefix.push_str("(?i)"),
                'm' => prefix.push_str("(?m)"),
                's' => prefix.push_str("(?s)"),
                'x' => prefix.push_str("(?x)"),
                _ => return Err(format!("Unknown regex flag: {}", c)),
            }
        }

        let full_pattern = format!("{}{}", prefix, pattern);

        match Regex::new(&full_pattern) {
            Ok(regex) => Ok(RegexValue {
                regex: Arc::new(regex),
                pattern: pattern.to_string(),
                flags: flags.to_string(),
            }),
            Err(e) => Err(format!("Invalid regex pattern: {}", e)),
        }
    }

    /// Check if a string matches this regex.
    pub fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    /// Find the first match of this regex in the string.
    pub fn find(&self, text: &str) -> Option<RegexMatchValue> {
        self.regex.captures(text).and_then(|caps| {
            let full_match = caps.get(0)?; // Group 0 is guaranteed by regex crate
            let offset = full_match.start() + 1; // Julia uses 1-based indexing

            // Collect capture groups (excluding the full match at index 0)
            let mut captures = Vec::new();
            let mut offsets = Vec::new();

            for i in 1..caps.len() {
                if let Some(m) = caps.get(i) {
                    captures.push(Some(m.as_str().to_string()));
                    offsets.push(m.start() as i64 + 1); // 1-based
                } else {
                    captures.push(None);
                    offsets.push(0); // 0 indicates no match
                }
            }

            Some(RegexMatchValue {
                match_str: full_match.as_str().to_string(),
                captures,
                offset: offset as i64,
                offsets,
            })
        })
    }

    /// Find all non-overlapping matches of this regex in the string.
    pub fn find_all(&self, text: &str) -> Vec<RegexMatchValue> {
        self.regex
            .captures_iter(text)
            .filter_map(|caps| {
                let full_match = caps.get(0)?; // Group 0 is guaranteed by regex crate
                let offset = full_match.start() + 1;

                let mut captures = Vec::new();
                let mut offsets = Vec::new();

                for i in 1..caps.len() {
                    if let Some(m) = caps.get(i) {
                        captures.push(Some(m.as_str().to_string()));
                        offsets.push(m.start() as i64 + 1);
                    } else {
                        captures.push(None);
                        offsets.push(0);
                    }
                }

                Some(RegexMatchValue {
                    match_str: full_match.as_str().to_string(),
                    captures,
                    offset: offset as i64,
                    offsets,
                })
            })
            .collect()
    }

    /// Replace all occurrences of the pattern with a replacement string.
    pub fn replace_all(&self, text: &str, replacement: &str) -> String {
        self.regex.replace_all(text, replacement).to_string()
    }

    /// Replace the first occurrence of the pattern with a replacement string.
    pub fn replace(&self, text: &str, replacement: &str) -> String {
        self.regex.replace(text, replacement).to_string()
    }

    /// Replace at most `limit` occurrences of the pattern with a replacement string.
    pub fn replacen(&self, text: &str, limit: usize, replacement: &str) -> String {
        self.regex.replacen(text, limit, replacement).to_string()
    }

    /// Split the string by this regex pattern.
    pub fn split<'a>(&self, text: &'a str) -> Vec<&'a str> {
        self.regex.split(text).collect()
    }
}

impl PartialEq for RegexValue {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern && self.flags == other.flags
    }
}

/// The result of a regex match (Julia's `RegexMatch` type).
///
/// In Julia:
/// ```julia
/// m = match(r"(\d+)", "abc123")
/// m.match     # "123"
/// m.captures  # ("123",)
/// m.offset    # 4
/// m.offsets   # (4,)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct RegexMatchValue {
    /// The matched substring
    pub match_str: String,
    /// Captured groups (None if group didn't participate in match)
    pub captures: Vec<Option<String>>,
    /// Starting position of the match (1-based)
    pub offset: i64,
    /// Starting positions of each capture group (1-based, 0 if not matched)
    pub offsets: Vec<i64>,
}

impl RegexMatchValue {
    /// Get a captured group by index (0 = full match, 1+ = capture groups).
    pub fn get(&self, index: usize) -> Option<&str> {
        if index == 0 {
            Some(&self.match_str)
        } else if index <= self.captures.len() {
            self.captures[index - 1].as_deref()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_basic_match() {
        let re = RegexValue::new(r"\d+", "").unwrap();
        assert!(re.is_match("abc123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn test_regex_case_insensitive() {
        let re = RegexValue::new(r"hello", "i").unwrap();
        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
    }

    #[test]
    fn test_regex_find() {
        let re = RegexValue::new(r"(\d+)", "").unwrap();
        let m = re.find("abc123def").unwrap();
        assert_eq!(m.match_str, "123");
        assert_eq!(m.offset, 4);
        assert_eq!(m.captures, vec![Some("123".to_string())]);
    }

    #[test]
    fn test_regex_find_all() {
        let re = RegexValue::new(r"\d+", "").unwrap();
        let matches = re.find_all("a1b2c3");
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].match_str, "1");
        assert_eq!(matches[1].match_str, "2");
        assert_eq!(matches[2].match_str, "3");
    }

    #[test]
    fn test_regex_replace() {
        let re = RegexValue::new(r"\d+", "").unwrap();
        assert_eq!(re.replace("a1b2c3", "X"), "aXb2c3");
        assert_eq!(re.replace_all("a1b2c3", "X"), "aXbXcX");
    }

    #[test]
    fn test_regex_split() {
        let re = RegexValue::new(r",\s*", "").unwrap();
        let parts = re.split("a, b,  c");
        assert_eq!(parts, vec!["a", "b", "c"]);
    }
}
