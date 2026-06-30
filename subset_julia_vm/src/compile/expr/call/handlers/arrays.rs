//! Array / storage special-case handlers extracted from `compile_call`
//! (Issue #6332): `getindex`, `setindex!`, `reshape`, `similar`,
//! `collect_similar`, `collect`.
//!
//! Each handler preserves the original match-arm bodies and their relative
//! order for a given function name. `None` = fall through to the generic
//! method-dispatch path (identical to the original arms ending without
//! `return`).

use crate::builtins::BuiltinId;
use crate::compile::{is_builtin_type_name, CResult, CoreCompiler};
use crate::ir::core::{BuiltinOp, Expr};
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::super::is_array_like_julia_type;
use super::{ctry, CallCtx};

/// Julia-compliant indexing: getindex / setindex!.
///
/// Issue #3729: do not unconditionally route public `getindex` /
/// `setindex!` calls to the builtin path — that shadows Pure Julia
/// methods on Pair, Range types (LinRange/StepRangeLen/OneTo/LogRange),
/// Broadcasted, SubArray/MatrixView, CartesianIndex, LinearIndices,
/// and any user-defined struct method.
///
/// Strategy: if the receiver (first arg) inferred type is a Struct
/// and the method table has a candidate for that struct, fall
/// through to the regular method-dispatch path below. Otherwise
/// (primitive Array/Tuple/Dict/String/Range/Memory etc.) keep the
/// fast builtin route via `compile_builtin_call`, which generates
/// typed `IndexLoad` / `IndexSlice` / Dict / String / Range index
/// instructions.
pub(super) fn compile_getindex_setindex(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.is_empty() {
        // Empty-args edge case (should never happen for these names) —
        // preserve the historical builtin route.
        return Some(c.compile_builtin_call(function, args));
    }
    if function == "getindex" && c.typed_array_literal_element_type(&args[0]).is_some() {
        return Some(c.compile_builtin_call(function, args));
    }
    let arg_types: Vec<JuliaType> = args.iter().map(|a| c.infer_julia_type(a)).collect();
    if function == "getindex"
        && matches!(
            arg_types.first(),
            Some(
                JuliaType::Array
                    | JuliaType::VectorOf(_)
                    | JuliaType::MatrixOf(_)
                    | JuliaType::AbstractArray
            )
        )
        && args[1..].iter().any(|idx| {
            matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. })
                || is_array_like_julia_type(&c.infer_julia_type(idx))
                || matches!(
                    c.infer_expr_type(idx),
                    ValueType::Array
                        | ValueType::ArrayOf(_, _)
                        | ValueType::Bool
                        | ValueType::Range
                        | ValueType::Rng
                )
        })
    {
        return Some(c.compile_builtin_call(function, args));
    }
    let names: &[&str] = match function {
        "getindex" => &["getindex", "Base.getindex"],
        "setindex!" => &["setindex!", "Base.setindex!"],
        _ => unreachable!("guarded by handler registration"),
    };
    if c.has_user_dispatch_method_for_arg_types(names, &arg_types) {
        // Fall through to method dispatch below so Base extensions from
        // user code can override primitive array indexing, matching Julia.
    } else {
        // Issue #6657: an `Any`-typed receiver cannot be matched against a
        // concrete user `getindex` override at compile time, so the check
        // above is false even when the runtime value would dispatch to a user
        // method. Route it through a runtime dispatch with a native-indexing
        // fallback before falling back to the builtin fast path.
        if function == "getindex" {
            if let Some(result) = c.try_compile_dynamic_getindex_dispatch(args) {
                return Some(result);
            }
        }
        let first_value_type = c.infer_expr_type(&args[0]);
        if !matches!(first_value_type, ValueType::Struct(_)) {
            return Some(c.compile_builtin_call(function, args));
        }

        let first_type = c.infer_julia_type(&args[0]);
        let is_struct_receiver = matches!(first_type, JuliaType::Struct(_));
        let has_struct_method = is_struct_receiver
            && c.method_tables
                .get(function)
                .map(|table| table.methods.iter().any(|m| m.param_count() == args.len()))
                .unwrap_or(false);
        if !has_struct_method {
            return Some(c.compile_builtin_call(function, args));
        }
    }
    // Else: fall through to method dispatch below.
    None
}

/// reshape: try user-backed methods before the retained VM fallback.
pub(super) fn compile_reshape(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.is_empty() {
        return None;
    }
    let first_type = c.infer_expr_type(&args[0]);
    if matches!(first_type, ValueType::Array | ValueType::ArrayOf(_, _)) {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallBuiltin(BuiltinId::Reshape, args.len()));
        return Some(Ok(ValueType::Any));
    }
    let candidates = c.runtime_candidates_for_names(&["reshape", "Base.reshape"], args.len());
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::Reshape,
            "reshape".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    Some(c.compile_builtin(&BuiltinOp::Reshape, args))
}

/// similar: known Array/storage-type receivers fall through to method
/// dispatch so Pure Julia `similar(a::Array{T}, ...)`,
/// `similar(Array{T}, dims)`, the bare `Array` eltype(a) fallback,
/// and user extensions can win before the transitional builtin
/// fallback (Issues #4018/#4643).
/// The Pure Julia broadcast pipeline calls similar(::Broadcasted, ::Type) which
/// we must NOT intercept; that case has Broadcasted as args[0] (a Struct), not
/// an Array, so it never matches the Array/Any first-arg cases here.
pub(super) fn compile_similar(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.is_empty() {
        return None;
    }
    let first_type = c.infer_expr_type(&args[0]);
    // When the first argument's type is unknown (e.g., a function parameter),
    // route to the builtin if the remaining args are dims or a typed
    // (T[, dims...]) form. The runtime distinguishes the two based on whether
    // the second arg is a DataType vs an integer.
    if matches!(
        first_type,
        ValueType::Any | ValueType::Array | ValueType::ArrayOf(_, _) | ValueType::DataType
    ) {
        if args.len() == 1 {
            let candidates = c
                .method_tables
                .get("similar")
                .or_else(|| c.method_tables.get("Base.similar"))
                .map(|table| c.user_unary_runtime_candidates(table))
                .unwrap_or_default();
            if !candidates.is_empty() {
                ctry!(c.compile_expr(&args[0]));
                c.emit(Instr::CallDynamicOrBuiltin(BuiltinId::Similar, candidates));
                return Some(Ok(ValueType::Array));
            }
            return Some(c.compile_builtin_call(function, args));
        }
        let second_type = c.infer_expr_type(&args[1]);
        let is_int = |t: &ValueType| {
            matches!(
                t,
                ValueType::I64
                    | ValueType::I8
                    | ValueType::I16
                    | ValueType::I32
                    | ValueType::I128
                    | ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128
            )
        };
        // Detect a Type-valued argument. `infer_expr_type` returns DataType
        // only for known type-producing calls; a bare `Int`/`Float64` etc.
        // is parsed as `Expr::Var` and falls back to `Any`. Recognise those
        // by name as typed-form markers (Issue #3751).
        let arg_is_type = |a: &Expr, vt: &ValueType| -> bool {
            if matches!(vt, ValueType::DataType) {
                return true;
            }
            if let Expr::Var(name, _) = a {
                return is_builtin_type_name(name);
            }
            false
        };
        // Accept Any-typed dim args for the shape form so that callers
        // like `similar(arr, length(arr) * n)` or `similar(arr, total)`
        // (where `total` is computed as `I64 * Any-param`) route to the
        // builtin instead of falling through to method dispatch (Issue
        // #3777). The runtime handler in `vm/builtins_arrays.rs::Similar`
        // already validates each dim by `pop_value` + integer check, so
        // an Any that resolves to a non-integer at runtime gets a clean
        // error there. A `DataType`-typed second arg still wins the
        // typed-form route below, since `arg_is_type` returns false for
        // bare `Any`.
        let is_int_or_any = |t: &ValueType| is_int(t) || matches!(t, ValueType::Any);
        let is_tuple_dims = |a: &Expr, t: &ValueType| {
            matches!(a, Expr::TupleLiteral { .. }) || matches!(t, ValueType::Tuple)
        };
        if arg_is_type(&args[1], &second_type) {
            // similar(arr, T[, dims...]) — typed form (Issue #3751).
            // Subsequent dim args may be I64 or Any (Issue #3777).
            let rest_are_dims = args[2..]
                .iter()
                .all(|a| is_int_or_any(&c.infer_expr_type(a)));
            if rest_are_dims {
                let candidates =
                    c.user_runtime_candidates_for_names(&["similar", "Base.similar"], args.len());
                if !candidates.is_empty() {
                    for arg in args {
                        ctry!(c.compile_expr(arg));
                    }
                    c.emit(Instr::CallTypedDispatchOrBuiltin(
                        BuiltinId::Similar,
                        "similar".to_string(),
                        args.len(),
                        candidates,
                    ));
                    return Some(Ok(ValueType::Any));
                }
                return Some(c.compile_builtin_call(function, args));
            }
        } else if args.len() == 2 && is_tuple_dims(&args[1], &second_type) {
            // similar(arr, dims::Tuple) — when `arr` is an untyped
            // parameter, generic compile-time dispatch can otherwise
            // freeze a vararg dims fallback and treat the tuple as an
            // element type. Defer to runtime typed dispatch so the
            // `dims::Tuple` method wins (Issue #4018).
            let candidates = c.runtime_candidates_for_names(&["similar", "Base.similar"], 2);
            if !candidates.is_empty() {
                for arg in args {
                    ctry!(c.compile_expr(arg));
                }
                c.emit(Instr::CallTypedDispatchOrBuiltin(
                    BuiltinId::Similar,
                    "similar".to_string(),
                    2,
                    candidates,
                ));
                return Some(Ok(ValueType::Any));
            }
            return Some(c.compile_builtin_call(function, args));
        } else if is_int_or_any(&second_type) {
            // similar(arr, n[, m, ...]) — multi-dim shape (Issue #3751,
            // Any-dim relaxation Issue #3777).
            let rest_are_dims = args[1..]
                .iter()
                .all(|a| is_int_or_any(&c.infer_expr_type(a)));
            if rest_are_dims {
                let candidates =
                    c.user_runtime_candidates_for_names(&["similar", "Base.similar"], args.len());
                if !candidates.is_empty() {
                    for arg in args {
                        ctry!(c.compile_expr(arg));
                    }
                    c.emit(Instr::CallTypedDispatchOrBuiltin(
                        BuiltinId::Similar,
                        "similar".to_string(),
                        args.len(),
                        candidates,
                    ));
                    return Some(Ok(ValueType::Any));
                }
                return Some(c.compile_builtin_call(function, args));
            }
        }
    }
    None
}

/// `collect_similar` / `Base.collect_similar` (2-arg form).
pub(super) fn compile_collect_similar(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let container_is_memory = matches!(
        c.infer_expr_type(&args[0]),
        ValueType::Memory | ValueType::MemoryOf(_)
    ) || matches!(&args[0], Expr::Call { function, .. }
        if function == "Memory"
            || function == "Base.Memory"
            || function.starts_with("Memory{")
            || function.starts_with("Base.Memory{"));
    let is_generator_call = matches!(&args[1], Expr::Call {
        function,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        ..
    } if (function == "Generator" || function == "Base.Generator")
        && kwargs.is_empty()
        && !splat_mask.iter().any(|&is_splat| is_splat)
        && !kwargs_splat_mask.iter().any(|&is_splat| is_splat));
    if !container_is_memory
        && (is_generator_call || matches!(c.infer_expr_type(&args[1]), ValueType::Generator))
    {
        // CollectFallback: collect-similar-generator-compile-boundary
        let collect_args = vec![args[1].clone()];
        return Some(c.compile_builtin(&BuiltinOp::Collect, &collect_args));
    }
    if matches!(c.infer_expr_type(&args[0]), ValueType::Any)
        || matches!(c.infer_expr_type(&args[1]), ValueType::Any)
    {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::PushFunction("collect_similar".to_string()));
        c.emit(Instr::CallFunctionVariable(args.len()));
        return Some(Ok(ValueType::Any));
    }
    None
}

/// collect: when called on primitive Range/Array/Tuple/String values
/// (which have no exact Pure Julia method match — only the generic
/// `collect(itr)` would dispatch and would lose element-type
/// information), route to BuiltinOp::Collect / BuiltinId::RangeCollect
/// so element-type inference is preserved (Issue #3735). Struct or
/// Any first-arg types fall through to the method-dispatch path; the
/// BuiltinOp::Collect handler itself also tries the method table
/// first for struct iterators.
pub(super) fn compile_collect(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 1 {
        return None;
    }
    // Short-circuit primitive Range values here. Upstream Julia uses
    // `collect(r::AbstractRange) = Array(r)` in `julia/base/range.jl`;
    // sjulia's VM-native `Value::Range` is not a Pure Julia struct, so
    // this is the current representation boundary for the same public
    // behavior. Removing this static Range branch makes `collect(1:5)`
    // fall through to generic `collect(::Any)` and lose the element type
    // (`Vector{Any}`), so keep it until VM-native ranges can participate
    // in the trait-shaped `_collect` path without field-access hazards
    // (Issue #4077).
    //
    // `collect(::Array)` / `collect(::SubArray{...})` /
    // `collect(::LinRange|StepRangeLen|LogRange)` are Pure Julia
    // methods and can be reached via normal method dispatch.
    //
    // Also short-circuit Any-typed first args: the BuiltinOp::Collect
    // handler in compile/expr/builtin.rs does its own struct-aware
    // method-dispatch and falls back to BuiltinId::RangeCollect for
    // primitive Value::Range values.
    let first_type = c.infer_expr_type(&args[0]);
    let is_unqualified_generator_call = matches!(&args[0], Expr::Call {
        function,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        ..
    } if function == "Generator"
        && kwargs.is_empty()
        && !splat_mask.iter().any(|&is_splat| is_splat)
        && !kwargs_splat_mask.iter().any(|&is_splat| is_splat));
    if is_unqualified_generator_call {
        return Some(c.compile_builtin(&BuiltinOp::Collect, args));
    }
    let is_zip_call = matches!(&args[0], Expr::Call {
        function,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        ..
    } if matches!(function.as_str(), "zip" | "Base.zip" | "Base.Iterators.zip")
        && kwargs.is_empty()
        && !splat_mask.iter().any(|&is_splat| is_splat)
        && !kwargs_splat_mask.iter().any(|&is_splat| is_splat));
    let first_julia_type = c.infer_julia_type(&args[0]);
    let is_dispatch_first_collect_struct = matches!(&first_julia_type, JuliaType::Struct(name) if {
        let base_name = name.split('{').next().unwrap_or(name.as_str());
        matches!(
            base_name,
            "Enumerate" | "Rest" | "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6"
            | "Zip7"
        )
    }) || is_zip_call;
    let is_pairs_collect = matches!(first_julia_type, JuliaType::Pairs)
        || matches!(&first_julia_type, JuliaType::Struct(name) if name.split('{').next() == Some("Pairs"));
    if matches!(first_type, ValueType::Pairs) || is_pairs_collect {
        ctry!(c.compile_expr(&args[0]));
        c.emit(Instr::PushFunction("_pairs_collect_dynamic".to_string()));
        c.emit(Instr::CallFunctionVariable(1));
        return Some(Ok(ValueType::Array));
    }
    let is_direct_range_bridge = c.is_direct_range_collect_bridge_expr(&args[0]);
    let has_user_range_collect_method = c.has_user_range_collect_method();
    if matches!(first_type, ValueType::Generator)
        || (matches!(first_type, ValueType::Range)
            && (!is_direct_range_bridge || has_user_range_collect_method))
        || matches!(
            first_julia_type,
            JuliaType::Array
                | JuliaType::VectorOf(_)
                | JuliaType::MatrixOf(_)
                | JuliaType::AbstractArray
        )
        || (matches!(first_type, ValueType::Any)
            && !is_pairs_collect
            && !is_dispatch_first_collect_struct
            && !has_user_range_collect_method)
    {
        // CollectFallback: public-collect-primitive-compile-boundary
        return Some(c.compile_builtin(&BuiltinOp::Collect, args));
    }
    // Otherwise fall through to normal method dispatch.
    None
}

/// Handle Array() and Vector() without type parameters.
pub(super) fn compile_array_vector_constructor(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.function == "Matrix"
        && ctx.args.len() == 1
        && ctx.kwargs.is_empty()
        && !ctx.has_splat
        && !ctx.has_kwargs_splat
    {
        let arg_value_type = c.infer_expr_type(&ctx.args[0]);
        let arg_julia_type = c.infer_julia_type(&ctx.args[0]);
        let already_matrix_like = matches!(arg_julia_type, JuliaType::MatrixOf(_))
            || matches!(arg_value_type, ValueType::Array)
            || matches!(arg_value_type, ValueType::ArrayOf(_, Some(2)));
        if !already_matrix_like {
            return None;
        }
    }
    Some(c.compile_array_constructor(&[], ctx.args, ctx.function))
}
