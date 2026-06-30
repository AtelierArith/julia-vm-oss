//! Array builtin functions for the VM.
//!
//! Array creation, mutation, and query operations.

// SAFETY: i64→usize casts for range lengths are from `r.length()` which returns ≥ 0.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{
    array_element_type_to_julia_type, array_wrapper_shape_from_tuple, is_scalar_carrier,
    new_array_ref, new_memory_ref, ArrayRef, ArrayValue, MemoryRefValue, MemoryValue,
    StructInstance, TupleValue, Value,
};
use super::Vm;
use crate::types::JuliaType;
use crate::vm::ArrayElementType;
use crate::vm::Instr;

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
            crate::vm::util::parse_parametric_params(&name)
                .first()
                .map(|param| JuliaType::from_name_or_struct(param))
        }
        _ => None,
    }
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
        let total_len: usize = shape.iter().product();
        let mut new_arr = match elem_type {
            ArrayElementType::ComplexF64 => {
                let mut arr = ArrayValue::memory_first_undef_with_override(
                    &ArrayElementType::F64,
                    total_len * 2,
                    shape,
                    ArrayElementType::ComplexF64,
                );
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

    /// Execute array builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not an array builtin.
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
                if let Ok(mut snapshot) = self.array_wrapper_vector_snapshot(arr_val.clone())? {
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
                if let Ok(mut snapshot) = self.array_wrapper_vector_snapshot(arr_val.clone())? {
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
                            if let Some(rank) = jt.array_type_ndims() {
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
                                if let Some(rank) = jt.array_type_ndims() {
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
                let key_type = if value_as_array_ref(&val).is_some()
                    || matches!(val, Value::Memory(_))
                    || is_array_wrapper_value(&val, &self.struct_heap)
                {
                    crate::types::JuliaType::Int64
                } else {
                    match &val {
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
                let val_type = if let Some(arr_ref) = value_as_array_ref(&val) {
                    let arr_borrow = arr_ref.borrow();
                    self.array_value_declared_element_julia_type(&arr_borrow)
                } else if let Value::Memory(mem_ref) = &val {
                    let element_type = mem_ref.borrow().element_type().clone();
                    array_element_type_to_julia_type(&element_type)
                } else {
                    match &val {
                        Value::StructRef(idx) => self
                            .struct_heap
                            .get(*idx)
                            .and_then(|s| s.array_wrapper_julia_type())
                            .and_then(array_container_element_type)
                            .unwrap_or(crate::types::JuliaType::Any),
                        Value::Struct(s) => s
                            .array_wrapper_julia_type()
                            .and_then(array_container_element_type)
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
