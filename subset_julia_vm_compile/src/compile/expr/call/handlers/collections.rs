//! Collection-mutation special-case handlers extracted from `compile_call`
//! (Issue #6332): `push!`, `pushfirst!`, `pop!`, `popfirst!`, `insert!`,
//! `deleteat!`, `delete!`, `empty!`, `merge!`, `get!`.
//!
//! Mutating collection functions: push!, pop!, pushfirst!,
//! popfirst!, insert!, deleteat!, delete! (Issues #3739/#3911)
//! The lowering shortcut (`map_builtin_name`) was removed so public calls
//! can dispatch to Pure Julia methods on `Set`/`Dict{K,V}` first. For Array
//! (the most common collection), the legacy `Value::Dict` Rust HashMap,
//! and `Any`-typed locals (e.g., inside Pure Julia function bodies where
//! parameter types are erased to `Any`), we must route directly to
//! `BuiltinOp::Push`/`Pop`/`DictDelete` here to preserve in-place
//! semantics and to avoid runtime dispatch picking the first
//! candidate method (which is `push!(::Set, x)` and would fail with
//! "expected Set" for Array values). Dispatch to a Pure Julia method
//! is left to the regular `method_tables.get(...)` path in `compile_call`
//! for `Set` (compile-time-typed) and `Struct` (e.g., `Dict{K,V}`).
//!
//! Each handler preserves the original match-arm bodies and their relative
//! order for a given function name. `None` = fall through to the generic
//! method-dispatch path (identical to the original arms ending without
//! `return`).

use crate::builtins::BuiltinId;
use crate::bytecode::{DynamicCallCandidate, Instr, ValueType};
use crate::compile::{base_function_to_builtin_op, err, CResult, CoreCompiler};
use crate::ir::core::{BuiltinOp, Expr};
use crate::types::JuliaType;

use super::super::{is_dict_like_julia_type, is_pairs_view_arg_type, is_set_like_julia_type};
use super::{ctry, CallCtx};

fn collection_mutation_runtime_candidates(
    c: &mut CoreCompiler<'_>,
    names: &[&str],
    args: &[Expr],
) -> Vec<usize> {
    let Some(first) = args.first() else {
        return Vec::new();
    };
    let first_value_type = c.infer_expr_type(first);
    if !matches!(first_value_type, ValueType::Any | ValueType::Dict)
        && is_dict_like_julia_type(&c.infer_julia_type(first))
    {
        return Vec::new();
    }

    if matches!(first_value_type, ValueType::Any) {
        c.runtime_candidates_for_names(names, args.len())
    } else {
        c.user_runtime_candidates_for_names(names, args.len())
    }
}

fn is_dict_view_call(c: &mut CoreCompiler<'_>, expr: &Expr) -> bool {
    let Expr::Call { function, args, .. } = expr else {
        return false;
    };
    matches!(
        function.as_str(),
        "keys" | "Base.keys" | "values" | "Base.values"
    ) && args.len() == 1
        && is_dict_like_julia_type(&c.infer_julia_type(&args[0]))
}

/// `pop!` / `Base.pop!` — three original arms in order:
/// the 2|3-arg dict form, the 1-arg form, and the unguarded catch-all
/// (which only matched the unqualified `"pop!"` name).
pub(super) fn compile_pop(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue #6721), so
    // `pop!(s::Set, ...)` must reach the Base/user method (which delegates to the
    // backing Dict) rather than the native `Pop`/`DictPop` builtins, which reject
    // a `StructRef` Set. Route through typed dispatch over the full `pop!`
    // candidates whenever the receiver is set-like.
    if function == "pop!" && matches!(args.len(), 1..=3) && !args.is_empty() {
        let receiver_is_set = matches!(c.infer_expr_type(&args[0]), ValueType::Set)
            || is_set_like_julia_type(&c.infer_julia_type(&args[0]));
        if receiver_is_set {
            let candidates = c.runtime_candidates_for_names(&["pop!", "Base.pop!"], args.len());
            if !candidates.is_empty() {
                for arg in args {
                    ctry!(c.compile_expr(arg));
                }
                c.emit(Instr::CallTypedDispatchOrBuiltinResult(
                    BuiltinId::Pop,
                    "pop!".to_string(),
                    args.len(),
                    candidates,
                ));
                return Some(Ok(ValueType::Any));
            }
        }
    }
    if matches!(args.len(), 2 | 3) {
        let candidates = collection_mutation_runtime_candidates(c, &["pop!", "Base.pop!"], args);
        if !candidates.is_empty() {
            ctry!(c.compile_expr(&args[0]));
            ctry!(c.compile_expr(&args[1]));
            if args.len() == 3 {
                ctry!(c.compile_expr(&args[2]));
            }
            if let Expr::Var(name, _) = &args[0] {
                c.emit(Instr::CallTypedDispatchOrBuiltinStoreDictResult(Box::new(
                    crate::bytecode::TypedDispatchStoreDict {
                        builtin: BuiltinId::DictPop,
                        function_name: "pop!".to_string(),
                        arg_count: args.len(),
                        candidates,
                        store_local: name.to_string(),
                    },
                )));
            } else {
                return Some(err("pop! first argument must be a variable for dict"));
            }
            return Some(Ok(ValueType::Any));
        }
        if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
            return Some(c.compile_builtin(&op, args));
        }
        // Fall through to method dispatch for non-Dict/non-Set Struct.
        return None;
    }
    if args.len() == 1 {
        let candidates = collection_mutation_runtime_candidates(c, &["pop!", "Base.pop!"], args);
        if !candidates.is_empty() {
            ctry!(c.compile_expr(&args[0]));
            c.emit(Instr::CallTypedDispatchOrBuiltinResult(
                BuiltinId::Pop,
                "pop!".to_string(),
                args.len(),
                candidates,
            ));
            return Some(Ok(ValueType::Any));
        }
        if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
            return Some(c.compile_builtin(&op, args));
        }
        // Fall through to method dispatch for Struct receivers.
        return None;
    }
    if matches!(function, "pop!" | "popfirst!") {
        if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
            return Some(c.compile_builtin(&op, args));
        }
        // Fall through to method dispatch for Set / Struct.
        return None;
    }
    None
}

/// `popfirst!` / `Base.popfirst!` — the 1-arg arm, then the unguarded
/// catch-all (which only matched the unqualified `"popfirst!"` name).
pub(super) fn compile_popfirst(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() == 1 {
        let candidates =
            c.user_runtime_candidates_for_names(&["popfirst!", "Base.popfirst!"], args.len());
        if !candidates.is_empty() {
            ctry!(c.compile_expr(&args[0]));
            c.emit(Instr::CallTypedDispatchOrBuiltinResult(
                BuiltinId::PopFirst,
                "popfirst!".to_string(),
                args.len(),
                candidates,
            ));
            return Some(Ok(ValueType::Any));
        }
        if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
            return Some(c.compile_builtin(&op, args));
        }
        // Fall through to method dispatch for Struct receivers.
        return None;
    }
    if matches!(function, "pop!" | "popfirst!") {
        if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
            return Some(c.compile_builtin(&op, args));
        }
        // Fall through to method dispatch for Set / Struct.
        return None;
    }
    None
}

/// `push!` / `Base.push!` with exactly 2 arguments.
pub(super) fn compile_push(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    // `Set` is now a pure-Julia struct over `Dict{T,Nothing}` (Issue #6721), so
    // `push!(s::Set, x)` must resolve to the Base/user `push!` method (which
    // delegates to `s.dict[x] = nothing`) rather than the native `SetAdd`
    // builtin. Route through typed dispatch over the `push!` candidates, keeping
    // `BuiltinId::Push` only as the legacy native-`Value::Set` / cache fallback.
    if matches!(c.infer_expr_type(&args[0]), ValueType::Set) {
        let candidates = c.runtime_candidates_for_names(&["push!", "Base.push!"], args.len());
        if candidates.is_empty() {
            return Some(c.compile_builtin(&BuiltinOp::Push, args));
        }
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::Push,
            "push!".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    let candidates = c.user_runtime_candidates_for_names(&["push!", "Base.push!"], args.len());
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::Push,
            "push!".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
        return Some(c.compile_builtin(&op, args));
    }
    // Fall through to method dispatch for Set / Struct.
    None
}

/// `pushfirst!` / `Base.pushfirst!` with exactly 2 arguments.
pub(super) fn compile_pushfirst(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let candidates =
        c.user_runtime_candidates_for_names(&["pushfirst!", "Base.pushfirst!"], args.len());
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::PushFirst,
            "pushfirst!".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
        return Some(c.compile_builtin(&op, args));
    }
    // Fall through to method dispatch for Struct receivers.
    None
}

/// `insert!` / `Base.insert!` with exactly 3 arguments.
pub(super) fn compile_insert(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 3 {
        return None;
    }
    let candidates = c.user_runtime_candidates_for_names(&["insert!", "Base.insert!"], args.len());
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::Insert,
            "insert!".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
        return Some(c.compile_builtin(&op, args));
    }
    // Fall through to method dispatch for Struct receivers.
    None
}

/// `deleteat!` / `Base.deleteat!` with exactly 2 arguments.
pub(super) fn compile_deleteat(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let candidates =
        c.user_runtime_candidates_for_names(&["deleteat!", "Base.deleteat!"], args.len());
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::DeleteAt,
            "deleteat!".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
        return Some(c.compile_builtin(&op, args));
    }
    // Fall through to method dispatch for Struct receivers.
    None
}

/// `delete!` / `Base.delete!` with exactly 2 arguments.
pub(super) fn compile_delete(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue #6721), so
    // `delete!(s::Set, x)` must reach the Base/user method (which delegates to
    // `delete!(s.dict, x)`) rather than the native `DictDelete` builtin, which
    // rejects a `StructRef` Set. Route through typed dispatch over the full
    // `delete!` candidates, keeping `DictDelete` only as the legacy fallback.
    if matches!(c.infer_expr_type(&args[0]), ValueType::Set) {
        let candidates = c.runtime_candidates_for_names(&["delete!", "Base.delete!"], args.len());
        if candidates.is_empty() {
            return Some(c.compile_builtin(&BuiltinOp::DictDelete, args));
        }
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::CallTypedDispatchOrBuiltinStoreDict(Box::new(
                crate::bytecode::TypedDispatchStoreDict {
                    builtin: BuiltinId::DictDelete,
                    function_name: "delete!".to_string(),
                    arg_count: args.len(),
                    candidates,
                    store_local: name.to_string(),
                },
            )));
        } else {
            c.emit(Instr::CallTypedDispatchOrBuiltin(
                BuiltinId::DictDelete,
                "delete!".to_string(),
                args.len(),
                candidates,
            ));
        }
        return Some(Ok(ValueType::Any));
    }
    let candidates = collection_mutation_runtime_candidates(c, &["delete!", "Base.delete!"], args);
    if !candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::CallTypedDispatchOrBuiltinStoreDict(Box::new(
                crate::bytecode::TypedDispatchStoreDict {
                    builtin: BuiltinId::DictDelete,
                    function_name: "delete!".to_string(),
                    arg_count: args.len(),
                    candidates,
                    store_local: name.to_string(),
                },
            )));
        } else {
            c.emit(Instr::CallTypedDispatchOrBuiltin(
                BuiltinId::DictDelete,
                "delete!".to_string(),
                args.len(),
                candidates,
            ));
        }
        return Some(Ok(ValueType::Any));
    }
    if let Some(op) = c.dispatch_first_collection_mutation_fallback(function, args) {
        return Some(c.compile_builtin(&op, args));
    }
    // Fall through to method dispatch for non-Dict/non-Set Struct.
    None
}

/// `empty!` / `Base.empty!`.
///
/// Dict mutating functions: empty!, merge!, get! - use builtin when argument
/// is Dict. These have Julia methods for Array, but Dict needs the Rust
/// builtin. Issue #2134: Must use LoadDict/StoreDict for in-place semantics.
pub(super) fn compile_empty(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 1 {
        return Some(err(
            "empty! requires exactly 1 argument: empty!(collection)",
        ));
    }
    // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue #6721), so
    // `empty!(s::Set)` must reach the Base/user method (which delegates to
    // `empty!(s.dict)`) rather than the native `DictEmpty` builtin, which rejects
    // a `StructRef` Set. Route through typed dispatch over the full `empty!`
    // candidates, keeping `DictEmpty` only as the legacy fallback.
    if matches!(c.infer_expr_type(&args[0]), ValueType::Set) {
        let candidates = c.runtime_candidates_for_names(&["empty!", "Base.empty!"], args.len());
        if candidates.is_empty() {
            return Some(c.compile_builtin(&BuiltinOp::DictEmpty, args));
        }
        ctry!(c.compile_expr(&args[0]));
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::CallTypedDispatchOrBuiltinStoreDict(Box::new(
                crate::bytecode::TypedDispatchStoreDict {
                    builtin: BuiltinId::DictEmpty,
                    function_name: "empty!".to_string(),
                    arg_count: args.len(),
                    candidates,
                    store_local: name.to_string(),
                },
            )));
        } else {
            c.emit(Instr::CallTypedDispatchOrBuiltin(
                BuiltinId::DictEmpty,
                "empty!".to_string(),
                args.len(),
                candidates,
            ));
        }
        return Some(Ok(ValueType::Any));
    }
    let arg_value_type = c.infer_expr_type(&args[0]);
    let arg_julia_type = c.infer_julia_type(&args[0]);
    if matches!(&arg_julia_type, JuliaType::Struct(name) if name.split('{').next() == Some("Dict"))
    {
        return None;
    }
    if !matches!(arg_value_type, ValueType::Dict) && is_dict_like_julia_type(&arg_julia_type) {
        return None;
    }
    let candidates = collection_mutation_runtime_candidates(c, &["empty!", "Base.empty!"], args);
    if !candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::CallTypedDispatchOrBuiltinStoreDict(Box::new(
                crate::bytecode::TypedDispatchStoreDict {
                    builtin: BuiltinId::DictEmpty,
                    function_name: "empty!".to_string(),
                    arg_count: args.len(),
                    candidates,
                    store_local: name.to_string(),
                },
            )));
        } else {
            c.emit(Instr::CallTypedDispatchOrBuiltin(
                BuiltinId::DictEmpty,
                "empty!".to_string(),
                args.len(),
                candidates,
            ));
        }
        return Some(Ok(ValueType::Any));
    }
    // Infer argument type to decide dispatch
    let arg_type = c.infer_julia_type(&args[0]);
    if matches!(arg_type, JuliaType::Dict) {
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::LoadDict(name.to_string()));
            c.emit(Instr::CallBuiltin(BuiltinId::DictEmpty, 1));
            c.emit(Instr::StoreDict(name.to_string()));
            c.emit(Instr::LoadDict(name.to_string()));
        } else {
            ctry!(c.compile_expr(&args[0]));
            c.emit(Instr::CallBuiltin(BuiltinId::DictEmpty, 1));
        }
        return Some(Ok(ValueType::Dict));
    }
    // Fall through to method dispatch for Array and other types
    None
}

/// `merge!` / `Base.merge!`.
pub(super) fn compile_merge_bang(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 2 {
        return Some(err("merge! requires exactly 2 arguments: merge!(d1, d2)"));
    }
    let candidates = collection_mutation_runtime_candidates(c, &["merge!", "Base.merge!"], args);
    if !candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::CallTypedDispatchOrBuiltinStoreDict(Box::new(
                crate::bytecode::TypedDispatchStoreDict {
                    builtin: BuiltinId::DictMergeBang,
                    function_name: "merge!".to_string(),
                    arg_count: args.len(),
                    candidates,
                    store_local: name.to_string(),
                },
            )));
        } else {
            c.emit(Instr::CallTypedDispatchOrBuiltin(
                BuiltinId::DictMergeBang,
                "merge!".to_string(),
                args.len(),
                candidates,
            ));
        }
        return Some(Ok(ValueType::Any));
    }
    // Infer argument type to decide dispatch
    let arg_type = c.infer_julia_type(&args[0]);
    if matches!(arg_type, JuliaType::Dict) {
        if let Expr::Var(name, _) = &args[0] {
            c.emit(Instr::LoadDict(name.to_string()));
            ctry!(c.compile_expr(&args[1]));
            c.emit(Instr::CallBuiltin(BuiltinId::DictMergeBang, 2));
            c.emit(Instr::StoreDict(name.to_string()));
            c.emit(Instr::LoadDict(name.to_string()));
        } else {
            ctry!(c.compile_expr(&args[0]));
            ctry!(c.compile_expr(&args[1]));
            c.emit(Instr::CallBuiltin(BuiltinId::DictMergeBang, 2));
        }
        return Some(Ok(ValueType::Dict));
    }
    // Fall through to method dispatch for other types
    None
}

/// `get!` / `Base.get!`.
pub(super) fn compile_get_bang(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 3 {
        return Some(err(
            "get! requires exactly 3 arguments: get!(dict, key, default)",
        ));
    }
    let first_julia_type = c.infer_julia_type(&args[0]);
    let second_julia_type = c.infer_julia_type(&args[1]);
    let first_value_type = c.infer_expr_type(&args[0]);
    let second_value_type = c.infer_expr_type(&args[1]);
    let first_is_dict_like =
        matches!(first_value_type, ValueType::Dict) || is_dict_like_julia_type(&first_julia_type);
    let second_is_dict_like =
        matches!(second_value_type, ValueType::Dict) || is_dict_like_julia_type(&second_julia_type);

    // Thunk form: get!(f, dict, key) (also produced by `get!(dict, key) do ... end`).
    // Upstream (julia/base/abstractdict.jl): the dict-first 3-arg form delegates to
    // `get!(() -> default, t, key)`, and the callable is invoked only when the key is
    // absent. We detect the callable-first form (a lambda/do-block lifts to a
    // FunctionRef, a `::Function`-typed variable, or a bare name that resolves to a
    // known function; the structural signal `args[1]` being a Dict also disambiguates)
    // and rewrite it lazily as
    //   haskey(dict, key) ? dict[key] : get!(dict, key, f())
    // so `f()` runs at most once and the insertion persists via the dict-first path
    // below (which now writes the mutated dict back to its bound variable, Issue #5225).
    let first_is_callable = matches!(&args[0], Expr::FunctionRef { .. })
        || c.infer_expr_type(&args[0]) == ValueType::Function
        || matches!(&args[0], Expr::Var(name, _)
            if !c.locals.contains_key(name.as_str())
                && c.method_tables.contains_key(name.as_str()));
    let is_thunk_first = first_is_callable || (second_is_dict_like && !first_is_dict_like);
    if is_thunk_first {
        let span = args[0].span();
        // f() — call the thunk; FunctionRef / Var both dispatch via the callable name.
        let thunk_call = match &args[0] {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => Expr::Call {
                function: name.to_string().into(),
                args: Vec::new(),
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            },
            // Other callable expressions (rare) — evaluate then apply with zero args.
            other => Expr::Call {
                function: "#__sjulia_apply_thunk__".to_string().into(),
                args: vec![other.clone()],
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            },
        };
        let dict = args[1].clone();
        let key = args[2].clone();
        let rewritten = Expr::Ternary {
            condition: Box::new(Expr::Call {
                function: "haskey".to_string().into(),
                args: vec![dict.clone(), key.clone()],
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            }),
            then_expr: Box::new(Expr::Index {
                array: Box::new(dict.clone()),
                indices: vec![key.clone()],
                span,
            }),
            else_expr: Box::new(Expr::Call {
                function: "get!".to_string().into(),
                args: vec![dict, key, thunk_call],
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            }),
            span,
        };
        return Some(c.compile_expr(&rewritten));
    }

    // Dict-first form: get!(dict, key, default). Struct-backed Dicts should stay on
    // ordinary Pure Julia dispatch (Issue #6621); the pure method mutates the
    // StructRef-backed table in place and returns the value. The older
    // DictGetBang builtin result-shaping path is retained only for unknown/non-dict
    // receivers that may still need its fallback semantics.
    if first_is_dict_like {
        let candidates = c
            .runtime_candidates_for_names(&["get!", "Base.get!"], args.len())
            .into_iter()
            .map(DynamicCallCandidate::Method)
            .collect();
        ctry!(c.compile_expr(&args[0]));
        ctry!(c.compile_expr(&args[1]));
        ctry!(c.compile_expr(&args[2]));
        c.emit_dynamic_call("get!", usize::MAX, args.len(), candidates);
        return Some(Ok(ValueType::Any));
    }

    let candidates = collection_mutation_runtime_candidates(c, &["get!", "Base.get!"], args);
    ctry!(c.compile_expr(&args[0]));
    ctry!(c.compile_expr(&args[1]));
    ctry!(c.compile_expr(&args[2]));
    if let Expr::Var(name, _) = &args[0] {
        c.emit(Instr::CallTypedDispatchOrBuiltinStoreDictResult(Box::new(
            crate::bytecode::TypedDispatchStoreDict {
                builtin: BuiltinId::DictGetBang,
                function_name: "get!".to_string(),
                arg_count: args.len(),
                candidates,
                store_local: name.to_string(),
            },
        )));
    } else {
        // Non-variable receiver cannot be written back. The Result variant runs the
        // builtin and keeps only the value, discarding the extra dict that
        // DictGetBang leaves on the stack.
        c.emit(Instr::CallTypedDispatchOrBuiltinResult(
            BuiltinId::DictGetBang,
            "get!".to_string(),
            args.len(),
            candidates,
        ));
    }
    Some(Ok(ValueType::Any))
}

/// Dict view functions: user extensions such as
/// `keys(::Dict{String,Int64})` must win before the retained
/// Rust-backed Dict view fallback. Struct-backed `Dict{K,V}` instances route
/// through the ordinary Pure Julia methods; legacy `Value::Dict` can still use
/// the retained builtin fallback.
pub(super) fn compile_keys_values_pairs(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 1 {
        return None;
    }
    let (builtin_id, dispatch_name, names): (BuiltinId, &str, &[&str]) = match function {
        "keys" | "Base.keys" => (BuiltinId::DictKeys, "keys", &["keys", "Base.keys"]),
        "values" | "Base.values" => (BuiltinId::DictValues, "values", &["values", "Base.values"]),
        "pairs" | "Base.pairs" => (BuiltinId::DictPairs, "pairs", &["pairs", "Base.pairs"]),
        _ => unreachable!("guarded by handler registration"),
    };
    let arg_value_type = c.infer_expr_type(&args[0]);
    let arg_julia_type = c.infer_julia_type(&args[0]);
    if !matches!(arg_value_type, ValueType::Dict) && is_dict_like_julia_type(&arg_julia_type) {
        return None;
    }
    if !matches!(function, "pairs" | "Base.pairs")
        && !is_dict_like_julia_type(&arg_julia_type)
        && is_pairs_view_arg_type(&arg_value_type, &arg_julia_type)
    {
        ctry!(c.compile_expr(&args[0]));
        c.emit(Instr::CallBuiltin(builtin_id, args.len()));
        return Some(Ok(match function {
            "keys" | "Base.keys" if matches!(arg_value_type, ValueType::NamedTuple) => {
                ValueType::Tuple
            }
            "keys" | "Base.keys" => ValueType::Range,
            "values" | "Base.values" if matches!(arg_value_type, ValueType::NamedTuple) => {
                ValueType::Tuple
            }
            _ => arg_value_type.clone(),
        }));
    }

    let candidates = c.user_runtime_candidates_for_names(names, args.len());
    if !candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            builtin_id,
            dispatch_name.to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    if matches!(function, "pairs" | "Base.pairs")
        && !matches!(arg_value_type, ValueType::Dict)
        && !is_dict_like_julia_type(&arg_julia_type)
        && is_pairs_view_arg_type(&arg_value_type, &arg_julia_type)
    {
        ctry!(c.compile_expr(&args[0]));
        c.emit(Instr::PushFunction("pairs".to_string()));
        c.emit(Instr::CallFunctionVariable(1));
        return Some(Ok(c
            .shared_ctx
            .get_struct_type_id("Pairs")
            .map(ValueType::Struct)
            .unwrap_or(ValueType::Any)));
    }
    None
}

/// `haskey` / `Base.haskey` (2-arg form).
pub(super) fn compile_haskey(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let receiver_type = c.infer_expr_type(&args[0]);
    if matches!(receiver_type, ValueType::Dict | ValueType::Any) {
        let candidates =
            c.user_runtime_candidates_for_names(&["haskey", "Base.haskey"], args.len());
        // A user `haskey` override may return a non-Bool value (Issue #6610).
        // When the receiver is only statically known to be `Any`, the emitted
        // typed dispatch can route to such an override at runtime, so the
        // result type must defer to `Any` instead of the builtin's `Bool` —
        // pinning `Bool` here emitted a typed `ReturnI64` that crashed on the
        // user's non-Bool value. A concrete `Dict` keeps the `Bool` fast path
        // (mirrors `compile_keytype_valtype`).
        let defer_to_any = matches!(receiver_type, ValueType::Any) && !candidates.is_empty();
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::DictHasKey,
            "haskey".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(if defer_to_any {
            ValueType::Any
        } else {
            ValueType::Bool
        }));
    }
    None
}

/// Dict type query functions: user extensions such as
/// `keytype(::Dict{String,Float64})` must win before the retained
/// Rust-backed Dict type-parameter fallback. Return type stays `Any`
/// when user candidates exist because Julia permits arbitrary
/// method bodies even though the builtin fallback returns DataType.
pub(super) fn compile_keytype_valtype(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 1 {
        return None;
    }
    let (builtin_id, dispatch_name, names): (BuiltinId, &str, &[&str]) = match function {
        "keytype" | "Base.keytype" => (BuiltinId::Keytype, "keytype", &["keytype", "Base.keytype"]),
        "valtype" | "Base.valtype" => (BuiltinId::Valtype, "valtype", &["valtype", "Base.valtype"]),
        _ => unreachable!("guarded by handler registration"),
    };
    let candidates = c.user_runtime_candidates_for_names(names, args.len());
    if !candidates.is_empty() {
        ctry!(c.compile_expr(&args[0]));
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            builtin_id,
            dispatch_name.to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    let arg_value_type = c.infer_expr_type(&args[0]);
    let arg_julia_type = c.infer_julia_type(&args[0]);
    let native_collection_arg = matches!(
        arg_value_type,
        ValueType::Array
            | ValueType::ArrayOf(_, _)
            | ValueType::Memory
            | ValueType::MemoryOf(_)
            | ValueType::Tuple
            | ValueType::Any
    ) || matches!(
        arg_julia_type,
        JuliaType::Array
            | JuliaType::VectorOf(_)
            | JuliaType::MatrixOf(_)
            | JuliaType::Tuple
            | JuliaType::TupleOf(_)
    ) || matches!(&arg_julia_type, JuliaType::Struct(name) if {
        let base_name = name.split('{').next().unwrap_or(name.as_str());
        matches!(base_name, "Array" | "Vector" | "Matrix" | "Memory" | "Tuple")
    });
    if native_collection_arg && !is_dict_like_julia_type(&arg_julia_type) {
        // Issue #5606: keep native collection type queries on the
        // builtin path. The runtime fallback added on main handles
        // Array wrappers, while this guard prevents compile-time
        // typed dispatch from selecting AbstractDict methods for
        // non-dict storage.
        return Some(c.compile_builtin(
            &match builtin_id {
                BuiltinId::Keytype => BuiltinOp::Keytype,
                BuiltinId::Valtype => BuiltinOp::Valtype,
                _ => unreachable!("guarded by handler registration"),
            },
            args,
        ));
    }
    None
}

/// `in` / `Base.in` (2-arg form).
pub(super) fn compile_in(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let collection_value_type = c.infer_expr_type(&args[1]);
    let collection_julia_type = c.infer_julia_type(&args[1]);
    let struct_collection = matches!(collection_value_type, ValueType::Struct(type_id)
        if c.shared_ctx.is_struct_type_of(&ValueType::Struct(type_id), "KeySet")
            || c.shared_ctx.is_struct_type_of(&ValueType::Struct(type_id), "ValueIterator"));
    // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue #6721), so
    // `x in s::Set` must reach the Base/user `in(x, s::Set) = haskey(s.dict, x)`
    // method, not the native `In` builtin (which only handles legacy
    // `Value::Set`/Array/Dict and rejects a `StructRef` Set).
    if is_dict_like_julia_type(&collection_julia_type)
        || struct_collection
        || is_set_like_julia_type(&collection_julia_type)
        || is_dict_view_call(c, &args[1])
    {
        return None;
    }
    let candidates = if matches!(collection_value_type, ValueType::Any) {
        c.runtime_candidates_for_names(&["in", "Base.in"], args.len())
    } else {
        c.user_runtime_candidates_for_names(&["in", "Base.in"], args.len())
    };
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::In,
            "in".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    if let Some(builtin_op) = base_function_to_builtin_op(function) {
        return Some(c.compile_builtin(&builtin_op, args));
    }
    Some(c.compile_builtin_call(function, args))
}

/// `∈` / `Base.∈` (2-arg form).
pub(super) fn compile_elem_of(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let collection_value_type = c.infer_expr_type(&args[1]);
    let collection_julia_type = c.infer_julia_type(&args[1]);
    let struct_collection = matches!(collection_value_type, ValueType::Struct(type_id)
        if c.shared_ctx.is_struct_type_of(&ValueType::Struct(type_id), "KeySet")
            || c.shared_ctx.is_struct_type_of(&ValueType::Struct(type_id), "ValueIterator"));
    if is_dict_like_julia_type(&collection_julia_type)
        || struct_collection
        || is_set_like_julia_type(&collection_julia_type)
        || is_dict_view_call(c, &args[1])
    {
        return None;
    }
    let candidates = if matches!(collection_value_type, ValueType::Any) {
        c.runtime_candidates_for_names(&["∈", "Base.∈", "in", "Base.in"], args.len())
    } else {
        c.user_runtime_candidates_for_names(&["∈", "Base.∈", "in", "Base.in"], args.len())
    };
    if !candidates.is_empty() {
        for arg in args {
            ctry!(c.compile_expr(arg));
        }
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::In,
            "∈".to_string(),
            args.len(),
            candidates,
        ));
        return Some(Ok(ValueType::Any));
    }
    // Fall through to the Pure Julia ∈ wrapper, which delegates to in.
    None
}

/// `∉` / `∋` / `∌` (+`Base.`-qualified forms), 2-arg.
pub(super) fn compile_membership_aliases(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let function = ctx.function;
    let args = ctx.args;
    if args.len() != 2 {
        return None;
    }
    let names: &[&str] = match function {
        "∉" | "Base.∉" => &["∉", "Base.∉"],
        "∋" | "Base.∋" => &["∋", "Base.∋"],
        "∌" | "Base.∌" => &["∌", "Base.∌"],
        _ => unreachable!("guarded by handler registration"),
    };
    if !c
        .specific_runtime_candidates_for_names(names, args.len())
        .is_empty()
    {
        if let Some((fallback_index, candidates)) =
            c.runtime_candidates_with_generic_fallback_for_names(names, args.len())
        {
            for arg in args {
                ctry!(c.compile_expr(arg));
            }
            c.emit(Instr::CallTypedDispatch(
                names[0].to_string(),
                args.len(),
                fallback_index,
                candidates,
            ));
            return Some(Ok(ValueType::Any));
        }
    }
    // Issue #3911: these Base aliases are wrappers around `in`.
    // Compiled Base wrappers can otherwise retain an early builtin
    // `in` fallback and miss later user `Base.in` methods.
    let (needle, collection, negate) = match function {
        "∉" | "Base.∉" => (&args[0], &args[1], true),
        "∋" | "Base.∋" => (&args[1], &args[0], false),
        "∌" | "Base.∌" => (&args[1], &args[0], true),
        _ => unreachable!("guarded by handler registration"),
    };
    // `Set`/`Dict` are pure-Julia structs; their membership must reach the
    // Base `in(x, s::Set)` / `in(k, d::Dict)` methods, so include Base `in`
    // candidates (not just user overrides) when the collection is a struct-backed
    // set/dict and the native `In` builtin would reject the `StructRef` value
    // (Issue #6721).
    let collection_is_struct_set_or_dict = {
        let jt = c.infer_julia_type(collection);
        is_set_like_julia_type(&jt) || is_dict_like_julia_type(&jt)
    };
    ctry!(c.compile_expr(needle));
    ctry!(c.compile_expr(collection));
    let candidates = if collection_is_struct_set_or_dict {
        c.runtime_candidates_for_names(&["in", "Base.in"], args.len())
    } else {
        c.user_runtime_candidates_for_names(&["in", "Base.in"], args.len())
    };
    if candidates.is_empty() {
        c.emit(Instr::CallBuiltin(BuiltinId::In, args.len()));
    } else {
        c.emit(Instr::CallTypedDispatchOrBuiltin(
            BuiltinId::In,
            "in".to_string(),
            args.len(),
            candidates,
        ));
    }
    if negate {
        c.emit(Instr::NotBool);
    }
    Some(Ok(ValueType::Bool))
}

/// Public `Dict(...)` construction is an ordinary Julia method call. The legacy
/// `NewDict*` instructions remain decodable for old bytecode, but new public
/// construction routes through `Dict`/`Dict{K,V}` methods (Issue #6619).
pub(super) fn compile_dict_constructor_call(
    _c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let _ = ctx;
    None
}

/// Public `Set(...)` construction is an ordinary Julia method call (Issue
/// #6721). `Set{T}` is now a pure-Julia struct over `Dict{T,Nothing}`
/// (base/set.jl), so `Set()`, `Set(itr)`, `Set([...])`, `Set{T}(...)`, and the
/// `Set(x for x in itr)` comprehension/generator form all resolve through the
/// `Set`/`Set{T}` constructor methods (the comprehension materializes the
/// generator and flows into `Set(itr)`), exactly like the `Dict` migration
/// (Issue #6619). Returning `None` falls through to method dispatch.
pub(super) fn compile_set_constructor_call(
    _c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let _ = ctx;
    None
}
