//! Array builtin functions for the VM.
//!
//! Array creation, mutation, and query operations.

// SAFETY: i64→usize casts for range lengths are from `r.length()` which returns ≥ 0.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use crate::vm::ArrayElementType;
use super::value::{new_array_ref, new_memory_ref, ArrayValue, MemoryValue, TupleValue, Value};
use super::Vm;

impl<R: RngLike> Vm<R> {
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
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::ZerosF64 => {
                // zeros(Float64, dims...) - create Float64 array of zeros
                // Memory-based: allocate Memory{Float64}, already zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::ZerosI64 => {
                // zeros(Int64, dims...) - create Int64 array of zeros
                // Memory-based: allocate Memory{Int64}, already zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::I64, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::ZerosComplexF64 => {
                // zeros(Complex{Float64}, dims...) - create ComplexF64 array of zeros
                // Memory-based: allocate Memory{Float64} with 2x size for interleaved storage (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                // Each complex number needs 2 f64 values (re, im) in interleaved storage
                let mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len * 2);
                let mut arr =
                    ArrayValue::from_memory_with_override(mem, dims, ArrayElementType::ComplexF64);
                // Store correct Complex type_id (Issue #1804)
                arr.struct_type_id = Some(self.get_complex_type_id());
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::Ones => {
                // ones(dims...) - create array of ones
                // Memory-based: allocate Memory{Float64} and fill with 1.0 (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mut mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len);
                mem.fill(Value::F64(1.0))?;
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::OnesF64 => {
                // ones(Float64, dims...) - create Float64 array of ones
                // Memory-based: allocate Memory{Float64} and fill with 1.0 (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mut mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len);
                mem.fill(Value::F64(1.0))?;
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::OnesI64 => {
                // ones(Int64, dims...) - create Int64 array of ones
                // Memory-based: allocate Memory{Int64} and fill with 1 (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mut mem = MemoryValue::undef_typed(&ArrayElementType::I64, total_len);
                mem.fill(Value::I64(1))?;
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
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
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::AllocUndefI64 => {
                // Array{Int64}(undef, dims...) - create uninitialized Int64 array
                // Memory-based: allocate Memory{Int64}, zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::I64, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::AllocUndefComplexF64 => {
                // Array{Complex{Float64}}(undef, dims...) - create uninitialized ComplexF64 array
                // Memory-based: allocate Memory{Float64} with 2x for interleaved storage (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::F64, total_len * 2);
                let mut arr =
                    ArrayValue::from_memory_with_override(mem, dims, ArrayElementType::ComplexF64);
                // Store the correct Complex type_id from struct_defs so that
                // get()/pop() return structs with the right runtime type_id
                // instead of a hardcoded 0 (Issue #1804)
                arr.struct_type_id = Some(self.get_complex_type_id());
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::AllocUndefBool => {
                // Array{Bool}(undef, dims...) - create uninitialized Bool array
                // Memory-based: allocate Memory{Bool}, zero-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::Bool, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::AllocUndefAny => {
                // Array{Any}(undef, dims...) - create uninitialized Any array
                // Memory-based: allocate Memory{Any}, Nothing-initialized (Issue #2762)
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let total_len: usize = dims.iter().product();
                let mem = MemoryValue::undef_typed(&ArrayElementType::Any, total_len);
                let arr = ArrayValue::from_memory(mem, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            // Note: Fill is now Pure Julia (base/array.jl) — Issue #2640
            BuiltinId::Similar => {
                // similar(a) - uninitialized array with same element type and shape
                // similar(a, n) - uninitialized array with same element type, length n
                // Memory-based: allocate Memory{T} of the same element type (Issue #2762)
                let new_len = if argc == 2 {
                    Some(self.stack.pop_usize()?)
                } else {
                    None
                };
                let arr_val = self.stack.pop_value()?;
                match arr_val {
                    Value::Array(arr_ref) => {
                        let borrowed = arr_ref.borrow();
                        let elem_type = borrowed.element_type();
                        let shape = if let Some(n) = new_len {
                            vec![n]
                        } else {
                            borrowed.shape.clone()
                        };
                        let total_len: usize = shape.iter().product();
                        drop(borrowed);
                        // Use Memory to allocate typed buffer, then wrap as Array
                        let new_arr = match elem_type {
                            ArrayElementType::ComplexF64 => {
                                // Complex needs 2x storage for interleaved re/im
                                let mem = MemoryValue::undef_typed(
                                    &ArrayElementType::F64,
                                    total_len * 2,
                                );
                                let mut arr = ArrayValue::from_memory_with_override(
                                    mem,
                                    shape,
                                    ArrayElementType::ComplexF64,
                                );
                                // Store correct Complex type_id (Issue #1804)
                                arr.struct_type_id = Some(self.get_complex_type_id());
                                arr
                            }
                            _ => {
                                let mem = MemoryValue::undef_typed(&elem_type, total_len);
                                ArrayValue::from_memory(mem, shape)
                            }
                        };
                        self.stack.push(Value::Array(new_array_ref(new_arr)));
                    }
                    Value::Memory(mem_ref) => {
                        let borrowed = mem_ref.borrow();
                        let elem_type = borrowed.element_type.clone();
                        let length = if let Some(n) = new_len {
                            n
                        } else {
                            borrowed.len()
                        };
                        drop(borrowed);
                        let new_mem = MemoryValue::undef_typed(&elem_type, length);
                        self.stack.push(Value::Memory(new_memory_ref(new_mem)));
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "similar requires an array or memory argument".to_string(),
                        ));
                    }
                }
            }

            BuiltinId::Reshape => {
                // reshape(arr, dims...) - reshape array to new dimensions
                // argc includes the array, so dims count = argc - 1
                let mut new_dims = Vec::with_capacity(argc - 1);
                for _ in 0..(argc - 1) {
                    new_dims.push(self.stack.pop_usize()?);
                }
                new_dims.reverse();
                let arr_val = self.stack.pop_value()?;

                match arr_val {
                    Value::Array(arr_ref) => {
                        // Validate size matches using logical element count (not raw data length)
                        // For complex arrays with interleaved storage, raw_len() = 2 * element_count()
                        let old_count = arr_ref.borrow().element_count();
                        let new_len: usize = new_dims.iter().product();
                        if old_count != new_len {
                            return Err(VmError::DimensionMismatch {
                                expected: old_count,
                                got: new_len,
                            });
                        }
                        // Update shape in place
                        arr_ref.borrow_mut().shape = new_dims;
                        self.stack.push(Value::Array(arr_ref));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "reshape: expected Array, got {:?}",
                            arr_val
                        )));
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

                match arr_val {
                    Value::Array(arr_ref) => {
                        let mut arr_mut = arr_ref.borrow_mut();
                        // Special handling for struct arrays
                        match (&mut arr_mut.data, &val) {
                            (super::value::ArrayData::StructRefs(refs), Value::Struct(s)) => {
                                let idx = self.struct_heap.len();
                                self.struct_heap.push(s.clone());
                                refs.push(idx);
                                arr_mut.shape[0] += 1;
                            }
                            (super::value::ArrayData::StructRefs(refs), Value::StructRef(idx)) => {
                                refs.push(*idx);
                                arr_mut.shape[0] += 1;
                            }
                            _ => {
                                arr_mut.push(val)?;
                            }
                        }
                        drop(arr_mut);
                        self.stack.push(Value::Array(arr_ref));
                    }
                    _ => return Err(VmError::TypeError("push! requires array".to_string())),
                }
            }

            BuiltinId::Pop => {
                // pop!(arr) - pop last value from array
                let arr_val = self.stack.pop_value()?;

                match arr_val {
                    Value::Array(arr_ref) => {
                        let val = arr_ref.borrow_mut().pop()?;
                        self.stack.push(Value::Array(arr_ref));
                        self.stack.push(val);
                    }
                    _ => return Err(VmError::TypeError("pop! requires array".to_string())),
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
                    let shape = match &val {
                        Value::Array(arr) => arr.borrow().shape.clone(),
                        Value::Range(r) => vec![r.length() as usize],
                        Value::Memory(mem) => vec![mem.borrow().len()],
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
                    let shape = match &val {
                        Value::Array(arr) => arr.borrow().shape.clone(),
                        Value::Range(r) => vec![r.length() as usize],
                        Value::Memory(mem) => vec![mem.borrow().len()],
                        // Scalars are 0-dimensional: size(::Number, d) is always out of bounds
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
                    };
                    if dim == 0 || dim > shape.len() {
                        return Err(VmError::IndexOutOfBounds {
                            indices: vec![dim as i64],
                            shape,
                        });
                    }
                    self.stack.push(Value::I64(shape[dim - 1] as i64));
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
                let ndims = match &val {
                    Value::Array(arr) => arr.borrow().shape.len(),
                    Value::Range(_) => 1,
                    // Scalar Number types are 0-dimensional (Issue #2171)
                    // Based on Julia's base/number.jl:85: ndims(x::Number) = 0
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
                    | Value::Bool(_) => 0,
                    _ => {
                        // Try method dispatch for struct types (e.g., Complex, Rational)
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
                let key_type = match &val {
                    Value::Dict(_) => crate::types::JuliaType::Any,
                    Value::Array(_) => crate::types::JuliaType::Int64,
                    Value::Tuple(_) => crate::types::JuliaType::Int64,
                    Value::Set(_) => crate::types::JuliaType::Any,
                    _ => crate::types::JuliaType::Any,
                };
                self.stack.push(Value::DataType(key_type));
            }

            BuiltinId::Valtype => {
                // valtype(x) - return value type of collection
                // For Dict: returns value type (Any in simplified implementation)
                // For Array: returns element type (same as eltype)
                let val = self.stack.pop_value()?;
                let val_type = match &val {
                    Value::Dict(_) => crate::types::JuliaType::Any,
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        match arr_borrow.data.element_type() {
                            super::value::ArrayElementType::F32 => crate::types::JuliaType::Float32,
                            super::value::ArrayElementType::F64 => crate::types::JuliaType::Float64,
                            super::value::ArrayElementType::I64 => crate::types::JuliaType::Int64,
                            super::value::ArrayElementType::I32 => crate::types::JuliaType::Int32,
                            super::value::ArrayElementType::Bool => crate::types::JuliaType::Bool,
                            super::value::ArrayElementType::String => {
                                crate::types::JuliaType::String
                            }
                            _ => crate::types::JuliaType::Any,
                        }
                    }
                    Value::Tuple(t) => {
                        if t.elements.is_empty() {
                            crate::types::JuliaType::Any
                        } else {
                            t.elements[0].runtime_type()
                        }
                    }
                    Value::Set(_) => crate::types::JuliaType::Any,
                    _ => crate::types::JuliaType::Any,
                };
                self.stack.push(Value::DataType(val_type));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
