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
use crate::ir::core::{Expr, Stmt};
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

fn extract_static_hof_callable(expr: &Expr) -> &Expr {
    if let Expr::LetBlock { body, .. } = expr {
        if let Some(Stmt::Expr { expr, .. }) = body.stmts.last() {
            if matches!(expr, Expr::FunctionRef { .. } | Expr::Var(_, _)) {
                return expr;
            }
        }
    }
    expr
}

fn callable_ref_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::FunctionRef { name, .. } | Expr::Var(name, _) => Some(name),
        _ => None,
    }
}

impl CoreCompiler<'_> {
    fn ntuple_needs_runtime_callable(&self, expr: &Expr) -> bool {
        if matches!(expr, Expr::LetBlock { .. }) {
            return true;
        }

        let callable = extract_static_hof_callable(expr);
        let Some(name) = callable_ref_name(callable) else {
            return true;
        };

        if self
            .shared_ctx
            .closure_captures
            .get(name)
            .is_some_and(|captures| !captures.is_empty())
        {
            return true;
        }

        matches!(callable, Expr::Var(_, _)) && self.locals.contains_key(name)
    }

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
            // findall / findfirst / findlast (all forms) are now Pure Julia
            // (Issue #3728). The HOF predicate forms `findall(f, arr)`,
            // `findfirst(f, arr)`, and `findlast(f, arr)` resolve to
            // base/{array,reduce}.jl methods through normal dispatch.
            // The VM instructions `FindAllFunc`, `FindFirstFunc`,
            // `FindLastFunc` remain in place for cache compatibility but
            // are no longer reachable from new IR.
            "findall" | "findfirst" | "findlast" => Ok(None),
            // map! and filter! are now Pure Julia in base/array.jl (Issue #3731).
            // - filter!(f, a::Array)
            // - map!(f, a::Array)
            // - map!(f, dest::Array, src::Array)
            // Returning Ok(None) lets the call fall through to method dispatch
            // so the Pure Julia methods take priority over the Rust HOF builtins
            // (`MapFuncInPlace`, `FilterFuncInPlace`). The VM instructions are
            // retained as cache-compatibility fallbacks but no longer reachable
            // from new IR.
            "map!" | "filter!" => Ok(None),
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
            // mapreduce / mapfoldl / mapfoldr are now Pure Julia in
            // base/iterators.jl (Issue #3731):
            //   - mapfoldl(f::Function, op::Function, itr [, init])
            //   - mapfoldr(f::Function, op::Function, itr [, init])
            //   - mapreduce(f::Function, op::Function, itr [, init])
            // Falling through to method dispatch lets user-extensible Julia
            // methods take priority over the Rust HOF instructions
            // (`MapReduceFunc*`, `MapFoldrFunc*`). The VM instructions remain
            // for cache-compatibility but are no longer reachable from new IR.
            "mapreduce" | "mapfoldl" | "mapfoldr" => Ok(None),
            // broadcast/broadcast! are now Pure Julia (base/broadcast.jl, Issue #2548, #2549).
            // Fall through to method table dispatch.
            "broadcast" | "broadcast!" => Ok(None),
            "foreach" => {
                // foreach is now Pure Julia in base/abstractarray.jl
                // Fall through to method table dispatch
                Ok(None)
            }
            // sum / any / all / count predicate-HOF forms are now Pure
            // Julia (Issue #3728). They resolve to `base/reduce.jl` methods
            // (`any(f::Function, arr)`, `all(f::Function, arr)`,
            // `count(f::Function, ::Array)`, `sum(f::Function, ::Array)`).
            // The VM instructions `SumFunc`, `AnyFunc`, `AllFunc`,
            // `CountFunc` are retained as cache-compatibility fallbacks but
            // no longer emitted from new IR.
            "sum" | "any" | "all" | "count" if args.len() == 2 => Ok(None),
            "ntuple" => {
                // ntuple(f, n) - Create tuple by calling f(i) for i in 1:n
                if args.len() != 2 {
                    return err("ntuple requires exactly 2 arguments: ntuple(f, n)");
                }
                if self.ntuple_needs_runtime_callable(&args[0]) {
                    self.compile_expr(&args[0])?; // Compile runtime callable
                    self.compile_expr(&args[1])?; // Compile n (integer)
                    self.emit(Instr::NtupleRuntime);
                    return Ok(Some(ValueType::Tuple));
                }
                let func_index =
                    self.resolve_function_ref(extract_static_hof_callable(&args[0]))?;
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
                Ok(Some(ValueType::Function))
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
                    let func_index = self.resolve_sprint_function_ref(&args[0], &args[1..])?;

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
