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
use crate::rng::{randn, RngInstance, RngLike};

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{ArrayValue, ComposedFunctionValue, TupleValue, Value};
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

fn runtime_rand_dim(value: &Value) -> Result<usize, VmError> {
    match value {
        Value::I64(v) if *v >= 0 => Ok(*v as usize),
        Value::I128(v) if *v >= 0 => Ok(*v as usize),
        Value::I32(v) if *v >= 0 => Ok(*v as usize),
        Value::I16(v) if *v >= 0 => Ok(*v as usize),
        Value::I8(v) if *v >= 0 => Ok(*v as usize),
        Value::U64(v) => Ok(*v as usize),
        Value::U128(v) => Ok(*v as usize),
        Value::U32(v) => Ok(*v as usize),
        Value::U16(v) => Ok(*v as usize),
        Value::U8(v) => Ok(*v as usize),
        other => Err(VmError::TypeError(format!(
            "rand/randn expected a non-negative integer dimension, got {:?}",
            other
        ))),
    }
}

impl<R: RngLike> Vm<R> {
    fn runtime_rand_f64_from(
        &mut self,
        explicit_rng: Option<&mut RngInstance>,
        is_randn: bool,
    ) -> f64 {
        match explicit_rng {
            Some(RngInstance::Global) => {
                if is_randn {
                    randn(&mut self.rng)
                } else {
                    self.rng.next_f64()
                }
            }
            Some(rng) => {
                if is_randn {
                    randn(rng)
                } else {
                    rng.next_f64()
                }
            }
            None => {
                if is_randn {
                    randn(&mut self.rng)
                } else {
                    self.rng.next_f64()
                }
            }
        }
    }

    fn runtime_rand_i64_from(&mut self, explicit_rng: Option<&mut RngInstance>) -> i64 {
        let raw = match explicit_rng {
            Some(RngInstance::Global) => self.rng.next_u64(),
            Some(rng) => rng.next_u64(),
            None => self.rng.next_u64(),
        };
        (raw as i64).abs()
    }

    fn execute_runtime_rand_builtin(&mut self, argc: usize, is_randn: bool) -> Result<(), VmError> {
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.stack.pop_value()?);
        }
        args.reverse();

        if args.is_empty() {
            let sample = if is_randn {
                randn(&mut self.rng)
            } else {
                self.rng.next_f64()
            };
            self.stack.push(Value::F64(sample));
            return Ok(());
        }

        let mut explicit_rng = None;
        let mut dims = args.as_slice();
        if let Some(Value::Rng(rng)) = args.first() {
            explicit_rng = Some(rng.clone());
            dims = &args[1..];
        }

        let mut int_array = false;
        if !is_randn {
            if let Some(Value::DataType(julia_type)) = dims.first() {
                match julia_type.name().as_ref() {
                    "Int" | "Int64" => {
                        int_array = true;
                        dims = &dims[1..];
                    }
                    "Float64" => {
                        dims = &dims[1..];
                    }
                    _ => {}
                }
            }
        }

        if dims.is_empty() {
            if int_array {
                let value = if let Some(rng) = explicit_rng.as_mut() {
                    self.runtime_rand_i64_from(Some(rng))
                } else {
                    self.runtime_rand_i64_from(None)
                };
                self.stack.push(Value::I64(value));
            } else {
                let sample = if let Some(rng) = explicit_rng.as_mut() {
                    self.runtime_rand_f64_from(Some(rng), is_randn)
                } else {
                    self.runtime_rand_f64_from(None, is_randn)
                };
                self.stack.push(Value::F64(sample));
            }
            return Ok(());
        }

        let dims = dims
            .iter()
            .map(runtime_rand_dim)
            .collect::<Result<Vec<_>, _>>()?;
        let size: usize = dims.iter().product();
        let mut data = Vec::with_capacity(size);
        for _ in 0..size {
            let sample = if int_array {
                if let Some(rng) = explicit_rng.as_mut() {
                    self.runtime_rand_i64_from(Some(rng)) as f64
                } else {
                    self.runtime_rand_i64_from(None) as f64
                }
            } else if let Some(rng) = explicit_rng.as_mut() {
                self.runtime_rand_f64_from(Some(rng), is_randn)
            } else {
                self.runtime_rand_f64_from(None, is_randn)
            };
            data.push(sample);
        }
        let arr = ArrayValue::memory_first_from_f64(data, dims);
        self.push_array_value_as_wrapper(arr)?;
        Ok(())
    }

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
            BuiltinId::Rand => {
                self.execute_runtime_rand_builtin(argc, false)?;
            }
            BuiltinId::Randn => {
                self.execute_runtime_rand_builtin(argc, true)?;
            }
            BuiltinId::Convert => {
                // convert(T, x) - convert x to type T
                // Uses shared convert_value() to prevent duplicated match arms (Issue #2259).
                let value = self.stack.pop_value()?;
                let target_type = self.stack.pop_value()?;
                let args = vec![target_type.clone(), value.clone()];

                if let Some(func_index) = self.find_best_method_index(&["Base.convert"], &args) {
                    self.start_function_call(func_index, args)?;
                    return Ok(());
                }

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
                    // An `InexactError` means `convert_value` recognized the target
                    // type but the value is out of its range — that is the final,
                    // correct result and must NOT fall back to a pure-Julia
                    // `convert` method. For `Bool`, the generic
                    // `convert(::Type{T}, x) = T(x)` fallback would call the
                    // (missing) `Bool(x)` constructor and mask the error with
                    // "Function 'Bool' not found" (Issue #7970).
                    Err(err @ VmError::InexactError(_)) => return Err(err),
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
            // Reflection / Introspection
            // =========================================================================
            // Note: _fieldnames and _fieldtypes are now handled by execute_builtin_reflection()
            // which is called earlier in the dispatch chain.

            // =========================================================================
            // Tuple Operations
            // =========================================================================
            BuiltinId::TupleFirst => {
                // first(collection) -> first element
                // Public Array first/last routes through Pure Julia indexing;
                // this fallback remains for tuple, range, string, and struct compatibility.
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
                    Value::Range(r) => {
                        // first(range) -> start value (Issue #3550: preserve typed
                        // element type, e.g. UInt8 for `UInt8(1):UInt8(3)`).
                        self.stack.push(r.typed_element(r.start));
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
                            "first: expected Tuple, Range, or String, got {:?}",
                            collection
                        )))
                    }
                }
            }

            BuiltinId::TupleLast => {
                // last(collection) -> last element
                // Public Array first/last routes through Pure Julia indexing;
                // this fallback remains for tuple, range, string, and struct compatibility.
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
                    Value::Range(r) => {
                        // last(range) -> computed last value (Issue #3550: preserve
                        // typed element type).
                        let last_val = r.last().unwrap_or(r.stop);
                        self.stack.push(r.typed_element(last_val));
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
                            "last: expected Tuple, Range, or String, got {:?}",
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
                        if let Value::Generator(generator) = &coll {
                            if self.start_lazy_generator_iterate_call(generator, None)? {
                                return Ok(());
                            }
                        }
                        let result = self.iterate_first(&coll)?;
                        self.stack.push(result);
                    }
                    2 => {
                        // Subsequent iteration: iterate(collection, state)
                        let state = self.stack.pop_value()?;
                        let coll = self.stack.pop_value()?;
                        if let Value::Generator(generator) = &coll {
                            if self.start_lazy_generator_iterate_call(generator, Some(&state))? {
                                return Ok(());
                            }
                        }
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
                // CollectFallback: rangecollect-builtin-entry
                let iter = self.stack.pop_value()?;
                if let Value::Generator(g) = &iter {
                    // Generator requires special handling (calls function for each element)
                    if let Some(result) = self.collect_generator(
                        g.callable.clone(),
                        g.iter.as_ref(),
                        g.result_element_type.clone(),
                    )? {
                        self.stack.push(result);
                    }
                } else {
                    let result = self.collect_iterator(&iter)?;
                    self.stack.push(result);
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
