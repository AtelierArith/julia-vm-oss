//! Array builtin functions for the VM.
//!
//! Array creation, mutation, and query operations.

// SAFETY: i64→usize casts for range lengths are from `r.length()` which returns ≥ 0.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::util;
use super::value::{
    array_element_type_to_julia_type, array_wrapper_shape_from_tuple,
    ensure_native_array_value_acyclic, is_scalar_carrier, new_array_ref, new_memory_ref, ArrayData,
    ArrayRef, ArrayValue, MemoryRefValue, MemoryValue, StructInstance, TupleValue, Value,
};
use super::Vm;
use crate::types::JuliaType;
use subset_julia_vm_bytecode::{ArrayElementType, Instr};

/// File-local alias for the shared
/// [`super::value::native_array_value_ref`] destructure helper. Keeps the
/// existing call sites in this file (query/mutation builtins) using the
/// same local name (Issue #3908).
#[inline]
fn value_as_array_ref(val: &Value) -> Option<&ArrayRef> {
    super::value::native_array_value_ref(val)
}

fn array_container_element_type(jt: JuliaType) -> Option<JuliaType> {
    match jt {
        JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) => Some(*elem),
        JuliaType::Struct(name) => {
            let base = name.split('{').next().unwrap_or(name.as_str());
            let base = base.rsplit('.').next().unwrap_or(base);
            if base != "Array" {
                return None;
            }
            subset_julia_vm_bytecode::parse_parametric_params(&name)
                .first()
                .map(|param| JuliaType::from_name_or_struct(param))
        }
        _ => None,
    }
}

fn parametric_type_parameter(
    jt: &JuliaType,
    expected_base: &str,
    index: usize,
) -> Option<JuliaType> {
    let name = match jt {
        JuliaType::Struct(name) => name.as_str(),
        _ => return None,
    };
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    if base != expected_base {
        return None;
    }
    subset_julia_vm_bytecode::parse_parametric_params(name)
        .get(index)
        .map(|param| JuliaType::from_name_or_struct(param.trim()))
}

fn dict_key_type(jt: &JuliaType) -> Option<JuliaType> {
    parametric_type_parameter(jt, "Dict", 0)
}

fn dict_value_type(jt: &JuliaType) -> Option<JuliaType> {
    parametric_type_parameter(jt, "Dict", 1)
}

fn is_array_wrapper_value(val: &Value, struct_heap: &[super::value::StructInstance]) -> bool {
    match val {
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(|s| s.array_wrapper_julia_type())
            .is_some(),
        Value::Struct(s) => s.array_wrapper_julia_type().is_some(),
        _ => false,
    }
}

fn pairs_key_value_types_from_name(name: &str) -> Option<(JuliaType, JuliaType)> {
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    if base != "Pairs" {
        return None;
    }
    let params = subset_julia_vm_bytecode::parse_parametric_params(name);
    let key = params.first()?;
    let value = params.get(1)?;
    Some((
        JuliaType::from_name_or_struct(key),
        JuliaType::from_name_or_struct(value),
    ))
}

fn pairs_key_value_types(
    val: &Value,
    struct_heap: &[super::value::StructInstance],
) -> Option<(JuliaType, JuliaType)> {
    match val {
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(|s| pairs_key_value_types_from_name(&s.struct_name)),
        Value::Struct(s) => pairs_key_value_types_from_name(&s.struct_name),
        _ => None,
    }
}

fn array_wrapper_shape(size: &Value) -> Option<Vec<usize>> {
    let Value::Tuple(size_tuple) = size else {
        return None;
    };

    if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
        return array_wrapper_shape_from_tuple(dims_tuple);
    }

    array_wrapper_shape_from_tuple(size_tuple)
}

fn array_wrapper_vector_len_and_offset(size: &Value) -> Option<(usize, usize)> {
    let Value::Tuple(size_tuple) = size else {
        return None;
    };

    if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
        if dims_tuple.elements.len() != 1 {
            return None;
        }
        let len = match dims_tuple.elements.first() {
            Some(Value::I64(n)) if *n >= 0 => usize::try_from(*n).ok()?,
            _ => return None,
        };
        let offset = match size_tuple.elements.get(1) {
            Some(Value::I64(offset)) if *offset >= 1 => usize::try_from(*offset).ok()?,
            _ => return None,
        };
        return Some((len, offset));
    }

    if size_tuple.elements.len() != 1 {
        return None;
    }
    let len = match size_tuple.elements.first() {
        Some(Value::I64(n)) if *n >= 0 => usize::try_from(*n).ok()?,
        _ => return None,
    };
    Some((len, 1))
}

struct ArrayWrapperVectorSnapshot {
    idx: usize,
    element_type: ArrayElementType,
    values: Vec<Value>,
    store_as_memory_ref: bool,
}

/// Issue #10566(c): the backing-storage identity of a numeric `Vector{T}` — see
/// [`Vm::numeric_vector_storage_id`]. Two of these ALIAS iff they name the same
/// storage `Rc` and their element ranges overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vm) struct VectorStorageId {
    /// `Rc::as_ptr` of the backing `Memory` (or of the native `ArrayRef`
    /// carrier when `native`), as an address. Only ever compared, never
    /// dereferenced; the `Rc` is kept alive for the comparison's lifetime by
    /// the caller's own live-in resolution.
    pub ptr: usize,
    /// `ptr` is a native `ArrayRef` carrier, not a `Memory`.
    pub native: bool,
    /// 1-based element start of this wrapper's window within the storage.
    pub start: usize,
    /// Element count of the window.
    pub len: usize,
}

impl VectorStorageId {
    /// Half-open element-range overlap within the same storage.
    pub(in crate::vm) fn overlaps(&self, other: &Self) -> bool {
        self.ptr == other.ptr
            && self.native == other.native
            && self.start < other.start.saturating_add(other.len)
            && other.start < self.start.saturating_add(self.len)
    }
}

fn array_wrapper_element_type(instance: &StructInstance) -> Option<ArrayElementType> {
    instance
        .array_wrapper_julia_type()
        .and_then(array_container_element_type)
        .map(|jt| super::exec::array_basic::array_element_type_from_julia_type(&jt))
        .or_else(|| match instance.values.first() {
            Some(Value::MemoryRef(memref)) => Some(memref.element_type()),
            Some(Value::Memory(mem)) => Some(mem.borrow().element_type().clone()),
            Some(value) => value_as_array_ref(value).map(|arr| arr.borrow().element_type()),
            None => None,
        })
}

fn array_wrapper_similar_spec(
    val: &Value,
    struct_heap: &[StructInstance],
) -> Option<(ArrayElementType, Vec<usize>)> {
    let instance = match val {
        Value::StructRef(idx) => struct_heap.get(*idx)?,
        Value::Struct(s) => s,
        _ => return None,
    };
    let elem_type = array_wrapper_element_type(instance)?;
    let shape = instance.values.get(1).and_then(array_wrapper_shape)?;
    Some((elem_type, shape))
}

impl<R: RngLike> Vm<R> {
    /// Element-type tag for the length-defined float range intrinsics
    /// (`_linspace_range_f64` / `_steprangelen_range_f64`, Issue #9509):
    /// 0 = Float64, 1 = Float32, 2 = Float16.
    fn range_float_element_for_tag(tag: i64) -> Result<super::value::RangeElementType, VmError> {
        match tag {
            0 => Ok(super::value::RangeElementType::Float64),
            1 => Ok(super::value::RangeElementType::Float32),
            2 => Ok(super::value::RangeElementType::Float16),
            other => Err(VmError::TypeError(format!(
                "invalid float range element tag {other} (internal, base/range.jl)"
            ))),
        }
    }

    fn pairs_key_value_types_for_value(&self, val: &Value) -> Option<(JuliaType, JuliaType)> {
        match val {
            Value::Pairs(pairs) => {
                pairs_key_value_types_from_name(&self.pairs_runtime_type_name(pairs))
            }
            _ => pairs_key_value_types(val, &self.struct_heap),
        }
    }

    fn push_array_value(&mut self, arr: ArrayValue) {
        self.push_array_ref(new_array_ref(arr));
    }

    fn push_array_ref(&mut self, arr_ref: ArrayRef) {
        self.stack
            .push(super::value::native_array_ref_value(arr_ref));
    }

    fn push_similar_array_value(
        &mut self,
        elem_type: ArrayElementType,
        shape: Vec<usize>,
        preserve_bitarray_surface: bool,
    ) {
        let mut new_arr = match elem_type {
            ArrayElementType::ComplexF64 => {
                // Issue #9198 S5: Complex{Float64} arrays back their interleaved
                // buffer with the general contiguous-isbits `StructF64` variant;
                // `undef_complex_f64` routes through the shared `complex_f64`
                // constructor rather than hardcoding `ArrayData::F64` storage.
                let mut arr = ArrayValue::undef_complex_f64(shape);
                arr.struct_type_id = Some(self.get_complex_type_id());
                arr
            }
            _ => ArrayValue::memory_first_undef(&elem_type, shape),
        };
        if preserve_bitarray_surface && elem_type == ArrayElementType::Bool {
            new_arr.mark_as_bitarray();
        }
        self.push_array_value(new_arr);
    }

    fn array_wrapper_vector_snapshot(
        &self,
        value: Value,
    ) -> Result<Result<ArrayWrapperVectorSnapshot, Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if instance.array_wrapper_julia_type().is_none() {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(storage) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };

        let snapshot = match storage {
            Value::Memory(mem_ref) => {
                let mem = mem_ref.borrow();
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(mem.get(offset + linear)?);
                }
                ArrayWrapperVectorSnapshot {
                    idx,
                    element_type: mem.element_type().clone(),
                    values,
                    store_as_memory_ref: false,
                }
            }
            Value::MemoryRef(memref) => {
                let parent = memref.parent();
                let start = memref.memory_index();
                let mem = parent.borrow();
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(mem.get(start + linear)?);
                }
                ArrayWrapperVectorSnapshot {
                    idx,
                    element_type: memref.element_type(),
                    values,
                    store_as_memory_ref: true,
                }
            }
            other => {
                let Some(arr_ref) = value_as_array_ref(&other) else {
                    return Ok(Err(Value::StructRef(idx)));
                };
                if offset != 1 {
                    return Ok(Err(Value::StructRef(idx)));
                }
                let arr = arr_ref.borrow();
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(arr.get_linear(linear)?);
                }
                ArrayWrapperVectorSnapshot {
                    idx,
                    element_type: arr.element_type(),
                    values,
                    store_as_memory_ref: false,
                }
            }
        };

        Ok(Ok(snapshot))
    }

    fn store_array_wrapper_vector_snapshot(
        &mut self,
        snapshot: ArrayWrapperVectorSnapshot,
    ) -> Result<Value, VmError> {
        let new_len = snapshot.values.len();
        let mut new_mem = MemoryValue::undef_typed(&snapshot.element_type, new_len);
        for (idx, value) in snapshot.values.into_iter().enumerate() {
            new_mem.set(idx + 1, value)?;
        }
        let new_ref = new_memory_ref(new_mem);
        if let Some(instance) = self.struct_heap.get_mut(snapshot.idx) {
            let storage = if snapshot.store_as_memory_ref {
                Value::MemoryRef(Box::new(MemoryRefValue::first(new_ref)))
            } else {
                Value::Memory(new_ref)
            };
            instance.set_field(0, storage)?;
            instance.set_field(
                1,
                Value::Tuple(TupleValue::new(vec![Value::I64(new_len as i64)])),
            )?;
        }
        Ok(Value::StructRef(snapshot.idx))
    }

    /// Issue #10104: read-only snapshot of a general MemoryRef-backed
    /// `Vector{Float64}` / `Vector{Int64}` struct as a fresh, owned 1-D
    /// [`ArrayRef`], so the typed-loop executor can read elements without
    /// per-element pure-Julia `getindex` dispatch. Returns `None` for any value
    /// that is not a plain contiguous numeric vector (views, other element
    /// types, non-struct arrays), so the caller conservatively falls back to the
    /// interpreter. Intended only for loops that read but never mutate the array
    /// (the typed-loop recognizer enforces this), so the snapshot copy is a
    /// faithful, constant view for the duration of the loop.
    pub(in crate::vm) fn snapshot_read_only_numeric_vector(
        &self,
        value: &Value,
    ) -> Option<ArrayRef> {
        if !matches!(value, Value::StructRef(_)) {
            return None;
        }
        let snapshot = match self.array_wrapper_vector_snapshot(value.clone()) {
            Ok(Ok(snapshot)) => snapshot,
            _ => return None,
        };
        let len = snapshot.values.len();
        let data = match snapshot.element_type {
            ArrayElementType::F64 => {
                let mut v = Vec::with_capacity(len);
                for val in &snapshot.values {
                    let Value::F64(x) = val else { return None };
                    v.push(*x);
                }
                ArrayData::F64(v)
            }
            ArrayElementType::I64 => {
                let mut v = Vec::with_capacity(len);
                for val in &snapshot.values {
                    let Value::I64(x) = val else { return None };
                    v.push(*x);
                }
                ArrayData::I64(v)
            }
            _ => return None,
        };
        Some(new_array_ref(ArrayValue::new(data, vec![len])))
    }

    /// Issue #10566(c): the BACKING-STORAGE identity of a MemoryRef-backed
    /// numeric `Vector{T}` struct — the `Rc` the elements actually live in,
    /// plus the `[start, start + len)` element range this wrapper addresses
    /// within it. The wrapper's own `StructRef` index is NOT a storage
    /// identity: two distinct `Array` wrapper structs (a reshape, a
    /// `MemoryRef` at a different offset, a second binding built over the
    /// same `Memory`) have different `StructRef` indices while sharing one
    /// backing `Memory`, and the typed-loop write-back commits through that
    /// `Memory` — so an alias check keyed on `StructRef` would miss them and
    /// silently lose or stale-read stores (found in adversarial review).
    ///
    /// `native == true` means the storage is the legacy native `ArrayRef`
    /// carrier rather than a `Memory`; its `ptr` is directly comparable with a
    /// native array local's own `ArrayRef` pointer, so a struct wrapper built
    /// over the same native carrier as a bare array local also collides.
    pub(in crate::vm) fn numeric_vector_storage_id(
        &self,
        value: &Value,
    ) -> Option<VectorStorageId> {
        let Value::StructRef(idx) = value else {
            return None;
        };
        let instance = self.struct_heap.get(*idx)?;
        instance.array_wrapper_julia_type()?;
        let storage = instance.values.first()?;
        let size_value = instance.values.get(1)?;
        let (len, offset) = array_wrapper_vector_len_and_offset(size_value)?;
        match storage {
            Value::Memory(mem_ref) => Some(VectorStorageId {
                ptr: std::rc::Rc::as_ptr(mem_ref) as *const u8 as usize,
                native: false,
                start: offset,
                len,
            }),
            Value::MemoryRef(memref) => {
                let parent = memref.parent();
                Some(VectorStorageId {
                    ptr: std::rc::Rc::as_ptr(&parent) as *const u8 as usize,
                    native: false,
                    start: memref.memory_index(),
                    len,
                })
            }
            other => {
                let arr_ref = value_as_array_ref(other)?;
                Some(VectorStorageId {
                    ptr: std::rc::Rc::as_ptr(arr_ref) as *const u8 as usize,
                    native: true,
                    start: offset,
                    len,
                })
            }
        }
    }

    /// Issue #10566(c): the STORE-loop mirror of `snapshot_read_only_numeric_vector`.
    /// Same eligibility (plain contiguous `Vector{Float64}`/`Vector{Int64}`,
    /// `Value::StructRef`) and the same fresh, owned buffer — but also returns
    /// the struct's `StructRef` index, which is everything
    /// `write_back_numeric_vector_buffer` needs to re-resolve the backing
    /// `Memory` and commit the buffer's contents back in place, elementwise,
    /// without reallocating. The buffer itself is never written back through
    /// `struct_heap` directly (that would rebind `values[0]` to a fresh
    /// `Memory`/`MemoryRef`, breaking any other alias of the current one); see
    /// `write_back_numeric_vector_buffer`.
    pub(in crate::vm) fn snapshot_numeric_vector_for_store(
        &self,
        value: &Value,
    ) -> Option<(ArrayRef, usize)> {
        let Value::StructRef(idx) = value else {
            return None;
        };
        let buffer = self.snapshot_read_only_numeric_vector(value)?;
        Some((buffer, *idx))
    }

    /// Issue #10566(c): commit a typed-loop array buffer back into the
    /// EXISTING `Memory` backing the MemoryRef-backed `Vector{T}` struct at
    /// `struct_heap[idx]`, elementwise, in place — never reallocating, so any
    /// other alias sharing the same `Memory` (another view, another binding)
    /// observes the write. `None` means the struct heap no longer matches the
    /// shape this buffer was snapshotted from (never expected in practice: a
    /// typed loop's op set cannot mutate `struct_heap` itself); the caller
    /// treats that as a best-effort no-op rather than propagating an error.
    pub(in crate::vm) fn write_back_numeric_vector_buffer(
        &mut self,
        idx: usize,
        buffer: &ArrayRef,
    ) -> Option<()> {
        let instance = self.struct_heap.get(idx)?;
        instance.array_wrapper_julia_type()?;
        let storage = instance.values.first().cloned()?;
        let size_value = instance.values.get(1)?.clone();
        let (len, offset) = array_wrapper_vector_len_and_offset(&size_value)?;
        let borrow = buffer.borrow();
        if borrow.shape.as_slice() != [len] {
            return None;
        }
        match storage {
            Value::Memory(mem_ref) => {
                let mut mem = mem_ref.borrow_mut();
                for linear in 0..len {
                    let v = borrow.get_linear(linear).ok()?;
                    mem.set(offset + linear, v).ok()?;
                }
            }
            Value::MemoryRef(memref) => {
                let parent = memref.parent();
                let start = memref.memory_index();
                let mut mem = parent.borrow_mut();
                for linear in 0..len {
                    let v = borrow.get_linear(linear).ok()?;
                    mem.set(start + linear, v).ok()?;
                }
            }
            _ => return None,
        }
        Some(())
    }

    /// Execute array builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not an array builtin.
    /// `_try_broadcast_binary_arith` (Issue #8797): upstream-exact
    /// elementwise `+`/`-`/`*` over a 2-argument numeric/complex broadcast,
    /// dispatched once (with a Base-method piracy guard) and executed as one
    /// Rust loop. `Ok(None)` = not applicable; the pure-Julia caller keeps
    /// the generic per-element path (the semantic reference).
    pub(crate) fn try_broadcast_binary_arith(
        &mut self,
        f: &Value,
        a: &Value,
        b: &Value,
    ) -> Result<Option<Value>, VmError> {
        use super::broadcast::{broadcast_binary_arith_exact, BinaryArithOp, Broadcastable};

        let op = match f {
            Value::Function(fv) => match fv.name.as_str() {
                "+" | "Base.+" => BinaryArithOp::Add,
                "-" | "Base.-" => BinaryArithOp::Sub,
                "*" | "Base.*" => BinaryArithOp::Mul,
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };

        // Operands: arrays (wrapper or native) and f64 scalars; at least one
        // array. Ranges, memories, and other scalars keep the generic path
        // (ranges deliberately so — their broadcast algebra has its own
        // upstream semantics, Issue #9659).
        let to_broadcastable = |vm: &Self, v: &Value| -> Result<Option<Broadcastable>, VmError> {
            if let Some(arr) = super::value::array_wrapper_value_to_array_value(v, &vm.struct_heap)?
            {
                return Ok(Some(Broadcastable::Array(arr)));
            }
            if let Some(arr_ref) = super::value::native_array_value_ref(v) {
                return Ok(Some(Broadcastable::Array(arr_ref.borrow().clone())));
            }
            match v {
                Value::F64(x) => Ok(Some(Broadcastable::ScalarF64(*x))),
                // An Int scalar against a Float64/Complex array promotes to
                // f64 per the dispatched Base method; only exactly-
                // representable values qualify (|v| ≤ 2^53), everything else
                // keeps the generic path.
                Value::I64(x) => {
                    let as_f64 = *x as f64;
                    if as_f64 as i64 == *x {
                        Ok(Some(Broadcastable::ScalarF64(as_f64)))
                    } else {
                        Ok(None)
                    }
                }
                _ => Ok(None),
            }
        };
        let Some(ba) = to_broadcastable(self, a)? else {
            return Ok(None);
        };
        let Some(bb) = to_broadcastable(self, b)? else {
            return Ok(None);
        };
        let a_is_array = matches!(&ba, Broadcastable::Array(_));
        let b_is_array = matches!(&bb, Broadcastable::Array(_));
        if !a_is_array && !b_is_array {
            return Ok(None);
        }

        // Base-method piracy guard: the dispatched method for a sample element
        // pair must be a Base method (whose formulas the kernel mirrors).
        let sample = |bc: &Broadcastable, orig: &Value| -> Option<Value> {
            match bc {
                Broadcastable::Array(arr) => {
                    if arr.shape.iter().product::<usize>() == 0 {
                        return None;
                    }
                    arr.get_linear(0).ok()
                }
                _ => Some(orig.clone()),
            }
        };
        let Some(ea) = sample(&ba, a) else {
            return Ok(None);
        };
        let Some(eb) = sample(&bb, b) else {
            return Ok(None);
        };
        let Some(func_index) = self.resolve_runtime_callable_function_index(f, &[ea, eb]) else {
            return Ok(None);
        };
        if func_index >= self.base_function_count {
            return Ok(None);
        }

        let Some(result) = broadcast_binary_arith_exact(&ba, &bb, op)? else {
            return Ok(None);
        };
        crate::vm::profiler::record_event("BroadcastBinaryArithHit");
        Ok(Some(self.array_value_to_wrapper(result)?))
    }

    pub(super) fn execute_builtin_arrays(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            // =========================================================================
            // Array Creation Operations
            // =========================================================================
            BuiltinId::Zeros => {
                // zeros(dims...) - create array of zeros
                // Memory-based: allocate Memory{Float64}, already zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::F64, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::ZerosF64 => {
                // zeros(Float64, dims...) - create Float64 array of zeros
                // Memory-based: allocate Memory{Float64}, already zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::F64, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::ZerosI64 => {
                // zeros(Int64, dims...) - create Int64 array of zeros
                // Memory-based: allocate Memory{Int64}, already zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::I64, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            // Note: `zeros(Complex{Float64}, dims...)` no longer has a dedicated
            // builtin. It routes through pure-Julia `zeros(::Type{T}, ...)` →
            // `_array_undef_from_dims` → the generic typed-allocation path, which
            // maps `Complex{Float64}` to interleaved (re,im) storage (Issue #5156).
            BuiltinId::Ones => {
                // ones(dims...) - create array of ones
                // Memory-based: allocate Memory{Float64} and fill with 1.0 (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr =
                    ArrayValue::memory_first_filled(&ArrayElementType::F64, dims, Value::F64(1.0))?;
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::OnesF64 => {
                // ones(Float64, dims...) - create Float64 array of ones
                // Memory-based: allocate Memory{Float64} and fill with 1.0 (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr =
                    ArrayValue::memory_first_filled(&ArrayElementType::F64, dims, Value::F64(1.0))?;
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::OnesI64 => {
                // ones(Int64, dims...) - create Int64 array of ones
                // Memory-based: allocate Memory{Int64} and fill with 1 (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr =
                    ArrayValue::memory_first_filled(&ArrayElementType::I64, dims, Value::I64(1))?;
                self.push_array_value_as_wrapper(arr)?;
            }

            // Note: Trues, Falses are now Pure Julia (base/array.jl) — Issue #2640
            BuiltinId::AllocUndefF64 => {
                // Array{Float64}(undef, dims...) - create uninitialized Float64 array
                // Memory-based: allocate Memory{Float64}, zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::F64, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::AllocUndefI64 => {
                // Array{Int64}(undef, dims...) - create uninitialized Int64 array
                // Memory-based: allocate Memory{Int64}, zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::I64, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            // Note: `Array{Complex{Float64}}(undef, dims...)` no longer has a
            // dedicated builtin. It routes through the generic typed-allocation
            // path (compiler intercept / pure-Julia `_array_undef_from_dims`),
            // which maps `Complex{Float64}` to interleaved (re,im) storage and
            // tags the array with the Complex struct type_id via
            // `push_undef_typed_array` (Issue #5156).
            BuiltinId::AllocUndefBool => {
                // Array{Bool}(undef, dims...) - create uninitialized Bool array
                // Memory-based: allocate Memory{Bool}, zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::Bool, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::AllocUndefAny => {
                // Array{Any}(undef, dims...) - create uninitialized Any array
                // Memory-based: allocate Memory{Any}, Nothing-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::memory_first_undef(&ArrayElementType::Any, dims);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::LinspaceF64 => {
                // _linspace_range_f64(start, stop, len[, tag]) —
                // TwicePrecision-backed float range for
                // `range(start, stop; length)` (Issue #9419). The optional
                // `tag` selects the element type (0 = Float64, 1 = Float32,
                // 2 = Float16; Issue #9509). Argument validation (negative
                // length, len == 1 with differing endpoints) happens in pure
                // Julia (base/range.jl) with upstream ArgumentError messages;
                // this guard is internal.
                if argc != 3 && argc != 4 {
                    return Err(VmError::TypeError(
                        "_linspace_range_f64 requires three or four arguments".to_string(),
                    ));
                }
                let tag = if argc == 4 { self.stack.pop_i64()? } else { 0 };
                let len = self.stack.pop_i64()?;
                let stop = self.pop_f64_or_i64()?;
                let start = self.pop_f64_or_i64()?;
                if len < 0 || (len == 1 && start != stop) {
                    return Err(VmError::TypeError(
                        "_linspace_range_f64: invalid length (validated in base/range.jl)"
                            .to_string(),
                    ));
                }
                let element_type = Self::range_float_element_for_tag(tag)?;
                self.stack
                    .push(Value::Range(super::value::RangeValue::float_linspace(
                        start,
                        stop,
                        len,
                        element_type,
                    )));
            }

            BuiltinId::SteprangelenF64 => {
                // _steprangelen_range_f64(start, step, len, tag) —
                // TwicePrecision-backed float range for
                // `range(start; step, length)` (Issue #9509), upstream
                // `range_start_step_length(::T, ::T, ::Integer) where
                // T<:IEEEFloat`. Negative-length validation happens in pure
                // Julia (base/range.jl) with the upstream ArgumentError
                // message; this guard is internal.
                if argc != 4 {
                    return Err(VmError::TypeError(
                        "_steprangelen_range_f64 requires exactly four arguments".to_string(),
                    ));
                }
                let tag = self.stack.pop_i64()?;
                let len = self.stack.pop_i64()?;
                let step = self.pop_f64_or_i64()?;
                let start = self.pop_f64_or_i64()?;
                if len < 0 {
                    return Err(VmError::TypeError(
                        "_steprangelen_range_f64: invalid length (validated in base/range.jl)"
                            .to_string(),
                    ));
                }
                let element_type = Self::range_float_element_for_tag(tag)?;
                self.stack
                    .push(Value::Range(super::value::RangeValue::float_steplen(
                        start,
                        step,
                        len,
                        element_type,
                    )));
            }

            BuiltinId::ComplexScaleTpRange => {
                // _try_complex_scale_tp_range_f64(re, im, r) — upstream range
                // broadcast fusion `x::Complex .* r::StepRangeLen{Float64,
                // TwicePrecision, TwicePrecision}` (julia/base/broadcast.jl:1169,
                // Issue #9659): scale ref/step in twice precision and index via
                // the scaled complex lerp, so element values are bit-identical
                // to upstream's lazy scaled range (elementwise `x * r[i]`
                // differs by 1ulp on a large fraction of elements). Pushes
                // `nothing` when `r` is not a TwicePrecision-backed Float64
                // range; the pure-Julia caller falls back to the generic
                // broadcast path.
                if argc != 3 {
                    return Err(VmError::TypeError(
                        "_try_complex_scale_tp_range_f64 requires exactly three arguments"
                            .to_string(),
                    ));
                }
                let range_val = self.stack.pop_value()?;
                let im = self.pop_f64_or_i64()?;
                let re = self.pop_f64_or_i64()?;
                let scaled = match &range_val {
                    Value::Range(r)
                        if matches!(r.element_type, super::value::RangeElementType::Float64) =>
                    {
                        r.float_hp().map(|hp| (hp, r.length()))
                    }
                    _ => None,
                };
                match scaled {
                    Some((hp, len)) => {
                        let scaled_range = hp.scale_complex(super::value::C64::new(re, im));
                        let mut data = Vec::with_capacity((len as usize) * 2);
                        for i in 1..=len {
                            let v = scaled_range.elem(i);
                            data.push(v.re);
                            data.push(v.im);
                        }
                        let arr = ArrayValue::memory_first_from_array_data_with_element_type(
                            super::value::ArrayData::StructF64(data),
                            vec![len as usize],
                            ArrayElementType::ComplexF64,
                        );
                        self.push_array_value_as_wrapper(arr)?;
                    }
                    None => self.stack.push(Value::Nothing),
                }
            }

            BuiltinId::BroadcastTypedKernel => {
                // _try_broadcast_typed_kernel(f, args...) — bulk typed-kernel
                // broadcast (Issues #9693/#8797). Pushes the result array on a
                // hit, `nothing` otherwise (the pure-Julia caller keeps the
                // generic per-element broadcast path).
                if argc < 2 {
                    return Err(VmError::TypeError(
                        "_try_broadcast_typed_kernel requires at least 2 arguments".to_string(),
                    ));
                }
                let mut vals = Vec::with_capacity(argc);
                for _ in 0..argc {
                    vals.push(self.stack.pop_value()?);
                }
                vals.reverse();
                match self.try_broadcast_typed_kernel(&vals[0], &vals[1..]) {
                    Some(arr) => self.push_array_value_as_wrapper(arr)?,
                    None => self.stack.push(Value::Nothing),
                }
            }

            BuiltinId::BroadcastBinaryArith => {
                // _try_broadcast_binary_arith(f, a, b) — upstream-exact
                // elementwise +/-/* broadcast fast path (Issue #8797).
                if argc != 3 {
                    return Err(VmError::TypeError(
                        "_try_broadcast_binary_arith requires exactly three arguments".to_string(),
                    ));
                }
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                let f = self.stack.pop_value()?;
                match self.try_broadcast_binary_arith(&f, &a, &b)? {
                    Some(result) => self.stack.push(result),
                    None => self.stack.push(Value::Nothing),
                }
            }

            BuiltinId::MarkBitVector => {
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "_mark_bitvector requires exactly one argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                if let Some(arr) = value_as_array_ref(&val).cloned() {
                    {
                        let mut borrow = arr.borrow_mut();
                        if borrow.shape.len() != 1
                            || borrow.element_type() != ArrayElementType::Bool
                        {
                            return Err(VmError::TypeError(
                                "_mark_bitvector requires a Bool vector".to_string(),
                            ));
                        }
                        borrow.mark_as_bitvector();
                    }
                    self.push_array_ref(arr);
                } else {
                    let mut arr = super::builtins_linalg::linalg_value_to_array_value(
                        val,
                        &self.struct_heap,
                        "_mark_bitvector",
                        Some("Bool vector"),
                    )?;
                    if arr.shape.len() != 1 || arr.element_type() != ArrayElementType::Bool {
                        return Err(VmError::TypeError(
                            "_mark_bitvector requires a Bool vector".to_string(),
                        ));
                    }
                    arr.mark_as_bitvector();
                    self.push_array_value(arr);
                }
            }

            BuiltinId::MarkBitArray => {
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "_mark_bitarray requires exactly one argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                if let Some(arr) = value_as_array_ref(&val).cloned() {
                    {
                        let mut borrow = arr.borrow_mut();
                        if borrow.element_type() != ArrayElementType::Bool {
                            return Err(VmError::TypeError(
                                "_mark_bitarray requires a Bool array".to_string(),
                            ));
                        }
                        borrow.mark_as_bitarray();
                    }
                    self.push_array_ref(arr);
                } else {
                    let mut arr = super::builtins_linalg::linalg_value_to_array_value(
                        val,
                        &self.struct_heap,
                        "_mark_bitarray",
                        Some("Bool array"),
                    )?;
                    if arr.element_type() != ArrayElementType::Bool {
                        return Err(VmError::TypeError(
                            "_mark_bitarray requires a Bool array".to_string(),
                        ));
                    }
                    arr.mark_as_bitarray();
                    self.push_array_value(arr);
                }
            }

            // Note: Fill is now Pure Julia (base/array.jl) — Issue #2640
            BuiltinId::Similar => {
                // similar(a) - uninitialized array with same element type and shape
                // similar(a, n) - uninitialized array with same element type, length n
                // similar(a, n, m, ...) - same element type, given multi-dim shape (Issue #3751)
                // similar(a, T) - same shape, element type T (Issue #3751)
                // similar(a, T, n, m, ...) - element type T, given shape (Issue #3751)
                // Memory-based: allocate Memory{T} of the same element type (Issue #2762)
                //
                // Pop strategy: collect raw values top-down then reverse to recover
                // source order [arr, arg1, arg2, ...]. The optional second arg may be a
                // DataType (typed form) or an integer dim; remaining args are dims.
                let mut raw_args: Vec<Value> = Vec::with_capacity(argc);
                for _ in 0..argc {
                    raw_args.push(self.stack.pop_value()?);
                }
                raw_args.reverse();
                let args_for_dispatch = raw_args.clone();
                let arr_val = raw_args.remove(0);
                let native_tuple_shape = raw_args.len() == 1
                    && matches!(raw_args[0], Value::Tuple(_))
                    && (value_as_array_ref(&arr_val).is_some()
                        || matches!(arr_val, Value::Memory(_)));
                if !native_tuple_shape {
                    if let Some(func_index) = self
                        .find_best_method_index(&["similar", "Base.similar"], &args_for_dispatch)
                    {
                        self.start_function_call(func_index, args_for_dispatch)?;
                        return Ok(Some(()));
                    }
                }
                // Detect typed form: second arg (if any) is a DataType.
                let typed_eltype: Option<ArrayElementType> =
                    if let Some(Value::DataType(jt)) = raw_args.first() {
                        Some(super::exec::array_basic::array_element_type_from_julia_type(jt))
                    } else {
                        None
                    };
                if typed_eltype.is_some() {
                    raw_args.remove(0); // consume the type
                }
                // The remaining raw_args should all be integer dims.
                let new_shape: Option<Vec<usize>> = if raw_args.is_empty() {
                    None
                } else if raw_args.len() == 1 {
                    if let Value::Tuple(tuple) = &raw_args[0] {
                        let mut dims = Vec::with_capacity(tuple.elements.len());
                        for v in &tuple.elements {
                            let dim = match v {
                                Value::I64(n) if *n >= 0 => *n as usize,
                                other => {
                                    return Err(VmError::TypeError(format!(
                                        "similar: expected integer dimension, got {:?}",
                                        other
                                    )));
                                }
                            };
                            dims.push(dim);
                        }
                        Some(dims)
                    } else {
                        let mut dims = Vec::with_capacity(raw_args.len());
                        for v in raw_args {
                            let dim = match v {
                                Value::I64(n) => {
                                    if n < 0 {
                                        return Err(VmError::TypeError(format!(
                                            "similar: negative dimension {}",
                                            n
                                        )));
                                    }
                                    n as usize
                                }
                                Value::I32(n) => n.max(0) as usize,
                                Value::I16(n) => n.max(0) as usize,
                                Value::I8(n) => n.max(0) as usize,
                                Value::U64(n) => n as usize,
                                Value::U32(n) => n as usize,
                                Value::U16(n) => n as usize,
                                Value::U8(n) => n as usize,
                                other => {
                                    return Err(VmError::TypeError(format!(
                                        "similar: expected integer dimension, got {:?}",
                                        other
                                    )));
                                }
                            };
                            dims.push(dim);
                        }
                        Some(dims)
                    }
                } else {
                    let mut dims = Vec::with_capacity(raw_args.len());
                    for v in raw_args {
                        let dim = match v {
                            Value::I64(n) => {
                                if n < 0 {
                                    return Err(VmError::TypeError(format!(
                                        "similar: negative dimension {}",
                                        n
                                    )));
                                }
                                n as usize
                            }
                            Value::I32(n) => n.max(0) as usize,
                            Value::I16(n) => n.max(0) as usize,
                            Value::I8(n) => n.max(0) as usize,
                            Value::U64(n) => n as usize,
                            Value::U32(n) => n as usize,
                            Value::U16(n) => n as usize,
                            Value::U8(n) => n as usize,
                            other => {
                                return Err(VmError::TypeError(format!(
                                    "similar: expected integer dimension, got {:?}",
                                    other
                                )));
                            }
                        };
                        dims.push(dim);
                    }
                    Some(dims)
                };
                if let Some(arr_ref) = value_as_array_ref(&arr_val) {
                    let borrowed = arr_ref.borrow();
                    let elem_type = typed_eltype
                        .clone()
                        .unwrap_or_else(|| borrowed.element_type());
                    let preserve_bitarray_surface =
                        typed_eltype.is_none() && borrowed.array_type_override().is_some();
                    let shape = if let Some(s) = new_shape.clone() {
                        s
                    } else {
                        borrowed.shape.clone()
                    };
                    drop(borrowed);
                    self.push_similar_array_value(elem_type, shape, preserve_bitarray_surface);
                } else if let Some((wrapper_elem_type, wrapper_shape)) =
                    array_wrapper_similar_spec(&arr_val, &self.struct_heap)
                {
                    let elem_type = typed_eltype.clone().unwrap_or(wrapper_elem_type);
                    let shape = new_shape.clone().unwrap_or(wrapper_shape);
                    self.push_similar_array_value(elem_type, shape, false);
                } else if let Value::Memory(mem_ref) = arr_val {
                    let borrowed = mem_ref.borrow();
                    let elem_type = typed_eltype
                        .clone()
                        .unwrap_or_else(|| borrowed.element_type.clone());
                    // For Memory, only single-dim form is meaningful; if a multi-dim
                    // shape was provided, use the product as the new buffer length.
                    let length = if let Some(s) = &new_shape {
                        s.iter().product()
                    } else {
                        borrowed.len()
                    };
                    drop(borrowed);
                    let new_mem = MemoryValue::undef_typed(&elem_type, length);
                    self.stack.push(Value::Memory(new_memory_ref(new_mem)));
                } else {
                    return Err(VmError::TypeError(
                        "similar requires an array or memory argument".to_string(),
                    ));
                }
            }

            BuiltinId::GetIndex => {
                // Issue #6657: native-indexing fallback for `getindex` reached
                // via `CallTypedDispatchOrBuiltin` when a dynamic (`Any`-typed)
                // receiver did not match any user `getindex` override at runtime.
                // The arguments `[collection, indices...]` are already on the
                // stack; reuse the shared `IndexLoad` path. `IndexLoad` always
                // returns `DispatchAction::Continue` (it may set up a frame for a
                // Dict-/wrapper-backed method, exactly like the standalone
                // instruction), so the action is safely discarded here.
                let _ = self.execute_array_index(&Instr::IndexLoad(argc.saturating_sub(1)))?;
            }

            BuiltinId::Reshape => {
                // reshape(arr, dims...) - reshape array to new dimensions
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                match args.first() {
                    Some(first) => {
                        if let Some(arr_ref) = value_as_array_ref(first) {
                            let new_dims = self.dims_from_values(&args[1..], "reshape")?;
                            let reshaped = ArrayValue::reshaped_from_ref(arr_ref, new_dims)?;
                            self.push_array_value(reshaped);
                        } else if let Some(func_index) =
                            self.find_best_method_index(&["reshape", "Base.reshape"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                        } else {
                            return Err(VmError::TypeError(format!(
                                "reshape: expected Array, got {:?}",
                                args[0]
                            )));
                        }
                    }
                    None => {
                        return Err(VmError::TypeError(
                            "reshape: expected at least one argument".to_string(),
                        ));
                    }
                }
            }

            // =========================================================================
            // Array Mutation Operations
            // =========================================================================
            BuiltinId::Push => {
                // push!(arr, val) - push value to array
                let val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;
                // Issue #7883: grow `Array{T,N}`-over-`Memory` wrappers (the faithful
                // representation backing every `Vector`) in place via the shared
                // amortized-growth path — the SAME path the native `ArrayPush`
                // instruction uses (Issue #6873). This builtin is the fallback reached
                // when `push!(v, x)` is routed through `CallTypedDispatchOrBuiltin`
                // because a user/Base `push!` method exists at the same arity (e.g.
                // `using Plots` defines `push!(::Plot, ::Number)`). Without this, the
                // `array_wrapper_vector_snapshot` path below snapshots and reallocates
                // the entire backing buffer on every call — O(n) per push, O(n^2) for
                // a `push!` accumulation loop. `push_array_wrapper` only intercepts the
                // `Value::StructRef` wrapper case and hands every other value back
                // unchanged, so the snapshot / plain native-array fallbacks below are
                // preserved verbatim.
                let (arr_val, val) = match self.push_array_wrapper(arr_val, val.clone(), false)? {
                    Ok(result) => {
                        self.stack.push(result);
                        return Ok(Some(()));
                    }
                    Err(arr_val) => (arr_val, val),
                };
                if let Ok(mut snapshot) = self.array_wrapper_vector_snapshot(arr_val.clone())? {
                    snapshot.values.push(val);
                    let array_value = self.store_array_wrapper_vector_snapshot(snapshot)?;
                    self.stack.push(array_value);
                } else {
                    let Some(arr_ref) = value_as_array_ref(&arr_val).cloned() else {
                        return Err(VmError::TypeError("push! requires array".to_string()));
                    };
                    ensure_native_array_value_acyclic(&arr_ref, &val)?;
                    let mut arr_mut = arr_ref.borrow_mut();
                    let push_value = if arr_mut.is_struct_ref_array() {
                        match val {
                            Value::Struct(s) => {
                                let idx = self.struct_heap.len();
                                self.struct_heap.push(s);
                                Value::StructRef(idx)
                            }
                            other => other,
                        }
                    } else {
                        val
                    };
                    arr_mut.push(push_value)?;
                    drop(arr_mut);
                    self.push_array_ref(arr_ref);
                }
            }

            BuiltinId::PushFirst => {
                // pushfirst!(arr, val) - prepend value to array
                let val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;
                if let Ok(mut snapshot) = self.array_wrapper_vector_snapshot(arr_val.clone())? {
                    snapshot.values.insert(0, val);
                    let array_value = self.store_array_wrapper_vector_snapshot(snapshot)?;
                    self.stack.push(array_value);
                } else {
                    let Some(arr_ref) = value_as_array_ref(&arr_val).cloned() else {
                        return Err(VmError::TypeError("pushfirst! requires array".to_string()));
                    };
                    ensure_native_array_value_acyclic(&arr_ref, &val)?;
                    arr_ref.borrow_mut().push_first(val)?;
                    self.push_array_ref(arr_ref);
                }
            }

            BuiltinId::Insert => {
                // insert!(arr, index, val) - insert value before index
                let val = self.stack.pop_value()?;
                let index = self.stack.pop_i64()?;
                let arr_val = self.stack.pop_value()?;
                if let Ok(mut snapshot) = self.array_wrapper_vector_snapshot(arr_val.clone())? {
                    if index < 1 || index as usize > snapshot.values.len() + 1 {
                        return Err(VmError::IndexOutOfBounds {
                            indices: vec![index],
                            shape: vec![snapshot.values.len()],
                        });
                    }
                    snapshot.values.insert(index as usize - 1, val);
                    let array_value = self.store_array_wrapper_vector_snapshot(snapshot)?;
                    self.stack.push(array_value);
                } else {
                    let Some(arr_ref) = value_as_array_ref(&arr_val).cloned() else {
                        return Err(VmError::TypeError("insert! requires array".to_string()));
                    };
                    ensure_native_array_value_acyclic(&arr_ref, &val)?;
                    arr_ref.borrow_mut().insert_at(index as usize, val)?;
                    self.push_array_ref(arr_ref);
                }
            }

            BuiltinId::DeleteAt => {
                // deleteat!(arr, index) - remove element at index and return array
                let index = self.stack.pop_i64()?;
                let arr_val = self.stack.pop_value()?;
                if let Ok(mut snapshot) = self.array_wrapper_vector_snapshot(arr_val.clone())? {
                    if index < 1 || index as usize > snapshot.values.len() {
                        return Err(VmError::IndexOutOfBounds {
                            indices: vec![index],
                            shape: vec![snapshot.values.len()],
                        });
                    }
                    snapshot.values.remove(index as usize - 1);
                    let array_value = self.store_array_wrapper_vector_snapshot(snapshot)?;
                    self.stack.push(array_value);
                } else {
                    let Some(arr_ref) = value_as_array_ref(&arr_val).cloned() else {
                        return Err(VmError::TypeError("deleteat! requires array".to_string()));
                    };
                    arr_ref.borrow_mut().delete_at(index as usize)?;
                    self.push_array_ref(arr_ref);
                }
            }

            BuiltinId::Pop => {
                // pop!(arr) - pop last value from array
                let arr_val = self.stack.pop_value()?;
                if matches!(arr_val, Value::StructRef(_)) {
                    let wrapper_result = self.pop_array_wrapper(arr_val);
                    let wrapper_result = match self.try_or_handle(wrapper_result)? {
                        Some(result) => result,
                        None => return Ok(Some(())),
                    };
                    match wrapper_result {
                        Ok((array_value, popped)) => {
                            self.stack.push(array_value);
                            self.stack.push(popped);
                        }
                        Err(other) => {
                            return Err(VmError::TypeError(format!(
                                "pop! requires array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else if let Ok(mut snapshot) =
                    self.array_wrapper_vector_snapshot(arr_val.clone())?
                {
                    let Some(val) = snapshot.values.pop() else {
                        return Err(VmError::EmptyArrayPop);
                    };
                    let array_value = self.store_array_wrapper_vector_snapshot(snapshot)?;
                    self.stack.push(array_value);
                    self.stack.push(val);
                } else {
                    let Some(arr_ref) = value_as_array_ref(&arr_val).cloned() else {
                        return Err(VmError::TypeError("pop! requires array".to_string()));
                    };
                    let val = arr_ref.borrow_mut().pop()?;
                    self.push_array_ref(arr_ref);
                    self.stack.push(val);
                }
            }

            BuiltinId::PopFirst => {
                // popfirst!(arr) - pop first value from array
                let arr_val = self.stack.pop_value()?;
                if matches!(arr_val, Value::StructRef(_)) {
                    let wrapper_result = self.pop_first_array_wrapper(arr_val);
                    let wrapper_result = match self.try_or_handle(wrapper_result)? {
                        Some(result) => result,
                        None => return Ok(Some(())),
                    };
                    match wrapper_result {
                        Ok((array_value, popped)) => {
                            self.stack.push(array_value);
                            self.stack.push(popped);
                        }
                        Err(other) => {
                            return Err(VmError::TypeError(format!(
                                "popfirst! requires array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else if let Ok(mut snapshot) =
                    self.array_wrapper_vector_snapshot(arr_val.clone())?
                {
                    if snapshot.values.is_empty() {
                        return Err(VmError::EmptyArrayPop);
                    }
                    let val = snapshot.values.remove(0);
                    let array_value = self.store_array_wrapper_vector_snapshot(snapshot)?;
                    self.stack.push(array_value);
                    self.stack.push(val);
                } else {
                    let Some(arr_ref) = value_as_array_ref(&arr_val).cloned() else {
                        return Err(VmError::TypeError("popfirst! requires array".to_string()));
                    };
                    let val = arr_ref.borrow_mut().pop_first()?;
                    self.push_array_ref(arr_ref);
                    self.stack.push(val);
                }
            }

            // =========================================================================
            // Array Query Operations
            // =========================================================================
            BuiltinId::Size => {
                // size(arr) or size(arr, dim)
                // Julia: size(::Number) = () (empty tuple) (Issue #2179)
                if argc == 1 {
                    // size(arr) - return tuple of all dimension sizes
                    let val = self.stack.pop_value()?;
                    if matches!(&val, Value::Struct(_) | Value::StructRef(_)) {
                        let args = vec![val];
                        if let Some(func_index) =
                            self.find_best_method_index(&["size", "Base.size"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching size({})",
                            type_name
                        )));
                    }
                    // `size(g::Generator)` delegates to `size(g.iter)` upstream.
                    // sjulia intercepts `size` natively (this builtin) instead of
                    // reaching the Julia method, so route generators here (Issue
                    // #9379). A FILTERED generator's base iterator is conceptually
                    // an `Iterators.Filter`, which has no `size` method → upstream
                    // raises a `MethodError`; mirror that using the same
                    // structural `callable` check as `length` (Issue #9320), not a
                    // type-name string. An UNFILTERED generator dispatches the
                    // Julia `size(g::Generator)` method, whose `size(g.iter)`
                    // reaches the full size machinery for the base iterator.
                    if let Value::Generator(g) = &val {
                        if self.generator_is_filtered(g) {
                            let filter_type = self.filtered_generator_iter_type_name(g);
                            return Err(VmError::MethodError(format!(
                                "no method matching size(::{})",
                                filter_type
                            )));
                        }
                        let args = vec![val];
                        if let Some(func_index) =
                            self.find_best_method_index(&["size", "Base.size"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching size({})",
                            type_name
                        )));
                    }
                    let shape = if let Some(arr_ref) = value_as_array_ref(&val) {
                        arr_ref.borrow().shape.clone()
                    } else {
                        match &val {
                            Value::Range(r) => vec![r.length() as usize],
                            Value::Memory(mem) => vec![mem.borrow().len()],
                            // Issue #7964: StaticArray flat reps — shape is (N,) for vector,
                            // (M, N) for matrix.
                            Value::StaticArray(sv) => {
                                if sv.is_vector() {
                                    vec![sv.len()]
                                } else {
                                    vec![sv.rows, sv.cols]
                                }
                            }
                            Value::StaticArrayInline(sv) => {
                                if sv.is_vector() {
                                    vec![sv.len()]
                                } else {
                                    vec![sv.rows(), sv.cols()]
                                }
                            }
                            // Scalars are 0-dimensional: size(::Number) = ()
                            Value::I64(_)
                            | Value::I32(_)
                            | Value::I16(_)
                            | Value::I8(_)
                            | Value::I128(_)
                            | Value::U8(_)
                            | Value::U16(_)
                            | Value::U32(_)
                            | Value::U64(_)
                            | Value::U128(_)
                            | Value::F64(_)
                            | Value::F32(_)
                            | Value::F16(_)
                            | Value::Bool(_) => vec![],
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "size: expected array, got {:?}",
                                    val
                                )))
                            }
                        }
                    };
                    let elements: Vec<Value> =
                        shape.iter().map(|&d| Value::I64(d as i64)).collect();
                    self.stack.push(Value::Tuple(TupleValue { elements }));
                } else if argc == 2 {
                    // size(arr, dim) - return size of specific dimension
                    let dim = self.stack.pop_usize()?;
                    let val = self.stack.pop_value()?;
                    if matches!(&val, Value::Struct(_) | Value::StructRef(_)) {
                        let args = vec![val, Value::I64(dim as i64)];
                        if let Some(func_index) =
                            self.find_best_method_index(&["size", "Base.size"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching size({}, {})",
                            type_name, dim
                        )));
                    }
                    // Upstream defines only the 1-arg `size(g::Generator)`; the
                    // 2-arg `size(::Generator, ::Integer)` has no method (a
                    // Generator is not an AbstractArray), so BOTH filtered and
                    // unfiltered generators raise a MethodError (Issue #9379).
                    // Route through dispatch to honor any user override, then
                    // fall through to the MethodError upstream produces.
                    if let Value::Generator(_) = &val {
                        let args = vec![val, Value::I64(dim as i64)];
                        if let Some(func_index) =
                            self.find_best_method_index(&["size", "Base.size"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching size({}, {})",
                            type_name, dim
                        )));
                    }
                    let shape = if let Some(arr_ref) = value_as_array_ref(&val) {
                        arr_ref.borrow().shape.clone()
                    } else {
                        match &val {
                            Value::Range(r) => vec![r.length() as usize],
                            Value::Memory(mem) => vec![mem.borrow().len()],
                            Value::StaticArray(sv) => {
                                if sv.is_vector() {
                                    vec![sv.len()]
                                } else {
                                    vec![sv.rows, sv.cols]
                                }
                            }
                            Value::StaticArrayInline(sv) => {
                                if sv.is_vector() {
                                    vec![sv.len()]
                                } else {
                                    vec![sv.rows(), sv.cols()]
                                }
                            }
                            // Scalars are 0-dimensional: size(::Number, d) returns 1 for d >= 1
                            Value::I64(_)
                            | Value::I32(_)
                            | Value::I16(_)
                            | Value::I8(_)
                            | Value::I128(_)
                            | Value::U8(_)
                            | Value::U16(_)
                            | Value::U32(_)
                            | Value::U64(_)
                            | Value::U128(_)
                            | Value::F64(_)
                            | Value::F32(_)
                            | Value::F16(_)
                            | Value::Bool(_) => vec![],
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "size: expected array, got {:?}",
                                    val
                                )))
                            }
                        }
                    };
                    if dim == 0 {
                        return Err(VmError::IndexOutOfBounds {
                            indices: vec![dim as i64],
                            shape,
                        });
                    }
                    let size = shape.get(dim - 1).copied().unwrap_or(1);
                    self.stack.push(Value::I64(size as i64));
                } else {
                    return Err(VmError::TypeError(format!(
                        "size requires 1 or 2 arguments, got {}",
                        argc
                    )));
                }
            }

            BuiltinId::Ndims => {
                // ndims(arr) - return number of dimensions
                // Julia: ndims(::AbstractArray{T,N}) = N, ndims(::Number) = 0
                let val = self.stack.pop_value()?;
                let ndims = if let Some(arr_ref) = value_as_array_ref(&val) {
                    arr_ref.borrow().ndims()
                } else {
                    match &val {
                        Value::Range(_) => 1,
                        Value::Memory(_) => 1,
                        // Issue #7964: StaticArray flat reps — 1 for vector, 2 for matrix.
                        Value::StaticArray(sv) => {
                            if sv.cols == 1 {
                                1
                            } else {
                                2
                            }
                        }
                        Value::StaticArrayInline(sv) => {
                            if sv.is_vector() {
                                1
                            } else {
                                2
                            }
                        }
                        // Issue #2171 / #4878: scalar 0-dim collection carriers
                        // (Number + AbstractChar subtypes) report `ndims == 0`.
                        // Mirrors `Base.ndims(x::Number) = 0` and
                        // `Base.ndims(x::AbstractChar) = 0`. The predicate
                        // lives in `vm/value/predicates.rs` (Issue #4875) so
                        // the carrier set stays in lock-step with `Length`
                        // (`vm/builtins_collections.rs`) and `IndexLoad`
                        // (`vm/exec/array_index.rs`).
                        v if is_scalar_carrier(v) => 0,
                        Value::DataType(jt) => {
                            // Issue #6260: project built-in array type ranks
                            // before generic method dispatch. Otherwise
                            // `ndims(Vector{Int})` can be misrouted to the value
                            // `ndims(a::Array)` method and try to read `_size`
                            // from the DataType object.
                            let type_object = match jt.as_ref() {
                                crate::types::JuliaType::TypeOf(inner) => inner.as_ref(),
                                other => other,
                            };
                            if let Some(rank) = type_object.array_type_ndims() {
                                rank
                            } else {
                                let args = vec![val];
                                if let Some(func_index) =
                                    self.find_best_method_index(&["ndims", "Base.ndims"], &args)
                                {
                                    self.start_function_call(func_index, args)?;
                                    return Ok(Some(()));
                                }
                                return Err(VmError::TypeError(format!(
                                    "ndims: expected array or number, got {:?}",
                                    args[0]
                                )));
                            }
                        }
                        _ => {
                            // Type-level `ndims(T)` for array types (Issue #5118):
                            // read the rank straight off the `DataType`, e.g.
                            // `ndims(Vector{Int}) === 1`, `ndims(Matrix{Int}) === 2`,
                            // `ndims(Array{Int,3}) === 3`. This must run before
                            // value-form method dispatch so a type object cannot
                            // enter `ndims(arr::AbstractArray)`.
                            if let Value::DataType(jt) = &val {
                                let type_object = match jt.as_ref() {
                                    crate::types::JuliaType::TypeOf(inner) => inner.as_ref(),
                                    other => other,
                                };
                                if let Some(rank) = type_object.array_type_ndims() {
                                    self.stack.push(Value::I64(rank as i64));
                                    return Ok(Some(()));
                                }
                            }
                            // Try method dispatch for struct types (e.g., Complex,
                            // Rational) and the pure-Julia type forms (e.g.
                            // `ndims(::Type{T}) where {T<:Number} = 0`).
                            let args = vec![val];
                            if let Some(func_index) =
                                self.find_best_method_index(&["ndims", "Base.ndims"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(Some(()));
                            }
                            return Err(VmError::TypeError(format!(
                                "ndims: expected array or number, got {:?}",
                                args[0]
                            )));
                        }
                    }
                };
                self.stack.push(Value::I64(ndims as i64));
            }

            // BuiltinId::Eltype is handled by builtins_collections.rs (runs before this handler).
            // Do not add Eltype here — it would be dead code (Issue #3031).
            BuiltinId::Keytype => {
                // keytype(x) - return key type of collection
                // For Dict: returns key type (Any in simplified implementation)
                // For Array/Tuple: returns Int64 (index type)
                let val = self.stack.pop_value()?;
                let key_type = if let Some((key, _)) = self.pairs_key_value_types_for_value(&val) {
                    key
                } else if value_as_array_ref(&val).is_some()
                    || matches!(val, Value::Memory(_))
                    || is_array_wrapper_value(&val, &self.struct_heap)
                {
                    crate::types::JuliaType::Int64
                } else {
                    match &val {
                        Value::DataType(jt) => {
                            dict_key_type(jt).unwrap_or(crate::types::JuliaType::Any)
                        }
                        Value::Struct(_) | Value::StructRef(_) => {
                            let jt = self.get_value_julia_type(&val);
                            dict_key_type(&jt).unwrap_or(crate::types::JuliaType::Any)
                        }
                        Value::Memory(_) => crate::types::JuliaType::Int64,
                        Value::Tuple(_) => crate::types::JuliaType::Int64,
                        _ => crate::types::JuliaType::Any,
                    }
                };
                self.stack.push(Value::DataType(Box::new(key_type)));
            }

            BuiltinId::Valtype => {
                // valtype(x) - return value type of collection
                // For Dict: returns value type (Any in simplified implementation)
                // For Array: returns element type (same as eltype)
                let val = self.stack.pop_value()?;
                let val_type = if let Some((_, value)) = self.pairs_key_value_types_for_value(&val)
                {
                    value
                } else if let Some(arr_ref) = value_as_array_ref(&val) {
                    let arr_borrow = arr_ref.borrow();
                    self.array_value_declared_element_julia_type(&arr_borrow)
                } else if let Value::Memory(mem_ref) = &val {
                    let element_type = mem_ref.borrow().element_type().clone();
                    array_element_type_to_julia_type(&element_type)
                } else {
                    match &val {
                        Value::DataType(jt) => {
                            dict_value_type(jt).unwrap_or(crate::types::JuliaType::Any)
                        }
                        Value::StructRef(idx) => self
                            .struct_heap
                            .get(*idx)
                            .and_then(|s| self.array_wrapper_julia_type_resolved(s))
                            .and_then(array_container_element_type)
                            .or_else(|| {
                                let jt = self.get_value_julia_type(&val);
                                dict_value_type(&jt)
                            })
                            .unwrap_or(crate::types::JuliaType::Any),
                        Value::Struct(s) => self
                            .array_wrapper_julia_type_resolved(s)
                            .and_then(array_container_element_type)
                            .or_else(|| {
                                let jt = self.get_value_julia_type(&val);
                                dict_value_type(&jt)
                            })
                            .unwrap_or(crate::types::JuliaType::Any),
                        Value::Tuple(t) => {
                            if t.elements.is_empty() {
                                crate::types::JuliaType::Any
                            } else {
                                t.elements[0].runtime_type()
                            }
                        }
                        _ => crate::types::JuliaType::Any,
                    }
                };
                self.stack.push(Value::DataType(Box::new(val_type)));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
