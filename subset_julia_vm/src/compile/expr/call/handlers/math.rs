//! Math / linear-algebra special-case handlers extracted from
//! `compile_call` (Issue #6332): `inv`, `\` (pre-match table), and the
//! post-struct-resolution `floor`/`ceil`/`round`/`trunc` digits-kwarg
//! forms plus the `sqrt` builtin route.
//!
//! Each handler preserves the original branch bodies and their relative
//! order for a given function name. `None` = fall through to the generic
//! method-dispatch path (identical to the original branches ending without
//! `return`).

use crate::builtins::BuiltinId;
use crate::compile::{err, CResult, CoreCompiler, MethodSig};
use crate::inference_core::CoreType;
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::{ctry, CallCtx};

/// Matrix inverse: type-dispatched (Array → faer builtin, Rational → Pure Julia).
/// We check argument type at compile time to route to the correct implementation.
pub(super) fn compile_inv(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 1 {
        return Some(err("inv requires exactly 1 argument: inv(A)"));
    }
    let arg_types: Vec<JuliaType> = args.iter().map(|arg| c.infer_julia_type(arg)).collect();
    if c.has_user_dispatch_method_for_arg_types(&["inv", "LinearAlgebra.inv"], &arg_types) {
        // User imports/extensions of LinearAlgebra.inv must win before
        // the retained VM kernel, matching upstream Julia dispatch.
    } else {
        // Infer argument type to decide dispatch
        let arg_type = arg_types[0].clone();
        // If the argument is an Array-like type (Array, VectorOf, MatrixOf),
        // use the faer-based builtin for matrix inverse
        let is_array_type = matches!(
            arg_type,
            JuliaType::Array
                | JuliaType::VectorOf(_)
                | JuliaType::MatrixOf(_)
                | JuliaType::AbstractArray
        );
        if is_array_type {
            ctry!(c.compile_expr(&args[0]));
            c.emit(Instr::CallBuiltin(BuiltinId::Inv, 1));
            return Some(Ok(ValueType::Array));
        }
    }
    // Otherwise (Rational, or unknown type), fall through to method dispatch
    // which will find inv(::Rational{T}) in Pure Julia
    None
}

/// Left division (backslash): A \ b solves Ax = b for x.
/// Uses faer LU solve for matrix/vector operations.
pub(super) fn compile_left_division(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 2 {
        return Some(err("\\ requires exactly 2 arguments: A \\ b"));
    }

    let override_candidates = c.user_override_candidates_for_names(&["\\", "Base.\\"], args.len());
    if !override_candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::Ldiv,
            "\\".to_string(),
            args.len(),
            override_candidates,
        ));
        return Some(Ok(ValueType::Any));
    }

    // Infer first argument type to decide dispatch
    let arg_type = c.infer_julia_type(&args[0]);
    // If the first argument is an Array-like type, use the faer-based builtin
    let is_array_type = matches!(
        arg_type,
        JuliaType::Array
            | JuliaType::VectorOf(_)
            | JuliaType::MatrixOf(_)
            | JuliaType::AbstractArray
    );
    if is_array_type {
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        c.emit(Instr::CallBuiltin(BuiltinId::Ldiv, 2));
        return Some(Ok(ValueType::Array));
    }

    let candidates = c.runtime_candidates_for_names(&["\\", "Base.\\"], args.len());
    if !candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::Ldiv,
            "\\".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }

    // For scalars, \ is just division (a \ b = b / a)
    // Fall through to method dispatch or handle as scalar division
    ctry!(c.compile_expr(&args[1]));
    ctry!(c.compile_expr(&args[0]));
    c.emit(Instr::DivF64);
    Some(Ok(ValueType::F64))
}

// Note: floor/ceil/round/trunc with digits/sigdigits/base keywords are now pure
// Julia (base/floatfuncs.jl, Issue #6742). The former Rust kwargs handlers
// (compile_{floor,ceil,round,trunc}_kwargs → BuiltinId::*Digits/*SigDigits) were
// removed so the keyword calls dispatch to the pure-Julia keyword methods.

/// `sqrt` has a primitive Float64 builtin path and Pure Julia methods for
/// struct values such as Complex. Route through the builtin wrapper before
/// generic method-table dispatch so real-valued `sqrt` in function bodies
/// returns Float64, matching Julia, while struct arguments still dispatch.
pub(super) fn compile_sqrt(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() == 1 && ctx.kwargs.is_empty() {
        let value_ty = c.infer_expr_type(&ctx.args[0]);
        if matches!(value_ty, ValueType::Struct(_)) {
            let arg_ty = c.value_type_to_julia_type(&value_ty);
            let hazards = recursion_hazard_sqrt_global_indices(c);
            let resolved = c.method_tables.iter().find_map(|(name, table)| {
                if !is_sqrt_method_name(name) {
                    return None;
                }
                let method = table.dispatch(std::slice::from_ref(&arg_ty)).ok()?;
                if hazards.contains(&method.global_index) {
                    return None;
                }
                c.shared_ctx
                    .function_ir_by_global_index
                    .contains_key(&method.global_index)
                    .then(|| (method.global_index, method.return_type.clone()))
            });
            if let Some((global_index, return_type)) = resolved {
                if let Err(err) = c.compile_expr(&ctx.args[0]) {
                    return Some(Err(err));
                }
                c.emit(Instr::Call(global_index, 1));
                return Some(Ok(return_type));
            }
        }
        if matches!(value_ty, ValueType::Any) {
            // Issue #8042: when a *foreign* module defines its own `sqrt`
            // generic function (e.g. NaNMath's `sqrt(x) = ... Base.sqrt(float(x))`),
            // sjulia merges that `sqrt(::Any)` catch-all into the global `sqrt`
            // table. Generic dispatch on an `Any`-typed `Float64` would then pick
            // the foreign method, whose `Base.sqrt(float(x))` body re-resolves
            // back to itself → unbounded recursion / stack overflow. Route such
            // calls through the builtin-backed dispatch instruction with a
            // candidate list that excludes the foreign methods, so a primitive
            // falls back to the builtin and terminates while genuine `Base.sqrt`
            // extensions in the candidate list still dispatch.
            //
            // Emit the builtin-backed dispatch when there is a genuine `sqrt`
            // method to try (the candidate list carries every non-hazard
            // extension, e.g. `Complex`/`Num`, so a struct value dispatches and
            // a primitive falls back to the builtin) OR when a recursion hazard
            // exists (a foreign catch-all that generic dispatch would otherwise
            // pick for a primitive and recurse through — Issue #8042). With
            // neither, fall through to generic dispatch (`return None` below),
            // matching the pre-#8042 behaviour for plain values.
            let candidates = sqrt_runtime_candidates(c);
            if !candidates.is_empty() || !recursion_hazard_sqrt_global_indices(c).is_empty() {
                if let Err(err) = c.compile_expr(&ctx.args[0]) {
                    return Some(Err(err));
                }
                c.emit(Instr::CallTypedDispatchOrBuiltin(
                    BuiltinId::Sqrt,
                    "sqrt".to_string(),
                    1,
                    candidates,
                ));
                return Some(Ok(ValueType::Any));
            }
        }
        let arg_ty = c.infer_julia_type(&ctx.args[0]);
        // Issue #7702: abstract/unknown values can hold structs with Base
        // extension methods, so let generic dispatch try those before the builtin.
        if matches!(value_ty, ValueType::Any | ValueType::Struct(_))
            || !matches!(
                arg_ty,
                JuliaType::Int8
                    | JuliaType::Int16
                    | JuliaType::Int32
                    | JuliaType::Int64
                    | JuliaType::Int128
                    | JuliaType::BigInt
                    | JuliaType::UInt8
                    | JuliaType::UInt16
                    | JuliaType::UInt32
                    | JuliaType::UInt64
                    | JuliaType::UInt128
                    | JuliaType::Bool
                    | JuliaType::Float16
                    | JuliaType::Float32
                    | JuliaType::Float64
                    | JuliaType::BigFloat
            )
        {
            return None;
        }
        return Some(c.compile_builtin_call(ctx.function, ctx.args));
    }
    None
}

/// Method-table names that may hold a method of the `sqrt` generic function:
/// the bare alias `sqrt`, the explicit `Base.sqrt` key, and module-qualified
/// keys such as `"Symbolics.sqrt"` — sjulia keys a `Base.sqrt(x::Num)`
/// extension *and* a foreign module's brand-new `sqrt(x)` generic identically
/// as `"<Module>.sqrt"`, so the name alone cannot tell a genuine extension from
/// a recursion hazard (Issue #8042).
///
/// That discrimination is therefore done by *signature*, by global index, in
/// [`recursion_hazard_sqrt_global_indices`] — never by name. Narrowing this
/// predicate to only `"sqrt"`/`"Base.sqrt"` would silently drop genuine struct
/// extensions like `Symbolics`' `Base.sqrt(x::Num)` and route `sqrt(::Num)` to
/// the builtin, which rejects the struct (the original #8042 fix's regression).
fn is_sqrt_method_name(name: &str) -> bool {
    name == "sqrt" || name.ends_with(".sqrt")
}

/// True when a primitive scalar (e.g. a `Float64`) could select this `sqrt`
/// method — i.e. its first parameter is `Any`, a numeric primitive, or an
/// abstract numeric supertype rather than a concrete struct / named user type.
///
/// A method whose first parameter is a struct (`Complex`, Symbolics `Num`, …)
/// can never be chosen for a bare `Float64`, so it is safe; a catch-all
/// `sqrt(x)` / `sqrt(x::Real)` *can*, which is what makes a foreign module's
/// merged `sqrt` a recursion hazard (Issue #8042). When the canonical signature
/// projection is unavailable we conservatively assume it admits a primitive so
/// an opaque catch-all cannot slip past the filter.
fn sqrt_method_admits_primitive(method: &MethodSig) -> bool {
    match method.expanded_core_param_types_for_arity(1).as_deref() {
        Some([first, ..]) => core_type_admits_primitive_scalar(first),
        Some([]) | None => true,
    }
}

/// Whether a bare numeric scalar value (`Float64`, `Int`, …) can inhabit `ty`.
/// Concrete structs / named user types cannot hold a primitive scalar; `Any`,
/// primitives, abstract numerics and unconstrained type variables can.
fn core_type_admits_primitive_scalar(ty: &CoreType) -> bool {
    match ty {
        CoreType::Any
        | CoreType::Bottom
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::TypeVar(_) => true,
        CoreType::Union(items) => items.iter().any(core_type_admits_primitive_scalar),
        CoreType::UnionAll { body, .. } => core_type_admits_primitive_scalar(body),
        // Concrete structs, named user types, modules, tuples, etc.: a bare
        // scalar value is never an instance of these.
        _ => false,
    }
}

/// Global indices of `sqrt` methods that are a *recursion hazard* when applied
/// to an `Any`-typed primitive `Float64`.
///
/// A module-local definition such as NaNMath's `sqrt(x) = ... Base.sqrt(float(x))`
/// is a brand-new generic function, yet sjulia merges it into the global bare
/// `sqrt` method table (keyed `"<Module>.sqrt"` — exactly as a genuine
/// `Base.sqrt(x::Num)` extension is, so the table *name* cannot tell them
/// apart). If such a catch-all captures a primitive `Float64`, its
/// `Base.sqrt(float(x))` body re-resolves back to itself → stack overflow
/// (Issue #8042). The recursion bites both at the outer call site and *inside*
/// the foreign method's own body.
///
/// The discriminator is therefore the *signature*, not the name: a method is a
/// hazard iff it is **not** a genuine `Base.sqrt` extension
/// ([`MethodSig::is_base_extension`]) **and** its parameter admits a primitive
/// scalar ([`sqrt_method_admits_primitive`]). Genuine struct extensions
/// (`Complex`, `Num`) and explicit `Base.sqrt` extensions are kept, so they
/// still dispatch for struct values while primitives fall back to the builtin.
fn recursion_hazard_sqrt_global_indices(c: &CoreCompiler<'_>) -> std::collections::HashSet<usize> {
    let mut hazards = std::collections::HashSet::new();
    for (name, table) in c.method_tables {
        if !is_sqrt_method_name(name) {
            continue;
        }
        for method in table.methods.iter() {
            if !method.is_base_extension && sqrt_method_admits_primitive(method) {
                hazards.insert(method.global_index);
            }
        }
    }
    hazards
}

fn sqrt_runtime_candidates(c: &CoreCompiler<'_>) -> Vec<usize> {
    let hazards = recursion_hazard_sqrt_global_indices(c);
    let mut candidates = Vec::new();
    for (name, table) in c.method_tables {
        if !is_sqrt_method_name(name) {
            continue;
        }
        for method in table.methods.iter() {
            if hazards.contains(&method.global_index)
                || !method.accepts_arity(1)
                || !c
                    .shared_ctx
                    .function_ir_by_global_index
                    .contains_key(&method.global_index)
                || candidates.contains(&method.global_index)
            {
                continue;
            }
            candidates.push(method.global_index);
        }
    }
    candidates
}
