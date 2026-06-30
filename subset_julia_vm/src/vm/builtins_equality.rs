//! Equality builtin functions for the VM.
//!
//! Object identity and equality: ===, isequal, hash.

use crate::vm::value::is_native_array_value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_utils::{normalize_struct_name, type_objects_equal};
use super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, ArrayValue, MemoryRef,
    StructInstance, Value,
};
use super::Vm;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum NumericInteger {
    NonNegative(u128),
    Negative(i128),
}

fn numeric_integer_value(value: &Value) -> Option<NumericInteger> {
    match value {
        Value::I8(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I16(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I32(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I64(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I128(v) => Some(signed_integer_value(*v)),
        Value::U8(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U16(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U32(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U64(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U128(v) => Some(NumericInteger::NonNegative(*v)),
        _ => None,
    }
}

fn signed_integer_value(value: i128) -> NumericInteger {
    if value >= 0 {
        NumericInteger::NonNegative(value.cast_unsigned())
    } else {
        NumericInteger::Negative(value)
    }
}

fn integer_values_isequal(left: &Value, right: &Value) -> Option<bool> {
    Some(numeric_integer_value(left)? == numeric_integer_value(right)?)
}

fn integer_values_identical(left: &Value, right: &Value) -> Option<bool> {
    let left_integer = numeric_integer_value(left)?;
    let right_integer = numeric_integer_value(right)?;
    Some(
        std::mem::discriminant(left) == std::mem::discriminant(right)
            && left_integer == right_integer,
    )
}

fn array_linear_value(arr: &ArrayValue, index: usize) -> Option<Value> {
    arr.get_linear(index).ok()
}

#[derive(Clone, Copy)]
struct RangeEqualityView {
    start: f64,
    step: f64,
    len: i64,
}

fn struct_base_name(name: &str) -> &str {
    let stripped = normalize_struct_name(name);
    stripped.split_once('{').map_or(stripped, |(base, _)| base)
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::I8(v) => Some(i64::from(*v)),
        Value::I16(v) => Some(i64::from(*v)),
        Value::I32(v) => Some(i64::from(*v)),
        Value::I64(v) => Some(*v),
        Value::U8(v) => Some(i64::from(*v)),
        Value::U16(v) => Some(i64::from(*v)),
        Value::U32(v) => Some(i64::from(*v)),
        Value::U64(v) => i64::try_from(*v).ok(),
        _ => None,
    }
}

fn range_equality_view(value: &Value) -> Option<RangeEqualityView> {
    match value {
        Value::Range(range) => Some(RangeEqualityView {
            start: range.start,
            step: range.step,
            len: range.length(),
        }),
        Value::Struct(instance) if struct_base_name(&instance.struct_name) == "OneTo" => {
            let stop = value_as_i64(instance.values.first()?)?;
            Some(RangeEqualityView {
                start: 1.0,
                step: 1.0,
                len: stop.max(0),
            })
        }
        _ => None,
    }
}

fn range_views_equal(left: RangeEqualityView, right: RangeEqualityView) -> bool {
    if left.len != right.len {
        return false;
    }
    if left.len <= 0 {
        return true;
    }
    if left.start != right.start {
        return false;
    }
    left.len == 1 || left.step == right.step
}

fn range_like_values_equal(left: &Value, right: &Value) -> Option<bool> {
    Some(range_views_equal(
        range_equality_view(left)?,
        range_equality_view(right)?,
    ))
}

fn values_isequal(a: Option<Value>, b: Option<Value>) -> bool {
    match (a, b) {
        (Some(ref x), Some(ref y)) if integer_values_isequal(x, y).is_some() => {
            integer_values_isequal(x, y).unwrap_or(false)
        }
        (Some(Value::I64(x)), Some(Value::I64(y))) => x == y,
        (Some(Value::F16(x)), Some(Value::F16(y))) => {
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
        }
        (Some(Value::F32(x)), Some(Value::F32(y))) => {
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
        }
        (Some(Value::F64(x)), Some(Value::F64(y))) => {
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
        }
        (Some(ref x), Some(ref y))
            if crate::vm::numeric_identity::mixed_int_float_values_equal(x, y).is_some() =>
        {
            // Value-based mixed integer/float equality, no rounding of the integer
            // (Issue #8187, all widths in #8199), with `isequal`'s sign-of-zero
            // distinction: an integer is always +0, so `isequal(0, -0.0)` is
            // false even though `0 == -0.0`.
            crate::vm::numeric_identity::mixed_int_float_values_equal(x, y).unwrap_or(false)
                && !crate::vm::numeric_identity::is_negative_zero_fixed_float(x)
                && !crate::vm::numeric_identity::is_negative_zero_fixed_float(y)
        }
        (Some(Value::Tuple(x)), Some(Value::Tuple(y)))
        | (Some(Value::SimpleVector(x)), Some(Value::SimpleVector(y))) => {
            x.elements.len() == y.elements.len()
                && x.elements
                    .iter()
                    .zip(y.elements.iter())
                    .all(|(xv, yv)| values_isequal(Some(xv.clone()), Some(yv.clone())))
        }
        // Immutable structs (e.g. `ComplexF64` array elements) compare by
        // structural value, not by the `Debug`-string fallback below: a literal
        // array (native carrier) and a broadcast/copy result (Memory carrier)
        // materialize equal `Complex` elements with differing internal
        // representations, so their `Debug` strings differed and the array
        // compared unequal (Issue #5789). Compare fields recursively.
        (Some(Value::Struct(x)), Some(Value::Struct(y))) => {
            crate::vm::type_utils::normalize_struct_name(&x.struct_name)
                == crate::vm::type_utils::normalize_struct_name(&y.struct_name)
                && x.values.len() == y.values.len()
                && x.values
                    .iter()
                    .zip(y.values.iter())
                    .all(|(xv, yv)| values_isequal(Some(xv.clone()), Some(yv.clone())))
        }
        (Some(x), Some(y)) => format!("{:?}", x) == format!("{:?}", y),
        _ => false,
    }
}

/// Coerce a numeric `Value` to a `RustBigFloat` at the active precision so a
/// BigFloat can be compared by value against any other numeric (Issue #6892).
/// Mirrors the coercion in `StackOps::pop_bigfloat`; returns `None` for
/// non-numeric values.
fn value_to_bigfloat(value: &Value) -> Option<super::value::RustBigFloat> {
    use super::value::{get_bigfloat_precision, RustBigFloat};
    let prec = get_bigfloat_precision();
    let from_f64 = |v: f64| Some(RustBigFloat::from_f64(v, prec));
    match value {
        Value::BigFloat(v) => Some(v.clone()),
        Value::F64(v) => from_f64(*v),
        Value::F32(v) => from_f64(*v as f64),
        Value::F16(v) => from_f64(v.to_f64()),
        Value::Bool(v) => from_f64(if *v { 1.0 } else { 0.0 }),
        Value::I8(v) => from_f64(*v as f64),
        Value::I16(v) => from_f64(*v as f64),
        Value::I32(v) => from_f64(*v as f64),
        Value::I64(v) => from_f64(*v as f64),
        Value::I128(v) => from_f64(*v as f64),
        Value::U8(v) => from_f64(*v as f64),
        Value::U16(v) => from_f64(*v as f64),
        Value::U32(v) => from_f64(*v as f64),
        Value::U64(v) => from_f64(*v as f64),
        Value::U128(v) => from_f64(*v as f64),
        Value::BigInt(v) => {
            use std::str::FromStr;
            from_f64(f64::from_str(&v.to_string()).unwrap_or(0.0))
        }
        _ => None,
    }
}

/// Tri-state result of `==` on two values: `Some(true)` / `Some(false)` for a
/// definite answer, `None` for `missing` (Julia's three-valued `==`). Used by
/// the `TupleEquals` builtin so tuple/named-tuple `==` folds `==` (not
/// `isequal`) over elements (Issue #5267): `0.0 == -0.0` is `true`,
/// `NaN == NaN` is `false`, and any element comparing `missing` propagates.
fn values_equal_tristate(a: &Value, b: &Value) -> Option<bool> {
    // `missing == anything` is `missing`.
    if matches!(a, Value::Missing) || matches!(b, Value::Missing) {
        return None;
    }

    // Integer/integer (including mixed signed/unsigned widths) by value.
    if let Some(eq) = integer_values_equal_by_value(a, b) {
        return Some(eq);
    }

    match (a, b) {
        // Floating point: `==` semantics — `0.0 == -0.0` is true, `NaN == NaN`
        // is false. (Plain `==` on the raw f64/f32/f16, not bit comparison.)
        (Value::F64(x), Value::F64(y)) => Some(x == y),
        (Value::F32(x), Value::F32(y)) => Some(x == y),
        (Value::F16(x), Value::F16(y)) => Some(x == y),
        // Mixed integer/float: value-based, no rounding of the integer to the
        // float type (Issue #8187, generalized to every Int*/UInt* × Float16/32/64
        // width in #8199). Catches both orders and never promotes the integer.
        (x, y) if crate::vm::numeric_identity::mixed_int_float_values_equal(x, y).is_some() => {
            crate::vm::numeric_identity::mixed_int_float_values_equal(x, y)
        }
        (Value::Bool(x), Value::Bool(y)) => Some(x == y),
        (Value::Str(x), Value::Str(y)) => Some(x == y),
        (Value::Char(x), Value::Char(y)) => Some(x == y),
        (Value::Nothing, Value::Nothing) => Some(true),
        (Value::DataType(x), Value::DataType(y)) => Some(type_objects_equal(x, y)),
        (Value::RuntimeTypeVar(x), Value::RuntimeTypeVar(y)) => Some(x.id == y.id),
        (Value::RuntimeTypeName(x), Value::RuntimeTypeName(y)) => Some(x.name == y.name),
        (x, y) if range_like_values_equal(x, y).is_some() => range_like_values_equal(x, y),
        // Nested tuples / svec fold `==` recursively.
        (Value::Tuple(x), Value::Tuple(y)) | (Value::SimpleVector(x), Value::SimpleVector(y)) => {
            tuple_elements_equal_tristate(&x.elements, &y.elements)
        }
        // Nested named tuples: equal iff same field names in order and each
        // value `==` (mirrors upstream `Tuple(a) == Tuple(b)` after name check).
        (Value::NamedTuple(x), Value::NamedTuple(y)) => {
            if x.names != y.names {
                return Some(false);
            }
            tuple_elements_equal_tristate(&x.values, &y.values)
        }
        // BigFloat mixed with any numeric (BigFloat/Float/Int): promote both to
        // BigFloat and compare by value, mirroring scalar `==` (Issue #6892).
        // Without this, tuple `==` fell through to the `Debug`-string fallback
        // below for BigFloat-vs-Float64/Int elements, so `(big(2.0),) == (2.0,)`
        // returned `false` even though scalar `big(2.0) == 2.0` is `true`.
        (a, b) if matches!(a, Value::BigFloat(_)) || matches!(b, Value::BigFloat(_)) => {
            match (value_to_bigfloat(a), value_to_bigfloat(b)) {
                (Some(x), Some(y)) => Some(matches!(x.cmp(&y), Some(0))),
                _ => Some(values_isequal(Some(a.clone()), Some(b.clone()))),
            }
        }
        // Anything else: fall back to `isequal`'s element semantics. For
        // non-float, non-missing values `==` and `isequal` agree, so this keeps
        // structs / symbols / etc. behaving as before while the float/NaN edge
        // cases above are handled with `==` semantics.
        _ => Some(values_isequal(Some(a.clone()), Some(b.clone()))),
    }
}

/// Fold `==` over two element slices with `missing` propagation, matching
/// upstream `Base._eq` on tuples: a definite `false` short-circuits to
/// `Some(false)`; otherwise any `missing` element makes the whole comparison
/// `missing` (`None`); all-equal yields `Some(true)`.
fn tuple_elements_equal_tristate(xs: &[Value], ys: &[Value]) -> Option<bool> {
    if xs.len() != ys.len() {
        return Some(false);
    }
    let mut any_missing = false;
    for (x, y) in xs.iter().zip(ys.iter()) {
        match values_equal_tristate(x, y) {
            Some(false) => return Some(false),
            None => any_missing = true,
            Some(true) => {}
        }
    }
    if any_missing {
        None
    } else {
        Some(true)
    }
}

/// `true` when `value` contains a heap struct reference (`Value::StructRef`)
/// anywhere reachable through tuple/svec/named-tuple elements or inline struct
/// fields. Used to keep the common all-primitive tuple `==` path allocation-free
/// (Issue #6685): the `resolve_structrefs_deep` snapshot below is only taken when
/// a heap struct ref is actually present. The walk never *follows* a `StructRef`
/// into the heap (it returns `true` on sight), so it is cheap and cannot loop.
fn contains_structref(value: &Value) -> bool {
    match value {
        Value::StructRef(_) => true,
        Value::Struct(inst) => inst.values.iter().any(contains_structref),
        Value::Tuple(t) | Value::SimpleVector(t) => t.elements.iter().any(contains_structref),
        Value::NamedTuple(nt) => nt.values.iter().any(contains_structref),
        _ => false,
    }
}

/// Recursively replace heap `Value::StructRef`s with inline `Value::Struct`
/// snapshots resolved against `struct_heap`, so the structural `==` fold
/// (`values_equal_tristate` / `values_isequal`) compares struct elements by
/// value instead of by heap index (Issue #6685). Before this, two separately
/// constructed but equal structs inside tuples — e.g. `(OneTo(3),)` vs
/// `(OneTo(3),)` — held distinct heap indices and the fold compared their
/// `Debug` strings, yielding `false`. Recurses through tuple / svec /
/// named-tuple elements and struct fields so nested ranges/structs also compare
/// by value. `visiting` records the heap indices currently being resolved so a
/// cyclic mutable struct keeps its `StructRef` (compared by identity) instead of
/// looping forever.
fn resolve_structrefs_deep(
    value: &Value,
    struct_heap: &[StructInstance],
    visiting: &mut Vec<usize>,
) -> Value {
    match value {
        Value::StructRef(idx) => {
            if visiting.contains(idx) {
                return value.clone();
            }
            match struct_heap.get(*idx) {
                Some(inst) => {
                    visiting.push(*idx);
                    let mut resolved = inst.clone();
                    for field in resolved.values.iter_mut() {
                        *field = resolve_structrefs_deep(field, struct_heap, visiting);
                    }
                    visiting.pop();
                    Value::Struct(resolved)
                }
                None => value.clone(),
            }
        }
        Value::Struct(inst) => {
            let mut resolved = inst.clone();
            for field in resolved.values.iter_mut() {
                *field = resolve_structrefs_deep(field, struct_heap, visiting);
            }
            Value::Struct(resolved)
        }
        Value::Tuple(t) => {
            let mut resolved = t.clone();
            for elem in resolved.elements.iter_mut() {
                *elem = resolve_structrefs_deep(elem, struct_heap, visiting);
            }
            Value::Tuple(resolved)
        }
        Value::SimpleVector(t) => {
            let mut resolved = t.clone();
            for elem in resolved.elements.iter_mut() {
                *elem = resolve_structrefs_deep(elem, struct_heap, visiting);
            }
            Value::SimpleVector(resolved)
        }
        Value::NamedTuple(nt) => {
            let mut resolved = nt.clone();
            for v in resolved.values.iter_mut() {
                *v = resolve_structrefs_deep(v, struct_heap, visiting);
            }
            Value::NamedTuple(resolved)
        }
        _ => value.clone(),
    }
}

/// The single canonical entry point for resolving heap struct refs at a native
/// value-op boundary (Issue #6694). Every native op that compares or hashes a
/// `Value` by *structure* — `==` over tuples/named-tuples (`TupleEquals`),
/// `hash`/`_hash`, `in`/`∈` membership, and `===` over **immutable** structs —
/// MUST route its operands through this before the structural fold, so a heap
/// `Value::StructRef(idx)` is never compared/hashed by its heap index (the
/// #6685 / #6691 / #6693 / #6709 bug class: a separately-constructed but equal
/// immutable struct holds a different index and the `Debug`/identity fold then
/// reports them unequal).
///
/// Allocates ONLY when a heap ref is actually reachable; the all-primitive hot
/// path returns `value` untouched (important per the VM-perf priority). Takes
/// the value by ownership so the hot-path passthrough is move-only (no clone);
/// borrowed callers use [`resolved_value_op_structrefs`].
pub(crate) fn resolve_value_op_structrefs(value: Value, struct_heap: &[StructInstance]) -> Value {
    if contains_structref(&value) {
        resolve_structrefs_deep(&value, struct_heap, &mut Vec::new())
    } else {
        value
    }
}

/// Borrowing variant of [`resolve_value_op_structrefs`]: returns a `Cow` that
/// borrows on the all-primitive hot path (no clone) and owns the resolved
/// snapshot only when a heap ref is present. Used by membership, which holds its
/// operands by reference.
fn resolved_value_op_structrefs<'a>(
    value: &'a Value,
    struct_heap: &[StructInstance],
) -> std::borrow::Cow<'a, Value> {
    if contains_structref(value) {
        std::borrow::Cow::Owned(resolve_structrefs_deep(value, struct_heap, &mut Vec::new()))
    } else {
        std::borrow::Cow::Borrowed(value)
    }
}

/// Value `==` for container membership (`in` / `∈`), Issue #6691. Resolves heap
/// struct refs (building on the #6685 tuple-`==` fix) and folds `==` semantics,
/// collapsing the tri-state to a definite bool (`missing` or not-equal → false).
/// The native `In` builtin's ad-hoc scalar comparison cannot compare tuple /
/// named-tuple / struct elements, so such elements previously fell through to
/// `false` — even `(1, 2) in [(1, 2)]`. The `In` builtin routes its non-scalar
/// fallback here so tuples / named tuples / structs compare by value.
pub(crate) fn values_equal_for_membership(
    a: &Value,
    b: &Value,
    struct_heap: &[StructInstance],
) -> bool {
    let ra = resolved_value_op_structrefs(a, struct_heap);
    let rb = resolved_value_op_structrefs(b, struct_heap);
    values_equal_tristate(&ra, &rb) == Some(true)
}

/// Integer-by-value equality across mixed signed/unsigned integer widths, or
/// `None` when either operand is not an integer value. Unlike
/// `integer_values_isequal`, this is purely numeric (it shares the same
/// implementation; integers have identical `==` and `isequal` semantics).
fn integer_values_equal_by_value(left: &Value, right: &Value) -> Option<bool> {
    integer_values_isequal(left, right)
}

/// Logical (values, shape) view of an Array/Memory native container, routed
/// through `ArrayValue` logical helpers (Issue #3908). Returns `None` for
/// non array-like values so the caller can fall through to other arms.
fn array_like_logical_view(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Option<(Vec<Value>, Vec<usize>)> {
    // Route the native Array arm through `native_array_value_ref` so the
    // unwrap stays centralized while #3908 retires the variant.
    if let Some(arr) = native_array_value_ref(value) {
        let arr = arr.borrow();
        let shape = arr.shape.clone();
        // `to_logical_value_vec` preserves reshape/Complex/struct-ref
        // semantics so equality matches Julia for shared-backing arrays.
        let values = arr.to_logical_value_vec().ok()?;
        return Some((values, shape));
    }
    if let Some(arr) = array_wrapper_value_to_array_value(value, struct_heap)
        .ok()
        .flatten()
    {
        let shape = arr.shape.clone();
        let values = arr.to_logical_value_vec().ok()?;
        return Some((values, shape));
    }
    match value {
        Value::Memory(mem) => {
            let mem = mem.borrow();
            let len = mem.len();
            let values: Option<Vec<Value>> = (0..len).map(|i| mem.data.get_value(i)).collect();
            values.map(|v| (v, vec![len]))
        }
        // Issue #8132: StaticArrays (`SVector`/`SMatrix`) are `AbstractArray`
        // subtypes carried in the flat `StaticArray`/`StaticArrayInline` reprs,
        // not the native array carrier. Expose their column-major element view
        // (matching `Array`'s logical layout) so `isequal`/`==` compare a
        // StaticArray element-wise against a native `Vector`/`Matrix` (or another
        // StaticArray), instead of falling back to object identity.
        Value::StaticArray(sv) => {
            let shape = if sv.is_vector() {
                vec![sv.rows]
            } else {
                vec![sv.rows, sv.cols]
            };
            Some((sv.elems.to_values(), shape))
        }
        Value::StaticArrayInline(sv) => {
            let shape = if sv.is_vector() {
                vec![sv.rows()]
            } else {
                vec![sv.rows(), sv.cols()]
            };
            let values: Vec<Value> = (0..sv.len()).map(|i| sv.get_0indexed(i)).collect();
            Some((values, shape))
        }
        _ => None,
    }
}

fn is_memory_value(value: &Value) -> bool {
    matches!(value, Value::Memory(_))
}

/// Isequal comparison routed through `ArrayValue` logical helpers for any
/// pair involving a native multi-dimensional array container. Pure
/// Memory/Memory pairs return `None` so the caller keeps the dedicated
/// `isequal_contents` fast path that preserves bitwise float semantics.
fn try_isequal_array_like(
    left: &Value,
    right: &Value,
    struct_heap: &[StructInstance],
) -> Option<bool> {
    if is_memory_value(left) && is_memory_value(right) {
        return None;
    }
    let (l_values, l_shape) = array_like_logical_view(left, struct_heap)?;
    let (r_values, r_shape) = array_like_logical_view(right, struct_heap)?;
    if l_shape != r_shape || l_values.len() != r_values.len() {
        return Some(false);
    }
    Some(
        l_values
            .into_iter()
            .zip(r_values)
            .all(|(a, b)| values_isequal(Some(a), Some(b))),
    )
}

fn hash_array_values(arr: &ArrayValue, hasher: &mut DefaultHasher) {
    for i in 0..arr.len() {
        if let Some(v) = array_linear_value(arr, i) {
            format!("{:?}", v).hash(hasher);
        }
    }
}

fn hash_memory_values(mem: &MemoryRef, hasher: &mut DefaultHasher) {
    let mem = mem.borrow();
    for i in 0..mem.len() {
        if let Some(v) = mem.data.get_value(i) {
            format!("{:?}", v).hash(hasher);
        }
    }
}

/// Hash a native multi-dimensional array container by routing through
/// `ArrayValue` / Memory logical accessors (Issue #3908). Returns `true`
/// when the value was handled so the caller can skip the legacy match arms.
fn try_hash_array_like(
    value: &Value,
    hasher: &mut DefaultHasher,
    struct_heap: &[StructInstance],
) -> bool {
    // Route the native Array arm through `native_array_value_ref` so the
    // unwrap stays centralized while #3908 retires the variant.
    if let Some(arr) = native_array_value_ref(value) {
        hash_array_values(&arr.borrow(), hasher);
        return true;
    }
    if let Some(arr) = array_wrapper_value_to_array_value(value, struct_heap)
        .ok()
        .flatten()
    {
        hash_array_values(&arr, hasher);
        return true;
    }
    match value {
        Value::Memory(mem) => {
            hash_memory_values(mem, hasher);
            true
        }
        _ => false,
    }
}

impl<R: RngLike> Vm<R> {
    /// Whether the heap struct at `idx` belongs to a `mutable struct` type.
    /// Parametric immutable structs (e.g. `OneTo{T}`) are absent from
    /// `struct_defs`, so an unknown `type_id` correctly defaults to immutable
    /// (`false`) — matching the field-assign mutability check in `struct_ops`.
    /// Used by `===` (`Egal`) to give immutable heap structs value identity
    /// while keeping mutable structs reference-identity (Issue #6709).
    pub(crate) fn heap_struct_is_mutable(&self, idx: usize) -> bool {
        self.struct_heap
            .get(idx)
            .map(|s| s.type_id)
            .and_then(|tid| self.struct_defs.get(tid))
            .map(|def| def.is_mutable)
            .unwrap_or(false)
    }

    /// Recursively resolve heap `StructRef`s to inline `Value::Struct`
    /// snapshots, but ONLY for IMMUTABLE structs — a mutable struct keeps its
    /// `StructRef` so `===` (egal) retains reference identity for it (Issue
    /// #6709). Recurses through tuple / svec / named-tuple elements and struct
    /// fields, so a tuple of immutable structs (`(OneTo(3),)`) compares by value
    /// while a tuple of mutable structs keeps per-element identity. `visiting`
    /// guards against cyclic mutable references (kept as `StructRef`). Unlike the
    /// value-op resolver [`resolve_value_op_structrefs`] (which resolves *all*
    /// structs for `==`/`hash`/`in`), this is mutability-aware because `===`
    /// distinguishes the two.
    fn resolve_immutable_structrefs(&self, value: &Value, visiting: &mut Vec<usize>) -> Value {
        match value {
            Value::StructRef(idx) => {
                if self.heap_struct_is_mutable(*idx) || visiting.contains(idx) {
                    return value.clone();
                }
                match self.struct_heap.get(*idx) {
                    Some(inst) => {
                        visiting.push(*idx);
                        let mut resolved = inst.clone();
                        for field in resolved.values.iter_mut() {
                            *field = self.resolve_immutable_structrefs(field, visiting);
                        }
                        visiting.pop();
                        Value::Struct(resolved)
                    }
                    None => value.clone(),
                }
            }
            Value::Struct(inst) => {
                let mut resolved = inst.clone();
                for field in resolved.values.iter_mut() {
                    *field = self.resolve_immutable_structrefs(field, visiting);
                }
                Value::Struct(resolved)
            }
            Value::Tuple(t) => {
                let mut resolved = t.clone();
                for elem in resolved.elements.iter_mut() {
                    *elem = self.resolve_immutable_structrefs(elem, visiting);
                }
                Value::Tuple(resolved)
            }
            Value::SimpleVector(t) => {
                let mut resolved = t.clone();
                for elem in resolved.elements.iter_mut() {
                    *elem = self.resolve_immutable_structrefs(elem, visiting);
                }
                Value::SimpleVector(resolved)
            }
            Value::NamedTuple(nt) => {
                let mut resolved = nt.clone();
                for v in resolved.values.iter_mut() {
                    *v = self.resolve_immutable_structrefs(v, visiting);
                }
                Value::NamedTuple(resolved)
            }
            _ => value.clone(),
        }
    }

    /// Whether `value` is an `AbstractArray`-subtype struct that the array view
    /// helpers (`array_like_logical_view`) cannot read element-wise — a user
    /// `struct <: AbstractArray` (generic struct ref) or a `SubArray` view —
    /// so an `isequal`/`==` against it must fall back to Pure-Julia dispatch.
    ///
    /// A native `Array` carrier is itself a struct ref (`Vector{T} <:
    /// AbstractArray`), but `array_like_logical_view` CAN read it, so it is
    /// excluded here and keeps its dedicated Rust fast path. Without that
    /// exclusion this predicate fired for native arrays too, and a
    /// `scalar == native-array` comparison (e.g. `5 == [1,2,3]` reached via an
    /// `Any`-typed operand) wrongly entered dispatch and raised a MethodError
    /// instead of returning `false` (Issue #8229).
    fn value_is_unreadable_abstractarray(&self, value: &Value) -> bool {
        if !matches!(value, Value::Struct(_) | Value::StructRef(_)) {
            return false;
        }
        if array_like_logical_view(value, &self.struct_heap).is_some() {
            return false;
        }
        let type_name = self.get_type_name(value);
        self.check_subtype(&type_name, "AbstractArray")
    }

    /// Execute equality builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not an equality builtin.
    pub(super) fn execute_builtin_equality(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::Egal => {
                // === (object identity)
                // For primitives: value equality
                // For reference types (Array, Dict, mutable struct): reference identity
                // Floating-point primitives compare by bit identity:
                // NaN === NaN is true for identical payloads, while -0.0 !== 0.0.
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // Resolve IMMUTABLE heap structs to inline snapshots so `===`
                // compares them by value (an immutable `OneTo`/`Pt` at distinct
                // heap slots is still `===`); MUTABLE structs keep their
                // `StructRef` so they retain reference identity (Issue #6709).
                // After this, the `StructRef` arm only sees mutable structs and
                // the `Struct`/`Tuple`/`SimpleVector` arms compare immutable
                // struct elements by value. No-op when no heap refs are present.
                let left = if contains_structref(&left) {
                    self.resolve_immutable_structrefs(&left, &mut Vec::new())
                } else {
                    left
                };
                let right = if contains_structref(&right) {
                    self.resolve_immutable_structrefs(&right, &mut Vec::new())
                } else {
                    right
                };

                let is_identical = if let Some(is_integer_identical) =
                    integer_values_identical(&left, &right)
                {
                    is_integer_identical
                } else {
                    match (&left, &right) {
                        // Primitives: value equality
                        (Value::I64(a), Value::I64(b)) => a == b,
                        (Value::F16(a), Value::F16(b)) => a.to_bits() == b.to_bits(),
                        (Value::F32(a), Value::F32(b)) => a.to_bits() == b.to_bits(),
                        (Value::F64(a), Value::F64(b)) => {
                            // === checks bit identity: NaN === NaN is true, -0.0 === 0.0 is false
                            a.to_bits() == b.to_bits()
                        }
                        (Value::Bool(a), Value::Bool(b)) => a == b,
                        (Value::Str(a), Value::Str(b)) => a == b,
                        (Value::Char(a), Value::Char(b)) => a == b,
                        (Value::Nothing, Value::Nothing) => true,
                        (Value::Missing, Value::Missing) => true,
                        // The `Undef` sentinel is used internally to mark an
                        // omitted optional keyword argument whose default is
                        // re-evaluated in the body (Issue #5121). The injected
                        // `kw === Undef` guard relies on this arm; without it
                        // `Undef === Undef` would fall through to `false`. Users
                        // cannot construct `Undef`, so this only affects the
                        // synthetic guard.
                        (Value::Undef, Value::Undef) => true,

                        // BigInt / BigFloat are heap-object-like numeric
                        // values in upstream Julia, so `===` observes
                        // reference identity, not numeric equality
                        // (Issue #4886). `==` / `isequal` keep value
                        // semantics through their separate arms.
                        (Value::BigInt(a), Value::BigInt(b)) => a.ptr_eq(b),
                        (Value::BigFloat(a), Value::BigFloat(b)) => a.ptr_eq(b),

                        // Symbols: name equality (symbols are interned)
                        (Value::Symbol(a), Value::Symbol(b)) => a == b,

                        // Modules: identity by module name (Issue #4959).
                        // Modules are singletons in Julia, so `Base === Base`
                        // is true while `Base === Core` is false.
                        (Value::Module(a), Value::Module(b)) => a.name == b.name,

                        // Generic functions are singleton objects: a function
                        // is `===` to itself no matter how the value was
                        // produced — a direct reference, stored and re-read
                        // through a struct field, or returned from a HOF — so
                        // `ff === ff` and `Box(ff).f === ff` are both true,
                        // while distinct functions (`ff === gg`) are not
                        // (Issue #7993). The stable identity is the (possibly
                        // module-qualified) function name; `candidate_indices`
                        // is only a cached dispatch hint and must NOT take part
                        // in identity, because the same function can be
                        // referenced once with it (`PushResolvedFunction`) and
                        // once without (`PushFunction`).
                        (Value::Function(a), Value::Function(b)) => a.name == b.name,

                        // Reference types: check if same reference (by index/pointer)
                        // Arrays: same reference = same object. Route both
                        // destructures through `native_array_value_ref` so the
                        // unwrap stays centralized while #3908 retires the
                        // native Array variant; the outer match's `_ => false`
                        // arm preserves exhaustiveness for non-array operands.
                        (a, b) if is_native_array_value(a) && is_native_array_value(b) => {
                            match (native_array_value_ref(a), native_array_value_ref(b)) {
                                (Some(la), Some(lb)) => std::ptr::eq(la.as_ptr(), lb.as_ptr()),
                                _ => false,
                            }
                        }
                        // Memory: same reference = same object
                        (Value::Memory(a), Value::Memory(b)) => {
                            std::ptr::eq(a.as_ptr(), b.as_ptr())
                        }

                        // Mutable structs: same reference = same object.
                        // (Immutable heap structs were already resolved to
                        // inline `Value::Struct` snapshots before this match,
                        // so only mutable ones reach here — Issue #6709.)
                        (Value::StructRef(a), Value::StructRef(b)) => a == b,

                        // Immutable structs: structural equality (all fields ===)
                        // For simplicity, compare struct_name and all values by Debug representation
                        // NOTE: We normalize struct names to handle module-qualified vs unqualified names
                        // e.g., "MyGeometry.Point{Int64}" should equal "Point{Int64}"
                        (Value::Struct(a), Value::Struct(b)) => {
                            normalize_struct_name(&a.struct_name)
                                == normalize_struct_name(&b.struct_name)
                                && a.values.len() == b.values.len()
                                && format!("{:?}", a.values) == format!("{:?}", b.values)
                        }

                        // Ranges are immutable, so `===` is structural (a
                        // `UnitRange`/`StepRange` is `===` another with the same
                        // fields): `(1:5) === (1:5)` and `(1:2:9) === (1:2:9)` are
                        // true (Issue #5666). Endpoints/step are compared bitwise
                        // to mirror float `===`.
                        (Value::Range(a), Value::Range(b)) => {
                            a.start.to_bits() == b.start.to_bits()
                                && a.step.to_bits() == b.step.to_bits()
                                && a.stop.to_bits() == b.stop.to_bits()
                                && a.is_float == b.is_float
                                && a.element_type == b.element_type
                        }

                        // Tuples: structural equality (compare Debug representation)
                        (Value::Tuple(a), Value::Tuple(b)) => {
                            a.elements.len() == b.elements.len()
                                && format!("{:?}", a.elements) == format!("{:?}", b.elements)
                        }

                        // Core.SimpleVector (svec): structural identity (Issue #4722).
                        // Upstream Julia gives svec `===` by-content semantics, so
                        // `Core.svec(1,2) === Core.svec(1,2)` is `true`. Mirror the
                        // Tuple arm above: compare element Debug representations.
                        (Value::SimpleVector(a), Value::SimpleVector(b)) => {
                            a.elements.len() == b.elements.len()
                                && format!("{:?}", a.elements) == format!("{:?}", b.elements)
                        }

                        // Expr: structural equality (head and args)
                        (Value::Expr(a), Value::Expr(b)) => {
                            let a_args = a.args_snapshot();
                            let b_args = b.args_snapshot();
                            a.head == b.head
                                && a_args.len() == b_args.len()
                                && format!("{:?}", a_args) == format!("{:?}", b_args)
                        }

                        // DataType: type identity by normalized name
                        // e.g., typeof(p) === Point{Int} should match Point{Int64}
                        (Value::DataType(a), Value::DataType(b)) => type_objects_equal(a, b),
                        (Value::RuntimeTypeVar(a), Value::RuntimeTypeVar(b)) => a.id == b.id,
                        (Value::RuntimeTypeName(a), Value::RuntimeTypeName(b)) => a.name == b.name,

                        // Issue #4915: `===` on two QuoteNode values fell
                        // through to `_ => false`, so `QuoteNode(:x) === QuoteNode(:x)`
                        // returned false even for structurally identical
                        // payloads. Compare the wrapped inner values
                        // structurally (mirrors the `Expr` arm above —
                        // QuoteNode is also a wrapper around a value).
                        (Value::QuoteNode(a), Value::QuoteNode(b)) => {
                            format!("{:?}", a) == format!("{:?}", b)
                        }

                        // Enum values are immutable bits-types: identity is
                        // type + integer value (Issue #5139). `red === red`,
                        // `Color(2) === blue`.
                        (
                            Value::Enum {
                                type_name: ta,
                                value: va,
                            },
                            Value::Enum {
                                type_name: tb,
                                value: vb,
                            },
                        ) => ta == tb && va == vb,

                        // Different types: not identical
                        _ => false,
                    }
                };

                self.stack.push(Value::Bool(is_identical));
            }

            BuiltinId::Isequal => {
                // isequal(x, y) - NaN-aware equality
                // isequal(NaN, NaN) is true (unlike ==)
                // isequal(-0.0, 0.0) is false (unlike ==)
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                if let Some(is_equal) =
                    self.compare_array_wrapper_boundary_values_equal(&left, &right)
                {
                    self.stack.push(Value::Bool(is_equal));
                    return Ok(Some(()));
                }

                // Route native Array vs Array / Memory comparisons through
                // ArrayValue logical helpers (Issue #3908) so reshape/Complex/
                // struct-ref storage stays correct while we retire the raw
                // array pattern matches from this file.
                if let Some(is_equal) = try_isequal_array_like(&left, &right, &self.struct_heap) {
                    self.stack.push(Value::Bool(is_equal));
                    return Ok(Some(()));
                }

                // Issue #8229: an operand that is an `AbstractArray` subtype the
                // view helpers cannot read — a user `struct <: AbstractArray`
                // (generic struct ref) or a `SubArray` view — must still
                // element-compare instead of falling through to the
                // object-identity struct arm below (which returns `false`).
                // Dispatch to the Pure-Julia `isequal(::AbstractArray,
                // ::AbstractArray)` method (base/abstractarray.jl), which reads
                // the operands through their `size`/`getindex` protocol. Gated on
                // an actually-matching method, so a non-array partner
                // (`isequal(v, 5)`) keeps the object-identity result below and the
                // fallback cannot recurse — the method only re-enters `isequal`
                // on the scalar elements, which take the fast arms above.
                if self.value_is_unreadable_abstractarray(&left)
                    || self.value_is_unreadable_abstractarray(&right)
                {
                    let args = vec![left.clone(), right.clone()];
                    // `find_best_method_index` resolves to a real Pure-Julia
                    // method (the `isequal(::AbstractArray, ::AbstractArray)`
                    // entry), never back to this builtin, so re-dispatching it
                    // cannot recurse on the whole-array pair. A non-array partner
                    // (`isequal(v, 5)`) matches no `isequal` method, leaving the
                    // object-identity arms below to answer.
                    if let Some(func_index) =
                        self.find_best_method_index(&["isequal", "Base.isequal"], &args)
                    {
                        self.start_function_call(func_index, args)?;
                        return Ok(Some(()));
                    }
                }

                // Resolve heap struct refs to inline struct snapshots before the
                // structural fold, so a struct (or tuple/named-tuple containing
                // one) whose field is itself a heap `Value::StructRef` compares
                // by value, not by heap index. Without this the `Struct`/`Tuple`
                // arms below fell back to `Debug`-string comparison and two
                // separately-constructed but equal nested structs reported
                // unequal — the same StructRef class as #6685 / #6693, surfaced
                // through `isequal` (Issue #6725). Shares the single canonical
                // value-op resolver (Issue #6694); a cheap no-op for the
                // all-primitive hot path so scalar `isequal` stays unaffected.
                let left = resolve_value_op_structrefs(left, &self.struct_heap);
                let right = resolve_value_op_structrefs(right, &self.struct_heap);

                let is_equal = if let Some(is_integer_equal) = integer_values_isequal(&left, &right)
                {
                    is_integer_equal
                } else {
                    match (&left, &right) {
                        (Value::F64(a), Value::F64(b)) => {
                            if a.is_nan() && b.is_nan() {
                                true
                            } else {
                                a.to_bits() == b.to_bits() // Handles -0.0 vs 0.0
                            }
                        }
                        (Value::F32(a), Value::F32(b)) => {
                            if a.is_nan() && b.is_nan() {
                                true
                            } else {
                                a.to_bits() == b.to_bits()
                            }
                        }
                        (Value::F16(a), Value::F16(b)) => {
                            if a.is_nan() && b.is_nan() {
                                true
                            } else {
                                a.to_bits() == b.to_bits()
                            }
                        }
                        // For other types, compare by value
                        (Value::I64(a), Value::I64(b)) => a == b,
                        (Value::Bool(a), Value::Bool(b)) => a == b,
                        // Cross-type numeric equality: isequal(1, 1.0) is true.
                        // Exact value equality with no rounding of the integer to
                        // the float type (Issue #8187: isequal(2^53+1, 2.0^53) is
                        // false; all Int*/UInt* × Float16/32/64 widths in #8199),
                        // plus isequal's sign-of-zero distinction — an integer is
                        // always +0, so isequal(0, -0.0) is false even though
                        // 0 == -0.0.
                        (a, b)
                            if crate::vm::numeric_identity::mixed_int_float_values_equal(a, b)
                                .is_some() =>
                        {
                            crate::vm::numeric_identity::mixed_int_float_values_equal(a, b)
                                .unwrap_or(false)
                                && !crate::vm::numeric_identity::is_negative_zero_fixed_float(a)
                                && !crate::vm::numeric_identity::is_negative_zero_fixed_float(b)
                        }
                        (Value::Str(a), Value::Str(b)) => a == b,
                        (Value::Char(a), Value::Char(b)) => a == b,
                        (Value::Nothing, Value::Nothing) => true,
                        (Value::Missing, Value::Missing) => true,
                        // Modules: identity by module name (Issue #4959).
                        (Value::Module(a), Value::Module(b)) => a.name == b.name,
                        (Value::DataType(a), Value::DataType(b)) => type_objects_equal(a, b),
                        (Value::RuntimeTypeVar(a), Value::RuntimeTypeVar(b)) => a.id == b.id,
                        (Value::RuntimeTypeName(a), Value::RuntimeTypeName(b)) => a.name == b.name,
                        (Value::Tuple(a), Value::Tuple(b)) => values_isequal(
                            Some(Value::Tuple(a.clone())),
                            Some(Value::Tuple(b.clone())),
                        ),
                        // Named tuples are equal iff they have the same field
                        // names in the same order and each field `isequal`
                        // (Issue #5265). `==` on named tuples routes through this
                        // builtin (the same early route bare tuples use); without
                        // this arm equal named tuples fell through to `false`.
                        (Value::NamedTuple(a), Value::NamedTuple(b)) => {
                            a.names == b.names
                                && a.values.len() == b.values.len()
                                && a.values.iter().zip(b.values.iter()).all(|(xv, yv)| {
                                    values_isequal(Some(xv.clone()), Some(yv.clone()))
                                })
                        }
                        // Core.SimpleVector isequal is element-wise (Issue #4722).
                        (Value::SimpleVector(a), Value::SimpleVector(b)) => values_isequal(
                            Some(Value::SimpleVector(a.clone())),
                            Some(Value::SimpleVector(b.clone())),
                        ),
                        (Value::Struct(a), Value::Struct(b)) => {
                            // Normalize struct names to handle module-qualified vs unqualified
                            normalize_struct_name(&a.struct_name)
                                == normalize_struct_name(&b.struct_name)
                                && a.values.len() == b.values.len()
                                && format!("{:?}", a.values) == format!("{:?}", b.values)
                        }
                        // Expr: structural equality (head and args)
                        (Value::Expr(a), Value::Expr(b)) => {
                            let a_args = a.args_snapshot();
                            let b_args = b.args_snapshot();
                            a.head == b.head
                                && a_args.len() == b_args.len()
                                && format!("{:?}", a_args) == format!("{:?}", b_args)
                        }
                        // Array vs Array/Memory comparisons are handled above by
                        // `try_isequal_array_like` so they no longer appear here.
                        (Value::Memory(ma), Value::Memory(mb)) => {
                            ma.borrow().isequal_contents(&mb.borrow())
                        }
                        // Different types are not equal
                        _ => false,
                    }
                };

                self.stack.push(Value::Bool(is_equal));
            }

            BuiltinId::TupleEquals => {
                // `==` folded over Tuple/NamedTuple elements (Issue #5267).
                // Uses `==` element semantics (so `0.0 == -0.0` and not
                // `NaN == NaN`) with three-valued `missing` propagation,
                // matching upstream `Base.:==(::Tuple, ::Tuple)` /
                // `Base.:==(::NamedTuple, ::NamedTuple)`.
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // Resolve heap struct refs to inline struct snapshots so struct
                // elements (e.g. `OneTo`) compare by value, not by heap index
                // (Issue #6685). No-op for all-primitive tuples so the hot path
                // stays allocation-free (Issue #6694 consolidation).
                let left = resolve_value_op_structrefs(left, &self.struct_heap);
                let right = resolve_value_op_structrefs(right, &self.struct_heap);

                let result = match (&left, &right) {
                    (Value::Tuple(a), Value::Tuple(b)) => {
                        tuple_elements_equal_tristate(&a.elements, &b.elements)
                    }
                    (Value::NamedTuple(a), Value::NamedTuple(b)) => {
                        if a.names == b.names {
                            tuple_elements_equal_tristate(&a.values, &b.values)
                        } else {
                            // `==(a::NamedTuple, b::NamedTuple) = false` when the
                            // names/arity differ (upstream namedtuple.jl).
                            Some(false)
                        }
                    }
                    // Mixed Tuple vs NamedTuple (or any other pairing) is not a
                    // tuple/named-tuple `==`; defer to `==` element semantics on
                    // the whole values, which yields `false` for distinct kinds.
                    _ => values_equal_tristate(&left, &right),
                };

                let pushed = match result {
                    Some(b) => Value::Bool(b),
                    None => Value::Missing,
                };
                self.stack.push(pushed);
            }

            BuiltinId::Hash => {
                // hash(x) - compute hash value
                let val = self.stack.pop_value()?;
                let mut hasher = DefaultHasher::new();

                if !try_hash_array_like(&val, &mut hasher, &self.struct_heap) {
                    // Resolve heap struct refs so equal structs — and tuples /
                    // named tuples containing them, e.g. `(OneTo(3),)` — hash
                    // consistently by structural value rather than by heap index
                    // (Issue #6693, same `StructRef` class as #6685). Without
                    // this, two separately constructed equal struct keys produce
                    // distinct `Debug` strings (`StructRef(14)` vs `StructRef(7)`)
                    // and a `Dict`/`Set` lookup never finds the key. Cheap no-op
                    // when no refs are present (Issue #6694 consolidation).
                    let val = resolve_value_op_structrefs(val, &self.struct_heap);
                    match &val {
                        v if numeric_integer_value(v).is_some() => {
                            numeric_integer_value(v).hash(&mut hasher)
                        }
                        Value::I64(v) => v.hash(&mut hasher),
                        Value::F64(v) => v.to_bits().hash(&mut hasher),
                        Value::Str(s) => s.hash(&mut hasher),
                        Value::Char(c) => c.hash(&mut hasher),
                        Value::Nothing => 0u64.hash(&mut hasher),
                        Value::Missing => 1u64.hash(&mut hasher), // Different hash from Nothing
                        Value::Tuple(t) => {
                            for v in &t.elements {
                                format!("{:?}", v).hash(&mut hasher);
                            }
                        }
                        _ => {
                            // For other types, hash the debug representation
                            format!("{:?}", val).hash(&mut hasher);
                        }
                    }
                }

                self.stack.push(Value::I64(hasher.finish() as i64));
            }

            BuiltinId::_Hash => {
                // _hash(x) - internal intrinsic for hash computation (Issue #2582)
                // Same logic as Hash builtin, used by Pure Julia hash methods in hashing.jl
                let val = self.stack.pop_value()?;
                let mut hasher = DefaultHasher::new();

                if !try_hash_array_like(&val, &mut hasher, &self.struct_heap) {
                    // Resolve heap struct refs for consistent structural hashing
                    // of struct keys and tuples/named tuples containing them
                    // (Issue #6693); see the `Hash` builtin above for details.
                    let val = resolve_value_op_structrefs(val, &self.struct_heap);
                    match &val {
                        v if numeric_integer_value(v).is_some() => {
                            numeric_integer_value(v).hash(&mut hasher)
                        }
                        Value::I64(v) => v.hash(&mut hasher),
                        Value::F64(v) => v.to_bits().hash(&mut hasher),
                        Value::Str(s) => s.hash(&mut hasher),
                        Value::Char(c) => c.hash(&mut hasher),
                        Value::Bool(b) => b.hash(&mut hasher),
                        Value::Nothing => 0u64.hash(&mut hasher),
                        Value::Missing => 1u64.hash(&mut hasher),
                        Value::Tuple(t) => {
                            for v in &t.elements {
                                format!("{:?}", v).hash(&mut hasher);
                            }
                        }
                        _ => {
                            format!("{:?}", val).hash(&mut hasher);
                        }
                    }
                }

                self.stack.push(Value::I64(hasher.finish() as i64));
            }

            BuiltinId::Isless => {
                // isless(x, y) - strict weak ordering for sorting
                // isless is used by sort() and defines a total order.
                // Key properties:
                // - isless(NaN, x) = false for all x (NaN is not less than anything)
                // - isless(x, NaN) = true for all non-NaN x (everything is less than NaN)
                // - isless(missing, x) = false for all x (missing is not less than anything)
                // - isless(x, missing) = true for all non-missing x (everything is less than missing)
                // This places NaN and missing at the end when sorting.
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // Resolve heap struct refs through the shared value-op resolver
                // (Issue #6694 consolidation) for parity with the other native
                // compare/hash entry points, so a future struct-bearing `isless`
                // case can't silently compare by heap index (the #6685 / #6693
                // StructRef class). A cheap no-op for the all-primitive scalar
                // hot path that this handler is reached for today (Issue #6725).
                let left = resolve_value_op_structrefs(left, &self.struct_heap);
                let right = resolve_value_op_structrefs(right, &self.struct_heap);

                let is_less = match (&left, &right) {
                    // Missing handling (Missing sorts to the end)
                    (Value::Missing, _) => false, // missing is not less than anything
                    (_, Value::Missing) => true,  // everything is less than missing
                    // NaN handling (NaN sorts to the end, but before missing conceptually)
                    (Value::F64(a), Value::F64(b)) => {
                        if a.is_nan() {
                            false // NaN is not less than anything
                        } else if b.is_nan() {
                            true // non-NaN is less than NaN
                        } else {
                            a < b
                        }
                    }
                    // Integer comparison
                    (Value::I64(a), Value::I64(b)) => a < b,
                    // Cross-type numeric comparison
                    (Value::I64(a), Value::F64(b)) => {
                        if b.is_nan() {
                            true // non-NaN is less than NaN
                        } else {
                            (*a as f64) < *b
                        }
                    }
                    (Value::F64(a), Value::I64(b)) => {
                        if a.is_nan() {
                            false // NaN is not less than anything
                        } else {
                            *a < (*b as f64)
                        }
                    }
                    // String lexicographic comparison
                    (Value::Str(a), Value::Str(b)) => a < b,
                    // Char comparison
                    (Value::Char(a), Value::Char(b)) => a < b,
                    // Bool comparison (false < true)
                    (Value::Bool(a), Value::Bool(b)) => !a && *b,
                    // Nothing handling (nothing sorts before values)
                    (Value::Nothing, Value::Nothing) => false, // nothing is not less than itself
                    (Value::Nothing, _) => true, // nothing is less than everything except itself
                    (_, Value::Nothing) => false, // nothing is not less than non-nothing
                    // Default: types without defined ordering return false
                    _ => false,
                };

                self.stack.push(Value::Bool(is_less));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}

#[cfg(test)]
mod structref_equality_tests {
    //! Unit tests for the heap-struct-ref resolution behind tuple `==`
    //! (Issue #6685) and consistent structural hashing of struct keys
    //! (Issue #6693).
    use super::*;
    use crate::vm::value::TupleValue;

    fn oneto(stop: i64) -> StructInstance {
        StructInstance::with_name(0, "OneTo".to_string(), vec![Value::I64(stop)])
    }

    fn tuple(elems: Vec<Value>) -> Value {
        Value::Tuple(TupleValue { elements: elems })
    }

    #[test]
    fn contains_structref_only_when_a_ref_is_reachable() {
        // Primitive tuple — fast path, no resolution needed.
        assert!(!contains_structref(&tuple(vec![
            Value::I64(1),
            Value::I64(2)
        ])));
        // Direct ref / ref inside a tuple / ref inside a struct field.
        assert!(contains_structref(&Value::StructRef(0)));
        assert!(contains_structref(&tuple(vec![Value::StructRef(0)])));
        assert!(contains_structref(&Value::Struct(
            StructInstance::with_name(1, "Wrapper".to_string(), vec![Value::StructRef(0)],)
        )));
    }

    #[test]
    fn separate_heap_structs_compare_equal_after_resolution() {
        // Two equal OneTo values at distinct heap indices: pre-resolution they
        // would compare by `Debug` string (heap index) and differ; after
        // resolution they are inline structs comparing equal by value.
        let heap = vec![oneto(3), oneto(3)];
        let t1 = tuple(vec![Value::StructRef(0)]);
        let t2 = tuple(vec![Value::StructRef(1)]);

        let r1 = resolve_structrefs_deep(&t1, &heap, &mut Vec::new());
        let r2 = resolve_structrefs_deep(&t2, &heap, &mut Vec::new());

        if let (Value::Tuple(a), Value::Tuple(b)) = (&r1, &r2) {
            assert_eq!(
                tuple_elements_equal_tristate(&a.elements, &b.elements),
                Some(true),
            );
        } else {
            panic!("expected tuples after resolution");
        }
    }

    #[test]
    fn tuple_equals_oneto_and_unitrange_by_range_value_issue_8478() {
        let oneto_tuple = tuple(vec![Value::Struct(oneto(2))]);
        let unitrange_tuple = tuple(vec![Value::Range(
            crate::vm::value::RangeValue::unit_range(1.0, 2.0),
        )]);

        assert_eq!(
            values_equal_tristate(&oneto_tuple, &unitrange_tuple),
            Some(true),
        );
    }

    #[test]
    fn unequal_heap_structs_still_compare_unequal() {
        let heap = vec![oneto(3), oneto(4)];
        let r1 = resolve_structrefs_deep(&tuple(vec![Value::StructRef(0)]), &heap, &mut Vec::new());
        let r2 = resolve_structrefs_deep(&tuple(vec![Value::StructRef(1)]), &heap, &mut Vec::new());
        if let (Value::Tuple(a), Value::Tuple(b)) = (&r1, &r2) {
            assert_eq!(
                tuple_elements_equal_tristate(&a.elements, &b.elements),
                Some(false),
            );
        } else {
            panic!("expected tuples after resolution");
        }
    }

    #[test]
    fn nested_struct_fields_are_resolved() {
        // A wrapper struct whose field is itself a heap ref to a OneTo: the
        // field must be resolved too so nested ranges compare by value.
        let heap = vec![
            oneto(3),                                                                       // 0
            StructInstance::with_name(1, "Wrapper".to_string(), vec![Value::StructRef(0)]), // 1
            oneto(3),                                                                       // 2
            StructInstance::with_name(1, "Wrapper".to_string(), vec![Value::StructRef(2)]), // 3
        ];
        let r1 = resolve_structrefs_deep(&Value::StructRef(1), &heap, &mut Vec::new());
        let r2 = resolve_structrefs_deep(&Value::StructRef(3), &heap, &mut Vec::new());
        assert_eq!(values_equal_tristate(&r1, &r2), Some(true));
    }

    #[test]
    fn equal_structs_hash_consistently_after_resolution_6693() {
        // The `Hash`/`_Hash` builtins resolve heap struct refs before hashing
        // the (Debug) representation. Equal structs at distinct heap indices
        // must then produce identical resolved forms — and thus identical
        // hashes — so a tuple-keyed Dict/Set lookup finds the key (Issue #6693).
        // Before the fix, the Debug string was `StructRef(<index>)`, so equal
        // keys at different indices hashed differently and lookups missed.
        let heap = vec![oneto(3), oneto(3), oneto(4)];
        let a = Value::StructRef(0);
        let b = Value::StructRef(1);
        let c = Value::StructRef(2);

        let ra = resolve_structrefs_deep(&a, &heap, &mut Vec::new());
        let rb = resolve_structrefs_deep(&b, &heap, &mut Vec::new());
        let rc = resolve_structrefs_deep(&c, &heap, &mut Vec::new());

        // The Debug representation is the hash key material for non-array values.
        assert_eq!(format!("{ra:?}"), format!("{rb:?}"));
        assert_ne!(format!("{ra:?}"), format!("{rc:?}"));

        // Same for a struct element inside a tuple key.
        let t1 = resolve_structrefs_deep(&tuple(vec![a]), &heap, &mut Vec::new());
        let t2 = resolve_structrefs_deep(&tuple(vec![b]), &heap, &mut Vec::new());
        if let (Value::Tuple(x), Value::Tuple(y)) = (&t1, &t2) {
            assert_eq!(
                format!("{:?}", x.elements[0]),
                format!("{:?}", y.elements[0]),
            );
        } else {
            panic!("expected tuples after resolution");
        }
    }

    #[test]
    fn isequal_resolves_nested_struct_field_refs_6725() {
        // The native `Isequal` builtin routes its operands through
        // `resolve_value_op_structrefs` before the `values_isequal` structural
        // fold (Issue #6725). Without that, a struct whose field is itself a
        // heap `StructRef` is compared by the `Debug`-string fallback in
        // `values_isequal` (which embeds the differing heap index) and two
        // separately-constructed but equal nested structs report unequal.
        //
        // heap: 0,2 = Inner(5); 1 = Outer{a=StructRef(0)}; 3 = Outer{a=StructRef(2)}.
        let inner = |v: i64| StructInstance::with_name(0, "Inner".to_string(), vec![Value::I64(v)]);
        let heap = vec![
            inner(5),                                                                     // 0
            StructInstance::with_name(1, "Outer".to_string(), vec![Value::StructRef(0)]), // 1
            inner(5),                                                                     // 2
            StructInstance::with_name(1, "Outer".to_string(), vec![Value::StructRef(2)]), // 3
        ];
        let x = Value::StructRef(1);
        let y = Value::StructRef(3);

        // Unresolved: the field StructRefs (0 vs 2) make the `Debug` strings of
        // the unresolved values differ, so a raw structural fold reports unequal.
        assert_ne!(format!("{x:?}"), format!("{y:?}"));

        // After routing through the shared value-op resolver — exactly what the
        // `Isequal` handler now does — the nested fields are inlined and the
        // structs compare equal by value, restoring the `isequal` answer.
        let rx = resolve_value_op_structrefs(x, &heap);
        let ry = resolve_value_op_structrefs(y, &heap);
        assert!(values_isequal(Some(rx.clone()), Some(ry.clone())));

        // The same resolution makes the `isequal ⟹ hash` contract hold: equal
        // resolved values share the `Debug` hash-key material the `Hash` builtin
        // uses for non-array values.
        assert_eq!(format!("{rx:?}"), format!("{ry:?}"));

        // A genuinely different inner value still compares unequal post-resolution.
        let heap2 = vec![
            inner(5),
            StructInstance::with_name(1, "Outer".to_string(), vec![Value::StructRef(0)]),
            inner(6),
            StructInstance::with_name(1, "Outer".to_string(), vec![Value::StructRef(2)]),
        ];
        let nx = resolve_value_op_structrefs(Value::StructRef(1), &heap2);
        let ny = resolve_value_op_structrefs(Value::StructRef(3), &heap2);
        assert!(!values_isequal(Some(nx), Some(ny)));
    }

    #[test]
    fn cyclic_mutable_struct_terminates() {
        // heap[0] references itself; resolution must terminate (visiting guard)
        // and keep the back-edge as a `StructRef` rather than looping forever.
        let heap = vec![StructInstance::with_name(
            0,
            "Node".to_string(),
            vec![Value::StructRef(0)],
        )];
        let resolved = resolve_structrefs_deep(&Value::StructRef(0), &heap, &mut Vec::new());
        match resolved {
            Value::Struct(inst) => {
                assert_eq!(inst.values.len(), 1);
                assert!(matches!(inst.values[0], Value::StructRef(0)));
            }
            other => panic!("expected resolved struct, got {other:?}"),
        }
    }
}
