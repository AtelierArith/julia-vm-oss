//! Miscellaneous core special-case handlers extracted from `compile_call`
//! (Issue #6332): `tuple`, `!`, empty `NamedTuple()`, `ntuple`, `hash`,
//! `convert`, `promote`, `widemul`, `reinterpret`, `deepcopy` (pre-match
//! table), plus the post-struct-resolution cases `mapreduce`/`reduce`
//! init-kwarg rewrites, `eltype`, and `length` on Pairs.
//!
//! Each handler preserves the original branch bodies and their relative
//! order for a given function name. `None` = fall through to the generic
//! method-dispatch path (identical to the original branches ending without
//! `return`).

use crate::builtins::BuiltinId;
use crate::bytecode::{Instr, ValueType};
use crate::compile::{base_function_to_builtin_op, err, CResult, CoreCompiler};
use crate::types::JuliaType;

use super::{ctry, CallCtx};

/// Core.tuple: create a tuple from arguments (Issue #2173).
/// tuple(a, b, c) is equivalent to (a, b, c).
pub(super) fn compile_tuple(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    for arg in ctx.args {
        ctry!(c.compile_expr(arg));
    }
    c.emit(Instr::NewTuple(ctx.args.len()));
    Some(Ok(ValueType::Tuple))
}

/// Handle ! operator as function call (from macro-generated code).
pub(super) fn compile_not(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 1 {
        return Some(err("! operator requires exactly 1 argument"));
    }
    ctry!(c.compile_expr(&ctx.args[0]));
    c.emit(Instr::NotBool);
    Some(Ok(ValueType::Bool))
}

/// `NamedTuple()` constructs the empty named tuple (Issue #5776),
/// mirroring the empty `(;)` literal. Non-empty argument lists fall
/// through to the generic constructor paths.
pub(super) fn compile_empty_named_tuple(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if !ctx.args.is_empty() {
        return None;
    }
    c.emit(Instr::NewNamedTuple(Vec::new()));
    Some(Ok(ValueType::NamedTuple))
}

/// ntuple: the direct 2-arg call shape keeps the constant-propagation
/// fast path in builtin_hof.rs (Instr::NtupleFunc / NtupleRuntime),
/// which also recovers the compile-time length from `ntuple(f, Val(N))`.
/// Issue #4973 added a pure-Julia `ntuple(f, n::Integer)` method so the
/// bare/qualified name is a first-class function value; that method now
/// lives in `method_tables`, so without this explicit route the call
/// would fall through to method dispatch below and raise NoMethodFound
/// for the `Val{N}` length argument. Route direct calls to the builtin
/// fast path; first-class indirect calls (`f = ntuple; f(...)`) use the
/// pure-Julia method via runtime dispatch.
pub(super) fn compile_ntuple(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() == 2 && ctx.kwargs.is_empty() {
        return Some(c.compile_builtin_call(ctx.function, ctx.args));
    }
    None
}

/// `rethrow` is a compiler/VM primitive, not an ordinary Julia method. Base has
/// declaration stubs so the binding exists as a value, but direct calls must
/// compile to `RethrowCurrent`/`RethrowOther` before generic method dispatch.
pub(super) fn compile_rethrow(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    Some(c.compile_builtin_call(ctx.function, ctx.args))
}

/// Resolve `convert`'s first argument to a concrete, codegen-specialized
/// [`ValueType`] when it is a bare type name (`Int`, `Float64`, `Bool`,
/// `MyStruct`, ...). Returns `None` for abstract/parametric/unknown targets,
/// where no static specialization is possible. Used by `compile_convert` to
/// elide no-op conversions and keep result slots typed (Issue #8147).
fn convert_target_concrete_type(
    c: &CoreCompiler<'_>,
    arg: &crate::ir::core::Expr,
) -> Option<ValueType> {
    let crate::ir::core::Expr::Var(name, _) = arg else {
        return None;
    };
    crate::compile::narrowing::value_type_for_type_name(name, |n| c.get_struct_type_id(n))
}

/// convert(T, x) consults Julia methods in the VM before falling
/// back to Rust conversions. Keeping this route avoids static
/// ambiguity when the first argument only infers as DataType.
pub(super) fn compile_convert(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 {
        return Some(err("convert requires exactly 2 arguments: convert(T, x)"));
    }

    // `convert(T, x)` where `T` is a statically-known concrete type drives two
    // optimizations that keep type annotations off the slow path (Issue #8147):
    //
    //  * No-op elision: when `x` is already inferred to be exactly `T`,
    //    `convert(::Type{T}, x::T) = x` is the identity. Compile just the value
    //    and skip both the type push and the `Convert` builtin — this avoids a
    //    method search + dispatch on every hot-path execution (e.g. a tail
    //    `return a` under `f(...)::Int`, or `tmp::Int = b`).
    //  * Typed result: even when a real conversion is needed, report the
    //    concrete target type instead of `Any` so the assigned local / return
    //    slot stays specialized (`LoadSlotI64` etc.) rather than collapsing to
    //    a generic slot that de-optimizes every later use.
    let target_ty = convert_target_concrete_type(c, &ctx.args[0]);

    if let Some(ref target) = target_ty {
        if c.infer_expr_type(&ctx.args[1]) == *target {
            let actual = ctry!(c.compile_expr(&ctx.args[1]));
            return Some(Ok(if actual == *target {
                actual
            } else {
                target.clone()
            }));
        }
    }

    ctry!(c.compile_expr(&ctx.args[0]));
    ctry!(c.compile_expr(&ctx.args[1]));
    c.emit(Instr::CallBuiltin(BuiltinId::Convert, 2));
    Some(Ok(target_ty.unwrap_or(ValueType::Any)))
}

/// promote(x, y, ...) - promote values to common type.
pub(super) fn compile_promote(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.is_empty() {
        return Some(err("promote requires at least 1 argument"));
    }
    // Compile all arguments
    for arg in ctx.args {
        ctry!(c.compile_expr(arg));
    }
    c.emit(Instr::CallBuiltin(BuiltinId::Promote, ctx.args.len()));
    // Returns a tuple of promoted values
    Some(Ok(ValueType::Tuple))
}

// widemul removed - now Pure Julia (base/number.jl: widen(x)*widen(y),
// Issue #6737). The old intercept forced BuiltinId::Widemul, whose I64-only
// handler errored on Int32 etc.; routing through method dispatch widens
// correctly for every integer width.

/// reinterpret(T, x) - bit-level type reinterpretation.
pub(super) fn compile_reinterpret(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 {
        return Some(err("reinterpret requires exactly 2 arguments"));
    }
    ctry!(c.compile_expr(&ctx.args[0]));
    ctry!(c.compile_expr(&ctx.args[1]));
    c.emit(Instr::CallBuiltin(BuiltinId::Reinterpret, 2));
    // Return type depends on first argument
    Some(Ok(ValueType::Any))
}

/// _deepcopy(x) - recursive deep copy boundary for public deepcopy.
pub(super) fn compile_deepcopy(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 1 {
        return Some(err("_deepcopy requires exactly 1 argument"));
    }
    ctry!(c.compile_expr(&ctx.args[0]));
    c.emit(Instr::CallBuiltin(BuiltinId::Deepcopy, 1));
    Some(Ok(ValueType::Any))
}

/// Special case: mapreduce/mapfoldl/mapfoldr with init keyword argument
/// (Issue #2077). mapreduce(f, op, arr; init=val) => mapreduce(f, op, arr, val)
///
/// After Issue #3731 the public mapreduce/mapfoldl/mapfoldr names dispatch
/// to Pure Julia methods in `base/iterators.jl`. Keyword-only Pure Julia
/// wrapper methods would shadow positional 4-arg methods, so we still
/// perform the kwarg→positional rewrite at the compiler level — but then
/// route the rewritten call back through normal method dispatch
/// (`compile_call`) instead of `compile_builtin_call`, which would now
/// return `Ok(None)` and surface "Unknown function".
pub(super) fn compile_mapreduce_init_kwarg(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 3 || ctx.kwargs.is_empty() {
        return None;
    }
    if let Some(init_expr) = ctx.kwargs.iter().find(|(k, _)| k == "init").map(|(_, v)| v) {
        let mut extended_args = ctx.args.to_vec();
        extended_args.push(init_expr.clone());
        return Some(c.compile_call(ctx.function, &extended_args, &[], &[], &[]));
    }
    None
}

/// Special case: reduce/foldl/foldr with init keyword argument
/// (Issue #2077, #2084). reduce(op, itr; init=val) => reduce(op, itr, val)
/// Keyword-only Pure Julia wrapper methods shadow positional 3-arg methods,
/// so we handle keyword→positional conversion at compiler level instead.
pub(super) fn compile_reduce_init_kwarg(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 || ctx.kwargs.is_empty() {
        return None;
    }
    if let Some(init_expr) = ctx.kwargs.iter().find(|(k, _)| k == "init").map(|(_, v)| v) {
        let mut extended_args = ctx.args.to_vec();
        extended_args.push(init_expr.clone());
        return Some(c.compile_call(ctx.function, &extended_args, &[], &[], &[]));
    }
    None
}

/// Julia Base array eltype is a simple type-parameter query.  Keep this
/// boundary as a builtin for array-like values so loop-carried arrays
/// produced by `collect` do not recursively compile the Pure Julia
/// `eltype(a::Array) = eltype(typeof(a))` method body.  Non-array
/// iterators still dispatch to user/Pure Julia `eltype` methods.
pub(super) fn compile_eltype_query(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if !(args.len() == 1 && ctx.kwargs.is_empty()) {
        return None;
    }
    let arg_type = c.infer_julia_type(&args[0]);
    let has_user_eltype_method = c.has_user_dispatch_method_for_arg_types(
        &["eltype", "Base.eltype"],
        std::slice::from_ref(&arg_type),
    );
    if !has_user_eltype_method
        && (CoreCompiler::is_builtin_collection_eltype_arg_type(&arg_type)
            || matches!(arg_type, JuliaType::Pairs)
            || c.expr_is_collection_type_object_for_eltype(&args[0]))
    {
        if let Some(builtin_op) = base_function_to_builtin_op(ctx.function) {
            return Some(c.compile_builtin(&builtin_op, args));
        }
    }
    None
}

/// `length` on a Pairs value routes to the builtin.
pub(super) fn compile_length_pairs(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if !(ctx.args.len() == 1 && ctx.kwargs.is_empty()) {
        return None;
    }
    let arg_type = c.infer_julia_type(&ctx.args[0]);
    if matches!(arg_type, JuliaType::Pairs) {
        if let Some(builtin_op) = base_function_to_builtin_op(ctx.function) {
            return Some(c.compile_builtin(&builtin_op, ctx.args));
        }
    }
    None
}
