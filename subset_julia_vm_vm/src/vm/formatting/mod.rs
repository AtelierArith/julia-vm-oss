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

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::field_indices::{
    ARRAY_FIRST_DIM_INDEX, ARRAY_SECOND_DIM_INDEX, COMPLEX_IMAG_FIELD_INDEX,
    COMPLEX_REAL_FIELD_INDEX, RATIONAL_DENOMINATOR_FIELD_INDEX, RATIONAL_NUMERATOR_FIELD_INDEX,
};
use super::value::{
    array_wrapper_shape_and_offset, native_array_value_ref, ArrayRef, MemoryRef, MemoryRefValue,
    RangeElementType, RangeValue, RegexMatchValue, RustBigInt, StructInstance, Value,
};
use crate::vm::value::{is_array_wrapper_struct_name, is_native_array_value};
use subset_julia_vm_bytecode::ArrayElementType;

// Formatting split by category (Issue #6835). The value-display dispatch core
// (struct/array/value) stays here; these submodules hold the self-contained
// leaf categories.
mod julia_code;
mod numeric;
mod sprintf;

pub(crate) use julia_code::expr_to_julia_string;
pub use numeric::format_bigfloat_julia;
pub(crate) use numeric::{format_float16_julia, format_float32_julia, format_float_julia};
pub(crate) use sprintf::{format_printf_float, format_sprintf};

// ============================================================================
// Basic formatting helpers
// ============================================================================

/// Format an `@enum` value (Issue #5139). Looks up the member name in the
/// thread-local enum registry so `red` prints as `red`; falls back to the
/// upstream `Type(value)` form when the type is not registered (e.g. a stale
/// cached value with no live registry entry).
fn format_enum_value(type_name: &str, value: i64) -> String {
    super::value::enum_registry::member_name(type_name, value)
        .unwrap_or_else(|| format!("{}({})", type_name, value))
}

/// Render `Complex{FloatNN}` type names via their `ComplexFNN` aliases for
/// DISPLAY only (Issue #5704). Mirrors upstream Julia, which shows
/// `Complex{Float64}` as `ComplexF64` (and recursively, e.g.
/// `Vector{Complex{Float64}}` → `Vector{ComplexF64}`). The replacement happens
/// at the display site rather than in `JuliaType::name()` because dispatch and
/// `is_complex_type_name` rely on the canonical `Complex{...}` spelling. Only
/// matches at a type-name boundary (start, or preceded by a non-identifier
/// char) so user types like `MyComplex{Float64}` are left untouched.
pub fn apply_complex_float_aliases(s: &str) -> String {
    const ALIASES: [(&str, &str); 3] = [
        ("Complex{Float64}", "ComplexF64"),
        ("Complex{Float32}", "ComplexF32"),
        ("Complex{Float16}", "ComplexF16"),
    ];
    if !s.contains("Complex{Float") {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    let mut prev_is_ident = false;
    while !rest.is_empty() {
        let mut matched = false;
        if !prev_is_ident {
            for (pat, alias) in ALIASES.iter() {
                if rest.starts_with(pat) {
                    result.push_str(alias);
                    rest = &rest[pat.len()..];
                    // The char preceding the next iteration is the alias's last
                    // char (a digit), i.e. an identifier char.
                    prev_is_ident = true;
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            let Some(ch) = rest.chars().next() else {
                break;
            };
            result.push(ch);
            prev_is_ident = ch.is_ascii_alphanumeric() || ch == '_';
            rest = &rest[ch.len_utf8()..];
        }
    }
    result
}

/// Format a runtime `TypeVar` the way upstream Julia does (Issue #4698):
/// an unbounded `T` prints as `T`, an upper-bounded one as `T<:Upper`, and a
/// fully bounded one as `Lower<:T<:Upper`. The default bounds are
/// `Union{}` (`JuliaType::Bottom`) for the lower bound and `Any` for the
/// upper bound, which are omitted.
fn format_runtime_typevar(tv: &super::value::RuntimeTypeVarValue) -> String {
    use crate::types::JuliaType;
    let has_lower = !matches!(tv.lower_bound, JuliaType::Bottom);
    let has_upper = !matches!(tv.upper_bound, JuliaType::Any);
    // An ANONYMOUS bounded typevar — the internal placeholder name `_`, produced
    // when parsing the covariant shorthand `Vector{<:Integer}` / `Vector{>:Int}`
    // — prints with the bound-only shorthand upstream (`<:Upper` / `>:Lower`),
    // never echoing the `_` placeholder (Issue #5644). A named typevar keeps its
    // name (`T<:Integer`). When an anonymous var carries BOTH bounds upstream
    // assigns it a real name and a `where` clause, so the `_` shorthand does not
    // apply there — fall back to the explicit `Lower<:_<:Upper` spelling.
    let anonymous = tv.name == "_";
    let lower = format_typevar_bound(&tv.lower_bound);
    let upper = format_typevar_bound(&tv.upper_bound);
    match (has_lower, has_upper) {
        (false, false) => tv.name.clone(),
        (false, true) if anonymous => format!("<:{upper}"),
        (false, true) => format!("{}<:{upper}", tv.name),
        (true, false) if anonymous => format!(">:{lower}"),
        (true, false) => format!("{lower}<:{}", tv.name),
        (true, true) => format!("{lower}<:{}<:{upper}", tv.name),
    }
}

fn format_typevar_bound(bound: &crate::types::JuliaType) -> String {
    match bound {
        crate::types::JuliaType::TypeVar(name, _) => name.clone(),
        other => other.to_string(),
    }
}

/// Format a Complex struct by formatting its fields directly,
/// preserving type-correct display (e.g., `3.0 + 2.0im` for Float64,
/// `3 + 2im` for Int64).
///
/// This is a Rust fallback used only by non-VM display contexts (error
/// messages, `Display` impls, array-element formatting) that cannot dispatch
/// to the pure-Julia `Base.show(io, ::Complex)` defined in
/// `src/julia/base/complex.jl`. It is kept byte-for-byte consistent with that
/// `show` method — including the `Complex{Bool}` / imaginary-unit special
/// cases — so every path matches upstream Julia (Issue #5155).
#[inline]
fn format_complex_struct(s: &StructInstance) -> String {
    if s.values.len() != 2 {
        return "Complex(?, ?)".to_string();
    }
    // Complex{Bool}: upstream prints `im` for the imaginary unit and
    // `Complex(re,im)` otherwise (julia/base/complex.jl:215-216). Mirror the
    // pure-Julia `Base.show(io, ::Complex)` branch exactly.
    if let (Value::Bool(re), Value::Bool(im)) = (
        &s.values[COMPLEX_REAL_FIELD_INDEX],
        &s.values[COMPLEX_IMAG_FIELD_INDEX],
    ) {
        if !*re && *im {
            return "im".to_string();
        }
        return format!("Complex({},{})", re, im);
    }
    let re_str = format_value_impl(&s.values[COMPLEX_REAL_FIELD_INDEX]);
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
            Value::F64(x) => format_value_impl(&Value::F64(-x)),
            Value::I64(x) => format_value_impl(&Value::I64(-x)),
            Value::F32(x) => format_value_impl(&Value::F32(-x)),
            Value::I32(x) => format_value_impl(&Value::I32(-x)),
            Value::I16(x) => format_value_impl(&Value::I16(-x)),
            Value::I8(x) => format_value_impl(&Value::I8(-x)),
            other => format_value_impl(other),
        };
        format!("{} - {}im", re_str, neg_im)
    } else {
        let im_str = format_value_impl(im_val);
        format!("{} + {}im", re_str, im_str)
    }
}

/// Format a value in show form: quote strings, single-quote chars,
/// escape special characters. Used for struct field display so that
/// `print(Foo("hi"))` matches upstream Julia's `Foo("hi")` rather than
/// the bare-print form `Foo(hi)` (Issue #4764).
///
/// For non-String/Char values, falls back to the plain `format_value`
/// path so numeric and nested-struct fields render unchanged.
/// Format a NamedTuple for display, matching upstream Julia: an empty NamedTuple
/// is `NamedTuple()`, a single field gets a trailing comma `(a = 1,)` (so it is not
/// confused with a parenthesized assignment), and otherwise `(a = 1, b = 2)`
/// (Issue #5776).
pub(crate) fn format_named_tuple_value(names: &[String], values: &[Value]) -> String {
    if names.is_empty() {
        return "NamedTuple()".to_string();
    }
    let parts: Vec<String> = names
        .iter()
        .zip(values.iter())
        .map(|(n, v)| format!("{} = {}", n, format_value_show_field(v)))
        .collect();
    if parts.len() == 1 {
        format!("({},)", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

pub(crate) fn format_value_show_field(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("\"{}\"", escape_string_for_show(s)),
        Value::StrBytes(bytes) => format!(
            "\"{}\"",
            escape_string_for_show(&String::from_utf8_lossy(bytes.as_ref()))
        ),
        Value::Char(c) => format!("'{}'", escape_char_for_show(*c)),
        Value::CharMalformed(bits) => format!("'{}'", escape_char_malformed_for_show(*bits)),
        Value::U8(n) => format!("0x{n:02x}"),
        Value::U16(n) => format!("0x{n:04x}"),
        Value::U32(n) => format!("0x{n:08x}"),
        Value::U64(n) => format!("0x{n:016x}"),
        Value::U128(n) => format!("0x{n:032x}"),
        _ => format_value_impl(v),
    }
}

/// String escape table for struct-field show form (Issue #4764).
/// Mirrors `escape_for_repr_str` in `vm/builtins_strings.rs` (which is
/// itself a port of Pure-Julia `Base.escape_string`).
fn escape_string_for_show(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            other => out.push(other),
        }
    }
    out
}

/// Show-form of a malformed Char (Issue #8995): each pattern byte rendered as
/// a `\xNN` escape, matching upstream `repr('\xff') == "'\\xff'"`.
fn escape_char_malformed_for_show(bits: u32) -> String {
    let (bytes, len) = crate::vm::value::julia_char_pattern_bytes(bits);
    let mut out = String::with_capacity(4 * len);
    for &b in &bytes[..len] {
        out.push_str(&format!("\\x{:02x}", b));
    }
    out
}

/// Char escape table for struct-field show form (Issue #4764).
fn escape_char_for_show(c: char) -> String {
    match c {
        '\\' => "\\\\".to_string(),
        '\'' => "\\'".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\0' => "\\0".to_string(),
        other => other.to_string(),
    }
}

/// Detect the Pure-Julia `Array{T}` wrapper struct name (Issue #4770).
fn is_array_wrapper_struct(name: &str) -> bool {
    is_array_wrapper_struct_name(name)
}

/// Extract the element type parameter from a `"Array{T}"` struct name,
/// returning `T` (e.g. `"Int64"` for `"Array{Int64}"`). Returns `"Any"`
/// when the parameter is missing or malformed (Issue #4770).
fn array_wrapper_eltype_name(name: &str) -> &str {
    let trimmed = name.split_once('{').map(|(_, params)| params).unwrap_or("");
    let inner = trimmed.strip_suffix('}').unwrap_or(trimmed);
    if inner.is_empty() {
        return "Any";
    }
    // The faithful `Array{T,N}` struct name carries the ndims param `N` after a
    // top-level comma; the element type is the first param (Issue #6649). Take
    // everything before the first top-level comma, respecting nested `{...}`.
    let mut depth = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let eltype = inner[..i].trim();
                return if eltype.is_empty() { "Any" } else { eltype };
            }
            _ => {}
        }
    }
    let eltype = inner.trim();
    if eltype.is_empty() {
        "Any"
    } else {
        eltype
    }
}

/// Decode the `(dims, offset?)` shape from the Pure-Julia Array wrapper
/// `_size` field. Returns `(shape, offset_1_indexed)`. Mirrors
/// `array_wrapper_shape_and_offset` in `vm/builtins_strings.rs` but
/// returns `None` rather than `VmError` for the display path which has
/// no error channel (Issue #4770).
/// Format a Pure-Julia `Array{T}` wrapper struct as the compact
/// `[a, b, c]` / `[a b; c d]` / `T[]` form that upstream Julia's
/// `show(io, ::Array)` produces (Issue #4770). The Pure-Julia
/// `show(io, ::Array)` method in `julia/base/io.jl` already produces
/// this form, but the print path (`format_value_print`) goes through
/// `format_struct_instance` instead of the user `show` method.
///
/// Returns `None` if the struct is not recognizable as an Array
/// wrapper or the shape decoding fails, so the caller falls through
/// to the generic struct-field-dump path.
/// Decode an `Array{T,N}` wrapper struct's storage into `(shape, elements,
/// element_type)` in column-major linear order. Returns `None` when `s` is not
/// a recognizable array-wrapper struct or its carrier cannot be read. Shared by
/// the compact formatter and the VM-side user-`show` pre-render path
/// (Issue #7893).
fn array_wrapper_decode(s: &StructInstance) -> Option<(Vec<usize>, Vec<Value>, ArrayElementType)> {
    if !is_array_wrapper_struct(&s.struct_name) {
        return None;
    }
    let mem = s.values.first()?;
    let size = s.values.get(1)?;
    let (shape, offset) = array_wrapper_shape_and_offset(size)?;
    let len: usize = shape.iter().product();

    // Empty array: no elements, but the shape is still meaningful.
    if len == 0 {
        // Pull the element type from the carrier when possible.
        let element_type = match mem {
            Value::Memory(mem_ref) => mem_ref.borrow().element_type().clone(),
            Value::MemoryRef(memref) => memref.element_type(),
            _ => ArrayElementType::Any,
        };
        return Some((shape, Vec::new(), element_type));
    }

    // Collect elements from the carrier. The carrier is usually a `Memory`
    // primitive, but some eltypes (e.g. `Bool`) keep a transitional native
    // `Array` carrier instead (Issue #3908). Handle both — mirroring
    // `array_wrapper_chars_to_string` in `vm/builtins_strings.rs` — so the
    // compact form is produced rather than dumping the internal struct fields
    // (Issue #5159: `print(Bool[true false; false true])` was leaking
    // `Array{Bool}(Bool[1, 0, 0, 1], (2, 2))`).
    if let Value::Memory(mem_ref) = mem {
        let borrow = mem_ref.borrow();
        let element_type = borrow.element_type().clone();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(borrow.get(offset + i).ok()?);
        }
        Some((shape, out, element_type))
    } else if let Value::MemoryRef(memref) = mem {
        // Faithful `Array{T,N}` struct (#6648) stores `ref::MemoryRef{T}`
        // rather than a bare `Memory`. Read elements from the parent
        // `Memory` starting at the ref's element offset so a struct-backed
        // array prints as `[...]` instead of dumping fields (Issue #6649).
        // `MemoryRef::memory_index` and `Memory::get` are both one-based.
        let parent = memref.parent();
        let borrow = parent.borrow();
        let base = memref.memory_index();
        let element_type = memref.element_type();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(borrow.get(base + i).ok()?);
        }
        Some((shape, out, element_type))
    } else if let Some(arr_ref) = native_array_value_ref(mem) {
        let borrow = arr_ref.borrow();
        let element_type = borrow.element_type();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(borrow.get_linear(offset - 1 + i).ok()?);
        }
        Some((shape, out, element_type))
    } else {
        None
    }
}

/// Column-major element values of an `Array{T,N}` wrapper struct, for the
/// VM-side per-element user-`show` pre-render (Issue #7893). Returns `None` for
/// non-wrapper structs or empty arrays (nothing to pre-render).
pub(crate) fn array_wrapper_elements(s: &StructInstance) -> Option<Vec<Value>> {
    let (_, elements, _) = array_wrapper_decode(s)?;
    if elements.is_empty() {
        return None;
    }
    Some(elements)
}

/// Render an `Array{T,N}` wrapper struct's compact form, overriding selected
/// elements with caller pre-rendered strings keyed by column-major linear index
/// (Issue #7893). Mirrors [`format_array_wrapper_compact`] but consults
/// `prerendered` per element so VM-rendered `Base.show(io, ::T)` output (e.g.
/// `Symbolics.Num`) replaces the struct-field dump. `None` slots fall back to
/// the default element rendering, so this is a no-op for elements without a
/// registered `show`.
pub(crate) fn format_array_wrapper_prerendered(
    s: &StructInstance,
    prerendered: &[Option<String>],
) -> Option<String> {
    let (shape, elements, element_type) = array_wrapper_decode(s)?;
    if elements.is_empty() {
        // Empty arrays have no per-element show to splice; defer to the
        // existing compact formatter for the `T[]` / `Matrix{T}(undef, ...)`
        // form.
        return format_array_wrapper_compact(s);
    }
    let eltype = apply_complex_float_aliases(array_wrapper_eltype_name(&s.struct_name));
    let bool_elt = element_type == ArrayElementType::Bool || eltype == "Bool";
    let (value_prefix, _) = array_show_prefix(&element_type, &elements);
    let prefix = apply_complex_float_aliases(&value_prefix);
    let render = |linear: usize, v: &Value| -> String {
        prerendered
            .get(linear)
            .and_then(|slot| slot.clone())
            .unwrap_or_else(|| format_array_wrapper_element(v, bool_elt))
    };

    match shape.len() {
        1 => {
            let parts: Vec<String> = elements
                .iter()
                .enumerate()
                .map(|(i, v)| render(i, v))
                .collect();
            Some(format!("{}[{}]", prefix, parts.join(", ")))
        }
        2 => {
            let rows = shape[0];
            let cols = shape[1];
            let mut row_strs = Vec::with_capacity(rows);
            for r in 0..rows {
                let mut col_strs = Vec::with_capacity(cols);
                for c in 0..cols {
                    let linear = c * rows + r;
                    col_strs.push(render(linear, &elements[linear]));
                }
                row_strs.push(col_strs.join(" "));
            }
            Some(format!("{}[{}]", prefix, row_strs.join("; ")))
        }
        _ => None,
    }
}

fn format_array_wrapper_compact(s: &StructInstance) -> Option<String> {
    let (shape, elements, element_type) = array_wrapper_decode(s)?;
    // Render a `Complex{FloatNN}` eltype via its `ComplexFNN` alias (Issue #5704).
    let eltype = apply_complex_float_aliases(array_wrapper_eltype_name(&s.struct_name));

    // Empty array: produce `Int64[]` / `Matrix{Int64}(undef, r, c)` form.
    if elements.is_empty() {
        if shape.len() <= 1 {
            return Some(format!("{}[]", eltype));
        }
        if shape.len() == 2 {
            return Some(format!(
                "Matrix{{{}}}(undef, {}, {})",
                eltype, shape[0], shape[1]
            ));
        }
        // Higher-rank empty: upstream prints the undef-constructor form,
        // e.g. `Array{Float64, 3}(undef, 2, 0, 2)` (Issue #10385).
        let dims: Vec<String> = shape.iter().map(|d| d.to_string()).collect();
        return Some(format!(
            "Array{{{}, {}}}(undef, {})",
            eltype,
            shape.len(),
            dims.join(", ")
        ));
    }

    // Issue #5159: `Bool` is a non-implicit eltype upstream, so the compact
    // form gains a `Bool[...]` prefix and elements render as `1`/`0`. Other
    // eltypes render via show-field rendering, but a NON-implicit eltype still
    // gains a `T[...]` typeinfo prefix to match upstream's `show` — e.g.
    // `Int8[0, 0, 0]`, `Float32[...]`, `ComplexF64[...]` (Issue #5774). Without
    // this only `Bool` was prefixed and `zeros(Int8, 3)` etc. dropped the prefix.
    let bool_elt = element_type == ArrayElementType::Bool || eltype == "Bool";
    let (value_prefix, _) = array_show_prefix(&element_type, &elements);
    let prefix = apply_complex_float_aliases(&value_prefix);

    match shape.len() {
        1 => {
            let parts: Vec<String> = elements
                .iter()
                .map(|v| format_array_wrapper_element(v, bool_elt))
                .collect();
            Some(format!("{}[{}]", prefix, parts.join(", ")))
        }
        2 => {
            let rows = shape[0];
            let cols = shape[1];
            // Memory is column-major: element at (r, c) lives at
            // linear index `(c-1)*rows + (r-1)`.
            let mut row_strs = Vec::with_capacity(rows);
            for r in 0..rows {
                let mut col_strs = Vec::with_capacity(cols);
                for c in 0..cols {
                    let linear = c * rows + r;
                    col_strs.push(format_array_wrapper_element(&elements[linear], bool_elt));
                }
                row_strs.push(col_strs.join(" "));
            }
            Some(format!("{}[{}]", prefix, row_strs.join("; ")))
        }
        // Rank >= 3: upstream's nested `;;`-literal compact form, e.g.
        // `[0.0 0.0; 0.0 0.0;;; 0.0 0.0; 0.0 0.0]` for `zeros(2,2,2)`
        // (Issue #10385) — previously fell through to the generic struct
        // dump, leaking the internal `MemoryRef` representation.
        _ => Some(format!(
            "{}[{}]",
            prefix,
            format_ndim_array_body(&elements, &shape, bool_elt)
        )),
    }
}

/// Recursive body of upstream's N-dimensional compact array literal
/// (Issue #10385): slices along dimension `k` are joined by `k` semicolons
/// (dim 1 rows by `"; "`, dims >= 3 by `";;;"`, `";;;;"`, ... with no
/// surrounding spaces), columns (dim 2) by a single space. `elements` is the
/// column-major storage of one sub-array of shape `shape`.
fn format_ndim_array_body(elements: &[Value], shape: &[usize], bool_elt: bool) -> String {
    match shape.len() {
        0 => elements
            .first()
            .map(|v| format_array_wrapper_element(v, bool_elt))
            .unwrap_or_default(),
        1 => {
            // A rank-1 slice inside a higher-rank literal joins its dim-1
            // entries with `"; "` (upstream: `reshape(1:4,2,1,2)` prints
            // `[1; 2;;; 3; 4]`).
            let parts: Vec<String> = elements
                .iter()
                .map(|v| format_array_wrapper_element(v, bool_elt))
                .collect();
            parts.join("; ")
        }
        2 => {
            let rows = shape[0];
            let cols = shape[1];
            let mut row_strs = Vec::with_capacity(rows);
            for r in 0..rows {
                let mut col_strs = Vec::with_capacity(cols);
                for c in 0..cols {
                    col_strs.push(format_array_wrapper_element(
                        &elements[c * rows + r],
                        bool_elt,
                    ));
                }
                row_strs.push(col_strs.join(" "));
            }
            row_strs.join("; ")
        }
        rank => {
            // Column-major: the last dimension's slices are contiguous blocks.
            let slice_count = shape[rank - 1];
            let block = elements.len().checked_div(slice_count).unwrap_or(0);
            // Upstream separates dim-`rank` slices with `rank` semicolons
            // followed by a single space: `1; 2;;; 3; 4`.
            let sep = format!("{} ", ";".repeat(rank));
            let mut parts = Vec::with_capacity(slice_count);
            for s in 0..slice_count {
                parts.push(format_ndim_array_body(
                    &elements[s * block..(s + 1) * block],
                    &shape[..rank - 1],
                    bool_elt,
                ));
            }
            parts.join(&sep)
        }
    }
}

/// Render one element of a Pure-Julia `Array{T}` wrapper struct for the
/// compact `print`/`string` form. When the array eltype is `Bool`
/// (`bool_elt`), elements render as the integers `1`/`0` to match upstream's
/// typeinfo-aware array show (Issue #5159); otherwise the usual show-field
/// rendering (with String/Char quoting) applies.
fn format_array_wrapper_element(v: &Value, bool_elt: bool) -> String {
    match (bool_elt, v) {
        (true, Value::Bool(true)) => "1".to_string(),
        (true, Value::Bool(false)) => "0".to_string(),
        _ => format_value_show_field(v),
    }
}

/// Format a struct instance for display.
/// Special cases for well-known types like Rational.
#[inline]
/// Render a reflection `Method` struct as `name(::T1, ::T2) @ Module file:line`
/// (Issue #5125), or `None` if `s` is not a `Method`. Mirrors the pure-Julia
/// `Base.show(io, ::Method)` in `base/reflection.jl` so the print path produces
/// the same listing form.
fn format_method_struct(s: &StructInstance) -> Option<String> {
    if &*s.struct_name != "Method" {
        return None;
    }
    // Field layout: 0=name, 1=sig, 2=nargs, 3=return_type, 4=module, 5=file,
    // 6=line (see base/reflection.jl). Be defensive about the count so an
    // unexpected layout falls through to the generic dump.
    let name = s.values.first()?;
    let sig = s.values.get(1)?;
    let nargs = s.values.get(2)?;
    let module = s.values.get(4)?;
    let file = s.values.get(5)?;
    let line = s.values.get(6)?;

    let name_str = match name {
        Value::Symbol(sym) => sym.as_str().to_string(),
        other => format_value_print_impl(other),
    };
    // sig is a Tuple of the explicit positional parameter types.
    let sig_types: Vec<Value> = match sig {
        Value::Tuple(t) => t.elements.clone(),
        _ => Vec::new(),
    };
    // nargs counts the function object, so explicit positional params = nargs-1.
    let nparams = match nargs {
        Value::I32(n) => (*n - 1).max(0) as usize,
        _ => sig_types.len(),
    };

    let mut out = String::new();
    out.push_str(&name_str);
    out.push('(');
    for i in 0..nparams {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str("::");
        match sig_types.get(i) {
            Some(ty) => out.push_str(&format_value_impl(ty)),
            None => out.push_str("Any"),
        }
    }
    out.push(')');
    out.push_str(" @ ");
    out.push_str(&format_value_print_impl(module));
    out.push(' ');
    out.push_str(&format_value_print_impl(file));
    out.push(':');
    out.push_str(&format_value_print_impl(line));
    Some(out)
}

/// Render a struct type name for display following upstream Julia's
/// visibility rule (Issues #7172/#11365): a type whose leaf name is reachable
/// unqualified from Main (declared at top level or `using`-imported) prints
/// bare (`"Geometry.Point{Int64}"` -> `"Point{Int64}"`); a module-owned type
/// that is NOT visible prints as the full path from the top
/// (`"M.B"` -> `"Main.M.B"`). Non-user owners (Base/Core spellings and roots
/// the current program did not declare) keep the historical bare form. Only
/// the type head (before the first `{`) is considered, so qualified type
/// parameters are left untouched.
fn display_type_name(struct_name: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let head_end = struct_name.find('{').unwrap_or(struct_name.len());
    let head = &struct_name[..head_end];
    let Some(dot) = head.rfind('.') else {
        return Cow::Borrowed(struct_name);
    };
    let leaf = &head[dot + 1..];
    if crate::vm::main_scope_visibility::main_visible_type_leaf(leaf, head) {
        return Cow::Borrowed(&struct_name[dot + 1..]);
    }
    // The user-root registry excludes builtin scopes by construction (the
    // seed filters them), so a hit always takes the Main. display prefix.
    let root = head.split('.').next().unwrap_or(head);
    if crate::vm::main_scope_visibility::is_user_module_root(root) {
        return Cow::Owned(format!("Main.{struct_name}"));
    }
    Cow::Borrowed(&struct_name[dot + 1..])
}

/// Apply the same visibility rule to a rendered TYPE OBJECT name
/// (`typeof(x)` display, Issue #11365): visible leaf -> strip the owner;
/// program-declared user root not visible from Main -> prefix `Main.`;
/// anything else unchanged.
fn qualify_datatype_display(name: String) -> String {
    let head_end = name.find('{').unwrap_or(name.len());
    let head = &name[..head_end];
    let Some(dot) = head.rfind('.') else {
        return name;
    };
    let leaf = &head[dot + 1..];
    if crate::vm::main_scope_visibility::main_visible_type_leaf(leaf, head) {
        return name[dot + 1..].to_string();
    }
    let root = head.split('.').next().unwrap_or(head);
    if crate::vm::main_scope_visibility::is_user_module_root(root) {
        return format!("Main.{name}");
    }
    name
}

pub(crate) fn is_version_number_struct_name(struct_name: &str) -> bool {
    struct_name == "VersionNumber" || struct_name.ends_with(".VersionNumber")
}

fn format_version_identifier(value: &Value) -> String {
    match value {
        Value::Str(s) => s.to_string(),
        Value::StrBytes(bytes) => String::from_utf8_lossy(bytes.as_ref()).into_owned(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        other => format_value_print_impl(other),
    }
}

fn format_version_ident_tuple(value: &Value, prefix: char) -> String {
    let Value::Tuple(tuple) = value else {
        return String::new();
    };
    if tuple.elements.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = tuple
        .elements
        .iter()
        .map(format_version_identifier)
        .collect();
    format!("{prefix}{}", parts.join("."))
}

fn format_version_number_print(s: &StructInstance) -> Option<String> {
    if !is_version_number_struct_name(&s.struct_name) || s.values.len() < 5 {
        return None;
    }
    let major = format_version_identifier(&s.values[0]);
    let minor = format_version_identifier(&s.values[1]);
    let patch = format_version_identifier(&s.values[2]);
    let prerelease = format_version_ident_tuple(&s.values[3], '-');
    let build = format_version_ident_tuple(&s.values[4], '+');
    Some(format!("{major}.{minor}.{patch}{prerelease}{build}"))
}

fn format_version_number_show(s: &StructInstance) -> Option<String> {
    format_version_number_print(s).map(|body| format!("v\"{body}\""))
}

/// Render a `Value::Rng`. The global handle (default_rng()/GLOBAL_RNG) prints as
/// `TaskLocalRNG()`, matching upstream `println(default_rng())` (Issue #7230).
/// Concrete instances keep the legacy compact "RNG" tag.
fn format_rng(rng: &crate::rng::RngInstance) -> String {
    match rng {
        crate::rng::RngInstance::Global => "TaskLocalRNG()".to_string(),
        _ => "RNG".to_string(),
    }
}

fn format_struct_instance(s: &StructInstance) -> String {
    if &*s.struct_name == "#__sjulia_circular_reference__" {
        let offset = match s.values.first() {
            Some(Value::I64(offset)) => *offset,
            _ => -1,
        };
        return format!("#= circular reference @{offset} =#");
    }
    if let Some(rendered) = format_version_number_show(s) {
        return rendered;
    }

    // Special case: Rational - display as num//den like Julia
    if s.is_rational() && s.values.len() == 2 {
        let num = format_value_show_field(&s.values[RATIONAL_NUMERATOR_FIELD_INDEX]);
        let den = format_value_show_field(&s.values[RATIONAL_DENOMINATOR_FIELD_INDEX]);
        return format!("{}//{}", num, den);
    }

    // Special case: Irrational{:sym} singleton (π, ℯ, ...) displays as the bare
    // symbol name like Julia's `show(io, ::Irrational{sym})` (Issue #5656),
    // rather than the constructor form `Irrational{:π}()`.
    if let Some(sym) = s
        .struct_name
        .strip_prefix("Irrational{:")
        .and_then(|rest| rest.strip_suffix('}'))
    {
        return sym.to_string();
    }

    // Special case: reflection `Method` — render the upstream listing form
    // `name(::T1, ::T2) @ Module file:line` (Issue #5125). The print path
    // (`print` / `println` / `string`) goes through `format_struct_instance`
    // rather than the user `Base.show(io, ::Method)` method, so without this it
    // would dump the raw struct (`Method(:foo, (Int64,), 2, ...)`). The field
    // layout mirrors the pure-Julia `struct Method`: name, sig, nargs,
    // return_type, module, file, line, ... (see base/reflection.jl).
    if let Some(rendered) = format_method_struct(s) {
        return rendered;
    }

    // Special case: Pair - display as "left => right" (Issue #4725).
    // Upstream Julia's `string(Pair(1, 2))` / `print(io, ::Pair)` writes
    // the infix `=>` form rather than the constructor-call form, and we
    // were leaking `StructRef(heap_idx=N)` for heap-allocated Pairs.
    // Issue #4764: Pair fields render in show form (`"a" => "b"`) to
    // match upstream — String/Char field values are quoted.
    if (&*s.struct_name == "Pair" || s.struct_name.starts_with("Pair{")) && s.values.len() == 2 {
        let left = format_value_show_field(&s.values[0]);
        let right = format_value_show_field(&s.values[1]);
        return format!("{} => {}", left, right);
    }

    // Special case: Pure-Julia `Array{T}` wrapper struct (Issue #4770).
    // Without this arm, `print(v)` / `string(v)` / `println(v)` would
    // dump the internal `Memory{T}` carrier and shape tuple, leaking
    // implementation details into user output.
    if let Some(compact) = format_array_wrapper_compact(s) {
        return compact;
    }

    // Special case: LinRange{T} — display as `LinRange{T}(start, stop, len)`
    // (Issue #4761, family of #4759). The Pure-Julia struct has 4 fields
    // (start, stop, len, lendiv) so the generic fallback below would dump
    // `LinRange{Float64}(0.0, 1.0, 5, 4)`, leaking the internal `lendiv`
    // helper field. Match the user-facing `show(io, ::LinRange)` shape
    // defined in base/io.jl. The print(io, ...) / string(...) paths do
    // not currently dispatch to user `show` methods on structs — until
    // they do, the canonical fields-to-display projection lives here.
    if (&*s.struct_name == "LinRange" || s.struct_name.starts_with("LinRange{"))
        && s.values.len() >= 3
    {
        // Extract the parametric tail from the struct name (e.g.,
        // "LinRange{Float64}") so the show form matches upstream.
        let header = s.struct_name.clone();
        let start = format_value_show_field(&s.values[0]);
        let stop = format_value_show_field(&s.values[1]);
        let len = format_value_show_field(&s.values[2]);
        return format!("{}({}, {}, {})", header, start, stop, len);
    }

    // General case: StructName(field1, field2, ...)
    // Issue #4764: fields render in show form so String/Char fields are
    // quoted (`Foo(42, "hi")`) rather than bare (`Foo(42, hi)`), matching
    // upstream Julia's default struct show fallback.
    let fields: Vec<String> = s.values.iter().map(format_value_show_field).collect();
    format!(
        "{}({})",
        display_type_name(&s.struct_name),
        fields.join(", ")
    )
}

/// Resolve any `Value::StructRef` nodes recursively against `struct_heap`,
/// returning a value tree with no `StructRef`s left at the top level or
/// inside composite containers. `format_value` is heap-less, so callers
/// that want a Julia-style string for a heap-allocated struct must pass
/// the resolved value (Issue #4725).
///
/// Composite containers handled: `Tuple`, `NamedTuple`, `Ref`, `QuoteNode`.
/// Other composite types (`Dict`, `Set`, `Array`, ...) are returned
/// unchanged for now — their format paths already have heap-aware
/// codepaths or do not nest user `StructRef`s in practice.
pub(crate) fn resolve_struct_refs_for_format(v: &Value, struct_heap: &[StructInstance]) -> Value {
    resolve_struct_refs_for_format_inner(v, struct_heap, &mut Vec::new())
}

fn resolve_struct_refs_for_format_inner(
    v: &Value,
    struct_heap: &[StructInstance],
    path: &mut Vec<usize>,
) -> Value {
    match v {
        Value::StructRef(idx) => match struct_heap.get(*idx) {
            Some(instance) => {
                if let Some(position) = path.iter().position(|ancestor| ancestor == idx) {
                    let offset = position as i64 - path.len() as i64;
                    return Value::Struct(StructInstance {
                        type_id: usize::MAX,
                        struct_name: std::rc::Rc::from("#__sjulia_circular_reference__"),
                        values: vec![Value::I64(offset)],
                    });
                }
                path.push(*idx);
                let mut resolved = instance.clone();
                resolved.values = resolved
                    .values
                    .iter()
                    .map(|inner| resolve_struct_refs_for_format_inner(inner, struct_heap, path))
                    .collect();
                path.pop();
                Value::Struct(resolved)
            }
            None => v.clone(),
        },
        Value::Tuple(t) => {
            let elements = t
                .elements
                .iter()
                .map(|e| resolve_struct_refs_for_format_inner(e, struct_heap, path))
                .collect();
            Value::Tuple(crate::vm::value::TupleValue { elements })
        }
        // Issue #4722: resolve struct refs inside Core.SimpleVector elements too.
        Value::SimpleVector(sv) => {
            let elements = sv
                .elements
                .iter()
                .map(|e| resolve_struct_refs_for_format_inner(e, struct_heap, path))
                .collect();
            Value::SimpleVector(crate::vm::value::TupleValue { elements })
        }
        Value::NamedTuple(nt) => {
            let values = nt
                .values
                .iter()
                .map(|e| resolve_struct_refs_for_format_inner(e, struct_heap, path))
                .collect();
            let mut copy = nt.clone();
            copy.values = values;
            Value::NamedTuple(copy)
        }
        Value::Ref(inner) => crate::vm::value::new_ref(resolve_struct_refs_for_format_inner(
            &inner.borrow(),
            struct_heap,
            path,
        )),
        Value::QuoteNode(inner) => Value::QuoteNode(Box::new(
            resolve_struct_refs_for_format_inner(inner.as_ref(), struct_heap, path),
        )),
        Value::Struct(instance) => {
            let mut resolved = instance.clone();
            resolved.values = resolved
                .values
                .iter()
                .map(|inner| resolve_struct_refs_for_format_inner(inner, struct_heap, path))
                .collect();
            Value::Struct(resolved)
        }
        // Issue #4774: `Dict{K, T}` and `Set{T}` where T is a heap-
        // allocated struct stores `Value::StructRef(idx)` in vals,
        // which leaks `StructRef(heap_idx=N)` inside `string(Dict(...))`
        // / `string(Set(...))` if not resolved upstream.
        // Issue #4772: Vector{T} where T is a heap-allocated struct
        // (any user struct, Pair, etc.) stores `Value::StructRef(idx)`
        // inside the backing Memory. Without resolving those, the
        // print path leaks `StructRef(heap_idx=N)` inside the new
        // `[a, b, c]` compact form (regression from PR #4771).
        //
        // Build a fresh `Value::Memory` whose element data is widened
        // to `ArrayData::Any` and holds resolved values. The widening
        // is throw-away (only used by the formatting copy of the
        // value tree); the original Memory is not mutated.
        Value::Memory(mem_ref) => {
            let borrow = mem_ref.borrow();
            let len = borrow.len();
            let mut resolved_elements: Vec<Value> = Vec::with_capacity(len);
            let mut any_resolution_needed = false;
            for i in 1..=len {
                let elem = match borrow.get(i) {
                    Ok(v) => v,
                    Err(_) => return v.clone(),
                };
                if matches!(&elem, Value::StructRef(_)) {
                    any_resolution_needed = true;
                }
                resolved_elements.push(resolve_struct_refs_for_format_inner(
                    &elem,
                    struct_heap,
                    path,
                ));
            }
            if !any_resolution_needed {
                return v.clone();
            }
            let element_type = borrow.element_type.clone();
            drop(borrow);
            use super::value::{new_memory_ref, MemoryValue};
            let new_mem = MemoryValue::new(
                super::value::ArrayData::Any(resolved_elements),
                element_type,
                len,
            );
            Value::Memory(new_memory_ref(new_mem))
        }
        Value::MemoryRef(memref) => {
            let len = memref.len();
            let mut resolved_elements: Vec<Value> = Vec::with_capacity(len);
            let mut any_resolution_needed = false;
            for i in 1..=len {
                let elem = match memref.get(i) {
                    Ok(v) => v,
                    Err(_) => return v.clone(),
                };
                if matches!(&elem, Value::StructRef(_)) {
                    any_resolution_needed = true;
                }
                resolved_elements.push(resolve_struct_refs_for_format_inner(
                    &elem,
                    struct_heap,
                    path,
                ));
            }
            if !any_resolution_needed {
                return v.clone();
            }
            use super::value::{new_memory_ref, ArrayData, MemoryValue};
            let new_mem = MemoryValue::new(
                ArrayData::Any(resolved_elements),
                memref.element_type(),
                len,
            );
            Value::MemoryRef(Box::new(MemoryRefValue::first(new_memory_ref(new_mem))))
        }
        // Issue #4772: the transitional native-array carrier used by
        // Vector/Matrix VM values
        // can hold `Value::StructRef(idx)` elements when the element
        // type is `Any` (or a struct type). Without resolving them,
        // `format_array_value` formats each element via
        // `format_value_impl(StructRef(N))` which leaks
        // `StructRef(heap_idx=N)` into the `[a, b, c]` form.
        _ => {
            if let Some(arr_ref) = native_array_value_ref(v) {
                let borrow = arr_ref.borrow();
                let n = borrow.element_count();
                let mut resolved_elements: Vec<Value> = Vec::with_capacity(n);
                let mut any_resolution_needed = false;
                for i in 0..n {
                    let elem = match borrow.get_linear(i) {
                        Ok(v) => v,
                        Err(_) => return v.clone(),
                    };
                    if matches!(&elem, Value::StructRef(_)) {
                        any_resolution_needed = true;
                    }
                    resolved_elements.push(resolve_struct_refs_for_format_inner(
                        &elem,
                        struct_heap,
                        path,
                    ));
                }
                if !any_resolution_needed {
                    return v.clone();
                }
                let shape = borrow.shape.clone();
                drop(borrow);
                use super::value::{
                    native_array_value_from_array, ArrayData, ArrayElementType, ArrayValue,
                    MemoryValue,
                };
                let len = resolved_elements.len();
                let mem = MemoryValue::new(
                    ArrayData::Any(resolved_elements),
                    ArrayElementType::Any,
                    len,
                );
                return native_array_value_from_array(ArrayValue::from_memory(mem, shape));
            }
            v.clone()
        }
    }
}

// ============================================================================
// Resolved — type-level "no unresolved StructRef" witness (Issue #8642)
// ============================================================================

/// Recursively check whether a value tree contains any [`Value::StructRef`]
/// heap handle. This is the *predicate twin* of
/// [`resolve_struct_refs_for_format`]: it must walk exactly the same set of
/// composite carriers (Tuple / SimpleVector / NamedTuple / Ref / QuoteNode /
/// Struct / Memory / MemoryRef / native-array `ExprArgs`), so that
/// `!value_contains_struct_ref(&resolve_struct_refs_for_format(v, heap))`
/// holds for every `v` whose refs are live in `heap`. When adding a new
/// recursion arm to the resolver, add the matching arm here (the
/// `resolved_deeply_*` unit tests below pin this pairing).
///
/// Pure-Julia `Dict` / `Set` are `Struct` instances whose backing store is a
/// `Memory`, so they are covered by the `Struct` + `Memory` arms.
pub(crate) fn value_contains_struct_ref(v: &Value) -> bool {
    match v {
        Value::StructRef(_) => true,
        Value::Tuple(t) | Value::SimpleVector(t) => {
            t.elements.iter().any(value_contains_struct_ref)
        }
        Value::NamedTuple(nt) => nt.values.iter().any(value_contains_struct_ref),
        Value::Ref(inner) => value_contains_struct_ref(&inner.borrow()),
        Value::QuoteNode(inner) => value_contains_struct_ref(inner.as_ref()),
        Value::Struct(s) => s.values.iter().any(value_contains_struct_ref),
        Value::Memory(mem_ref) => {
            let borrow = mem_ref.borrow();
            let len = borrow.len();
            (1..=len).any(|i| match borrow.get(i) {
                Ok(elem) => value_contains_struct_ref(&elem),
                // Unreadable element: conservatively report a possible
                // StructRef (mirrors the resolver, which bails out with the
                // original value on read errors).
                Err(_) => true,
            })
        }
        Value::MemoryRef(memref) => {
            let len = memref.len();
            (1..=len).any(|i| match memref.get(i) {
                Ok(elem) => value_contains_struct_ref(&elem),
                Err(_) => true,
            })
        }
        _ => {
            if let Some(arr_ref) = native_array_value_ref(v) {
                let borrow = arr_ref.borrow();
                let n = borrow.element_count();
                (0..n).any(|i| match borrow.get_linear(i) {
                    Ok(elem) => value_contains_struct_ref(&elem),
                    Err(_) => true,
                })
            } else {
                false
            }
        }
    }
}

/// A [`Value`] witness that no unresolved [`Value::StructRef`] heap handle
/// remains anywhere in the value tree — the type-level replacement for the
/// former grep-based audit `check_format_value_resolves_structref.sh`, which
/// this newtype retired (Issue #8642, generalizing fix #5234).
///
/// The heap-less display sinks ([`format_value`] / [`format_value_print`] /
/// [`value_to_string`]) render a bare `StructRef` as the Rust-debug-ish
/// `StructRef(heap_idx=N)`, which must never reach user-visible output.
/// Requiring `&Resolved` at those sinks turns a forgotten heap resolution
/// from a runtime display leak into a compile error, because a `Resolved`
/// can only be obtained through one of the explicit constructors:
///
/// - [`Resolved::new`] — the canonical constructor: deep-resolves every
///   `StructRef` (top-level *and* nested inside composite carriers) against
///   the VM `struct_heap` via [`resolve_struct_refs_for_format`].
/// - [`Resolved::trivial`] — for values that structurally cannot contain a
///   `StructRef` (freshly built scalars/strings, error-message literals).
///   Debug builds verify the claim with a recursive assert.
/// - [`Resolved::assume_ffi_placeholder`] — for the FFI display layer only,
///   which by design has no `struct_heap` access and deliberately keeps its
///   placeholder rendering for a bare `StructRef` instead of resolving it.
pub struct Resolved<'a>(std::borrow::Cow<'a, Value>);

impl<'a> Resolved<'a> {
    /// Canonical constructor: deep-resolve every `Value::StructRef` in `v`
    /// (including refs nested inside Tuple / NamedTuple / Ref / QuoteNode /
    /// Struct / Memory / MemoryRef / native-array carriers) against
    /// `struct_heap`. Values that contain no `StructRef` are borrowed
    /// unchanged (no clone).
    pub(crate) fn new(v: &'a Value, struct_heap: &[StructInstance]) -> Self {
        if value_contains_struct_ref(v) {
            Resolved(std::borrow::Cow::Owned(resolve_struct_refs_for_format(
                v,
                struct_heap,
            )))
        } else {
            Resolved(std::borrow::Cow::Borrowed(v))
        }
    }

    /// Constructor for values that structurally cannot contain a
    /// `Value::StructRef` — e.g. a `Value::Str` just built from a Rust
    /// `String`, or scalar literals. Zero-cost (borrows; no heap needed).
    ///
    /// Debug builds recursively assert the claim; if the assert fires, the
    /// call site must switch to [`Resolved::new`] with the VM `struct_heap`.
    pub(crate) fn trivial(v: &'a Value) -> Self {
        debug_assert!(
            !value_contains_struct_ref(v),
            "Resolved::trivial() called on a value containing Value::StructRef; \
             use Resolved::new(v, &self.struct_heap) instead (Issue #8642)"
        );
        Resolved(std::borrow::Cow::Borrowed(v))
    }

    /// FFI-layer constructor (Issue #8660): the FFI display path
    /// (`subset_julia_vm_ffi`, via `ffi_support`) has **no `struct_heap`
    /// access by design**, and a bare `StructRef` reaching it renders as a
    /// benign placeholder (e.g. `<struct ref>` in the iOS REPL) rather than
    /// a resolved struct. This constructor keeps that behavior explicit:
    /// it performs no resolution and no assertion. Do NOT use it inside the
    /// VM crate's display entry points — those must use [`Resolved::new`].
    pub fn assume_ffi_placeholder(v: &'a Value) -> Self {
        Resolved(std::borrow::Cow::Borrowed(v))
    }

    /// Access the underlying resolved value.
    pub(crate) fn value(&self) -> &Value {
        &self.0
    }
}

// ============================================================================
// format_value - Julia-style display format
// ============================================================================

/// `format_value` flavored for the `print` codepath (Issue #4741).
/// `format_value` itself is shared between `print` and `show`
/// codepaths and unconditionally produces the show-form `:foo` for
/// `Value::Symbol(foo)` — that's correct for `show(io, :foo)` but
/// wrong for `print(io, :foo)` / interpolation, which upstream Julia
/// writes as the bare name `foo`.
///
/// This wrapper strips the `:` for Symbol at the top level; everything
/// else delegates to `format_value`. Containers are intentionally not
/// rewalked: upstream Julia keeps the `:foo` form for Symbols *inside*
/// containers (e.g. `Dict(:k => 1)` shows as `Dict(:k => 1)`), so the
/// recursion should stop at the top level.
#[inline]
pub(crate) fn format_value_print(r: &Resolved) -> String {
    format_value_print_impl(r.value())
}

/// Heap-less print-form implementation (Issue #8661). Operates on a raw
/// `&Value`; only the `Resolved`-typed [`format_value_print`] wrapper is
/// reachable from outside the formatting module, so every VM display sink
/// must first construct a [`Resolved`]. Internal recursion within the
/// formatting module (which already walks a resolved value tree) calls this
/// directly.
#[inline]
fn format_value_print_impl(v: &Value) -> String {
    match v {
        Value::Symbol(s) => s.as_str().to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::Struct(s) if is_version_number_struct_name(&s.struct_name) => {
            format_version_number_print(s).unwrap_or_else(|| format_value_impl(v))
        }
        _ => format_value_impl(v),
    }
}

/// Format any Value as a string (for PrintAny instruction).
///
/// Fast path: the most common types (I64, F64, Bool, Str, Nothing) are handled
/// inline so the compiler can keep them on the hot path. All other variants
/// are dispatched to `format_value_slow`, which is marked `#[cold]`.
#[inline]
pub fn format_value(r: &Resolved) -> String {
    format_value_impl(r.value())
}

/// Heap-less Julia-display implementation (Issue #8661). See
/// [`format_value_print_impl`] for the wrapper/impl split rationale.
#[inline]
fn format_value_impl(v: &Value) -> String {
    match v {
        Value::I64(x) => x.to_string(),
        Value::F64(x) => format_float_julia(*x),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Str(s) => s.to_string(),
        Value::StrBytes(bytes) => String::from_utf8_lossy(bytes.as_ref()).into_owned(),
        Value::Nothing => "nothing".to_string(),
        _ => format_value_slow(v),
    }
}

/// Format a float value for range display.
fn format_range_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

fn range_struct_base_name(name: &str) -> &str {
    let unqualified = name.rsplit('.').next().unwrap_or(name);
    unqualified.split('{').next().unwrap_or(unqualified)
}

fn range_struct_element_type(name: &str) -> RangeElementType {
    if name.contains("BigInt") {
        RangeElementType::BigInt
    } else if name.contains("UInt8") {
        RangeElementType::UInt8
    } else if name.contains("UInt16") {
        RangeElementType::UInt16
    } else if name.contains("UInt32") {
        RangeElementType::UInt32
    } else if name.contains("UInt64") {
        RangeElementType::UInt64
    } else if name.contains("Int8") {
        RangeElementType::Int8
    } else if name.contains("Int16") {
        RangeElementType::Int16
    } else if name.contains("Int32") {
        RangeElementType::Int32
    } else if name.contains("Char") {
        RangeElementType::Char
    } else {
        RangeElementType::Default
    }
}

fn range_struct_step_type(name: &str, step: Option<&Value>) -> RangeElementType {
    match step {
        Some(Value::BigInt(_)) => RangeElementType::BigInt,
        Some(Value::U8(_)) => RangeElementType::UInt8,
        Some(Value::U16(_)) => RangeElementType::UInt16,
        Some(Value::U32(_)) => RangeElementType::UInt32,
        Some(Value::U64(_)) => RangeElementType::UInt64,
        Some(Value::I8(_)) => RangeElementType::Int8,
        Some(Value::I16(_)) => RangeElementType::Int16,
        Some(Value::I32(_)) => RangeElementType::Int32,
        Some(Value::Char(_)) => RangeElementType::Char,
        Some(_) => RangeElementType::Default,
        None => range_struct_element_type(name),
    }
}

fn range_struct_value_to_bigint(value: &Value) -> Option<RustBigInt> {
    match value {
        Value::BigInt(v) => Some(v.clone()),
        Value::I64(v) => Some(RustBigInt::from(*v)),
        Value::I32(v) => Some(RustBigInt::from(*v)),
        Value::I16(v) => Some(RustBigInt::from(*v)),
        Value::I8(v) => Some(RustBigInt::from(*v)),
        Value::I128(v) => Some(RustBigInt::from(*v)),
        Value::U64(v) => Some(RustBigInt::from(*v)),
        Value::U32(v) => Some(RustBigInt::from(*v)),
        Value::U16(v) => Some(RustBigInt::from(*v)),
        Value::U8(v) => Some(RustBigInt::from(*v)),
        Value::U128(v) => Some(RustBigInt::from(*v)),
        _ => None,
    }
}

fn range_struct_value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::I64(v) => Some(*v as f64),
        Value::I32(v) => Some(*v as f64),
        Value::I16(v) => Some(*v as f64),
        Value::I8(v) => Some(*v as f64),
        Value::I128(v) => Some(*v as f64),
        Value::U64(v) => Some(*v as f64),
        Value::U32(v) => Some(*v as f64),
        Value::U16(v) => Some(*v as f64),
        Value::U8(v) => Some(*v as f64),
        Value::U128(v) => Some(*v as f64),
        Value::Char(c) => Some(u32::from(*c) as f64),
        _ => None,
    }
}

fn struct_instance_as_range_value(instance: &StructInstance) -> Option<RangeValue> {
    let base = range_struct_base_name(&instance.struct_name);
    let unit_step = Value::I64(1);
    let (start, step, stop, is_step_range) = match base {
        "UnitRange" => (
            instance.values.first()?,
            &unit_step,
            instance.values.get(1)?,
            false,
        ),
        "StepRange" => (
            instance.values.first()?,
            instance.values.get(1)?,
            instance.values.get(2)?,
            true,
        ),
        _ => return None,
    };
    let element_type = range_struct_element_type(&instance.struct_name);
    let step_type = range_struct_step_type(&instance.struct_name, is_step_range.then_some(step));
    if matches!(element_type, RangeElementType::BigInt) {
        return Some(RangeValue::bigint_range(
            range_struct_value_to_bigint(start)?,
            range_struct_value_to_bigint(step)?,
            range_struct_value_to_bigint(stop)?,
            is_step_range,
            element_type,
            step_type,
        ));
    }
    Some(RangeValue {
        start: range_struct_value_to_f64(start)?,
        step: range_struct_value_to_f64(step)?,
        stop: range_struct_value_to_f64(stop)?,
        is_float: false,
        element_type,
        step_type,
        is_step_range,
        linspace_len: None,
        step_defined: false,
        bigint: None,
    })
}

fn format_range_value(r: &RangeValue) -> String {
    // Char ranges render as `'a':1:'e'` (matching upstream
    // `StepRange{Char, Int}` show form) instead of leaking raw codepoints.
    // Issue #4795.
    if matches!(r.element_type, RangeElementType::Char) {
        let start_ch = char::from_u32(r.start as u32).unwrap_or('\u{FFFD}');
        let stop_ch = char::from_u32(r.stop as u32).unwrap_or('\u{FFFD}');
        let step = r.step as i64;
        format!("'{}':{}:'{}'", start_ch, step, stop_ch)
    } else if let Some(parts) = &r.bigint {
        if r.is_unit_range() {
            format!("{}:{}", parts.start, parts.stop)
        } else {
            format!("{}:{}:{}", parts.start, parts.step, parts.stop)
        }
    } else if r.is_float {
        // A step-0 length-defined range (`range(1, 1, length=5)`) has
        // no valid colon form; upstream shows the constructor form
        // `StepRangeLen(1.0, 0.0, 5)` (Issue #9419).
        if r.step == 0.0 && r.linspace_len.is_some() {
            format!(
                "StepRangeLen({}, {}, {})",
                format_range_float(r.start),
                format_range_float(r.step),
                r.length()
            )
        } else if r.is_unit_range() {
            // `is_unit_range()` (not `step == 1`) so an explicit-step `1.0:1.0:3.0`
            // renders with its step, matching upstream `StepRange` (Issue #5667).
            format!(
                "{}:{}",
                format_range_float(r.start),
                format_range_float(r.stop)
            )
        } else {
            format!(
                "{}:{}:{}",
                format_range_float(r.start),
                format_range_float(r.step),
                format_range_float(r.stop)
            )
        }
    } else if r.is_unit_range() {
        format!("{}:{}", r.start as i64, r.stop as i64)
    } else {
        format!("{}:{}:{}", r.start as i64, r.step as i64, r.stop as i64)
    }
}

/// Slow path for less common Value variants.
#[cold]
fn format_value_slow(v: &Value) -> String {
    // Route the legacy native-array carrier through the shared
    // `native_array_value_ref` helper so the match below no longer holds a
    // native-array arm (Issue #3908). `format_array_value` handles
    // shape-aware Julia-style display.
    if let Some(arr) = native_array_value_ref(v) {
        return format_array_value(arr);
    }
    match v {
        // Signed integers (non-I64)
        Value::I8(x) => x.to_string(),
        Value::I16(x) => x.to_string(),
        Value::I32(x) => x.to_string(),
        Value::I64(x) => x.to_string(),
        Value::I128(x) => x.to_string(),
        // Unsigned integers
        Value::U8(x) => format!("0x{x:02x}"),
        Value::U16(x) => format!("0x{x:04x}"),
        Value::U32(x) => format!("0x{x:08x}"),
        Value::U64(x) => format!("0x{x:016x}"),
        Value::U128(x) => format!("0x{x:032x}"),
        // Boolean
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        // Floating point
        Value::F16(x) => format_float16_julia(*x),
        Value::F32(x) => format_float32_julia(*x),
        Value::F64(x) => format_float_julia(*x),
        Value::BigInt(x) => x.to_string(),
        Value::BigFloat(x) => format_bigfloat_julia(x),
        Value::Str(s) => s.to_string(),
        Value::StrBytes(bytes) => String::from_utf8_lossy(bytes.as_ref()).into_owned(),
        Value::Char(c) => c.to_string(),
        // Print-form of a malformed Char: the VM's output pipeline is
        // String-typed, so the raw invalid bytes cannot ride through it —
        // display the Unicode replacement character (Issue #8995).
        Value::CharMalformed(_) => '\u{FFFD}'.to_string(),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::Struct(s) if s.is_complex() => format_complex_struct(s),
        Value::Range(r) => format_range_value(r),
        Value::SliceAll => ":".to_string(),
        Value::Struct(s) => struct_instance_as_range_value(s)
            .map(|range| format_range_value(&range))
            .unwrap_or_else(|| format_struct_instance(s)),
        Value::StructRef(idx) => format!("StructRef(heap_idx={})", idx),
        Value::Rng(rng) => format_rng(rng),
        Value::Tuple(t) => {
            // Issue #4777: elements render in show form so String/Char
            // values are quoted (`(1, "hi", 'c')`) rather than bare
            // (`(1, hi, c)`), matching upstream Julia's default tuple
            // show fallback. Sibling of the struct-field fix #4764.
            let parts: Vec<String> = t.elements.iter().map(format_value_show_field).collect();
            // Single-element tuple needs trailing comma to disambiguate
            // from a parenthesized expression.
            if t.elements.len() == 1 {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        // Issue #4722: Core.SimpleVector (svec) prints as `svec(elem, ...)`,
        // matching upstream `<DataType>.parameters` display.
        Value::SimpleVector(sv) => {
            let parts: Vec<String> = sv.elements.iter().map(format_value_show_field).collect();
            format!("svec({})", parts.join(", "))
        }
        Value::NamedTuple(nt) => format_named_tuple_value(&nt.names, &nt.values),
        Value::Ref(inner) => {
            // Base.RefValue{T}(value) (Issue #5130) - matches upstream display.
            let v = inner.borrow();
            format!(
                "Base.RefValue{{{}}}({})",
                v.runtime_type(),
                format_value_impl(&v)
            )
        }
        Value::Generator(_) => "Generator(...)".to_string(),
        Value::DataType(jt) => {
            // display_name (not name/to_string): trailing unbounded where
            // binders elide only at display boundaries (Issue #10505).
            qualify_datatype_display(apply_complex_float_aliases(&jt.display_name()))
        }
        Value::RuntimeTypeVar(tv) => format_runtime_typevar(tv),
        Value::RuntimeTypeName(type_name) => format!("typename({})", type_name.name),
        // Upstream `string(Main)` / `print(Main)` / `repr(Main)` all render the
        // bare module name (`Main`), not a `Module(...)` wrapper. Matching this
        // is required for `.module` field display and the ` @ Main file:line`
        // suffix of `show(::Method)` (Issue #5125).
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
            let outer_str = format_value_impl(&cf.outer);
            let inner_str = format_value_impl(&cf.inner);
            format!("{} ∘ {}", outer_str, inner_str)
        }
        Value::Undef => "#undef".to_string(),
        Value::IO(io_ref) => {
            if io_ref.borrow().is_stdout() {
                "stdout".to_string()
            } else if io_ref.borrow().is_stderr() {
                "stderr".to_string()
            } else if io_ref.borrow().is_devnull() {
                "devnull".to_string()
            } else if io_ref.borrow().is_pipe() {
                "Pipe()".to_string()
            } else {
                "IOBuffer(...)".to_string()
            }
        }
        // Macro system types
        Value::Symbol(s) => format!(":{}", s.as_str()),
        // `print`/`println` of an `Expr` renders the Julia-code form
        // (e.g. `[a, b]`, `1 + 2`), matching upstream `print(::Expr)` and the
        // `string(::Expr)` builtin path. The `Expr(:head, ...)` s-expr form is
        // only produced by `dump`/`Meta.show_sexpr`, never by `print` (Issue
        // #7641: quoted vector LHS printed as `Expr(:vect, ...)`).
        Value::Expr(e) => expr_to_julia_string(e),
        Value::QuoteNode(v) => format!("QuoteNode({})", format_value_impl(v)),
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        Value::Binding(binding) => binding.to_string(),
        // Base.Pairs type (for kwargs...)
        Value::Pairs(p) => {
            let parts: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(n, v)| format!(":{} => {}", n, format_value_impl(v)))
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
        Value::Enum { type_name, value } => format_enum_value(type_name, *value),
        // Memory{T} flat typed buffer renders compactly (`[1, 2, 3]` / `Int64[]`)
        // for `print` / `string` / `repr`, matching upstream `show(io, ::Memory)`
        // (Issue #6697). The verbose "N-element Memory{T}:" form is `display`
        // semantics and lives on the REPL / 3-arg show path.
        Value::Memory(mem) => format_memory_compact(mem),
        Value::MemoryRef(memref) => format_memory_ref_value(memref),
        Value::WeakRef(cell) => {
            let referent = cell.borrow();
            format!("WeakRef({})", format_value_show_field(&referent))
        }
        // Issue #7964: flat static array — display as the backing tuple of elements.
        Value::StaticArray(sv) => {
            let elems: Vec<String> = (0..sv.len())
                .filter_map(|i| sv.elems.get_value(i))
                .map(|v| format_value_impl(&v))
                .collect();
            format!("{}(({}))", sv.julia_type_name(), elems.join(", "))
        }
        // Issue #7964 Phase 3: inline variant — same display, no heap Vec for elements.
        Value::StaticArrayInline(sv) => {
            let type_name = sv.julia_type_name_owned();
            let elems: Vec<String> = (0..sv.len())
                .map(|i| format_value_impl(&sv.get_0indexed(i)))
                .collect();
            format!("{}(({}))", type_name, elems.join(", "))
        }
        // The legacy native-array carrier is filtered out by the early-return
        // above (Issue #3908). This wildcard satisfies Rust's exhaustiveness
        // checking and provides a safe default for any future `Value` variant:
        // fall back to the value's Debug representation.
        _ => format!("{:?}", v),
    }
}

/// Format an Array value for display. Uses index-based access to avoid
/// allocating the full element vector when only the first 100 are shown.
/// Format a single element inside an array/matrix display.
///
/// Julia's `print(io, ::AbstractVector)` uses `show(io, x)` for each element
/// (rather than `print(io, x)`), which adds quotes around strings and chars.
/// For numbers, `show` and `print` produce identical output. This mirrors
/// `Base.show_vector` from `julia/base/arrayshow.jl`. (Issue #3574)
#[cold]
fn format_array_element(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("\"{}\"", s),
        Value::StrBytes(bytes) => format!("\"{}\"", String::from_utf8_lossy(bytes.as_ref())),
        Value::Char(c) => format!("'{}'", c),
        Value::CharMalformed(bits) => format!("'{}'", escape_char_malformed_for_show(*bits)),
        _ => format_value_impl(v),
    }
}

/// Determine, for a single element value, its concrete type *name* (as it would
/// appear in a `T[...]` array-show prefix) and whether that type is "implicit"
/// per upstream Julia's `typeinfo_implicit`. Returns `None` when the concrete
/// type cannot be cheaply derived, in which case the caller treats the array as
/// heterogeneous (`Any`). Mirrors the value-level reasoning upstream performs on
/// `eltype(X)` in `typeinfo_prefix` (`julia/base/arrayshow.jl`), used here to
/// recover the prefix for arrays whose element-type tag is the opaque
/// `Any`/`Struct` family (sjulia stores `Pair`/`Tuple`/struct/`Complex`
/// element arrays under those tags).
fn value_show_type(v: &Value) -> Option<(String, bool)> {
    match v {
        // Implicit scalar types (parseable back from their bare form).
        Value::I64(_) => Some(("Int64".to_string(), true)),
        Value::F64(_) => Some(("Float64".to_string(), true)),
        Value::Char(_) => Some(("Char".to_string(), true)),
        Value::Str(_) | Value::StrBytes(_) => Some(("String".to_string(), true)),
        Value::Symbol(_) => Some(("Symbol".to_string(), true)),
        // Non-implicit scalar widths (carry a type prefix).
        Value::I8(_) => Some(("Int8".to_string(), false)),
        Value::I16(_) => Some(("Int16".to_string(), false)),
        Value::I32(_) => Some(("Int32".to_string(), false)),
        Value::I128(_) => Some(("Int128".to_string(), false)),
        Value::U8(_) => Some(("UInt8".to_string(), false)),
        Value::U16(_) => Some(("UInt16".to_string(), false)),
        Value::U32(_) => Some(("UInt32".to_string(), false)),
        Value::U64(_) => Some(("UInt64".to_string(), false)),
        Value::U128(_) => Some(("UInt128".to_string(), false)),
        Value::F16(_) => Some(("Float16".to_string(), false)),
        Value::F32(_) => Some(("Float32".to_string(), false)),
        Value::Bool(_) => Some(("Bool".to_string(), false)),
        // Tuples: implicit iff every component is implicit (matches upstream
        // `typeinfo_implicit` over `Tuple`).
        Value::Tuple(t) => {
            let mut all_implicit = true;
            for e in &t.elements {
                match value_show_type(e) {
                    Some((_, true)) => {}
                    _ => {
                        all_implicit = false;
                        break;
                    }
                }
            }
            // The prefix name for a heterogeneous-but-concrete tuple is not
            // derived here; only the implicit flag matters for the no-prefix
            // decision, and a non-implicit tuple eltype is rare in practice.
            Some(("Tuple".to_string(), all_implicit))
        }
        Value::NamedTuple(nt) => {
            let mut all_implicit = true;
            let mut fields = Vec::with_capacity(nt.values.len());
            for (name, value) in nt.names.iter().zip(nt.values.iter()) {
                match value_show_type(value) {
                    Some((ty, implicit)) => {
                        fields.push(format!("{name}::{ty}"));
                        all_implicit = all_implicit && implicit;
                    }
                    None => {
                        fields.push(format!("{name}::Any"));
                        all_implicit = false;
                    }
                }
            }
            Some((
                format!("@NamedTuple{{{}}}", fields.join(", ")),
                all_implicit,
            ))
        }
        // Structs carry their own name. `Pair` is implicit iff both fields are
        // implicit (upstream treats `Pair{Int64,Int64}` as implicit but
        // `Pair{Symbol,Any}` etc. as non-implicit). Other structs (e.g. user
        // `Foo`, `Complex{Int64}`) are non-implicit and prefixed by name.
        Value::Struct(s) => {
            // Issue #6882: an inline Memory-backed `Array{T,N}` *wrapper* element
            // must display like the native-array carrier — a `Vector` of such
            // wrappers prints bare (`[[1], [2]]`) for an implicit inner eltype,
            // not a spurious `Array{Int64, 1}[...]` prefix. Mirror the
            // native-array arm below using the wrapper's own Memory storage,
            // which is self-contained (no `struct_heap` needed for the eltype).
            if s.array_wrapper_julia_type().is_some() {
                if let Ok(Some(arr)) = crate::vm::value::array_wrapper_value_to_array_value(v, &[])
                {
                    let elem = arr.element_type();
                    let implicit = elem.typeinfo_implicit();
                    let container = match arr.shape.len() {
                        1 => format!("Vector{{{}}}", elem.julia_type_name()),
                        2 => format!("Matrix{{{}}}", elem.julia_type_name()),
                        n => format!("Array{{{}, {}}}", elem.julia_type_name(), n),
                    };
                    return Some((container, implicit));
                }
            }
            if (&*s.struct_name == "Pair" || s.struct_name.starts_with("Pair{"))
                && s.values.len() == 2
            {
                let implicit = matches!(value_show_type(&s.values[0]), Some((_, true)))
                    && matches!(value_show_type(&s.values[1]), Some((_, true)));
                return Some(("Pair".to_string(), implicit));
            }
            if s.struct_name.starts_with("Dict{") {
                let element_type = ArrayElementType::Abstract(s.struct_name.to_string());
                return Some((s.struct_name.to_string(), element_type.typeinfo_implicit()));
            }
            if s.is_complex() && s.values.len() == 2 {
                // Complex{T}: prefix is `Complex{T}` where T is the field type.
                let inner =
                    value_show_type(&s.values[0]).map_or_else(|| "Float64".to_string(), |(n, _)| n);
                return Some((format!("Complex{{{}}}", inner), false));
            }
            Some((s.struct_name.to_string(), false))
        }
        // Nested arrays: upstream `typeinfo_implicit` treats `Array{T,N}` of an
        // implicit eltype as implicit, so `[[1, 2], [3, 4]]` prints bare. A
        // non-implicit inner eltype carries a `Vector{T}[...]` / `Matrix{T}[...]`
        // outer prefix (e.g. `Vector{Int8}[...]`). The element-type tag drives
        // the decision so empty inner arrays are handled correctly.
        _ if is_native_array_value(v) => {
            let arr = native_array_value_ref(v)?;
            let borrow = arr.borrow();
            let elem = borrow.element_type();
            let implicit = elem.typeinfo_implicit();
            let container = match borrow.shape.len() {
                1 => format!("Vector{{{}}}", elem.julia_type_name()),
                2 => format!("Matrix{{{}}}", elem.julia_type_name()),
                n => format!("Array{{{}, {}}}", elem.julia_type_name(), n),
            };
            Some((container, implicit))
        }
        _ => None,
    }
}

/// Compute the array-show type prefix and whether the eltype is implicit,
/// mirroring upstream Julia's `typeinfo_prefix`/`typeinfo_implicit`
/// (`julia/base/arrayshow.jl`).
///
/// - Implicit eltype (`Int64`/`Float64`/`Char`/`String`/`Symbol`/implicit
///   `Tuple`/implicit `Pair`) → `("", true)`, so the array prints bare
///   (`[1, 2]`, `[1 => 2]`, `[(1, 2)]`).
/// - Non-implicit eltype (`Int8`, `Float32`, `Bool`, `Complex{Int64}`, user
///   `Foo`, …) → `("Foo", false)`, so the array prints `Foo[...]`.
///
/// For opaque element-type tags (`Any`/`Struct`/`StructOf`/`StructInlineOf`),
/// the effective eltype is derived from the element *values*: a homogeneous run
/// of one concrete type uses that type's name/implicit flag, while a
/// heterogeneous run (a genuine `Any` array such as `Any[1, "x"]`) prints the
/// `Any[...]` prefix. This is intentionally value-driven because sjulia stores
/// `Pair`/`Tuple`/`Complex`/struct arrays under the `Any`/`Struct` tags rather
/// than a precise parametric eltype (see `docs/vm/UNIMPLEMENTED.md` for the
/// inference divergences that keep some eltypes wider than upstream).
fn array_show_prefix(element_type: &ArrayElementType, elements: &[Value]) -> (String, bool) {
    // Tags with a precise, non-derived eltype answer directly.
    if element_type.typeinfo_implicit() {
        return (String::new(), true);
    }
    let opaque = matches!(
        element_type,
        ArrayElementType::Any
            | ArrayElementType::Struct
            | ArrayElementType::StructOf(_)
            | ArrayElementType::StructInlineOf(_, _)
            // Contiguous inline-f64 struct arrays derive their concrete
            // prefix from the reconstructed element values (Issue #9198 S4),
            // matching `StructInlineOf`.
            | ArrayElementType::StructInlineF64(_, _)
    );
    if matches!(element_type, ArrayElementType::Abstract(name) if name == "Pair")
        && elements
            .iter()
            .all(|e| matches!(value_show_type(e), Some((name, true)) if name == "Pair"))
    {
        return (String::new(), true);
    }
    if !opaque {
        // Concrete, non-implicit, named tag (Int8, Float32, ComplexF64, …).
        return (element_type.julia_type_name(), false);
    }
    // Opaque tag: derive the effective eltype from the element values.
    if elements.is_empty() {
        // Empty opaque arrays keep the tag's name (`Any` for the `Any` tag).
        return (element_type.julia_type_name(), false);
    }
    // For a genuine `Any` eltype, the value-driven derivation drops the prefix
    // only when the homogeneous element type is one sjulia *widened to `Any` from
    // a precise eltype* — `Pair`/`Tuple`/nested arrays, which upstream would have
    // inferred as `Vector{Pair{…}}`/`Vector{Tuple{…}}`/`Vector{Vector{…}}` and
    // print bare. A homogeneous run of a *scalar* implicit type (`Int64`,
    // `Float64`, `Char`, `String`, `Symbol`) under the `Any` tag means the user
    // *explicitly* wrote `Any[...]` (sjulia never widens scalar literals to
    // `Any`), so the `Any[...]` prefix is kept to match upstream's type-driven
    // `typeinfo_prefix` (`Any[1, 2, 3]`, not `[1, 2, 3]`). Issue #7303.
    let any_tag = matches!(element_type, ArrayElementType::Any);
    let mut common: Option<(String, bool)> = None;
    let mut all_inference_widened = true;
    for e in elements {
        all_inference_widened = all_inference_widened && value_is_inference_widened_composite(e);
        match value_show_type(e) {
            Some(info) => match &common {
                None => common = Some(info),
                Some((name, _)) if *name == info.0 => {}
                Some(_) => return ("Any".to_string(), false), // heterogeneous → Any
            },
            None => return ("Any".to_string(), false), // unknown element type → Any
        }
    }
    match common {
        // Homogeneous implicit eltype: drop the prefix only when the eltype is an
        // inference-widened composite, or when the tag itself is precise (a
        // struct tag never reaches this implicit arm). A scalar implicit run
        // under the genuine `Any` tag keeps the `Any[...]` prefix (Issue #7303).
        Some((_, true)) if all_inference_widened || !any_tag => (String::new(), true),
        Some((_, true)) => ("Any".to_string(), false), // explicit `Any[scalars]`
        Some((name, false)) => (name, false),          // homogeneous non-implicit eltype → prefix
        None => ("Any".to_string(), false),
    }
}

/// Whether a value's type is one that sjulia *widens to `Any`* in an array
/// literal where upstream Julia would have inferred a precise element type —
/// `Pair`, `Tuple`, and nested `Array`/`Vector`/`Matrix` (see
/// `docs/vm/UNIMPLEMENTED.md`). These are the only cases for which the
/// value-driven array-show prefix derivation may drop the `Any[...]` prefix; a
/// scalar element under an `Any` tag denotes an explicit `Any[...]` literal that
/// must keep its prefix (Issue #7303).
fn value_is_inference_widened_composite(v: &Value) -> bool {
    match v {
        Value::Tuple(_) => true,
        Value::NamedTuple(_) => true,
        Value::Struct(s) => {
            // A Memory-backed `Array{T,N}` *wrapper* element is a nested array
            // (Issue #6882); a `Pair` is the widened composite eltype.
            s.array_wrapper_julia_type().is_some()
                || &*s.struct_name == "Pair"
                || s.struct_name.starts_with("Pair{")
        }
        _ => is_native_array_value(v),
    }
}

/// Format an array's elements with a caller-supplied per-element formatter.
///
/// The formatter receives `(linear_index, &element)` so a caller can override
/// specific elements by their column-major linear position — used by the
/// VM-side user-`show` pre-rendering path (Issue #7893), where struct elements
/// (e.g. `Symbolics.Num`) are rendered by running their registered
/// `Base.show(io, ::T)` and the result is keyed by linear index.
fn format_array_value_with<F>(arr: &ArrayRef, mut format_element: F) -> String
where
    F: FnMut(usize, &Value) -> String,
{
    let arr_borrow = arr.borrow();
    let element_type = arr_borrow.element_type();
    // Render a `Complex{FloatNN}` eltype prefix via its `ComplexFNN` alias to
    // match upstream (e.g. `ComplexF64[...]`, not `Complex{Float64}[...]`) —
    // Issue #5704.
    let type_name = apply_complex_float_aliases(&element_type.julia_type_name());

    if arr_borrow.shape.len() == 1 {
        // 1D Vector: compact "[a, b, c]" form (matches Julia's `print`/
        // `string` semantics for AbstractArray; the multi-line "N-element
        // Vector{T}:" form is reserved for `display`/`show(io, "text/plain", ...)`,
        // which the REPL routes through a different formatter).
        // See Julia's base/arrayshow.jl `show_vector` and Issue #3553.
        let n = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        // Issue #3548: empty 1D vectors should display as `T[]` (matches Julia's
        // `show` output, which `println` falls through to via `display`).
        if n == 0 {
            return format!("{}[]", type_name);
        }
        let display_count = n.min(100);
        let mut values = Vec::with_capacity(display_count);
        for i in 0..display_count {
            if let Ok(v) = arr_borrow.get_linear(i) {
                values.push(v);
            }
        }
        // Issues #5236 / #5237: emit the upstream `typeinfo_prefix` type prefix
        // (e.g. `Int8[...]`, `Float32[...]`, `Complex{Int64}[...]`, `Foo[...]`,
        // `Any[...]`) when the eltype is non-implicit, and nothing (`[...]`)
        // for implicit eltypes (`Int64`/`Float64`/`Char`/`String`/`Symbol`/
        // implicit `Tuple`/`Pair`). The previous code only emitted a prefix
        // for `SubString`, dropping the eltype prefix for every other
        // non-implicit eltype.
        let (prefix, _implicit) = array_show_prefix(&element_type, &values);
        let prefix = apply_complex_float_aliases(&prefix);
        let mut parts: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(i, v)| format_element(i, v))
            .collect();
        if n > 100 {
            parts.push("…".to_string());
        }
        format!("{}[{}]", prefix, parts.join(", "))
    } else if arr_borrow.shape.len() == 2 {
        // 2D Matrix: compact "[a b c; d e f]" form to match Julia's
        // `print(matrix)`. The aligned multi-line "m×n Matrix{T}:" form is
        // reserved for `display` semantics. See Issue #3553.
        let rows = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        let cols = arr_borrow.shape[ARRAY_SECOND_DIM_INDEX];
        // Gather elements (column-major) for the prefix derivation
        // (Issues #5236 / #5237), mirroring the 1D path above.
        let total = arr_borrow.element_count();
        let mut all_values = Vec::with_capacity(total);
        for i in 0..total {
            all_values.push(arr_borrow.get_linear(i).unwrap_or(Value::Nothing));
        }
        let (prefix, _implicit) = array_show_prefix(&element_type, &all_values);
        let prefix = apply_complex_float_aliases(&prefix);
        let mut row_strs = Vec::with_capacity(rows);
        for r in 0..rows {
            let row: Vec<String> = (0..cols)
                .map(|c| {
                    let lin = r + c * rows;
                    arr_borrow
                        .get_linear(lin)
                        .map_or_else(|_| String::new(), |v| format_element(lin, &v))
                })
                .collect();
            row_strs.push(row.join(" "));
        }
        format!("{}[{}]", prefix, row_strs.join("; "))
    } else {
        // Higher dimensions: summary (index-based, limit to 100)
        let total = arr_borrow.element_count();
        let display_count = total.min(100);
        let parts: Vec<String> = (0..display_count)
            .filter_map(|i| arr_borrow.get_linear(i).ok().map(|v| (i, v)))
            .map(|(i, v)| format_element(i, &v))
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

fn format_array_value(arr: &ArrayRef) -> String {
    // Issue #5159: Bool arrays match upstream's typeinfo-aware array show via
    // `print`/`string`: a `Bool[...]` type prefix is emitted and elements
    // render as the integers `1`/`0` (not `true`/`false`). This mirrors the
    // pure-Julia `_show_vector_compact` / `_show_matrix_compact` path used by
    // `repr`, keeping `print([true, false])`, `string(...)`, and `repr(...)`
    // all equal to `"Bool[1, 0]"`. Empty Bool arrays keep the existing
    // `Bool[]` / `Matrix{Bool}(undef, r, c)` rendering handled below.
    if matches!(arr.borrow().element_type(), ArrayElementType::Bool) {
        if let Some(s) = format_bool_array_value(arr) {
            return s;
        }
    }
    format_array_value_with(arr, |_idx, v| format_array_element(v))
}

/// Format an array, overriding selected elements with caller-supplied,
/// pre-rendered strings keyed by column-major linear index (Issue #7893).
///
/// The VM uses this to splice in per-element `Base.show(io, ::T)` output for
/// struct elements (e.g. `Symbolics.Num`) that cannot be produced by the
/// pure-Rust formatter (which has no way to re-enter the interpreter). Any
/// `None` slot falls back to the default `format_array_element`, so numeric
/// arrays and arrays of structs without a registered `show` are unchanged.
pub(crate) fn format_array_value_prerendered(
    arr: &ArrayRef,
    prerendered: &[Option<String>],
) -> String {
    format_array_value_with(arr, |idx, v| {
        prerendered
            .get(idx)
            .and_then(|slot| slot.clone())
            .unwrap_or_else(|| format_array_element(v))
    })
}

/// Render a `Bool`-eltype array in upstream's `print`/`string` form
/// (Issue #5159): `Bool[1, 0]` for vectors and `Bool[1 0; 0 1]` for matrices,
/// with elements as `1`/`0`. Returns `None` for shapes/sizes that should fall
/// through to the generic formatter (empty arrays and >2-D arrays), which
/// already match upstream.
fn format_bool_array_value(arr: &ArrayRef) -> Option<String> {
    let arr_borrow = arr.borrow();
    let bool_str = |v: &Value| -> String {
        match v {
            Value::Bool(true) => "1".to_string(),
            Value::Bool(false) => "0".to_string(),
            other => format_value_impl(other),
        }
    };
    if arr_borrow.shape.len() == 1 {
        let n = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        if n == 0 {
            return None; // `Bool[]` already handled by the generic path
        }
        let display_count = n.min(100);
        let mut parts = Vec::with_capacity(display_count);
        for i in 0..display_count {
            if let Ok(v) = arr_borrow.get_linear(i) {
                parts.push(bool_str(&v));
            }
        }
        if n > 100 {
            parts.push("…".to_string());
        }
        Some(format!("Bool[{}]", parts.join(", ")))
    } else if arr_borrow.shape.len() == 2 {
        let rows = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        let cols = arr_borrow.shape[ARRAY_SECOND_DIM_INDEX];
        if rows == 0 || cols == 0 {
            return None; // `Matrix{Bool}(undef, r, c)` handled by generic path
        }
        let mut row_strs = Vec::with_capacity(rows);
        for r in 0..rows {
            let row: Vec<String> = (0..cols)
                .map(|c| {
                    arr_borrow
                        .get_linear(r + c * rows)
                        .map_or_else(|_| String::new(), |v| bool_str(&v))
                })
                .collect();
            row_strs.push(row.join(" "));
        }
        Some(format!("Bool[{}]", row_strs.join("; ")))
    } else {
        None
    }
}

/// Format a RegexMatch value for display, matching upstream `show(::IO,
/// ::RegexMatch)` (Issue #10182): `RegexMatch("a", 1="a", 2=nothing)`. Each
/// capture group is printed as `key=value`, where `key` is the group's name for
/// named groups and its 1-based index otherwise, and `value` is the captured
/// substring (quoted) or `nothing`.
#[cold]
fn format_regexmatch_value(m: &RegexMatchValue) -> String {
    let mut out = format!("RegexMatch(\"{}\"", m.match_str);
    for (i, capture) in m.captures.iter().enumerate() {
        let key = match m.capture_names.get(i).and_then(|name| name.as_deref()) {
            Some(name) => name.to_string(),
            None => (i + 1).to_string(),
        };
        let shown = match capture {
            Some(text) => format!("\"{}\"", text),
            None => "nothing".to_string(),
        };
        out.push_str(", ");
        out.push_str(&key);
        out.push('=');
        out.push_str(&shown);
    }
    out.push(')');
    out
}

/// Compact `[a, b, c]` / `T[]` form of a `Memory`, matching upstream
/// `print` / `string` / `repr` / `show(io, ::Memory)` (Issue #6697). Mirrors the
/// `Array{T}` wrapper compact form (`format_array_wrapper_compact`). The
/// multi-line "N-element Memory{T}:" form is `display` (REPL) semantics and is
/// produced on the separate REPL display path.
#[cold]
fn format_memory_compact(mem: &MemoryRef) -> String {
    let borrow = mem.borrow();
    let n = borrow.len();
    let element_type = borrow.element_type().clone();
    if n == 0 {
        // Empty memory renders like an empty array: `Int64[]` (Issue #6697).
        let eltype = apply_complex_float_aliases(&element_type.julia_type_name());
        return format!("{}[]", eltype);
    }
    let mut elements = Vec::with_capacity(n);
    for i in 1..=n {
        if let Ok(v) = borrow.get(i) {
            elements.push(v);
        }
    }
    // Non-implicit eltypes (Bool, Int8, Float32, ComplexF64, …) gain a `T[...]`
    // typeinfo prefix, exactly as the array compact form does (Issue #5774).
    let bool_elt = element_type == ArrayElementType::Bool;
    let (value_prefix, _) = array_show_prefix(&element_type, &elements);
    let prefix = apply_complex_float_aliases(&value_prefix);
    let parts: Vec<String> = elements
        .iter()
        .map(|v| format_array_wrapper_element(v, bool_elt))
        .collect();
    format!("{}[{}]", prefix, parts.join(", "))
}

/// Format a MemoryRef value for display.
#[cold]
fn format_memory_ref_value(memref: &MemoryRefValue) -> String {
    format!(
        "{}(index={})",
        memref.julia_type_name(),
        memref.memory_index()
    )
}

// ============================================================================
// value_to_string - Simple string conversion
// ============================================================================

/// Convert a Value to its string representation.
pub(crate) fn value_to_string(r: &Resolved) -> String {
    value_to_string_impl(r.value())
}

/// Heap-less string-conversion implementation (Issue #8661). See
/// [`format_value_print_impl`] for the wrapper/impl split rationale.
fn value_to_string_impl(val: &Value) -> String {
    // Route the legacy native-array carrier through the shared
    // `native_array_value_ref` helper so the match below no longer holds a
    // native-array arm (Issue #3908). `format_array_value` handles
    // shape-aware Julia-style display.
    if let Some(arr) = native_array_value_ref(val) {
        return format_array_value(arr);
    }
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
        Value::F16(f) => format_float16_julia(*f),
        Value::F32(f) => f.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::BigFloat(n) => format_bigfloat_julia(n),
        Value::F64(f) => {
            // Format like Julia: remove trailing zeros but keep decimal point if needed
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                f.to_string()
            }
        }
        Value::Str(s) => s.to_string(),
        Value::StrBytes(bytes) => String::from_utf8_lossy(bytes.as_ref()).into_owned(),
        Value::Char(c) => format!("'{}'", c),
        Value::CharMalformed(bits) => format!("'{}'", escape_char_malformed_for_show(*bits)),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::SliceAll => ":".to_string(),
        Value::Range(r) => format_range_value(r),
        Value::Struct(s) if s.is_complex() => format_complex_struct(s),
        Value::Struct(s) => struct_instance_as_range_value(s)
            .map(|range| format_range_value(&range))
            .unwrap_or_else(|| {
                let fields_str: Vec<String> = s.values.iter().map(value_to_string_impl).collect();
                format!("Struct({})", fields_str.join(", "))
            }),
        Value::StructRef(idx) => format!("StructRef({})", idx),
        Value::Rng(rng) => format_rng(rng),
        Value::Tuple(t) => {
            let elements_str: Vec<String> = t.elements.iter().map(value_to_string_impl).collect();
            format!("({})", elements_str.join(", "))
        }
        // Issue #4722: Core.SimpleVector (svec) prints as `svec(...)`.
        Value::SimpleVector(sv) => {
            let elements_str: Vec<String> = sv.elements.iter().map(value_to_string_impl).collect();
            format!("svec({})", elements_str.join(", "))
        }
        Value::NamedTuple(nt) => {
            // Empty -> `NamedTuple()`, single field -> trailing comma (Issue #5776).
            if nt.names.is_empty() {
                "NamedTuple()".to_string()
            } else {
                let fields_str: Vec<String> = nt
                    .names
                    .iter()
                    .zip(nt.values.iter())
                    .map(|(name, val)| format!("{} = {}", name, value_to_string_impl(val)))
                    .collect();
                if fields_str.len() == 1 {
                    format!("({},)", fields_str[0])
                } else {
                    format!("({})", fields_str.join(", "))
                }
            }
        }
        Value::Ref(inner) => {
            // Base.RefValue{T}(value) (Issue #5130) - matches upstream display.
            let v = inner.borrow();
            format!(
                "Base.RefValue{{{}}}({})",
                v.runtime_type(),
                value_to_string_impl(&v)
            )
        }
        Value::Generator(_) => "Generator(...)".to_string(),
        Value::DataType(jt) => {
            // display_name (not name/to_string): trailing unbounded where
            // binders elide only at display boundaries (Issue #10505).
            qualify_datatype_display(apply_complex_float_aliases(&jt.display_name()))
        }
        Value::RuntimeTypeVar(tv) => format_runtime_typevar(tv),
        Value::RuntimeTypeName(type_name) => format!("typename({})", type_name.name),
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
            let outer_str = value_to_string_impl(&cf.outer);
            let inner_str = value_to_string_impl(&cf.inner);
            format!("{} ∘ {}", outer_str, inner_str)
        }
        Value::Undef => "#undef".to_string(),
        Value::IO(io_ref) => {
            if io_ref.borrow().is_stdout() {
                "stdout".to_string()
            } else if io_ref.borrow().is_stderr() {
                "stderr".to_string()
            } else if io_ref.borrow().is_devnull() {
                "devnull".to_string()
            } else if io_ref.borrow().is_pipe() {
                "Pipe()".to_string()
            } else {
                "IOBuffer(...)".to_string()
            }
        }
        // Macro system types
        Value::Symbol(s) => format!(":{}", s.as_str()),
        Value::Expr(e) => expr_to_julia_string(e),
        Value::QuoteNode(v) => format!("QuoteNode({})", value_to_string_impl(v)),
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        Value::Binding(binding) => binding.to_string(),
        // Base.Pairs type (for kwargs...)
        Value::Pairs(p) => {
            let pairs_str: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(name, val)| format!(":{} => {}", name, value_to_string_impl(val)))
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
        Value::RegexMatch(m) => format_regexmatch_value(m),
        // Enum type
        Value::Enum { type_name, value } => format_enum_value(type_name, *value),
        // Memory type
        Value::Memory(mem) => {
            let mem = mem.borrow();
            let n = mem.len();
            let type_name = mem.element_type().julia_type_name();
            format!("{}-element Memory{{{}}}", n, type_name)
        }
        Value::MemoryRef(memref) => format_memory_ref_value(memref),
        // Issue #7964: flat static array.
        Value::StaticArray(sv) => {
            let elems: Vec<String> = (0..sv.len())
                .filter_map(|i| sv.elems.get_value(i))
                .map(|v| format_value_impl(&v))
                .collect();
            format!("{}(({}))", sv.julia_type_name(), elems.join(", "))
        }
        // Issue #7964 Phase 3: inline variant.
        Value::StaticArrayInline(sv) => {
            let type_name = sv.julia_type_name_owned();
            let elems: Vec<String> = (0..sv.len())
                .map(|i| format_value_impl(&sv.get_0indexed(i)))
                .collect();
            format!("{}(({}))", type_name, elems.join(", "))
        }
        // The legacy native-array carrier is filtered out by the early-return
        // above (Issue #3908). This wildcard satisfies Rust's exhaustiveness
        // checking and provides a safe default for any future `Value` variant:
        // fall back to the value's Debug representation.
        _ => format!("{:?}", val),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn main_visibility_grants_and_revokes_display_qualification_11365() {
        use crate::vm::main_scope_visibility as visibility;
        visibility::reset_main_scope_visibility();
        let dt = Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Main.Geo.Point".to_string(),
        )));
        visibility::note_main_scope_binding("Point", &dt);
        assert!(visibility::main_visible_type_leaf("Point", "Geo.Point"));
        assert!(visibility::main_visible_type_leaf(
            "Point",
            "Geo.Point{Int64}"
        ));
        // A different family with the same leaf is NOT visible through it.
        assert!(!visibility::main_visible_type_leaf("Point", "Other.Point"));
        // The visible leaf strips its owner in struct display.
        assert_eq!(display_type_name("Geo.Point{Int64}"), "Point{Int64}");
        // Rebinding to a non-type revokes visibility.
        visibility::note_main_scope_binding("Point", &Value::I64(1));
        assert!(!visibility::main_visible_type_leaf("Point", "Geo.Point"));
        // Qualified and internal names never participate.
        visibility::note_main_scope_binding("Geo.Point", &dt);
        visibility::note_main_scope_binding("#internal", &dt);
        assert!(!visibility::main_visible_type_leaf("Point", "Geo.Point"));
        visibility::reset_main_scope_visibility();
    }

    #[test]
    fn user_module_roots_gate_main_prefix_qualification_11365() {
        use crate::vm::main_scope_visibility as visibility;
        visibility::reset_main_scope_visibility();
        visibility::set_user_module_roots(["Geo".to_string()]);
        assert!(visibility::is_user_module_root("Geo"));
        assert!(!visibility::is_user_module_root("Base"));
        // A program-declared user root that is not Main-visible gets the
        // Main. prefix; unknown roots keep the historical bare form.
        assert_eq!(display_type_name("Geo.Hidden"), "Main.Geo.Hidden");
        assert_eq!(
            qualify_datatype_display("Geo.Hidden{Int64}".to_string()),
            "Main.Geo.Hidden{Int64}"
        );
        assert_eq!(display_type_name("Elsewhere.Thing"), "Thing");
        visibility::reset_main_scope_visibility();
        assert!(!visibility::is_user_module_root("Geo"));
    }

    #[test]
    fn cyclic_struct_ref_formatting_terminates_issue_10893() {
        let heap = vec![StructInstance::with_name(
            0,
            "Node".to_string(),
            vec![Value::StructRef(0)],
        )];
        let resolved = resolve_struct_refs_for_format(&Value::StructRef(0), &heap);
        assert_eq!(
            format_value_impl(&resolved),
            "Node(#= circular reference @-1 =#)"
        );
        assert!(!value_contains_struct_ref(&resolved));
    }

    #[test]
    fn two_node_struct_ref_cycle_reports_relative_depth_issue_10893() {
        let heap = vec![
            StructInstance::with_name(0, "Left".to_string(), vec![Value::StructRef(1)]),
            StructInstance::with_name(1, "Right".to_string(), vec![Value::StructRef(0)]),
        ];
        let resolved = resolve_struct_refs_for_format(&Value::StructRef(0), &heap);
        assert_eq!(
            format_value_impl(&resolved),
            "Left(Right(#= circular reference @-2 =#))"
        );
        assert!(!value_contains_struct_ref(&resolved));
    }
    // Julia-code stringification moved to the `julia_code` submodule (#6835);
    // these tests exercise it directly.
    use super::julia_code::{
        format_symbol_name, is_unary_op, operator_precedence, value_to_julia_code,
    };

    // ── format_symbol_name / var"name" round-trip (Issue #7676) ──────────────

    #[test]
    fn test_format_symbol_name_var_string_issue_7676() {
        // Valid identifiers print bare.
        assert_eq!(format_symbol_name("postwalk"), "postwalk");
        assert_eq!(format_symbol_name("foo!"), "foo!");
        assert_eq!(format_symbol_name("_x"), "_x");
        assert_eq!(format_symbol_name("α"), "α");
        // Operators stay bare (the formatter's existing operator handling).
        assert_eq!(format_symbol_name("+"), "+");
        assert_eq!(format_symbol_name("=="), "==");
        assert_eq!(format_symbol_name("!"), "!");
        // Non-identifier, non-operator symbols become var"name" (Issue #7676):
        // macro names (@q), names with spaces, and digit-leading names.
        assert_eq!(format_symbol_name("@q"), "var\"@q\"");
        assert_eq!(format_symbol_name("@qq"), "var\"@qq\"");
        assert_eq!(format_symbol_name("a b"), "var\"a b\"");
        assert_eq!(format_symbol_name("2x"), "var\"2x\"");
        // Embedded quote/backslash are escaped inside the var"..." literal.
        assert_eq!(format_symbol_name("a\"b"), "var\"a\\\"b\"");
        assert_eq!(format_symbol_name("a\\b"), "var\"a\\\\b\"");
    }

    // ── apply_complex_float_aliases (Issue #5704) ────────────────────────────

    #[test]
    fn test_apply_complex_float_aliases_issue_5704() {
        // Top-level aliases
        assert_eq!(
            apply_complex_float_aliases("Complex{Float64}"),
            "ComplexF64"
        );
        assert_eq!(
            apply_complex_float_aliases("Complex{Float32}"),
            "ComplexF32"
        );
        assert_eq!(
            apply_complex_float_aliases("Complex{Float16}"),
            "ComplexF16"
        );
        // Recursive / nested
        assert_eq!(
            apply_complex_float_aliases("Vector{Complex{Float64}}"),
            "Vector{ComplexF64}"
        );
        assert_eq!(
            apply_complex_float_aliases("Tuple{Complex{Float64}, Int64}"),
            "Tuple{ComplexF64, Int64}"
        );
        // Non-aliased complex stays as-is
        assert_eq!(
            apply_complex_float_aliases("Complex{Int64}"),
            "Complex{Int64}"
        );
        // Boundary: a user type whose name merely ENDS with the alias pattern is
        // NOT mangled (the `Complex` must be at a type-name boundary).
        assert_eq!(
            apply_complex_float_aliases("MyComplex{Float64}"),
            "MyComplex{Float64}"
        );
        // No complex at all → untouched
        assert_eq!(
            apply_complex_float_aliases("Vector{Int64}"),
            "Vector{Int64}"
        );
        // Array prefix form
        assert_eq!(
            apply_complex_float_aliases("Complex{Float64}[1.0 + 2.0im]"),
            "ComplexF64[1.0 + 2.0im]"
        );
    }

    // ── format_runtime_typevar (Issue #5644) ─────────────────────────────────

    #[test]
    fn test_format_runtime_typevar_anonymous_bounds_issue_5644() {
        use crate::types::JuliaType;
        use crate::vm::value::RuntimeTypeVarValue;
        let tv = |name: &str, lower: JuliaType, upper: JuliaType| RuntimeTypeVarValue {
            id: 0,
            name: name.to_string(),
            lower_bound: lower,
            upper_bound: upper,
        };

        // Anonymous (`_`) bounds use the bound-only shorthand, no `_`.
        assert_eq!(
            format_runtime_typevar(&tv("_", JuliaType::Bottom, JuliaType::Integer)),
            "<:Integer"
        );
        assert_eq!(
            format_runtime_typevar(&tv("_", JuliaType::Int64, JuliaType::Any)),
            ">:Int64"
        );

        // Named typevars keep their name; unbounded prints the bare name.
        assert_eq!(
            format_runtime_typevar(&tv("T", JuliaType::Bottom, JuliaType::Real)),
            "T<:Real"
        );
        assert_eq!(
            format_runtime_typevar(&tv("T", JuliaType::Bottom, JuliaType::Any)),
            "T"
        );

        // A bound that refers to an outer TypeVar keeps the binder name only.
        // The outer binder carries its own bound in the enclosing `where` list
        // (Issue #9721).
        assert_eq!(
            format_runtime_typevar(&tv(
                "T",
                JuliaType::Bottom,
                JuliaType::TypeVar("S".to_string(), Some("Real".to_string()))
            )),
            "T<:S"
        );
    }

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
        assert!(
            result.starts_with("1."),
            "Expected '1.25...', got: {}",
            result
        );
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
        assert_eq!(format_value_impl(&Value::I64(42)), "42");
        assert_eq!(format_value_impl(&Value::I64(-7)), "-7");
        assert_eq!(format_value_impl(&Value::I64(0)), "0");
    }

    #[test]
    fn test_format_value_f64_whole_number() {
        // Whole-number floats get ".0" suffix (Julia style)
        assert_eq!(format_value_impl(&Value::F64(1.0)), "1.0");
        assert_eq!(format_value_impl(&Value::F64(0.0)), "0.0");
        assert_eq!(format_value_impl(&Value::F64(-3.0)), "-3.0");
    }

    #[test]
    fn test_format_value_bool() {
        assert_eq!(format_value_impl(&Value::Bool(true)), "true");
        assert_eq!(format_value_impl(&Value::Bool(false)), "false");
    }

    #[test]
    fn test_format_value_str() {
        assert_eq!(
            format_value_impl(&Value::str_new("hello".to_string())),
            "hello"
        );
        assert_eq!(format_value_impl(&Value::str_new(String::new())), "");
    }

    #[test]
    fn test_format_value_nothing() {
        assert_eq!(format_value_impl(&Value::Nothing), "nothing");
    }

    #[test]
    fn test_format_value_missing() {
        assert_eq!(format_value_impl(&Value::Missing), "missing");
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
            format_sprintf("%s", &[Value::str_new("hello".to_string())]),
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
        assert_eq!(value_to_string_impl(&Value::I64(100)), "100");
        assert_eq!(value_to_string_impl(&Value::I64(-1)), "-1");
    }

    #[test]
    fn test_value_to_string_bool() {
        assert_eq!(value_to_string_impl(&Value::Bool(true)), "true");
        assert_eq!(value_to_string_impl(&Value::Bool(false)), "false");
    }

    #[test]
    fn test_value_to_string_str_is_unquoted() {
        // value_to_string returns the raw string without quotes (unlike repr)
        assert_eq!(
            value_to_string_impl(&Value::str_new("hi".to_string())),
            "hi"
        );
    }

    #[test]
    fn test_value_to_string_nothing() {
        assert_eq!(value_to_string_impl(&Value::Nothing), "nothing");
    }

    #[test]
    fn test_value_to_string_f64_whole_number() {
        // value_to_string also applies Julia-style .0 suffix for whole floats
        assert_eq!(value_to_string_impl(&Value::F64(5.0)), "5.0");
        assert_eq!(value_to_string_impl(&Value::F64(0.0)), "0.0");
    }

    // ── format_value (Vector / Matrix compact form, Issue #3553) ──────────────
    // These tests lock in the `print`/`string` semantics for AbstractArray:
    // 1D Vectors must print as `[a, b, c]` and 2D Matrices as `[a b; c d]`,
    // matching official Julia. The multi-line "N-element Vector{T}:" form is
    // reserved for `display` semantics and lives in the Pure-Julia
    // `_show_vector` / `_show_matrix` helpers in `julia/base/io.jl`.

    use super::super::value::{
        native_array_value_from_array as array_value, new_memory_ref, ArrayElementType, ArrayValue,
        MemoryValue,
    };

    #[test]
    fn test_format_value_vector_compact_inline() {
        // 1D Vector{Int64}: `print([1, 2, 3])` → "[1, 2, 3]"
        let arr = ArrayValue::from_i64(vec![1, 2, 3], vec![3]);
        let v = array_value(arr);
        assert_eq!(format_value_impl(&v), "[1, 2, 3]");
        assert_eq!(value_to_string_impl(&v), "[1, 2, 3]");
        assert_eq!(value_to_julia_code(&v), "[1, 2, 3]");
    }

    #[test]
    fn test_format_value_empty_vector_compact_inline() {
        // Empty Vector{Int64}: `print(Int64[])` → "Int64[]" (matches Julia 1.12).
        let arr = ArrayValue::from_i64(vec![], vec![0]);
        let v = array_value(arr);
        assert_eq!(format_value_impl(&v), "Int64[]");
        assert_eq!(value_to_string_impl(&v), "Int64[]");
        assert_eq!(value_to_julia_code(&v), "Int64[]");
    }

    #[test]
    fn test_format_value_matrix_compact_inline() {
        // 2D Matrix: column-major layout `[1 2 3; 4 5 6]`
        // Element at (r, c) is data[r + c * rows], so for shape [2, 3]:
        // column 0 = [1, 4], column 1 = [2, 5], column 2 = [3, 6]
        let arr = ArrayValue::from_i64(vec![1, 4, 2, 5, 3, 6], vec![2, 3]);
        let v = array_value(arr);
        assert_eq!(format_value_impl(&v), "[1 2 3; 4 5 6]");
        assert_eq!(value_to_string_impl(&v), "[1 2 3; 4 5 6]");
        assert_eq!(value_to_julia_code(&v), "[1 2 3; 4 5 6]");
    }

    #[test]
    fn test_format_value_struct_backed_vector_memoryref_storage() {
        // Issue #6649: a faithful `Array{T,1}` struct storing `ref::MemoryRef{T}`
        // (rather than a bare `Memory`) prints as `[...]`, not its raw fields.
        use super::super::value::{MemoryRefValue, StructInstance, TupleValue};
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::I64, 3);
        mem.set(1, Value::I64(10)).unwrap();
        mem.set(2, Value::I64(20)).unwrap();
        mem.set(3, Value::I64(30)).unwrap();
        let memref = MemoryRefValue::first(new_memory_ref(mem));
        let size = Value::Tuple(TupleValue::new(vec![Value::I64(3)]));
        let si = StructInstance::with_name(
            0,
            "Array{Int64, 1}".to_string(),
            vec![Value::MemoryRef(Box::new(memref)), size],
        );
        assert_eq!(format_value_impl(&Value::Struct(si)), "[10, 20, 30]");
    }

    #[test]
    fn test_array_wrapper_eltype_name_strips_ndims_param() {
        // Issue #6649: the faithful `Array{T,N}` name carries the ndims param `N`
        // after a top-level comma; the element type is the first param (and
        // nested braces are respected).
        assert_eq!(super::array_wrapper_eltype_name("Array{Int64, 1}"), "Int64");
        assert_eq!(
            super::array_wrapper_eltype_name("Array{Float64, 2}"),
            "Float64"
        );
        assert_eq!(
            super::array_wrapper_eltype_name("Array{Complex{Float64}, 1}"),
            "Complex{Float64}"
        );
        assert_eq!(super::array_wrapper_eltype_name("Array{Int64}"), "Int64");
        assert_eq!(super::array_wrapper_eltype_name("Array{}"), "Any");
    }

    #[test]
    fn test_value_to_julia_code_memory_reads_storage_directly() {
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::I64, 3);
        assert!(mem.set(1, Value::I64(1)).is_ok());
        assert!(mem.set(2, Value::I64(2)).is_ok());
        assert!(mem.set(3, Value::I64(3)).is_ok());

        let value = Value::Memory(new_memory_ref(mem));
        assert_eq!(value_to_julia_code(&value), "[1, 2, 3]");
    }

    // ── Resolved newtype: deep StructRef resolution witness ─────────────────
    // (Issue #8660, step 1/3 of #8642 — the typed replacement that retired the
    // grep-based audit check_format_value_resolves_structref.sh in #8662.)

    /// Two-entry struct heap: heap[0] = Pair(1, 2); heap[1] = Wrap(StructRef(0))
    /// so that resolving heap[1] exercises the chained ref → ref case.
    fn resolved_test_heap() -> Vec<StructInstance> {
        use super::super::value::StructInstance;
        vec![
            StructInstance::with_name(1, "Pair".to_string(), vec![Value::I64(1), Value::I64(2)]),
            StructInstance::with_name(2, "Wrap".to_string(), vec![Value::StructRef(0)]),
        ]
    }

    /// The carrier must start with a reachable StructRef, and `Resolved::new`
    /// must leave none anywhere in the tree (deep resolution, Issue #5234).
    fn assert_resolved_deeply(carrier: &Value) {
        let heap = resolved_test_heap();
        assert!(
            value_contains_struct_ref(carrier),
            "test carrier must contain an unresolved StructRef"
        );
        let resolved = Resolved::new(carrier, &heap);
        assert!(
            !value_contains_struct_ref(resolved.value()),
            "Resolved::new left an unresolved StructRef in the tree"
        );
        // Parity with the raw resolver the display call sites use today:
        // the witness must not change what gets formatted.
        assert_eq!(
            format_value_impl(resolved.value()),
            format_value_impl(&resolve_struct_refs_for_format(carrier, &heap))
        );
    }

    fn any_memory(elements: Vec<Value>) -> MemoryValue {
        let len = elements.len();
        MemoryValue::new(
            super::super::value::ArrayData::Any(elements),
            ArrayElementType::Any,
            len,
        )
    }

    #[test]
    fn resolved_deeply_resolves_bare_structref() {
        let heap = resolved_test_heap();
        let v = Value::StructRef(0);
        let resolved = Resolved::new(&v, &heap);
        assert!(!value_contains_struct_ref(resolved.value()));
        assert_eq!(format_value_impl(resolved.value()), "1 => 2");
    }

    #[test]
    fn resolved_deeply_resolves_tuple_carrier() {
        use super::super::value::TupleValue;
        assert_resolved_deeply(&Value::Tuple(TupleValue {
            elements: vec![Value::I64(7), Value::StructRef(0)],
        }));
    }

    #[test]
    fn resolved_deeply_resolves_simple_vector_carrier() {
        use super::super::value::TupleValue;
        assert_resolved_deeply(&Value::SimpleVector(TupleValue {
            elements: vec![Value::StructRef(0)],
        }));
    }

    #[test]
    fn resolved_deeply_resolves_named_tuple_carrier() {
        use super::super::value::NamedTupleValue;
        let nt = NamedTupleValue::new(vec!["p".to_string()], vec![Value::StructRef(0)]).unwrap();
        assert_resolved_deeply(&Value::NamedTuple(nt));
    }

    #[test]
    fn resolved_deeply_resolves_ref_carrier() {
        use super::super::value::new_ref;
        assert_resolved_deeply(&new_ref(Value::StructRef(0)));
    }

    #[test]
    fn resolved_deeply_resolves_quote_node_carrier() {
        assert_resolved_deeply(&Value::QuoteNode(Box::new(Value::StructRef(0))));
    }

    #[test]
    fn resolved_deeply_resolves_struct_field_and_chained_ref() {
        use super::super::value::StructInstance;
        // Outer struct field → StructRef(1) = Wrap(StructRef(0)) → Pair(1, 2):
        // two levels of heap indirection under an inline Struct carrier.
        assert_resolved_deeply(&Value::Struct(StructInstance::with_name(
            3,
            "Outer".to_string(),
            vec![Value::StructRef(1)],
        )));
    }

    #[test]
    fn resolved_deeply_resolves_memory_carrier() {
        // The `println([1 => 2, 3 => 4])` leak of Issue #5234: array elements
        // held as StructRef inside the backing Memory.
        assert_resolved_deeply(&Value::Memory(new_memory_ref(any_memory(vec![
            Value::StructRef(0),
            Value::I64(9),
        ]))));
    }

    #[test]
    fn resolved_deeply_resolves_memory_ref_carrier() {
        use super::super::value::MemoryRefValue;
        assert_resolved_deeply(&Value::MemoryRef(Box::new(MemoryRefValue::first(
            new_memory_ref(any_memory(vec![Value::StructRef(0)])),
        ))));
    }

    #[test]
    fn resolved_deeply_resolves_native_array_carrier() {
        // The transitional native-array carrier (`Value::ExprArgs`).
        assert_resolved_deeply(&array_value(ArrayValue::from_memory(
            any_memory(vec![Value::StructRef(0), Value::StructRef(1)]),
            vec![2],
        )));
    }

    #[test]
    fn resolved_deeply_resolves_dict_shaped_struct() {
        use super::super::value::StructInstance;
        // Pure-Julia Dict{K,V} is a Struct whose `vals` field is a Memory;
        // heap-struct values are stored as StructRef inside it (Issue #4774).
        assert_resolved_deeply(&Value::Struct(StructInstance::with_name(
            4,
            "Dict{Int64, Pair{Int64, Int64}}".to_string(),
            vec![
                Value::Memory(new_memory_ref(any_memory(vec![Value::StructRef(0)]))),
                Value::I64(1),
            ],
        )));
    }

    #[test]
    fn resolved_new_borrows_when_no_structref_present() {
        let heap = resolved_test_heap();
        let v = Value::str_new("plain");
        let resolved = Resolved::new(&v, &heap);
        assert!(
            matches!(resolved.0, std::borrow::Cow::Borrowed(_)),
            "StructRef-free values must not be cloned by Resolved::new"
        );
        assert_eq!(format_value_impl(resolved.value()), "plain");
    }

    #[test]
    fn resolved_trivial_accepts_structref_free_value() {
        let v = Value::I64(42);
        let resolved = Resolved::trivial(&v);
        assert_eq!(format_value_impl(resolved.value()), "42");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Resolved::trivial() called on a value containing Value::StructRef")]
    fn resolved_trivial_rejects_structref_in_debug() {
        let v = Value::StructRef(0);
        let _ = Resolved::trivial(&v);
    }

    #[test]
    fn resolved_ffi_placeholder_keeps_value_unresolved() {
        // The FFI layer has no struct_heap by design; the constructor must
        // pass the value through untouched (placeholder rendering happens in
        // the FFI formatter itself).
        let v = Value::StructRef(5);
        let resolved = Resolved::assume_ffi_placeholder(&v);
        assert!(value_contains_struct_ref(resolved.value()));
        assert!(matches!(resolved.value(), Value::StructRef(5)));
    }
}
