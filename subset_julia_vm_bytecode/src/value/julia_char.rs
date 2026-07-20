//! Julia `Char` bit-pattern helpers (Issue #8995).
//!
//! Julia represents a `Char` as a 32-bit value holding the character's UTF-8
//! byte sequence left-aligned (`reinterpret(UInt32, 'a') == 0x61000000`).
//! Malformed byte sequences inside a `String` still iterate to `Char`s — the
//! bytes are packed into the same left-aligned form and the resulting `Char`
//! is simply invalid (`isvalid(c) == false`) — e.g. iterating
//! `String(UInt8[0xff, 0x61])` yields `'\xff'` then `'a'`.
//!
//! sjulia's `Value::Char(char)` can only carry valid Unicode scalars, so
//! malformed sequences are represented as `Value::CharMalformed(u32)` holding
//! the Julia bit pattern. This module is the single authority for decoding
//! string bytes into Julia char bits, mirroring upstream
//! `julia/base/strings/string.jl` (`iterate` / `iterate_continued`).

/// Decode one Julia character starting at 0-based byte offset `i` of
/// `bytes`, following upstream `iterate(s::String, i)`. Returns the Julia
/// 32-bit char pattern and the 0-based offset of the next character.
///
/// The upstream algorithm consumes the lead byte, then up to 3 continuation
/// bytes gated by the lead's class (`u < 0xc0000000` → none, `< 0xe0000000`
/// → one, `< 0xf0000000` → two, else three), stopping early at a
/// non-continuation byte or end of input. Bytes ≥ 0xf8 and stray
/// continuation bytes yield a single-byte (malformed) char.
///
/// # Panics
/// Panics if `i >= bytes.len()`; callers bounds-check first (the same
/// contract as upstream's `@inbounds codeunit`).
#[inline]
pub fn decode_julia_char(bytes: &[u8], i: usize) -> (u32, usize) {
    let b = bytes[i];
    let mut u = (b as u32) << 24;
    let mut j = i + 1;
    if !(0x80..=0xf7).contains(&b) {
        // ASCII (or 0xf8..0xff invalid lead): single code unit.
        return (u, j);
    }
    if u < 0xc000_0000 {
        // Stray continuation byte: single (malformed) unit.
        return (u, j);
    }
    let n = bytes.len();
    // first continuation byte
    if j >= n {
        return (u, j);
    }
    let b = bytes[j];
    if b & 0xc0 != 0x80 {
        return (u, j);
    }
    u |= (b as u32) << 16;
    j += 1;
    // second continuation byte
    if j >= n || u < 0xe000_0000 {
        return (u, j);
    }
    let b = bytes[j];
    if b & 0xc0 != 0x80 {
        return (u, j);
    }
    u |= (b as u32) << 8;
    j += 1;
    // third continuation byte
    if j >= n || u < 0xf000_0000 {
        return (u, j);
    }
    let b = bytes[j];
    if b & 0xc0 != 0x80 {
        return (u, j);
    }
    u |= b as u32;
    j += 1;
    (u, j)
}

/// The Julia 32-bit char pattern of a valid Unicode scalar: its UTF-8
/// encoding left-aligned in the word.
#[inline]
pub fn julia_char_bits(c: char) -> u32 {
    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf);
    let mut u = 0u32;
    for (k, &b) in s.as_bytes().iter().enumerate() {
        u |= (b as u32) << (24 - 8 * k);
    }
    u
}

/// The UTF-8-ish bytes stored in a Julia char pattern: the leading bytes up
/// to and including the last nonzero one (a pattern of all zeros is the one
/// -byte NUL char). Malformed sequences never contain interior 0x00 bytes
/// (continuations are 0x80..0xbf and multi-byte leads are ≥ 0xc0), so
/// trailing-zero trimming is unambiguous.
#[inline]
pub fn julia_char_pattern_bytes(u: u32) -> ([u8; 4], usize) {
    let bytes = u.to_be_bytes();
    let len = 4 - (u.trailing_zeros() / 8).min(3) as usize;
    (bytes, len)
}

/// Reconstruct a Rust `char` from a Julia char pattern when the pattern is a
/// well-formed UTF-8 encoding of a single scalar; `None` for malformed,
/// overlong, or surrogate patterns (Julia's invalid `Char`s).
#[inline]
pub fn julia_char_from_bits(u: u32) -> Option<char> {
    let (bytes, len) = julia_char_pattern_bytes(u);
    let s = core::str::from_utf8(&bytes[..len]).ok()?;
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(c)
}

/// Number of Julia characters in a string's bytes, using the same
/// segmentation as [`decode_julia_char`]. For valid UTF-8 this equals
/// `str::chars().count()`; for malformed bytes it matches upstream `length`
/// (e.g. the overlong `[0xc0, 0x80]` is ONE malformed char, where a
/// WHATWG-lossy scan would count two replacement chars).
pub fn julia_char_count(bytes: &[u8]) -> usize {
    let mut i = 0;
    let mut count = 0;
    while i < bytes.len() {
        let (_, next) = decode_julia_char(bytes, i);
        i = next;
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_and_multibyte_roundtrip() {
        assert_eq!(decode_julia_char(b"ab", 0), (0x6100_0000, 1));
        // "あ" = e3 81 82
        let a = "あb".as_bytes();
        assert_eq!(decode_julia_char(a, 0), (0xe381_8200, 3));
        assert_eq!(decode_julia_char(a, 3), (0x6200_0000, 4));
        assert_eq!(julia_char_bits('a'), 0x6100_0000);
        assert_eq!(julia_char_bits('あ'), 0xe381_8200);
        assert_eq!(julia_char_from_bits(0xe381_8200), Some('あ'));
    }

    #[test]
    fn malformed_sequences_match_upstream_segmentation() {
        // 0xff lead: single unit, no continuation consumption
        assert_eq!(decode_julia_char(&[0xff, 0x61], 0), (0xff00_0000, 1));
        // truncated 3-byte lead: consumes available continuations
        assert_eq!(decode_julia_char(&[0xe3, 0x81], 0), (0xe381_0000, 2));
        // overlong 2-byte: lead 0xc0 consumes one continuation (Julia
        // segmentation, unlike WHATWG's one-byte maximal subpart)
        assert_eq!(decode_julia_char(&[0xc0, 0x80], 0), (0xc080_0000, 2));
        // stray continuation: single unit
        assert_eq!(decode_julia_char(&[0x80, 0x61], 0), (0x8000_0000, 1));
        // malformed patterns do not round-trip to a Rust char
        assert_eq!(julia_char_from_bits(0xff00_0000), None);
        assert_eq!(julia_char_from_bits(0xc080_0000), None);
        // surrogate ed a0 80
        assert_eq!(decode_julia_char(&[0xed, 0xa0, 0x80], 0), (0xeda0_8000, 3));
        assert_eq!(julia_char_from_bits(0xeda0_8000), None);
    }

    #[test]
    fn pattern_bytes_lengths() {
        assert_eq!(julia_char_pattern_bytes(0x6100_0000).1, 1);
        assert_eq!(julia_char_pattern_bytes(0xe381_0000).1, 2);
        assert_eq!(julia_char_pattern_bytes(0xeda0_8000).1, 3);
        assert_eq!(julia_char_pattern_bytes(0xe381_8261).1, 4);
        // NUL char: all-zero pattern is one byte
        assert_eq!(julia_char_pattern_bytes(0).1, 1);
    }
}
