//! Higher-order function operations for the VM.
//!
//! This module handles HOF instructions:
//! - MapFunc: Apply function to each element
//! - MapFuncInPlace: Apply function in-place (map!)
//! - FilterFunc: Filter elements by predicate

// SAFETY: i64→usize cast for range lengths uses `r.length()` which returns ≥ 0;
// i64→usize for n-tuple count is from the instruction operand, always non-negative.
#![allow(clippy::cast_sign_loss)]
//! - FilterFuncInPlace: Filter elements in-place (filter!)
//! - ReduceFunc, ReduceFuncWithInit: Reduce array to single value
//! - MapFoldrFunc, MapFoldrFuncWithInit: Map then right-fold
//! - SumFunc: Sum with function applied
//! - AnyFunc, AllFunc: Check if any/all elements satisfy predicate
//! - CountFunc: Count elements satisfying predicate
//! - NtupleFunc: Create tuple by calling function for each index
//! - MakeGenerator: Create a generator from iterator and function
//!
//! Note: ForEachFunc removed - foreach is now Pure Julia (base/abstractarray.jl)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{GeneratorCallable, GeneratorValue, TupleValue, Value};
use super::super::Vm;
use subset_julia_vm_bytecode::GeneratorCallableSpec;

fn generator_callable_from_spec(spec: &GeneratorCallableSpec) -> GeneratorCallable {
    match spec {
        GeneratorCallableSpec::FunctionIndex(func_index) => {
            GeneratorCallable::FunctionIndex(*func_index)
        }
        GeneratorCallableSpec::FilteredFunctionIndex {
            map_func_index,
            predicate_func_index,
        } => GeneratorCallable::FilteredFunctionIndex {
            map_func_index: *map_func_index,
            predicate_func_index: *predicate_func_index,
        },
        GeneratorCallableSpec::TupleSplatFunctionIndex(func_index) => {
            GeneratorCallable::TupleSplatFunctionIndex(*func_index)
        }
    }
}

/// Extract the numeric value parameter `N` from a `Val{N}` type name.
///
/// Returns `Some(N)` when `name` is exactly `Val{<integer literal>}`, e.g.
/// `"Val{3}"` -> `Some(3)`. Used so `ntuple(f, Val(N))` can recover the length
/// directly from the `Val` wrapper struct (Issue #4975).
fn val_struct_numeric_param(name: &str) -> Option<i64> {
    let rest = name.strip_prefix("Val")?;
    let inner = rest.strip_prefix('{')?.strip_suffix('}')?;
    inner.trim().parse::<i64>().ok()
}

impl<R: RngLike> Vm<R> {
    /// Pop the `ntuple` length argument, accepting either an integer value or a
    /// `Val{N}` length wrapper (Issue #4975).
    ///
    /// Upstream Julia exposes `ntuple(f, ::Val{N})` so callers can pass the
    /// length as a compile-time `Val` value (e.g. `ntuple(identity, Val(3))`).
    /// In this VM `ntuple` is a builtin HOF whose length operand is normally an
    /// integer; when a `Val{N}` struct arrives instead we recover `N` from the
    /// struct's parametric type name.
    fn pop_ntuple_length(&mut self) -> Result<i64, VmError> {
        let value = self.stack.pop_value()?;
        let struct_name: Option<String> = match &value {
            Value::Struct(s) => Some(s.struct_name.to_string()),
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .map(|s| s.struct_name.to_string()),
            _ => None,
        };
        if let Some(name) = struct_name {
            if let Some(n) = val_struct_numeric_param(&name) {
                return Ok(n);
            }
        }
        // Not a Val{N} wrapper: push the value back and use the normal numeric
        // coercion so non-integer arguments report the usual type error.
        self.stack.push(value);
        self.stack.pop_i64()
    }

    /// Execute higher-order function instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_hof(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            // Note: MapFunc, FilterFunc removed - now Pure Julia (base/iterators.jl)
            Instr::NtupleFunc(func_index) => {
                // ntuple(f, n) - Create tuple by calling f(i) for i in 1:n.
                // `n` may be an integer or a `Val{N}` length wrapper (Issue #4975).
                let n = self.pop_ntuple_length()?;
                if n < 0 {
                    // User-visible: user can call ntuple with a negative n argument
                    return Err(VmError::TypeError(
                        "ntuple: n must be non-negative".to_string(),
                    ));
                }
                if n == 0 {
                    // Return empty tuple
                    self.stack.push(Value::Tuple(TupleValue::new(vec![])));
                } else {
                    let indices: Vec<Value> = (1..=n).map(Value::I64).collect();
                    self.start_ntuple_call(*func_index, indices)?;
                }
                Ok(DispatchAction::Continue)
            }

            Instr::NtupleRuntime => {
                // ntuple(f, n) where f is a runtime Function/Closure value.
                // `n` may be an integer or a `Val{N}` length wrapper (Issue #4975).
                let n = self.pop_ntuple_length()?;
                let callable = self.stack.pop_value()?;
                if n < 0 {
                    // User-visible: ntuple length arguments must be non-negative.
                    return Err(VmError::TypeError(
                        "ntuple: n must be non-negative".to_string(),
                    ));
                }
                let indices: Vec<Value> = (1..=n).map(Value::I64).collect();
                self.start_ntuple_runtime_call(callable, indices)?;
                Ok(DispatchAction::Continue)
            }

            Instr::MakeGenerator(operands) => {
                // Pop the underlying iterator and create a Generator
                let iter = self.stack.pop_value()?;
                // Issue #9127: a Dict / KeySet / ValueIterator (or user iterable)
                // base is materialized eagerly so lazy-generator consumers can
                // drive it; the body mapping stays lazy.
                let iter = self.materialize_generator_base_if_needed(iter)?;
                let generator = GeneratorValue::with_result_element_type(
                    generator_callable_from_spec(&operands.callable),
                    iter,
                    operands.result_element_type.clone(),
                );
                self.stack.push(Value::Generator(Box::new(generator)));
                Ok(DispatchAction::Continue)
            }

            Instr::MakeGeneratorRuntime(tuple_splat, result_element_type) => {
                let iter = self.stack.pop_value()?;
                let callable = self.stack.pop_value()?;
                // Issue #9127: materialize a pure-Julia-iterable base (Dict / …)
                // before wrapping, so every consumer can drive it (see above).
                let iter = self.materialize_generator_base_if_needed(iter)?;
                let (generator_callable, result_element_type) = self
                    .runtime_generator_callable_and_eltype(
                        callable,
                        &iter,
                        *tuple_splat,
                        result_element_type.clone(),
                    );
                let generator = GeneratorValue::with_result_element_type(
                    generator_callable,
                    iter,
                    result_element_type,
                );
                self.stack.push(Value::Generator(Box::new(generator)));
                Ok(DispatchAction::Continue)
            }

            Instr::MakeGeneratorRuntimeFiltered(result_element_type) => {
                // Issue #9271: filtered generator whose lifted body/predicate are
                // runtime callables. The compiler pushed `predicate`, then `map`,
                // then `iter`, so pop in reverse.
                let iter = self.stack.pop_value()?;
                let map = self.stack.pop_value()?;
                let predicate = self.stack.pop_value()?;
                // Materialize a pure-Julia-iterable base (Dict / …) before wrapping
                // so every consumer can drive it (mirrors MakeGeneratorRuntime).
                let iter = self.materialize_generator_base_if_needed(iter)?;
                let generator = GeneratorValue::with_result_element_type(
                    GeneratorCallable::FilteredRuntimeValue {
                        map: Box::new(map),
                        predicate: Box::new(predicate),
                    },
                    iter,
                    result_element_type.clone(),
                );
                self.stack.push(Value::Generator(Box::new(generator)));
                Ok(DispatchAction::Continue)
            }

            Instr::WrapInGenerator => {
                // Pop an array and wrap it in a Generator for eager-evaluated generator expressions
                let arr = self.stack.pop_value()?;
                let generator = GeneratorValue::eager(arr);
                self.stack.push(Value::Generator(Box::new(generator)));
                Ok(DispatchAction::Continue)
            }

            Instr::SprintFunc(func_index, arg_count) => {
                // sprint(f, args...) - Call f(io, args...) and return result as string
                // Pop args from stack (in reverse order since they were pushed left-to-right)
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    let arg = self.stack.pop_value()?;
                    args.push(arg);
                }
                args.reverse(); // Restore original order

                // Create an IOBuffer reference for interior mutability
                let io = super::super::value::IOValue::buffer_ref();

                // Start the sprint call: call f(io, args...)
                self.start_sprint_call(*func_index, io, args)?;
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
