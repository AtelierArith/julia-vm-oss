//! Builtin function execution for the VM.
//!
//! Builtins are library functions implemented in Rust (Layer 2 in the VM hierarchy).
//! They are one layer above intrinsics (which are CPU-level operations).
//! This corresponds to Julia's `src/builtin_proto.h` and Base functions.

// SAFETY: i64→usize casts are guarded by bounds checks (e.g. `i < 1 || i as usize > bytes.len()`);
// i64→u32 cast for char codepoint is wrapped in char::from_u32 which validates the value.
#![allow(clippy::cast_sign_loss)]

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_utils::normalize_type_for_isa;
use super::util::{format_sprintf, format_value};
use super::value::{
    new_array_ref, ArrayData, ArrayValue, ComposedFunctionValue, DictKey, TupleValue, Value,
};
use super::Vm;

/// Macro for dispatching builtin execution to specialized modules.
///
/// This macro simplifies the common pattern of delegating builtin execution
/// to multiple specialized handler methods. Each method returns `Option<()>`,
/// and the first one to return `Some(())` handles the builtin.
///
/// # Usage
///
/// This macro is designed for internal use within the VM's `execute_builtin` method.
/// It requires a VM instance (`self`) with handler methods that match the signature:
/// `fn handler(&mut self, builtin: &BuiltinId, argc: usize) -> Result<Option<()>, VmError>`
///
/// ```no_run
/// # // This example shows the macro pattern; it cannot run standalone
/// # // as it requires the full VM context.
/// # macro_rules! dispatch_builtin {
/// #     ($self:expr, $builtin:expr, $argc:expr, [$($handler:ident),* $(,)?]) => {};
/// # }
/// # struct MockVm;
/// # impl MockVm {
/// #     fn execute_builtin_math(&mut self, _: &(), _: usize) -> Result<Option<()>, ()> { Ok(None) }
/// #     fn execute_builtin_io(&mut self, _: &(), _: usize) -> Result<Option<()>, ()> { Ok(None) }
/// # }
/// # let mut vm = MockVm;
/// # let builtin = ();
/// # let argc = 0usize;
/// dispatch_builtin!(vm, builtin, argc, [
///     execute_builtin_math,
///     execute_builtin_io,
/// ]);
/// ```
///
/// # Adding New Builtin Categories
/// To add a new category of builtins:
/// 1. Create a new file `builtins_<category>.rs`
/// 2. Implement `fn execute_builtin_<category>(&mut self, builtin: &BuiltinId, argc: usize) -> Result<Option<()>, VmError>`
/// 3. Add the handler to the list in `execute_builtin()`
macro_rules! dispatch_builtin {
    ($self:expr, $builtin:expr, $argc:expr, [$($handler:ident),* $(,)?]) => {
        $(
            if $self.$handler(&$builtin, $argc)?.is_some() {
                return Ok(());
            }
        )*
    };
}

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_builtin(
        &mut self,
        builtin: BuiltinId,
        argc: usize,
    ) -> Result<(), VmError> {
        // Delegate to specialized modules using the dispatch macro.
        // Each handler is tried in order; the first to return Ok(Some(())) wins.
        //
        // DISPATCH CHAIN ORDER AND OWNERSHIP (Issue #3030):
        // Each BuiltinId MUST be handled by exactly one file.
        // Adding the same BuiltinId to multiple files causes silent first-match shadowing.
        // See docs/vm/BUILTIN_OWNERSHIP.md for the authoritative BuiltinId-to-file table.
        //
        //  1. execute_builtin_math         — builtins_math.rs       (Round, Trunc, Fma, ...)
        //  2. execute_builtin_io           — builtins_io.rs         (Print, Println, Read, ...)
        //  3. execute_builtin_collections  — builtins_collections.rs (Length, Eltype, _Eltype)
        //  4. execute_builtin_dicts        — builtins_dicts.rs      (DictGet, DictSet, ...)
        //  5. execute_builtin_sets         — builtins_sets/         (SetNew, SetPush, ...)
        //  6. execute_builtin_numeric      — builtins_numeric.rs    (BigInt, Int8..UInt128, ...)
        //  7. execute_builtin_strings      — builtins_strings.rs    (StringNew, Repr, ...)
        //  8. execute_builtin_arrays       — builtins_arrays.rs     (Zeros, Ones, Size, Push, ...)
        //  9. execute_builtin_types        — builtins_types.rs      (TypeOf, Isa, Sizeof, ...)
        // 10. execute_builtin_reflection   — builtins_reflection/   (Getfield, HasMethod, ...)
        // 11. execute_builtin_equality     — builtins_equality.rs   (Egal, Isequal, Hash, ...)
        // 12. execute_builtin_macro        — builtins_macro/        (Eval, RegexNew, ...)
        // 13. execute_builtin_linalg       — builtins_linalg.rs     (Lu, Det, Svd, ...)
        dispatch_builtin!(
            self,
            builtin,
            argc,
            [
                execute_builtin_math,
                execute_builtin_io,
                execute_builtin_collections,
                execute_builtin_dicts,
                execute_builtin_sets,
                execute_builtin_numeric,
                execute_builtin_strings,
                execute_builtin_arrays,
                execute_builtin_types,
                execute_builtin_reflection,
                execute_builtin_equality,
                execute_builtin_macro,
                execute_builtin_linalg,
            ]
        );

        match builtin {
            // Sum: Now Pure Julia (base/array.jl)

            // =========================================================================
            // Statistics Functions: Now Pure Julia (stdlib/Statistics/src/Statistics.jl)
            // Mean, Var, Varm, Std, Stdm, Median, Middle, Cov, Cor, Quantile
            // =========================================================================

            // =========================================================================
            // Array Creation Operations
            // =========================================================================
            BuiltinId::Zeros => {
                // zeros(dims...) - create array of zeros
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::zeros(dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            BuiltinId::Ones => {
                // ones(dims...) - create array of ones
                let mut dims = Vec::with_capacity(argc);
                for _ in 0..argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = ArrayValue::ones(dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
            }

            // Note: Trues, Falses, Fill are now Pure Julia (base/array.jl) — Issue #2640
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
                // push!(arr, val) - push value to array or set
                let val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;

                match arr_val {
                    Value::Array(arr_ref) => {
                        let mut arr_mut = arr_ref.borrow_mut();
                        // Special handling for struct arrays
                        match (&mut arr_mut.data, &val) {
                            (ArrayData::StructRefs(refs), Value::Struct(s)) => {
                                let idx = self.struct_heap.len();
                                self.struct_heap.push(s.clone());
                                refs.push(idx);
                                arr_mut.shape[0] += 1;
                            }
                            (ArrayData::StructRefs(refs), Value::StructRef(idx)) => {
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
                    Value::Set(mut set) => {
                        // push!(set, val) - add element to set
                        let key = DictKey::from_value(&val)?;
                        set.insert(key);
                        self.stack.push(Value::Set(set));
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "push! requires array or set".to_string(),
                        ))
                    }
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

            // Note: Ndims is handled in builtins_arrays.rs (Issue #2171)

            // =========================================================================
            // Type Operations
            // =========================================================================
            BuiltinId::TypeOf => {
                // typeof(x) - return DataType (the type of the value)
                // Special case: when called with a string literal from ParametrizedTypeExpression
                // (e.g., "Union{Int, Float64}"), parse it as a type name and return the DataType.
                let val = self.stack.pop_value()?;

                // Check if this is a type name literal (from lowered ParametrizedTypeExpression)
                if let Value::Str(type_name_str) = &val {
                    // First try to parse as a known built-in type
                    if let Some(parsed_type) = crate::types::JuliaType::from_name(type_name_str) {
                        self.stack.push(Value::DataType(parsed_type));
                        return Ok(());
                    }
                    // If it looks like a parametric user-defined type (contains '{' and starts with uppercase),
                    // treat it as a struct type. This handles types like "Point{Int64}" correctly.
                    if type_name_str.contains('{')
                        && type_name_str
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                    {
                        let parsed_type =
                            crate::types::JuliaType::from_name_or_struct(type_name_str);
                        self.stack.push(Value::DataType(parsed_type));
                        return Ok(());
                    }
                    // Otherwise, fall through to get the type of the string value (String)
                }

                // Handle StructRef specially - need to resolve from heap
                let julia_type = match &val {
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            // Preserve actual struct name including type parameters
                            crate::types::JuliaType::Struct(s.struct_name.clone())
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                    Value::Struct(s) => {
                        // Preserve actual struct name including type parameters
                        if s.struct_name.is_empty() {
                            crate::types::JuliaType::Any
                        } else {
                            crate::types::JuliaType::Struct(s.struct_name.clone())
                        }
                    }
                    Value::NamedTuple(nt) => {
                        // Julia shows @NamedTuple{a::T1, b::T2} (short form)
                        let fields: Vec<String> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                            .collect();
                        crate::types::JuliaType::Struct(format!(
                            "@NamedTuple{{{}}}",
                            fields.join(", ")
                        ))
                    }
                    Value::Array(arr) => {
                        // Handle Array with StructOf element type specially
                        let arr_borrow = arr.borrow();
                        let elem_type = match arr_borrow.data.element_type() {
                            crate::vm::ArrayElementType::F32 => crate::types::JuliaType::Float32,
                            crate::vm::ArrayElementType::F64 => crate::types::JuliaType::Float64,
                            crate::vm::ArrayElementType::ComplexF32 => {
                                crate::types::JuliaType::Struct("Complex{Float32}".to_string())
                            }
                            crate::vm::ArrayElementType::ComplexF64 => {
                                crate::types::JuliaType::Struct("Complex{Float64}".to_string())
                            }
                            crate::vm::ArrayElementType::I8 => crate::types::JuliaType::Int8,
                            crate::vm::ArrayElementType::I16 => crate::types::JuliaType::Int16,
                            crate::vm::ArrayElementType::I32 => crate::types::JuliaType::Int32,
                            crate::vm::ArrayElementType::I64 => crate::types::JuliaType::Int64,
                            crate::vm::ArrayElementType::U8 => crate::types::JuliaType::UInt8,
                            crate::vm::ArrayElementType::U16 => crate::types::JuliaType::UInt16,
                            crate::vm::ArrayElementType::U32 => crate::types::JuliaType::UInt32,
                            crate::vm::ArrayElementType::U64 => crate::types::JuliaType::UInt64,
                            crate::vm::ArrayElementType::Bool => crate::types::JuliaType::Bool,
                            crate::vm::ArrayElementType::String => crate::types::JuliaType::String,
                            crate::vm::ArrayElementType::Char => crate::types::JuliaType::Char,
                            crate::vm::ArrayElementType::StructOf(type_id) => {
                                // Look up struct name from type_id (index into struct_defs)
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            crate::vm::ArrayElementType::StructInlineOf(type_id, _) => {
                                // Look up struct name from type_id for isbits struct arrays
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            crate::vm::ArrayElementType::Struct => {
                                // For StructRefs arrays, try to get type_id from first element
                                if let crate::vm::ArrayData::StructRefs(ref struct_refs) =
                                    arr_borrow.data
                                {
                                    if let Some(&first_ref_idx) = struct_refs.first() {
                                        if let Some(struct_instance) =
                                            self.struct_heap.get(first_ref_idx)
                                        {
                                            crate::types::JuliaType::Struct(
                                                struct_instance.struct_name.clone(),
                                            )
                                        } else {
                                            crate::types::JuliaType::Any
                                        }
                                    } else {
                                        crate::types::JuliaType::Any
                                    }
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            crate::vm::ArrayElementType::Any => crate::types::JuliaType::Any,
                            crate::vm::ArrayElementType::TupleOf(_) => crate::types::JuliaType::Any,
                        };
                        // Determine array type based on dimensionality
                        match arr_borrow.shape.len() {
                            1 => crate::types::JuliaType::VectorOf(Box::new(elem_type)),
                            2 => crate::types::JuliaType::MatrixOf(Box::new(elem_type)),
                            _ => crate::types::JuliaType::Array, // 3D+ arrays remain as Array
                        }
                    }
                    Value::Dict(d) => {
                        // Return Dict{K, V} with type parameters (Issue #2742)
                        let k = d.key_type.as_deref().unwrap_or("Any");
                        let v = d.value_type.as_deref().unwrap_or("Any");
                        crate::types::JuliaType::Struct(format!("Dict{{{}, {}}}", k, v))
                    }
                    _ => val.runtime_type(),
                };
                self.stack.push(Value::DataType(julia_type));
            }

            BuiltinId::Isa => {
                // isa(x, T) - check if x is of type T
                // T can be DataType (from typeof) or a type identifier
                // Also supports user-defined abstract types: isa(dog, Animal)
                let type_val = self.stack.pop_value()?;
                let val = self.stack.pop_value()?;

                let target_type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    // Allow constructor functions as type arguments (e.g., isa(x, Ref))
                    // In Julia, Ref is both a type and a callable constructor (Issue #2687)
                    Value::Function(f) => f.name.clone(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isa: second argument must be a type, got {:?}",
                            type_val
                        )));
                    }
                };

                // Resolve StructRef to get the actual struct info
                let (struct_name_opt, resolved_val_type) = match &val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            (
                                Some(si.struct_name.clone()),
                                crate::types::JuliaType::Struct(si.struct_name.clone()),
                            )
                        } else {
                            (None, crate::types::JuliaType::Any)
                        }
                    }
                    Value::Struct(si) => (
                        Some(si.struct_name.clone()),
                        crate::types::JuliaType::Struct(si.struct_name.clone()),
                    ),
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        let elem_type = match &arr_borrow.data {
                            ArrayData::F32(_) => crate::types::JuliaType::Float32,
                            ArrayData::F64(_) => crate::types::JuliaType::Float64,
                            ArrayData::I8(_) => crate::types::JuliaType::Int8,
                            ArrayData::I16(_) => crate::types::JuliaType::Int16,
                            ArrayData::I32(_) => crate::types::JuliaType::Int32,
                            ArrayData::I64(_) => crate::types::JuliaType::Int64,
                            ArrayData::U8(_) => crate::types::JuliaType::UInt8,
                            ArrayData::U16(_) => crate::types::JuliaType::UInt16,
                            ArrayData::U32(_) => crate::types::JuliaType::UInt32,
                            ArrayData::U64(_) => crate::types::JuliaType::UInt64,
                            ArrayData::Bool(_) => crate::types::JuliaType::Bool,
                            ArrayData::String(_) => crate::types::JuliaType::String,
                            ArrayData::Char(_) => crate::types::JuliaType::Char,
                            ArrayData::StructRefs(refs) => {
                                if let Some(type_id) = arr_borrow.struct_type_id {
                                    self.struct_defs
                                        .get(type_id)
                                        .map(|def| {
                                            crate::types::JuliaType::Struct(def.name.clone())
                                        })
                                        .unwrap_or(crate::types::JuliaType::Any)
                                } else if let Some(&first_ref_idx) = refs.first() {
                                    self.struct_heap
                                        .get(first_ref_idx)
                                        .map(|s| {
                                            crate::types::JuliaType::Struct(s.struct_name.clone())
                                        })
                                        .unwrap_or(crate::types::JuliaType::Any)
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            ArrayData::Any(values) => {
                                if let Some(first) = values.first() {
                                    match first {
                                        Value::StructRef(idx) => self
                                            .struct_heap
                                            .get(*idx)
                                            .map(|s| {
                                                crate::types::JuliaType::Struct(
                                                    s.struct_name.clone(),
                                                )
                                            })
                                            .unwrap_or(crate::types::JuliaType::Any),
                                        Value::Struct(s) => {
                                            crate::types::JuliaType::Struct(s.struct_name.clone())
                                        }
                                        _ => crate::types::JuliaType::Any,
                                    }
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                        };

                        let array_type = match arr_borrow.shape.len() {
                            1 => crate::types::JuliaType::VectorOf(Box::new(elem_type)),
                            2 => crate::types::JuliaType::MatrixOf(Box::new(elem_type)),
                            _ => crate::types::JuliaType::Array,
                        };
                        (None, array_type)
                    }
                    Value::NamedTuple(nt) => {
                        // NamedTuple is a special type - use concrete type name
                        let fields: Vec<String> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                            .collect();
                        let type_name = format!("@NamedTuple{{{}}}", fields.join(", "));
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    // Ref: isa(Ref(x), Ref) should be true (Issue #2687)
                    // runtime_type() returns inner type, so we need special handling
                    Value::Ref(_) => (None, crate::types::JuliaType::Struct("Ref".to_string())),
                    _ => (None, val.runtime_type()),
                };

                // Check if target is a user-defined struct type (direct match)
                // Normalize both names to handle module prefixes and type aliases
                let normalized_target = normalize_type_for_isa(&target_type_name);
                let is_match = if let Some(ref struct_name) = struct_name_opt {
                    let normalized_struct = normalize_type_for_isa(struct_name);
                    if normalized_struct == normalized_target {
                        true
                    } else {
                        // Check if target is a user-defined abstract type (Issue #2896)
                        let is_abstract_type = self
                            .abstract_type_name_index
                            .contains_key(&target_type_name);

                        if is_abstract_type {
                            // Check struct's parent type chain against abstract type hierarchy
                            self.check_isa_with_abstract_resolved(
                                &struct_name_opt,
                                &target_type_name,
                            )
                        } else {
                            // Standard type check using JuliaType::is_subtype_of
                            let target_type = crate::types::JuliaType::from_name(&target_type_name)
                                .unwrap_or(crate::types::JuliaType::Struct(target_type_name));
                            resolved_val_type.is_subtype_of(&target_type)
                        }
                    }
                } else {
                    // Non-struct value: use standard type check
                    let target_type = crate::types::JuliaType::from_name(&target_type_name)
                        .unwrap_or(crate::types::JuliaType::Struct(target_type_name));
                    resolved_val_type.is_subtype_of(&target_type)
                };
                self.stack.push(Value::Bool(is_match));
            }

            BuiltinId::Subtype => {
                // <: (subtype check)
                // T1 <: T2 checks if T1 is a subtype of T2
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // Get type names from values (DataType, String, or Struct)
                let left_type = match &left {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    Value::Struct(s) => s.struct_name.clone(),
                    _ => format!("{:?}", left),
                };
                let right_type = match &right {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    Value::Struct(s) => s.struct_name.clone(),
                    _ => format!("{:?}", right),
                };

                // Check subtype relationship
                let is_subtype = self.check_subtype(&left_type, &right_type);
                self.stack.push(Value::Bool(is_subtype));
            }

            BuiltinId::Convert => {
                // convert(T, x) - convert x to type T
                // Uses shared convert_value() to prevent duplicated match arms (Issue #2259).
                let value = self.stack.pop_value()?;
                let target_type = self.stack.pop_value()?;

                // Get target type name (from DataType or String)
                let type_name_owned: String;
                let type_name: &str = match &target_type {
                    Value::DataType(jt) => {
                        type_name_owned = jt.name().to_string();
                        &type_name_owned
                    }
                    Value::Str(s) => s.as_str(),
                    _ => {
                        return Err(VmError::TypeError(
                            "convert first argument must be a type".to_string(),
                        ))
                    }
                };

                let converted = super::convert::convert_value(type_name, &value);

                match converted {
                    Ok(val) => self.stack.push(val),
                    Err(err) => {
                        let args = vec![target_type, value];
                        if let Some(func_index) =
                            self.find_best_method_index(&["convert", "Base.convert"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(());
                        }
                        return Err(err);
                    }
                }
            }

            BuiltinId::Promote => {
                // promote(x, y, ...) - promote values to a common type
                // Returns a tuple of values all converted to the same type
                // Always uses Julia's promotion.jl path, matching official Julia behavior.
                let mut values: Vec<Value> = Vec::with_capacity(argc);
                for _ in 0..argc {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse(); // Restore original order

                // Dispatch to Julia promote function from promotion.jl
                if let Some(func_index) =
                    self.find_best_method_index(&["promote", "Base.promote"], &values)
                {
                    self.start_function_call(func_index, values)?;
                    return Ok(());
                }
                // If no Julia promote found, return values unchanged as tuple
                self.stack
                    .push(Value::Tuple(TupleValue { elements: values }));
            }

            // =========================================================================
            // Copy Operations
            // =========================================================================
            BuiltinId::Deepcopy => {
                // deepcopy(x) - recursive deep copy
                let val = self.stack.pop_value()?;
                let copied = self.deep_copy_value(&val)?;
                self.stack.push(copied);
            }

            // =========================================================================
            // Reflection / Introspection
            // =========================================================================
            // Note: _fieldnames and _fieldtypes are now handled by execute_builtin_reflection()
            // which is called earlier in the dispatch chain.

            // =========================================================================
            // Tuple Operations
            // =========================================================================
            BuiltinId::TupleFirst => {
                // first(collection) -> first element
                // Supports Tuple, Array, Range, and user-defined struct types (e.g., LinRange)
                let collection = self.stack.pop_value()?;
                // For struct types, fall back to Julia method dispatch
                if matches!(collection, Value::Struct(_) | Value::StructRef(_)) {
                    let args = vec![collection];
                    if let Some(func_index) =
                        self.find_best_method_index(&["first", "Base.first"], &args)
                    {
                        self.start_function_call(func_index, args)?;
                        return Ok(());
                    }
                    return Err(VmError::TypeError(
                        "first: no method found for struct type".to_string(),
                    ));
                }
                match collection {
                    Value::Tuple(t) => {
                        if t.elements.is_empty() {
                            return Err(VmError::TypeError(
                                "first: collection is empty".to_string(),
                            ));
                        }
                        self.stack.push(t.elements[0].clone());
                    }
                    Value::Array(ref arr) => {
                        let arr_ref = arr.borrow();
                        if arr_ref.is_empty() {
                            return Err(VmError::TypeError(
                                "first: collection is empty".to_string(),
                            ));
                        }
                        // Julia uses 1-based indexing
                        self.stack.push(arr_ref.get(&[1])?);
                    }
                    Value::Range(r) => {
                        // first(range) -> start value
                        // Return as integer if step is 1.0 and start is integer-like
                        if r.step == 1.0 && r.start.fract() == 0.0 {
                            self.stack.push(Value::I64(r.start as i64));
                        } else {
                            self.stack.push(Value::F64(r.start));
                        }
                    }
                    Value::Str(s) => {
                        // first(s::String) -> first character as Char (Issue #2048)
                        if s.is_empty() {
                            return Err(VmError::TypeError("first: string is empty".to_string()));
                        }
                        let ch = s.chars().next().ok_or_else(|| {
                            VmError::TypeError("first: string is empty".to_string())
                        })?;
                        self.stack.push(Value::Char(ch));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "first: expected Tuple or Array, got {:?}",
                            collection
                        )))
                    }
                }
            }

            BuiltinId::TupleLast => {
                // last(collection) -> last element
                // Supports Tuple, Array, Range, and user-defined struct types (e.g., LinRange)
                let collection = self.stack.pop_value()?;
                // For struct types, fall back to Julia method dispatch
                if matches!(collection, Value::Struct(_) | Value::StructRef(_)) {
                    let args = vec![collection];
                    if let Some(func_index) =
                        self.find_best_method_index(&["last", "Base.last"], &args)
                    {
                        self.start_function_call(func_index, args)?;
                        return Ok(());
                    }
                    return Err(VmError::TypeError(
                        "last: no method found for struct type".to_string(),
                    ));
                }
                match collection {
                    Value::Tuple(t) => {
                        if t.elements.is_empty() {
                            return Err(VmError::TypeError(
                                "last: collection is empty".to_string(),
                            ));
                        }
                        let last = t.elements.last().ok_or_else(|| {
                            VmError::TypeError("last: collection is empty".to_string())
                        })?;
                        self.stack.push(last.clone());
                    }
                    Value::Array(ref arr) => {
                        let arr_ref = arr.borrow();
                        let len = arr_ref.len();
                        if len == 0 {
                            return Err(VmError::TypeError(
                                "last: collection is empty".to_string(),
                            ));
                        }
                        // Julia uses 1-based indexing
                        self.stack.push(arr_ref.get(&[len as i64])?);
                    }
                    Value::Range(r) => {
                        // last(range) -> computed last value
                        // For 1:10 (step=1), last is stop
                        // For 1:2:10, last is the largest value <= stop
                        let last_val = if r.step == 1.0 {
                            r.stop
                        } else {
                            // Compute: start + floor((stop - start) / step) * step
                            let n = ((r.stop - r.start) / r.step).floor();
                            r.start + n * r.step
                        };
                        // Return as integer if step is 1.0 and last is integer-like
                        if r.step == 1.0 && last_val.fract() == 0.0 {
                            self.stack.push(Value::I64(last_val as i64));
                        } else {
                            self.stack.push(Value::F64(last_val));
                        }
                    }
                    Value::Str(s) => {
                        // last(s::String) -> last character as Char (Issue #2048)
                        if s.is_empty() {
                            return Err(VmError::TypeError("last: string is empty".to_string()));
                        }
                        let ch = s.chars().last().ok_or_else(|| {
                            VmError::TypeError("last: string is empty".to_string())
                        })?;
                        self.stack.push(Value::Char(ch));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "last: expected Tuple or Array, got {:?}",
                            collection
                        )))
                    }
                }
            }

            BuiltinId::TupleLen => {
                // length(tuple) -> number of elements
                let tuple = self.stack.pop_value()?;
                match tuple {
                    Value::Tuple(t) => {
                        self.stack.push(Value::I64(t.elements.len() as i64));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "length: expected Tuple, got {:?}",
                            tuple
                        )))
                    }
                }
            }

            // =========================================================================
            // Iterator Protocol (Julia-compatible)
            // =========================================================================
            BuiltinId::Iterate => {
                // iterate(collection) -> (element, state) or nothing
                // iterate(collection, state) -> (element, state) or nothing
                match argc {
                    1 => {
                        // First iteration: iterate(collection)
                        let coll = self.stack.pop_value()?;
                        let result = self.iterate_first(&coll)?;
                        self.stack.push(result);
                    }
                    2 => {
                        // Subsequent iteration: iterate(collection, state)
                        let state = self.stack.pop_value()?;
                        let coll = self.stack.pop_value()?;
                        let result = self.iterate_next(&coll, &state)?;
                        self.stack.push(result);
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "iterate requires 1 or 2 arguments".to_string(),
                        ));
                    }
                }
            }

            BuiltinId::RangeCollect => {
                // collect(iterator) -> Array
                // Supports Array, Range, Tuple, Generator
                let iter = self.stack.pop_value()?;
                let result = if let Value::Generator(g) = &iter {
                    // Generator requires special handling (calls function for each element)
                    self.collect_generator(g.func_index, g.iter.as_ref())?
                } else {
                    self.collect_iterator(&iter)?
                };
                self.stack.push(result);
            }

            // =========================================================================
            // String Operations
            // =========================================================================
            BuiltinId::StringNew => {
                // string(args...) - concatenate all arguments into a string
                let mut parts = Vec::with_capacity(argc);
                for _ in 0..argc {
                    let val = self.stack.pop_value()?;
                    parts.push(format_value(&val));
                }
                parts.reverse();
                self.stack.push(Value::Str(parts.join("")));
            }

            BuiltinId::Repr => {
                // repr(x) - return string representation with quotes for strings
                let val = self.stack.pop_value()?;
                let s = match &val {
                    Value::Str(s) => format!("\"{}\"", s),
                    _ => format_value(&val),
                };
                self.stack.push(Value::Str(s));
            }

            BuiltinId::Sprintf => {
                // sprintf(fmt, args...) - C-style formatted string
                // First arg is format string, remaining are values
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

                let args = &values[1..];
                let result = format_sprintf(&fmt, args);
                self.stack.push(Value::Str(result));
            }

            BuiltinId::Ncodeunits => {
                // ncodeunits(s) - number of code units (bytes for UTF-8)
                let s = self.stack.pop_str()?;
                self.stack.push(Value::I64(s.len() as i64));
            }

            BuiltinId::Codeunit => {
                // codeunit(s, i) - get byte at position i (1-indexed)
                let i = self.stack.pop_i64()?;
                let s = self.stack.pop_str()?;
                let bytes = s.as_bytes();
                if i < 1 || i as usize > bytes.len() {
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![i],
                        shape: vec![bytes.len()],
                    });
                }
                self.stack.push(Value::I64(bytes[(i - 1) as usize] as i64));
            }

            BuiltinId::CodeUnits => {
                // codeunits(s) - get all bytes as Vector{UInt8}
                let s = self.stack.pop_str()?;
                let bytes: Vec<u8> = s.as_bytes().to_vec();
                let len = bytes.len();
                let arr = ArrayValue {
                    data: ArrayData::U8(bytes),
                    shape: vec![len],
                    struct_type_id: None,
                    element_type_override: None,
                };
                self.stack.push(Value::Array(new_array_ref(arr)));
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
                        self.stack.push(Value::Bool(haystack.contains(&s)));
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

            // BuiltinId::StringRepeat removed - now Pure Julia in base/strings/basic.jl

            // BuiltinId::StringReverse removed - now Pure Julia in base/strings/basic.jl

            // BuiltinId::StringToInt removed - now Pure Julia (base/parse.jl)
            BuiltinId::StringToFloat => {
                // parse(Float64, s)
                let s = self.stack.pop_str()?;
                match s.trim().parse::<f64>() {
                    Ok(n) => self.stack.push(Value::F64(n)),
                    Err(_) => {
                        return Err(VmError::TypeError(format!(
                            "cannot parse \"{}\" as Float64",
                            s
                        )))
                    }
                }
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

            BuiltinId::IntToChar => {
                // Char(n) - codepoint to char
                let n = self.stack.pop_i64()?;
                match char::from_u32(n as u32) {
                    Some(c) => self.stack.push(Value::Char(c)),
                    None => {
                        return Err(VmError::TypeError(format!("Char: invalid codepoint {}", n)))
                    }
                }
            }

            // =========================================================================
            // Higher-Order Functions
            // =========================================================================
            BuiltinId::Compose => {
                // compose(f, g) - create ComposedFunction
                let inner = self.stack.pop_value()?;
                let outer = self.stack.pop_value()?;

                // Both args must be callable (Function, Closure, or ComposedFunction)
                match (&outer, &inner) {
                    (Value::Function(_), Value::Function(_))
                    | (Value::Function(_), Value::ComposedFunction(_))
                    | (Value::Function(_), Value::Closure(_))
                    | (Value::ComposedFunction(_), Value::Function(_))
                    | (Value::ComposedFunction(_), Value::ComposedFunction(_))
                    | (Value::ComposedFunction(_), Value::Closure(_))
                    | (Value::Closure(_), Value::Function(_))
                    | (Value::Closure(_), Value::ComposedFunction(_))
                    | (Value::Closure(_), Value::Closure(_)) => {
                        self.stack
                            .push(Value::ComposedFunction(ComposedFunctionValue::new(
                                outer, inner,
                            )));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "compose: expected functions, got {:?} and {:?}",
                            outer, inner
                        )));
                    }
                }
            }

            // =========================================================================
            // Module introspection (Julia 1.11+ features)
            // =========================================================================
            BuiltinId::IsExported => {
                // isexported(m::Module, s::Symbol) -> Bool
                // Check if a symbol is exported by a module
                let symbol = self.stack.pop_value()?;
                let module = self.stack.pop_value()?;

                match (&module, &symbol) {
                    (Value::Module(m), Value::Symbol(s)) => {
                        let is_exported = m.exports.contains(&s.as_str().to_string());
                        self.stack.push(Value::Bool(is_exported));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isexported: expected (Module, Symbol), got ({}, {})",
                            super::util::value_type_name(&module),
                            super::util::value_type_name(&symbol)
                        )));
                    }
                }
            }

            BuiltinId::IsPublic => {
                // ispublic(m::Module, s::Symbol) -> Bool
                // Check if a symbol is public in a module (Julia 1.11+)
                // Exported symbols are also considered public
                let symbol = self.stack.pop_value()?;
                let module = self.stack.pop_value()?;

                match (&module, &symbol) {
                    (Value::Module(m), Value::Symbol(s)) => {
                        let symbol_str = s.as_str().to_string();
                        let is_public =
                            m.publics.contains(&symbol_str) || m.exports.contains(&symbol_str);
                        self.stack.push(Value::Bool(is_public));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "ispublic: expected (Module, Symbol), got ({}, {})",
                            super::util::value_type_name(&module),
                            super::util::value_type_name(&symbol)
                        )));
                    }
                }
            }

            // =========================================================================
            // Not yet implemented - fallback to error
            // New builtins are implemented incrementally
            // =========================================================================
            _ => {
                return Err(VmError::NotImplemented(format!(
                    "Builtin {:?} not yet implemented in execute_builtin",
                    builtin
                )));
            }
        }
        Ok(())
    }
}
