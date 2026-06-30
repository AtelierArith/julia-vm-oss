//! Value equality and comparison helpers for the VM dispatcher.
//!
//! Split out of `vm/mod.rs` (Issue #6826). These `impl Vm<R>` methods implement
//! the structural/egal/`==` comparison family and the host-return array
//! normalization helpers used by boundary equality.

use super::*;

impl<R: RngLike> Vm<R> {
    /// Compare two struct values by comparing all fields recursively.
    /// Returns true if both are structs of the same type with equal fields.
    /// This is the default == behavior for immutable structs without custom ==.
    pub(super) fn compare_struct_fields(&self, left: &Value, right: &Value) -> bool {
        if let Some(result) = self.compare_array_wrapper_boundary_values_equal(left, right) {
            return result;
        }

        // Resolve StructRef to actual struct data
        let left_struct = match left {
            Value::Struct(s) => Some(s.clone()),
            Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
            _ => None,
        };
        let right_struct = match right {
            Value::Struct(s) => Some(s.clone()),
            Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
            _ => None,
        };

        match (left_struct, right_struct) {
            (Some(l), Some(r)) => {
                // Check struct type names match exactly (including type parameters)
                // In Julia, Point{Int64}(3, 4) != Point{Float64}(3.0, 4.0)
                // because they have different concrete types
                if l.struct_name != r.struct_name {
                    return false;
                }
                // Check field count matches
                if l.values.len() != r.values.len() {
                    return false;
                }
                // Julia's default immutable struct equality follows `===`-style
                // field identity: primitive/isbits fields compare by value, while
                // mutable reference-like fields (Array, Memory, mutable structs)
                // must be the same object. Direct Array/Memory `==` remains
                // element-wise in compare_values_equal.
                for (lv, rv) in l.values.iter().zip(r.values.iter()) {
                    if !self.compare_struct_field_values_egal(lv, rv) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn array_wrapper_memory_and_shape(
        &self,
        value: &Value,
    ) -> Option<(MemoryRef, Vec<usize>)> {
        let struct_value = match value {
            Value::Struct(s) => s,
            Value::StructRef(idx) => self.struct_heap.get(*idx)?,
            _ => return None,
        };
        let base_name = struct_value
            .struct_name
            .split('{')
            .next()
            .unwrap_or(&struct_value.struct_name)
            .rsplit('.')
            .next()
            .unwrap_or(&struct_value.struct_name);
        if !matches!(base_name, "Array" | "Vector" | "Matrix") {
            return None;
        }
        let memory = match struct_value.values.first()? {
            Value::Memory(mem) => mem.clone(),
            // The faithful `Array{T,N}` (Issue #6648) stores `ref::MemoryRef`.
            // Unwrap the parent `Memory` when the reference starts at the buffer
            // head (offset 0), which covers freshly-constructed arrays such as
            // `collect(...)`/comprehensions — the shape field still bounds the
            // logical length, so reading `prod(shape)` elements from offset 0 is
            // correct even when the buffer is over-allocated. Offset views keep
            // their dedicated wrapper paths (Issue #6663).
            Value::MemoryRef(memref) if memref.memory_index() == 1 => memref.parent(),
            _ => return None,
        };
        let shape = match struct_value.values.get(1)? {
            Value::Tuple(t) => {
                let dims = match t.elements.first() {
                    Some(Value::Tuple(inner)) => &inner.elements,
                    _ => &t.elements,
                };
                let mut shape = Vec::with_capacity(dims.len());
                for dim in dims {
                    match dim {
                        Value::I64(n) if *n >= 0 => shape.push(usize::try_from(*n).ok()?),
                        _ => return None,
                    }
                }
                shape
            }
            Value::I64(n) if *n >= 0 => vec![usize::try_from(*n).ok()?],
            _ => return None,
        };
        Some((memory, shape))
    }

    pub(super) fn normalize_host_return_value(&self, value: Value) -> Value {
        match builtins_linalg::linalg_value_to_array_value(
            value.clone(),
            &self.struct_heap,
            "return",
            None,
        ) {
            // Issue #6864 / #6807: resolve `StructRef` array elements into inline
            // `Struct`s (so the heap-less host can read them), then return the
            // self-contained inline `Array{T,N}` wrapper. It must be inline
            // (`Value::Struct`, not a heap `StructRef`) because the host drops the
            // `Vm`/`struct_heap` right after `run()` — a `StructRef` would dangle.
            Ok(arr) => {
                let resolved = self.normalize_host_array_value(arr);
                value::array_wrapper_value_from_array_value_inline(
                    resolved,
                    self.get_array_type_id(),
                )
                .unwrap_or(value)
            }
            Err(_) => value,
        }
    }

    pub(super) fn normalize_host_array_value(&self, mut arr: ArrayValue) -> ArrayValue {
        if let ArrayData::StructRefs(indices) = &arr.data {
            let values = indices
                .iter()
                .map(|idx| self.struct_heap.get(*idx).cloned().map(Value::Struct))
                .collect::<Option<Vec<_>>>();
            if let Some(values) = values {
                arr.data = ArrayData::Any(values);
            }
        } else if let ArrayData::Any(values) = &arr.data {
            let values = values
                .iter()
                .map(|value| match value {
                    Value::StructRef(idx) => self.struct_heap.get(*idx).cloned().map(Value::Struct),
                    other => Some(other.clone()),
                })
                .collect::<Option<Vec<_>>>();
            if let Some(values) = values {
                arr.data = ArrayData::Any(values);
            }
        }
        arr
    }

    pub(super) fn compare_memory_values_with_shape(
        &self,
        left_mem: &MemoryRef,
        left_shape: &[usize],
        right_mem: &MemoryRef,
        right_shape: &[usize],
    ) -> bool {
        if left_shape != right_shape {
            return false;
        }
        let left = left_mem.borrow();
        let right = right_mem.borrow();
        left.len() == right.len()
            && (0..left.len()).all(|i| {
                self.compare_boundary_values_equal(left.data.get_value(i), right.data.get_value(i))
            })
    }

    pub(super) fn compare_array_values_logical_equal(
        &self,
        left: &ArrayValue,
        right: &ArrayValue,
    ) -> bool {
        let Ok(left_values) = left.to_logical_value_vec() else {
            return false;
        };
        let Ok(right_values) = right.to_logical_value_vec() else {
            return false;
        };
        left.shape == right.shape
            && left_values.len() == right_values.len()
            && left_values
                .into_iter()
                .zip(right_values)
                .all(|(left, right)| self.compare_boundary_values_equal(Some(left), Some(right)))
    }

    pub(super) fn array_value_for_equality(&self, value: &Value) -> Option<ArrayValue> {
        let result = builtins_linalg::linalg_value_to_array_value(
            value.clone(),
            &self.struct_heap,
            "array equality",
            None,
        );
        result.ok()
    }

    /// Structural value equality for an immutable `Struct` or a mutable
    /// `StructRef` element (resolving the latter via the heap), comparing fields
    /// recursively. A literal `ComplexF64` array stores its elements as immutable
    /// `Value::Struct`, while a broadcast/copy result stores them as `StructRef`,
    /// so the two carriers must compare equal element-by-element (Issue #5789).
    pub(super) fn compare_struct_like_values_equal(&self, left: &Value, right: &Value) -> bool {
        let resolve = |v: &Value| -> Option<(String, Vec<Value>)> {
            match v {
                Value::Struct(s) => Some((s.struct_name.to_string(), s.values.clone())),
                Value::StructRef(idx) => self
                    .struct_heap
                    .get(*idx)
                    .map(|s| (s.struct_name.to_string(), s.values.clone())),
                _ => None,
            }
        };
        match (resolve(left), resolve(right)) {
            (Some((ln, lv)), Some((rn, rv))) => {
                crate::vm::type_utils::normalize_struct_name(&ln)
                    == crate::vm::type_utils::normalize_struct_name(&rn)
                    && lv.len() == rv.len()
                    && lv.iter().zip(rv.iter()).all(|(xv, yv)| {
                        self.compare_boundary_values_equal(Some(xv.clone()), Some(yv.clone()))
                    })
            }
            _ => false,
        }
    }

    pub(super) fn compare_boundary_values_equal(
        &self,
        left: Option<Value>,
        right: Option<Value>,
    ) -> bool {
        match (left, right) {
            (Some(ref x), Some(ref y)) if numeric_integer_values_equal(x, y).is_some() => {
                numeric_integer_values_equal(x, y).unwrap_or(false)
            }
            (Some(Value::I64(x)), Some(Value::I64(y))) => x == y,
            (Some(Value::F64(x)), Some(Value::F64(y))) => {
                x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
            }
            (Some(ref x), Some(ref y))
                if super::numeric_identity::mixed_int_float_values_equal(x, y).is_some() =>
            {
                // Value-based mixed integer/float `==`, no rounding of the integer
                // (Issue #8187, all widths in #8199).
                super::numeric_identity::mixed_int_float_values_equal(x, y).unwrap_or(false)
            }
            // Immutable `Struct` and mutable `StructRef` elements (e.g.
            // `ComplexF64` array elements) compare by structural value, not by the
            // `Debug`-string fallback below. A literal array stores `Value::Struct`
            // elements while a broadcast/copy result stores `StructRef`, so the two
            // carriers' equal `Complex` elements had different `Debug` strings and
            // compared unequal — making `ComplexF64[...] == (a .+ 0)` false despite
            // equal values (Issue #5789). Resolve both and compare fields.
            (Some(ref x), Some(ref y))
                if matches!(x, Value::Struct(_) | Value::StructRef(_))
                    && matches!(y, Value::Struct(_) | Value::StructRef(_)) =>
            {
                self.compare_struct_like_values_equal(x, y)
            }
            (Some(x), Some(y)) => format!("{:?}", x) == format!("{:?}", y),
            _ => false,
        }
    }

    pub(super) fn compare_array_wrapper_boundary_values_equal(
        &self,
        left: &Value,
        right: &Value,
    ) -> Option<bool> {
        let left_array_for_equality = self.array_value_for_equality(left);
        let right_array_for_equality = self.array_value_for_equality(right);
        match (left_array_for_equality, right_array_for_equality) {
            (Some(left_arr), Some(right_arr)) => {
                return Some(self.compare_array_values_logical_equal(&left_arr, &right_arr));
            }
            (Some(left_arr), None) => {
                if let Value::Memory(right_mem) = right {
                    let right_len = right_mem.borrow().len();
                    return Some(self.compare_array_values_logical_equal(
                        &left_arr,
                        &ArrayValue::from_memory(right_mem.borrow().clone(), vec![right_len]),
                    ));
                }
            }
            (None, Some(right_arr)) => {
                if let Value::Memory(left_mem) = left {
                    let left_len = left_mem.borrow().len();
                    return Some(self.compare_array_values_logical_equal(
                        &ArrayValue::from_memory(left_mem.borrow().clone(), vec![left_len]),
                        &right_arr,
                    ));
                }
            }
            (None, None) => {}
        }

        let left_wrapper = self.array_wrapper_memory_and_shape(left);
        let right_wrapper = self.array_wrapper_memory_and_shape(right);
        match (left_wrapper, right_wrapper) {
            (Some((left_mem, left_shape)), Some((right_mem, right_shape))) => {
                Some(self.compare_memory_values_with_shape(
                    &left_mem,
                    &left_shape,
                    &right_mem,
                    &right_shape,
                ))
            }
            (Some((mem, shape)), None) => {
                self.compare_array_wrapper_against_other(&mem, &shape, right)
            }
            (None, Some((mem, shape))) => {
                self.compare_array_wrapper_against_other(&mem, &shape, left)
            }
            _ => None,
        }
    }

    /// Compare a Pure Julia Array wrapper's `(memory, shape)` against an
    /// arbitrary other `Value`. The native-array side is destructured through
    /// the shared `native_array_value_ref` helper so this file no longer
    /// holds a native-array tuple-pattern arm in the wrapper equality bridge
    /// (Issue #3908). Returns `None` for non-array-like `other` values,
    /// matching the previous catch-all `_ => None` arm.
    pub(super) fn compare_array_wrapper_against_other(
        &self,
        wrapper_mem: &MemoryRef,
        wrapper_shape: &[usize],
        other: &Value,
    ) -> Option<bool> {
        if let Some(arr) = native_array_value_ref(other) {
            let mem_borrow = wrapper_mem.borrow();
            let arr_ref = arr.borrow();
            if arr_ref.shape != wrapper_shape || arr_ref.len() != mem_borrow.len() {
                return Some(false);
            }
            Some((0..arr_ref.len()).all(|i| {
                self.compare_boundary_values_equal(
                    mem_borrow.data.get_value(i),
                    arr_ref.get_linear(i).ok(),
                )
            }))
        } else if let Value::Memory(other_mem) = other {
            Some(self.compare_memory_values_with_shape(
                wrapper_mem,
                wrapper_shape,
                other_mem,
                &[other_mem.borrow().len()],
            ))
        } else {
            None
        }
    }

    pub(super) fn compare_struct_field_values_egal(&self, left: &Value, right: &Value) -> bool {
        if let Some(is_integer_identical) = numeric_integer_values_identical(left, right) {
            return is_integer_identical;
        }

        match (left, right) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F16(a), Value::F16(b)) => a.to_bits() == b.to_bits(),
            (Value::F32(a), Value::F32(b)) => a.to_bits() == b.to_bits(),
            (Value::F64(a), Value::F64(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Nothing, Value::Nothing) => true,
            (Value::Missing, Value::Missing) => true,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::DataType(a), Value::DataType(b)) => type_utils::type_objects_equal(a, b),
            (Value::RuntimeTypeVar(a), Value::RuntimeTypeVar(b)) => a.id == b.id,
            (Value::RuntimeTypeName(a), Value::RuntimeTypeName(b)) => a.name == b.name,
            _ if is_native_array_value(left) && is_native_array_value(right) => {
                native_array_value_ptr_eq(left, right)
            }
            (Value::Memory(a), Value::Memory(b)) => std::ptr::eq(a.as_ptr(), b.as_ptr()),
            (Value::MemoryRef(a), Value::MemoryRef(b)) => {
                std::ptr::eq(a.parent().as_ptr(), b.parent().as_ptr())
                    && a.memory_index() == b.memory_index()
            }
            (Value::StructRef(a), Value::StructRef(b)) => {
                // Default `==`/`isequal` field identity: a MUTABLE nested struct
                // field compares by reference identity (same heap object), while
                // an IMMUTABLE nested struct field recurses by value — two
                // separately-constructed but equal immutable structs are equal
                // (the #6685 / #6693 StructRef class, surfaced via nested struct
                // fields; Issue #6725). Mirrors the mutability-aware resolution
                // `===` uses (Issue #6709). Same heap index is always equal.
                if a == b {
                    true
                } else if self.heap_struct_is_mutable(*a) || self.heap_struct_is_mutable(*b) {
                    false
                } else {
                    self.compare_struct_fields(left, right)
                }
            }
            (Value::Struct(_), _) | (_, Value::Struct(_)) => {
                self.compare_struct_fields(left, right)
            }
            (Value::Tuple(a), Value::Tuple(b)) => {
                a.elements.len() == b.elements.len()
                    && a.elements
                        .iter()
                        .zip(b.elements.iter())
                        .all(|(av, bv)| self.compare_struct_field_values_egal(av, bv))
            }
            _ => false,
        }
    }

    /// Test helper for direct equality regressions that must remain separate
    /// from default struct field identity semantics.
    #[cfg(test)]
    pub(super) fn compare_values_equal(&self, left: &Value, right: &Value) -> bool {
        if let Some(is_integer_equal) = numeric_integer_values_equal(left, right) {
            return is_integer_equal;
        }

        match (left, right) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Nothing, Value::Nothing) => true,
            (Value::Missing, Value::Missing) => true,
            // Cross-type numeric comparison: value-based, no rounding of the
            // integer (Issue #8187, all integer/float widths in #8199).
            (x, y) if super::numeric_identity::mixed_int_float_values_equal(x, y).is_some() => {
                super::numeric_identity::mixed_int_float_values_equal(x, y).unwrap_or(false)
            }
            // Struct comparison (recursive)
            (Value::Struct(_), _)
            | (_, Value::Struct(_))
            | (Value::StructRef(_), _)
            | (_, Value::StructRef(_)) => self.compare_struct_fields(left, right),
            // Arrays — route through `native_array_value_ref` so the test
            // helper keeps the legacy carrier unwrap centralized while #3908
            // retires the native container (Issue #4189).
            _ if is_native_array_value(left) && is_native_array_value(right) => {
                let a_ref = native_array_value_ref(left).unwrap().borrow();
                let b_ref = native_array_value_ref(right).unwrap().borrow();
                self.compare_array_values_equal(&a_ref, &b_ref)
            }
            (Value::Memory(m), right) if is_native_array_value(right) => {
                let b_ref = native_array_value_ref(right).unwrap().borrow();
                self.compare_memory_array_values_equal(m, &b_ref)
            }
            (left, Value::Memory(m)) if is_native_array_value(left) => {
                let a_ref = native_array_value_ref(left).unwrap().borrow();
                self.compare_memory_array_values_equal(m, &a_ref)
            }
            (Value::Memory(m1), Value::Memory(m2)) => self.compare_memory_values_equal(m1, m2),
            // Tuples
            (Value::Tuple(a), Value::Tuple(b)) => {
                if a.elements.len() != b.elements.len() {
                    return false;
                }
                for (av, bv) in a.elements.iter().zip(b.elements.iter()) {
                    if !self.compare_values_equal(av, bv) {
                        return false;
                    }
                }
                true
            }
            // Different types are not equal
            _ => false,
        }
    }

    #[cfg(test)]
    pub(super) fn compare_optional_values_equal(
        &self,
        left: Option<Value>,
        right: Option<Value>,
    ) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => self.compare_values_equal(&left, &right),
            _ => false,
        }
    }

    #[cfg(test)]
    pub(super) fn compare_array_values_equal(&self, left: &ArrayValue, right: &ArrayValue) -> bool {
        left.shape == right.shape
            && left.element_count() == right.element_count()
            && (0..left.element_count()).all(|i| {
                self.compare_optional_values_equal(
                    left.get_linear(i).ok(),
                    right.get_linear(i).ok(),
                )
            })
    }

    #[cfg(test)]
    pub(super) fn compare_memory_array_values_equal(
        &self,
        memory: &MemoryRef,
        array: &ArrayValue,
    ) -> bool {
        let memory = memory.borrow();
        memory.len() == array.element_count()
            && array.shape.as_slice() == [memory.len()]
            && (0..memory.len()).all(|i| {
                self.compare_optional_values_equal(
                    memory.data.get_value(i),
                    array.get_linear(i).ok(),
                )
            })
    }

    #[cfg(test)]
    pub(super) fn compare_memory_values_equal(&self, left: &MemoryRef, right: &MemoryRef) -> bool {
        let left = left.borrow();
        let right = right.borrow();
        left.len() == right.len()
            && (0..left.len()).all(|i| {
                self.compare_optional_values_equal(left.data.get_value(i), right.data.get_value(i))
            })
    }

    // ==================== End Helpers ====================
}
