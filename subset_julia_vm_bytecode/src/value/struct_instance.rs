//! StructInstance - User-defined struct instances.
//!
//! This module contains the `StructInstance` struct for representing
//! user-defined struct instances, including Complex numbers.

use std::rc::Rc;

use super::super::error::VmError;
use super::array_element::{
    array_element_type_to_julia_type, julia_array_type_for_ndims, ArrayElementType,
};
use super::Value;
use crate::field_indices::{RATIONAL_DENOMINATOR_FIELD_INDEX, RATIONAL_NUMERATOR_FIELD_INDEX};
use subset_julia_vm_types::types::JuliaType;

/// Well-known struct name for Complex numbers
/// Complex struct name matching Julia's Complex{Float64} type
pub const COMPLEX_STRUCT_NAME: &str = "Complex";

/// Well-known struct name for Rational numbers
/// Rational struct name matching Julia's `Rational{T<:Integer}` type
pub const RATIONAL_STRUCT_NAME: &str = "Rational";

/// Well-known struct name for Irrational singleton constants.
pub const IRRATIONAL_STRUCT_NAME: &str = "Irrational";

/// Whether a struct type name denotes `Complex` — i.e. the bare `"Complex"` or
/// a parametric variant like `"Complex{Float64}"`.
///
/// Issue #5153: single source for the Complex type-name test so that the
/// definition-name / `JuliaType::Struct(name)` call sites (which work on a
/// `&str`, not a [`StructInstance`]) share the exact same predicate as
/// [`StructInstance::is_complex`], instead of re-spelling
/// `name == "Complex" || name.starts_with("Complex{")` inline.
#[inline]
pub fn is_complex_type_name(name: &str) -> bool {
    name == COMPLEX_STRUCT_NAME || name.starts_with(&format!("{}{{", COMPLEX_STRUCT_NAME))
}

/// Whether a struct type name denotes `Rational` — i.e. the bare `"Rational"`
/// or a parametric variant like `"Rational{Int64}"`.
///
/// Issue #5151/#5160: the `&str`-level analogue of [`is_complex_type_name`],
/// shared by the definition-name / `JuliaType::Struct(name)` call sites and by
/// [`StructInstance::is_rational`].
#[inline]
pub fn is_rational_type_name(name: &str) -> bool {
    name == RATIONAL_STRUCT_NAME || name.starts_with(&format!("{}{{", RATIONAL_STRUCT_NAME))
}

/// Whether a struct type name denotes `Irrational{sym}`.
#[inline]
pub fn is_irrational_type_name(name: &str) -> bool {
    name == IRRATIONAL_STRUCT_NAME || name.starts_with(&format!("{}{{", IRRATIONAL_STRUCT_NAME))
}

/// Struct instance value
#[derive(Debug, Clone)]
pub struct StructInstance {
    /// Index into the struct_defs table identifying the type
    pub type_id: usize,
    /// Name of the struct type (e.g., "Point", "Vector3D").
    ///
    /// Stored as `Rc<str>` (Issue #9125): the name is immutable once
    /// constructed, so sharing it via reference-count keeps `StructInstance`
    /// at 48 bytes (fat pointer = 16 B, same as `Box<str>`) while making
    /// `Clone` a refcount bump rather than a heap allocation.  This is the
    /// primary allocation win for Complex/Rational arithmetic hot paths where
    /// loading a struct from a slot clones the entire `StructInstance`; with
    /// a shared name the clone cost drops from 2 allocs to 1 (only the
    /// field-`Vec` data remains heap-resident per instance).
    ///
    /// Previously `Box<str>` (Issue #7976); `Rc<str>` derefs to `str`
    /// identically so all name predicates/display are unchanged.  Full
    /// elimination of this redundant-with-`type_id` field is a larger
    /// follow-up (needs name reconstruction from `struct_defs` at predicate
    /// sites that have no VM context).
    pub struct_name: Rc<str>,
    /// Field values in definition order
    pub values: Vec<Value>,
}

thread_local! {
    /// `type_id -> struct name`, mirroring the VM's `struct_defs` order (Issue
    /// #9198 S4). Populated by the VM (`sync_struct_name_registry`) so that
    /// values reconstructed from unboxed array storage in this crate — which
    /// has no access to the VM's `struct_defs` — can recover their concrete
    /// struct name for `show`/`typeof`. Thread-local per the single-threaded VM
    /// model, like the enum registry.
    static STRUCT_NAME_BY_TYPE_ID: std::cell::RefCell<Vec<Rc<str>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Replace the `type_id -> name` registry with `names` (indexed by `type_id`).
/// Called by the VM whenever its `struct_defs` are (re)installed.
pub fn set_struct_name_registry<I: IntoIterator<Item = String>>(names: I) {
    STRUCT_NAME_BY_TYPE_ID.with(|cell| {
        let mut v = cell.borrow_mut();
        v.clear();
        v.extend(names.into_iter().map(|n| Rc::from(n.as_str())));
    });
}

/// Resolve a struct name from `type_id` via the registry, or `None` if unknown.
pub fn struct_name_for_type_id(type_id: usize) -> Option<Rc<str>> {
    STRUCT_NAME_BY_TYPE_ID.with(|cell| cell.borrow().get(type_id).cloned())
}

impl StructInstance {
    pub fn new(type_id: usize, values: Vec<Value>) -> Self {
        Self {
            type_id,
            struct_name: Rc::from(""),
            values,
        }
    }

    /// Reconstruct a struct instance from unboxed inline array/`Memory` storage
    /// (Issue #9198 S4), resolving the concrete struct name from the thread-local
    /// `type_id -> name` registry so `show`/`typeof` of the element match a
    /// heap-boxed struct of the same type. Falls back to an empty name (as
    /// [`StructInstance::new`]) when the registry has no entry.
    pub fn new_inline(type_id: usize, values: Vec<Value>) -> Self {
        Self {
            type_id,
            struct_name: struct_name_for_type_id(type_id).unwrap_or_else(|| Rc::from("")),
            values,
        }
    }

    /// Create a new struct instance with a named type
    pub fn with_name(type_id: usize, struct_name: String, values: Vec<Value>) -> Self {
        Self {
            type_id,
            struct_name: Rc::from(struct_name.as_str()),
            values,
        }
    }

    /// Create a Complex struct instance with specified type_id.
    ///
    /// Uses a thread-local `Rc<str>` for the bare `"Complex"` name so that
    /// repeated constructions in the same thread incur zero heap allocations
    /// for the name (Issue #9125).
    pub fn complex(type_id: usize, re: f64, im: f64) -> Self {
        thread_local! {
            static COMPLEX_NAME: Rc<str> = Rc::from(COMPLEX_STRUCT_NAME);
        }
        Self {
            type_id,
            struct_name: COMPLEX_NAME.with(Rc::clone),
            values: vec![Value::F64(re), Value::F64(im)],
        }
    }

    /// Create a Complex struct instance re-using an already-interned name
    /// `Rc<str>`.  The caller is responsible for ensuring `struct_name` is a
    /// valid Complex type name (Issue #9125 fast path).
    #[inline]
    pub fn complex_with_shared_name(
        type_id: usize,
        struct_name: Rc<str>,
        re: f64,
        im: f64,
    ) -> Self {
        Self {
            type_id,
            struct_name,
            values: vec![Value::F64(re), Value::F64(im)],
        }
    }

    /// Create a Complex struct instance with specified type_id
    /// Note: type_id must be looked up from struct_table at runtime
    pub fn new_complex(type_id: usize, re: f64, im: f64) -> Self {
        Self::complex(type_id, re, im)
    }

    /// Reconstruct a Complex struct instance from interleaved array storage.
    ///
    /// Issue #5152: the interleaved (re, im) array storage previously hardcoded
    /// the `"Complex{Float64}"` / `"Complex{Float32}"` struct-name string literals
    /// at every read site (`get_linear_value`, `pop`, ...). The element-type tag
    /// already knows its own Julia type name, so the name is now derived from a
    /// single runtime lookup (`ArrayElementType::julia_type_name()`) rather than
    /// being duplicated as a string literal at each call site. `re`/`im` are the
    /// already-typed field `Value`s (`F64`/`F32`) read straight out of the backing
    /// buffer, so the reconstructed instance is bit-for-bit identical to the
    /// previous code — only the source of the name string changes.
    pub fn complex_from_storage(type_id: usize, struct_name: String, re: Value, im: Value) -> Self {
        Self {
            type_id,
            struct_name: Rc::from(struct_name.as_str()),
            values: vec![re, im],
        }
    }

    /// Check if this is a Complex struct
    #[inline]
    pub fn is_complex(&self) -> bool {
        is_complex_type_name(&self.struct_name)
    }

    /// Extract (re, im) from a Complex struct
    /// Returns None if not a Complex struct or fields are wrong type
    #[inline]
    pub fn as_complex_parts(&self) -> Option<(f64, f64)> {
        if !self.is_complex() || self.values.len() != 2 {
            return None;
        }
        Some((
            complex_part_value_to_f64(&self.values[0])?,
            complex_part_value_to_f64(&self.values[1])?,
        ))
    }

    /// Extract `(re, im)` from a Complex struct **only when both fields are
    /// genuinely `Value::F64`** (Issue #9167).
    ///
    /// Unlike [`Self::as_complex_parts`], this does NOT widen `I64`/`Bool`/`F32`/
    /// `F16` fields. It backs the `Complex{Float64}` scalar-arithmetic fast paths
    /// (`+ - * / ==` in `binary_both.rs`, `^` in `dynamic_ops`), which compute in
    /// `f64` and re-tag the result with the operand's `struct_name`/`type_id`.
    /// Firing those on a `Complex{Int}`/`{Bool}`/`{Float32}` operand would build a
    /// value with `F64` fields still labelled `Complex{Int}` — a tag/payload
    /// mismatch that later trips "expected I64, got Float64" (e.g.
    /// `(2+3im)^2 == -5+12im`). Returning `None` here lets every non-`Float64`
    /// component type fall through to the correct pure-Julia dispatch, which
    /// preserves the integer/Bool/Float32 component type. `as_complex_parts`
    /// itself stays permissive for the array/setindex/FFI paths (#5358).
    #[inline]
    pub fn complex_f64_parts(&self) -> Option<(f64, f64)> {
        if !self.is_complex() || self.values.len() != 2 {
            return None;
        }
        match (&self.values[0], &self.values[1]) {
            (Value::F64(re), Value::F64(im)) => Some((*re, *im)),
            _ => None,
        }
    }

    /// Check if this is a Rational struct.
    ///
    /// Issue #5160: centralizes the `"Rational"` / `"Rational{...}"` name test
    /// that was previously duplicated inline across the VM conversion paths
    /// (`exec/conversion.rs`, `stack_ops.rs`), mirroring [`is_complex`].
    ///
    /// [`is_complex`]: Self::is_complex
    #[inline]
    pub fn is_rational(&self) -> bool {
        is_rational_type_name(&self.struct_name)
    }

    /// Extract `(num, den)` from a Rational struct as `i64`.
    ///
    /// Issue #5160: single source for the numerator/denominator extraction the
    /// VM previously duplicated (with slightly diverging type coverage) in
    /// `exec/conversion.rs` and `stack_ops.rs`. Supports the integer field
    /// representations the VM stores for `Rational{Int64/Int32/Int16/Int8/Bool}`.
    /// Returns `None` for non-Rational structs or unsupported field types
    /// (e.g. `BigInt`, which flows through the pure-Julia `Rational{BigInt}`
    /// method specializations rather than these Rust fast paths).
    pub fn as_rational_parts_i64(&self) -> Option<(i64, i64)> {
        if !self.is_rational() {
            return None;
        }
        let num = rational_part_value_to_i64(self.values.get(RATIONAL_NUMERATOR_FIELD_INDEX)?)?;
        let den = rational_part_value_to_i64(self.values.get(RATIONAL_DENOMINATOR_FIELD_INDEX)?)?;
        Some((num, den))
    }

    /// Extract `(num, den)` from a Rational struct as `f64`.
    /// See [`as_rational_parts_i64`](Self::as_rational_parts_i64) for the
    /// supported field representations.
    pub fn as_rational_parts_f64(&self) -> Option<(f64, f64)> {
        let (num, den) = self.as_rational_parts_i64()?;
        Some((num as f64, den as f64))
    }

    /// Return the symbol parameter for supported zero-field Irrational singletons.
    pub fn irrational_symbol(&self) -> Option<&str> {
        if !is_irrational_type_name(&self.struct_name) || !self.values.is_empty() {
            return None;
        }
        self.struct_name
            .strip_prefix("Irrational{:")
            .and_then(|rest| rest.strip_suffix('}'))
    }

    /// Convert supported Irrational singletons to their Float64 value.
    pub fn as_irrational_f64(&self) -> Option<f64> {
        match self.irrational_symbol()? {
            "π" => Some(std::f64::consts::PI),
            "ℯ" => Some(std::f64::consts::E),
            _ => None,
        }
    }

    /// Return a high-precision decimal expansion for BigFloat conversion.
    pub fn irrational_decimal(&self) -> Option<&'static str> {
        match self.irrational_symbol()? {
            "π" => Some(
                "3.141592653589793238462643383279502884197169399375105820974944592307816406286198",
            ),
            "ℯ" => Some(
                "2.718281828459045235360287471352662497757247093699959574966967627724076630353547",
            ),
            _ => None,
        }
    }

    /// Recover the public `Array{T,N}` projection for the Pure Julia
    /// `Array{T,N}` wrapper.
    pub fn array_wrapper_julia_type(&self) -> Option<JuliaType> {
        if !is_array_wrapper_name(&self.struct_name) {
            return None;
        }
        let elem_type = self
            .values
            .first()
            .and_then(array_wrapper_memory_element_type)
            .or_else(|| array_wrapper_element_type(&self.struct_name))?;
        let ndims = array_wrapper_ndims(self.values.get(1)?)?;
        Some(julia_array_type_for_ndims(elem_type, ndims))
    }

    /// Raw `(ArrayElementType, ndims)` of an array wrapper's storage, when the
    /// wrapper is `Memory`/`MemoryRef`-backed. Unlike [`array_wrapper_julia_type`]
    /// (which maps the element tag to a `JuliaType` registry-free and so reports
    /// `Any` for a `StructOf` user-struct eltype), this preserves the `StructOf`
    /// tag so a `struct_defs`-aware caller can resolve it to the struct name
    /// (Issue #7304). Returns `None` for non-wrapper / legacy-carrier storage so
    /// callers fall back to [`array_wrapper_julia_type`].
    pub fn array_wrapper_element_array_type(&self) -> Option<(ArrayElementType, usize)> {
        if !is_array_wrapper_name(&self.struct_name) {
            return None;
        }
        let elem_type = match self.values.first()? {
            Value::MemoryRef(memref) => memref.element_type(),
            Value::Memory(mem) => mem.borrow().element_type().clone(),
            _ => return None,
        };
        let ndims = array_wrapper_ndims(self.values.get(1)?)?;
        Some((elem_type, ndims))
    }

    #[inline]
    pub fn get_field(&self, index: usize) -> Option<&Value> {
        self.values.get(index)
    }

    #[inline]
    pub fn set_field(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        if index < self.values.len() {
            self.values[index] = value;
            Ok(())
        } else {
            Err(VmError::FieldIndexOutOfBounds {
                index,
                field_count: self.values.len(),
            })
        }
    }
}

fn complex_part_value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::F64(v) => Some(*v),
        // Issue #5358: `Complex{Float32}` (and `Float16`) values carry F32/F16
        // fields. Without these arms `as_complex_parts` returned None for any
        // ComplexF32 value, breaking `ComplexF32` array `setindex!`
        // (`as_complex_parts` -> "Invalid Complex struct for IndexStore") and
        // silently degrading ComplexF32 arithmetic / matmul / ffi paths that
        // fall back to 0 / NaN on None.
        Value::F32(v) => Some(*v as f64),
        Value::F16(v) => Some(v.to_f64()),
        Value::I64(v) => Some(*v as f64),
        Value::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Read a single Rational numerator/denominator field as `i64`.
///
/// Issue #5160: matches the integer field representations the VM stores for
/// `Rational{Int64/Int32/Int16/Int8/Bool}`. Returns `None` for unsupported
/// field types (e.g. `BigInt`).
fn rational_part_value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::I64(v) => Some(*v),
        Value::I32(v) => Some(*v as i64),
        Value::I16(v) => Some(*v as i64),
        Value::I8(v) => Some(*v as i64),
        Value::Bool(v) => Some(if *v { 1 } else { 0 }),
        _ => None,
    }
}

fn array_wrapper_element_type(struct_name: &str) -> Option<JuliaType> {
    let (base, params) = split_top_level_params(struct_name)?;
    let base = match base.rsplit_once('.') {
        // A user module's qualified `Faux.Array{..}` is not the wrapper
        // (Issues #11388/#11395).
        Some((owner, leaf)) if super::array_wrapper::array_wrapper_owner_is_builtin(owner) => leaf,
        Some(_) => return None,
        None => base,
    };
    if base != "Array" || params.is_empty() {
        return None;
    }
    Some(JuliaType::from_name_or_struct(params[0]))
}

fn is_array_wrapper_name(struct_name: &str) -> bool {
    // Issue #6846: this is on the hot dynamic-dispatch path (`get_type_name`
    // calls `array_wrapper_julia_type` per dispatch of an array wrapper). Only
    // the *base* name (before `{`) is needed, so check it directly instead of
    // routing through `split_top_level_params`, which allocated a `Vec` of all
    // top-level params just to discard them.
    let Some(brace) = struct_name.find('{') else {
        return false;
    };
    if !struct_name.ends_with('}') {
        return false;
    }
    let base = &struct_name[..brace];
    match base.rsplit_once('.') {
        // A user module's qualified `Faux.Array{..}` is not the wrapper
        // (Issues #11388/#11395).
        Some((owner, leaf)) => {
            leaf == "Array" && super::array_wrapper::array_wrapper_owner_is_builtin(owner)
        }
        None => base == "Array",
    }
}

fn array_wrapper_memory_element_type(mem_value: &Value) -> Option<JuliaType> {
    // Issue #6846: map the storage's `ArrayElementType` straight to a
    // `JuliaType` instead of rendering it to a name string and re-parsing it via
    // `from_name_or_struct`. The direct mapping is byte-identical to the old
    // round-trip and removes two per-dispatch allocations (the element name
    // `String` and the parser's working buffers) from the array-wrapper
    // `get_type_name` path.
    if let Value::MemoryRef(memref) = mem_value {
        return Some(array_element_type_to_julia_type(&memref.element_type()));
    }
    if let Value::Memory(mem) = mem_value {
        return Some(array_element_type_to_julia_type(
            mem.borrow().element_type(),
        ));
    }
    // Issue #4340: during the Memory-backed Array migration, Pure Julia
    // Array wrappers may still carry the legacy Rust ArrayValue as `_mem`.
    // Use its logical element type so reshape preserves Complex arrays.
    if let Some(arr) = super::array_value::native_array_value_ref(mem_value) {
        let elem_name = arr.borrow().element_type().julia_type_name();
        return Some(JuliaType::from_name_or_struct(&elem_name));
    }
    None
}

fn array_wrapper_ndims(size_value: &Value) -> Option<usize> {
    match size_value {
        Value::Tuple(t) => match t.elements.first() {
            Some(Value::Tuple(dims)) => Some(dims.elements.len()),
            _ => Some(t.elements.len()),
        },
        Value::I64(_) => Some(1),
        _ => None,
    }
}

fn split_top_level_params(name: &str) -> Option<(&str, Vec<&str>)> {
    let brace_start = name.find('{')?;
    if !name.ends_with('}') {
        return None;
    }
    let base = &name[..brace_start];
    let inner = &name[brace_start + 1..name.len() - 1];
    let mut params = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let param = inner[start..i].trim();
                if !param.is_empty() {
                    params.push(param);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let last = inner[start..].trim();
    if !last.is_empty() {
        params.push(last);
    }

    Some((base, params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{native_array_value_from_array, ArrayElementType, ArrayValue, TupleValue};

    // ── constructors ──────────────────────────────────────────────────────────

    #[test]
    fn test_new_stores_type_id_and_values() {
        let s = StructInstance::new(5, vec![Value::I64(1), Value::I64(2)]);
        assert_eq!(s.type_id, 5);
        assert_eq!(s.values.len(), 2);
        assert!(
            s.struct_name.is_empty(),
            "new() should leave struct_name empty"
        );
    }

    #[test]
    fn test_with_name_stores_name() {
        let s = StructInstance::with_name(3, "Point".to_string(), vec![Value::F64(1.0)]);
        assert_eq!(&*s.struct_name, "Point");
        assert_eq!(s.type_id, 3);
    }

    #[test]
    fn test_complex_constructor_sets_name_and_values() {
        let c = StructInstance::complex(0, 3.0, 4.0);
        assert_eq!(&*c.struct_name, COMPLEX_STRUCT_NAME);
        assert_eq!(c.values.len(), 2);
        assert!(matches!(c.values[0], Value::F64(re) if (re - 3.0).abs() < 1e-15));
        assert!(matches!(c.values[1], Value::F64(im) if (im - 4.0).abs() < 1e-15));
    }

    // ── complex_from_storage (Issue #5152) ────────────────────────────────────

    #[test]
    fn test_complex_from_storage_f64_matches_hardcoded_construction() {
        // Issue #5152: deriving the struct name from the element type's
        // `julia_type_name()` must produce the exact same instance the read
        // sites used to build with a hardcoded "Complex{Float64}" literal.
        let name = ArrayElementType::ComplexF64.julia_type_name();
        let from_storage =
            StructInstance::complex_from_storage(7, name, Value::F64(3.0), Value::F64(-4.0));
        let hardcoded = StructInstance {
            type_id: 7,
            struct_name: "Complex{Float64}".into(),
            values: vec![Value::F64(3.0), Value::F64(-4.0)],
        };
        assert_eq!(from_storage.type_id, hardcoded.type_id);
        assert_eq!(from_storage.struct_name, hardcoded.struct_name);
        assert_eq!(&*from_storage.struct_name, "Complex{Float64}");
        assert!(from_storage.is_complex());
        assert!(matches!(from_storage.values[0], Value::F64(re) if (re - 3.0).abs() < 1e-15));
        assert!(matches!(from_storage.values[1], Value::F64(im) if (im + 4.0).abs() < 1e-15));
    }

    #[test]
    fn test_complex_from_storage_f32_matches_hardcoded_construction() {
        let name = ArrayElementType::ComplexF32.julia_type_name();
        let from_storage =
            StructInstance::complex_from_storage(2, name, Value::F32(1.0), Value::F32(2.0));
        assert_eq!(from_storage.type_id, 2);
        assert_eq!(&*from_storage.struct_name, "Complex{Float32}");
        assert!(from_storage.is_complex());
        assert!(matches!(from_storage.values[0], Value::F32(re) if (re - 1.0).abs() < 1e-6));
        assert!(matches!(from_storage.values[1], Value::F32(im) if (im - 2.0).abs() < 1e-6));
    }

    // ── is_complex ────────────────────────────────────────────────────────────

    #[test]
    fn test_is_complex_for_exact_name() {
        let c = StructInstance::complex(0, 1.0, 2.0);
        assert!(
            c.is_complex(),
            "\"Complex\" struct should be recognised as complex"
        );
    }

    #[test]
    fn test_is_complex_for_parametric_name() {
        let c = StructInstance::with_name(0, "Complex{Float64}".to_string(), vec![]);
        assert!(
            c.is_complex(),
            "\"Complex{{Float64}}\" should be recognised as complex"
        );
    }

    #[test]
    fn test_is_complex_returns_false_for_other_structs() {
        let s = StructInstance::with_name(0, "Point".to_string(), vec![]);
        assert!(!s.is_complex(), "\"Point\" struct should not be complex");
    }

    // ── as_complex_parts ──────────────────────────────────────────────────────

    #[test]
    fn test_as_complex_parts_f64_f64() {
        let c = StructInstance::complex(0, 3.0, -1.5);
        let Some((re, im)) = c.as_complex_parts() else {
            panic!("as_complex_parts should return Some for valid Complex");
        };
        assert!((re - 3.0).abs() < 1e-15);
        assert!((im - (-1.5)).abs() < 1e-15);
    }

    #[test]
    fn test_as_complex_parts_i64_i64_converted_to_f64() {
        let c =
            StructInstance::with_name(0, "Complex".to_string(), vec![Value::I64(2), Value::I64(3)]);
        let Some((re, im)) = c.as_complex_parts() else {
            panic!("as_complex_parts should return Some for Complex{{Int64}}");
        };
        assert!((re - 2.0).abs() < 1e-15);
        assert!((im - 3.0).abs() < 1e-15);
    }

    #[test]
    fn test_as_complex_parts_bool_bool_converted_to_f64() {
        let c = StructInstance::with_name(
            0,
            "Complex{Bool}".to_string(),
            vec![Value::Bool(false), Value::Bool(true)],
        );
        let Some((re, im)) = c.as_complex_parts() else {
            panic!("as_complex_parts should return Some for Complex{{Bool}}");
        };
        assert!((re - 0.0).abs() < 1e-15);
        assert!((im - 1.0).abs() < 1e-15);
    }

    #[test]
    fn test_as_complex_parts_f32_fields_issue_5358() {
        // Issue #5358: Complex{Float32} values carry F32 fields; extraction
        // must succeed (previously returned None, breaking ComplexF32 setindex).
        let c = StructInstance::with_name(
            0,
            "Complex{Float32}".to_string(),
            vec![Value::F32(7.0), Value::F32(8.0)],
        );
        let Some((re, im)) = c.as_complex_parts() else {
            panic!("as_complex_parts should return Some for Complex{{Float32}}");
        };
        assert!((re - 7.0).abs() < 1e-6);
        assert!((im - 8.0).abs() < 1e-6);
    }

    #[test]
    fn test_as_complex_parts_returns_none_for_non_complex() {
        let s = StructInstance::with_name(
            0,
            "Point".to_string(),
            vec![Value::F64(1.0), Value::F64(2.0)],
        );
        assert!(
            s.as_complex_parts().is_none(),
            "non-Complex struct should return None"
        );
    }

    // ── is_rational (Issue #5160) ─────────────────────────────────────────────

    #[test]
    fn test_is_rational_for_exact_name() {
        let r = StructInstance::with_name(
            0,
            "Rational".to_string(),
            vec![Value::I64(1), Value::I64(2)],
        );
        assert!(r.is_rational(), "\"Rational\" struct should be rational");
    }

    #[test]
    fn test_is_rational_for_parametric_name() {
        let r = StructInstance::with_name(
            0,
            "Rational{Int64}".to_string(),
            vec![Value::I64(1), Value::I64(2)],
        );
        assert!(
            r.is_rational(),
            "\"Rational{{Int64}}\" should be recognised as rational"
        );
    }

    #[test]
    fn test_is_rational_returns_false_for_other_structs() {
        let s = StructInstance::with_name(0, "Point".to_string(), vec![]);
        assert!(!s.is_rational(), "\"Point\" struct should not be rational");
        let c = StructInstance::complex(0, 1.0, 2.0);
        assert!(!c.is_rational(), "Complex struct should not be rational");
    }

    // ── as_rational_parts_i64 / as_rational_parts_f64 (Issue #5160) ────────────

    #[test]
    fn test_as_rational_parts_i64_int64() {
        let r = StructInstance::with_name(
            0,
            "Rational{Int64}".to_string(),
            vec![Value::I64(3), Value::I64(4)],
        );
        assert_eq!(r.as_rational_parts_i64(), Some((3, 4)));
        let Some((num, den)) = r.as_rational_parts_f64() else {
            panic!("as_rational_parts_f64 should return Some");
        };
        assert!((num - 3.0).abs() < 1e-15);
        assert!((den - 4.0).abs() < 1e-15);
    }

    #[test]
    fn test_as_rational_parts_i64_small_int_and_bool_fields() {
        // Rational{Int32/Int16/Int8/Bool} field representations all extract.
        let r32 = StructInstance::with_name(
            0,
            "Rational{Int32}".to_string(),
            vec![Value::I32(-6), Value::I32(8)],
        );
        assert_eq!(r32.as_rational_parts_i64(), Some((-6, 8)));
        let r8 = StructInstance::with_name(
            0,
            "Rational{Int8}".to_string(),
            vec![Value::I8(1), Value::I8(2)],
        );
        assert_eq!(r8.as_rational_parts_i64(), Some((1, 2)));
        let rbool = StructInstance::with_name(
            0,
            "Rational{Bool}".to_string(),
            vec![Value::Bool(true), Value::Bool(true)],
        );
        assert_eq!(rbool.as_rational_parts_i64(), Some((1, 1)));
    }

    #[test]
    fn test_as_rational_parts_returns_none_for_non_rational() {
        let s =
            StructInstance::with_name(0, "Point".to_string(), vec![Value::I64(1), Value::I64(2)]);
        assert!(s.as_rational_parts_i64().is_none());
        assert!(s.as_rational_parts_f64().is_none());
    }

    #[test]
    fn test_as_rational_parts_returns_none_for_unsupported_field_type() {
        // BigInt fields flow through the pure-Julia Rational{BigInt} methods,
        // not these Rust fast paths, so extraction returns None.
        let r = StructInstance::with_name(
            0,
            "Rational{BigInt}".to_string(),
            vec![Value::F64(1.0), Value::F64(2.0)],
        );
        assert!(r.as_rational_parts_i64().is_none());
    }

    // ── get_field / set_field ─────────────────────────────────────────────────

    #[test]
    fn test_get_field_valid_index() {
        let s = StructInstance::new(0, vec![Value::I64(42), Value::Bool(true)]);
        assert!(matches!(s.get_field(0), Some(Value::I64(42))));
        assert!(matches!(s.get_field(1), Some(Value::Bool(true))));
    }

    #[test]
    fn test_get_field_out_of_bounds_returns_none() {
        let s = StructInstance::new(0, vec![Value::I64(1)]);
        assert!(
            s.get_field(5).is_none(),
            "out-of-bounds index should return None"
        );
    }

    #[test]
    fn test_set_field_valid_index_updates_value() {
        let mut s = StructInstance::new(0, vec![Value::I64(0)]);
        let result = s.set_field(0, Value::I64(99));
        assert!(result.is_ok(), "set_field on valid index should succeed");
        assert!(matches!(s.values[0], Value::I64(99)));
    }

    #[test]
    fn test_set_field_out_of_bounds_returns_error() {
        let mut s = StructInstance::new(0, vec![Value::I64(0)]);
        let result = s.set_field(10, Value::I64(99));
        assert!(
            result.is_err(),
            "set_field on out-of-bounds index should fail"
        );
    }

    #[test]
    fn array_wrapper_julia_type_uses_native_array_mem_element_type_issue_4340() {
        let mem = native_array_value_from_array(ArrayValue::memory_first_undef(
            &ArrayElementType::ComplexF64,
            vec![4],
        ));
        let size = Value::Tuple(TupleValue::new(vec![Value::I64(2), Value::I64(2)]));
        let wrapper = StructInstance::with_name(0, "Array{Float64}".to_string(), vec![mem, size]);

        assert_eq!(
            wrapper.array_wrapper_julia_type(),
            Some(JuliaType::MatrixOf(Box::new(JuliaType::Struct(
                "Complex{Float64}".to_string()
            ))))
        );
    }
}
