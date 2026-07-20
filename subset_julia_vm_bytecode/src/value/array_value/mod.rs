//! ArrayValue - N-dimensional array with type-segregated storage.
//!
//! This module contains the `ArrayValue` struct for representing Julia arrays
//! with efficient homogeneous storage using `ArrayData`.
//!
//! # Sub-modules
//!
//! - `access`: Element access, slicing, type-checked data accessors
//! - `mutation`: Element mutation, push/pop, insert/delete operations

// SAFETY: i64→usize casts for linear/multi-dimensional index computation are
// guarded by `index < 1 || index as usize > total_size` and `dim_idx < 1 || dim_idx as usize > shape[i]`.
#![allow(clippy::cast_sign_loss)]

mod access;
mod mutation;

pub use mutation::push_into_array_data;
pub(crate) use mutation::real_scalar_as_f64;

use std::cell::RefCell;
use std::rc::Rc;

use super::super::error::VmError;
use super::array_data::{ArrayData, BitPackedBoolData};
use super::array_element::ArrayElementType;
use super::memory_value::MemoryValue;
use super::StrRef;
use super::Value;

/// N-dimensional array value with type-segregated storage (column-major order like Julia)
/// Supports all Value types using ArrayData for efficient homogeneous storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArrayValue {
    /// Type-segregated storage for efficient operations
    pub data: ArrayData,
    /// Shape: [dim1, dim2, ...] for N-D arrays
    pub shape: Vec<usize>,
    /// Optional concrete struct type for StructRefs arrays
    pub struct_type_id: Option<usize>,
    /// Optional element type override (for complex arrays that use F32/F64 storage)
    /// When Some, this takes precedence over data.element_type()
    pub element_type_override: Option<ArrayElementType>,
    /// Optional container type override for Array-compatible wrappers.
    ///
    /// This is intentionally separate from `element_type_override`: `BitVector`
    /// still has `Bool` elements, but upstream Julia reports the container as a
    /// distinct `BitVector` type (Issue #5484).
    #[serde(default)]
    pub array_type_override: Option<String>,
    /// Storage owner for reshape-created arrays.
    ///
    /// Upstream Julia keeps reshaped Arrays structurally distinct while sharing
    /// `a.ref` (`julia/base/reshapedarray.jl`). This parent pointer is the
    /// legacy VM bridge until ArrayValue itself is backed by MemoryRef.
    #[serde(skip)]
    pub shared_parent: Option<ArrayRef>,
}

pub type ArrayRef = Rc<RefCell<ArrayValue>>;

pub fn new_array_ref(arr: ArrayValue) -> ArrayRef {
    Rc::new(RefCell::new(arr))
}

/// Witness newtype confining the `expr.args` carrier payload (Issue #8918).
///
/// The inner [`ArrayRef`] is a *private* field of this module, so a
/// `Value::ExprArgs(..)` value can only be **constructed**, and its inner
/// `ArrayRef` **destructured**, through the `native_array_*` hub helpers in this
/// file (the sole code with access to the private field). A new carrier site
/// anywhere else — an `ExprArgs` construction or an `ExprArgs(ExprArgsCarrier(x))`
/// destructure outside the hub — is a **compile error**. This replaces the
/// fragile `EXPR_ARGS_ALLOWLIST` grep ratchet (`check_value_array_allowlist.sh`,
/// Issue #6807) that confined the variant *text* to a hand-maintained file list:
/// the confinement invariant is now a type the compiler checks on every call,
/// following the `Resolved` newtype template (#8642). External consumers hold the
/// opaque carrier bound by `Value::ExprArgs(carrier)` and reach its storage via
/// [`ExprArgsCarrier::as_array_ref`] / [`ExprArgsCarrier::into_array_ref`] — the
/// typed witness accessors — never by matching the inner field.
///
/// Only `Debug + Clone` are derived (matching what `Value` derives): the carrier
/// is never serialized — `Value`'s manual `Serialize` errors on the `ExprArgs`
/// variant (only literal variants are cache-serializable), so the payload's
/// wire format is unchanged by this newtype.
#[derive(Debug, Clone)]
pub struct ExprArgsCarrier(ArrayRef);

impl ExprArgsCarrier {
    /// Wrap an [`ArrayRef`] as the `expr.args` carrier payload. Private to the
    /// hub module so carrier construction stays confined here (Issue #8918).
    #[inline]
    fn new(arr: ArrayRef) -> Self {
        ExprArgsCarrier(arr)
    }

    /// Borrow the carried [`ArrayRef`] — the typed witness read accessor.
    #[inline]
    pub fn as_array_ref(&self) -> &ArrayRef {
        &self.0
    }

    /// Consume the carrier, returning the owned [`ArrayRef`].
    #[inline]
    pub fn into_array_ref(self) -> ArrayRef {
        self.0
    }
}

/// Shared destructure helper for the explicit native Array compatibility
/// carrier. Returns the inner [`ArrayRef`] when present, otherwise `None`.
#[inline]
pub fn native_array_value_ref(value: &Value) -> Option<&ArrayRef> {
    match value {
        Value::ExprArgs(carrier) => Some(carrier.as_array_ref()),
        _ => None,
    }
}

/// High-level boundary predicate: whether `value` is the explicit native Array
/// compatibility carrier (`Value::ExprArgs`). Callers should prefer this over
/// `native_array_value_ref(value).is_some()` so the carrier representation stays
/// confined to the `native_array_*` boundary helpers (Issue #6834).
#[inline]
pub fn is_native_array_value(value: &Value) -> bool {
    native_array_value_ref(value).is_some()
}

pub fn ensure_native_array_value_acyclic(target: &ArrayRef, value: &Value) -> Result<(), VmError> {
    let mut seen_arrays = Vec::new();
    if value_contains_array_ref(value, target, &mut seen_arrays) {
        return Err(VmError::TypeError(
            "cannot store expr.args into itself".to_string(),
        ));
    }
    Ok(())
}

fn array_ref_id(arr: &ArrayRef) -> usize {
    Rc::as_ptr(arr) as usize
}

fn array_contains_array_ref(
    arr: &ArrayRef,
    target: &ArrayRef,
    seen_arrays: &mut Vec<usize>,
) -> bool {
    if Rc::ptr_eq(arr, target) {
        return true;
    }
    let id = array_ref_id(arr);
    if seen_arrays.contains(&id) {
        return false;
    }
    seen_arrays.push(id);
    let Ok(arr_ref) = arr.try_borrow() else {
        return false;
    };
    for idx in 0..arr_ref.element_count() {
        if arr_ref
            .get_linear(idx)
            .is_ok_and(|value| value_contains_array_ref(&value, target, seen_arrays))
        {
            return true;
        }
    }
    false
}

fn value_contains_array_ref(
    value: &Value,
    target: &ArrayRef,
    seen_arrays: &mut Vec<usize>,
) -> bool {
    match value {
        Value::ExprArgs(carrier) => {
            array_contains_array_ref(carrier.as_array_ref(), target, seen_arrays)
        }
        Value::Expr(expr) => array_contains_array_ref(&expr.args, target, seen_arrays),
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => tuple
            .elements
            .iter()
            .any(|value| value_contains_array_ref(value, target, seen_arrays)),
        Value::NamedTuple(named_tuple) => named_tuple
            .values
            .iter()
            .any(|value| value_contains_array_ref(value, target, seen_arrays)),
        Value::Pairs(pairs) => pairs
            .data
            .values
            .iter()
            .any(|value| value_contains_array_ref(value, target, seen_arrays)),
        Value::Struct(instance) => instance
            .values
            .iter()
            .any(|value| value_contains_array_ref(value, target, seen_arrays)),
        Value::QuoteNode(value) => value_contains_array_ref(value, target, seen_arrays),
        Value::Closure(closure) => closure
            .captures
            .iter()
            .any(|(_, value)| value_contains_array_ref(value, target, seen_arrays)),
        Value::ComposedFunction(composed) => {
            value_contains_array_ref(&composed.outer, target, seen_arrays)
                || value_contains_array_ref(&composed.inner, target, seen_arrays)
        }
        Value::Ref(cell) => cell
            .try_borrow()
            .is_ok_and(|value| value_contains_array_ref(&value, target, seen_arrays)),
        Value::Generator(generator) => {
            let callable_has_target = match &generator.callable {
                super::generator::GeneratorCallable::RuntimeValue(value)
                | super::generator::GeneratorCallable::TupleSplatRuntimeValue(value) => {
                    value_contains_array_ref(value, target, seen_arrays)
                }
                super::generator::GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
                    value_contains_array_ref(map, target, seen_arrays)
                        || value_contains_array_ref(predicate, target, seen_arrays)
                }
                _ => false,
            };
            callable_has_target || value_contains_array_ref(&generator.iter, target, seen_arrays)
        }
        _ => false,
    }
}

/// Shared constructor wrapping an [`ArrayRef`] in the explicit native Array
/// compatibility carrier.
#[inline]
pub fn native_array_ref_value(arr: ArrayRef) -> Value {
    Value::ExprArgs(ExprArgsCarrier::new(arr))
}

/// Shared constructor wrapping an owned [`ArrayValue`] in the transitional
/// native array carrier via [`new_array_ref`]. Companion of
/// [`native_array_ref_value`] for the common per-file pattern that wraps a
/// freshly-built [`ArrayValue`] in the legacy native-array carrier through
/// [`new_array_ref`]; callers can delegate to this single source of truth
/// while Issue #3908 retires the legacy native carrier.
///
/// Exposed publicly so external-crate consumers (integration tests, host
/// adapters) can route their constructions through the shared helper
/// instead of pattern-matching the native-array variant directly.
#[inline]
pub fn native_array_value_from_array(arr: ArrayValue) -> Value {
    native_array_ref_value(new_array_ref(arr))
}

/// Shared owned-value destructure for the transitional native array
/// carrier. Returns `Ok(arr)` when `value` holds the native-array carrier,
/// otherwise returns the original [`Value`] in `Err(value)` so the caller
/// can chain with `or_else` / match remaining variants. Centralizes the
/// `try_consume_array_value` / `native_array_ref_from_value` pattern that several
/// VM files keep file-locally while Issue #3908 retires the carrier.
#[inline]
pub fn native_array_ref_from_value(value: Value) -> Result<ArrayRef, Value> {
    match value {
        Value::ExprArgs(carrier) => Ok(carrier.into_array_ref()),
        other => Err(other),
    }
}

// Backward compatibility aliases (to be removed after full migration)
pub type TypedArrayValue = ArrayValue;
pub type TypedArrayRef = ArrayRef;

pub fn new_typed_array_ref(arr: ArrayValue) -> ArrayRef {
    new_array_ref(arr)
}

impl ArrayValue {
    /// Create a new array with given data and shape
    pub fn new(data: ArrayData, shape: Vec<usize>) -> Self {
        Self::memory_first_from_array_data(data, shape)
    }

    /// Create an ArrayValue from a MemoryValue by extracting the data and adding a
    /// shape wrapper. Used by zeros/ones/similar builtins internally.
    pub fn from_memory(mem: super::memory_value::MemoryValue, shape: Vec<usize>) -> Self {
        let element_type_override = if mem.data.element_type() != mem.element_type {
            Some(mem.element_type.clone())
        } else {
            None
        };
        Self {
            data: mem.data,
            shape,
            struct_type_id: None,
            element_type_override,
            array_type_override: None,
            shared_parent: None,
        }
    }

    /// Create an ArrayValue from a MemoryValue with an element type override.
    ///
    /// Used for complex arrays where the underlying storage (F64) needs to be
    /// tagged with ComplexF64 element type.
    pub fn from_memory_with_override(
        mem: super::memory_value::MemoryValue,
        shape: Vec<usize>,
        element_type_override: ArrayElementType,
    ) -> Self {
        Self {
            data: mem.data,
            shape,
            struct_type_id: None,
            element_type_override: Some(element_type_override),
            array_type_override: None,
            shared_parent: None,
        }
    }

    fn memory_first_from_array_data(data: ArrayData, shape: Vec<usize>) -> Self {
        let element_type = data.element_type();
        Self::memory_first_from_array_data_with_element_type(data, shape, element_type)
    }

    pub fn memory_first_from_array_data_with_element_type(
        data: ArrayData,
        shape: Vec<usize>,
        element_type: ArrayElementType,
    ) -> Self {
        let len = data.raw_len();
        let mem = MemoryValue::new(data, element_type.clone(), len);
        let mut arr = Self::from_memory(mem, shape);
        arr.struct_type_id = match element_type {
            ArrayElementType::StructOf(type_id)
            | ArrayElementType::StructInlineOf(type_id, _)
            | ArrayElementType::StructInlineF64(type_id, _) => Some(type_id),
            _ => None,
        };
        arr
    }

    /// Allocate primitive Memory{T} first, then wrap it as the transitional
    /// ArrayValue container. This mirrors Julia's Memory-backed Array model
    /// while old VM paths still traffic in the legacy array container.
    pub fn memory_first_undef(element_type: &ArrayElementType, shape: Vec<usize>) -> Self {
        let total_len: usize = shape.iter().product();
        let data = Self::undef_data_for(element_type, total_len);
        let mem = MemoryValue::new(data, element_type.clone(), total_len);
        let mut arr = Self::from_memory(mem, shape);
        arr.struct_type_id = match element_type {
            ArrayElementType::StructOf(type_id)
            | ArrayElementType::StructInlineOf(type_id, _)
            | ArrayElementType::StructInlineF64(type_id, _) => Some(*type_id),
            _ => None,
        };
        arr
    }

    /// Allocate Memory{T}, fill it, then wrap it as transitional ArrayValue.
    pub fn memory_first_filled(
        element_type: &ArrayElementType,
        shape: Vec<usize>,
        value: Value,
    ) -> Result<Self, VmError> {
        let total_len: usize = shape.iter().product();
        let mut mem = MemoryValue::undef_typed(element_type, total_len);
        mem.fill(value)?;
        Ok(Self::from_memory(mem, shape))
    }

    /// Mark this Bool vector as a `BitVector` container for Julia type
    /// projection while preserving the underlying `Bool` element storage.
    pub fn mark_as_bitvector(&mut self) {
        self.array_type_override = Some("BitVector".to_string());
    }

    /// Mark this Bool array as a `BitArray` family container and move Bool
    /// storage to the bit-packed backend.
    pub fn mark_as_bitarray(&mut self) {
        let container = match self.shape.len() {
            1 => "BitVector".to_string(),
            2 => "BitMatrix".to_string(),
            n => format!("BitArray{{{n}}}"),
        };
        if let Some(parent) = self.shared_parent.clone() {
            parent.borrow_mut().pack_bool_storage();
        }
        self.pack_bool_storage();
        self.array_type_override = Some(container);
    }

    fn pack_bool_storage(&mut self) {
        if let ArrayData::Bool(values) = &self.data {
            self.data = ArrayData::BitPackedBool(BitPackedBoolData::from_bools(values));
        }
    }

    /// Report a supported container type override when it is still valid for
    /// the current array shape and element storage.
    pub fn array_type_override(&self) -> Option<&str> {
        match self.array_type_override.as_deref() {
            Some("BitVector")
                if self.shape.len() == 1 && self.element_type() == ArrayElementType::Bool =>
            {
                Some("BitVector")
            }
            Some("BitMatrix")
                if self.shape.len() == 2 && self.element_type() == ArrayElementType::Bool =>
            {
                Some("BitMatrix")
            }
            Some(name)
                if name.starts_with("BitArray{")
                    && name.ends_with('}')
                    && self.element_type() == ArrayElementType::Bool
                    && name
                        .strip_prefix("BitArray{")
                        .and_then(|s| s.strip_suffix('}'))
                        .and_then(|s| s.parse::<usize>().ok())
                        == Some(self.shape.len()) =>
            {
                Some(name)
            }
            _ => None,
        }
    }

    /// Allocate primitive storage with an explicit storage length, then report
    /// a different logical element type. Complex arrays use this while their
    /// transitional storage remains interleaved real values.
    pub fn memory_first_undef_with_override(
        storage_type: &ArrayElementType,
        storage_len: usize,
        shape: Vec<usize>,
        element_type_override: ArrayElementType,
    ) -> Self {
        let mem = MemoryValue::undef_typed(storage_type, storage_len);
        Self::from_memory_with_override(mem, shape, element_type_override)
    }

    /// Allocate an empty primitive Memory buffer with capacity, then wrap it as
    /// the transitional ArrayValue builder used by array literals. The logical
    /// length starts at zero and grows through `push`.
    pub fn memory_first_with_capacity(element_type: ArrayElementType, capacity: usize) -> Self {
        let data = Self::capacity_data_for(&element_type, capacity);
        let mem = MemoryValue::new(data, element_type.clone(), 0);
        let mut arr = Self::from_memory(mem, vec![0]);

        arr.struct_type_id = match &element_type {
            ArrayElementType::StructOf(type_id)
            | ArrayElementType::StructInlineOf(type_id, _)
            | ArrayElementType::StructInlineF64(type_id, _) => Some(*type_id),
            _ => None,
        };

        // Some logical element types share a physical storage representation,
        // so the tag must survive the Memory -> Array wrapper boundary.
        let needs_override = element_type.is_complex()
            || element_type.is_tuple()
            || element_type.is_struct_inline()
            || matches!(
                element_type,
                ArrayElementType::I128
                    | ArrayElementType::U128
                    // Issue #9301: Float16 shares ArrayData::Any storage, so the
                    // logical eltype tag must survive the Memory -> Array boundary.
                    | ArrayElementType::F16
                    | ArrayElementType::Symbol
                    | ArrayElementType::Nothing
                    | ArrayElementType::SubString
                    | ArrayElementType::Abstract(_)
                    | ArrayElementType::UnionOf(_)
                    | ArrayElementType::Structured(_)
            );
        if needs_override {
            arr.element_type_override = Some(element_type);
        }

        arr
    }

    /// Materialize already-computed Int64 data through Memory first, then wrap
    /// it as the transitional ArrayValue container.
    pub fn memory_first_from_i64(data: Vec<i64>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_array_data(ArrayData::I64(data), shape)
    }

    /// Materialize already-computed Float64 data through Memory first, then wrap
    /// it as the transitional ArrayValue container.
    pub fn memory_first_from_f64(data: Vec<f64>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_array_data(ArrayData::F64(data), shape)
    }

    /// Materialize already-computed Char data through Memory first, then wrap it
    /// as the transitional ArrayValue container.
    pub fn memory_first_from_char(data: Vec<char>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_array_data(ArrayData::Char(data), shape)
    }

    /// Materialize already-computed Bool data through Memory first, then wrap it
    /// as the transitional ArrayValue container.
    pub fn memory_first_from_bool(data: Vec<bool>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_array_data(ArrayData::Bool(data), shape)
    }

    /// Allocate bit-packed Bool storage for a BitArray-family container.
    pub fn memory_first_bitpacked_bool_undef(shape: Vec<usize>) -> Self {
        let total_len: usize = shape.iter().product();
        Self::memory_first_from_array_data(
            ArrayData::BitPackedBool(BitPackedBoolData::new_false(total_len)),
            shape,
        )
    }

    /// Materialize already-computed UInt8 data through Memory first, then wrap
    /// it as the transitional ArrayValue container.
    pub fn memory_first_from_u8(data: Vec<u8>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_array_data(ArrayData::U8(data), shape)
    }

    /// Materialize already-computed String data through Memory first, then wrap
    /// it as the transitional ArrayValue container.
    pub fn memory_first_from_strings(data: Vec<StrRef>, shape: Vec<usize>) -> Self {
        let data = data.into_iter().map(Value::str_new).collect();
        Self::memory_first_from_array_data(ArrayData::String(data), shape)
    }

    /// Copy logical Array elements through Memory first, then wrap the new
    /// storage as an independent transitional ArrayValue.
    pub fn memory_first_copy_from_array(source: &Self) -> Result<Self, VmError> {
        let element_type = source.element_type();
        let preserve_bitarray =
            source.array_type_override().is_some() && element_type == ArrayElementType::Bool;
        let mut copy = if preserve_bitarray {
            let mut arr = Self::memory_first_bitpacked_bool_undef(vec![0]);
            arr.data.reserve(source.element_count());
            arr
        } else {
            Self::memory_first_with_capacity(element_type, source.element_count())
        };
        for idx in 0..source.element_count() {
            copy.push(source.get_linear(idx)?)?;
        }
        copy.shape = source.shape.clone();
        copy.struct_type_id = source.struct_type_id;
        if preserve_bitarray {
            copy.mark_as_bitarray();
        }
        Ok(copy)
    }

    /// Materialize collected iterator values through Memory first. This is the
    /// runtime equivalent of Julia's collect/grow path for iterators whose
    /// result element type is discovered from produced values.
    pub fn memory_first_collect_values(
        values: Vec<Value>,
        empty_element_type: ArrayElementType,
    ) -> Result<Self, VmError> {
        let element_type =
            Self::collected_values_element_type(&values).unwrap_or(empty_element_type);
        let mut arr = Self::memory_first_with_capacity(element_type, values.len());
        for value in values {
            arr.push(value)?;
        }
        Ok(arr)
    }

    /// Materialize values whose element type is discovered from the produced
    /// values, using Julia's collection typejoin rather than numeric
    /// promotion. This mirrors the `EltypeUnknown` path in upstream
    /// `base/array.jl` (`collect_to_with_first!` / `push_widen`), where
    /// collecting `1` and `2.0` yields `Vector{Real}` and preserves boxed
    /// values instead of converting `1` to `1.0`.
    pub fn memory_first_collect_typejoin_values(
        values: Vec<Value>,
        empty_element_type: ArrayElementType,
    ) -> Result<Self, VmError> {
        let element_type =
            Self::collected_values_typejoin_element_type(&values).unwrap_or(empty_element_type);
        let mut arr = Self::memory_first_with_capacity(element_type, values.len());
        for value in values {
            arr.push(value)?;
        }
        Ok(arr)
    }

    /// Materialize a sliced Array result through Memory first while preserving
    /// the source element tag for empty slices and specialized storage.
    pub fn memory_first_slice_from_values(
        source: &Self,
        values: Vec<Value>,
        shape: Vec<usize>,
    ) -> Result<Self, VmError> {
        let mut arr = Self::memory_first_with_capacity(source.element_type(), values.len());
        for value in values {
            arr.push(value)?;
        }
        arr.shape = shape;
        arr.struct_type_id = source.struct_type_id;
        if source.element_type_override.is_some() {
            arr.element_type_override = source.element_type_override.clone();
        }
        Ok(arr)
    }

    fn collected_values_element_type(values: &[Value]) -> Option<ArrayElementType> {
        let first = values.first()?;
        let mut element_type = Self::collect_value_element_type(first);
        for value in &values[1..] {
            element_type = Self::join_collect_element_types(
                element_type,
                Self::collect_value_element_type(value),
            );
        }
        Some(element_type)
    }

    fn collected_values_typejoin_element_type(values: &[Value]) -> Option<ArrayElementType> {
        let first = values.first()?;
        let mut element_type = Self::collect_value_element_type(first);
        for value in &values[1..] {
            element_type = Self::typejoin_collect_element_types(
                element_type,
                Self::collect_value_element_type(value),
            );
        }
        Some(element_type)
    }

    fn collect_value_element_type(value: &Value) -> ArrayElementType {
        match value {
            Value::I8(_) => ArrayElementType::I8,
            Value::I16(_) => ArrayElementType::I16,
            Value::I32(_) => ArrayElementType::I32,
            Value::I64(_) => ArrayElementType::I64,
            Value::I128(_) => ArrayElementType::I128,
            Value::BigInt(_) => ArrayElementType::Abstract("BigInt".to_string()),
            Value::U8(_) => ArrayElementType::U8,
            Value::U16(_) => ArrayElementType::U16,
            Value::U32(_) => ArrayElementType::U32,
            Value::U64(_) => ArrayElementType::U64,
            Value::U128(_) => ArrayElementType::U128,
            Value::F16(_) => ArrayElementType::F16,
            Value::F32(_) => ArrayElementType::F32,
            Value::F64(_) => ArrayElementType::F64,
            Value::BigFloat(_) => ArrayElementType::Abstract("BigFloat".to_string()),
            Value::Bool(_) => ArrayElementType::Bool,
            Value::Str(_) | Value::StrBytes(_) => ArrayElementType::String,
            Value::Char(_) | Value::CharMalformed(_) => ArrayElementType::Char,
            Value::Symbol(_) => ArrayElementType::Symbol,
            Value::Struct(s) if s.is_complex() => match s.struct_name.as_ref() {
                "Complex{Float32}" | "ComplexF32" => ArrayElementType::ComplexF32,
                "Complex{Float64}" | "ComplexF64" | "Complex" => ArrayElementType::ComplexF64,
                name => ArrayElementType::Abstract(name.to_string()),
            },
            Value::Tuple(t) => ArrayElementType::TupleOf(
                t.elements
                    .iter()
                    .map(Self::collect_value_element_type)
                    .collect(),
            ),
            _ => ArrayElementType::Any,
        }
    }

    fn join_collect_element_types(
        left: ArrayElementType,
        right: ArrayElementType,
    ) -> ArrayElementType {
        if left == right {
            return left;
        }
        match (&left, &right) {
            (ArrayElementType::I64, ArrayElementType::F64)
            | (ArrayElementType::F64, ArrayElementType::I64)
            | (ArrayElementType::F32, ArrayElementType::F64)
            | (ArrayElementType::F64, ArrayElementType::F32)
            | (ArrayElementType::I64, ArrayElementType::F32)
            | (ArrayElementType::F32, ArrayElementType::I64) => ArrayElementType::F64,
            _ => ArrayElementType::Any,
        }
    }

    fn typejoin_collect_element_types(
        left: ArrayElementType,
        right: ArrayElementType,
    ) -> ArrayElementType {
        if left == right {
            return left;
        }
        if let Some(common) = Self::typejoin_numeric_abstract_name(&left, &right) {
            return ArrayElementType::Abstract(common.to_string());
        }
        ArrayElementType::Any
    }

    fn typejoin_numeric_abstract_name(
        left: &ArrayElementType,
        right: &ArrayElementType,
    ) -> Option<&'static str> {
        let left_chain = Self::numeric_abstract_chain(left)?;
        let right_chain = Self::numeric_abstract_chain(right)?;
        left_chain
            .iter()
            .find(|candidate| right_chain.contains(candidate))
            .copied()
    }

    fn numeric_abstract_chain(element_type: &ArrayElementType) -> Option<&'static [&'static str]> {
        match element_type {
            ArrayElementType::Bool => Some(&["Integer", "Real", "Number", "Any"]),
            ArrayElementType::I8
            | ArrayElementType::I16
            | ArrayElementType::I32
            | ArrayElementType::I64
            | ArrayElementType::I128 => Some(&["Signed", "Integer", "Real", "Number", "Any"]),
            ArrayElementType::U8
            | ArrayElementType::U16
            | ArrayElementType::U32
            | ArrayElementType::U64
            | ArrayElementType::U128 => Some(&["Unsigned", "Integer", "Real", "Number", "Any"]),
            ArrayElementType::F16 | ArrayElementType::F32 | ArrayElementType::F64 => {
                Some(&["AbstractFloat", "Real", "Number", "Any"])
            }
            ArrayElementType::Abstract(name) => match name.as_str() {
                "BigInt" => Some(&["BigInt", "Signed", "Integer", "Real", "Number", "Any"]),
                "BigFloat" => Some(&["BigFloat", "AbstractFloat", "Real", "Number", "Any"]),
                "Signed" => Some(&["Signed", "Integer", "Real", "Number", "Any"]),
                "Unsigned" => Some(&["Unsigned", "Integer", "Real", "Number", "Any"]),
                "Integer" => Some(&["Integer", "Real", "Number", "Any"]),
                "AbstractFloat" => Some(&["AbstractFloat", "Real", "Number", "Any"]),
                "Real" => Some(&["Real", "Number", "Any"]),
                "Number" => Some(&["Number", "Any"]),
                "Any" => Some(&["Any"]),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn capacity_data_for(element_type: &ArrayElementType, capacity: usize) -> ArrayData {
        match element_type {
            ArrayElementType::F32 => ArrayData::F32(Vec::with_capacity(capacity)),
            ArrayElementType::F64 => ArrayData::F64(Vec::with_capacity(capacity)),
            // Complex types use interleaved storage: [re1, im1, re2, im2, ...]
            // Each complex number takes 2 slots in the underlying storage.
            // Issue #9198 S5: Complex{Float64} shares the contiguous-isbits
            // `StructF64` buffer with user 2×f64 structs (was `ArrayData::F64`);
            // Complex{Float32} keeps `ArrayData::F32` (no f32 struct variant yet).
            ArrayElementType::ComplexF32 => ArrayData::F32(Vec::with_capacity(capacity * 2)),
            ArrayElementType::ComplexF64 => ArrayData::StructF64(Vec::with_capacity(capacity * 2)),
            ArrayElementType::I8 => ArrayData::I8(Vec::with_capacity(capacity)),
            ArrayElementType::I16 => ArrayData::I16(Vec::with_capacity(capacity)),
            ArrayElementType::I32 => ArrayData::I32(Vec::with_capacity(capacity)),
            ArrayElementType::I64 => ArrayData::I64(Vec::with_capacity(capacity)),
            // Issue #3557: I128/U128 stored as boxed Any with override.
            // Issue #9301: Float16 likewise has no inline storage variant.
            ArrayElementType::I128 | ArrayElementType::U128 | ArrayElementType::F16 => {
                ArrayData::Any(Vec::with_capacity(capacity))
            }
            ArrayElementType::U8 => ArrayData::U8(Vec::with_capacity(capacity)),
            ArrayElementType::U16 => ArrayData::U16(Vec::with_capacity(capacity)),
            ArrayElementType::U32 => ArrayData::U32(Vec::with_capacity(capacity)),
            ArrayElementType::U64 => ArrayData::U64(Vec::with_capacity(capacity)),
            ArrayElementType::Bool => ArrayData::Bool(Vec::with_capacity(capacity)),
            ArrayElementType::String => ArrayData::String(Vec::<Value>::with_capacity(capacity)),
            // SubString{String}: shares storage with String (Issue #3574).
            ArrayElementType::SubString => ArrayData::String(Vec::<Value>::with_capacity(capacity)),
            ArrayElementType::Char => ArrayData::Char(Vec::with_capacity(capacity)),
            ArrayElementType::Symbol => ArrayData::Any(Vec::with_capacity(capacity)),
            ArrayElementType::Nothing => ArrayData::Any(Vec::with_capacity(capacity)),
            ArrayElementType::StructOf(_) => ArrayData::StructRefs(Vec::with_capacity(capacity)),
            // isbits struct inline storage: AoS format.
            ArrayElementType::StructInlineOf(_, field_count) => {
                ArrayData::Any(Vec::with_capacity(capacity * field_count))
            }
            // All-`Float64` isbits struct: contiguous interleaved f64 (Issue
            // #9198 S4). `field_count` raw f64 slots per element.
            ArrayElementType::StructInlineF64(_, field_count) => {
                ArrayData::StructF64(Vec::with_capacity(capacity * field_count))
            }
            ArrayElementType::Struct | ArrayElementType::Any => {
                ArrayData::Any(Vec::with_capacity(capacity))
            }
            // Tuple arrays use AoS storage in ArrayData::Any.
            ArrayElementType::TupleOf(field_types) => {
                ArrayData::Any(Vec::with_capacity(capacity * field_types.len()))
            }
            // Issue #3549: Union element type uses heterogeneous Any storage.
            ArrayElementType::UnionOf(_)
            | ArrayElementType::Abstract(_)
            | ArrayElementType::Structured(_) => ArrayData::Any(Vec::with_capacity(capacity)),
        }
    }

    fn undef_data_for(element_type: &ArrayElementType, length: usize) -> ArrayData {
        match element_type {
            ArrayElementType::F32 => ArrayData::F32(vec![0.0; length]),
            ArrayElementType::F64 => ArrayData::F64(vec![0.0; length]),
            ArrayElementType::ComplexF32 => ArrayData::F32(vec![0.0; length * 2]),
            // Issue #9198 S5: contiguous-isbits `StructF64` buffer (was `F64`).
            ArrayElementType::ComplexF64 => ArrayData::StructF64(vec![0.0; length * 2]),
            ArrayElementType::I8 => ArrayData::I8(vec![0; length]),
            ArrayElementType::I16 => ArrayData::I16(vec![0; length]),
            ArrayElementType::I32 => ArrayData::I32(vec![0; length]),
            ArrayElementType::I64 => ArrayData::I64(vec![0; length]),
            ArrayElementType::I128 => ArrayData::Any(vec![Value::I128(0); length]),
            ArrayElementType::U8 => ArrayData::U8(vec![0; length]),
            ArrayElementType::U16 => ArrayData::U16(vec![0; length]),
            ArrayElementType::U32 => ArrayData::U32(vec![0; length]),
            ArrayElementType::U64 => ArrayData::U64(vec![0; length]),
            ArrayElementType::U128 => ArrayData::Any(vec![Value::U128(0); length]),
            // Issue #9301: Float16 uses boxed Any storage tagged Float16.
            ArrayElementType::F16 => {
                ArrayData::Any(vec![Value::F16(half::f16::from_f32(0.0)); length])
            }
            ArrayElementType::Bool => ArrayData::Bool(vec![false; length]),
            ArrayElementType::String => ArrayData::String(vec![Value::str_new(""); length]),
            ArrayElementType::SubString => ArrayData::String(vec![Value::str_new(""); length]),
            ArrayElementType::Char => ArrayData::Char(vec!['\0'; length]),
            ArrayElementType::Symbol => ArrayData::Any(vec![Value::Nothing; length]),
            ArrayElementType::Nothing => ArrayData::Any(vec![Value::Nothing; length]),
            ArrayElementType::StructOf(_) => ArrayData::Any(vec![Value::Nothing; length]),
            ArrayElementType::Struct | ArrayElementType::Any => {
                ArrayData::Any(vec![Value::Nothing; length])
            }
            ArrayElementType::StructInlineOf(_, field_count) => {
                ArrayData::Any(vec![Value::Nothing; length * field_count])
            }
            // All-`Float64` isbits struct: zero-filled contiguous f64 (Issue
            // #9198 S4). Zero-fill is a valid isbits value (each field 0.0),
            // matching how the primitive-scalar undef arms zero-initialize.
            ArrayElementType::StructInlineF64(_, field_count) => {
                ArrayData::StructF64(vec![0.0; length * field_count])
            }
            ArrayElementType::TupleOf(field_types) => {
                ArrayData::Any(vec![Value::Nothing; length * field_types.len()])
            }
            ArrayElementType::UnionOf(_)
            | ArrayElementType::Abstract(_)
            | ArrayElementType::Structured(_) => ArrayData::Any(vec![Value::Nothing; length]),
        }
    }

    /// Create a new f64 array from raw data
    pub fn from_f64(data: Vec<f64>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_f64(data, shape)
    }

    /// Create a new i64 array from raw data
    pub fn from_i64(data: Vec<i64>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_i64(data, shape)
    }

    /// Create a 1D f64 vector
    pub fn vector(data: Vec<f64>) -> Self {
        let len = data.len();
        Self::memory_first_from_f64(data, vec![len])
    }

    /// Create a 1D i64 vector
    pub fn i64_vector(data: Vec<i64>) -> Self {
        let len = data.len();
        Self::memory_first_from_i64(data, vec![len])
    }

    /// Create a bool array from raw data
    pub fn from_bool(data: Vec<bool>, shape: Vec<usize>) -> Self {
        Self::memory_first_from_bool(data, shape)
    }

    /// Create a 1D bool vector
    pub fn bool_vector(data: Vec<bool>) -> Self {
        let len = data.len();
        Self::memory_first_from_bool(data, vec![len])
    }

    /// Create a zeros array (f64)
    pub fn zeros(shape: Vec<usize>) -> Self {
        Self::zeros_f64(shape)
    }

    /// Create a ones array (f64)
    pub fn ones(shape: Vec<usize>) -> Self {
        Self::ones_f64(shape)
    }

    /// Create a zeros array (f64) - explicit type version
    pub fn zeros_f64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self::memory_first_from_f64(vec![0.0; total], shape)
    }

    /// Create a zeros array (i64) - explicit type version
    pub fn zeros_i64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self::memory_first_from_i64(vec![0; total], shape)
    }

    /// Create a ones array (f64) - explicit type version
    pub fn ones_f64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self::memory_first_from_f64(vec![1.0; total], shape)
    }

    /// Create a ones array (i64) - explicit type version
    pub fn ones_i64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self::memory_first_from_i64(vec![1; total], shape)
    }

    /// Create a filled array with a specific f64 value
    pub fn fill(value: f64, shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self::memory_first_from_f64(vec![value; total], shape)
    }

    /// Create a filled array with a specific Value
    pub fn fill_value(value: Value, shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        let data = match &value {
            Value::F64(v) => ArrayData::F64(vec![*v; total]),
            Value::F32(v) => ArrayData::F32(vec![*v; total]),
            Value::I64(v) => ArrayData::I64(vec![*v; total]),
            Value::Bool(v) => ArrayData::Bool(vec![*v; total]),
            Value::Str(_) | Value::StrBytes(_) => ArrayData::String(vec![value; total]),
            Value::Char(c) => ArrayData::Char(vec![*c; total]),
            _ => ArrayData::Any(vec![value; total]),
        };
        Self::memory_first_from_array_data(data, shape)
    }

    /// Create a new empty array with given element type and capacity
    pub fn with_capacity(element_type: ArrayElementType, capacity: usize) -> Self {
        Self::memory_first_with_capacity(element_type, capacity)
    }

    /// Create a struct array with given type_id and capacity
    pub fn with_struct_type(type_id: usize, capacity: usize) -> Self {
        Self::memory_first_from_array_data_with_element_type(
            ArrayData::StructRefs(Vec::with_capacity(capacity)),
            vec![0],
            ArrayElementType::StructOf(type_id),
        )
    }

    /// Create a heterogeneous (Any) array from a Vec<Value>
    pub fn any_vector(values: Vec<Value>) -> Self {
        let len = values.len();
        let mem = MemoryValue::new(ArrayData::Any(values), ArrayElementType::Any, len);
        Self::from_memory(mem, vec![len])
    }

    /// Create a complex F64 array with interleaved storage
    /// data should be [re1, im1, re2, im2, ...] with shape indicating logical dimensions
    ///
    /// Issue #9198 S5: `Complex{Float64}` IS the 2×f64 isbits struct, so its
    /// interleaved buffer is the general contiguous-isbits `ArrayData::StructF64`
    /// variant (shared with user all-`Float64` structs), not the scalar-`Float64`
    /// `ArrayData::F64`. The `ComplexF64` element-type override still tags the
    /// logical type, so `is_complex`/matmul/display/inference are unchanged.
    pub fn complex_f64(data: Vec<f64>, shape: Vec<usize>) -> Self {
        let len = data.len();
        let mem = MemoryValue::new(ArrayData::StructF64(data), ArrayElementType::F64, len);
        Self::from_memory_with_override(mem, shape, ArrayElementType::ComplexF64)
    }

    /// Create a complex F32 array with interleaved storage
    /// data should be [re1, im1, re2, im2, ...] with shape indicating logical dimensions
    pub fn complex_f32(data: Vec<f32>, shape: Vec<usize>) -> Self {
        let len = data.len();
        let mem = MemoryValue::new(ArrayData::F32(data), ArrayElementType::F32, len);
        Self::from_memory_with_override(mem, shape, ArrayElementType::ComplexF32)
    }

    /// Create a complex F64 array filled with zeros
    pub fn zeros_complex_f64(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        // Each complex number needs 2 f64 values (re, im)
        Self::complex_f64(vec![0.0; size * 2], shape)
    }

    /// Create a complex F32 array filled with zeros
    pub fn zeros_complex_f32(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        // Each complex number needs 2 f32 values (re, im)
        Self::complex_f32(vec![0.0; size * 2], shape)
    }

    /// Create an uninitialized Float64 array (for Vector{Float64}(undef, n))
    /// Values are initialized to 0.0 for safety (Rust doesn't have true undef)
    pub fn undef_f64(shape: Vec<usize>) -> Self {
        Self::memory_first_undef(&ArrayElementType::F64, shape)
    }

    /// Create an uninitialized Int64 array (for Vector{Int64}(undef, n))
    /// Values are initialized to 0 for safety (Rust doesn't have true undef)
    pub fn undef_i64(shape: Vec<usize>) -> Self {
        Self::memory_first_undef(&ArrayElementType::I64, shape)
    }

    /// Create an uninitialized Bool array (for Vector{Bool}(undef, n))
    /// Values are initialized to false for safety
    pub fn undef_bool(shape: Vec<usize>) -> Self {
        Self::memory_first_undef(&ArrayElementType::Bool, shape)
    }

    /// Create an uninitialized Complex{Float64} array (for Vector{Complex{Float64}}(undef, n))
    /// Values are initialized to 0.0+0.0im for safety
    pub fn undef_complex_f64(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        // Each complex number needs 2 f64 values (re, im)
        Self::complex_f64(vec![0.0; size * 2], shape)
    }

    /// Create an uninitialized array for any supported element type (Issue #2218).
    /// This is the generic version that handles all types including small integers and floats.
    pub fn undef_typed(elem_type: &ArrayElementType, shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        match elem_type {
            ArrayElementType::F64 => Self::undef_f64(shape),
            ArrayElementType::I64 => Self::undef_i64(shape),
            ArrayElementType::Bool => Self::undef_bool(shape),
            ArrayElementType::ComplexF64 => Self::undef_complex_f64(shape),
            ArrayElementType::ComplexF32 => Self::complex_f32(vec![0.0; size * 2], shape),
            ArrayElementType::F32 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::F32(vec![0.0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::I8 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::I8(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::I16 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::I16(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::I32 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::I32(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::U8 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::U8(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::U16 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::U16(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::U32 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::U32(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::U64 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::U64(vec![0; size]),
                shape,
                elem_type.clone(),
            ),
            // Issue #3557: Vector{Int128}(undef, n)/Vector{UInt128}(undef, n)
            // store boxed values in `ArrayData::Any` and tag the override.
            ArrayElementType::I128 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::Any(vec![Value::I128(0); size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::U128 => Self::memory_first_from_array_data_with_element_type(
                ArrayData::Any(vec![Value::U128(0); size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::String => Self::memory_first_from_array_data_with_element_type(
                ArrayData::String(vec![Value::str_new(""); size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::Char => Self::memory_first_from_array_data_with_element_type(
                ArrayData::Char(vec!['\0'; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::Symbol => Self::memory_first_from_array_data_with_element_type(
                ArrayData::Any(vec![Value::Nothing; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::Nothing => Self::memory_first_from_array_data_with_element_type(
                ArrayData::Any(vec![Value::Nothing; size]),
                shape,
                elem_type.clone(),
            ),
            ArrayElementType::UnionOf(_)
            | ArrayElementType::Abstract(_)
            | ArrayElementType::Structured(_) => {
                Self::memory_first_from_array_data_with_element_type(
                    ArrayData::Any(vec![Value::Nothing; size]),
                    shape,
                    elem_type.clone(),
                )
            }
            ArrayElementType::TupleOf(field_types) => {
                Self::memory_first_from_array_data_with_element_type(
                    ArrayData::Any(vec![Value::Nothing; size * field_types.len()]),
                    shape,
                    elem_type.clone(),
                )
            }
            _ => Self::undef_any(shape),
        }
    }

    /// Create an uninitialized Any array (for Vector{Any}(undef, n))
    /// Values are initialized to nothing for safety
    pub fn undef_any(shape: Vec<usize>) -> Self {
        use super::Value;
        let size: usize = shape.iter().product();
        Self::memory_first_from_array_data(ArrayData::Any(vec![Value::Nothing; size]), shape)
    }

    /// Create a tuple array with AoS (Array of Structs) storage
    /// data should be [a1, b1, a2, b2, ...] for Tuple{A, B} tuples
    /// shape indicates logical dimensions (number of tuples)
    pub fn tuple_array(
        data: Vec<Value>,
        shape: Vec<usize>,
        field_types: Vec<ArrayElementType>,
    ) -> Self {
        Self::memory_first_from_array_data_with_element_type(
            ArrayData::Any(data),
            shape,
            ArrayElementType::TupleOf(field_types),
        )
    }

    /// Create an empty tuple array with given field types and capacity
    pub fn with_tuple_capacity(field_types: Vec<ArrayElementType>, capacity: usize) -> Self {
        Self::memory_first_with_capacity(ArrayElementType::TupleOf(field_types), capacity)
    }

    /// Create an isbits struct array with inline AoS storage
    /// data should be [f1_1, f2_1, f1_2, f2_2, ...] for Point{x, y} structs
    /// shape indicates logical dimensions (number of structs)
    pub fn isbits_struct_array(
        type_id: usize,
        field_count: usize,
        data: Vec<Value>,
        shape: Vec<usize>,
    ) -> Self {
        Self::memory_first_from_array_data_with_element_type(
            ArrayData::Any(data),
            shape,
            ArrayElementType::StructInlineOf(type_id, field_count),
        )
    }

    /// Create an empty isbits struct array with capacity
    pub fn with_isbits_struct_capacity(
        type_id: usize,
        field_count: usize,
        capacity: usize,
    ) -> Self {
        Self::memory_first_with_capacity(
            ArrayElementType::StructInlineOf(type_id, field_count),
            capacity,
        )
    }

    /// Check if this is an isbits struct array
    pub fn is_isbits_struct_array(&self) -> bool {
        matches!(
            self.element_type_override,
            Some(ArrayElementType::StructInlineOf(_, _))
        )
    }

    /// Check if this array stores heap-backed struct references.
    pub fn is_struct_ref_array(&self) -> bool {
        matches!(self.data, ArrayData::StructRefs(_))
            || matches!(
                self.element_type_override,
                Some(ArrayElementType::StructOf(_))
            )
    }

    /// Return whether this array can stay on the retained inline dynamic-op path.
    ///
    /// Public dispatch code should not inspect raw storage variants directly;
    /// this keeps the transitional storage test owned by ArrayValue while array
    /// arithmetic moves toward Pure Julia dispatch.
    pub fn supports_inline_dynamic_storage(&self) -> bool {
        !matches!(self.data, ArrayData::StructRefs(_) | ArrayData::Any(_))
    }

    /// Get the element type
    /// Returns element_type_override if set, otherwise infers from data
    pub fn element_type(&self) -> ArrayElementType {
        self.element_type_override
            .clone()
            .unwrap_or_else(|| self.data.element_type())
    }

    /// Get the number of logical elements
    pub fn element_count(&self) -> usize {
        self.shape.iter().product()
    }

    /// Reserve backing capacity for at least `additional` more logical elements
    /// (Issue #5186). Pure capacity hint: the logical length and shape are left
    /// unchanged. The raw count is scaled by the per-element storage multiplier
    /// so interleaved Complex storage (2 raw slots/element) and AoS Tuple/struct
    /// storage (one raw slot per field) reserve the right amount.
    pub fn reserve(&mut self, additional: usize) {
        let raw_multiplier = match &self.element_type_override {
            Some(ArrayElementType::ComplexF32 | ArrayElementType::ComplexF64) => 2,
            Some(ArrayElementType::TupleOf(field_types)) => field_types.len().max(1),
            Some(
                ArrayElementType::StructInlineOf(_, field_count)
                | ArrayElementType::StructInlineF64(_, field_count),
            ) => (*field_count).max(1),
            _ => 1,
        };
        self.data.reserve(additional.saturating_mul(raw_multiplier));
    }

    /// Return a distinct ArrayValue with the same storage owner and a new shape.
    ///
    /// Julia's `reshape(::Array, ::Dims)` returns a new Array structure and keeps
    /// the source array shape unchanged while reusing `a.ref`
    /// (`julia/base/reshapedarray.jl`).
    pub fn reshaped_from_ref(source: &ArrayRef, shape: Vec<usize>) -> Result<Self, VmError> {
        let source_borrow = source.borrow();
        let old_count = source_borrow.element_count();
        let new_count: usize = shape.iter().product();
        if old_count != new_count {
            return Err(VmError::DimensionMismatch {
                expected: old_count,
                got: new_count,
            });
        }

        let parent = source_borrow
            .shared_parent
            .clone()
            .unwrap_or_else(|| Rc::clone(source));
        let mut reshaped = source_borrow.clone();
        reshaped.shape = shape;
        reshaped.array_type_override = None;
        reshaped.shared_parent = Some(parent);
        Ok(reshaped)
    }

    /// Total number of elements (alias for element_count)
    pub fn len(&self) -> usize {
        self.data.raw_len()
    }

    /// Check if array is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Number of dimensions
    pub fn ndims(&self) -> usize {
        self.shape.len()
    }

    /// Size in a specific dimension (1-indexed like Julia)
    pub fn size(&self, dim: usize) -> Option<usize> {
        if dim >= 1 && dim <= self.shape.len() {
            Some(self.shape[dim - 1])
        } else {
            None
        }
    }

    /// Convert N-dimensional indices to linear index (column-major, 1-indexed)
    /// Supports both:
    /// - Full indexing: arr[i, j, k] for 3D array (indices.len() == shape.len())
    /// - Linear indexing: arr[i] for any dimension array (indices.len() == 1)
    pub fn linear_index(&self, indices: &[i64]) -> Result<usize, VmError> {
        // Linear indexing: single index for any dimension array
        if indices.len() == 1 {
            let index = indices[0];
            let total_size = self.element_count();

            // Bounds check (1-indexed)
            if index < 1 || index as usize > total_size {
                return Err(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                });
            }

            // Convert to 0-indexed
            return Ok((index - 1) as usize);
        }

        // Full indexing: indices count must match dimensions
        if indices.len() != self.shape.len() {
            return Err(VmError::DimensionMismatch {
                expected: self.shape.len(),
                got: indices.len(),
            });
        }

        let mut linear = 0;
        let mut stride = 1;
        for (i, &dim_idx) in indices.iter().enumerate() {
            if dim_idx < 1 || dim_idx as usize > self.shape[i] {
                return Err(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                });
            }
            linear += ((dim_idx - 1) as usize) * stride;
            stride *= self.shape[i];
        }
        Ok(linear)
    }

    /// Get the type_id for Complex structs returned from this array.
    /// Uses the stored struct_type_id if available, falls back to 0.
    /// The struct_type_id is set when the array is created (e.g., in the generic
    /// typed-allocation path `push_undef_typed_array`) to match the runtime
    /// struct_defs ordering.
    fn complex_type_id(&self) -> usize {
        self.struct_type_id.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{ExprValue, SymbolValue};
    use subset_julia_vm_types::types::JuliaType;

    #[test]
    fn reshaped_from_ref_shares_parent_storage() {
        let source = new_array_ref(ArrayValue::from_i64(vec![1, 2, 3, 4, 5, 6], vec![6]));
        let mut reshaped = ArrayValue::reshaped_from_ref(&source, vec![2, 3]).unwrap();

        reshaped.set(&[1, 2], Value::I64(99)).unwrap();
        assert!(matches!(source.borrow().get(&[3]).unwrap(), Value::I64(99)));
        assert_eq!(source.borrow().shape, vec![6]);

        source.borrow_mut().set(&[6], Value::I64(42)).unwrap();
        assert!(matches!(reshaped.get(&[2, 3]).unwrap(), Value::I64(42)));
    }

    #[test]
    fn native_array_cycle_guard_rejects_direct_expr_args_cycle_issue_8610() {
        let args = new_array_ref(ArrayValue::any_vector(vec![]));
        let value = native_array_ref_value(args.clone());

        assert!(ensure_native_array_value_acyclic(&args, &value).is_err());
    }

    #[test]
    fn native_array_cycle_guard_rejects_expr_owning_target_args_issue_8610() {
        let args = new_array_ref(ArrayValue::any_vector(vec![]));
        let expr = Value::Expr(ExprValue {
            head: SymbolValue::new("call"),
            args: args.clone(),
        });

        assert!(ensure_native_array_value_acyclic(&args, &expr).is_err());
    }

    #[test]
    fn memory_first_undef_allocates_typed_storage() {
        let arr = ArrayValue::memory_first_undef(&ArrayElementType::I64, vec![2, 3]);

        assert_eq!(arr.shape, vec![2, 3]);
        assert_eq!(arr.element_type(), ArrayElementType::I64);
        assert_eq!(arr.data.raw_len(), 6);
        assert!(matches!(arr.get(&[2, 3]).unwrap(), Value::I64(0)));
    }

    #[test]
    fn mark_as_bitarray_packs_bool_storage() {
        let mut arr = ArrayValue::memory_first_from_bool(vec![true; 130], vec![130]);
        arr.mark_as_bitarray();

        assert_eq!(arr.array_type_override(), Some("BitVector"));
        match &arr.data {
            ArrayData::BitPackedBool(bits) => {
                assert_eq!(bits.len(), 130);
                assert_eq!(bits.raw_word_len(), 3);
            }
            other => panic!("expected bit-packed Bool storage, got {other:?}"),
        }
    }

    #[test]
    fn supports_inline_dynamic_storage_hides_raw_array_tags() {
        assert!(ArrayValue::from_f64(vec![1.0], vec![1]).supports_inline_dynamic_storage());
        assert!(
            ArrayValue::memory_first_from_i64(vec![1], vec![1]).supports_inline_dynamic_storage()
        );
        assert!(ArrayValue::memory_first_from_bool(vec![true], vec![1])
            .supports_inline_dynamic_storage());
        assert!(ArrayValue::complex_f64(vec![1.0, 2.0], vec![1]).supports_inline_dynamic_storage());

        assert!(!ArrayValue::any_vector(vec![Value::I64(1)]).supports_inline_dynamic_storage());
        assert!(!ArrayValue::new(ArrayData::StructRefs(vec![0]), vec![1])
            .supports_inline_dynamic_storage());
    }

    #[test]
    fn memory_first_undef_preserves_logical_element_tags() {
        // Issue #9198 S5: Complex{Float64} arrays back their interleaved buffer
        // with the general contiguous-isbits `StructF64` variant.
        let complex = ArrayValue::memory_first_undef(&ArrayElementType::ComplexF64, vec![2]);
        assert!(matches!(complex.data, ArrayData::StructF64(ref v) if v.len() == 4));
        assert_eq!(complex.element_type(), ArrayElementType::ComplexF64);

        let i128_arr = ArrayValue::memory_first_undef(&ArrayElementType::I128, vec![2]);
        assert!(matches!(i128_arr.data, ArrayData::Any(ref v) if v.len() == 2));
        assert_eq!(i128_arr.element_type(), ArrayElementType::I128);

        let tuple_ty =
            ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        let tuple_arr = ArrayValue::memory_first_undef(&tuple_ty, vec![3]);
        assert!(matches!(tuple_arr.data, ArrayData::Any(ref v) if v.len() == 6));
        assert_eq!(tuple_arr.element_type(), tuple_ty);
    }

    #[test]
    fn memory_first_filled_allocates_and_fills_memory() {
        let arr = ArrayValue::memory_first_filled(&ArrayElementType::F64, vec![2], Value::F64(1.5))
            .unwrap();

        assert_eq!(arr.shape, vec![2]);
        assert_eq!(arr.element_type(), ArrayElementType::F64);
        assert!(matches!(arr.get(&[1]).unwrap(), Value::F64(1.5)));
        assert!(matches!(arr.get(&[2]).unwrap(), Value::F64(1.5)));
    }

    #[test]
    fn memory_first_undef_with_override_keeps_logical_element_type() {
        let arr = ArrayValue::memory_first_undef_with_override(
            &ArrayElementType::F64,
            4,
            vec![2],
            ArrayElementType::ComplexF64,
        );

        assert_eq!(arr.shape, vec![2]);
        assert_eq!(arr.data.raw_len(), 4);
        assert_eq!(arr.element_type(), ArrayElementType::ComplexF64);
    }

    #[test]
    fn complex_helpers_preserve_interleaved_storage_and_logical_type() {
        let arr = ArrayValue::complex_f64(vec![1.0, 2.0, 3.0, 4.0], vec![2]);
        assert_eq!(arr.shape, vec![2]);
        assert_eq!(arr.data.raw_len(), 4);
        assert_eq!(arr.element_type(), ArrayElementType::ComplexF64);

        let zeros = ArrayValue::zeros_complex_f32(vec![3]);
        assert_eq!(zeros.shape, vec![3]);
        assert_eq!(zeros.data.raw_len(), 6);
        assert_eq!(zeros.element_type(), ArrayElementType::ComplexF32);

        let undef = ArrayValue::undef_typed(&ArrayElementType::ComplexF32, vec![2]);
        assert_eq!(undef.shape, vec![2]);
        assert_eq!(undef.data.raw_len(), 4);
        assert_eq!(undef.element_type(), ArrayElementType::ComplexF32);
    }

    #[test]
    fn memory_first_with_capacity_builds_literal_builder() {
        let mut arr = ArrayValue::memory_first_with_capacity(ArrayElementType::I64, 3);

        assert_eq!(arr.shape, vec![0]);
        assert_eq!(arr.element_type(), ArrayElementType::I64);
        assert_eq!(arr.data.raw_len(), 0);

        arr.push(Value::I64(10)).unwrap();
        arr.push(Value::I64(20)).unwrap();

        assert_eq!(arr.shape, vec![2]);
        assert_eq!(arr.data.raw_len(), 2);
        assert!(matches!(arr.get(&[1]).unwrap(), Value::I64(10)));
        assert!(matches!(arr.get(&[2]).unwrap(), Value::I64(20)));
    }

    #[test]
    fn memory_first_with_capacity_preserves_logical_tags() {
        let i128_arr = ArrayValue::memory_first_with_capacity(ArrayElementType::I128, 1);
        assert_eq!(i128_arr.element_type(), ArrayElementType::I128);

        let union_arr = ArrayValue::memory_first_with_capacity(
            ArrayElementType::union_from_body("Nothing, Int64"),
            2,
        );
        assert_eq!(
            union_arr.element_type(),
            ArrayElementType::UnionOf(vec![JuliaType::Nothing, JuliaType::Int64])
        );

        let substring_arr = ArrayValue::memory_first_with_capacity(ArrayElementType::SubString, 2);
        assert_eq!(substring_arr.element_type(), ArrayElementType::SubString);

        let tuple_fields = vec![ArrayElementType::I64, ArrayElementType::F64];
        let tuple_ty = ArrayElementType::TupleOf(tuple_fields.clone());
        let tuple_arr = ArrayValue::with_tuple_capacity(tuple_fields, 2);
        assert_eq!(tuple_arr.element_type(), tuple_ty);

        let struct_arr = ArrayValue::with_isbits_struct_capacity(7, 2, 3);
        assert_eq!(struct_arr.struct_type_id, Some(7));
        assert_eq!(
            struct_arr.element_type(),
            ArrayElementType::StructInlineOf(7, 2)
        );

        let struct_ref_arr = ArrayValue::with_struct_type(11, 2);
        assert_eq!(struct_ref_arr.shape, vec![0]);
        assert_eq!(struct_ref_arr.struct_type_id, Some(11));
        assert_eq!(
            struct_ref_arr.element_type(),
            ArrayElementType::StructOf(11)
        );
    }

    /// Issue #9198 S4: an all-`Float64` isbits struct array stores its elements
    /// as CONTIGUOUS raw f64 (`ArrayData::StructF64`), not one boxed `Value` per
    /// field — the layout test the acceptance asks for (proves no per-element
    /// box). `raw_len == n * field_count` f64 slots, and getindex reconstructs
    /// the struct value field-for-field.
    #[test]
    fn struct_inline_f64_array_is_byte_contiguous_no_box_9198() {
        use super::super::StructInstance;
        let type_id = 7usize;
        let field_count = 2usize;
        let mut arr = ArrayValue::memory_first_with_capacity(
            ArrayElementType::StructInlineF64(type_id, field_count),
            3,
        );
        for i in 0..3 {
            let v = Value::Struct(StructInstance::new(
                type_id,
                vec![Value::F64(i as f64), Value::F64((i * 10) as f64)],
            ));
            arr.push(v).unwrap();
        }
        // Logical shape is 3 elements; the eltype tag survived the wrapper boundary.
        assert_eq!(arr.shape, vec![3]);
        assert_eq!(
            arr.element_type(),
            ArrayElementType::StructInlineF64(type_id, field_count)
        );
        // The storage is a single flat f64 buffer of `n * field_count` slots —
        // NOT a `Vec<Value>` of boxed fields.
        match &arr.data {
            ArrayData::StructF64(raw) => {
                assert_eq!(raw.len(), 3 * field_count);
                assert_eq!(raw, &vec![0.0, 0.0, 1.0, 10.0, 2.0, 20.0]);
            }
            other => panic!(
                "expected contiguous StructF64 storage, got {:?}",
                other.type_name()
            ),
        }
        // getindex reconstructs the struct value from the contiguous slots.
        let got = arr.get(&[2]).unwrap();
        match got {
            Value::Struct(s) => {
                assert_eq!(s.type_id, type_id);
                assert_eq!(s.values.len(), field_count);
                assert!(matches!(s.values[0], Value::F64(x) if x == 1.0));
                assert!(matches!(s.values[1], Value::F64(x) if x == 10.0));
            }
            other => panic!(
                "expected reconstructed Struct, got {:?}",
                other.value_type()
            ),
        }
        // setindex! packs a new struct's fields back into the contiguous buffer.
        arr.set(
            &[1],
            Value::Struct(StructInstance::new(
                type_id,
                vec![Value::F64(99.0), Value::F64(88.0)],
            )),
        )
        .unwrap();
        match &arr.data {
            ArrayData::StructF64(raw) => assert_eq!(&raw[0..2], &[99.0, 88.0]),
            _ => unreachable!(),
        }
    }

    #[test]
    fn memory_first_from_materialized_data_preserves_type_and_shape() {
        let int_arr = ArrayValue::memory_first_from_i64(vec![1, 2, 3], vec![3]);
        assert_eq!(int_arr.shape, vec![3]);
        assert_eq!(int_arr.element_type(), ArrayElementType::I64);
        assert!(matches!(int_arr.get(&[3]).unwrap(), Value::I64(3)));

        let float_arr = ArrayValue::memory_first_from_f64(vec![1.5, 2.5], vec![2]);
        assert_eq!(float_arr.shape, vec![2]);
        assert_eq!(float_arr.element_type(), ArrayElementType::F64);
        assert!(matches!(float_arr.get(&[2]).unwrap(), Value::F64(2.5)));

        let char_arr = ArrayValue::memory_first_from_char(vec!['a', 'b'], vec![2]);
        assert_eq!(char_arr.shape, vec![2]);
        assert_eq!(char_arr.element_type(), ArrayElementType::Char);
        assert!(matches!(char_arr.get(&[1]).unwrap(), Value::Char('a')));

        let bool_arr = ArrayValue::memory_first_from_bool(vec![true, false], vec![2]);
        assert_eq!(bool_arr.shape, vec![2]);
        assert_eq!(bool_arr.element_type(), ArrayElementType::Bool);
        assert!(matches!(bool_arr.get(&[2]).unwrap(), Value::Bool(false)));

        let u8_arr = ArrayValue::memory_first_from_u8(vec![0x41, 0x42], vec![2]);
        assert_eq!(u8_arr.shape, vec![2]);
        assert_eq!(u8_arr.element_type(), ArrayElementType::U8);
        assert!(matches!(u8_arr.get(&[2]).unwrap(), Value::U8(0x42)));

        let string_arr = ArrayValue::memory_first_from_strings(
            vec![StrRef::from("a"), StrRef::from("b")],
            vec![2],
        );
        assert_eq!(string_arr.shape, vec![2]);
        assert_eq!(string_arr.element_type(), ArrayElementType::String);
        assert!(matches!(string_arr.get(&[1]).unwrap(), Value::Str(ref s) if s.as_ref() == "a"));

        let any_arr = ArrayValue::any_vector(vec![Value::I64(1), Value::str_new("x".to_string())]);
        assert_eq!(any_arr.shape, vec![2]);
        assert_eq!(any_arr.element_type(), ArrayElementType::Any);
        assert!(matches!(any_arr.get(&[2]).unwrap(), Value::Str(ref s) if s.as_ref() == "x"));
    }

    #[test]
    fn memory_first_copy_from_array_preserves_shape_type_and_independence() {
        let source_ref = new_array_ref(ArrayValue::memory_first_from_i64(
            vec![1, 2, 3, 4],
            vec![2, 2],
        ));
        let reshaped = ArrayValue::reshaped_from_ref(&source_ref, vec![4]).unwrap();
        source_ref
            .borrow_mut()
            .set(&[2, 2], Value::I64(40))
            .unwrap();

        let mut copy = ArrayValue::memory_first_copy_from_array(&reshaped).unwrap();
        assert_eq!(copy.shape, vec![4]);
        assert_eq!(copy.element_type(), ArrayElementType::I64);
        assert!(copy.shared_parent.is_none());
        assert!(matches!(copy.get(&[4]).unwrap(), Value::I64(40)));

        copy.set(&[1], Value::I64(99)).unwrap();
        assert!(matches!(
            source_ref.borrow().get(&[1, 1]).unwrap(),
            Value::I64(1)
        ));
    }

    #[test]
    fn memory_first_slice_from_values_preserves_source_type_and_shape() {
        let source = ArrayValue::memory_first_from_i64(vec![1, 2, 3, 4], vec![2, 2]);
        let slice = ArrayValue::memory_first_slice_from_values(
            &source,
            vec![Value::I64(2), Value::I64(4)],
            vec![2],
        )
        .unwrap();

        assert_eq!(slice.shape, vec![2]);
        assert_eq!(slice.element_type(), ArrayElementType::I64);
        assert!(matches!(slice.get(&[1]).unwrap(), Value::I64(2)));
        assert!(matches!(slice.get(&[2]).unwrap(), Value::I64(4)));

        let empty = ArrayValue::memory_first_slice_from_values(&source, vec![], vec![0]).unwrap();
        assert_eq!(empty.shape, vec![0]);
        assert_eq!(empty.element_type(), ArrayElementType::I64);
    }

    #[test]
    fn memory_first_collect_values_widens_and_preserves_empty_type() {
        let ints = ArrayValue::memory_first_collect_values(
            vec![Value::I64(1), Value::I64(2)],
            ArrayElementType::Any,
        )
        .unwrap();
        assert_eq!(ints.shape, vec![2]);
        assert_eq!(ints.element_type(), ArrayElementType::I64);
        assert!(matches!(ints.get(&[2]).unwrap(), Value::I64(2)));

        let mixed_numeric = ArrayValue::memory_first_collect_values(
            vec![Value::I64(1), Value::F64(2.5)],
            ArrayElementType::Any,
        )
        .unwrap();
        assert_eq!(mixed_numeric.element_type(), ArrayElementType::F64);
        assert!(matches!(mixed_numeric.get(&[1]).unwrap(), Value::F64(1.0)));
        assert!(matches!(mixed_numeric.get(&[2]).unwrap(), Value::F64(2.5)));

        let typejoined_numeric = ArrayValue::memory_first_collect_typejoin_values(
            vec![Value::I64(1), Value::F64(2.5)],
            ArrayElementType::Any,
        )
        .unwrap();
        assert_eq!(
            typejoined_numeric.element_type(),
            ArrayElementType::Abstract("Real".to_string())
        );
        assert!(matches!(
            typejoined_numeric.get(&[1]).unwrap(),
            Value::I64(1)
        ));
        assert!(matches!(
            typejoined_numeric.get(&[2]).unwrap(),
            Value::F64(2.5)
        ));

        let empty = ArrayValue::memory_first_collect_values(vec![], ArrayElementType::I64).unwrap();
        assert_eq!(empty.shape, vec![0]);
        assert_eq!(empty.element_type(), ArrayElementType::I64);
    }

    #[test]
    fn try_data_f64_ok_on_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0], vec![3]);
        let data = arr.try_data_f64().unwrap();
        assert_eq!(data, &vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn try_data_f64_err_on_i64_array() {
        let arr = ArrayValue::from_i64(vec![1, 2, 3], vec![3]);
        assert!(arr.try_data_f64().is_err());
    }

    #[test]
    fn try_data_f64_err_on_bool_array() {
        let arr = ArrayValue::from_bool(vec![true, false], vec![2]);
        assert!(arr.try_data_f64().is_err());
    }

    #[test]
    fn try_data_f64_mut_ok_on_f64_array() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        let data = arr.try_data_f64_mut().unwrap();
        data[0] = 42.0;
        assert_eq!(arr.try_data_f64().unwrap()[0], 42.0);
    }

    #[test]
    fn try_data_f64_mut_err_on_i64_array() {
        let mut arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        assert!(arr.try_data_f64_mut().is_err());
    }

    #[test]
    fn try_data_i64_ok_on_i64_array() {
        let arr = ArrayValue::from_i64(vec![10, 20, 30], vec![3]);
        let data = arr.try_data_i64().unwrap();
        assert_eq!(data, &vec![10, 20, 30]);
    }

    #[test]
    fn try_data_i64_err_on_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.try_data_i64().is_err());
    }

    #[test]
    fn try_data_i64_mut_ok_on_i64_array() {
        let mut arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        let data = arr.try_data_i64_mut().unwrap();
        data[0] = 99;
        assert_eq!(arr.try_data_i64().unwrap()[0], 99);
    }

    #[test]
    fn try_data_i64_mut_err_on_f64_array() {
        let mut arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(arr.try_data_i64_mut().is_err());
    }

    #[test]
    fn try_data_bool_ok_on_bool_array() {
        let arr = ArrayValue::from_bool(vec![true, false, true], vec![3]);
        let data = arr.try_data_bool().unwrap();
        assert_eq!(data, &vec![true, false, true]);
    }

    #[test]
    fn try_data_bool_err_on_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(arr.try_data_bool().is_err());
    }

    #[test]
    fn try_as_f64_vec_ok_on_f64() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.5], vec![2]);
        let v = arr.try_as_f64_vec().unwrap();
        assert_eq!(v, vec![1.0, 2.5]);
    }

    #[test]
    fn try_as_f64_vec_ok_on_i64() {
        let arr = ArrayValue::from_i64(vec![3, 4], vec![2]);
        let v = arr.try_as_f64_vec().unwrap();
        assert_eq!(v, vec![3.0, 4.0]);
    }

    #[test]
    fn try_as_f64_vec_ok_on_bool() {
        let arr = ArrayValue::from_bool(vec![true, false], vec![2]);
        let v = arr.try_as_f64_vec().unwrap();
        assert_eq!(v, vec![1.0, 0.0]);
    }

    #[test]
    fn try_as_f64_vec_err_on_string_array() {
        let arr = ArrayValue::new(ArrayData::String(vec![Value::str_new("hello")]), vec![1]);
        assert!(arr.try_as_f64_vec().is_err());
    }

    #[test]
    fn try_as_f64_vec_err_on_any_array() {
        let arr = ArrayValue::any_vector(vec![Value::Nothing]);
        assert!(arr.try_as_f64_vec().is_err());
    }
}
