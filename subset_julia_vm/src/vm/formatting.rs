//! Formatting utilities for the VM.
//!
//! This module provides functions for converting Values to string representations
//! in various formats:
//! - `format_value`: Julia-style display format
//! - `format_sprintf`: C-style sprintf formatting

// SAFETY: i64/i32→u32 casts for Char formatting use char::from_u32 which safely
// handles invalid codepoints by returning None (mapped to a fallback character).
#![allow(clippy::cast_sign_loss)]
//! - `value_to_string`: Simple string conversion
//! - `value_to_julia_code`: Julia source code format
//! - `expr_to_julia_string`: Expr to Julia code format

use super::value::{
    ArrayRef, DictValue, ExprValue, MemoryRef, RegexMatchValue, StructInstance, Value,
};
use super::field_indices::{
    ARRAY_FIRST_DIM_INDEX, ARRAY_SECOND_DIM_INDEX, COMPLEX_IMAG_FIELD_INDEX,
    COMPLEX_REAL_FIELD_INDEX, RATIONAL_DENOMINATOR_FIELD_INDEX, RATIONAL_NUMERATOR_FIELD_INDEX,
};

// ============================================================================
// Basic formatting helpers
// ============================================================================

/// Format a Complex struct by formatting its fields directly,
/// preserving type-correct display (e.g., `3.0 + 2.0im` for Float64,
/// `3 + 2im` for Int64).
#[inline]
fn format_complex_struct(s: &StructInstance) -> String {
    if s.values.len() != 2 {
        return "Complex(?, ?)".to_string();
    }
    let re_str = format_value(&s.values[COMPLEX_REAL_FIELD_INDEX]);
    let im_val = &s.values[COMPLEX_IMAG_FIELD_INDEX];
    // Check if imaginary part is negative
    let is_negative = match im_val {
        Value::F64(x) => *x < 0.0,
        Value::I64(x) => *x < 0,
        Value::F32(x) => *x < 0.0,
        Value::I32(x) => *x < 0,
        Value::I16(x) => *x < 0,
        Value::I8(x) => *x < 0,
        _ => false,
    };
    if is_negative {
        let neg_im = match im_val {
            Value::F64(x) => format_value(&Value::F64(-x)),
            Value::I64(x) => format_value(&Value::I64(-x)),
            Value::F32(x) => format_value(&Value::F32(-x)),
            Value::I32(x) => format_value(&Value::I32(-x)),
            Value::I16(x) => format_value(&Value::I16(-x)),
            Value::I8(x) => format_value(&Value::I8(-x)),
            other => format_value(other),
        };
        format!("{} - {}im", re_str, neg_im)
    } else {
        let im_str = format_value(im_val);
        format!("{} + {}im", re_str, im_str)
    }
}

/// Format a struct instance for display.
/// Special cases for well-known types like Rational.
#[inline]
fn format_struct_instance(s: &StructInstance) -> String {
    // Special case: Rational - display as num//den like Julia
    if (s.struct_name == "Rational" || s.struct_name.starts_with("Rational{"))
        && s.values.len() == 2
    {
        let num = format_value(&s.values[RATIONAL_NUMERATOR_FIELD_INDEX]);
        let den = format_value(&s.values[RATIONAL_DENOMINATOR_FIELD_INDEX]);
        return format!("{}//{}", num, den);
    }

    // General case: StructName(field1, field2, ...)
    let fields: Vec<String> = s.values.iter().map(format_value).collect();
    format!("{}({})", s.struct_name, fields.join(", "))
}

/// Format a float value Julia-style: whole numbers get ".0" suffix
#[inline]
pub(crate) fn format_float_julia(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf" } else { "-Inf" }.to_string();
    }

    // Check if it's a whole number (no fractional part)
    if x.fract() == 0.0 && x.abs() < 1e15 {
        // Whole number - format with .0 suffix
        format!("{}.0", x as i64)
    } else {
        // Has fractional part or very large - use default formatting
        x.to_string()
    }
}

/// Format a 32-bit float value Julia-style: whole numbers get ".0" suffix
#[inline]
pub(crate) fn format_float32_julia(x: f32) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf" } else { "-Inf" }.to_string();
    }

    // Check if it's a whole number (no fractional part)
    if x.fract() == 0.0 && x.abs() < 1e7 {
        // Whole number - format with .0 suffix
        format!("{}.0", x as i32)
    } else {
        // Has fractional part or very large - use default formatting
        x.to_string()
    }
}

// ============================================================================
// format_value - Julia-style display format
// ============================================================================

/// Format any Value as a string (for PrintAny instruction).
///
/// Fast path: the most common types (I64, F64, Bool, Str, Nothing) are handled
/// inline so the compiler can keep them on the hot path. All other variants
/// are dispatched to `format_value_slow`, which is marked `#[cold]`.
#[inline]
pub(crate) fn format_value(v: &Value) -> String {
    match v {
        Value::I64(x) => x.to_string(),
        Value::F64(x) => format_float_julia(*x),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Str(s) => s.clone(),
        Value::Nothing => "nothing".to_string(),
        _ => format_value_slow(v),
    }
}

/// Slow path for less common Value variants.
#[cold]
fn format_value_slow(v: &Value) -> String {
    match v {
        // Signed integers (non-I64)
        Value::I8(x) => x.to_string(),
        Value::I16(x) => x.to_string(),
        Value::I32(x) => x.to_string(),
        Value::I64(x) => x.to_string(),
        Value::I128(x) => x.to_string(),
        // Unsigned integers
        Value::U8(x) => x.to_string(),
        Value::U16(x) => x.to_string(),
        Value::U32(x) => x.to_string(),
        Value::U64(x) => x.to_string(),
        Value::U128(x) => x.to_string(),
        // Boolean
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        // Floating point
        Value::F16(x) => format!("Float16({})", x.to_f32()),
        Value::F32(x) => format_float32_julia(*x),
        Value::F64(x) => format_float_julia(*x),
        Value::BigInt(x) => x.to_string(),
        Value::BigFloat(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Char(c) => c.to_string(),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::Struct(s) if s.is_complex() => format_complex_struct(s),
        Value::Array(arr) => format_array_value(arr),
        Value::Range(r) => {
            if r.step == 1.0 {
                format!("{}:{}", r.start, r.stop)
            } else {
                format!("{}:{}:{}", r.start, r.step, r.stop)
            }
        }
        Value::SliceAll => ":".to_string(),
        Value::Struct(s) => format_struct_instance(s),
        Value::StructRef(idx) => format!("StructRef(heap_idx={})", idx),
        Value::Rng(_) => "RNG".to_string(),
        Value::Tuple(t) => {
            let parts: Vec<String> = t.elements.iter().map(format_value).collect();
            format!("({})", parts.join(", "))
        }
        Value::NamedTuple(nt) => {
            let parts: Vec<String> = nt
                .names
                .iter()
                .zip(nt.values.iter())
                .map(|(n, v)| format!("{} = {}", n, format_value(v)))
                .collect();
            format!("({})", parts.join(", "))
        }
        Value::Dict(d) => format_dict_value(d),
        Value::Set(s) => {
            let parts: Vec<String> = s.elements.iter().map(|e| format!("{:?}", e)).collect();
            format!("Set([{}])", parts.join(", "))
        }
        Value::Ref(inner) => format!("Ref({})", format_value(inner)),
        Value::Generator(_) => "Generator(...)".to_string(),
        Value::DataType(jt) => jt.to_string(),
        Value::Module(m) => format!("Module({})", m.name),
        Value::Function(f) => format!("function {}", f.name),
        Value::Closure(c) => {
            if c.captures.is_empty() {
                format!("closure {}", c.name)
            } else {
                let caps: Vec<String> = c.captures.iter().map(|(n, _)| n.clone()).collect();
                format!("closure {} [captures: {}]", c.name, caps.join(", "))
            }
        }
        Value::ComposedFunction(cf) => {
            let outer_str = format_value(&cf.outer);
            let inner_str = format_value(&cf.inner);
            format!("{} ∘ {}", outer_str, inner_str)
        }
        Value::Undef => "#undef".to_string(),
        Value::IO(io_ref) => {
            if io_ref.borrow().is_stdout() {
                "stdout".to_string()
            } else {
                "IOBuffer(...)".to_string()
            }
        }
        // Macro system types
        Value::Symbol(s) => format!(":{}", s.as_str()),
        Value::Expr(e) => e.to_string(),
        Value::QuoteNode(v) => format!("QuoteNode({})", format_value(v)),
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        // Base.Pairs type (for kwargs...)
        Value::Pairs(p) => {
            let parts: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(n, v)| format!(":{} => {}", n, format_value(v)))
                .collect();
            format!("pairs({})", parts.join(", "))
        }
        // Regex types
        Value::Regex(r) => {
            if r.flags.is_empty() {
                format!("r\"{}\"", r.pattern)
            } else {
                format!("r\"{}\"{}", r.pattern, r.flags)
            }
        }
        Value::RegexMatch(m) => format_regexmatch_value(m),
        // Enum type
        Value::Enum { type_name, value } => format!("{}({})", type_name, value),
        // Memory{T} flat typed buffer
        Value::Memory(mem) => format_memory_value(mem),
    }
}

/// Format an Array value for display. Uses index-based access to avoid
/// allocating the full element vector when only the first 100 are shown.
fn format_array_value(arr: &ArrayRef) -> String {
    let arr_borrow = arr.borrow();
    let element_type = arr_borrow.element_type();
    let type_name = element_type.julia_type_name();

    if arr_borrow.shape.len() == 1 {
        // 1D Vector: "n-element Vector{T}:\n elem1\n elem2\n ..."
        let n = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        let display_count = n.min(100);
        let mut lines = Vec::with_capacity(display_count + 2);
        lines.push(format!("{}-element Vector{{{}}}:", n, type_name));
        for i in 0..display_count {
            if let Some(v) = arr_borrow.data.get_value(i) {
                lines.push(format!(" {}", format_value(&v)));
            }
        }
        if n > 100 {
            lines.push(" ...".to_string());
        }
        lines.join("\n")
    } else if arr_borrow.shape.len() == 2 {
        // 2D Matrix: "m×n Matrix{T}:\n row1\n row2\n ..."
        let rows = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        let cols = arr_borrow.shape[ARRAY_SECOND_DIM_INDEX];
        let mut lines = Vec::with_capacity(rows + 1);
        lines.push(format!("{}×{} Matrix{{{}}}:", rows, cols, type_name));
        for r in 0..rows {
            let row: Vec<String> = (0..cols)
                .map(|c| {
                    arr_borrow
                        .data
                        .get_value(r + c * rows)
                        .map_or_else(String::new, |v| format_value(&v))
                })
                .collect();
            lines.push(format!(" {}", row.join("  ")));
        }
        lines.join("\n")
    } else {
        // Higher dimensions: summary (index-based, limit to 100)
        let total = arr_borrow.data.raw_len();
        let display_count = total.min(100);
        let parts: Vec<String> = (0..display_count)
            .filter_map(|i| arr_borrow.data.get_value(i))
            .map(|v| format_value(&v))
            .collect();
        if total > 100 {
            format!(
                "Array{{{}, {}}}[{}, ...]",
                type_name,
                arr_borrow.shape.len(),
                parts.join(", ")
            )
        } else {
            format!(
                "Array{{{}, {}}}[{}]",
                type_name,
                arr_borrow.shape.len(),
                parts.join(", ")
            )
        }
    }
}

/// Format a Dict value for display.
#[cold]
fn format_dict_value(d: &DictValue) -> String {
    let parts: Vec<String> = d
        .iter()
        .map(|(k, v)| format!("{:?} => {}", k, format_value(v)))
        .collect();
    format!("Dict({})", parts.join(", "))
}

/// Format a RegexMatch value for display.
#[cold]
fn format_regexmatch_value(m: &RegexMatchValue) -> String {
    let captures_str = if m.captures.is_empty() {
        String::new()
    } else {
        let caps: Vec<String> = m
            .captures
            .iter()
            .map(|c| match c {
                Some(s) => format!("\"{}\"", s),
                None => "nothing".to_string(),
            })
            .collect();
        format!(", captures=({})", caps.join(", "))
    };
    format!(
        "RegexMatch(\"{}\", offset={}{})",
        m.match_str, m.offset, captures_str
    )
}

/// Format a Memory value for display.
#[cold]
fn format_memory_value(mem: &MemoryRef) -> String {
    let mem = mem.borrow();
    let n = mem.len();
    let type_name = mem.element_type().julia_type_name();
    if n == 0 {
        format!("0-element Memory{{{}}}", type_name)
    } else {
        let display_count = n.min(100);
        let mut lines = Vec::with_capacity(display_count + 2);
        lines.push(format!("{}-element Memory{{{}}}:", n, type_name));
        for i in 1..=display_count {
            if let Ok(v) = mem.get(i) {
                lines.push(format!(" {}", format_value(&v)));
            }
        }
        if n > 100 {
            lines.push(" ...".to_string());
        }
        lines.join("\n")
    }
}

// ============================================================================
// format_sprintf - C-style sprintf formatting
// ============================================================================

/// C-style sprintf formatting
pub(crate) fn format_sprintf(fmt: &str, args: &[Value]) -> String {
    let mut result = String::new();
    let mut chars = fmt.chars().peekable();
    let mut arg_idx = 0;

    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('%') => {
                    result.push('%');
                    chars.next();
                }
                Some(_) => {
                    // Skip flags, width, and precision
                    while chars
                        .peek()
                        .is_some_and(|&c| c == '-' || c == '+' || c == ' ' || c == '#' || c == '0')
                    {
                        chars.next();
                    }
                    // Skip width
                    while chars.peek().is_some_and(|&c| c.is_ascii_digit()) {
                        chars.next();
                    }
                    // Skip precision
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        while chars.peek().is_some_and(|&c| c.is_ascii_digit()) {
                            chars.next();
                        }
                    }
                    // Get type specifier
                    if let Some(&spec) = chars.peek() {
                        chars.next();
                        if arg_idx < args.len() {
                            let formatted = match spec {
                                's' => format_value(&args[arg_idx]),
                                'd' | 'i' => format_sprintf_int(&args[arg_idx]),
                                'f' | 'e' | 'E' | 'g' | 'G' => format_sprintf_float(&args[arg_idx]),
                                'x' => format_sprintf_hex(&args[arg_idx], false),
                                'X' => format_sprintf_hex(&args[arg_idx], true),
                                'o' => format_sprintf_octal(&args[arg_idx]),
                                'c' => format_sprintf_char(&args[arg_idx]),
                                _ => format_value(&args[arg_idx]),
                            };
                            result.push_str(&formatted);
                            arg_idx += 1;
                        }
                    }
                }
                None => result.push('%'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[inline]
fn format_sprintf_int(v: &Value) -> String {
    match v {
        Value::I64(x) => x.to_string(),
        Value::I32(x) => x.to_string(),
        Value::I16(x) => x.to_string(),
        Value::I8(x) => x.to_string(),
        Value::F64(x) => (*x as i64).to_string(),
        Value::F32(x) => (*x as i64).to_string(),
        Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        _ => format_value(v),
    }
}

#[inline]
fn format_sprintf_float(v: &Value) -> String {
    match v {
        Value::F64(x) => x.to_string(),
        Value::F32(x) => x.to_string(),
        Value::I64(x) => (*x as f64).to_string(),
        Value::I32(x) => (*x as f64).to_string(),
        _ => format_value(v),
    }
}

#[inline]
fn format_sprintf_hex(v: &Value, uppercase: bool) -> String {
    let n = match v {
        Value::I64(x) => *x,
        Value::I32(x) => *x as i64,
        Value::I16(x) => *x as i64,
        Value::I8(x) => *x as i64,
        _ => return format_value(v),
    };
    if uppercase {
        format!("{:X}", n)
    } else {
        format!("{:x}", n)
    }
}

#[inline]
fn format_sprintf_octal(v: &Value) -> String {
    let n = match v {
        Value::I64(x) => *x,
        Value::I32(x) => *x as i64,
        Value::I16(x) => *x as i64,
        Value::I8(x) => *x as i64,
        _ => return format_value(v),
    };
    format!("{:o}", n)
}

#[inline]
fn format_sprintf_char(v: &Value) -> String {
    match v {
        Value::Char(c) => c.to_string(),
        Value::I64(x) => char::from_u32(*x as u32).map_or("?".to_string(), |c| c.to_string()),
        Value::I32(x) => char::from_u32(*x as u32).map_or("?".to_string(), |c| c.to_string()),
        _ => format_value(v),
    }
}

// ============================================================================
// value_to_string - Simple string conversion
// ============================================================================

/// Convert a Value to its string representation
pub(crate) fn value_to_string(val: &Value) -> String {
    match val {
        // Signed integers
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        // Unsigned integers
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        // Boolean
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        // Floating point
        Value::F16(f) => format!("Float16({})", f.to_f32()),
        Value::F32(f) => f.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::BigFloat(n) => n.to_string(),
        Value::F64(f) => {
            // Format like Julia: remove trailing zeros but keep decimal point if needed
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                f.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Char(c) => format!("'{}'", c),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::Array(arr) => {
            // Simple array representation
            let arr = arr.borrow();
            let values = arr.to_value_vec();
            let data_str: Vec<String> = values.iter().take(100).map(value_to_string).collect();
            if values.len() > 100 {
                format!("[{}, ...]", data_str.join(", "))
            } else {
                format!("[{}]", data_str.join(", "))
            }
        }
        Value::SliceAll => ":".to_string(),
        Value::Range(r) => {
            if r.is_unit_range() {
                format!("{:.0}:{:.0}", r.start, r.stop)
            } else {
                format!("{:.0}:{:.0}:{:.0}", r.start, r.step, r.stop)
            }
        }
        Value::Struct(s) if s.is_complex() => format_complex_struct(s),
        Value::Struct(s) => {
            // Simple struct representation
            let fields_str: Vec<String> = s.values.iter().map(value_to_string).collect();
            format!("Struct({})", fields_str.join(", "))
        }
        Value::StructRef(idx) => format!("StructRef({})", idx),
        Value::Rng(_) => "RNG".to_string(),
        Value::Tuple(t) => {
            let elements_str: Vec<String> = t.elements.iter().map(value_to_string).collect();
            format!("({})", elements_str.join(", "))
        }
        Value::NamedTuple(nt) => {
            let fields_str: Vec<String> = nt
                .names
                .iter()
                .zip(nt.values.iter())
                .map(|(name, val)| format!("{} = {}", name, value_to_string(val)))
                .collect();
            format!("({})", fields_str.join(", "))
        }
        Value::Dict(d) => {
            let pairs_str: Vec<String> = d
                .iter()
                .map(|(k, v)| format!("{} => {}", k, value_to_string(v)))
                .collect();
            format!("Dict({})", pairs_str.join(", "))
        }
        Value::Set(s) => {
            let elements_str: Vec<String> = s.elements.iter().map(|e| format!("{}", e)).collect();
            format!("Set([{}])", elements_str.join(", "))
        }
        Value::Ref(inner) => format!("Ref({})", value_to_string(inner)),
        Value::Generator(_) => "Generator(...)".to_string(),
        Value::DataType(jt) => jt.to_string(),
        Value::Module(m) => m.name.clone(),
        Value::Function(f) => format!("function {}", f.name),
        Value::Closure(c) => {
            if c.captures.is_empty() {
                format!("closure {}", c.name)
            } else {
                let caps: Vec<String> = c.captures.iter().map(|(n, _)| n.clone()).collect();
                format!("closure {} [captures: {}]", c.name, caps.join(", "))
            }
        }
        Value::ComposedFunction(cf) => {
            let outer_str = value_to_string(&cf.outer);
            let inner_str = value_to_string(&cf.inner);
            format!("{} ∘ {}", outer_str, inner_str)
        }
        Value::Undef => "#undef".to_string(),
        Value::IO(io_ref) => {
            if io_ref.borrow().is_stdout() {
                "stdout".to_string()
            } else {
                "IOBuffer(...)".to_string()
            }
        }
        // Macro system types
        Value::Symbol(s) => format!(":{}", s.as_str()),
        Value::Expr(e) => e.to_string(),
        Value::QuoteNode(v) => format!("QuoteNode({})", value_to_string(v)),
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        // Base.Pairs type (for kwargs...)
        Value::Pairs(p) => {
            let pairs_str: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(name, val)| format!(":{} => {}", name, value_to_string(val)))
                .collect();
            format!("pairs({})", pairs_str.join(", "))
        }
        // Regex types
        Value::Regex(r) => {
            if r.flags.is_empty() {
                format!("r\"{}\"", r.pattern)
            } else {
                format!("r\"{}\"{}", r.pattern, r.flags)
            }
        }
        Value::RegexMatch(m) => {
            let captures_str = if m.captures.is_empty() {
                String::new()
            } else {
                let caps: Vec<String> = m
                    .captures
                    .iter()
                    .map(|c| match c {
                        Some(s) => format!("\"{}\"", s),
                        None => "nothing".to_string(),
                    })
                    .collect();
                format!(", captures=({})", caps.join(", "))
            };
            format!(
                "RegexMatch(\"{}\", offset={}{})",
                m.match_str, m.offset, captures_str
            )
        }
        // Enum type
        Value::Enum { type_name, value } => format!("{}({})", type_name, value),
        // Memory type
        Value::Memory(mem) => {
            let mem = mem.borrow();
            let n = mem.len();
            let type_name = mem.element_type().julia_type_name();
            format!("{}-element Memory{{{}}}", n, type_name)
        }
    }
}

// ============================================================================
// Julia code format stringification for Expr and Symbol
// ============================================================================

/// Operator precedence table (higher = binds tighter)
/// Based on Julia's operator precedence
#[inline]
fn operator_precedence(op: &str) -> i32 {
    match op {
        // Assignment (lowest)
        "=" | "+=" | "-=" | "*=" | "/=" | "\\=" | "^=" | "&=" | "|=" | "÷=" | "%=" => 1,
        // Pair
        "=>" => 2,
        // Ternary
        "?" => 3,
        // Or
        "||" => 4,
        // And
        "&&" => 5,
        // Comparison
        "<" | ">" | "<=" | ">=" | "==" | "!=" | "===" | "!==" | "<:" | ">:" | "≤" | "≥" | "≠"
        | "≡" | "≢" => 6,
        // Range
        ":" => 7,
        // Plus
        "+" | "-" | "|" | "⊻" => 11,
        // Times
        "*" | "/" | "÷" | "%" | "&" | "\\" => 12,
        // Rational
        "//" => 13,
        // Power
        "^" => 14,
        // Type declaration
        "::" => 15,
        // Dot
        "." => 17,
        // Not an operator
        _ => 0,
    }
}

/// Check if operator is a unary operator
#[inline]
fn is_unary_op(op: &str) -> bool {
    matches!(op, "+" | "-" | "!" | "~" | "¬" | "√" | "∛" | "∜")
}

/// Convert a Value to Julia code format string (used recursively in expressions)
pub(crate) fn value_to_julia_code(val: &Value) -> String {
    match val {
        Value::Symbol(s) => s.as_str().to_string(),
        Value::Expr(e) => expr_to_julia_string(e),
        Value::I64(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::F64(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{:.1}", n)
            } else {
                n.to_string()
            }
        }
        Value::F32(n) => n.to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Str(s) => format!("\"{}\"", s),
        Value::Char(c) => format!("'{}'", c),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::QuoteNode(inner) => format!("QuoteNode({})", value_to_julia_code(inner)),
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        Value::Tuple(t) => {
            let parts: Vec<String> = t.elements.iter().map(value_to_julia_code).collect();
            if parts.len() == 1 {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        Value::Array(arr) => {
            let arr = arr.borrow();
            let values = arr.to_value_vec();
            let parts: Vec<String> = values.iter().take(100).map(value_to_julia_code).collect();
            if values.len() > 100 {
                format!("[{}, ...]", parts.join(", "))
            } else {
                format!("[{}]", parts.join(", "))
            }
        }
        // Memory → Array (Issue #2764)
        Value::Memory(mem) => {
            let arr = super::util::memory_to_array_ref(mem);
            let arr = arr.borrow();
            let values = arr.to_value_vec();
            let parts: Vec<String> = values.iter().take(100).map(value_to_julia_code).collect();
            if values.len() > 100 {
                format!("[{}, ...]", parts.join(", "))
            } else {
                format!("[{}]", parts.join(", "))
            }
        }
        // Fall back to format_value for other types
        _ => format_value(val),
    }
}

/// Convert an Expr to Julia code format string
pub(crate) fn expr_to_julia_string(expr: &ExprValue) -> String {
    let head = expr.head.as_str();
    let args = &expr.args;

    match head {
        "call" => format_call(args),
        "tuple" => format_tuple(args),
        "vect" => format_vect(args),
        "ref" => format_ref(args),
        "." => format_dot(args),
        "block" => format_block(args),
        "quote" => format_quote(args),
        "comparison" => format_comparison(args),
        "&&" => format_short_circuit("&&", args),
        "||" => format_short_circuit("||", args),
        "if" => format_if(args),
        "=" => format_assignment(args),
        "kw" => format_kw(args),
        "parameters" => format_parameters(args),
        "curly" => format_curly(args),
        "string" => format_string_interpolation(args),
        "macrocall" => format_macrocall(args),
        // Fallback: show as Expr(...) for unsupported heads
        _ => {
            let args_str: Vec<String> = args.iter().map(value_to_julia_code).collect();
            if args_str.is_empty() {
                format!("Expr(:{})", head)
            } else {
                format!("Expr(:{}, {})", head, args_str.join(", "))
            }
        }
    }
}

/// Format a :call expression
fn format_call(args: &[Value]) -> String {
    if args.is_empty() {
        return "()".to_string();
    }

    // First argument is the function/operator
    let func = &args[0];
    let func_name = match func {
        Value::Symbol(s) => s.as_str(),
        // For non-symbol callables, use function call syntax
        _ => {
            let func_str = value_to_julia_code(func);
            let func_args = &args[1..];
            let args_str: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
            return format!("({})({})", func_str, args_str.join(", "));
        }
    };

    let prec = operator_precedence(func_name);
    let func_args = &args[1..];

    // Binary operator with 2 arguments
    if prec > 0 && func_args.len() == 2 {
        let left = value_to_julia_code(&func_args[0]);
        let right = value_to_julia_code(&func_args[1]);
        format!("{} {} {}", left, func_name, right)
    }
    // Unary operator with 1 argument
    else if is_unary_op(func_name) && func_args.len() == 1 {
        let operand = value_to_julia_code(&func_args[0]);
        // Check if operand needs parentheses (if it's a complex expression)
        if matches!(&func_args[0], Value::Expr(_)) {
            format!("{}({})", func_name, operand)
        } else {
            format!("{}{}", func_name, operand)
        }
    }
    // N-ary operators like + and * with more than 2 arguments
    else if (func_name == "+" || func_name == "*") && func_args.len() > 2 {
        let parts: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
        parts.join(&format!(" {} ", func_name))
    }
    // Range operator :
    else if func_name == ":" && (func_args.len() == 2 || func_args.len() == 3) {
        let parts: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
        parts.join(":")
    }
    // Regular function call
    else {
        let args_str: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
        format!("{}({})", func_name, args_str.join(", "))
    }
}

/// Format a :tuple expression
fn format_tuple(args: &[Value]) -> String {
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    if parts.len() == 1 {
        format!("({},)", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

/// Format a :vect expression (array literal)
fn format_vect(args: &[Value]) -> String {
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    format!("[{}]", parts.join(", "))
}

/// Format a :ref expression (indexing)
fn format_ref(args: &[Value]) -> String {
    if args.is_empty() {
        return "[]".to_string();
    }
    let array = value_to_julia_code(&args[0]);
    let indices: Vec<String> = args[1..].iter().map(value_to_julia_code).collect();
    format!("{}[{}]", array, indices.join(", "))
}

/// Format a :. (dot) expression (field access or broadcasting)
fn format_dot(args: &[Value]) -> String {
    if args.len() >= 2 {
        let obj = value_to_julia_code(&args[0]);
        // Second arg could be QuoteNode or Symbol for field access
        let field = match &args[1] {
            Value::QuoteNode(inner) => value_to_julia_code(inner),
            Value::Symbol(s) => s.as_str().to_string(),
            other => value_to_julia_code(other),
        };
        format!("{}.{}", obj, field)
    } else if args.len() == 1 {
        value_to_julia_code(&args[0])
    } else {
        ".".to_string()
    }
}

/// Format a :block expression
fn format_block(args: &[Value]) -> String {
    // Filter out LineNumberNode
    let stmts: Vec<String> = args
        .iter()
        .filter(|a| !matches!(a, Value::LineNumberNode(_)))
        .map(value_to_julia_code)
        .collect();

    if stmts.is_empty() {
        "begin\nend".to_string()
    } else if stmts.len() == 1 {
        stmts[0].clone()
    } else {
        format!("begin\n    {}\nend", stmts.join("\n    "))
    }
}

/// Format a :quote expression
fn format_quote(args: &[Value]) -> String {
    if args.len() == 1 {
        let inner = value_to_julia_code(&args[0]);
        format!(":({})", inner)
    } else {
        let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
        format!("quote {} end", parts.join("; "))
    }
}

/// Format a :comparison expression
fn format_comparison(args: &[Value]) -> String {
    // comparison has format: [left, op, right, op2, right2, ...]
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    parts.join(" ")
}

/// Format && or || expression
fn format_short_circuit(op: &str, args: &[Value]) -> String {
    if args.len() == 2 {
        let left = value_to_julia_code(&args[0]);
        let right = value_to_julia_code(&args[1]);
        format!("{} {} {}", left, op, right)
    } else {
        let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
        parts.join(&format!(" {} ", op))
    }
}

/// Format an :if expression
fn format_if(args: &[Value]) -> String {
    if args.len() >= 2 {
        let cond = value_to_julia_code(&args[0]);
        let then_branch = value_to_julia_code(&args[1]);
        if args.len() >= 3 {
            let else_branch = value_to_julia_code(&args[2]);
            format!(
                "if {}\n    {}\nelse\n    {}\nend",
                cond, then_branch, else_branch
            )
        } else {
            format!("if {}\n    {}\nend", cond, then_branch)
        }
    } else {
        "if ... end".to_string()
    }
}

/// Format an := (assignment) expression
fn format_assignment(args: &[Value]) -> String {
    if args.len() == 2 {
        let lhs = value_to_julia_code(&args[0]);
        let rhs = value_to_julia_code(&args[1]);
        format!("{} = {}", lhs, rhs)
    } else {
        "= ...".to_string()
    }
}

/// Format a :kw expression (keyword argument)
fn format_kw(args: &[Value]) -> String {
    if args.len() == 2 {
        let name = value_to_julia_code(&args[0]);
        let value = value_to_julia_code(&args[1]);
        format!("{} = {}", name, value)
    } else {
        "kw(...)".to_string()
    }
}

/// Format a :parameters expression (keyword arguments after semicolon)
fn format_parameters(args: &[Value]) -> String {
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    format!("; {}", parts.join(", "))
}

/// Format a :curly expression (type parameters)
fn format_curly(args: &[Value]) -> String {
    if args.is_empty() {
        return "{}".to_string();
    }
    let base = value_to_julia_code(&args[0]);
    let params: Vec<String> = args[1..].iter().map(value_to_julia_code).collect();
    format!("{}{{{}}}", base, params.join(", "))
}

/// Format a :string expression (string interpolation)
fn format_string_interpolation(args: &[Value]) -> String {
    let mut result = String::new();
    result.push('"');
    for arg in args {
        match arg {
            Value::Str(s) => result.push_str(s),
            _ => {
                result.push_str("$(");
                result.push_str(&value_to_julia_code(arg));
                result.push(')');
            }
        }
    }
    result.push('"');
    result
}

/// Format a :macrocall expression
fn format_macrocall(args: &[Value]) -> String {
    if args.is_empty() {
        return "@...".to_string();
    }
    let macro_name = match &args[0] {
        Value::Symbol(s) => s.as_str().to_string(),
        other => value_to_julia_code(other),
    };
    // Skip LineNumberNode if present (usually args[1])
    let macro_args: Vec<String> = args[1..]
        .iter()
        .filter(|a| !matches!(a, Value::LineNumberNode(_)))
        .map(value_to_julia_code)
        .collect();
    if macro_args.is_empty() {
        macro_name
    } else {
        format!("{} {}", macro_name, macro_args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_float_julia ────────────────────────────────────────────────────

    #[test]
    fn test_format_float_julia_nan() {
        assert_eq!(format_float_julia(f64::NAN), "NaN");
    }

    #[test]
    fn test_format_float_julia_positive_infinity() {
        assert_eq!(format_float_julia(f64::INFINITY), "Inf");
    }

    #[test]
    fn test_format_float_julia_negative_infinity() {
        assert_eq!(format_float_julia(f64::NEG_INFINITY), "-Inf");
    }

    #[test]
    fn test_format_float_julia_whole_number_gets_dot_zero() {
        // Julia prints 1.0, 42.0, -7.0 etc. for whole floats
        assert_eq!(format_float_julia(1.0_f64), "1.0");
        assert_eq!(format_float_julia(42.0_f64), "42.0");
        assert_eq!(format_float_julia(-7.0_f64), "-7.0");
        assert_eq!(format_float_julia(0.0_f64), "0.0");
    }

    #[test]
    fn test_format_float_julia_fractional() {
        // Fractional numbers use default Rust formatting (same as Julia)
        let result = format_float_julia(1.25_f64);
        assert!(
            result.contains('.'),
            "Fractional float should contain '.', got: {}",
            result
        );
        assert!(result.starts_with("1."), "Expected '1.25...', got: {}", result);
    }

    #[test]
    fn test_format_float_julia_very_large_number() {
        // Numbers >= 1e15 use default formatting (no .0 suffix)
        let result = format_float_julia(1e15_f64);
        // Should NOT produce "1000000000000000.0" — uses Rust's default exponent fmt
        assert!(
            !result.ends_with("000000000000000.0"),
            "Large numbers should not get .0 suffix, got: {}",
            result
        );
    }

    // ── format_float32_julia ──────────────────────────────────────────────────

    #[test]
    fn test_format_float32_julia_nan() {
        assert_eq!(format_float32_julia(f32::NAN), "NaN");
    }

    #[test]
    fn test_format_float32_julia_positive_infinity() {
        assert_eq!(format_float32_julia(f32::INFINITY), "Inf");
    }

    #[test]
    fn test_format_float32_julia_negative_infinity() {
        assert_eq!(format_float32_julia(f32::NEG_INFINITY), "-Inf");
    }

    #[test]
    fn test_format_float32_julia_whole_number_gets_dot_zero() {
        assert_eq!(format_float32_julia(1.0_f32), "1.0");
        assert_eq!(format_float32_julia(0.0_f32), "0.0");
        assert_eq!(format_float32_julia(-5.0_f32), "-5.0");
    }

    // ── operator_precedence ───────────────────────────────────────────────────

    #[test]
    fn test_operator_precedence_power_is_highest_arithmetic() {
        let pow = operator_precedence("^");
        let mul = operator_precedence("*");
        let add = operator_precedence("+");
        assert!(pow > mul, "^ should have higher precedence than *");
        assert!(mul > add, "* should have higher precedence than +");
    }

    #[test]
    fn test_operator_precedence_comparison_lower_than_arithmetic() {
        let cmp = operator_precedence("==");
        let add = operator_precedence("+");
        assert!(cmp < add, "== should have lower precedence than +");
    }

    #[test]
    fn test_operator_precedence_assignment_is_lowest() {
        let assign = operator_precedence("=");
        let or = operator_precedence("||");
        assert!(assign < or, "= should have lower precedence than ||");
    }

    #[test]
    fn test_operator_precedence_unknown_is_zero() {
        assert_eq!(operator_precedence("not_an_op"), 0);
        assert_eq!(operator_precedence(""), 0);
    }

    #[test]
    fn test_operator_precedence_rational_slash_slash() {
        // // (rational division) should be higher than * (12)
        let rational = operator_precedence("//");
        let mul = operator_precedence("*");
        assert!(rational > mul, "// should have higher precedence than *");
    }

    // ── is_unary_op ───────────────────────────────────────────────────────────

    #[test]
    fn test_is_unary_op_standard_unary() {
        assert!(is_unary_op("+"), "+ is a unary op");
        assert!(is_unary_op("-"), "- is a unary op");
        assert!(is_unary_op("!"), "! is a unary op");
        assert!(is_unary_op("~"), "~ is a unary op");
    }

    #[test]
    fn test_is_unary_op_unicode_unary() {
        assert!(is_unary_op("√"), "√ is a unary op");
        assert!(is_unary_op("∛"), "∛ is a unary op");
        assert!(is_unary_op("∜"), "∜ is a unary op");
        assert!(is_unary_op("¬"), "¬ is a unary op");
    }

    #[test]
    fn test_is_unary_op_binary_only_operators() {
        assert!(!is_unary_op("*"), "* is not a unary op");
        assert!(!is_unary_op("/"), "/ is not a unary op");
        assert!(!is_unary_op("&&"), "&& is not a unary op");
        assert!(!is_unary_op("=="), "== is not a unary op");
        assert!(!is_unary_op("^"), "^ is not a unary op");
    }

    // ── format_value (scalar cases) ───────────────────────────────────────────

    #[test]
    fn test_format_value_i64() {
        assert_eq!(format_value(&Value::I64(42)), "42");
        assert_eq!(format_value(&Value::I64(-7)), "-7");
        assert_eq!(format_value(&Value::I64(0)), "0");
    }

    #[test]
    fn test_format_value_f64_whole_number() {
        // Whole-number floats get ".0" suffix (Julia style)
        assert_eq!(format_value(&Value::F64(1.0)), "1.0");
        assert_eq!(format_value(&Value::F64(0.0)), "0.0");
        assert_eq!(format_value(&Value::F64(-3.0)), "-3.0");
    }

    #[test]
    fn test_format_value_bool() {
        assert_eq!(format_value(&Value::Bool(true)), "true");
        assert_eq!(format_value(&Value::Bool(false)), "false");
    }

    #[test]
    fn test_format_value_str() {
        assert_eq!(format_value(&Value::Str("hello".to_string())), "hello");
        assert_eq!(format_value(&Value::Str(String::new())), "");
    }

    #[test]
    fn test_format_value_nothing() {
        assert_eq!(format_value(&Value::Nothing), "nothing");
    }

    #[test]
    fn test_format_value_missing() {
        assert_eq!(format_value(&Value::Missing), "missing");
    }

    // ── format_sprintf ────────────────────────────────────────────────────────

    #[test]
    fn test_format_sprintf_percent_escape() {
        assert_eq!(format_sprintf("%%", &[]), "%");
        assert_eq!(format_sprintf("100%%", &[]), "100%");
    }

    #[test]
    fn test_format_sprintf_d_integer() {
        assert_eq!(format_sprintf("%d", &[Value::I64(42)]), "42");
        assert_eq!(format_sprintf("%d", &[Value::I64(-7)]), "-7");
        assert_eq!(format_sprintf("%d", &[Value::I64(0)]), "0");
    }

    #[test]
    fn test_format_sprintf_s_string() {
        assert_eq!(
            format_sprintf("%s", &[Value::Str("hello".to_string())]),
            "hello"
        );
    }

    #[test]
    fn test_format_sprintf_x_hex_lowercase() {
        assert_eq!(format_sprintf("%x", &[Value::I64(255)]), "ff");
        assert_eq!(format_sprintf("%x", &[Value::I64(16)]), "10");
    }

    #[test]
    fn test_format_sprintf_x_hex_uppercase() {
        assert_eq!(format_sprintf("%X", &[Value::I64(255)]), "FF");
        assert_eq!(format_sprintf("%X", &[Value::I64(16)]), "10");
    }

    #[test]
    fn test_format_sprintf_o_octal() {
        assert_eq!(format_sprintf("%o", &[Value::I64(8)]), "10");
        assert_eq!(format_sprintf("%o", &[Value::I64(255)]), "377");
    }

    #[test]
    fn test_format_sprintf_literal_text_passthrough() {
        assert_eq!(format_sprintf("hello", &[]), "hello");
        assert_eq!(
            format_sprintf("x=%d, y=%d", &[Value::I64(1), Value::I64(2)]),
            "x=1, y=2"
        );
    }

    // ── value_to_string ───────────────────────────────────────────────────────

    #[test]
    fn test_value_to_string_i64() {
        assert_eq!(value_to_string(&Value::I64(100)), "100");
        assert_eq!(value_to_string(&Value::I64(-1)), "-1");
    }

    #[test]
    fn test_value_to_string_bool() {
        assert_eq!(value_to_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_string(&Value::Bool(false)), "false");
    }

    #[test]
    fn test_value_to_string_str_is_unquoted() {
        // value_to_string returns the raw string without quotes (unlike repr)
        assert_eq!(value_to_string(&Value::Str("hi".to_string())), "hi");
    }

    #[test]
    fn test_value_to_string_nothing() {
        assert_eq!(value_to_string(&Value::Nothing), "nothing");
    }

    #[test]
    fn test_value_to_string_f64_whole_number() {
        // value_to_string also applies Julia-style .0 suffix for whole floats
        assert_eq!(value_to_string(&Value::F64(5.0)), "5.0");
        assert_eq!(value_to_string(&Value::F64(0.0)), "0.0");
    }
}
