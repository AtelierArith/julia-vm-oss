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
use super::util::{expr_to_julia_string, format_sprintf, format_value, Resolved};
use super::value::{
    array_wrapper_value_to_array_value, is_array_wrapper_struct_name as is_array_wrapper_name,
    native_array_ref_value as array_value, new_array_ref, ArrayElementType, ArrayRef, ArrayValue,
    MemoryValue, StructInstance, TupleValue, Value,
};
use super::Vm;

fn char_value_to_char(value: Value) -> Result<char, VmError> {
    match value {
        Value::Char(c) => Ok(c),
        // Error-message path. `other` here is a non-Char array element being
        // rejected; any StructRef would have been peeled by the caller's match,
        // so `Resolved::trivial` encodes (and debug-asserts) that invariant
        // (Issue #8642). No `struct_heap` is threaded into this free fn.
        other => Err(VmError::TypeError(format!(
            "String: expected Vector{{Char}}, got array containing {}",
            format_value(&Resolved::trivial(&other))
        ))),
    }
}

fn value_to_u8(value: Value) -> Result<u8, VmError> {
    match value {
        Value::U8(byte) => Ok(byte),
        other => Err(VmError::TypeError(format!(
            "String: expected Vector{{UInt8}}, got array containing {}",
            format_value(&Resolved::trivial(&other))
        ))),
    }
}

/// Issue #4749: escape a string for inclusion inside `"..."` so the
/// result round-trips through `Meta.parse`. Mirrors the Pure Julia
/// `Base.escape_string` helper in `julia/base/strings/util.jl`.
fn escape_for_repr_str(s: &str) -> String {
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

fn escape_byte_as_hex(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push_str("\\x");
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
}

fn escape_for_repr_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0 => out.push_str("\\0"),
            0x20..=0x7e => out.push(byte as char),
            other => escape_byte_as_hex(&mut out, other),
        }
    }
    out
}

/// Issue #4749: escape a char for inclusion inside `'...'`. Same
/// table as `escape_for_repr_str` plus `'` itself.
fn escape_for_repr_char(c: char) -> String {
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

fn array_chars_to_string(arr: &ArrayValue) -> Result<String, VmError> {
    let mut result = String::with_capacity(arr.element_count());
    for idx in 0..arr.element_count() {
        result.push(char_value_to_char(arr.get_linear(idx)?)?);
    }
    Ok(result)
}

fn memory_chars_to_string(mem: &MemoryValue) -> Result<String, VmError> {
    let mut result = String::with_capacity(mem.len());
    for idx in 0..mem.len() {
        result.push(char_value_to_char(mem.get(idx + 1)?)?);
    }
    Ok(result)
}

fn array_u8_to_bytes(arr: &ArrayValue) -> Result<Vec<u8>, VmError> {
    let mut result = Vec::with_capacity(arr.element_count());
    for idx in 0..arr.element_count() {
        result.push(value_to_u8(arr.get_linear(idx)?)?);
    }
    Ok(result)
}

fn memory_u8_to_bytes(mem: &MemoryValue) -> Result<Vec<u8>, VmError> {
    let mut result = Vec::with_capacity(mem.len());
    for idx in 0..mem.len() {
        result.push(value_to_u8(mem.get(idx + 1)?)?);
    }
    Ok(result)
}

fn try_u8_bytes_from_array_like(value: &Value) -> Result<Option<Vec<u8>>, VmError> {
    if let Some(arr) = native_array_ref_from_value(value) {
        let borrowed = arr.borrow();
        if borrowed.element_type() == ArrayElementType::U8 {
            return Ok(Some(array_u8_to_bytes(&borrowed)?));
        }
        return Ok(None);
    }
    if let Value::Memory(mem) = value {
        let borrowed = mem.borrow();
        if *borrowed.element_type() == ArrayElementType::U8 {
            return Ok(Some(memory_u8_to_bytes(&borrowed)?));
        }
    }
    Ok(None)
}

/// Try to decode a Vector{Char}-like value into a `String` by routing through
/// `ArrayValue::get_linear` for native Array carriers and `MemoryValue::get`
/// for Memory carriers. Returns `Ok(None)` for non array-like values so the
/// caller can fall through to other dispatch arms (e.g. the Pure Julia Array
/// wrapper bridge or the `Value::Str` identity case).
///
/// Centralizing the Array / Memory char dispatch behind one helper lets the
/// `String(::Vector{Char})` constructor and the Array-wrapper `_mem` reader
/// stop spelling the native array carrier directly while behavior is
/// preserved for reshape shared backing and Complex/struct-ref storage
/// (Issue #3908).
fn try_chars_to_string_from_array_like(value: &Value) -> Result<Option<String>, VmError> {
    if let Some(arr) = native_array_ref_from_value(value) {
        let borrowed = arr.borrow();
        return Ok(Some(array_chars_to_string(&borrowed)?));
    }
    if let Value::Memory(mem) = value {
        let borrowed = mem.borrow();
        return Ok(Some(memory_chars_to_string(&borrowed)?));
    }
    Ok(None)
}

/// File-local alias for the shared
/// [`super::value::native_array_value_ref`] destructure helper. Keeps the
/// existing call sites in this file (`Expr` splat handlers, `_mem`
/// readers, etc.) using the same local name (Issue #3908).
#[inline]
fn native_array_ref_from_value(value: &Value) -> Option<&ArrayRef> {
    super::value::native_array_value_ref(value)
}

fn array_wrapper_shape_and_offset(size: &Value) -> Result<(Vec<usize>, usize), VmError> {
    let Value::Tuple(size_tuple) = size else {
        return Err(VmError::TypeError(
            "String: Array wrapper _size must be Tuple".to_string(),
        ));
    };

    if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
        let shape = array_wrapper_shape_from_tuple(dims_tuple)?;
        let offset = match size_tuple.elements.get(1) {
            Some(Value::I64(i)) if *i >= 1 => usize::try_from(*i).map_err(|_| {
                VmError::TypeError(format!(
                    "String: Array wrapper offset must fit usize, got {i}"
                ))
            })?,
            Some(other) => {
                return Err(VmError::TypeError(format!(
                    "String: Array wrapper offset must be positive Int64, got {:?}",
                    other.value_type()
                )))
            }
            None => {
                return Err(VmError::TypeError(
                    "String: Array wrapper offset-encoded _size missing offset".to_string(),
                ))
            }
        };
        return Ok((shape, offset));
    }

    Ok((array_wrapper_shape_from_tuple(size_tuple)?, 1))
}

fn array_wrapper_shape_from_tuple(dims_tuple: &TupleValue) -> Result<Vec<usize>, VmError> {
    dims_tuple
        .elements
        .iter()
        .map(|dim| match dim {
            Value::I64(i) if *i >= 0 => usize::try_from(*i).map_err(|_| {
                VmError::TypeError(format!(
                    "String: Array wrapper dimension must fit usize, got {i}"
                ))
            }),
            other => Err(VmError::TypeError(format!(
                "String: Array wrapper dimensions must be non-negative Int64 values, got {:?}",
                other.value_type()
            ))),
        })
        .collect()
}

fn array_wrapper_chars_to_string(
    instance: &StructInstance,
    struct_heap: &[StructInstance],
) -> Result<Option<String>, VmError> {
    if !is_array_wrapper_name(&instance.struct_name) {
        return Ok(None);
    }

    let Some(mem) = instance.values.first() else {
        return Err(VmError::TypeError(
            "String: Array wrapper missing _mem field".to_string(),
        ));
    };
    let Some(size) = instance.values.get(1) else {
        return Err(VmError::TypeError(
            "String: Array wrapper missing _size field".to_string(),
        ));
    };
    let (shape, offset) = array_wrapper_shape_and_offset(size)?;
    let len: usize = shape.iter().product();

    let mut result = String::with_capacity(len);
    // Route both the Memory primitive and the transitional native Array
    // carrier through `ArrayValue::get_linear` / `MemoryValue::get` so the
    // Pure Julia `Array{Char}` wrapper boundary stops pattern-matching the
    // native array carrier directly while reshape offset semantics stay
    // correct (Issue #3908).
    if let Value::Memory(mem_ref) = mem {
        let mem_borrow = mem_ref.borrow();
        for linear in 0..len {
            result.push(char_value_to_char(mem_borrow.get(offset + linear)?)?);
        }
    } else if let Value::MemoryRef(memref) = mem {
        // Faithful `Array{Char,N}` (Issue #6648) stores `ref::MemoryRef`. The
        // reference's 1-based `memory_index()` already encodes its start offset
        // into the parent `Memory`, so read `len` elements from there (the size
        // field carries only the plain dims for this storage form, Issue #6663).
        let parent = memref.parent();
        let mem_borrow = parent.borrow();
        let base = memref.memory_index();
        for linear in 0..len {
            result.push(char_value_to_char(mem_borrow.get(base + linear)?)?);
        }
    } else if let Some(array_ref) = native_array_ref_from_value(mem) {
        let array_borrow = array_ref.borrow();
        for linear in 0..len {
            result.push(char_value_to_char(
                array_borrow.get_linear(offset - 1 + linear)?,
            )?);
        }
    } else if let Ok(arr) = super::builtins_linalg::linalg_value_to_array_value(
        mem.clone(),
        struct_heap,
        "String",
        None,
    ) {
        for linear in 0..len {
            result.push(char_value_to_char(arr.get_linear(offset - 1 + linear)?)?);
        }
    } else {
        return Err(VmError::TypeError(format!(
            "String: Array wrapper _mem must be Memory or Array, got {:?}",
            mem.value_type()
        )));
    }

    Ok(Some(result))
}

fn array_wrapper_u8_to_bytes(
    instance: &StructInstance,
    struct_heap: &[StructInstance],
) -> Result<Option<Vec<u8>>, VmError> {
    if !is_array_wrapper_name(&instance.struct_name) {
        return Ok(None);
    }

    let Some(mem) = instance.values.first() else {
        return Err(VmError::TypeError(
            "String: Array wrapper missing _mem field".to_string(),
        ));
    };
    let Some(size) = instance.values.get(1) else {
        return Err(VmError::TypeError(
            "String: Array wrapper missing _size field".to_string(),
        ));
    };
    let (shape, offset) = array_wrapper_shape_and_offset(size)?;
    let len: usize = shape.iter().product();
    let mut result = Vec::with_capacity(len);

    if let Value::Memory(mem_ref) = mem {
        let mem_borrow = mem_ref.borrow();
        if *mem_borrow.element_type() != ArrayElementType::U8 {
            return Ok(None);
        }
        for linear in 0..len {
            result.push(value_to_u8(mem_borrow.get(offset + linear)?)?);
        }
    } else if let Value::MemoryRef(memref) = mem {
        if memref.element_type() != ArrayElementType::U8 {
            return Ok(None);
        }
        let parent = memref.parent();
        let mem_borrow = parent.borrow();
        let base = memref.memory_index();
        for linear in 0..len {
            result.push(value_to_u8(mem_borrow.get(base + linear)?)?);
        }
    } else if let Some(array_ref) = native_array_ref_from_value(mem) {
        let array_borrow = array_ref.borrow();
        if array_borrow.element_type() != ArrayElementType::U8 {
            return Ok(None);
        }
        for linear in 0..len {
            result.push(value_to_u8(array_borrow.get_linear(offset - 1 + linear)?)?);
        }
    } else if let Ok(arr) = super::builtins_linalg::linalg_value_to_array_value(
        mem.clone(),
        struct_heap,
        "String",
        None,
    ) {
        if arr.element_type() != ArrayElementType::U8 {
            return Ok(None);
        }
        for linear in 0..len {
            result.push(value_to_u8(arr.get_linear(offset - 1 + linear)?)?);
        }
    } else {
        return Err(VmError::TypeError(format!(
            "String: Array wrapper _mem must be Memory or Array, got {:?}",
            mem.value_type()
        )));
    }

    Ok(Some(result))
}

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
                // Issue #4725: dereference Value::StructRef (top-level and
                // nested inside Tuples / NamedTuples / Ref / QuoteNode) so
                // heap-allocated structs like `Pair(1, 2)` get their proper
                // Julia format ("1 => 2") instead of leaking the Rust
                // `StructRef(heap_idx=N)` debug repr.
                //
                // Issue #4761: when a single struct arg has a user-defined
                // `show(io::IO, ::T)` method, route through that method via
                // a sprint-style IOBuffer so `string(x)` matches `repr(x)`
                // for the user-customized shape (e.g. `LinRange{Float64}(0.0,
                // 1.0, 5)` instead of the raw 4-field dump). Multi-arg
                // `string(x, y, ...)` keeps the existing per-value Rust
                // formatting so the join behavior is preserved.
                if argc == 1 {
                    let val = self.stack.pop_value()?;
                    let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                        &val,
                        &self.struct_heap,
                    );
                    if !matches!(resolved, Value::Expr(_) | Value::Symbol(_)) {
                        if let Some(func_index) = self.user_show_method_for_print(&resolved) {
                            let io = super::value::IOValue::buffer_ref();
                            self.start_sprint_call(func_index, io, vec![resolved])?;
                            return Ok(Some(()));
                        }
                        // Issue #7893: `string([x y; x x])` of an array whose
                        // struct elements have a registered `Base.show` renders
                        // each element via that method (upstream array `string`
                        // calls `show` per element).
                        if let Some(s) = self.render_array_via_user_show(&resolved) {
                            self.stack.push(Value::str_new(s));
                            return Ok(Some(()));
                        }
                    }
                    let s = match &resolved {
                        Value::Expr(e) => expr_to_julia_string(e),
                        Value::Symbol(s) => s.as_str().to_string(),
                        _ => {
                            crate::vm::formatting::format_value_print(&Resolved::trivial(&resolved))
                        }
                    };
                    self.stack.push(Value::str_new(s));
                } else {
                    let mut parts = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        let val = self.stack.pop_value()?;
                        let s = match &val {
                            // Expr: format as Julia code (no :() wrapper)
                            Value::Expr(e) => expr_to_julia_string(e),
                            // Symbol: format as name (no : prefix)
                            Value::Symbol(s) => s.as_str().to_string(),
                            // Other values: deep-resolve any StructRefs against
                            // the heap then use standard formatting.
                            _ => crate::vm::formatting::format_value_print(&Resolved::new(
                                &val,
                                &self.struct_heap,
                            )),
                        };
                        parts.push(s);
                    }
                    parts.reverse();
                    self.stack.push(Value::str_new(parts.join("")));
                }
            }

            BuiltinId::StringFromChars => {
                // String(bytes/chars) - construct string from Vector{UInt8}
                // preserving invalid UTF-8 bytes (Issue #8995), or from
                // Vector{Char} via the existing character path (Issue #2038).
                let val = self.stack.pop_value()?;
                if let Some(bytes) = try_u8_bytes_from_array_like(&val)? {
                    self.stack.push(Value::str_from_bytes(bytes));
                    return Ok(Some(()));
                }
                // Route native Array / Memory Char carriers through a single
                // shared helper that decodes each element via
                // `ArrayValue::get_linear` / `MemoryValue::get`, so this
                // constructor stops pattern-matching the native array carrier
                // directly while reshape shared backing stays correct
                // (Issue #3908).
                if let Some(s) = try_chars_to_string_from_array_like(&val)? {
                    self.stack.push(Value::str_new(s));
                    return Ok(Some(()));
                }
                let s = match &val {
                    Value::StructRef(idx) => {
                        let instance = self.struct_heap.get(*idx).ok_or_else(|| {
                            VmError::TypeError(format!(
                                "String: invalid StructRef({idx}) for Array wrapper conversion"
                            ))
                        })?;
                        if let Some(bytes) = array_wrapper_u8_to_bytes(instance, &self.struct_heap)?
                        {
                            self.stack.push(Value::str_from_bytes(bytes));
                            return Ok(Some(()));
                        }
                        if let Some(s) = array_wrapper_chars_to_string(instance, &self.struct_heap)?
                        {
                            s
                        } else {
                            // Error-message path (failed Vector{Char}->String
                            // conversion); `Resolved::new` deep-resolves any
                            // heap struct into the message (Issue #8642).
                            return Err(VmError::TypeError(format!(
                                "String: cannot convert {} to String",
                                format_value(&Resolved::new(&val, &self.struct_heap))
                            )));
                        }
                    }
                    Value::Struct(instance) => {
                        if let Some(bytes) = array_wrapper_u8_to_bytes(instance, &self.struct_heap)?
                        {
                            self.stack.push(Value::str_from_bytes(bytes));
                            return Ok(Some(()));
                        }
                        if let Some(s) = array_wrapper_chars_to_string(instance, &self.struct_heap)?
                        {
                            s
                        } else {
                            // Error-message path; `Resolved::new` deep-resolves
                            // any heap struct into the message (Issue #8642).
                            return Err(VmError::TypeError(format!(
                                "String: cannot convert {} to String",
                                format_value(&Resolved::new(&val, &self.struct_heap))
                            )));
                        }
                    }
                    Value::Str(s) => s.to_string(), // String(s) is identity for strings
                    Value::StrBytes(bytes) => {
                        self.stack.push(Value::StrBytes(bytes.clone()));
                        return Ok(Some(()));
                    }
                    _ => {
                        // Error-message path; `Resolved::new` deep-resolves any
                        // heap struct into the message (Issue #8642).
                        return Err(VmError::TypeError(format!(
                            "String: cannot convert {} to String",
                            format_value(&Resolved::new(&val, &self.struct_heap))
                        )));
                    }
                };
                self.stack.push(Value::str_new(s));
            }

            BuiltinId::Repr => {
                // repr(x) - return string representation with quotes for strings.
                // Issue #4725: same StructRef-resolution as `string(...)` so
                // `repr(Pair(1,2))` returns `"1 => 2"` instead of leaking
                // `StructRef(heap_idx=N)`.
                // Issue #4749: escape special chars in strings and chars so
                // the result round-trips through `Meta.parse` — wrap the
                // escaped form in `"..."` for strings and `'...'` for chars.
                let val = self.stack.pop_value()?;
                let s = match &val {
                    Value::Str(s) => format!("\"{}\"", escape_for_repr_str(s)),
                    Value::StrBytes(bytes) => format!("\"{}\"", escape_for_repr_bytes(bytes)),
                    Value::Char(c) => format!("'{}'", escape_for_repr_char(*c)),
                    // Malformed Char (Issue #8995): upstream prints each raw
                    // byte as a \xNN escape, e.g. repr('\xff') == "'\\xff'".
                    Value::CharMalformed(bits) => {
                        let (bytes, len) = crate::vm::value::julia_char_pattern_bytes(*bits);
                        let mut out = String::from("'");
                        for &b in &bytes[..len] {
                            escape_byte_as_hex(&mut out, b);
                        }
                        out.push('\'');
                        out
                    }
                    _ => {
                        let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                            &val,
                            &self.struct_heap,
                        );
                        // Issue #7893: `repr([x y; x x])` renders array struct
                        // elements via their registered `Base.show`.
                        self.render_value_via_user_show(&resolved)
                            .or_else(|| self.render_array_via_user_show(&resolved))
                            .unwrap_or_else(|| format_value(&Resolved::trivial(&resolved)))
                    }
                };
                self.stack.push(Value::str_new(s));
            }

            BuiltinId::Sprintf => {
                // sprintf(fmt, args...) - C-style formatted string.
                // Issue #4729 sweep (follows #4725 / #4727): pre-resolve each
                // arg through resolve_struct_refs_for_format so `%s` against
                // a heap-allocated struct like `Pair(1, 2)` renders as
                // `"1 => 2"` instead of leaking `StructRef(heap_idx=N)`.
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

                let resolved_args: Vec<Value> = values[1..]
                    .iter()
                    .map(|v| {
                        crate::vm::formatting::resolve_struct_refs_for_format(v, &self.struct_heap)
                    })
                    .collect();
                let result = format_sprintf(&fmt, &resolved_args);
                self.stack.push(Value::str_new(result));
            }

            BuiltinId::PrintfFmtFloat => {
                // _printf_fmt_float(x, conv::Char, precision::Int) -> String
                // The C float→string boundary for the pure-Julia Printf engine
                // (Issue #6746). Args are pushed x, conv, precision (so popped in
                // reverse). precision < 0 means the C default (6).
                let prec = self.stack.pop_i64()?;
                let conv_v = self.stack.pop_value()?;
                let x_v = self.stack.pop_value()?;
                let conv = match conv_v {
                    Value::Char(c) => c,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_printf_fmt_float: conv must be a Char, got {:?}",
                            other.value_type()
                        )))
                    }
                };
                let x = match x_v {
                    Value::F64(x) => x,
                    Value::F32(x) => f64::from(x),
                    Value::F16(x) => f64::from(x),
                    Value::I64(n) => n as f64,
                    Value::I32(n) => f64::from(n),
                    Value::I16(n) => f64::from(n),
                    Value::I8(n) => f64::from(n),
                    Value::U64(n) => n as f64,
                    Value::U32(n) => f64::from(n),
                    Value::U16(n) => f64::from(n),
                    Value::U8(n) => f64::from(n),
                    Value::Bool(b) => f64::from(b),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_printf_fmt_float: x must be a real number, got {:?}",
                            other.value_type()
                        )))
                    }
                };
                let result = crate::vm::formatting::format_printf_float(x, conv, prec);
                self.stack.push(Value::str_new(result));
            }

            BuiltinId::Ncodeunits => {
                // ncodeunits(s) - number of code units (bytes for UTF-8)
                let val = self.stack.pop_value()?;
                let n = match &val {
                    Value::Char(c) => c.len_utf8(),
                    _ => {
                        let bytes = val.string_bytes().ok_or_else(|| {
                            VmError::TypeError(format!(
                                "ncodeunits: expected String or Char, got {:?}",
                                val.value_type()
                            ))
                        })?;
                        bytes.len()
                    }
                };
                self.stack.push(Value::I64(n as i64));
            }

            BuiltinId::Codeunit => {
                // codeunit(s, i) - get byte at position i (1-indexed)
                let i = self.stack.pop_i64()?;
                let val = self.stack.pop_value()?;
                let bytes = val.string_bytes().ok_or_else(|| {
                    VmError::TypeError(format!(
                        "codeunit: expected String, got {:?}",
                        val.value_type()
                    ))
                })?;
                if i < 1 || i as usize > bytes.len() {
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![i],
                        shape: vec![bytes.len()],
                    });
                }
                self.stack.push(Value::U8(bytes[(i - 1) as usize]));
            }

            BuiltinId::CodeUnits => {
                // codeunits(s) - get all bytes as Vector{UInt8}
                let val = self.stack.pop_value()?;
                let bytes: Vec<u8> = val
                    .string_bytes()
                    .ok_or_else(|| {
                        VmError::TypeError(format!(
                            "codeunits: expected String, got {:?}",
                            val.value_type()
                        ))
                    })?
                    .to_vec();
                let len = bytes.len();
                let arr = ArrayValue::memory_first_from_u8(bytes, vec![len]);
                self.stack.push(array_value(new_array_ref(arr)));
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
                        self.stack.push(Value::Bool(haystack.contains(&*s)));
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

            // BuiltinId::StringToInt / StringToFloat removed - now Pure Julia
            // (base/parse.jl, Issue #6748). parse(Float64,s) = pure wrapper over
            // the _tryparse_float64 intrinsic (TryparseFloat64) + ArgumentError.
            // BuiltinId::StringToIntBase removed - now Pure Julia (base/parse.jl,
            // Issue #7875). `parse(Int, s; base=N)` is rewritten by the compiler
            // (compile_parse_tryparse) to `_parse_int_base(s, base)`, which wraps
            // the existing pure-Julia `_tryparse_int` base-parsing logic. The
            // kwargs-dispatch limitation that originally kept this in Rust no
            // longer applies (the compiler extracts the base kwarg and emits a
            // positional call).
            BuiltinId::StringIntToBase => {
                // string(x; base=N[, pad=P]) - convert integer to string in given
                // base with optional zero-padding (Issue #2036, widened in Issue
                // #4723 to cover all integer widths, pad added in Issue #8884).
                // Stack order (top-to-bottom): pad, base, val. pad=0 means no
                // padding. Signed integer widths and F64 normalize to i128
                // (sign-preserving); unsigned widths and Bool normalize to u128
                // (no sign, full bit range), matching upstream Julia's
                // `string(n::Integer; base, pad)` which prints unsigned types
                // without a leading `-`.
                let pad = self.stack.pop_i64().unwrap_or(0);
                let base = self.stack.pop_i64()?;
                let val = self.stack.pop_value()?;
                let signed_n: Option<i128> = match &val {
                    Value::I8(n) => Some(i128::from(*n)),
                    Value::I16(n) => Some(i128::from(*n)),
                    Value::I32(n) => Some(i128::from(*n)),
                    Value::I64(n) => Some(i128::from(*n)),
                    Value::I128(n) => Some(*n),
                    Value::F64(f) => Some(*f as i128),
                    _ => None,
                };
                let unsigned_n: Option<u128> = match &val {
                    Value::U8(n) => Some(u128::from(*n)),
                    Value::U16(n) => Some(u128::from(*n)),
                    Value::U32(n) => Some(u128::from(*n)),
                    Value::U64(n) => Some(u128::from(*n)),
                    Value::U128(n) => Some(*n),
                    Value::Bool(b) => Some(u128::from(*b)),
                    _ => None,
                };
                if signed_n.is_none() && unsigned_n.is_none() {
                    // Error-message path (non-integer rejected for base
                    // conversion); `Resolved::new` deep-resolves any heap
                    // struct into the message (Issue #8642).
                    return Err(VmError::TypeError(format!(
                        "string: cannot convert {} to integer for base conversion",
                        format_value(&Resolved::new(&val, &self.struct_heap))
                    )));
                }

                if !(2..=36).contains(&base) {
                    return Err(VmError::TypeError(format!(
                        "string: base must be between 2 and 36, got {}",
                        base
                    )));
                }

                fn format_unsigned(num: u128, base: i64) -> String {
                    match base {
                        2 => format!("{:b}", num),
                        8 => format!("{:o}", num),
                        10 => format!("{}", num),
                        16 => format!("{:x}", num),
                        _ => {
                            let base_u = base as u128;
                            if num == 0 {
                                return "0".to_string();
                            }
                            let mut digits = Vec::with_capacity(129);
                            let mut n = num;
                            while n > 0 {
                                let d = (n % base_u) as u8;
                                digits.push(if d < 10 {
                                    (b'0' + d) as char
                                } else {
                                    (b'a' + d - 10) as char
                                });
                                n /= base_u;
                            }
                            digits.reverse();
                            digits.into_iter().collect()
                        }
                    }
                }

                let s = if let Some(num) = unsigned_n {
                    format_unsigned(num, base)
                } else {
                    let n = signed_n.unwrap_or(0);
                    let negative = n < 0;
                    let abs = n.unsigned_abs();
                    let body = format_unsigned(abs, base);
                    if negative {
                        format!("-{}", body)
                    } else {
                        body
                    }
                };
                // Apply zero-padding (pad > 0): left-pad with '0' to reach `pad` chars.
                // For signed negatives the sign is outside the padding (e.g. "-06").
                let s = if pad > 0 && (s.len() as i64) < pad {
                    let n_zeros = (pad as usize).saturating_sub(s.len());
                    format!("{}{}", "0".repeat(n_zeros), s)
                } else {
                    s
                };
                self.stack.push(Value::str_new(s));
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

            // BuiltinId::Codepoint removed - pure Julia (Issue #6747)
            BuiltinId::IntToChar => {
                // Char(n) - codepoint to char
                let n = self.stack.pop_i64()?;
                if !(0..=u32::MAX as i64).contains(&n) {
                    return Err(VmError::InexactError(format!("Char({})", n)));
                }
                match char::from_u32(n as u32) {
                    Some(c) => self.stack.push(Value::Char(c)),
                    None => {
                        return Err(VmError::TypeError(format!("Char: invalid codepoint {}", n)))
                    }
                }
            }

            // BuiltinId::Bitstring removed - pure Julia (Issue #6747)
            // BuiltinId::Ascii removed - now Pure Julia in base/strings/util.jl
            // BuiltinId::Nextind, Prevind, Thisind, Reverseind removed - now Pure Julia (base/strings/basic.jl)
            // BuiltinId::Bytes2Hex, Hex2Bytes removed - now Pure Julia (base/strings/util.jl)
            // UnescapeString removed - now Pure Julia (base/strings/util.jl, Issue #6724)
            // BuiltinId::Isnumeric removed - now Pure Julia (base/strings/unicode.jl,
            // Issue #6752). `isnumeric(c::Char)` binary-searches an embedded
            // Nd/Nl/No codepoint range table (generated from upstream utf8proc),
            // correct for non-ASCII numerics. Routed DispatchFirst to the method
            // table; the compiler no longer intercepts the name.
            BuiltinId::SubStringRetag => {
                // _substring_retag(v) — retag a Vector{String} as
                // Vector{SubString{String}} for display (Issue #3574).
                // The VM has no separate substring runtime type, so this only
                // changes the array's `element_type_override`. The same
                // `Rc<RefCell<ArrayValue>>` is pushed back so callers see the
                // mutation; this matches `split`'s "build then return" usage
                // where the caller uses the return value directly.
                let val = self.stack.pop_value()?;
                let retagged = if let Some(arr) = native_array_ref_from_value(&val).cloned() {
                    {
                        let mut borrow = arr.borrow_mut();
                        borrow.element_type_override =
                            Some(super::value::ArrayElementType::SubString);
                    }
                    array_value(arr)
                } else if let Some(mut arr) =
                    array_wrapper_value_to_array_value(&val, &self.struct_heap)?
                {
                    arr.element_type_override = Some(super::value::ArrayElementType::SubString);
                    self.array_wrapper_value(arr)?
                } else {
                    // Error-message path (non-Vector rejected); `Resolved::new`
                    // deep-resolves any heap struct into the message (Issue #8642).
                    return Err(VmError::TypeError(format!(
                        "_substring_retag: expected Vector, got {}",
                        format_value(&Resolved::new(&val, &self.struct_heap))
                    )));
                };
                self.stack.push(retagged);
            }

            BuiltinId::IsvalidIndex if argc == 1 => {
                // isvalid(x) one-arg form (Issue #8995): a String is valid iff
                // its bytes are valid UTF-8 (`Str` by construction, `StrBytes`
                // never), a `Char` is a Unicode scalar, and a malformed Char
                // is not.
                let val = self.stack.pop_value()?;
                let result = match &val {
                    Value::Str(_) => true,
                    Value::StrBytes(_) => false,
                    Value::Char(_) => true,
                    Value::CharMalformed(_) => false,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "isvalid: expected String or Char, got {:?}",
                            other.value_type()
                        )))
                    }
                };
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::IsvalidIndex => {
                // isvalid(s, i) - check if index is valid character boundary
                let i = self.stack.pop_i64()?;
                let val = self.stack.pop_value()?;
                let bytes = val.string_bytes().ok_or_else(|| {
                    VmError::TypeError(format!(
                        "isvalid: expected String, got {:?}",
                        val.value_type()
                    ))
                })?;
                // Julia uses 1-based indexing; negative or zero indices are invalid
                let result = if i <= 0 || i as usize > bytes.len() {
                    false
                } else {
                    !matches!(bytes[(i - 1) as usize], 0x80..=0xbf)
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
            // StringFindAll / StringCount removed - dead (findall/count on
            // String/Char patterns are pure Julia in base/strings/search.jl,
            // Issue #6724)
            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
