//! String builtin functions for the VM.
//!
//! String operations: uppercase, lowercase, split, join, replace, etc.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→usize casts are guarded by bounds checks (`i < 1 || i as usize > len`);
// i64→u32/u64 casts for char codepoints and bitstring formatting use the full range.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::util::{expr_to_julia_string, format_sprintf, format_value};
use super::value::{new_array_ref, ArrayData, ArrayValue, Value};
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute string builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a string builtin.
    pub(super) fn execute_builtin_strings(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::StringNew => {
                // string(args...) - concatenate all arguments into a string
                // Special handling for Expr and Symbol to produce Julia code format
                // Note: Julia's string() outputs the content WITHOUT the :() wrapper
                //   string(:foo) => "foo" (not ":foo")
                //   string(:(x + 1)) => "x + 1" (not ":(x + 1)")
                let mut parts = Vec::with_capacity(argc);
                for _ in 0..argc {
                    let val = self.stack.pop_value()?;
                    let s = match &val {
                        // Expr: format as Julia code (no :() wrapper)
                        Value::Expr(e) => expr_to_julia_string(e),
                        // Symbol: format as name (no : prefix)
                        Value::Symbol(s) => s.as_str().to_string(),
                        // Other values: use standard formatting
                        _ => format_value(&val),
                    };
                    parts.push(s);
                }
                parts.reverse();
                self.stack.push(Value::Str(parts.join("")));
            }

            BuiltinId::StringFromChars => {
                // String(chars) - construct string from Vector{Char} (Issue #2038)
                let val = self.stack.pop_value()?;
                let s = match &val {
                    Value::Array(arr) => {
                        let borrowed = arr.borrow();
                        match &borrowed.data {
                            ArrayData::Char(chars) => chars.iter().collect::<String>(),
                            ArrayData::Any(vals) => {
                                // Handle Vector{Any} containing Char values
                                let mut result = String::new();
                                for v in vals {
                                    match v {
                                        Value::Char(c) => result.push(*c),
                                        _ => {
                                            return Err(VmError::TypeError(format!(
                                                "String: expected Vector{{Char}}, got array containing {}",
                                                format_value(v)
                                            )));
                                        }
                                    }
                                }
                                result
                            }
                            _ => {
                                return Err(VmError::TypeError(
                                    "String: expected Vector{Char}".to_string(),
                                ));
                            }
                        }
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = super::util::memory_to_array_ref(mem);
                        let borrowed = arr.borrow();
                        match &borrowed.data {
                            ArrayData::Char(chars) => chars.iter().collect::<String>(),
                            ArrayData::Any(vals) => {
                                let mut result = String::new();
                                for v in vals {
                                    match v {
                                        Value::Char(c) => result.push(*c),
                                        _ => {
                                            return Err(VmError::TypeError(format!(
                                                "String: expected Vector{{Char}}, got array containing {}",
                                                format_value(v)
                                            )));
                                        }
                                    }
                                }
                                result
                            }
                            _ => {
                                return Err(VmError::TypeError(
                                    "String: expected Vector{Char}".to_string(),
                                ));
                            }
                        }
                    }
                    Value::Str(s) => s.clone(), // String(s) is identity for strings
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "String: cannot convert {} to String",
                            format_value(&val)
                        )));
                    }
                };
                self.stack.push(Value::Str(s));
            }

            BuiltinId::Repr => {
                // repr(x) - return string representation with quotes for strings
                let val = self.stack.pop_value()?;
                let s = match &val {
                    Value::Str(s) => format!("\"{}\"", s),
                    _ => format_value(&val),
                };
                self.stack.push(Value::Str(s));
            }

            BuiltinId::Sprintf => {
                // sprintf(fmt, args...) - C-style formatted string
                let mut values = Vec::with_capacity(argc);
                for _ in 0..argc {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();

                if values.is_empty() {
                    return Err(VmError::TypeError(
                        "sprintf requires a format string".to_string(),
                    ));
                }

                let fmt = match &values[0] {
                    Value::Str(s) => s.clone(),
                    _ => {
                        return Err(VmError::TypeError(
                            "sprintf format must be a string".to_string(),
                        ))
                    }
                };

                let args = &values[1..];
                let result = format_sprintf(&fmt, args);
                self.stack.push(Value::Str(result));
            }

            BuiltinId::Ncodeunits => {
                // ncodeunits(s) - number of code units (bytes for UTF-8)
                let s = self.stack.pop_str()?;
                self.stack.push(Value::I64(s.len() as i64));
            }

            BuiltinId::Codeunit => {
                // codeunit(s, i) - get byte at position i (1-indexed)
                let i = self.stack.pop_i64()?;
                let s = self.stack.pop_str()?;
                let bytes = s.as_bytes();
                if i < 1 || i as usize > bytes.len() {
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![i],
                        shape: vec![bytes.len()],
                    });
                }
                self.stack.push(Value::I64(bytes[(i - 1) as usize] as i64));
            }

            BuiltinId::CodeUnits => {
                // codeunits(s) - get all bytes as Vector{UInt8}
                let s = self.stack.pop_str()?;
                let bytes: Vec<u8> = s.as_bytes().to_vec();
                let len = bytes.len();
                let arr = ArrayValue {
                    data: ArrayData::U8(bytes),
                    shape: vec![len],
                    struct_type_id: None,
                    element_type_override: None,
                };
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            // BuiltinId::StringFirst removed - now Pure Julia in base/strings/basic.jl

            // BuiltinId::StringLast removed - now Pure Julia in base/strings/basic.jl

            // BuiltinId::Uppercase removed - now Pure Julia in base/strings/unicode.jl

            // BuiltinId::Lowercase removed - now Pure Julia in base/strings/unicode.jl

            // BuiltinId::Titlecase removed - now Pure Julia in base/strings/unicode.jl

            // Strip, Lstrip, Rstrip, Chomp, Chop removed - now Pure Julia (base/strings/util.jl)
            BuiltinId::Occursin => {
                // occursin(needle, haystack) - needle can be String or Regex
                let haystack = self.stack.pop_str()?;
                let needle = self.stack.pop_value()?;
                match needle {
                    Value::Str(s) => {
                        self.stack.push(Value::Bool(haystack.contains(&s)));
                    }
                    Value::Regex(r) => {
                        self.stack.push(Value::Bool(r.is_match(&haystack)));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "occursin: expected String or Regex, got {:?}",
                            needle.value_type()
                        )));
                    }
                }
            }

            // BuiltinId::Findfirst removed - now Pure Julia in base/strings/search.jl

            // BuiltinId::Findlast removed - now Pure Julia in base/strings/search.jl

            // BuiltinId::StringSplit removed - now Pure Julia in base/strings/util.jl

            // BuiltinId::StringRsplit removed - now Pure Julia in base/strings/util.jl

            // BuiltinId::StringRepeat removed - now Pure Julia in base/strings/basic.jl

            // BuiltinId::StringReverse removed - now Pure Julia in base/strings/basic.jl

            // BuiltinId::StringToInt removed - now Pure Julia (base/parse.jl)
            BuiltinId::StringToFloat => {
                // parse(Float64, s)
                let s = self.stack.pop_str()?;
                match s.trim().parse::<f64>() {
                    Ok(n) => self.stack.push(Value::F64(n)),
                    Err(_) => {
                        return Err(VmError::TypeError(format!(
                            "cannot parse \"{}\" as Float64",
                            s
                        )))
                    }
                }
            }

            BuiltinId::StringToIntBase => {
                // parse(Int, s; base=N) - parse string in given base (Issue #2036)
                // Kept as Rust builtin because kwargs dispatch not supported in Pure Julia Base
                let base = self.stack.pop_i64()? as u32;
                let s = self.stack.pop_str()?;
                match i64::from_str_radix(s.trim(), base) {
                    Ok(n) => self.stack.push(Value::I64(n)),
                    Err(_) => {
                        return Err(VmError::TypeError(format!(
                            "cannot parse \"{}\" as Int64 in base {}",
                            s, base
                        )))
                    }
                }
            }

            BuiltinId::StringIntToBase => {
                // string(x; base=N) - convert integer to string in given base (Issue #2036)
                let base = self.stack.pop_i64()?;
                let val = self.stack.pop_value()?;
                let n = match &val {
                    Value::I64(n) => *n,
                    Value::F64(f) => *f as i64,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "string: cannot convert {} to integer for base conversion",
                            format_value(&val)
                        )))
                    }
                };
                let s = match base {
                    2 => format!("{:b}", n),
                    8 => format!("{:o}", n),
                    10 => format!("{}", n),
                    16 => format!("{:x}", n),
                    _ => {
                        // Generic base conversion for bases 2-36
                        if !(2..=36).contains(&base) {
                            return Err(VmError::TypeError(format!(
                                "string: base must be between 2 and 36, got {}",
                                base
                            )));
                        }
                        let base_u = base as u64;
                        let negative = n < 0;
                        let mut num = n.unsigned_abs();
                        if num == 0 {
                            "0".to_string()
                        } else {
                            let mut digits = Vec::with_capacity(65);
                            while num > 0 {
                                let d = (num % base_u) as u8;
                                digits.push(if d < 10 {
                                    (b'0' + d) as char
                                } else {
                                    (b'a' + d - 10) as char
                                });
                                num /= base_u;
                            }
                            digits.reverse();
                            let result: String = digits.into_iter().collect();
                            if negative {
                                format!("-{}", result)
                            } else {
                                result
                            }
                        }
                    }
                };
                self.stack.push(Value::Str(s));
            }

            BuiltinId::CharToInt => {
                // Int(c) - char to codepoint
                let val = self.stack.pop_value()?;
                match val {
                    Value::Char(c) => self.stack.push(Value::I64(c as i64)),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Int: expected Char, got {:?}",
                            val
                        )))
                    }
                }
            }

            BuiltinId::Codepoint => {
                // codepoint(c) - Unicode codepoint as UInt32
                // In Julia, codepoint(c::Char) returns the Unicode codepoint as UInt32.
                let val = self.stack.pop_value()?;
                match val {
                    Value::Char(c) => self.stack.push(Value::U32(c as u32)),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "codepoint: expected Char, got {:?}",
                            val
                        )))
                    }
                }
            }

            BuiltinId::IntToChar => {
                // Char(n) - codepoint to char
                let n = self.stack.pop_i64()?;
                match char::from_u32(n as u32) {
                    Some(c) => self.stack.push(Value::Char(c)),
                    None => {
                        return Err(VmError::TypeError(format!("Char: invalid codepoint {}", n)))
                    }
                }
            }

            BuiltinId::Bitstring => {
                // bitstring(x) - binary representation as string
                let val = self.stack.pop_value()?;
                let result = match val {
                    Value::I64(n) => format!("{:064b}", n as u64),
                    Value::F64(f) => format!("{:064b}", f.to_bits()),
                    Value::Bool(b) => {
                        if b {
                            "1".to_string()
                        } else {
                            "0".to_string()
                        }
                    }
                    Value::Char(c) => format!("{:032b}", c as u32),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "bitstring: unsupported type {:?}",
                            val
                        )))
                    }
                };
                self.stack.push(Value::Str(result));
            }

            // BuiltinId::Ascii removed - now Pure Julia in base/strings/util.jl
            // BuiltinId::Nextind, Prevind, Thisind, Reverseind removed - now Pure Julia (base/strings/basic.jl)
            // BuiltinId::Bytes2Hex, Hex2Bytes removed - now Pure Julia (base/strings/util.jl)
            BuiltinId::UnescapeString => {
                // unescape_string(s) - unescape escape sequences in string
                let s = self.stack.pop_str()?;
                let mut result = String::with_capacity(s.len());
                let mut chars = s.chars().peekable();

                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(&next) = chars.peek() {
                            chars.next();
                            match next {
                                'n' => result.push('\n'),
                                't' => result.push('\t'),
                                'r' => result.push('\r'),
                                '\\' => result.push('\\'),
                                '"' => result.push('"'),
                                '\'' => result.push('\''),
                                '0' => result.push('\0'),
                                'a' => result.push('\x07'), // bell
                                'b' => result.push('\x08'), // backspace
                                'f' => result.push('\x0C'), // form feed
                                'v' => result.push('\x0B'), // vertical tab
                                'e' => result.push('\x1B'), // escape
                                'x' => {
                                    // \xNN - hex escape (2 digits)
                                    let mut hex = String::new();
                                    for _ in 0..2 {
                                        if let Some(&c) = chars.peek() {
                                            if c.is_ascii_hexdigit() {
                                                hex.push(c);
                                                chars.next();
                                            } else {
                                                break;
                                            }
                                        }
                                    }
                                    if let Ok(n) = u8::from_str_radix(&hex, 16) {
                                        result.push(n as char);
                                    } else {
                                        result.push('\\');
                                        result.push('x');
                                        result.push_str(&hex);
                                    }
                                }
                                'u' => {
                                    // \uNNNN - unicode escape (4 digits)
                                    let mut hex = String::new();
                                    for _ in 0..4 {
                                        if let Some(&c) = chars.peek() {
                                            if c.is_ascii_hexdigit() {
                                                hex.push(c);
                                                chars.next();
                                            } else {
                                                break;
                                            }
                                        }
                                    }
                                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                                        if let Some(c) = char::from_u32(n) {
                                            result.push(c);
                                        } else {
                                            result.push('\\');
                                            result.push('u');
                                            result.push_str(&hex);
                                        }
                                    } else {
                                        result.push('\\');
                                        result.push('u');
                                        result.push_str(&hex);
                                    }
                                }
                                _ => {
                                    // Unknown escape, keep as-is
                                    result.push('\\');
                                    result.push(next);
                                }
                            }
                        } else {
                            result.push('\\');
                        }
                    } else {
                        result.push(c);
                    }
                }
                self.stack.push(Value::Str(result));
            }

            BuiltinId::Isnumeric => {
                // isnumeric(c) - check if character is numeric (Unicode)
                let val = self.stack.pop_value()?;
                let result = match val {
                    Value::Char(c) => c.is_numeric(),
                    Value::Str(s) => {
                        let mut iter = s.chars();
                        match (iter.next(), iter.next()) {
                            (Some(c), None) => c.is_numeric(),
                            _ => {
                                return Err(VmError::ErrorException(
                                    "isnumeric: expected a single character".to_string(),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(VmError::ErrorException(
                            "isnumeric: expected Char".to_string(),
                        ));
                    }
                };
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::IsvalidIndex => {
                // isvalid(s, i) - check if index is valid character boundary
                let i = self.stack.pop_i64()?;
                let s = self.stack.pop_str()?;
                // Julia uses 1-based indexing; negative or zero indices are invalid
                let result = if i <= 0 || i as usize > s.len() {
                    false
                } else {
                    s.is_char_boundary((i - 1) as usize)
                };
                self.stack.push(Value::Bool(result));
            }

            // BuiltinId::TryparseInt64 removed - now Pure Julia (base/parse.jl)
            BuiltinId::TryparseFloat64 => {
                // tryparse(Float64, s) - parse string as Float64, return nothing on failure
                let s = self.stack.pop_str()?;
                match s.trim().parse::<f64>() {
                    Ok(n) => self.stack.push(Value::F64(n)),
                    Err(_) => self.stack.push(Value::Nothing),
                }
            }

            // BuiltinId::FindNextString removed - now Pure Julia in base/strings/search.jl

            // BuiltinId::FindPrevString removed - now Pure Julia in base/strings/search.jl
            BuiltinId::StringFindAll => {
                // findall(pattern, string) - find all non-overlapping occurrences (Issue #2013)
                // Returns Vector{UnitRange{Int64}} matching Julia's behavior
                let s = self.stack.pop_str()?;
                let pattern = self.stack.pop_value()?;

                let mut ranges = Vec::new();
                match pattern {
                    Value::Str(needle) => {
                        if !needle.is_empty() {
                            let needle_len = needle.len();
                            let mut start = 0;
                            while start <= s.len().saturating_sub(needle_len) {
                                if let Some(idx) = s[start..].find(&needle) {
                                    let match_start = start + idx;
                                    let match_end = match_start + needle_len - 1;
                                    // Julia uses 1-based indexing
                                    ranges.push(Value::Range(
                                        super::value::RangeValue::unit_range(
                                            (match_start + 1) as f64,
                                            (match_end + 1) as f64,
                                        ),
                                    ));
                                    start = match_start + needle_len; // non-overlapping
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                    Value::Char(c) => {
                        for (byte_idx, ch) in s.char_indices() {
                            if ch == c {
                                let pos = byte_idx + 1; // 1-based
                                ranges.push(Value::Range(super::value::RangeValue::unit_range(
                                    pos as f64, pos as f64,
                                )));
                            }
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "findall: pattern must be String or Char, got {:?}",
                            crate::vm::util::value_type_name(&pattern)
                        )));
                    }
                }
                self.stack
                    .push(Value::Array(new_array_ref(ArrayValue::any_vector(ranges))));
            }

            BuiltinId::StringCount => {
                // count(pattern, string) - count non-overlapping occurrences (Issue #2009)
                // Pattern can be String or Char
                let s = self.stack.pop_str()?;
                let pattern = self.stack.pop_value()?;

                let count = match pattern {
                    Value::Str(needle) => {
                        if needle.is_empty() {
                            // Julia: count("", s) returns length(s) + 1
                            // (counts the empty string between each character and at boundaries)
                            s.chars().count() + 1
                        } else {
                            s.matches(&needle).count()
                        }
                    }
                    Value::Char(c) => s.chars().filter(|&ch| ch == c).count(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "count: pattern must be String or Char, got {:?}",
                            crate::vm::util::value_type_name(&pattern)
                        )));
                    }
                };
                self.stack.push(Value::I64(count as i64));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
