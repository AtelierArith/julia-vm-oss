//! Array creation and storage instructions.
//!
//! Handles: NewArray, PushElem, FinalizeArray, PushArrayValue,
//!          NewArrayTyped, PushElemTyped, FinalizeArrayTyped,
//!          AllocUndefTyped, AllocUndefTypedFromTuple,
//!          AllocUndefDynamicTyped, AllocUndefDynamicTypedFromTuple,
//!          LoadArray, StoreArray

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::DispatchAction;
use crate::rng::RngLike;
use crate::types::JuliaType;
use crate::vm::util::value_type_name;
use crate::vm::value::{new_memory_ref, MemoryRefValue, MemoryValue, StructInstance, TupleValue};

/// Resolve the value to store into a typed `Memory` build buffer's element slot
/// during `PushElemTyped`. Boxes `Struct` values into the heap, materializes an
/// `Int64` into a `Rational` literal when the element type is a rational struct
/// (Issue #5775), and resolves a `StructRef` against the heap for interleaved
/// struct element types. Extracted from the `PushElemTyped` handler to keep it
/// flat (Issue #6833); takes the `struct_defs`/`struct_heap` fields directly so
/// it can run while the build buffer (a `self.stack` element) is borrowed.
fn typed_array_element_push_value(
    val: Value,
    is_struct_ref_array: bool,
    struct_type_id: Option<usize>,
    struct_defs: &[StructDefInfo],
    struct_heap: &mut Vec<StructInstance>,
) -> Value {
    if is_struct_ref_array {
        return match val {
            Value::Struct(s) => {
                let idx = struct_heap.len();
                struct_heap.push(s);
                Value::StructRef(idx)
            }
            Value::StructRef(_) => val,
            Value::I64(i) => {
                let rational = struct_type_id
                    .and_then(|type_id| struct_defs.get(type_id).map(|def| (type_id, &def.name)))
                    .filter(|(_, name)| crate::vm::value::is_rational_type_name(name));
                let Some((type_id, name)) = rational else {
                    return Value::I64(i);
                };
                let struct_name = name.clone();
                let idx = struct_heap.len();
                struct_heap.push(StructInstance::with_name(
                    type_id,
                    struct_name,
                    vec![Value::I64(i), Value::I64(1)],
                ));
                Value::StructRef(idx)
            }
            other => other,
        };
    }
    // A non-struct-ref typed array (e.g. an interleaved `ComplexF64`/`ComplexF32`
    // array) needs the concrete struct value, not a heap reference, to extract
    // its fields. Resolve the `StructRef` against the heap so a verbatim-store
    // typed literal like `ComplexF64[1+2im]` stores the complex element instead
    // of failing with "Cannot push Any to Complex{...} array" (Issue #5775).
    if let Value::StructRef(idx) = val {
        return struct_heap
            .get(idx)
            .map(|s| Value::Struct(s.clone()))
            .unwrap_or(Value::StructRef(idx));
    }
    val
}

/// Map a runtime DataType to the matching ArrayElementType for typed allocation
/// (Issue #3648). Concrete primitive types yield specialized storage. Abstract
/// numeric supertypes use boxed storage with a logical element-type tag so
/// Julia Base widening can report `Vector{Real}` / `Vector{Integer}`.
/// Also reused by `similar(arr, T, dims...)` (Issue #3751).
pub(crate) fn array_element_type_from_julia_type(jt: &JuliaType) -> ArrayElementType {
    match jt {
        JuliaType::Int8 => ArrayElementType::I8,
        JuliaType::Int16 => ArrayElementType::I16,
        JuliaType::Int32 => ArrayElementType::I32,
        JuliaType::Int64 => ArrayElementType::I64,
        JuliaType::Int128 => ArrayElementType::I128,
        JuliaType::UInt8 => ArrayElementType::U8,
        JuliaType::UInt16 => ArrayElementType::U16,
        JuliaType::UInt32 => ArrayElementType::U32,
        JuliaType::UInt64 => ArrayElementType::U64,
        JuliaType::UInt128 => ArrayElementType::U128,
        JuliaType::Bool => ArrayElementType::Bool,
        JuliaType::Float32 => ArrayElementType::F32,
        JuliaType::Float64 => ArrayElementType::F64,
        JuliaType::Number => ArrayElementType::Abstract("Number".to_string()),
        JuliaType::Real => ArrayElementType::Abstract("Real".to_string()),
        JuliaType::Integer => ArrayElementType::Abstract("Integer".to_string()),
        JuliaType::Signed => ArrayElementType::Abstract("Signed".to_string()),
        JuliaType::Unsigned => ArrayElementType::Abstract("Unsigned".to_string()),
        JuliaType::AbstractFloat => ArrayElementType::Abstract("AbstractFloat".to_string()),
        JuliaType::Bottom => ArrayElementType::UnionOf(Vec::new()),
        JuliaType::Nothing => ArrayElementType::Nothing,
        JuliaType::Char => ArrayElementType::Char,
        JuliaType::Symbol => ArrayElementType::Symbol,
        JuliaType::String => ArrayElementType::String,
        JuliaType::TupleOf(types) => ArrayElementType::TupleOf(
            types
                .iter()
                .map(array_element_type_from_julia_type)
                .collect(),
        ),
        // Issue #6720: store the structured union members directly (display and
        // lattice conversion derive the rendered/canonical forms on demand).
        JuliaType::Union(types) => ArrayElementType::UnionOf(types.clone()),
        JuliaType::Struct(name) => match name.as_str() {
            "Tuple{}" => ArrayElementType::TupleOf(Vec::new()),
            "Symbol" => ArrayElementType::Symbol,
            "Complex{Float64}" | "ComplexF64" => ArrayElementType::ComplexF64,
            "Complex{Float32}" | "ComplexF32" => ArrayElementType::ComplexF32,
            "Pair" => ArrayElementType::Abstract("Pair".to_string()),
            _ if name.starts_with("Union{") && name.ends_with('}') => {
                ArrayElementType::union_from_body(&name[6..name.len() - 1])
            }
            _ if name.starts_with("Pair{") => ArrayElementType::Abstract(name.clone()),
            _ if name.starts_with("SubArray{") => ArrayElementType::Abstract(name.clone()),
            _ if name.starts_with("Tuple{") => {
                let parsed = JuliaType::from_name(name).unwrap_or(JuliaType::Any);
                array_element_type_from_julia_type(&parsed)
            }
            _ => ArrayElementType::Any,
        },
        _ => ArrayElementType::Any,
    }
}

/// Like [`array_element_type_from_julia_type`], but resolves a *user struct*
/// element type to a `StructOf(type_id)` tag (instead of falling through to
/// `Any`) by looking the struct name up in `struct_defs`. This preserves the
/// concrete element type for `Vector{T}(undef, n)` / `T[...]` where `T` is a
/// user-defined `struct`/`mutable struct`, so `typeof`/`eltype` report
/// `Vector{T}` rather than `Vector{Any}` (Issue #7304). The `StructOf` tag is
/// resolved back to the struct name by reflection (`type_ops::introspection`).
pub(crate) fn array_element_type_from_julia_type_resolved(
    jt: &JuliaType,
    struct_defs: &[StructDefInfo],
) -> ArrayElementType {
    let base = array_element_type_from_julia_type(jt);
    // Only user structs fall through to `Any`; everything precise is already
    // mapped. Resolve `Struct(name)` → `StructOf(type_id)` when registered.
    if matches!(base, ArrayElementType::Any) {
        if let JuliaType::Struct(name) = jt {
            // Match the struct by its (possibly parametric) base name so a
            // concrete `Foo{Int}` still resolves to the `Foo` definition.
            let base_name = name.split('{').next().unwrap_or(name);
            if let Some(type_id) = struct_defs
                .iter()
                .position(|def| def.name == *name || def.name == base_name)
            {
                return ArrayElementType::StructOf(type_id);
            }
        }
    }
    base
}

/// Extract the inner `ArrayRef` from an owned `Value` that may hold the
/// transitional native Array carrier. Returns `None` for non-Array values so
/// the caller can chain it with `Option::and_then` after a slot/global
/// lookup. Used by `LoadArray` to keep the four owned-array destructure sites
/// routed through a single boundary while the runtime migrates to
/// Memory-first storage and Pure Julia `Array{T,N}` wrappers (Issue #3908).
///
/// Delegates to [`try_consume_array_value`] so the native-array destructure
/// lives in a single place inside this file.
#[inline]
fn native_array_value_into(value: Value) -> Option<ArrayRef> {
    try_consume_array_value(value).ok()
}

fn is_array_wrapper_value(value: &Value, struct_heap: &[StructInstance]) -> bool {
    match value {
        Value::Struct(s) => s.array_wrapper_julia_type().is_some(),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(StructInstance::array_wrapper_julia_type)
            .is_some(),
        _ => false,
    }
}

fn array_like_value_into(value: Value, struct_heap: &[StructInstance]) -> Option<Value> {
    if let Some(arr) = native_array_value_into(value.clone()) {
        return Some(super::super::value::native_array_ref_value(arr));
    }
    if is_array_wrapper_value(&value, struct_heap) {
        return Some(value);
    }
    None
}

/// File-local alias for the shared
/// [`super::super::value::native_array_ref_from_value`] destructure helper. Used
/// by `StoreArray` to keep the native-array destructure routed through a
/// single source of truth across the VM (Issue #3908).
#[inline]
fn try_consume_array_value(value: Value) -> Result<ArrayRef, Value> {
    super::super::value::native_array_ref_from_value(value)
}

impl<R: RngLike> Vm<R> {
    fn array_dims_from_tuple(&self, tuple: &TupleValue) -> Result<Vec<usize>, VmError> {
        let mut dims = Vec::with_capacity(tuple.len());
        for value in &tuple.elements {
            let dim = self.convert_to_i64(value)?;
            if dim < 0 {
                return Err(VmError::TypeError(format!(
                    "expected non-negative integer, got {}",
                    dim
                )));
            }
            dims.push(dim as usize);
        }
        Ok(dims)
    }

    /// Push the MemoryRef-backed `Array{T,N}` wrapper equivalent of a freshly
    /// built native [`ArrayValue`] onto the stack (Issue #6806).
    ///
    /// Array *producers* (literals, comprehensions, undef constructors, range
    /// materialization, RNG arrays, matrix ops, ...) emit the Pure Julia wrapper
    /// directly so the VM's array output is unified onto the `MemoryRef`
    /// representation, matching the public constructors that already return
    /// wrappers (`zeros`/`collect`/`similar`, Issue #6653). The conversion reuses
    /// the shared `ArrayData` storage through [`crate::vm::value::MemoryValue`].
    /// Shared across exec modules as producers migrate off the native carrier
    /// (Issue #6807).
    pub(crate) fn push_array_value_as_wrapper(&mut self, arr: ArrayValue) -> Result<(), VmError> {
        let wrapper = self.array_value_to_wrapper(arr)?;
        self.stack.push(wrapper);
        Ok(())
    }

    /// Build (but do not push) the MemoryRef-backed `Array{T,N}` wrapper
    /// equivalent of a freshly built native [`ArrayValue`] (Issue #6806).
    ///
    /// The returning companion of [`Self::push_array_value_as_wrapper`], for
    /// producers in return position (`Ok(...)`) rather than stack-push position
    /// (e.g. `@eval` vector literals, reflection helpers). Both route through the
    /// shared `array_wrapper_value_from_array_value` conversion so the wrapper
    /// representation stays in one place as producers migrate off the native
    /// carrier (Issue #6807).
    pub(crate) fn array_value_to_wrapper(&mut self, arr: ArrayValue) -> Result<Value, VmError> {
        let type_id = self.get_array_type_id();
        crate::vm::value::array_wrapper_value_from_array_value(arr, type_id, &mut self.struct_heap)
    }

    /// Push the MemoryRef-backed `Array{T,N}` wrapper equivalent of a freshly
    /// built native [`ArrayRef`] onto the stack (Issue #6806).
    ///
    /// Companion of [`Self::push_array_value_as_wrapper`] for producers that hold
    /// an owned [`ArrayRef`] result rather than an [`ArrayValue`] (e.g. the HOF
    /// broadcast/map `dest` buffer). Moves the inner [`ArrayValue`] out when the
    /// ref is uniquely owned (the common producer case), else clones it, then
    /// defers to the shared wrapper conversion. Shared across exec modules as
    /// producers migrate off the native carrier (Issue #6807).
    pub(crate) fn push_array_ref_as_wrapper(&mut self, arr_ref: ArrayRef) -> Result<(), VmError> {
        let arr = match std::rc::Rc::try_unwrap(arr_ref) {
            Ok(cell) => cell.into_inner(),
            Err(shared) => shared.borrow().clone(),
        };
        self.push_array_value_as_wrapper(arr)
    }

    /// Finalize the `Value::Memory` build buffer left on top of the stack by the
    /// incremental build (`FinalizeArray`/`FinalizeArrayTyped`) — and, since
    /// Issue #6846, every array *literal* — into the equivalent `Array{T,N}`
    /// wrapper with the requested `shape`.
    ///
    /// The build buffer is the de-varianted replacement for the legacy
    /// native-array carrier: a flat, growable [`MemoryValue`]. The wrapper is a
    /// `StructInstance{ref: MemoryRef, size: NTuple}`, so we point its `ref`
    /// field directly at the finished `Memory` — a zero-copy `MemoryRef` view,
    /// mirroring the pure-Julia `wrap(::Type{Array}, m, dims) =
    /// _array_construct(T, memoryref(m), dims)`. Wrapping the `Memory` in place
    /// (rather than reconstructing an `ArrayValue` and re-materializing its
    /// storage element-by-element) is correct for *every* element-type layout —
    /// interleaved `Complex`, AoS isbits structs, boxed `Any` — which the
    /// `ArrayValue` round-trip mishandled for non-primitive element types
    /// (Issue #6846).
    fn finalize_memory_build_buffer(&mut self, shape: Vec<usize>) -> Result<(), VmError> {
        let value = self.stack.pop_value()?;
        let Value::Memory(memref) = value else {
            // INTERNAL: NewArray*/Finalize* are a contiguous compiler-emitted
            // sequence, so the build buffer is always a Memory at this point.
            return Err(VmError::InternalError(
                "FinalizeArray: expected Memory build buffer on stack (compiler invariant)"
                    .to_string(),
            ));
        };
        let element_type = memref.borrow().element_type.clone();
        let ndims = shape.len();
        let size_elems = shape
            .iter()
            .map(|dim| {
                i64::try_from(*dim).map(Value::I64).map_err(|_| {
                    VmError::TypeError("Array dimension exceeds Int64 range".to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let storage = Value::MemoryRef(Box::new(MemoryRefValue::first(memref)));
        let size = Value::Tuple(TupleValue::new(size_elems));
        let struct_name = format!("Array{{{}, {}}}", element_type.julia_type_name(), ndims);
        let type_id = self.get_array_type_id();
        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            struct_name,
            vec![storage, size],
        ));
        self.stack.push(Value::StructRef(idx));
        Ok(())
    }

    fn push_undef_typed_array(
        &mut self,
        elem_type: &ArrayElementType,
        dims: Vec<usize>,
    ) -> Result<(), VmError> {
        let mut arr = ArrayValue::memory_first_undef(elem_type, dims);
        if matches!(
            elem_type,
            ArrayElementType::ComplexF64 | ArrayElementType::ComplexF32
        ) {
            arr.struct_type_id = Some(self.get_complex_type_id());
        }
        self.push_array_value_as_wrapper(arr)
    }

    /// Execute array creation and storage instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not an array basic operation.
    #[inline]
    pub(super) fn execute_array_basic(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::NewArray(capacity) => {
                // The incremental build buffer is a flat, growable `Value::Memory`
                // (the de-varianted replacement for the legacy native carrier,
                // Issue #6807). `FinalizeArray` wraps it into the Array{T,N}
                // wrapper. Untyped `NewArray` always builds F64 storage.
                let mem = MemoryValue::with_capacity(ArrayElementType::F64, *capacity);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(DispatchAction::Continue)
            }

            Instr::PushArrayValue(arr) => {
                // Array literals emit the MemoryRef-backed Array{T,N} wrapper
                // directly instead of the native carrier (Issue #6806).
                self.push_array_value_as_wrapper((**arr).clone())?;
                Ok(DispatchAction::Continue)
            }

            Instr::ReserveArray => {
                // Pop the requested element count, then reserve that much backing
                // capacity on the build buffer left on the stack (Issue #5186). A
                // pure capacity hint: a non-positive count or a non-Memory operand
                // (e.g. a finished wrapper) is a no-op so the instruction can never
                // affect observable results.
                let count = self.stack.pop_value()?;
                let additional = match count {
                    Value::I64(n) if n > 0 => usize::try_from(n).unwrap_or(0),
                    _ => 0,
                };
                if additional > 0 {
                    if let Some(Value::Memory(memref)) = self.stack.last_mut() {
                        memref.borrow_mut().reserve(additional);
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PushElem => {
                let val = self.pop_f64_or_i64()?;
                match self.stack.last_mut() {
                    Some(Value::Memory(memref)) => {
                        memref.borrow_mut().push_f64(val)?;
                    }
                    _ => {
                        // INTERNAL: compiler always emits NewArray before PushElem.
                        return Err(VmError::InternalError(
                            "PushElem: expected Memory build buffer on stack (compiler invariant)"
                                .to_string(),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::FinalizeArray(shape) => {
                // Wrap the finished `Value::Memory` build buffer into the
                // Array{T,N} wrapper with the requested shape (Issue #6807).
                self.finalize_memory_build_buffer(shape.clone())?;
                Ok(DispatchAction::Continue)
            }

            Instr::NewArrayTyped(ref elem_type, capacity) => {
                let mem = MemoryValue::with_capacity(elem_type.clone(), *capacity);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(DispatchAction::Continue)
            }

            Instr::PushElemTyped => {
                let val = self.stack.pop_value()?;
                match self.stack.last_mut() {
                    Some(Value::Memory(memref)) => {
                        let mut mem = memref.borrow_mut();
                        let struct_type_id = match &mem.element_type {
                            ArrayElementType::StructOf(type_id)
                            | ArrayElementType::StructInlineOf(type_id, _) => Some(*type_id),
                            _ => None,
                        };
                        let is_struct_ref_array = mem.is_struct_ref_array();
                        let push_value = typed_array_element_push_value(
                            val,
                            is_struct_ref_array,
                            struct_type_id,
                            &self.struct_defs,
                            &mut self.struct_heap,
                        );
                        mem.push(push_value)?;
                    }
                    _ => {
                        // INTERNAL: compiler always emits NewArrayTyped before PushElemTyped.
                        return Err(VmError::InternalError(
                            "PushElemTyped: expected Memory build buffer on stack (compiler invariant)"
                                .to_string(),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::FinalizeArrayTyped(shape) => {
                // Wrap the finished `Value::Memory` build buffer into the
                // Array{T,N} wrapper with the requested shape (Issue #6807).
                self.finalize_memory_build_buffer(shape.clone())?;
                Ok(DispatchAction::Continue)
            }

            Instr::AllocUndefTyped(ref elem_type, argc) => {
                // Generic Array{T}(undef, dims...) for all element types (Issue #2218)
                let mut dims = Vec::with_capacity(*argc);
                for _ in 0..*argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                self.push_undef_typed_array(elem_type, dims)?;
                Ok(DispatchAction::Continue)
            }

            Instr::AllocUndefTypedFromTuple(ref elem_type) => {
                let dims_value = self.stack.pop_value()?;
                let dims = match dims_value {
                    Value::Tuple(tuple) => self.array_dims_from_tuple(&tuple)?,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Array undef tuple constructor: expected Tuple, got {:?}",
                            value_type_name(&other)
                        )));
                    }
                };
                self.push_undef_typed_array(elem_type, dims)?;
                Ok(DispatchAction::Continue)
            }

            Instr::AllocUndefDynamicTyped(argc) => {
                // Vector{T}(undef, dims...) where T is a runtime DataType (Issue #3648)
                let mut dims = Vec::with_capacity(*argc);
                for _ in 0..*argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let type_val = self.stack.pop_value()?;
                let elem_type = match type_val {
                    Value::DataType(jt) => {
                        array_element_type_from_julia_type_resolved(&jt, &self.struct_defs)
                    }
                    _ => ArrayElementType::Any,
                };
                self.push_undef_typed_array(&elem_type, dims)?;
                Ok(DispatchAction::Continue)
            }

            Instr::AllocUndefDynamicTypedFromTuple => {
                let dims_value = self.stack.pop_value()?;
                let dims = match dims_value {
                    Value::Tuple(tuple) => self.array_dims_from_tuple(&tuple)?,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Array undef tuple constructor: expected Tuple, got {:?}",
                            value_type_name(&other)
                        )));
                    }
                };
                let type_val = self.stack.pop_value()?;
                let elem_type = match type_val {
                    Value::DataType(jt) => {
                        array_element_type_from_julia_type_resolved(&jt, &self.struct_defs)
                    }
                    _ => ArrayElementType::Any,
                };
                self.push_undef_typed_array(&elem_type, dims)?;
                Ok(DispatchAction::Continue)
            }

            Instr::LoadArray(name) => {
                // First check slots and locals in current frame
                if let Some(frame) = self.frames.last() {
                    if let Some(value) = self
                        .load_slot_value_by_name(frame, name)
                        .and_then(|value| array_like_value_into(value, &self.struct_heap))
                    {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                    // Check if it's a TypedArray in locals_any
                    if let Some(value) = frame
                        .locals_any
                        .get(name)
                        .cloned()
                        .and_then(|value| array_like_value_into(value, &self.struct_heap))
                    {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                }
                // Fall back to global frame if present
                if self.frames.len() > 1 {
                    if let Some(frame) = self.frames.first() {
                        if let Some(value) = self
                            .load_slot_value_by_name(frame, name)
                            .and_then(|value| array_like_value_into(value, &self.struct_heap))
                        {
                            self.stack.push(value);
                            return Ok(DispatchAction::Continue);
                        }
                        if let Some(value) = frame
                            .locals_any
                            .get(name)
                            .cloned()
                            .and_then(|value| array_like_value_into(value, &self.struct_heap))
                        {
                            self.stack.push(value);
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                // Variable not found - raise error instead of creating empty array
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(DispatchAction::Continue)
            }

            Instr::StoreArray(name) => {
                let val = self.stack.pop_value()?;
                match try_consume_array_value(val) {
                    Ok(arr) => {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.remove_var(name);
                            frame
                                .locals_any
                                .insert(name.clone(), native_array_ref_value(arr));
                            frame
                                .var_types
                                .insert(name.clone(), frame::VarTypeTag::Array);
                        }
                    }
                    Err(val @ (Value::Struct(_) | Value::StructRef(_))) => {
                        // Runtime fallback: store Set/StructRef/Dict in locals_any
                        // (Issue #1828, Issue #2748). Array wrappers are StructRef
                        // values, but StoreArray/LoadArray still treat them as arrays
                        // during the Memory-backed construction migration (Issue #6649).
                        let tag = if is_array_wrapper_value(&val, &self.struct_heap) {
                            frame::VarTypeTag::Array
                        } else {
                            frame::VarTypeTag::Any
                        };
                        if let Some(frame) = self.frames.last_mut() {
                            frame.remove_var(name);
                            frame.locals_any.insert(name.clone(), val);
                            frame.var_types.insert(name.clone(), tag);
                        }
                    }
                    Err(other) => {
                        // INTERNAL: StoreArray is emitted only when the compiler typed the variable as Array; wrong type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "StoreArray: expected Array or Set, got {:?}",
                            util::value_type_name(&other)
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

#[cfg(test)]
mod typed_array_push_tests {
    use super::typed_array_element_push_value;
    use crate::vm::value::{StructInstance, Value};

    #[test]
    fn non_struct_array_passes_value_through() {
        let mut heap: Vec<StructInstance> = Vec::new();
        let out = typed_array_element_push_value(Value::I64(42), false, None, &[], &mut heap);
        assert!(matches!(out, Value::I64(42)));
        assert!(heap.is_empty());
    }

    #[test]
    fn struct_array_boxes_struct_into_heap() {
        let mut heap: Vec<StructInstance> = Vec::new();
        let s = StructInstance::with_name(0, "Foo".to_string(), vec![]);
        let out = typed_array_element_push_value(Value::Struct(s), true, None, &[], &mut heap);
        assert!(matches!(out, Value::StructRef(0)));
        assert_eq!(heap.len(), 1);
    }

    #[test]
    fn struct_array_i64_without_rational_type_stays_i64() {
        let mut heap: Vec<StructInstance> = Vec::new();
        // struct_type_id None => no rational materialization, value stays I64.
        let out = typed_array_element_push_value(Value::I64(3), true, None, &[], &mut heap);
        assert!(matches!(out, Value::I64(3)));
        assert!(heap.is_empty());
    }
}
