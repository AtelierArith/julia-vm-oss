//! Regex value types for Julia's Regex and RegexMatch.
//!
//! This module provides:
//! - `RegexValue`: A compiled regex pattern (Julia's `Regex` type)
//! - `RegexMatchValue`: The result of a regex match (Julia's `RegexMatch` type)
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use fancy_regex::Regex;
use std::sync::Arc;

use super::array_element::ArrayElementType;
use super::array_value::{native_array_value_from_array, ArrayValue};
use super::value_enum::Value;
use crate::error::VmError;
use subset_julia_vm_types::types::JuliaType;

/// PCRE2 horizontal-whitespace set (`\h`), as character-class *body* (no
/// enclosing brackets): HT, space, NBSP, and the Unicode space separators.
/// Hex escapes are used throughout (`\x20` for space) so the body is safe to
/// splice into a pattern regardless of the extended (`x`) free-spacing flag.
const PCRE2_HORIZONTAL_WS: &str =
    r"\x09\x20\x{a0}\x{1680}\x{2000}-\x{200a}\x{202f}\x{205f}\x{3000}";
/// PCRE2 vertical-whitespace set (`\v`), as character-class *body*: LF, VT, FF,
/// CR, U+0085 (NEL), U+2028 (LS), U+2029 (PS).
const PCRE2_VERTICAL_WS: &str = r"\x0a\x0b\x0c\x0d\x{85}\x{2028}\x{2029}";

/// Count the capturing groups (numbered + named) in `pattern`, so a bare
/// `\ddd` escape can be disambiguated between a back reference and an octal
/// character exactly the way PCRE2 does. Escaped characters and character
/// classes are skipped; `(?:`, lookarounds, inline flags, etc. are
/// non-capturing, while `(?<name>` / `(?'name'` / `(?P<name>` are capturing.
fn count_capture_groups(chars: &[char]) -> usize {
    let mut count = 0usize;
    let mut i = 0usize;
    let mut class_depth = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            // Skip the escaped character (backslash + one codepoint).
            i += 2;
            continue;
        }
        if class_depth > 0 {
            match c {
                '[' => class_depth += 1,
                ']' => class_depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            '[' => {
                class_depth += 1;
                i += 1;
            }
            '(' => {
                if chars.get(i + 1) == Some(&'?') {
                    // (?...) — capturing only for named-group forms.
                    let is_named = match chars.get(i + 2) {
                        // (?<name>  but NOT (?<=  or (?<!
                        Some('<') => !matches!(chars.get(i + 3), Some('=') | Some('!')),
                        Some('\'') => true,
                        Some('P') => chars.get(i + 3) == Some(&'<'),
                        _ => false,
                    };
                    if is_named {
                        count += 1;
                    }
                } else {
                    count += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    count
}

/// Read up to `max` octal digits (0-7) starting at `start`; returns the value
/// and the index just past the consumed digits. Reads zero digits (value 0) if
/// `start` is not an octal digit.
fn read_octal(chars: &[char], start: usize, max: usize) -> (u32, usize) {
    let mut val: u32 = 0;
    let mut i = start;
    let mut n = 0usize;
    while i < chars.len() && n < max {
        match chars[i].to_digit(8) {
            Some(d) => {
                // Saturating: an over-long \o{...} value stays a large u32 and
                // is later rejected by the engine (matching PCRE2, which errors
                // on out-of-range \o{}), rather than overflow-panicking in a
                // debug build.
                val = val.saturating_mul(8).saturating_add(d);
                i += 1;
                n += 1;
            }
            None => break,
        }
    }
    (val, i)
}

/// Rewrite the PCRE2 escape sequences that upstream Julia accepts but
/// `fancy-regex` (regex-syntax semantics) does not handle compatibly, into
/// forms `fancy-regex` compiles to the same match set. This runs once at
/// `Regex` construction, before compilation (Issues #10179, #10180, #10203):
///
/// - Octal `\ddd`, `\0`, `\o{...}` and control `\cX` → `\x{..}`
///   (`fancy-regex` otherwise treats every `\ddd` as a back reference and
///   rejects `\o` / `\c`). Genuine back references (`\1` with a matching
///   group) are left untouched.
/// - `\v` / `\V` → the PCRE2 vertical-whitespace class / its complement
///   (`fancy-regex` maps `\v` to the single char U+000B).
/// - `\h` / `\H` → the PCRE2 horizontal-whitespace class / its complement
///   (`fancy-regex` maps `\h` to the hex-digit class `[0-9A-Fa-f]`).
///
/// The three whitespace-class rewrites also apply inside `[...]` character
/// classes; `\H` / `\V` inside a class become a nested negated class, which
/// `fancy-regex` supports. Hex (`\xHH`, `\x{...}`), `\u`, `\U`, `\p{...}` and
/// all other escapes are copied through verbatim.
fn rewrite_pcre2_escapes(pattern: &str) -> String {
    // Fast path: nothing to do if there is no backslash at all.
    if !pattern.contains('\\') {
        return pattern.to_string();
    }
    let chars: Vec<char> = pattern.chars().collect();
    let group_count = count_capture_groups(&chars);
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut i = 0usize;
    let mut class_depth = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' {
            match c {
                '[' => class_depth += 1,
                ']' => {
                    class_depth = class_depth.saturating_sub(1);
                }
                _ => {}
            }
            out.push(c);
            i += 1;
            continue;
        }
        // c == '\\': inspect the escaped character.
        let Some(&e) = chars.get(i + 1) else {
            // Trailing backslash: copy verbatim, let the engine report it.
            out.push('\\');
            i += 1;
            continue;
        };
        let in_class = class_depth > 0;
        match e {
            'h' | 'H' | 'v' | 'V' => {
                let body = if e == 'h' || e == 'H' {
                    PCRE2_HORIZONTAL_WS
                } else {
                    PCRE2_VERTICAL_WS
                };
                let negated = e.is_ascii_uppercase();
                if !in_class {
                    out.push('[');
                    if negated {
                        out.push('^');
                    }
                    out.push_str(body);
                    out.push(']');
                } else if negated {
                    // Negated class inside a class: nest a negated class,
                    // which fancy-regex/regex-syntax supports.
                    out.push_str("[^");
                    out.push_str(body);
                    out.push(']');
                } else {
                    out.push_str(body);
                }
                i += 2;
            }
            'o' => {
                // \o{ddd...} octal escape.
                if chars.get(i + 2) == Some(&'{') {
                    let (val, end) = read_octal(&chars, i + 3, usize::MAX);
                    if end > i + 3 && chars.get(end) == Some(&'}') {
                        out.push_str(&format!("\\x{{{:x}}}", val));
                        i = end + 1;
                        continue;
                    }
                }
                // Malformed \o: copy verbatim (engine will report the error).
                out.push('\\');
                out.push('o');
                i += 2;
            }
            'c' => {
                // \cX control escape: uppercase then XOR 0x40 (ASCII only).
                match chars.get(i + 2) {
                    Some(&x) if x.is_ascii() => {
                        let code = (x.to_ascii_uppercase() as u32) ^ 0x40;
                        out.push_str(&format!("\\x{{{:x}}}", code));
                        i += 3;
                    }
                    _ => {
                        out.push('\\');
                        out.push('c');
                        i += 2;
                    }
                }
            }
            '0'..='9' => {
                if in_class {
                    // Inside a character class every \ddd is octal (no back
                    // references). 8/9 are not octal — copy them verbatim.
                    if ('0'..='7').contains(&e) {
                        let (val, end) = read_octal(&chars, i + 1, 3);
                        out.push_str(&format!("\\x{{{:x}}}", val));
                        i = end;
                    } else {
                        out.push('\\');
                        out.push(e);
                        i += 2;
                    }
                } else if e == '0' {
                    // Leading zero is always octal (up to 3 digits incl. the 0).
                    let (val, end) = read_octal(&chars, i + 1, 3);
                    out.push_str(&format!("\\x{{{:x}}}", val));
                    i = end;
                } else {
                    // \1..\9 first digit: read the whole decimal run.
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    let digits: String = chars[i + 1..j].iter().collect();
                    let decimal: usize = digits.parse().unwrap_or(usize::MAX);
                    if digits.len() == 1 || decimal <= group_count {
                        // Back reference: leave untouched for fancy-regex.
                        out.push('\\');
                        out.push_str(&digits);
                        i = j;
                    } else {
                        // Octal: re-read up to 3 octal digits from the start;
                        // any following digits are literal characters.
                        let (val, end) = read_octal(&chars, i + 1, 3);
                        out.push_str(&format!("\\x{{{:x}}}", val));
                        for &d in &chars[end..j] {
                            out.push(d);
                        }
                        i = j;
                    }
                }
            }
            _ => {
                // Any other escape (\x, \u, \U, \p, \d, \., \\, ...): copy the
                // backslash and its escaped codepoint through unchanged.
                out.push('\\');
                out.push(e);
                i += 2;
            }
        }
    }
    out
}

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

        // Reject PCRE2 pattern-recursion / subroutine-call constructs up front.
        // fancy-regex has no recursion support: `(?1)` fails to compile with an
        // opaque "Unknown group flag" message, while `(?R)` compiles to something
        // else entirely and *silently returns a wrong match* (Issue #10181). A
        // silent mis-match is worse than a hard failure, so until real recursion
        // support exists we turn every recursion construct into a clear,
        // documented compile error. See docs/vm/REGEX_PCRE2_PARITY.md.
        if let Some(construct) = detect_regex_recursion(pattern) {
            return Err(format!(
                "regex recursion is not supported: {} (Issue #10181)",
                construct
            ));
        }

        // Translate the PCRE2 escape sequences that upstream Julia accepts but
        // fancy-regex does not handle compatibly (octal / \o{} / \cX, \v/\V,
        // \h/\H) before compilation. The original pattern text is preserved on
        // the value (for `.pattern` and equality); only the compiled form is
        // rewritten (Issues #10179, #10180, #10203).
        let full_pattern = format!("{}{}", prefix, rewrite_pcre2_escapes(pattern));

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
        self.regex.is_match(text).unwrap_or(false)
    }

    /// Test whether this pattern matches ending exactly at the end of `text`.
    ///
    /// Emulates PCRE's `ENDANCHORED` option, which Julia's `endswith(s, ::Regex)`
    /// uses (Issue #5676), by anchoring the pattern with a trailing end-of-text
    /// `$`. The `regex` crate's `$` matches only the very end of the haystack in
    /// the default (non-multiline) mode, which is exactly the desired anchor; the
    /// original flags are preserved. (Patterns compiled with the `m` flag retain
    /// line-boundary `$` semantics — a known divergence from PCRE ENDANCHORED.)
    pub fn ends_with_match(&self, text: &str) -> bool {
        match RegexValue::new(&format!("(?:{})$", self.pattern), &self.flags) {
            Ok(anchored) => anchored.is_match(text),
            Err(_) => false,
        }
    }

    /// Names of the capture groups, parallel to `RegexMatchValue::captures`
    /// (index `i` is the name of capture group `i + 1`, or `None` when that
    /// group is unnamed). Group 0 (the full match) is intentionally excluded.
    ///
    /// Mirrors upstream `PCRE.capture_names`, which sjulia uses for named-group
    /// access (`m[:name]` / `keys(m)` / `haskey(m, name)`, Issue #10173).
    fn capture_name_table(&self) -> Vec<Option<String>> {
        // fancy-regex yields one entry per group starting at group 0; skip it so
        // the table lines up with `captures` (which starts at group 1).
        self.regex
            .capture_names()
            .skip(1)
            .map(|name| name.map(str::to_string))
            .collect()
    }

    /// Build a `RegexMatchValue` from a fancy-regex `Captures`, translating
    /// byte offsets to Julia's 1-based indexing (0 marks a non-participating
    /// capture group). Shared by `find`/`find_all`/`find_all_overlapping`.
    /// `capture_names` is the parallel name table for the compiled pattern
    /// (Issue #10173), captured once by the caller. `regex` is the originating
    /// `RegexValue`, stored verbatim as the match's `regex` field (Issue #11382).
    fn match_value_from_captures(
        caps: &fancy_regex::Captures<'_>,
        capture_names: &[Option<String>],
        regex: &RegexValue,
    ) -> Option<RegexMatchValue> {
        let full_match = caps.get(0)?; // Group 0 is guaranteed by regex engines
        let offset = full_match.start() as i64 + 1; // Julia uses 1-based indexing

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
            offset,
            offsets,
            capture_names: capture_names.to_vec(),
            regex: regex.clone(),
        })
    }

    /// Find the first match of this regex in the string.
    pub fn find(&self, text: &str) -> Option<RegexMatchValue> {
        let capture_names = self.capture_name_table();
        self.regex
            .captures(text)
            .ok()
            .flatten()
            .and_then(|caps| Self::match_value_from_captures(&caps, &capture_names, self))
    }

    /// Find the first match of this regex at or after a 0-based byte offset.
    ///
    /// Mirrors Julia's 3-arg `match(re, s, i)`, which searches starting from
    /// byte index `i` (1-based); the caller subtracts 1 to obtain the 0-based
    /// `start_byte`. Uses `captures_from_pos`, which keeps the *whole* string
    /// as context (so anchors like `^`/`\b` behave as in PCRE) and reports
    /// capture positions as absolute byte offsets — hence the returned
    /// `RegexMatchValue.offset`/`offsets` are absolute 1-based offsets into
    /// `text`, exactly like `find` (Issue #10178). Returns `None` when
    /// `start_byte` is past the end of `text` (matching `captures_from_pos`).
    pub fn find_from(&self, text: &str, start_byte: usize) -> Option<RegexMatchValue> {
        let capture_names = self.capture_name_table();
        self.regex
            .captures_from_pos(text, start_byte)
            .ok()
            .flatten()
            .and_then(|caps| Self::match_value_from_captures(&caps, &capture_names, self))
    }

    /// Find all non-overlapping matches of this regex in the string.
    pub fn find_all(&self, text: &str) -> Vec<RegexMatchValue> {
        let capture_names = self.capture_name_table();
        self.regex
            .captures_iter(text)
            .filter_map(|caps| Self::match_value_from_captures(&caps.ok()?, &capture_names, self))
            .collect()
    }

    /// Find all matches, allowing overlaps — Julia's
    /// `eachmatch(re, s; overlap=true)` (Issue #10199).
    ///
    /// Mirrors the `overlap=true` branch of upstream `Base.RegexMatchIterator`
    /// (`julia/base/regex.jl`): after each match the search restarts one
    /// character past the match START (`nextind(s, m.offset)`) rather than past
    /// the match END, so successive matches may share indices. Empty matches
    /// also advance by one character, so an empty-capable pattern cannot loop
    /// forever at the same position. The full-string context is preserved
    /// (`captures_from_pos` searches the whole haystack from `pos`), so
    /// anchors and look-behind keep working across restarts.
    pub fn find_all_overlapping(&self, text: &str) -> Vec<RegexMatchValue> {
        let capture_names = self.capture_name_table();
        let mut result = Vec::new();
        let mut pos = 0usize; // 0-based byte offset (upstream 1-based offset - 1)
        while pos <= text.len() {
            let caps = match self.regex.captures_from_pos(text, pos) {
                Ok(Some(caps)) => caps,
                _ => break, // no further match, or an engine error
            };
            let Some(full_match) = caps.get(0) else { break };
            let start = full_match.start();
            let Some(m) = Self::match_value_from_captures(&caps, &capture_names, self) else {
                break;
            };
            result.push(m);
            // Restart one character past the match START. `start` is always a
            // char boundary (matches begin on one), so this is `nextind`.
            pos = next_char_boundary(text, start);
        }
        result
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

    /// Split the string by this regex pattern (unlimited, keep empty parts).
    pub fn split<'a>(&self, text: &'a str) -> Vec<&'a str> {
        self.split_with(text, 0, true)
    }

    /// Split the string by this regex pattern with `limit`/`keepempty` support,
    /// mirroring upstream Julia's `SplitIterator` semantics (Issue #10176).
    ///
    /// - `limit <= 0` means no limit; `limit == N > 0` yields at most `N`
    ///   substrings (after `limit - 1` splits the remainder becomes the last
    ///   part).
    /// - `keepempty == true` keeps empty substrings between adjacent matches
    ///   and at the start/end; `keepempty == false` drops them.
    ///
    /// The returned slices borrow `text`, so they can be materialized as
    /// `SubString{String}` on the Julia side.
    pub fn split_with<'a>(&self, text: &'a str, limit: i64, keepempty: bool) -> Vec<&'a str> {
        let n = text.len();
        // Byte ranges of the non-overlapping matches, in order. fancy-regex's
        // `find_iter` advances past empty matches, matching upstream's
        // `findnext`-from-`k` enumeration.
        let matches: Vec<(usize, usize)> = self
            .regex
            .find_iter(text)
            .filter_map(|m| m.ok().map(|m| (m.start(), m.end())))
            .collect();

        let mut result: Vec<&'a str> = Vec::new();
        let mut i: usize = 0; // start byte of the current segment
        let mut count: i64 = 0; // number of substrings emitted so far
        let mut idx = 0; // index into `matches`

        loop {
            let mut emitted = false;
            while idx < matches.len() {
                // Stop splitting once `limit - 1` splits have been made.
                if limit > 0 && count == limit - 1 {
                    break;
                }
                let (ms, me) = matches[idx];
                // A match starting at/after the end can't produce another split.
                if ms >= n {
                    idx = matches.len();
                    break;
                }
                if i < me {
                    if keepempty || i < ms {
                        result.push(&text[i..ms]);
                        count += 1;
                        i = me;
                        idx += 1;
                        emitted = true;
                        break;
                    } else {
                        // Empty segment with keepempty=false: skip it.
                        i = me;
                    }
                }
                idx += 1;
            }
            if emitted {
                continue;
            }
            // No further split. Emit the trailing segment unless it is an empty
            // tail dropped by keepempty=false.
            if !keepempty && i >= n {
                break;
            }
            result.push(&text[i..]);
            break;
        }
        result
    }
}

/// Byte index of the character boundary immediately after `byte_pos`
/// (`nextind` semantics). Returns `text.len() + 1` once at or past the end so
/// the overlapping-match loop terminates instead of re-matching an empty match.
fn next_char_boundary(text: &str, byte_pos: usize) -> usize {
    if byte_pos >= text.len() {
        return text.len() + 1;
    }
    let mut i = byte_pos + 1;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Detect PCRE2 pattern-recursion / subroutine-call constructs that
/// `fancy-regex` cannot handle (Issue #10181).
///
/// Returns `Some(construct)` naming the offending token if the pattern uses any
/// of:
/// - `(?R)` / `(?0)` — whole-pattern recursion,
/// - `(?n)` — numbered subroutine call (e.g. `(?1)`, `(?12)`),
/// - `(?+n)` / `(?-n)` — relative subroutine call,
/// - `(?&name)` / `(?P>name)` — named subroutine call.
///
/// The scan is escape- and character-class-aware so that a literal `\(?R\)` or a
/// class such as `[(?R)]` is not mistaken for a group. It deliberately does NOT
/// flag legitimate look-alikes: non-capturing `(?:…)`, inline flags `(?i)` /
/// `(?-i)`, lookaround `(?=…)` / `(?!…)` / `(?<=…)` / `(?<!…)`, atomic groups
/// `(?>…)`, comments `(?#…)`, conditionals `(?(1)…)`, or named captures
/// `(?<name>…)` / `(?P<name>…)` / `(?'name'…)`.
pub fn detect_regex_recursion(pattern: &str) -> Option<String> {
    let bytes = pattern.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut in_class = false;

    while i < n {
        let c = bytes[i];
        if c == b'\\' {
            // Skip the escaped character (e.g. `\(`), so escaped parens never
            // open a group.
            i += 2;
            continue;
        }
        if in_class {
            if c == b']' {
                in_class = false;
            }
            i += 1;
            continue;
        }
        if c == b'[' {
            in_class = true;
            i += 1;
            continue;
        }
        if c == b'(' && bytes.get(i + 1) == Some(&b'?') {
            // `(?#...)` comment group: its body is ignored by the engine, so
            // recursion-looking text inside a comment must NOT be flagged. PCRE
            // comments end at the first `)`, including when it is preceded by a
            // backslash.
            if bytes.get(i + 2) == Some(&b'#') {
                let mut j = i + 3;
                while j < n {
                    if bytes[j] == b')' {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            if let Some(construct) = classify_recursion_group(&pattern[i + 2..]) {
                return Some(construct);
            }
        }
        i += 1;
    }
    None
}

/// Given the text immediately following an unescaped `(?`, classify whether it
/// begins a recursion / subroutine-call construct, returning the reconstructed
/// token for the error message.
fn classify_recursion_group(rest: &str) -> Option<String> {
    let b = rest.as_bytes();
    if b.is_empty() {
        return None;
    }

    // (?R) — whole-pattern recursion.
    if b[0] == b'R' && b.get(1) == Some(&b')') {
        return Some("(?R)".to_string());
    }

    // (?P>name) — named subroutine call (Python syntax). Note: `(?P<name>…)` is
    // a named capture and `(?P=name)` a named backreference, neither of which
    // is recursion.
    if rest.starts_with("P>") {
        return Some("(?P>…)".to_string());
    }

    // (?&name) — named subroutine call.
    if b[0] == b'&' {
        return Some("(?&…)".to_string());
    }

    // (?n) / (?0) / (?+n) / (?-n) — numbered or relative subroutine call. An
    // optional sign followed by one or more digits and a closing `)`. Requiring
    // digits after the sign keeps inline flag toggles like `(?-i)` out.
    let mut j = 0;
    if b[0] == b'+' || b[0] == b'-' {
        j = 1;
    }
    let digits_start = j;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j > digits_start && b.get(j) == Some(&b')') {
        return Some(format!("(?{})", &rest[..j]));
    }

    None
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
/// m.captures  # Union{Nothing, SubString{String}}["123"]
/// m.offset    # 4
/// m.offsets   # [4]
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
    /// Names of the capture groups, parallel to `captures` (`None` = unnamed).
    /// Populated from the compiled regex so named-group access
    /// (`m[:name]` / `keys(m)` / `haskey(m, name)`) works (Issue #10173).
    pub capture_names: Vec<Option<String>>,
    /// The `Regex` that produced this match, upstream's 5th physical field
    /// (`m.regex`). Cloning is cheap: the compiled matcher is `Arc`-shared,
    /// only `pattern`/`flags` (two `String`s) are duplicated (Issue #11382).
    pub regex: RegexValue,
}

/// Upstream `fieldnames(RegexMatch)`, in declaration order (Issue #11382):
/// `julia/base/regex.jl`'s `struct RegexMatch{S<:AbstractString} <: AbstractMatch`.
pub const REGEXMATCH_FIELD_NAMES: [&str; 5] = ["match", "captures", "offset", "offsets", "regex"];

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

    /// Resolve a named capture group to its 1-based capture index, mirroring
    /// upstream `PCRE.substring_number_from_name` (Issue #10173). Returns `None`
    /// when no capture group carries `name`.
    pub fn capture_index_by_name(&self, name: &str) -> Option<usize> {
        self.capture_names
            .iter()
            .position(|group| group.as_deref() == Some(name))
            .map(|zero_based| zero_based + 1)
    }

    /// Build the upstream `Vector{Union{Nothing,SubString{String}}}` value for
    /// the `captures` field (Issue #10182). Upstream declares the field type
    /// as that union regardless of the actual contents, so the tag is fixed.
    /// sjulia has no distinct `SubString` runtime type — matched groups are
    /// `String` values under the display-only `SubString{String}` union tag,
    /// and unmatched groups are `nothing`.
    fn captures_vector_value(&self) -> Result<Value, VmError> {
        let element_type = ArrayElementType::UnionOf(vec![
            JuliaType::Nothing,
            JuliaType::Struct("SubString{String}".to_string()),
        ]);
        let mut arr = ArrayValue::memory_first_with_capacity(element_type, self.captures.len());
        for capture in &self.captures {
            let value = match capture {
                Some(text) => Value::str_new(text.clone()),
                None => Value::Nothing,
            };
            arr.push(value)?;
        }
        Ok(native_array_value_from_array(arr))
    }

    /// Build the upstream `Vector{Int}` value for the `offsets` field (Issue
    /// #10182): the 1-based start of each capture group, `0` when the group
    /// did not participate in the match.
    fn offsets_vector_value(&self) -> Value {
        let len = self.offsets.len();
        native_array_value_from_array(ArrayValue::memory_first_from_i64(
            self.offsets.clone(),
            vec![len],
        ))
    }

    /// Project one of `RegexMatch`'s five upstream physical fields by name
    /// (Issue #11382). Centralized here so `getfield`/`getproperty`/dot-access
    /// callers cannot drift apart (mirrors `BindingValue::field_by_name`).
    /// Returns `Ok(None)` for any other name (-> `FieldError` upstream).
    pub fn field_by_name(&self, field_name: &str) -> Result<Option<Value>, VmError> {
        Ok(match field_name {
            "match" => Some(Value::str_new(self.match_str.clone())),
            "captures" => Some(self.captures_vector_value()?),
            "offset" => Some(Value::I64(self.offset)),
            "offsets" => Some(self.offsets_vector_value()),
            "regex" => Some(Value::Regex(Box::new(self.regex.clone()))),
            _ => None,
        })
    }

    /// Project one of `RegexMatch`'s five upstream physical fields by 0-based
    /// positional index (Issue #11382); upstream `getfield(m, i)` is 1-based —
    /// callers subtract 1 before calling this. Returns `Ok(None)` for any
    /// other index (-> `FieldIndexOutOfBounds`/`BoundsError` upstream).
    pub fn field_by_index(&self, field_idx: usize) -> Result<Option<Value>, VmError> {
        match field_idx {
            0..=4 => self.field_by_name(REGEXMATCH_FIELD_NAMES[field_idx]),
            _ => Ok(None),
        }
    }
}

/// Resolve a named capture group of `re` to its 1-based-in-Julia group index
/// (0 = whole match). Returns `None` if no group has that name.
///
/// `Regex::capture_names()` yields one entry per group in order, index 0 being
/// the whole match (name `None`), so the iterator position is exactly the group
/// number.
fn capture_index_by_name(re: &RegexValue, name: &str) -> Option<usize> {
    re.regex.capture_names().position(|n| n == Some(name))
}

/// Context a `SubstitutionString` is expanded against: a full regex match (with
/// named/numbered groups) or a non-Regex match where only `\0` (the whole
/// match) is a valid group reference.
pub enum SubstContext<'a> {
    Regex {
        m: &'a RegexMatchValue,
        re: &'a RegexValue,
    },
    Plain {
        matched: &'a str,
    },
}

/// Append the text captured by numeric `group` in this context to `out`.
///
/// - group 0 → whole match.
/// - Regex: a group that exists but did not participate contributes nothing;
///   a group index beyond the pattern's group count is an error ("unknown
///   substring"), matching PCRE.
/// - Plain (non-Regex): only group 0 is valid; any other group errors, matching
///   upstream `_write_capture(io, group, str, r, re)` for a non-Regex `re`.
fn write_capture(out: &mut String, group: usize, ctx: &SubstContext) -> Result<(), String> {
    match ctx {
        SubstContext::Regex { m, .. } => {
            if group == 0 {
                out.push_str(&m.match_str);
                return Ok(());
            }
            if group <= m.captures.len() {
                if let Some(text) = &m.captures[group - 1] {
                    out.push_str(text);
                }
                Ok(())
            } else {
                Err("PCRE error: unknown substring".to_string())
            }
        }
        SubstContext::Plain { matched } => {
            if group == 0 {
                out.push_str(matched);
                Ok(())
            } else {
                Err("Bad replacement string: pattern is not a Regex".to_string())
            }
        }
    }
}

/// Resolve a `\g<name>` group name (already known to be non-numeric) to a group
/// index in this context.
fn resolve_named_group(name: &str, ctx: &SubstContext) -> Result<usize, String> {
    match ctx {
        SubstContext::Regex { re, .. } => capture_index_by_name(re, name)
            .ok_or_else(|| format!("Group {} not found in regex {}", name, re.pattern)),
        SubstContext::Plain { .. } => {
            Err("Bad replacement string: pattern is not a Regex".to_string())
        }
    }
}

/// Decode a non-capture escape `\c` at `chars[after]` (the character after the
/// backslash), returning the decoded character and the index just past the
/// escape. Mirrors `Base.unescape_string` for the escapes not kept by the
/// SubstitutionString `KEEP_ESC` set: control escapes, quotes, and `\xHH` /
/// `\uHHHH` / `\UHHHHHHHH` hex forms. Any other escape is an error, matching
/// upstream ("invalid escape sequence \c").
fn decode_escape(chars: &[char], after: usize) -> Result<(char, usize), String> {
    let c = chars[after];
    let simple = match c {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        'a' => Some('\u{07}'),
        'b' => Some('\u{08}'),
        'f' => Some('\u{0c}'),
        'v' => Some('\u{0b}'),
        'e' => Some('\u{1b}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    };
    if let Some(ch) = simple {
        return Ok((ch, after + 1));
    }
    let max_digits = match c {
        'x' => 2,
        'u' => 4,
        'U' => 8,
        _ => return Err(format!("invalid escape sequence \\{}", c)),
    };
    let mut j = after + 1;
    let mut val: u32 = 0;
    let mut count = 0;
    while j < chars.len() && count < max_digits && chars[j].is_ascii_hexdigit() {
        val = val * 16 + chars[j].to_digit(16).unwrap_or(0);
        j += 1;
        count += 1;
    }
    if count == 0 {
        return Err(format!("invalid escape sequence \\{}", c));
    }
    let ch =
        char::from_u32(val).ok_or_else(|| format!("invalid unicode escape sequence \\{}", c))?;
    Ok((ch, j))
}

/// Core SubstitutionString expansion, shared by the Regex and non-Regex paths.
/// Mirrors upstream `Base._replace(io, repl::SubstitutionString, str, r, re)`
/// (`julia/base/regex.jl`): the raw string is unescaped (`\n`, `\xHH`, …) except
/// for capture references `\N` (greedy multi-digit group number, `\0` = whole
/// match), `\g<name>` (named or numeric group), and `\\` (literal backslash).
fn expand_substitution_ctx(repl_s: &str, ctx: &SubstContext) -> Result<String, String> {
    let chars: Vec<char> = repl_s.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        // c == '\\'
        if i + 1 >= n {
            return Err(format!("Bad replacement string: {}", repl_s));
        }
        let nx = chars[i + 1];
        if nx == '\\' {
            out.push('\\');
            i += 2;
        } else if nx.is_ascii_digit() {
            // Greedy multi-digit group number, e.g. `\10`.
            let mut j = i + 1;
            let mut group: usize = 0;
            while j < n && chars[j].is_ascii_digit() {
                group = group * 10 + (chars[j] as usize - '0' as usize);
                j += 1;
            }
            write_capture(&mut out, group, ctx)?;
            i = j;
        } else if nx == 'g' {
            // `\g<name>` where name is a group name or a numeric group index.
            let mut j = i + 2;
            if j >= n || chars[j] != '<' {
                return Err(format!("Bad replacement string: {}", repl_s));
            }
            j += 1; // skip '<'
            let start = j;
            while j < n && chars[j] != '>' {
                j += 1;
            }
            if j >= n {
                return Err(format!("Bad replacement string: {}", repl_s));
            }
            let name: String = chars[start..j].iter().collect();
            j += 1; // skip '>'
            let group = if !name.is_empty() && name.chars().all(|d| d.is_ascii_digit()) {
                name.parse::<usize>()
                    .map_err(|_| format!("Bad replacement string: {}", repl_s))?
            } else {
                resolve_named_group(&name, ctx)?
            };
            write_capture(&mut out, group, ctx)?;
            i = j;
        } else {
            let (ch, next) = decode_escape(&chars, i + 1)?;
            out.push(ch);
            i = next;
        }
    }
    Ok(out)
}

/// Expand a `SubstitutionString` against a single regex match `m` of pattern
/// `re` (`\N`, `\g<name>`, `\0`).
pub fn expand_substitution(
    repl_s: &str,
    m: &RegexMatchValue,
    re: &RegexValue,
) -> Result<String, String> {
    expand_substitution_ctx(repl_s, &SubstContext::Regex { m, re })
}

/// Expand a `SubstitutionString` against a non-Regex (String/Char) match, whose
/// only valid group reference is `\0` / `\g<0>` (the whole matched substring).
pub fn expand_substitution_plain(repl_s: &str, matched: &str) -> Result<String, String> {
    expand_substitution_ctx(repl_s, &SubstContext::Plain { matched })
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
    fn test_regex_find_from_3arg() {
        // Offset search backs Julia's 3-arg `match(re, s, start)` (Issue #10178).
        // `start_byte` is 0-based; reported offsets stay absolute (1-based).
        let re = RegexValue::new("bc", "").unwrap();
        // Default search finds the first "bc" at 1-based offset 2.
        assert_eq!(re.find("abcbc").unwrap().offset, 2);
        // Starting at byte 3 (Julia idx 4) skips the first "bc".
        assert_eq!(re.find_from("abcbc", 3).unwrap().offset, 4);
        // Starting past every remaining match yields nothing.
        assert!(re.find_from("abcbc", 4).is_none());

        // Captures keep absolute 1-based offsets when searching from a position.
        let re2 = RegexValue::new(r"(\d+)", "").unwrap();
        let m = re2.find_from("abc123def456", 6).unwrap();
        assert_eq!(m.match_str, "456");
        assert_eq!(m.offset, 10);
        assert_eq!(m.offsets, vec![10]);

        // `start_byte` beyond the string end returns nothing (no panic).
        assert!(re2.find_from("abc123def456", 100).is_none());
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
    fn test_regex_find_all_overlapping() {
        // Overlapping matches restart one char past each match START, so the
        // three 2-char windows of "aaaa" are all reported (Issue #10199).
        let re = RegexValue::new(r"aa", "").unwrap();
        let baseline: Vec<i64> = re.find_all("aaaa").iter().map(|m| m.offset).collect();
        assert_eq!(baseline, vec![1, 3]); // find_all stays non-overlapping
        let overlap: Vec<i64> = re
            .find_all_overlapping("aaaa")
            .iter()
            .map(|m| m.offset)
            .collect();
        assert_eq!(overlap, vec![1, 2, 3]);
    }

    #[test]
    fn test_regex_find_all_overlapping_empty_capable() {
        // Empty-capable patterns advance one char per position (no infinite
        // loop) and match upstream `eachmatch(r"a*", "baab"; overlap=true)`.
        let re = RegexValue::new(r"a*", "").unwrap();
        let pairs: Vec<(i64, String)> = re
            .find_all_overlapping("baab")
            .iter()
            .map(|m| (m.offset, m.match_str.clone()))
            .collect();
        let expected: Vec<(i64, String)> = [(1, ""), (2, "aa"), (3, "a"), (4, ""), (5, "")]
            .iter()
            .map(|(o, s)| (*o as i64, (*s).to_string()))
            .collect();
        assert_eq!(pairs, expected);
    }

    #[test]
    fn test_regex_find_all_overlapping_multibyte_capture() {
        // Multibyte + capture groups keep 1-based byte offsets across restarts.
        let re = RegexValue::new(r"α(.)α", "").unwrap();
        let ms = re.find_all_overlapping("αβαγα");
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].offset, 1);
        assert_eq!(ms[0].captures, vec![Some("β".to_string())]);
        assert_eq!(ms[0].offsets, vec![3]);
        assert_eq!(ms[1].offset, 5);
        assert_eq!(ms[1].captures, vec![Some("γ".to_string())]);
        assert_eq!(ms[1].offsets, vec![7]);
    }

    #[test]
    fn test_regex_find_from() {
        let re = RegexValue::new(r"\d+", "").unwrap();
        // Positional search skips an earlier match and finds the next one.
        let m = re.find_from("ab12cd34", 4).unwrap(); // 0-based byte 4 = 'c'
        assert_eq!(m.match_str, "34");
        assert_eq!(m.offset, 7); // 1-based absolute offset into the full string
                                 // pos past the end yields no match.
        assert!(re.find_from("ab12cd34", 8).is_none());
    }

    #[test]
    fn test_regex_find_from_overlapping() {
        // A fresh positional search finds an overlapping match that the
        // non-overlapping `find_all` scan would skip.
        let re = RegexValue::new(r"\d\d", "").unwrap();
        let m = re.find_from("123", 1).unwrap(); // 0-based byte 1 = '2'
        assert_eq!(m.match_str, "23");
        assert_eq!(m.offset, 2);
    }

    #[test]
    fn test_regex_find_from_preserves_anchor_context() {
        // `^` anchors to the true start of the full text, not to `pos`, because
        // the search runs against the whole string (context preserved).
        let re = RegexValue::new(r"^\d", "").unwrap();
        assert!(re.find_from("a1", 1).is_none());
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

    #[test]
    fn test_regex_find_from_greedy() {
        // r".+" is greedy: matching from a 0-based interior byte re-matches the
        // tail, unlike a non-overlapping eachmatch that would only see the whole
        // string once.
        let re = RegexValue::new(r".+", "").unwrap();
        let m = re.find_from("abcabc", 1).unwrap();
        assert_eq!(m.match_str, "bcabc");
        assert_eq!(m.offset, 2);
        // Past the end → no match.
        assert!(re.find_from("abc", 5).is_none());
    }

    #[test]
    fn test_expand_substitution_numbered_and_named() {
        let re = RegexValue::new(r"(?<first>\w+) (?<second>\w+)", "").unwrap();
        let m = re.find("hello world").unwrap();
        assert_eq!(
            expand_substitution(r"\2 \1", &m, &re).unwrap(),
            "world hello"
        );
        assert_eq!(
            expand_substitution(r"\g<second>-\g<first>", &m, &re).unwrap(),
            "world-hello"
        );
        // \0 is the whole match; \g<1> is a numeric group via \g<...>.
        assert_eq!(
            expand_substitution(r"[\0]", &m, &re).unwrap(),
            "[hello world]"
        );
        assert_eq!(expand_substitution(r"\g<1>", &m, &re).unwrap(), "hello");
    }

    #[test]
    fn test_expand_substitution_escapes_and_errors() {
        let re = RegexValue::new(r"(a)", "").unwrap();
        let m = re.find("a").unwrap();
        // \\ → a single literal backslash; control escapes are unescaped.
        assert_eq!(expand_substitution(r"x\\y", &m, &re).unwrap(), r"x\y");
        assert_eq!(expand_substitution(r"a\nb", &m, &re).unwrap(), "a\nb");
        // Hex / unicode escapes decode to their code point (upstream parity).
        assert_eq!(expand_substitution(r"\x41", &m, &re).unwrap(), "A");
        assert_eq!(expand_substitution(r"A", &m, &re).unwrap(), "A");
        assert_eq!(expand_substitution(r"\U00000041", &m, &re).unwrap(), "A");
        // A group index beyond the pattern's groups is an error.
        assert!(expand_substitution(r"\9", &m, &re).is_err());
        // An unknown named group is an error.
        assert!(expand_substitution(r"\g<missing>", &m, &re).is_err());
        // An unknown escape is an error, matching upstream unescape_string.
        assert!(expand_substitution(r"\q", &m, &re).is_err());
    }

    #[test]
    fn test_expand_substitution_plain() {
        // Non-Regex pattern: only \0 / \g<0> (the whole match) are valid groups.
        assert_eq!(expand_substitution_plain(r"[\0]", "x").unwrap(), "[x]");
        assert_eq!(expand_substitution_plain(r"[\g<0>]", "x").unwrap(), "[x]");
        assert_eq!(expand_substitution_plain(r"a\tb", "x").unwrap(), "a\tb");
        assert_eq!(expand_substitution_plain(r"\x41", "x").unwrap(), "A");
        // A numbered/named group other than 0 is an error (no regex captures).
        assert!(expand_substitution_plain(r"\1", "x").is_err());
        assert!(expand_substitution_plain(r"\g<1>", "x").is_err());
        assert!(expand_substitution_plain(r"\g<name>", "x").is_err());
    }

    // --- PCRE2 escape rewrite (Issues #10179, #10180, #10203) ---

    #[test]
    fn test_rewrite_octal_escapes() {
        // \101 = octal 101 = 'A'; no capture groups -> octal, not backref.
        assert_eq!(rewrite_pcre2_escapes(r"\101"), r"\x{41}");
        // Leading zero is always octal.
        assert_eq!(rewrite_pcre2_escapes(r"\0"), r"\x{0}");
        assert_eq!(rewrite_pcre2_escapes(r"\012"), r"\x{a}");
        // Up to three octal digits, then literal digits.
        assert_eq!(rewrite_pcre2_escapes(r"\1010"), r"\x{41}0");
        // Two-digit octal below the group count boundary.
        assert_eq!(rewrite_pcre2_escapes(r"\11"), r"\x{9}");
        assert_eq!(rewrite_pcre2_escapes(r"\50"), r"\x{28}");
    }

    #[test]
    fn test_rewrite_preserves_backreferences() {
        // Single-digit \1 with a matching group stays a back reference.
        assert_eq!(rewrite_pcre2_escapes(r"(a)\1"), r"(a)\1");
        // \12 with >=12 capture groups stays a back reference.
        let twelve_groups = "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)";
        let pat = format!("{}\\12", twelve_groups);
        assert_eq!(rewrite_pcre2_escapes(&pat), pat);
        // Named groups also count toward the capture total.
        assert_eq!(
            count_capture_groups(&"(?<x>a)(?'y'b)".chars().collect::<Vec<_>>()),
            2
        );
        // Non-capturing / lookaround forms do not count.
        assert_eq!(
            count_capture_groups(&"(?:a)(?=b)(?<!c)".chars().collect::<Vec<_>>()),
            0
        );
    }

    // --- Issue #10181: PCRE2 recursion / subroutine calls are rejected -------

    #[test]
    fn test_detect_recursion_positive() {
        // Whole-pattern recursion.
        assert_eq!(detect_regex_recursion("(?R)").as_deref(), Some("(?R)"));
        assert_eq!(detect_regex_recursion("(?0)").as_deref(), Some("(?0)"));
        // Numbered subroutine calls (single and multi-digit).
        assert_eq!(detect_regex_recursion("(?1)").as_deref(), Some("(?1)"));
        assert_eq!(detect_regex_recursion("(?12)").as_deref(), Some("(?12)"));
        // Relative subroutine calls.
        assert_eq!(detect_regex_recursion("(?+1)").as_deref(), Some("(?+1)"));
        assert_eq!(detect_regex_recursion("(?-1)").as_deref(), Some("(?-1)"));
        // Named subroutine calls.
        assert_eq!(detect_regex_recursion("(?&foo)").as_deref(), Some("(?&…)"));
        assert_eq!(
            detect_regex_recursion("(?P>foo)").as_deref(),
            Some("(?P>…)")
        );
        // Embedded in the issue's MWEs.
        assert!(detect_regex_recursion(r"\((?:[^()]|(?R))*\)").is_some());
        assert!(detect_regex_recursion(r"^(x(?1)?y)$").is_some());
        // Real recursion following a comment group is still caught (the comment
        // is skipped, not the whole tail).
        assert_eq!(
            detect_regex_recursion("(?#a comment)(?R)").as_deref(),
            Some("(?R)")
        );
    }

    #[test]
    fn test_rewrite_o_brace_and_control() {
        assert_eq!(rewrite_pcre2_escapes(r"\o{101}"), r"\x{41}");
        assert_eq!(rewrite_pcre2_escapes(r"\o{12}"), r"\x{a}");
        // \cA -> 0x01, \cZ -> 0x1a, lowercase folded to uppercase first.
        assert_eq!(rewrite_pcre2_escapes(r"\cA"), r"\x{1}");
        assert_eq!(rewrite_pcre2_escapes(r"\cZ"), r"\x{1a}");
        assert_eq!(rewrite_pcre2_escapes(r"\ca"), r"\x{1}");
        // Malformed \o (no brace) is copied verbatim for the engine to report.
        assert_eq!(rewrite_pcre2_escapes(r"\oq"), r"\oq");
        assert_eq!(rewrite_pcre2_escapes(r"\o{}"), r"\o{}");
        // An over-long \o{...} must not overflow-panic (saturates, then the
        // engine rejects the out-of-range codepoint).
        let _ = rewrite_pcre2_escapes(r"\o{7777777777777777777777}");
        assert!(RegexValue::new(r"\o{7777777777777777777777}", "").is_err());
    }

    #[test]
    fn test_rewrite_vertical_and_horizontal_classes() {
        assert_eq!(
            rewrite_pcre2_escapes(r"\v"),
            format!("[{}]", PCRE2_VERTICAL_WS)
        );
        assert_eq!(
            rewrite_pcre2_escapes(r"\V"),
            format!("[^{}]", PCRE2_VERTICAL_WS)
        );
        assert_eq!(
            rewrite_pcre2_escapes(r"\h"),
            format!("[{}]", PCRE2_HORIZONTAL_WS)
        );
        assert_eq!(
            rewrite_pcre2_escapes(r"\H"),
            format!("[^{}]", PCRE2_HORIZONTAL_WS)
        );
    }

    #[test]
    fn test_rewrite_whitespace_inside_classes() {
        // Positive forms inline the body; negated forms nest a negated class.
        assert_eq!(
            rewrite_pcre2_escapes(r"[a\h]"),
            format!("[a{}]", PCRE2_HORIZONTAL_WS)
        );
        assert_eq!(
            rewrite_pcre2_escapes(r"[a\H]"),
            format!("[a[^{}]]", PCRE2_HORIZONTAL_WS)
        );
        assert_eq!(
            rewrite_pcre2_escapes(r"[\v]"),
            format!("[{}]", PCRE2_VERTICAL_WS)
        );
        // Octal inside a class.
        assert_eq!(rewrite_pcre2_escapes(r"[\101]"), r"[\x{41}]");
    }

    #[test]
    fn test_rewrite_preserves_hex_and_other_escapes() {
        // Hex escapes that already work must be untouched.
        assert_eq!(rewrite_pcre2_escapes(r"\x41"), r"\x41");
        assert_eq!(rewrite_pcre2_escapes(r"\x{3042}"), r"\x{3042}");
        assert_eq!(rewrite_pcre2_escapes(r"\d+\w*\s"), r"\d+\w*\s");
        assert_eq!(rewrite_pcre2_escapes(r"\p{L}"), r"\p{L}");
        // Escaped backslash then a digit: the digit is literal, not a backref.
        assert_eq!(rewrite_pcre2_escapes(r"\\1"), r"\\1");
    }

    #[test]
    fn test_end_to_end_escape_matches() {
        // Issue #10179
        assert!(RegexValue::new(r"\101", "").unwrap().is_match("A"));
        assert!(RegexValue::new(r"\o{101}", "").unwrap().is_match("A"));
        assert!(RegexValue::new(r"\cA", "").unwrap().is_match("\u{1}"));
        // Issue #10180
        assert!(RegexValue::new(r"\v", "").unwrap().is_match("a\nb"));
        assert!(!RegexValue::new(r"\v", "").unwrap().is_match("ab"));
        // Issue #10203
        assert!(!RegexValue::new(r"\h", "").unwrap().is_match("ab"));
        assert!(RegexValue::new(r"\h", "").unwrap().is_match("a\tb"));
        assert!(!RegexValue::new(r"\H", "").unwrap().is_match("   "));
        let m = RegexValue::new(r"\h+", "").unwrap().find("a \t b").unwrap();
        assert_eq!(m.match_str, " \t ");
        // Back reference still works end to end.
        assert!(RegexValue::new(r"(ab)\1", "").unwrap().is_match("abab"));
    }

    /// Characterization guard for the three PCRE2-vs-fancy-regex permissiveness
    /// divergences recorded in `docs/vm/REGEX_PCRE2_PARITY.md` (Issue #10183).
    /// Upstream Julia (PCRE2) *errors* on all three; sjulia (fancy-regex)
    /// accepts/completes. The accepted policy decision is that these are
    /// permanent engine-boundary divergences (Native Boundary Policy A, #8992):
    /// sjulia does not reimplement PCRE2's `ALT_BSUX`/match-limit semantics and
    /// does not add an ad-hoc variable-length-lookbehind rejector. This test pins
    /// sjulia's current divergent behavior so a future `fancy-regex` bump that
    /// changes it is caught and the parity doc is updated together. It
    /// intentionally does NOT assert parity with upstream (which errors).
    #[test]
    fn test_pcre2_permissiveness_divergences_10183() {
        // 1. Variable-length lookbehind: PCRE2 rejects at compile
        //    ("length of lookbehind assertion is not limited"); fancy-regex
        //    compiles and matches.
        let vlb = RegexValue::new(r"(?<=ab*)c", "")
            .expect("variable-length lookbehind compiles under fancy-regex");
        let m = vlb
            .find("abbc")
            .expect("variable-length lookbehind matches 'c'");
        assert_eq!(m.match_str, "c");
        assert_eq!(m.offset, 4);

        // 2. \x{HHHH}: PCRE2 ALT_BSUX reads ECMAScript \x and does NOT match
        //    U+3042; fancy-regex reads it as a Unicode code-point escape and
        //    matches "あ".
        let hex = RegexValue::new(r"\x{3042}", "").expect(r"\x{3042} compiles");
        assert!(hex.is_match("あ"));

        // 3. Catastrophic backtracking: PCRE2 raises "match limit exceeded" on
        //    "a"^28 * "b"; fancy-regex completes and returns the valid empty
        //    match at end-of-string (offset 30).
        let cat = RegexValue::new(r"(a|a?)+$", "").expect("catastrophic pattern compiles");
        let input = format!("{}b", "a".repeat(28));
        let m = cat
            .find(&input)
            .expect("fancy-regex completes without a match-limit error");
        assert_eq!(m.match_str, "");
        assert_eq!(m.offset, 30);
    }

    #[test]
    fn test_detect_recursion_negative() {
        // Legitimate group constructs must NOT be flagged.
        for p in [
            "(?:abc)",          // non-capturing
            "(?i)abc",          // inline flag
            "(?-i)abc",         // inline flag negation
            "(?i-s:abc)",       // scoped flags
            "(?=abc)",          // lookahead
            "(?!abc)",          // negative lookahead
            "(?<=abc)",         // lookbehind
            "(?<!abc)",         // negative lookbehind
            "(?>a+)b",          // atomic group
            "(?#comment)",      // comment
            "(?(1)a|b)",        // conditional
            "(?<name>abc)",     // named capture (PCRE)
            "(?P<name>abc)",    // named capture (Python)
            "(?P=name)",        // named backreference (Python)
            "(?'name'abc)",     // named capture (quoted)
            "abc",              // no groups at all
            r"\(?R\)",          // escaped parens, not a group
            "[(?R)]",           // recursion token inside a character class
            r"[a\]](?:x)",      // class with escaped ] then non-capturing group
            "(?#see (?R) doc)", // recursion text inside a comment group
            "a(?#(?1))b",       // comment body ignored, no real recursion
        ] {
            assert_eq!(detect_regex_recursion(p), None, "misflagged: {p}");
        }
    }

    #[test]
    fn test_regex_comment_backslash_does_not_hide_recursion_10738() {
        let pattern = r"(?#\)(?R)";
        assert_eq!(detect_regex_recursion(pattern), Some("(?R)".to_string()));

        let err = RegexValue::new(pattern, "").unwrap_err();
        assert!(err.contains("recursion"), "unexpected error: {err}");
        assert!(err.contains("(?R)"), "unexpected error: {err}");
    }

    #[test]
    fn test_regex_new_rejects_recursion() {
        let err = RegexValue::new(r"\((?:[^()]|(?R))*\)", "").unwrap_err();
        assert!(err.contains("recursion"), "unexpected error: {err}");
        assert!(err.contains("(?R)"), "unexpected error: {err}");

        let err = RegexValue::new(r"^(x(?1)?y)$", "").unwrap_err();
        assert!(err.contains("recursion"), "unexpected error: {err}");

        // A recursion-free pattern still compiles.
        assert!(RegexValue::new(r"(?:a|b)+(?i)c", "").is_ok());
    }
}
