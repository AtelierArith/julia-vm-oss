//! Higher-order function compilation.
//!
//! Handles compilation of Julia higher-order functions:
//! - map(f, arr): Apply function to each element
//! - map!(f, dest, src): Apply function in-place
//! - filter(f, arr): Filter elements by predicate
//! - filter!(f, arr): Filter elements in-place
//! - reduce(f, arr [, init]): Reduce array with function
//! - mapfoldl(f, op, arr [, init]): Map then left fold
//! - mapfoldr(f, op, arr [, init]): Map then right fold
//! - sum(f, arr): Apply function and sum results
//! - any(f, arr): Check if predicate holds for any element
//! - all(f, arr): Check if predicate holds for all elements
//! - count(f, arr): Count elements where predicate is true
//! - ntuple(f, n): Create tuple by calling f(i) for i in 1:n
//!
//! Note: foreach(f, arr) is now Pure Julia in base/abstractarray.jl

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
    /// Compile higher-order function calls.
    /// Returns Some(type) if handled, None if not a HOF.
    pub(in super::super) fn compile_builtin_hof(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            // map and filter are now implemented in Pure Julia (iteration.jl)
            // using Generator and Filter structs with the iterate protocol.
            // This allows dynamic function dispatch via struct field function calls.
            "map" | "filter" => {
                // Fall through to Pure Julia implementation
                Ok(None)
            }
            "findall" => {
                // findall(f, arr) - return Int64 indices where predicate returns true
                // findall(A) - single argument form is handled by Pure Julia (base/array.jl)
                if args.len() == 1 {
                    // Single argument: fallback to Pure Julia findall(A::Array{Bool})
                    return Ok(None);
                }
                if args.len() != 2 {
                    return err("findall requires 1 or 2 arguments: findall(A) or findall(f, arr)");
                }
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::FindAllFunc(func_index));
                // findall always returns Vector{Int64}
                use crate::vm::value::ArrayElementType;
                Ok(Some(ValueType::ArrayOf(ArrayElementType::I64)))
            }
            "findfirst" if args.len() == 2 => {
                // findfirst(f, arr) - return first 1-based index where predicate returns true, or nothing
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::FindFirstFunc(func_index));
                Ok(Some(ValueType::Any)) // Returns I64 or Nothing
            }
            "findlast" if args.len() == 2 => {
                // findlast(f, arr) - return last 1-based index where predicate returns true, or nothing
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::FindLastFunc(func_index));
                Ok(Some(ValueType::Any)) // Returns I64 or Nothing
            }
            "map!" => {
                // map!(f, dest, src) - apply f to each element of src and store in dest
                if args.len() != 3 {
                    return err("map! requires exactly 3 arguments: map!(f, dest, src)");
                }
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile dest array
                self.compile_expr(&args[2])?; // Compile src array
                self.emit(Instr::MapFuncInPlace(func_index));
                Ok(Some(ValueType::Array))
            }
            "filter!" => {
                // filter!(f, arr) - filter elements in-place, removing non-matching elements
                if args.len() != 2 {
                    return err("filter! requires exactly 2 arguments: filter!(f, arr)");
                }
                let func_index = self.resolve_function_ref(&args[0])?;
                let input_type = self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::FilterFuncInPlace(func_index));
                // filter! preserves input element type
                match input_type {
                    ValueType::ArrayOf(elem) => Ok(Some(ValueType::ArrayOf(elem))),
                    _ => Ok(Some(ValueType::Array)),
                }
            }
            // reduce and foldl are now implemented in Pure Julia (iteration.jl)
            "reduce" | "foldl" => {
                // Fall through to Pure Julia implementation
                Ok(None)
            }
            // foldr is now implemented in Pure Julia (iteration.jl)
            "foldr" => {
                // Fall through to Pure Julia implementation
                Ok(None)
            }
            "mapreduce" | "mapfoldl" => {
                // mapreduce(f, op, arr) or mapreduce(f, op, arr, init)
                // mapfoldl is an alias for mapreduce (both are left-associative)
                if args.len() < 3 || args.len() > 4 {
                    return err(format!(
                        "{} requires 3 or 4 arguments: {}(f, op, arr) or {}(f, op, arr, init)",
                        name, name, name
                    ));
                }
                let map_func_index = self.resolve_function_ref(&args[0])?;
                // Reduce operator needs binary (2-arg) resolution (Issue #2004)
                let reduce_func_index = self.resolve_function_ref_with_arity(&args[1], 2)?;
                self.compile_expr(&args[2])?; // Compile array
                if args.len() == 4 {
                    self.compile_expr(&args[3])?; // Compile init value
                    self.emit(Instr::MapReduceFuncWithInit(
                        map_func_index,
                        reduce_func_index,
                    ));
                } else {
                    self.emit(Instr::MapReduceFunc(map_func_index, reduce_func_index));
                }
                // Infer return type from reduce function's return type
                if let Some(return_type) = self.get_function_return_type(&args[1]) {
                    Ok(Some(return_type))
                } else {
                    Ok(Some(ValueType::F64))
                }
            }
            "mapfoldr" => {
                // mapfoldr(f, op, arr) or mapfoldr(f, op, arr, init) - right-associative
                if args.len() < 3 || args.len() > 4 {
                    return err("mapfoldr requires 3 or 4 arguments: mapfoldr(f, op, arr) or mapfoldr(f, op, arr, init)");
                }
                let map_func_index = self.resolve_function_ref(&args[0])?;
                // Reduce operator needs binary (2-arg) resolution (Issue #2004)
                let reduce_func_index = self.resolve_function_ref_with_arity(&args[1], 2)?;
                self.compile_expr(&args[2])?; // Compile array
                if args.len() == 4 {
                    self.compile_expr(&args[3])?; // Compile init value
                    self.emit(Instr::MapFoldrFuncWithInit(
                        map_func_index,
                        reduce_func_index,
                    ));
                } else {
                    self.emit(Instr::MapFoldrFunc(map_func_index, reduce_func_index));
                }
                // Infer return type from reduce function's return type
                if let Some(return_type) = self.get_function_return_type(&args[1]) {
                    Ok(Some(return_type))
                } else {
                    Ok(Some(ValueType::F64))
                }
            }
            // broadcast/broadcast! are now Pure Julia (base/broadcast.jl, Issue #2548, #2549).
            // Fall through to method table dispatch.
            "broadcast" | "broadcast!" => Ok(None),
            "foreach" => {
                // foreach is now Pure Julia in base/abstractarray.jl
                // Fall through to method table dispatch
                Ok(None)
            }
            "sum" if args.len() == 2 => {
                // sum(f, arr) - Apply f to each element and sum the results
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::SumFunc(func_index));
                Ok(Some(ValueType::F64))
            }
            "any" if args.len() == 2 => {
                // any(f, arr) - Check if f returns true for any element
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::AnyFunc(func_index));
                Ok(Some(ValueType::Bool))
            }
            "all" if args.len() == 2 => {
                // all(f, arr) - Check if f returns true for all elements
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::AllFunc(func_index));
                Ok(Some(ValueType::Bool))
            }
            "count" if args.len() == 2 => {
                // count(f, arr) - Count elements where f returns true
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile array
                self.emit(Instr::CountFunc(func_index));
                Ok(Some(ValueType::I64))
            }
            "ntuple" => {
                // ntuple(f, n) - Create tuple by calling f(i) for i in 1:n
                if args.len() != 2 {
                    return err("ntuple requires exactly 2 arguments: ntuple(f, n)");
                }
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?; // Compile n (integer)
                self.emit(Instr::NtupleFunc(func_index));
                Ok(Some(ValueType::Tuple))
            }
            "compose" => {
                // compose(f, g) - Create composed function f ∘ g
                if args.len() != 2 {
                    return err("compose requires exactly 2 arguments: compose(f, g)");
                }
                self.compile_expr(&args[0])?; // Compile outer function
                self.compile_expr(&args[1])?; // Compile inner function
                self.emit(Instr::CallBuiltin(BuiltinId::Compose, 2));
                Ok(Some(ValueType::Any))
            }
            "sprint" => {
                // sprint(f, args...) - Call f(io, args...) and return the result as a string
                // sprint(x) - Convert x to string (when x is not a function)
                if args.is_empty() {
                    return err(
                        "sprint requires at least 1 argument: sprint(f, args...) or sprint(x)",
                    );
                }

                // Check if the first argument is a function reference
                let is_func_ref = match &args[0] {
                    Expr::FunctionRef { .. } => true,
                    Expr::Var(name, _) => self.method_tables.contains_key(name),
                    _ => false,
                };

                if is_func_ref {
                    // sprint(f) or sprint(f, args...) - call f(io, args...)
                    // Use type-directed dispatch to select the correct overload (Issue #3120).
                    // sprint calls f(io, args...) so arity = 1 + extra_args.len().
                    // resolve_sprint_function_ref infers arg types and uses MethodTable::dispatch.
                    let arg_count = args.len() - 1; // Number of additional args (0 for sprint(f))
                    let func_index =
                        self.resolve_sprint_function_ref(&args[0], &args[1..])?;

                    // Compile all remaining arguments onto the stack
                    for arg in args.iter().skip(1) {
                        self.compile_expr(arg)?;
                    }

                    // Emit the sprint instruction
                    self.emit(Instr::SprintFunc(func_index, arg_count));
                    Ok(Some(ValueType::Str))
                } else {
                    // sprint(x) -> convert x to string using write
                    // This is equivalent to take!(write(IOBuffer(), x))
                    if args.len() != 1 {
                        return err(
                            "sprint(x) requires exactly 1 argument when x is not a function",
                        );
                    }
                    self.emit(Instr::CallBuiltin(BuiltinId::IOBufferNew, 0));
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::IOWrite, 2));
                    self.emit(Instr::CallBuiltin(BuiltinId::TakeString, 1));
                    Ok(Some(ValueType::Str))
                }
            }
            _ => Ok(None),
        }
    }
}
