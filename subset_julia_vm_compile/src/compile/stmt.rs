//! Statement compilation for CoreCompiler.
//!
//! This module contains statement-level compilation methods including
//! block, function body, and individual statement compilation.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, RuntimeNominalDef, Stmt, UnaryOp};
use crate::types::JuliaType;

mod stmt_try_catch;
use crate::bytecode::value::is_array_wrapper_struct_name;
use crate::bytecode::{ArrayElementType, Instr, ValueType};
use subset_julia_vm_bytecode::{
    julia_enum_member_binding_order, AbstractTypeDefInfo, DefineRuntimeNominalOperands,
    EnumDefInfo, PrimitiveTypeDefInfo, RegisterEnumOperands, RuntimeNominalDefInfo,
    RuntimeStructDefInfo, StructDefInfo,
};

use super::core_compiler::ScopeCleanupContext;
use super::types::{err, internal_compile_error, CResult, CompileError};
use super::{
    analyze_free_variables, static_assignment_types_compatible, CoreCompiler, EnumInfo,
    LoopContext, ShadowedLocal,
};
use std::collections::{HashMap, HashSet};

/// Compiler-side binding facts that belong to the currently visible lexical
/// owners.  Explicit module/main hard scopes use distinct VM environments, so
/// their temporary/binder/body-local facts must disappear together with the
/// runtime owner instead of leaking into the following top-level statement.
#[derive(Clone)]
struct ExplicitScopeBindingMetadata {
    locals: HashMap<String, ValueType>,
    initialized_locals: HashSet<String>,
    julia_type_locals: HashMap<String, JuliaType>,
    known_any_rank_array_locals: HashSet<String>,
    mixed_type_vars: HashSet<String>,
    function_aliases: HashMap<String, String>,
    lexical_function_tables: HashMap<String, String>,
    type_value_aliases: HashMap<String, JuliaType>,
    module_aliases: HashMap<String, String>,
}

/// Evaluate a compile-time-constant integer step for a `for` range loop.
///
/// Returns `Some(k)` when the loop step is statically known to be the Int64 value
/// `k` (Issue #5166). The default (no explicit step) is `1`. Negative literals are
/// represented as a `UnaryOp::Neg` over a positive `Literal::Int` at this stage of
/// the pipeline (the lowering that turns `-1` into `NegInt` happens later, during
/// `compile_expr_as`), so they are matched directly here rather than via const_prop.
///
/// Returns `None` for any non-constant step (e.g. a variable or call), leaving the
/// caller to fall back to the dynamic per-iteration sign-check path.
fn const_int_step(step: &Option<Expr>) -> Option<i64> {
    match step {
        None => Some(1),
        Some(expr) => match expr {
            Expr::Literal(Literal::Int(k), _) => Some(*k),
            Expr::UnaryOp {
                op: crate::ir::core::UnaryOp::Neg,
                operand,
                ..
            } => match operand.as_ref() {
                Expr::Literal(Literal::Int(k), _) => k.checked_neg(),
                _ => None,
            },
            Expr::UnaryOp {
                op: crate::ir::core::UnaryOp::Pos,
                operand,
                ..
            } => match operand.as_ref() {
                Expr::Literal(Literal::Int(k), _) => Some(*k),
                _ => None,
            },
            _ => None,
        },
    }
}

fn is_lifted_generator_helper_name(name: &str) -> bool {
    let leaf = name.rsplit('#').next().unwrap_or(name);
    leaf.starts_with("__gen_body_") || leaf.starts_with("__gen_pred_")
}

/// Stable token for a runtime-conditional nominal declaration. Normal program
/// assembly assigns `definition_order`; direct compiler entry points used by
/// embedders and unit tests may still carry zero, so fold the complete source
/// location into a deterministic FNV-1a token instead of using a vector index.
fn runtime_nominal_site_id(span: crate::span::Span) -> u64 {
    if span.definition_order != 0 {
        return span.definition_order;
    }

    let mut token = 0xcbf2_9ce4_8422_2325_u64;
    for component in [
        span.start,
        span.end,
        span.start_line,
        span.end_line,
        span.start_column,
        span.end_column,
    ] {
        for byte in (component as u64).to_le_bytes() {
            token ^= u64::from(byte);
            token = token.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    token
}

/// Function-signature `where`-bound names that must resolve when the method
/// definition executes (Issue #10396).
///
/// Upstream Julia evaluates a signature's `where`-bounds eagerly at method
/// definition time — `h2(x::T) where T<:UndefZZZ = x` raises `UndefVarError`
/// for `UndefZZZ` before the definition takes effect. sjulia's declaration
/// path stores bounds as plain strings inside `TypeParam` for structural
/// dispatch matching, so an undefined bound was silently accepted. Issue
/// #10226 fixed the same class of gap for VALUE-position `where`
/// (`typevar_bound_value_expr`); this helper is its declaration-position
/// sibling, feeding definition-time `Instr::LoadAny` probes.
///
/// A bound name is skipped (no candidate returned) when it is:
/// - absent (unbounded parameter),
/// - a recognized builtin/static type (`JuliaType::from_name`),
/// - not a single bare identifier (compound bounds like `Vector{Int}`,
///   `Union{A,B}`, or `Base.Number` keep their existing permissive path,
///   matching the #10226 scoping compromise),
/// - the name of any `where`-binder of the same method (sibling references
///   `where {T, S<:T}` and chained `where S<:T where T` are valid upstream;
///   the final `type_params` vector interleaves braced/chained scope order,
///   so the whole binder set is excluded rather than a positional prefix —
///   this deliberately also accepts `where T<:T` with no outer `T`, which
///   upstream rejects, rather than risk rejecting valid programs).
///
/// The caller (`CoreCompiler::emit_where_bound_definition_probes`) further
/// drops names the compiler statically resolves as type objects, then probes
/// the rest at runtime exactly like a variable read: `LoadAny` consults
/// enclosing-frame `where` type-bindings and runtime globals and raises
/// `UndefVarError` otherwise, matching upstream.
///
/// Per parameter the lower bound is probed before the upper bound, mirroring
/// upstream's left-to-right `TypeVar(:T, lb, ub)` argument evaluation
/// (`UndefA<:T<:UndefB` reports `UndefA`).
fn undeclared_where_bound_names(type_params: &[crate::types::TypeParam]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for tp in type_params {
        for bound in [tp.lower_bound.as_ref(), tp.get_upper_bound()]
            .into_iter()
            .flatten()
        {
            if JuliaType::from_name(bound).is_some() {
                continue;
            }
            if !crate::lowering::expr::is_bare_identifier_name(bound) {
                continue;
            }
            if type_params.iter().any(|p| p.name == *bound) {
                continue;
            }
            if names.iter().any(|n| n == bound) {
                continue;
            }
            names.push(bound.clone());
        }
    }
    names
}

/// Function-signature parameter-annotation type names that must resolve when
/// the method definition executes (Issue #10582).
///
/// Upstream Julia evaluates every parameter type annotation eagerly at method
/// definition time — `f(x::SomeUndefName) = 1` raises `UndefVarError` before
/// the definition takes effect (positional, optional, vararg, and keyword
/// parameters alike; verified against julia 1.12). sjulia's signature lowering
/// resolves an unknown annotation name to a nominal `JuliaType::Struct(name)`
/// placeholder for structural dispatch, so an undefined annotation was
/// silently accepted and the method simply unmatchable. This helper is the
/// annotation-position sibling of [`undeclared_where_bound_names`]
/// (Issue #10396) and feeds the same definition-time `Instr::LoadAny` probes.
///
/// An annotation is skipped (no candidate appended) when it is:
/// - absent (untyped parameter),
/// - anything but a nominal `Struct(name)` placeholder (builtin/parametric
///   annotations were already resolved by lowering),
/// - a recognized builtin/static type name (`JuliaType::from_name`),
/// - not a single bare identifier (compound annotations like `Vector{Int}`,
///   `Union{A,B}`, or `Base.Number` keep their existing permissive path,
///   matching the #10226/#10396 scoping compromise),
/// - the name of any `where`-binder of the same method (`f(x::T) where T`).
///
/// Return-type annotations are deliberately NOT probed: upstream evaluates
/// `f(x)::UndefRet = 1` lazily at call time, not at definition time.
///
/// Keyword parameters are chained in for completeness, but lowering currently
/// drops kwparam annotations entirely (`KwParam.type_annotation` is always
/// `None` — Issue #11024), so they contribute no candidates until that gap is
/// fixed; the probes then activate automatically. Forward references to types
/// defined later in the same file remain accepted because statically
/// registered type objects skip the probe (Issue #11025, shared with #10396).
fn append_undeclared_param_annotation_names(
    params: &[crate::ir::core::TypedParam],
    kwparams: &[crate::ir::core::KwParam],
    type_params: &[crate::types::TypeParam],
    names: &mut Vec<String>,
) {
    let annotations = params
        .iter()
        .map(|p| p.type_annotation.as_ref())
        .chain(kwparams.iter().map(|p| p.type_annotation.as_ref()));
    for annotation in annotations.flatten() {
        let name = annotation.name().into_owned();
        let builtin_type = super::type_helpers::is_builtin_type_name(&name);
        let nominal_placeholder = matches!(
            annotation,
            JuliaType::Struct(struct_name)
                if crate::lowering::expr::is_bare_identifier_name(struct_name)
        );
        if !builtin_type && !nominal_placeholder {
            continue;
        }
        if type_params.iter().any(|p| p.name == name.as_str()) {
            continue;
        }
        if names.iter().any(|candidate| candidate == &name) {
            continue;
        }
        names.push(name);
    }
}

/// Recursively collect bare-identifier leaf names from a signature
/// annotation, descending into composite forms (`Vector{T}`, `Union{A, B}`,
/// `Tuple{T, Int}`, `Type{T}`) so a name used INSIDE a composite is found the
/// same way a bare annotation's own top-level name already is (Issue
/// #11321). `append_undeclared_param_annotation_names` (Issue #10582) only
/// probes a top-level nominal placeholder for UndefVarError; this feeds a
/// separate check (`emit_signature_definition_probes` below) that validates
/// a name CURRENTLY bound to a live runtime local (e.g. an active `catch`
/// binder, or an ordinary local) actually holds a legal type-parameter value
/// at definition time — a Type/TypeVar, a `Symbol`, or an `isbits` value,
/// matching upstream's rule for ANY parametric-type argument.
///
/// Deliberately does not parse a `Struct(name)` leaf's own braced text (e.g.
/// `Dict{T,Int}`, a flat parametric spelling for bases other than
/// Vector/Matrix/Tuple/Union/Type) — that composite shape is out of this
/// bounded fix's scope.
fn collect_composite_annotation_leaf_names(annotation: &JuliaType, names: &mut Vec<String>) {
    match annotation {
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            collect_composite_annotation_leaf_names(inner, names);
        }
        JuliaType::TupleOf(elems) | JuliaType::Union(elems) => {
            for elem in elems {
                collect_composite_annotation_leaf_names(elem, names);
            }
        }
        JuliaType::Struct(name)
            if crate::lowering::expr::is_bare_identifier_name(name)
                && !names.iter().any(|n| n == name) =>
        {
            names.push(name.clone());
        }
        _ => {}
    }
}

/// Collect nominal spellings from a hoisted module signature for the narrow
/// runtime-conditional publication check. Unlike the ordinary undeclared-name
/// probe, qualified `Struct` identities are intentionally retained: module
/// lowering has already resolved `T` to `Owner.T`, but that compiler identity
/// is only an inventory entry until the conditional declaration executes
/// (Issues #11025/#11654).
fn collect_runtime_nominal_annotation_names(annotation: &JuliaType, names: &mut Vec<String>) {
    match annotation {
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            collect_runtime_nominal_annotation_names(inner, names);
        }
        JuliaType::TupleOf(elements) | JuliaType::Union(elements) => {
            for element in elements {
                collect_runtime_nominal_annotation_names(element, names);
            }
        }
        JuliaType::Struct(name) if !names.contains(name) => names.push(name.clone()),
        _ => {}
    }
}

fn literal_i64(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(Literal::Int(v), _) => Some(*v),
        _ => None,
    }
}

fn is_unqualified_or_base_call(function: &str, name: &str) -> bool {
    function == name
        || function
            .strip_prefix("Base.")
            .is_some_and(|qualified| qualified == name)
}

fn is_dict_struct_name(name: &str) -> bool {
    // Split on `{` BEFORE stripping a module prefix: a parametric name like
    // `Dict{Symbolics.Num,Int64}` has a dot *inside* its type parameters, so
    // `rsplit('.')` on the whole string would wrongly yield `Num,Int64}`
    // (Issue #7173). Isolate the base (`Dict`) first, then drop any module
    // qualifier on it (`Base.Dict` -> `Dict`).
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    base == "Dict"
}

fn is_array_wrapper_compat_field(field: &str) -> bool {
    matches!(field, "_mem" | "_size")
}

/// Whether `value` is (syntactically) an `expr.args` field access — the sole
/// current producer of a genuinely-known `ArrayOf(ArrayElementType::Any,
/// Some(1))` (Issue #10206 / #10267): `Expr.args` really is `Vector{Any}` at
/// both compile time and runtime, unlike a comprehension's rank-known,
/// element-unresolved placeholder of the exact same `ValueType` shape. Used to
/// seed `known_any_rank_array_locals` conservatively — only this proven-Any
/// shape is marked; every other producer defaults to the safe "unknown, defer
/// to runtime dispatch" bridge treatment.
fn is_expr_args_field_access(value: &Expr) -> bool {
    matches!(value, Expr::FieldAccess { field, .. } if field == "args")
}

fn eachindex_array_var(iterable: &Expr) -> Option<&str> {
    base_unary_call_array_var(iterable, &["eachindex"])
}

fn proven_inbounds_loop_array_var(iterable: &Expr) -> Option<&str> {
    eachindex_array_var(iterable)
        .or_else(|| axes_dim1_array_var(iterable))
        .or_else(|| one_to_length_array_var(iterable))
}

fn axes_dim1_array_var(iterable: &Expr) -> Option<&str> {
    match iterable {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if is_unqualified_or_base_call(function, "axes")
            && args.len() == 2
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s)
            && literal_i64(&args[1]) == Some(1) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if module == "Base"
            && function == "axes"
            && args.len() == 2
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s)
            && literal_i64(&args[1]) == Some(1) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn one_to_length_array_var(iterable: &Expr) -> Option<&str> {
    match iterable {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if (is_unqualified_or_base_call(function, "OneTo")
            || is_unqualified_or_base_call(function, "oneto"))
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            base_unary_call_array_var(args.first()?, &["length", "lastindex"])
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if module == "Base"
            && (function == "OneTo" || function == "oneto")
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            base_unary_call_array_var(args.first()?, &["length", "lastindex"])
        }
        _ => None,
    }
}

fn base_unary_call_array_var<'a>(expr: &'a Expr, names: &[&str]) -> Option<&'a str> {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if names
            .iter()
            .any(|name| is_unqualified_or_base_call(function, name))
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if module == "Base"
            && names.contains(&function.as_str())
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|s| !*s)
            && kwargs_splat_mask.iter().all(|s| !*s) =>
        {
            match args.first()? {
                Expr::Var(name, _) => Some(name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn positive_unit_length_loop_array_var<'a>(
    start: &Expr,
    end: &'a Expr,
    const_step: i64,
) -> Option<&'a str> {
    if const_step != 1 {
        return None;
    }

    let end_var = base_unary_call_array_var(end, &["length", "lastindex"])?;
    if literal_i64(start) == Some(1) {
        return Some(end_var);
    }

    let start_var = base_unary_call_array_var(start, &["firstindex"])?;
    (start_var == end_var).then_some(end_var)
}

/// Fold a pure, side-effect-free expression to a compile-time constant value.
///
/// Returns `Some(value)` only when the entire expression is built from constant
/// literals combined with pure arithmetic / comparison / boolean operators that
/// the const-evaluator (`compile::const_prop`) can evaluate. Any variable, call,
/// indexing, or unsupported operator yields `None` — folding is conservative so
/// it can never change observable behaviour.
///
/// Reuses the same `eval_const_binary` / `eval_const_unary` semantics that the
/// abstract interpreter relies on, so Julia-specific rules (truncated `%`, `÷`,
/// Int64 overflow checks, `/` producing Float64) stay in a single place.
#[cfg(test)]
#[allow(dead_code)]
fn fold_const_value(expr: &Expr) -> Option<crate::compile::lattice::types::ConstValue> {
    crate::compile::const_prop::fold_expr_const_value(expr, &|_| None)
}

/// Fold an `if`/ternary condition to a statically-known boolean when possible.
///
/// Powers dead-branch elimination (Issue #5182): conditions like `if 1 < 2`,
/// `if true && false`, or `if !false` collapse to a single branch at compile
/// time, removing the condition computation, the `JumpIfZero`, and the dead
/// branch's bytecode entirely. A bare `Expr::Literal(Literal::Bool(_))` is the
/// trivial case; this generalises it to any pure const-foldable expression that
/// evaluates to a `Bool`.
/// True only for conditions whose `Bool` value is determined WITHOUT any method
/// dispatch: literal `Bool`s and `&&`/`||`/`!` combinations of such.
///
/// Comparison/equality operators (`==`, `!=`, `<`, `<=`, `>`, `>=`, `<:`) and
/// `arithmetic` all dispatch to methods that user code may override
/// (Issue #4298 — e.g. a user `==(::String, ::String)`), so a condition that
/// contains one is NOT safe to const-fold for dead-branch elimination: the
/// runtime value can differ from the literal fold. `&&`/`||` (`BinaryOp::And`/
/// `Or`) are short-circuit control flow, not method calls, so they are safe.
#[cfg(test)]
#[allow(dead_code)]
fn is_dispatch_free_bool_condition(expr: &Expr) -> bool {
    is_dispatch_free_bool_condition_with_lookup(expr, &|_| None)
}

fn is_dispatch_free_bool_condition_with_lookup<F>(expr: &Expr, lookup_const: &F) -> bool
where
    F: Fn(&str) -> Option<crate::compile::lattice::types::ConstValue>,
{
    use crate::ir::core::{BinaryOp, UnaryOp};
    match expr {
        Expr::Literal(Literal::Bool(_), _) => true,
        Expr::Var(name, _) => matches!(
            lookup_const(name),
            Some(crate::compile::lattice::types::ConstValue::Bool(_))
        ),
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
            ..
        } => is_dispatch_free_bool_condition_with_lookup(operand, lookup_const),
        Expr::BinaryOp {
            op: BinaryOp::And | BinaryOp::Or,
            left,
            right,
            ..
        } => {
            is_dispatch_free_bool_condition_with_lookup(left, lookup_const)
                && is_dispatch_free_bool_condition_with_lookup(right, lookup_const)
        }
        _ => false,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(super) fn const_bool_condition(condition: &Expr) -> Option<bool> {
    const_bool_condition_with_lookup(condition, &|_| None)
}

pub(super) fn const_bool_condition_with_lookup<F>(
    condition: &Expr,
    lookup_const: &F,
) -> Option<bool>
where
    F: Fn(&str) -> Option<crate::compile::lattice::types::ConstValue>,
{
    // Dead-branch elimination (Issue #5182) must only fire when the condition's
    // `Bool` value is independent of method dispatch. Folding comparison/equality
    // operators here discards user-overridden methods: `if "a" == "a"` with a
    // user `==(::String, ::String) = false` was being eliminated to the wrong
    // (then) branch, regressing Issue #4298. Restrict to dispatch-free conditions.
    if !is_dispatch_free_bool_condition_with_lookup(condition, lookup_const) {
        return None;
    }
    match crate::compile::const_prop::fold_expr_const_value(condition, lookup_const)? {
        crate::compile::lattice::types::ConstValue::Bool(b) => Some(b),
        _ => None,
    }
}

/// Check if a direct type conversion is possible between two value types.
///
/// Only I64↔F64 conversions are supported by dedicated VM instructions
/// (ToF64 and ToI64). All other type coercions go through Pure Julia `convert()`.
pub(super) fn can_convert_type(from: ValueType, to: ValueType) -> bool {
    matches!(
        (from, to),
        (ValueType::I64, ValueType::F64) | (ValueType::F64, ValueType::I64)
    )
}

fn target_preserves_boxed_numeric_values(target_ty: Option<&ValueType>) -> bool {
    matches!(
        target_ty,
        Some(
            ValueType::MemoryOf(
                ArrayElementType::Any
                    | ArrayElementType::UnionOf(_)
                    | ArrayElementType::Abstract(_)
                    | ArrayElementType::Structured(_)
            ) | ValueType::ArrayOf(
                ArrayElementType::Any
                    | ArrayElementType::UnionOf(_)
                    | ArrayElementType::Abstract(_)
                    | ArrayElementType::Structured(_),
                _,
            ) | ValueType::Struct(_)
                | ValueType::Any
        )
    )
}

pub(in crate::compile) fn should_return_as_expected_type(
    actual_ty: &ValueType,
    expected_ty: &ValueType,
) -> bool {
    actual_ty == expected_ty
        || matches!(expected_ty, ValueType::Any)
        // A `Union` return type is inherently type-unstable: at runtime the value
        // can be any member of the union, so the return must preserve the value's
        // runtime tag via `emit_return_for_type(Union) => ReturnAny`, NOT narrow to
        // the compiler's flow-sensitive guess for one path. Without this, a
        // `Union{Int64,Float64}`-returning function whose tail reads a boxed
        // loop-carried variable typed `F64` emitted `ReturnF64`, coercing a
        // runtime `Int64` (empty-loop path) to `Float64` (Issue #9145).
        || matches!(expected_ty, ValueType::Union(_))
        || (matches!(actual_ty, ValueType::Any)
            && matches!(
                expected_ty,
                ValueType::I64 | ValueType::F64 | ValueType::F32 | ValueType::F16
            ))
}

fn const_declaration_marker(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call { function, args, .. }
            if function == "#__sjulia_declare_const__" && args.len() == 1 =>
        {
            match &args[0] {
                Expr::Literal(Literal::Str(name), _) => Some(name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Determine the iteration strategy for a type known at compile time.
///
/// Returns:
/// - `Some(true)`  — call Pure Julia `iterate()` (custom struct iterators, `Any` dispatch)
/// - `Some(false)` — emit a VM builtin instruction (faster path for known collections)
/// - `None`        — type is unknown; requires a runtime method-table lookup
///
/// The `None` case is handled by `should_use_pure_julia_iterate`, which falls back to
/// checking `self.method_tables` at compile time.
pub(super) fn static_iterate_strategy(ty: &JuliaType) -> Option<bool> {
    match ty {
        // CartesianIndices uses VM builtin iterate for better performance
        JuliaType::Struct(name) if name == "CartesianIndices" => Some(false),
        // All other struct types use Pure Julia iterate (custom iterators)
        JuliaType::Struct(_) => Some(true),
        // Any type: use Pure Julia dispatch (handles unknown runtime structs)
        JuliaType::Any => Some(true),
        // Builtin collection types: faster VM instructions
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => Some(false),
        JuliaType::Tuple | JuliaType::TupleOf(_) => Some(false),
        JuliaType::String => Some(false),
        JuliaType::Int64 => Some(false), // Range-like types
        // A native `Value::Generator` iterates via the VM's lazy generator
        // protocol. Use the TUPLE-based loop (not the split path, which cannot
        // suspend a frame for a filtered generator's predicate call); the
        // `emit_iterate_call_*` helpers emit `IterateFirst`/`IterateNext` for a
        // generator, which reach `start_lazy_generator_iterate_call` and drive
        // EVERY callable variant — including the collapsed FILTERED shapes (Issue
        // #9200 S3 / #9127 / #9271). Dispatching the pure-Julia
        // `iterate(g::Generator)` method instead accesses `g.f`, which is not
        // representable for a filtered generator.
        JuliaType::Generator => Some(true),
        // Unknown type; let the caller perform a dynamic method-table lookup
        _ => None,
    }
}

impl CoreCompiler<'_> {
    /// Pop the loop-frame stack pushed by the caller a few lines above, for
    /// every looping construct's `compile_*_stmt` method (`for`/`while`
    /// share this exact push → compile body → pop shape, six call sites).
    /// The `None` case is not reachable given the immediately preceding
    /// unconditional push, but is reported as a typed internal-compiler
    /// error rather than a raw unwrap so a future refactor that breaks the
    /// invariant surfaces a diagnosable bug instead of an uncaught host
    /// crash (Issue #10905, Phase 1b of #10869).
    fn pop_loop_frame(&mut self) -> CResult<LoopContext> {
        self.loop_stack
            .pop()
            .ok_or_else(|| internal_compile_error("loop frame pushed immediately above"))
    }

    /// Emit definition-time resolution probes for a function signature's
    /// `where`-bound names (Issue #10396; see [`undeclared_where_bound_names`])
    /// and parameter-annotation type names (Issue #10582; see
    /// [`append_undeclared_param_annotation_names`]).
    ///
    /// Builtin names need no probe. User struct/abstract type objects are
    /// skipped only when their source-order definition precedes this method
    /// (Issue #11025). Canonically expanded aliases reach those same concrete
    /// forms. An alias spelling deliberately left unexpanded by the source-order
    /// filter (Issue #11086) MUST still be probed: `LoadAny` resolves an
    /// earlier/prior-eval runtime binding, but raises `UndefVarError` when the
    /// alias is defined later in this source, matching upstream's eager
    /// signature evaluation.
    ///
    /// `def_span_start` dedupes per definition: a function inside a
    /// top-level block statement is reached both by the `Stmt::FunctionDef`
    /// arm (first, inside any enclosing `try` handler region) and by the
    /// top-level source-order activation flush (later, outside it) — only
    /// the first emission may probe, or an UndefVarError caught by the
    /// user's `try` would be re-raised uncatchably at the flush.
    /// The source-order ordinal of a user-defined type's own definition, when
    /// known (Issue #11025).
    fn type_definition_position(
        &self,
        name: &str,
    ) -> Option<super::context::TypeDefinitionPosition> {
        self.shared_ctx.type_definition_positions.get(name).copied()
    }

    /// Emit the definition-time visibility checks that remain necessary when a
    /// module-level function has been hoisted out of `Module.body`.
    ///
    /// The general eager signature probe below also validates unresolved user
    /// type names. Module lowering historically leaves those names to the
    /// module import/type resolver, whose qualified and renamed bindings have
    /// separate activation machinery. Replaying that broader probe here would
    /// change those established semantics. Issue #11419 specifically closes
    /// the hoist bypass for authority-owned builtin spellings, including
    /// parametric annotations and `where` bounds. Runtime-conditional nominal
    /// declarations are the other exception: their compiler registry entry is
    /// only an inventory, so a hoisted signature must probe the qualified
    /// runtime binding before it can activate (Issues #11025/#11654).
    pub(super) fn emit_hoisted_module_builtin_signature_probes(
        &mut self,
        type_params: &[crate::types::TypeParam],
        params: &[crate::ir::core::TypedParam],
        kwparams: &[crate::ir::core::KwParam],
        def_span_start: usize,
    ) {
        let mut hidden_names = Vec::new();
        for type_param in type_params {
            for bound in [
                type_param.lower_bound.as_ref(),
                type_param.get_upper_bound(),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(hidden) = self.first_hidden_builtin_type_binding(bound) {
                    if !hidden_names.contains(&hidden) {
                        hidden_names.push(hidden);
                    }
                }
            }
        }
        for annotation in params
            .iter()
            .map(|param| param.type_annotation.as_ref())
            .chain(kwparams.iter().map(|param| param.type_annotation.as_ref()))
            .flatten()
        {
            if let Some(hidden) = self.first_hidden_builtin_type_binding(annotation.name().as_ref())
            {
                if !hidden_names.contains(&hidden) {
                    hidden_names.push(hidden);
                }
            }
        }
        let mut runtime_nominal_names = Vec::new();
        let mut runtime_probe_names = undeclared_where_bound_names(type_params);
        for annotation in params
            .iter()
            .map(|parameter| parameter.type_annotation.as_ref())
            .chain(
                kwparams
                    .iter()
                    .map(|parameter| parameter.type_annotation.as_ref()),
            )
            .flatten()
        {
            collect_runtime_nominal_annotation_names(annotation, &mut runtime_probe_names);
        }
        for bound_name in runtime_probe_names {
            let resolved = if let Some(module) = &self.current_module_path {
                let qualified = format!("{module}.{bound_name}");
                self.shared_ctx
                    .current_input_runtime_nominal_names
                    .contains(&qualified)
                    .then_some(qualified)
            } else {
                self.shared_ctx
                    .current_input_runtime_nominal_names
                    .contains(&bound_name)
                    .then_some(bound_name.clone())
            };
            if let Some(resolved) = resolved {
                if !runtime_nominal_names.contains(&resolved) {
                    runtime_nominal_names.push(resolved);
                }
            }
        }
        if (hidden_names.is_empty() && runtime_nominal_names.is_empty())
            || !self.where_probe_emitted_spans.insert(def_span_start)
        {
            return;
        }
        for hidden_name in hidden_names {
            self.emit_unbound_module_name(&hidden_name);
            self.emit(Instr::Pop);
        }
        for runtime_nominal_name in runtime_nominal_names {
            self.emit(Instr::ProbeRuntimeBinding(runtime_nominal_name));
            self.emit(Instr::Pop);
        }
    }

    pub(super) fn emit_signature_definition_probes(
        &mut self,
        type_params: &[crate::types::TypeParam],
        params: &[crate::ir::core::TypedParam],
        kwparams: &[crate::ir::core::KwParam],
        def_span_start: usize,
        def_definition_order: u64,
    ) {
        if !self.where_probe_emitted_spans.insert(def_span_start) {
            return;
        }
        // `where`-bounds first: upstream constructs the method TypeVars before
        // evaluating the signature's parameter annotations.
        let mut probe_names = undeclared_where_bound_names(type_params);
        append_undeclared_param_annotation_names(params, kwparams, type_params, &mut probe_names);
        for bound_name in probe_names {
            if super::type_helpers::is_builtin_type_name(&bound_name) {
                if let Some(hidden_name) = self.first_hidden_builtin_type_binding(&bound_name) {
                    self.emit_unbound_module_name(&hidden_name);
                    self.emit(Instr::Pop);
                }
                continue;
            }
            // Issue #11025: "the compiler can resolve this name as a type object"
            // is NOT the same as "the type exists at this point in the source" —
            // the struct table is populated for the WHOLE program regardless of
            // source order, so a FORWARD reference (`f(x::S) = 1` before
            // `struct S end`) used to be skipped silently, while upstream, which
            // evaluates signature annotations eagerly when the definition
            // executes, raises `UndefVarError` there. Skip only a type whose own
            // definition comes EARLIER in evaluation order than this definition.
            let owned_type = self.resolve_owned_type_object_name(&bound_name);
            let declared_runtime_name = self.runtime_nominal_declared_name(&bound_name);
            let runtime_nominal_probe_name = self
                .shared_ctx
                .current_input_runtime_nominal_names
                .contains(&declared_runtime_name)
                .then_some(declared_runtime_name)
                .or_else(|| {
                    owned_type.as_ref().and_then(|resolved| {
                        self.shared_ctx
                            .current_input_runtime_nominal_names
                            .contains(resolved)
                            .then_some(resolved.clone())
                    })
                });
            let resolved_type = runtime_nominal_probe_name
                .clone()
                .or_else(|| owned_type.clone())
                .or_else(|| self.resolved_active_imported_type_name(&bound_name));
            let runtime_nominal_type = runtime_nominal_probe_name.is_some();
            if !runtime_nominal_type
                && resolved_type.as_ref().is_some_and(|resolved| {
                    self.type_definition_position(resolved)
                        .or_else(|| self.type_definition_position(&bound_name))
                        .is_none_or(|type_position| {
                            type_position.is_before(def_definition_order, def_span_start)
                        })
                })
            {
                continue;
            }
            if owned_type.is_none() && self.imported_bindings.contains(&bound_name) {
                // Whole-program metadata knows every eventual import, but the
                // runtime activation state is authoritative at this source
                // position, including an unresolved ambiguity (Issues
                // #11203/#11216).
                self.emit_load_imported_binding(&bound_name);
            } else {
                let probe_name = if runtime_nominal_type {
                    runtime_nominal_probe_name.as_deref().unwrap_or(&bound_name)
                } else {
                    &bound_name
                };
                self.emit(Instr::ProbeRuntimeBinding(probe_name.to_string()));
            }
            self.emit(Instr::Pop);
        }

        // Issue #11321: a name used INSIDE a composite annotation
        // (`Vector{T}`) that is CURRENTLY shadowed by a live runtime local —
        // most notably an active `catch` binder — is invisible to the flat
        // probe above (it only inspects a top-level nominal placeholder).
        // Upstream evaluates every signature annotation eagerly against the
        // binding visible at the definition's source position; when that
        // binding is a runtime local rather than a compile-time type, its
        // value must be validated the same way upstream validates ANY
        // parametric-type argument: a Type/TypeVar, a `Symbol`, or an
        // `isbits` value are all legal (e.g. `x = 7; Vector{x}` is
        // `Vector{7}`, a real upstream `DataType` — NOT a `TypeError`), and
        // everything else (a caught exception instance, a `String`, ...)
        // raises `TypeError`. An earlier version of this probe emitted a
        // bare `name <: Any` (`BuiltinId::Subtype`), which demands the
        // operand literally BE a Type — that wrongly rejected the isbits
        // case above, a verified regression against both upstream and
        // pre-existing sjulia `main` behavior for that shape.
        //
        // Route through `ApplyTypeDynamic` / `apply_type_to_runtime_base`'s
        // existing `type_arg_value_to_julia_type` classification instead of
        // re-deriving the rule here — that is the runtime's one existing
        // authority for "is this value a legal type-parameter argument",
        // already exercised by a dynamic-base application (`T{x}`). Every
        // builtin container family's single element-type binder carries no
        // bound tighter than `<: Any`, so which literal head is used to
        // reach that authority does not change which VALUES it accepts;
        // `Vector` is used as a fixed, arbitrary carrier and the resulting
        // `DataType` is immediately discarded — this is a pure validation
        // probe, not a real type application.
        //
        // Gating on BOTH `self.locals` and `self.initialized_locals` (not
        // just the former) is essential: the compiler's whole-function
        // pre-scan seeds `locals` for every eventual local before any
        // statement compiles, so `locals` alone would also fire for a
        // genuine forward reference to a name assigned LATER in the same
        // function — that gap belongs to #11114/#11118's separate probe
        // machinery, not this fix, and is left untouched.
        let mut composite_leaf_names = Vec::new();
        for annotation in params
            .iter()
            .map(|p| p.type_annotation.as_ref())
            .chain(kwparams.iter().map(|p| p.type_annotation.as_ref()))
            .flatten()
        {
            collect_composite_annotation_leaf_names(annotation, &mut composite_leaf_names);
        }
        for name in composite_leaf_names {
            if type_params.iter().any(|p| p.name == name) {
                continue;
            }
            if !self.locals.contains_key(name.as_str())
                || !self.initialized_locals.contains(name.as_str())
            {
                continue;
            }
            self.emit(Instr::PushDataType("Vector".to_string()));
            self.emit(Instr::LoadAny(name.clone()));
            self.emit(Instr::ApplyTypeDynamic(1));
            self.emit(Instr::Pop);
        }
    }

    pub(super) fn emit_eval_function_activation_once(&mut self, index: usize) {
        if self.emitted_eval_function_activations.insert(index) {
            self.emit(Instr::DefineEvalFunction(index));
        }
    }

    /// Mark every function definition inside a DEAD-CODE-ELIMINATED branch as
    /// already probe-handled (Issue #10396 follow-up, PR #10594 review): the
    /// branch's statements never compile, so its `Stmt::FunctionDef` arms never
    /// run — but the top-level source-order activation flush still activates
    /// the hoisted definitions and would probe their `where`-bound names,
    /// raising `UndefVarError` for a definition upstream never evaluates
    /// (`if false; h(x::T) where T<:Undef = x; end` must be a no-op). Inserting
    /// the span into `where_probe_emitted_spans` makes the flush's dedupe skip
    /// the probe. Recurses through nested block-bearing statements.
    pub(super) fn suppress_where_probes_in_eliminated_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                    self.where_probe_emitted_spans.insert(func.span.start);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.suppress_where_probes_in_eliminated_block(then_branch);
                    if let Some(else_block) = else_branch {
                        self.suppress_where_probes_in_eliminated_block(else_block);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::For { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForEachTuple { body, .. }
                | Stmt::Timed { body, .. }
                | Stmt::TestSet { body, .. } => {
                    self.suppress_where_probes_in_eliminated_block(body);
                }
                Stmt::Try {
                    try_block,
                    catch_block,
                    else_block,
                    finally_block,
                    ..
                } => {
                    self.suppress_where_probes_in_eliminated_block(try_block);
                    for b in [catch_block, else_block, finally_block]
                        .into_iter()
                        .flatten()
                    {
                        self.suppress_where_probes_in_eliminated_block(b);
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn compile_block(&mut self, block: &Block) -> CResult<()> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn snapshot_explicit_scope_binding_metadata(&self) -> ExplicitScopeBindingMetadata {
        ExplicitScopeBindingMetadata {
            locals: self.locals.clone(),
            initialized_locals: self.initialized_locals.clone(),
            julia_type_locals: self.julia_type_locals.clone(),
            known_any_rank_array_locals: self.known_any_rank_array_locals.clone(),
            mixed_type_vars: self.mixed_type_vars.clone(),
            function_aliases: self.function_aliases.clone(),
            lexical_function_tables: self.lexical_function_tables.clone(),
            type_value_aliases: self.type_value_aliases.clone(),
            module_aliases: self.module_aliases.clone(),
        }
    }

    fn restore_explicit_scope_binding_metadata(&mut self, metadata: ExplicitScopeBindingMetadata) {
        self.locals = metadata.locals;
        self.initialized_locals = metadata.initialized_locals;
        self.julia_type_locals = metadata.julia_type_locals;
        self.known_any_rank_array_locals = metadata.known_any_rank_array_locals;
        self.mixed_type_vars = metadata.mixed_type_vars;
        self.function_aliases = metadata.function_aliases;
        self.lexical_function_tables = metadata.lexical_function_tables;
        self.type_value_aliases = metadata.type_value_aliases;
        self.module_aliases = metadata.module_aliases;
    }

    /// Join compiler facts after a loop that may have reassigned a binding
    /// owned by an enclosing explicit lexical scope.
    ///
    /// The inner loop snapshots restore fresh induction/temp bindings, but a
    /// blanket restore would also resurrect the enclosing binding's pre-loop
    /// type/alias facts. That is unsound for both zero-or-more iteration flow
    /// and widening assignments such as `s = 0; for i in big_range; s += i`.
    fn widen_outer_lexical_assignments_after_loop(&mut self, body: &Block, binders: &[String]) {
        if !self.explicit_lexical_scopes {
            return;
        }
        let binders: HashSet<&str> = binders.iter().map(String::as_str).collect();
        let inventory = crate::lowering::soft_scope::ScopeBindingInventory::collect(body);
        for name in inventory.binding_names() {
            if binders.contains(name.as_str())
                || inventory.globals.contains(name)
                || !self.explicit_lexical_owner_active(name)
            {
                continue;
            }
            self.locals.insert(name.clone(), ValueType::Any);
            self.julia_type_locals.remove(name);
            self.known_any_rank_array_locals.remove(name);
            self.mixed_type_vars.insert(name.clone());
            self.function_aliases.remove(name);
            self.lexical_function_tables.remove(name);
            self.type_value_aliases.remove(name);
            self.module_aliases.remove(name);
        }
    }

    fn compile_soft_scope_block(&mut self, block: &Block, binders: &[String]) -> CResult<()> {
        let explicit_lexical = self.explicit_lexical_scopes;
        let previous_binding_metadata =
            explicit_lexical.then(|| self.snapshot_explicit_scope_binding_metadata());
        let previous_scope = self.lexical_scope_locals.clone();
        let previous_declared_globals = self.declared_globals.clone();
        let inventory = crate::lowering::soft_scope::ScopeBindingInventory::collect(block);
        let binder_names: HashSet<&str> = binders.iter().map(String::as_str).collect();
        let mut owned_names: Vec<String> = inventory
            .binding_names()
            .filter(|name| {
                if binder_names.contains(name.as_str()) {
                    return false;
                }
                let explicitly_new = inventory.explicit_locals.contains(*name)
                    || inventory.compiler_enclosing.contains(*name);
                // A soft assignment updates an enclosing lexical owner (for
                // example `let x=0; for ...; x += 1; end`) or an already
                // initialized interactive/global binding. Only an explicit
                // `local` declaration creates a fresh inner owner in those
                // cases (Issue #11569).
                explicitly_new
                    || !(self.explicit_lexical_owner_active(name)
                        || self.initialized_locals.contains(*name)
                        || (self.local_scope_depth == 0
                            && self.preexisting_global_bindings.contains(*name)))
            })
            .cloned()
            .collect();
        owned_names.sort();
        owned_names.dedup();
        self.lexical_scope_locals.extend(binders.iter().cloned());
        self.lexical_scope_locals
            .extend(inventory.binding_names().cloned());
        // At module depth zero, `global x` is already implicit and the normal
        // store path must retain `current_module_path` qualification. A bare
        // `declared_globals` entry is only the right routing signal from a
        // genuine local scope (function/let/testset), matching try clauses.
        if self.strict_undefined_check || self.local_scope_depth > 0 {
            self.declared_globals
                .extend(inventory.globals.iter().cloned());
        }
        for name in inventory
            .explicit_locals
            .iter()
            .chain(&inventory.compiler_enclosing)
            .chain(binders)
        {
            self.declared_globals.remove(name);
        }

        // A soft-scope assignment owns its binding throughout the body, so a
        // read before the first reached store must observe an uninitialised
        // local rather than falling back to frame zero.  The loop binder(s)
        // already have a longer-lived physical owner and are deliberately not
        // redeclared here.
        if explicit_lexical {
            for name in &owned_names {
                self.locals.insert(name.clone(), ValueType::Any);
                self.initialized_locals.remove(name);
                self.julia_type_locals.remove(name);
                self.known_any_rank_array_locals.remove(name);
                self.mixed_type_vars.insert(name.clone());
            }
        }
        let entered_lexical = explicit_lexical && self.enter_explicit_lexical_scope(owned_names);
        if entered_lexical {
            self.scope_cleanup_stack.push(ScopeCleanupContext {
                names: Vec::new(),
                shadows: Vec::new(),
                lexical_scope_count: 1,
                loop_depth: self.loop_stack.len(),
                cleanup_on_loop_exit: true,
                nonlocal_pop_handler: false,
                nonlocal_pop_caught_exception: false,
            });
        }
        let result = self.compile_block(block);
        if entered_lexical {
            self.scope_cleanup_stack.pop();
            // Keep the compiler's declaration-owner stack balanced even when
            // compilation fails.  Failed bytecode is discarded, while normal
            // execution reaches this exit only when no non-local jump already
            // emitted the matching cleanup.
            self.exit_explicit_lexical_scope();
        }
        if let Some(metadata) = previous_binding_metadata {
            self.restore_explicit_scope_binding_metadata(metadata);
        }
        self.lexical_scope_locals = previous_scope;
        self.declared_globals = previous_declared_globals;
        result
    }

    fn compile_condition_value(&mut self, condition: &Expr) -> CResult<ValueType> {
        match condition {
            Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => {
                self.compile_block_as_condition_value(body)
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => self.compile_ternary_as_condition_value(condition, then_expr, else_expr),
            _ => self.compile_expr(condition),
        }
    }

    fn compile_block_as_condition_value(&mut self, block: &Block) -> CResult<ValueType> {
        let stmts = &block.stmts;
        if stmts.is_empty() {
            self.emit(Instr::PushNothing);
            return Ok(ValueType::Nothing);
        }

        for stmt in stmts.iter().take(stmts.len() - 1) {
            self.compile_stmt(stmt)?;
        }

        match &stmts[stmts.len() - 1] {
            Stmt::Expr { expr, .. } => self.compile_condition_value(expr),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.compile_if_as_condition_value(condition, then_branch, else_branch.as_ref()),
            Stmt::Block(block) => self.compile_block_as_condition_value(block),
            stmt => {
                self.compile_stmt(stmt)?;
                self.emit(Instr::PushNothing);
                Ok(ValueType::Nothing)
            }
        }
    }

    fn compile_ternary_as_condition_value(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
    ) -> CResult<ValueType> {
        let false_jumps = self.compile_condition_false_jumps(condition)?;
        let then_ty = self.compile_condition_value(then_expr)?;
        let jump_end = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let else_start = self.here();
        for patch_pos in false_jumps {
            self.patch_jump(patch_pos, else_start);
        }

        let else_ty = self.compile_condition_value(else_expr)?;
        let end = self.here();
        self.patch_jump(jump_end, end);

        if then_ty == else_ty {
            Ok(then_ty)
        } else {
            Ok(ValueType::Any)
        }
    }

    fn compile_if_as_condition_value(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> CResult<ValueType> {
        let false_jumps = self.compile_condition_false_jumps(condition)?;
        let then_ty = self.compile_block_as_condition_value(then_branch)?;
        let jump_end = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let else_start = self.here();
        for patch_pos in false_jumps {
            self.patch_jump(patch_pos, else_start);
        }

        let else_ty = if let Some(else_branch) = else_branch {
            self.compile_block_as_condition_value(else_branch)?
        } else {
            self.emit(Instr::PushNothing);
            ValueType::Nothing
        };

        let end = self.here();
        self.patch_jump(jump_end, end);

        if then_ty == else_ty {
            Ok(then_ty)
        } else {
            Ok(ValueType::Any)
        }
    }

    /// Compile a condition in branch context, returning jumps to patch to the
    /// false target. The generated code falls through when the condition is
    /// true and leaves no Bool value on the stack.
    ///
    /// This keeps `if`/`while` conditions from materializing `&&` / `||` as
    /// stack Bool values. For `a && b`, false exits are emitted directly after
    /// each operand; for `a || b`, a true left operand skips the right operand.
    /// Leaf conditions still use `JumpIfZero`, preserving the VM's Bool-only
    /// control-flow check instead of treating numbers as truthy (Issue #6162).
    pub(in crate::compile) fn compile_condition_false_jumps(
        &mut self,
        condition: &Expr,
    ) -> CResult<Vec<usize>> {
        match condition {
            Expr::Literal(Literal::Bool(true), _) => Ok(Vec::new()),
            Expr::Literal(Literal::Bool(false), _) => {
                let j_false = self.here();
                self.emit(Instr::Jump(usize::MAX));
                Ok(vec![j_false])
            }
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
                ..
            } => self.compile_condition_true_jumps(operand),
            Expr::BinaryOp {
                op: BinaryOp::And,
                left,
                right,
                ..
            } => {
                let mut false_jumps = self.compile_condition_false_jumps(left)?;
                false_jumps.extend(self.compile_condition_false_jumps(right)?);
                Ok(false_jumps)
            }
            Expr::BinaryOp {
                op: BinaryOp::Or,
                left,
                right,
                ..
            } => {
                let true_jumps = self.compile_condition_true_jumps(left)?;
                let false_jumps = self.compile_condition_false_jumps(right)?;
                let true_start = self.here();
                for patch_pos in true_jumps {
                    self.patch_jump(patch_pos, true_start);
                }
                Ok(false_jumps)
            }
            _ => {
                self.compile_condition_value(condition)?;
                let j_false = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                Ok(vec![j_false])
            }
        }
    }

    fn compile_condition_true_jumps(&mut self, condition: &Expr) -> CResult<Vec<usize>> {
        match condition {
            Expr::Literal(Literal::Bool(true), _) => {
                let j_true = self.here();
                self.emit(Instr::Jump(usize::MAX));
                Ok(vec![j_true])
            }
            Expr::Literal(Literal::Bool(false), _) => Ok(Vec::new()),
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
                ..
            } => self.compile_condition_false_jumps(operand),
            Expr::BinaryOp {
                op: BinaryOp::And,
                left,
                right,
                ..
            } => {
                let false_jumps = self.compile_condition_false_jumps(left)?;
                let true_jumps = self.compile_condition_true_jumps(right)?;
                let false_start = self.here();
                for patch_pos in false_jumps {
                    self.patch_jump(patch_pos, false_start);
                }
                Ok(true_jumps)
            }
            Expr::BinaryOp {
                op: BinaryOp::Or,
                left,
                right,
                ..
            } => {
                let mut true_jumps = self.compile_condition_true_jumps(left)?;
                true_jumps.extend(self.compile_condition_true_jumps(right)?);
                Ok(true_jumps)
            }
            _ => {
                self.compile_condition_value(condition)?;
                let j_false = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_true = self.here();
                self.emit(Instr::Jump(usize::MAX));
                let false_start = self.here();
                self.patch_jump(j_false, false_start);
                Ok(vec![j_true])
            }
        }
    }

    /// Refine `self.locals` for the duration of a guarded `then` branch
    /// (Issue #5181). Recognizes `x isa T` / `c1 && c2` guards and overlays a
    /// concrete [`ValueType`] for each narrowed variable so that loads and
    /// arithmetic inside the branch specialize.
    ///
    /// Returns a restore snapshot: for every refined variable, the
    /// `(name, narrowed_type, previous_binding)` triple. Pass it to
    /// [`Self::restore_then_narrowings`] right after the branch is compiled so
    /// the refinement never leaks past the branch.
    ///
    /// Variables that are abstract-numeric params or captured closure vars are
    /// skipped: those are always loaded via `LoadAny`/`LoadCaptured` regardless
    /// of `self.locals`, so refining them would be inert at best and risk
    /// confusing downstream return-type handling.
    pub(in crate::compile) fn apply_then_narrowings(
        &mut self,
        condition: &Expr,
    ) -> Vec<(String, ValueType, Option<ValueType>)> {
        let struct_id_for = |name: &str| self.shared_ctx.get_exact_struct_type_id(name);
        let current_type_for = |name: &str| self.locals.get(name).cloned();
        let facts = super::narrowing::then_branch_narrowings_with_current(
            condition,
            &current_type_for,
            &struct_id_for,
        );
        self.apply_branch_narrowing_facts(facts)
    }

    /// Refine `self.locals` for the duration of a guarded `else` branch when
    /// union splitting proves the negated guard has a single concrete type
    /// (Issue #5077).
    pub(in crate::compile) fn apply_else_narrowings(
        &mut self,
        condition: &Expr,
    ) -> Vec<(String, ValueType, Option<ValueType>)> {
        let struct_id_for = |name: &str| self.shared_ctx.get_exact_struct_type_id(name);
        let current_type_for = |name: &str| self.locals.get(name).cloned();
        let facts =
            super::narrowing::else_branch_narrowings(condition, &current_type_for, &struct_id_for);
        self.apply_branch_narrowing_facts(facts)
    }

    fn apply_branch_narrowing_facts(
        &mut self,
        facts: Vec<(String, ValueType)>,
    ) -> Vec<(String, ValueType, Option<ValueType>)> {
        let mut restore = Vec::new();
        for (name, narrowed) in facts {
            if self.abstract_numeric_params.contains(&name) || self.captured_vars.contains(&name) {
                continue;
            }
            // Only refine when the variable is an actual local whose current
            // static type is strictly less precise than the narrowed type.
            // Refining an already-concrete or unrelated typed slot could only
            // mistype it, so we leave those alone.
            match self.locals.get(&name) {
                Some(ValueType::Any) | Some(ValueType::Union(_)) => {}
                _ => continue,
            }
            let prev = self.locals.insert(name.clone(), narrowed.clone());
            restore.push((name, narrowed, prev));
        }
        restore
    }

    /// Undo the refinements applied by branch narrowing.
    ///
    /// If the branch reassigned a narrowed variable, `self.locals` no longer
    /// holds the narrowed type we inserted — Julia variables are function-scoped
    /// so that assignment must persist past the branch. We therefore only roll a
    /// variable back when its current binding is still exactly the narrowed type
    /// we installed (i.e. the branch only *read* it).
    pub(in crate::compile) fn restore_then_narrowings(
        &mut self,
        restore: Vec<(String, ValueType, Option<ValueType>)>,
    ) {
        for (name, narrowed, prev) in restore {
            if self.locals.get(&name) != Some(&narrowed) {
                // The branch rebound the variable; keep its post-branch type.
                continue;
            }
            match prev {
                Some(ty) => {
                    self.locals.insert(name, ty);
                }
                None => {
                    self.locals.remove(&name);
                }
            }
        }
    }

    /// Undo branch refinements even when the guarded expression assigned to the
    /// narrowed variable. This is used for short-circuit value expressions:
    /// `cond && (x = ...)` only executes the assignment on one path, so keeping
    /// the RHS-only slot type after the expression is unsound (Issue #7546).
    pub(super) fn restore_short_circuit_narrowings(
        &mut self,
        restore: Vec<(String, ValueType, Option<ValueType>)>,
    ) {
        for (name, _narrowed, prev) in restore {
            match prev {
                Some(ty) => {
                    self.locals.insert(name, ty);
                }
                None => {
                    self.locals.remove(&name);
                }
            }
        }
    }

    /// Compile an integer range `for` loop whose step is a compile-time constant
    /// (Issue #5166).
    ///
    /// Because the step sign is statically known, the per-iteration sign check is
    /// hoisted out entirely: a positive step emits a single `JumpIfGtI64` exit test
    /// (exit when `var > stop`) and a negative step emits `JumpIfLtI64` (exit when
    /// `var < stop`). The increment is specialized to `IncVarI64` / `DecVarI64`
    /// (with a `PushI64` of the magnitude for non-unit steps).
    ///
    /// The user-provided `stop` is stored verbatim (no `last` precompute), so the
    /// number of iterations and overflow/wrapping behavior match the dynamic path.
    /// `const_step` must be non-zero (callers route zero steps to the dynamic path).
    fn compile_const_step_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        const_step: i64,
        body: &Block,
        shadow: Option<ShadowedLocal>,
    ) -> CResult<()> {
        debug_assert!(const_step != 0, "zero step must use the dynamic path");

        let stop_var = self.new_temp("stop");
        let start_var = self.new_temp("start");
        let explicit_lexical = self.explicit_lexical_scopes;
        let outer_binding_metadata =
            explicit_lexical.then(|| self.snapshot_explicit_scope_binding_metadata());
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![stop_var.clone(), start_var.clone()]);
        }

        // Compile and store the (user-provided) stop value, then initialize the
        // loop variable to start.
        //
        // Issue #9321: an `Any`-inferred bound (`for i in 1:n`) may arrive as a
        // `Float` at runtime (`n = 5.5`). Coerce it to `Int` with upstream range
        // last-element semantics — `floor` for an ascending step, `ceil` for a
        // descending one — instead of `compile_expr_as`'s `DynamicToI64` which
        // truncates toward zero (so `for i in -3:-1.5` would iterate three times
        // instead of upstream's two). A statically integer-typed bound keeps the
        // direct path.
        //
        // Issue #9377: `CoerceRangeStopI64` peeks `start` (bottom) and `step`
        // (middle) beneath the bound to distinguish the legal empty direction
        // (`1:-Inf` → length 0) from a counting-direction non-finite /
        // out-of-`Int64` bound, which raises the upstream `InexactError`
        // (`1:Inf`, `1:1e30`). `const_step` is pushed purely as the sign
        // operand and discarded; the `start` operand is reused to initialize
        // the loop variable so `start` is still evaluated exactly once.
        self.compile_expr_as(start, ValueType::I64)?;
        self.emit(Instr::StoreI64(start_var.clone()));
        if matches!(self.infer_expr_type(end), ValueType::Any) {
            self.emit(Instr::LoadI64(start_var.clone()));
            self.emit(Instr::PushI64(const_step));
            self.compile_expr(end)?;
            self.emit(Instr::CoerceRangeStopI64);
            self.emit(Instr::StoreI64(stop_var.clone()));
            self.emit(Instr::Pop);
        } else {
            self.compile_expr_as(end, ValueType::I64)?;
            self.emit(Instr::StoreI64(stop_var.clone()));
        }
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![var.to_string()]);
            self.locals.insert(var.to_string(), ValueType::I64);
            self.initialized_locals.insert(var.to_string());
        }
        self.emit(Instr::LoadI64(start_var));
        self.emit(Instr::StoreI64(var.to_string()));

        let loop_start = self.here();

        let mut loop_ctx = LoopContext {
            exit_patches: Vec::new(),
            continue_patches: Vec::new(),
        };

        // Single-direction exit test. The step sign is known at compile time:
        //   step > 0: exit when var > stop  (JumpIfGtI64)
        //   step < 0: exit when var < stop  (JumpIfLtI64)
        self.emit(Instr::LoadI64(var.to_string()));
        self.emit(Instr::LoadI64(stop_var));
        let j_exit = self.here();
        if const_step > 0 {
            self.emit(Instr::JumpIfGtI64(usize::MAX));
        } else {
            self.emit(Instr::JumpIfLtI64(usize::MAX));
        }
        loop_ctx.exit_patches.push(j_exit);

        // Compile body with loop context.
        let inbounds_array_var = positive_unit_length_loop_array_var(start, end, const_step);
        if let Some(array_var) = inbounds_array_var {
            self.push_proven_inbounds_index(array_var, var);
        }
        self.loop_stack.push(loop_ctx);
        if explicit_lexical {
            self.scope_cleanup_stack.push(ScopeCleanupContext {
                names: Vec::new(),
                shadows: Vec::new(),
                lexical_scope_count: 2,
                loop_depth: self.loop_stack.len(),
                cleanup_on_loop_exit: false,
                nonlocal_pop_handler: false,
                nonlocal_pop_caught_exception: false,
            });
        }
        let body_result = self.compile_soft_scope_block(body, &[var.to_string()]);
        if explicit_lexical {
            self.scope_cleanup_stack.pop();
        }
        let loop_ctx = self.pop_loop_frame()?;
        if inbounds_array_var.is_some() {
            self.pop_proven_inbounds_index();
        }
        body_result?;

        let continue_target = self.here();

        // Constant increment. `IncVarI64`/`DecVarI64` pop the (de/in)crement from the
        // stack and wrapping-add/sub it into the slot, matching the AddI64 wrapping
        // semantics of the dynamic path. We push the magnitude (always >= 1) and use
        // `IncVarI64` for positive steps and `DecVarI64` for negative steps so the
        // single-direction loop never needs the step's sign at runtime.
        if const_step > 0 {
            if explicit_lexical {
                self.emit(Instr::LoadI64(var.to_string()));
                self.emit(Instr::PushI64(const_step));
                self.emit(Instr::AddI64);
                self.emit(Instr::StoreI64(var.to_string()));
            } else {
                self.emit(Instr::PushI64(const_step));
                self.emit(Instr::IncVarI64(var.to_string()));
            }
        } else {
            // step < 0: decrement by |step|. `const_step` is negative and non-zero;
            // negate it to obtain a positive magnitude. `i64::MIN` cannot reach here:
            // it has no positive literal counterpart, so `const_int_step` returns
            // `None` (via `checked_neg`) for that pathological case and the loop falls
            // back to the dynamic path. Hence the negation below cannot overflow.
            let magnitude = const_step.checked_neg().ok_or_else(|| {
                internal_compile_error(
                    "non-zero constant step magnitude must be representable (i64::MIN excluded above)",
                )
            })?;
            if explicit_lexical {
                self.emit(Instr::LoadI64(var.to_string()));
                self.emit(Instr::PushI64(magnitude));
                self.emit(Instr::SubI64);
                self.emit(Instr::StoreI64(var.to_string()));
            } else {
                self.emit(Instr::PushI64(magnitude));
                self.emit(Instr::DecVarI64(var.to_string()));
            }
        }

        self.emit(Instr::Jump(loop_start));

        let exit = self.here();
        // Issue #10984 / #10903: restore a shadowed outer local, if any, at
        // the loop's single normal/break-exit convergence point (`exit`).
        if explicit_lexical {
            self.exit_explicit_lexical_scope();
            self.exit_explicit_lexical_scope();
        } else if let Some(shadow) = shadow {
            self.shadow_local_exit(shadow);
        }
        for patch_pos in loop_ctx.exit_patches {
            self.patch_jump(patch_pos, exit);
        }
        for patch_pos in loop_ctx.continue_patches {
            self.patch_jump(patch_pos, continue_target);
        }

        if let Some(metadata) = outer_binding_metadata {
            self.restore_explicit_scope_binding_metadata(metadata);
        }
        self.widen_outer_lexical_assignments_after_loop(body, &[var.to_string()]);

        Ok(())
    }

    /// Compile a function body with implicit return handling.
    /// In Julia, the last expression in a function is its return value.
    /// Issue #8118: pre-scan a function body's directly-nested function
    /// definitions and transitively propagate the captured locals of sibling
    /// closures into the capture set of every nested function that
    /// (transitively) calls them.
    ///
    /// A nested function `b` that captures an enclosing local `s` becomes a
    /// closure invoked through its captured environment. A sibling `a` that
    /// calls `b` must be able to reconstruct `b`'s environment at the call site
    /// (see `compile_self_or_sibling_closure_call`), which requires `a` to hold
    /// every local `b` captured — even when `a` does not reference those locals
    /// directly. Without this, mutually-recursive closures that capture an
    /// enclosing local fail at runtime with `Unknown function: <sibling>`
    /// (PR #8142 fixed the self-recursive and capture-free mutual cases; this
    /// covers the remaining capture-an-enclosing-local mutual case).
    ///
    /// We compute each nested function's base free variables against the *full*
    /// enclosing local scope (the body's statements have not been compiled yet,
    /// so `self.locals` lacks them), then fixpoint-union the captures of every
    /// called sibling, and merge the expanded sets into
    /// `shared_ctx.closure_captures` so both the `CreateClosure` emission below
    /// and the per-function `captured_vars` setup observe them.
    fn prescan_mutual_closure_captures(&mut self, block: &Block) {
        // Closures only capture *enclosing locals*, which exist inside a
        // function body (strict scope), not at module top level.
        if !self.strict_undefined_check {
            return;
        }
        let Some(parent) = self.current_function_name.clone() else {
            return;
        };

        // Directly-nested function definitions in this block.
        let nested: Vec<&Function> = block
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::FunctionDef { func, .. } => Some(func.as_ref()),
                _ => None,
            })
            .collect();
        // Sibling mutual recursion needs at least two nested functions; a single
        // self-recursive closure is already handled by reconstruction.
        if nested.len() < 2 {
            return;
        }

        // Full enclosing local scope: params + already-captured names + every
        // name bound in this function's own lexical scope, regardless of source
        // order. Do not descend into nested hard scopes: a name first assigned
        // inside a loop/try/let belongs to that child and is not capturable by a
        // later sibling function (Issue #11278).
        let mut outer_scope_vars: HashSet<String> = self.locals.keys().cloned().collect();
        outer_scope_vars.extend(self.captured_vars.iter().cloned());
        outer_scope_vars.extend(
            crate::lowering::soft_scope::ScopeBindingInventory::collect(block)
                .binding_names()
                .cloned(),
        );

        let nested_names: HashSet<String> = nested.iter().map(|f| f.name.clone()).collect();

        // Base captures (enclosing-scope DATA variables only) + called-sibling
        // references for each nested function. Sibling function names are
        // EXCLUDED from captures: a sibling is resolved at the call site, either
        // by name (plain nested function) or by reconstructing its closure from
        // the captures both siblings now share — never data-captured. Capturing
        // a sibling's name would make a mutually-recursive group uncapturable
        // (each name's value is another not-yet-built closure).
        let mut caps: HashMap<String, HashSet<String>> = HashMap::new();
        let mut called: HashMap<String, HashSet<String>> = HashMap::new();
        for f in &nested {
            let base: HashSet<String> = analyze_free_variables(f, &outer_scope_vars)
                .into_iter()
                .filter(|name| !nested_names.contains(name))
                .collect();
            let refs: HashSet<String> =
                crate::compile::ipo::call_graph::extract_called_functions(&f.body)
                    .into_iter()
                    .filter(|name| nested_names.contains(name) && name != &f.name)
                    .collect();
            caps.insert(f.name.clone(), base);
            called.insert(f.name.clone(), refs);
        }

        // Fixpoint: a function that calls a sibling closure must capture
        // everything that sibling captures. Iterate until no set grows.
        loop {
            let mut changed = false;
            let names: Vec<String> = caps.keys().cloned().collect();
            for name in &names {
                let siblings = called[name].clone();
                let mut additions: HashSet<String> = HashSet::new();
                for sib in &siblings {
                    if let Some(sib_caps) = caps.get(sib) {
                        for c in sib_caps {
                            if !caps[name].contains(c) {
                                additions.insert(c.clone());
                            }
                        }
                    }
                }
                if !additions.is_empty() {
                    if let Some(set) = caps.get_mut(name) {
                        set.extend(additions);
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // Record the expanded capture sets as authoritative for this body's
        // FunctionDef compilation. Only non-empty sets matter: an empty set is a
        // plain nested function (or a capture-free mutual group like the cases
        // PR #8142 already handles) and is left to the existing free-variable
        // path, keeping this change scoped to the capture-an-enclosing-local
        // mutual-recursion case.
        for f in &nested {
            let Some(expanded) = caps.get(&f.name) else {
                continue;
            };
            if expanded.is_empty() {
                continue;
            }
            let qualified = format!("{}#{}", parent, f.name);
            self.mutual_closure_captures
                .insert(qualified, expanded.clone());
        }
    }

    pub(super) fn compile_function_body(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        self.lexical_scope_locals = self.initialized_locals.clone();
        crate::lowering::soft_scope::collect_scope_level_bindings(
            block,
            &mut self.lexical_scope_locals,
        );
        // Pre-scan the body for `global x` declarations so that reads and writes
        // of those names route to the module-level frame for the whole scope,
        // matching upstream Julia (Issues #5548, #5549). A `global` declaration
        // applies to the entire local scope regardless of its position, so this
        // must happen before any statement is compiled. This only matters inside
        // a function: at module scope the binding is *already* global, so a
        // `global x` there is a no-op and routing it through `declared_globals`
        // would needlessly widen the variable's type to `Any`.
        if self.strict_undefined_check {
            collect_declared_globals(block, &mut self.declared_globals);
        }
        for name in &self.declared_globals {
            self.lexical_scope_locals.remove(name);
        }

        // Issue #8118: propagate sibling closures' captures so mutually-recursive
        // nested closures that capture an enclosing local can reconstruct each
        // other at their call sites. Must run before any FunctionDef statement is
        // compiled (it emits CreateClosure from the capture set).
        self.prescan_mutual_closure_captures(block);

        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty function - return default value
            self.emit_default_return(return_type);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement specially
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                // Explicit return with value - compile and return it
                let ty = self.compile_expr(expr)?;
                if should_return_as_expected_type(&ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(ty);
                }
            }
            Stmt::Return { value: None, .. } => {
                // Explicit return without value
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                // Implicit return - the last expression is the return value
                let actual_ty = self.compile_expr(expr)?;
                // Try to convert to the declared return type if needed
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else if should_return_as_expected_type(&actual_ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    // Use the actual type when conversion isn't possible
                    // This handles DataType returns and other non-convertible types
                    self.emit_return_for_type(actual_ty);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // If statement as last statement in function - handle implicit return
                // Each branch should return its last expression's value
                self.compile_if_with_implicit_return(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    return_type,
                )?;
            }
            Stmt::Try { .. } => {
                // A `try/catch[/else/finally]` in tail position is an expression
                // whose value is the last expression of whichever branch ran, not
                // the type's default value (Issue #6223).
                self.compile_try_with_implicit_return(last_stmt, return_type)?;
            }
            Stmt::Block(block) => {
                self.compile_block_tail_or_destructure(block, return_type)?;
            }
            Stmt::FunctionDef { func, .. } => {
                self.compile_stmt(last_stmt)?;
                self.emit(Instr::LoadAny(func.name.clone()));
                self.emit(Instr::ReturnAny);
            }
            Stmt::EvalFunctionDef { .. } => {
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
            Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => {
                // Julia: assignment in tail position returns the assigned value.
                // `x += 4` lowers to `Stmt::AddAssign { var: "x", .. }` (or plain
                // `Stmt::Assign` from `x = value`) and previously fell into the
                // `_` arm → emit_default_return → wrong value (0 for I64, Nothing
                // for Any). (Issue #8976)
                self.compile_assign_as_tail_return(last_stmt, var)?;
            }
            Stmt::DestructuringAssign { .. } => {
                self.compile_destructuring_assign_as_tail_return(last_stmt)?;
            }
            Stmt::IndexAssign { .. } | Stmt::FieldAssign { .. } | Stmt::DictAssign { .. } => {
                // Same rule as `Stmt::Assign`/`Stmt::AddAssign` above, extended
                // to indexed/field/dict targets (`v[i] = x`, `obj.field = x`,
                // `d[k] = x`, and their `+=`-desugared shapes), which have no
                // single named variable to reload afterward (Issue #10431).
                self.compile_assign_stmt_tail_via_temp(last_stmt, return_type)?;
            }
            _ => {
                // Other statements (while, for, etc.) - compile normally and add default return
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
        }

        Ok(())
    }

    /// Compile an indexed/field/dict assignment (`Stmt::IndexAssign`/
    /// `Stmt::FieldAssign`/`Stmt::DictAssign`) sitting in tail position, then
    /// return the value that was assigned.
    ///
    /// Mirrors `compile_assign_as_tail_return`, but there is no single named
    /// variable to reload afterward, so it goes through
    /// `split_assign_stmt_via_temp` (shared with the lowering-layer
    /// `assign_block_tail_value`, Issue #10431): bind the RHS to a fresh
    /// compiler-internal temporary, perform the store using the temporary,
    /// then reload and return the temporary's value.
    fn compile_assign_stmt_tail_via_temp(
        &mut self,
        last_stmt: &Stmt,
        return_type: ValueType,
    ) -> CResult<()> {
        let Some((tmp, init, store)) =
            crate::lowering::expr::split_assign_stmt_via_temp(last_stmt.clone())
        else {
            // The match arm that dispatches here only matches
            // IndexAssign/FieldAssign/DictAssign, all of which
            // `split_assign_stmt_via_temp` handles, so this is unreachable —
            // stay defensive rather than panicking.
            self.compile_stmt(last_stmt)?;
            self.emit_default_return(return_type);
            return Ok(());
        };
        self.compile_stmt(&init)?;
        self.compile_stmt(&store)?;
        let loaded_ty = self.locals.get(&tmp).cloned().unwrap_or(ValueType::Any);
        self.load_local(&tmp)?;
        self.emit_return_for_type(loaded_ty);
        Ok(())
    }

    fn compile_destructuring_assign_as_tail_return(&mut self, stmt: &Stmt) -> CResult<()> {
        let Stmt::DestructuringAssign { targets, value, .. } = stmt else {
            unreachable!("destructuring tail helper requires DestructuringAssign");
        };
        let Expr::TupleLiteral { elements, span } = value else {
            return self.compile_nonliteral_destructuring_assign(targets, value, true);
        };
        if elements.len() != targets.len() {
            return self.compile_iterated_destructuring_assign(targets, value, true);
        }
        let temps: Vec<String> = elements
            .iter()
            .map(|_| self.new_temp("destructure"))
            .collect();
        for ((target, element), temp) in targets.iter().zip(elements).zip(&temps) {
            self.compile_stmt(&Stmt::Assign {
                var: temp.clone(),
                value: element.clone(),
                span: *span,
            })?;
            self.compile_stmt(&Stmt::Assign {
                var: target.clone(),
                value: Expr::Var(temp.clone().into(), *span),
                span: *span,
            })?;
        }
        let result = Expr::TupleLiteral {
            elements: temps
                .iter()
                .map(|temp| Expr::Var(temp.clone().into(), *span))
                .collect(),
            span: *span,
        };
        let ty = self.compile_expr(&result)?;
        self.emit_return_for_type(ty);
        Ok(())
    }

    fn compile_nonliteral_destructuring_assign(
        &mut self,
        targets: &[String],
        value: &Expr,
        tail: bool,
    ) -> CResult<()> {
        let inferred = self.infer_julia_type(value);
        let elem_value_types: Vec<ValueType> = match &inferred {
            JuliaType::TupleOf(elems) => elems
                .iter()
                .map(|jt| self.julia_type_to_value_type_resolved(jt))
                .collect(),
            _ => return self.compile_iterated_destructuring_assign(targets, value, tail),
        };

        let actual_ty = self.compile_expr(value)?;
        if actual_ty != ValueType::Tuple {
            let temp = self.new_temp("destructure");
            self.store_local(&temp, actual_ty);
            return self.compile_iterated_destructuring_from_temp(
                targets,
                &temp,
                value.span(),
                &inferred,
                tail,
            );
        }
        let temp_tuple = self.new_temp("tuple");
        self.emit(Instr::StoreTuple(temp_tuple.clone()));

        for (i, target) in targets.iter().enumerate() {
            self.emit(Instr::LoadTuple(temp_tuple.clone()));
            self.emit(Instr::PushI64((i + 1) as i64));
            self.emit(Instr::TupleGet);
            match elem_value_types.get(i) {
                Some(ValueType::I64) => {
                    self.emit(Instr::DynamicToI64);
                    self.emit(Instr::StoreI64(target.clone()));
                    self.locals.insert(target.clone(), ValueType::I64);
                }
                Some(ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                    self.emit(Instr::StoreF64(target.clone()));
                    self.locals.insert(target.clone(), ValueType::F64);
                }
                _ => {
                    self.emit(Instr::StoreAny(target.clone()));
                    self.locals.insert(target.clone(), ValueType::Any);
                }
            }
        }

        if tail {
            self.emit(Instr::LoadTuple(temp_tuple));
            self.emit_return_for_type(ValueType::Tuple);
        }
        Ok(())
    }

    fn compile_iterated_destructuring_assign(
        &mut self,
        targets: &[String],
        value: &Expr,
        tail: bool,
    ) -> CResult<()> {
        let span = value.span();
        let temp = self.new_temp("destructure");
        let iterable_ty = self.infer_julia_type(value);
        let actual_ty = self.compile_expr(value)?;
        self.store_local(&temp, actual_ty);

        self.compile_iterated_destructuring_from_temp(targets, &temp, span, &iterable_ty, tail)
    }

    fn compile_iterated_destructuring_from_temp(
        &mut self,
        targets: &[String],
        temp: &str,
        span: crate::span::Span,
        iterable_ty: &JuliaType,
        tail: bool,
    ) -> CResult<()> {
        let state = self.new_temp("destructure_state");
        let result = self.new_temp("destructure_iterate");
        for (index, target) in targets.iter().enumerate() {
            self.load_local(temp)?;
            if index == 0 {
                self.emit_iterate_call_1(iterable_ty)?;
            } else {
                self.emit(Instr::LoadAny(state.clone()));
                self.emit_iterate_call_2(iterable_ty)?;
            }
            self.emit(Instr::StoreAny(result.clone()));

            self.emit(Instr::LoadAny(result.clone()));
            self.emit(Instr::IsNothing);
            let has_value = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            self.compile_destructuring_bounds_error(temp, index + 1, span)?;
            let extract = self.here();
            self.patch_jump(has_value, extract);

            self.emit(Instr::LoadAny(result.clone()));
            self.emit(Instr::TupleSecond);
            self.emit(Instr::StoreAny(state.clone()));
            self.emit(Instr::LoadAny(result.clone()));
            self.emit(Instr::TupleFirst);
            self.store_local(target, ValueType::Any);
        }

        if tail {
            let loaded_ty = self.locals.get(temp).cloned().unwrap_or(ValueType::Any);
            self.load_local(temp)?;
            self.emit_return_for_type(loaded_ty);
        }
        Ok(())
    }

    fn compile_destructuring_bounds_error(
        &mut self,
        iterable: &str,
        index: usize,
        span: crate::span::Span,
    ) -> CResult<()> {
        let bounds_error = Expr::Call {
            function: "BoundsError".to_string().into(),
            args: vec![
                Expr::Var(iterable.to_string().into(), span),
                Expr::Literal(crate::ir::core::Literal::Int(index as i64), span),
            ],
            kwargs: vec![],
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span,
        };
        let throw = Expr::Call {
            function: "throw".to_string().into(),
            args: vec![bounds_error],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        };
        let _ = self.compile_expr(&throw)?;
        Ok(())
    }

    /// Compile an assignment (`Stmt::Assign` or `Stmt::AddAssign`) sitting in
    /// tail position, then reload the assigned variable and return its value.
    ///
    /// `compile_stmt` handles all the store logic (global routing, type
    /// widening, const checks). After it runs, the stack is empty, so this
    /// reloads `var` and returns it — matching upstream Julia, where an
    /// assignment expression evaluates to the assigned value.
    fn compile_assign_as_tail_return(&mut self, last_stmt: &Stmt, var: &str) -> CResult<()> {
        self.compile_stmt(last_stmt)?;
        let loaded_ty = if self.declared_globals.contains(var) {
            // Declared globals are always loaded as Any (LoadGlobalAny).
            ValueType::Any
        } else {
            self.locals.get(var).cloned().unwrap_or(ValueType::Any)
        };
        self.load_local(var)?;
        self.emit_return_for_type(loaded_ty);
        Ok(())
    }

    fn emit_default_return(&mut self, return_type: ValueType) {
        match return_type {
            ValueType::I64 => {
                self.emit(Instr::PushI64(0));
                self.emit(Instr::ReturnI64);
            }
            ValueType::F64 => {
                self.emit(Instr::PushF64(0.0));
                self.emit(Instr::ReturnF64);
            }
            ValueType::Struct(_type_id) => {
                // For struct return types without explicit return, return Nothing
                self.emit(Instr::ReturnNothing);
            }
            _ => {
                self.emit(Instr::ReturnNothing);
            }
        }
    }

    pub(super) fn emit_return_for_type(&mut self, ty: ValueType) {
        match ty {
            ValueType::I64 => self.emit(Instr::ReturnI64),
            ValueType::F64 => self.emit(Instr::ReturnF64),
            ValueType::Array | ValueType::ArrayOf(_, _) => self.emit(Instr::ReturnArray),
            ValueType::Str => self.emit(Instr::ReturnAny), // String uses dynamic return
            // Nothing type: use ReturnAny to consume the Nothing value pushed by compile_expr.
            // ReturnNothing does NOT pop the stack, so using it here would leave an orphaned
            // Nothing on the stack, corrupting nested call chains (Issue #2072).
            ValueType::Nothing => self.emit(Instr::ReturnAny),
            ValueType::Missing => self.emit(Instr::ReturnAny),
            ValueType::Struct(_) | ValueType::ComplexF32 | ValueType::ComplexF64 => {
                self.emit(Instr::ReturnStruct)
            }
            ValueType::Rng => self.emit(Instr::ReturnRng),
            ValueType::Range => self.emit(Instr::ReturnRange),
            ValueType::Tuple => self.emit(Instr::ReturnTuple),
            ValueType::NamedTuple => self.emit(Instr::ReturnNamedTuple),
            ValueType::Dict | ValueType::Set => self.emit(Instr::ReturnDict),
            ValueType::Generator => self.emit(Instr::ReturnAny),
            ValueType::Char => self.emit(Instr::ReturnAny),
            ValueType::Any => self.emit(Instr::ReturnAny),
            ValueType::DataType => self.emit(Instr::ReturnAny),
            ValueType::Module => self.emit(Instr::ReturnAny),
            ValueType::BigInt => self.emit(Instr::ReturnAny),
            ValueType::BigFloat => self.emit(Instr::ReturnAny),
            ValueType::IO => self.emit(Instr::ReturnAny),
            ValueType::Function => self.emit(Instr::ReturnAny),
            // Narrow integer types: ReturnI64 handler already preserves the original Value type
            // (I8/I16/I32/I128/U8–U128/Bool) via `preserved_val`, so using ReturnI64 is safe
            // and informs the AoT compiler that the return type is integer-family. (Issue #3255)
            ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::Bool => self.emit(Instr::ReturnI64),
            ValueType::F32 => self.emit(Instr::ReturnF32),
            ValueType::F16 => self.emit(Instr::ReturnF16),
            // Macro system types
            ValueType::Symbol
            | ValueType::Expr
            | ValueType::QuoteNode
            | ValueType::LineNumberNode
            | ValueType::GlobalRef => self.emit(Instr::ReturnAny),
            // Pairs type (for kwargs...)
            ValueType::Pairs => self.emit(Instr::ReturnAny),
            // Regex types
            ValueType::Regex | ValueType::RegexMatch => self.emit(Instr::ReturnAny),
            // Enum type
            ValueType::Enum => self.emit(Instr::ReturnAny),
            // Union type
            ValueType::Union(_) => self.emit(Instr::ReturnAny),
            // Memory type
            ValueType::Memory | ValueType::MemoryOf(_) => self.emit(Instr::ReturnAny),
        }
    }

    /// Emit type conversion instructions from actual to target type.
    /// Note: Complex conversions are handled via Pure Julia convert() functions.
    pub(in crate::compile) fn emit_type_conversion(&mut self, from: ValueType, to: ValueType) {
        match (from, to) {
            (ValueType::I64, ValueType::F64) => self.emit(Instr::ToF64),
            (ValueType::F64, ValueType::I64) => self.emit(Instr::ToI64),
            // Other conversions are not needed or not possible
            _ => {}
        }
    }

    /// Compile an if statement as the last statement in a function with implicit return.
    /// Each branch returns its last expression's value instead of falling through.
    fn compile_if_with_implicit_return(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
        return_type: ValueType,
    ) -> CResult<()> {
        // Dead code elimination: skip provably dead branches.
        // Fires on a bare Bool literal (Issue #3364) and, via the const-bool
        // folder, on any pure const-foldable condition such as `if 1 < 2` or
        // `if true && false` (Issue #5182).
        if let Some(b) = const_bool_condition_with_lookup(condition, &|name| {
            self.const_values.get(name).cloned()
        }) {
            if b {
                // Condition is always true: only compile then-branch
                self.compile_block_with_implicit_return(then_branch, return_type)?;
                if let Some(else_block) = else_branch {
                    self.suppress_where_probes_in_eliminated_block(else_block);
                }
            } else {
                self.suppress_where_probes_in_eliminated_block(then_branch);
                if let Some(else_block) = else_branch {
                    // Condition is always false: only compile else-branch
                    self.compile_block_with_implicit_return(else_block, return_type)?;
                } else {
                    // Condition is always false, no else: return default
                    self.emit_default_return(return_type);
                }
            }
            return Ok(());
        }

        let condition_false_jumps = self.compile_condition_false_jumps(condition)?;

        // Compile then-branch with implicit return, with flow-sensitive local
        // narrowing applied for `isa`-guarded conditions (Issue #5181).
        let narrow_restore = self.apply_then_narrowings(condition);
        self.compile_block_with_implicit_return(then_branch, return_type.clone())?;
        self.restore_then_narrowings(narrow_restore);

        // If there's an else branch, we need to jump over it after then-branch
        // (But since then-branch ends with a return, this jump is actually unreachable)
        // However, we still need the else label for the JumpIfZero
        let else_start = self.here();
        for patch_pos in condition_false_jumps {
            self.patch_jump(patch_pos, else_start);
        }

        // Compile else-branch with implicit return. For two-member unions, an
        // `isa` guard can prove the negated branch has the remaining concrete
        // type, giving the first codegen-connected union-split path (Issue #5077).
        if let Some(else_block) = else_branch {
            let else_restore = self.apply_else_narrowings(condition);
            self.compile_block_with_implicit_return(else_block, return_type)?;
            self.restore_then_narrowings(else_restore);
        } else {
            // No else branch - return default value
            self.emit_default_return(return_type);
        }

        Ok(())
    }

    /// Compile a nested `Stmt::Block` reached in tail/implicit-return
    /// position (from `compile_function_body` or
    /// `compile_block_with_implicit_return`), routing a tuple-destructuring
    /// decomposition (`(a, b) = rhs`) to its reconstructed RHS value instead
    /// of blindly recursing and returning the last per-target assignment's
    /// value (Issue #10431). See
    /// `crate::lowering::expr::destructuring_tail_value` for the shape this
    /// detects and why an ordinary nested `begin ... end` is unaffected.
    fn compile_block_tail_or_destructure(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        let Some(value_expr) = crate::lowering::expr::destructuring_tail_value(&block.stmts) else {
            return self.compile_block_with_implicit_return(block, return_type);
        };
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        let actual_ty = self.compile_expr(&value_expr)?;
        if actual_ty != return_type && can_convert_type(actual_ty.clone(), return_type.clone()) {
            self.emit_type_conversion(actual_ty, return_type.clone());
            self.emit_return_for_type(return_type);
        } else if should_return_as_expected_type(&actual_ty, &return_type) {
            self.emit_return_for_type(return_type);
        } else {
            self.emit_return_for_type(actual_ty);
        }
        Ok(())
    }

    /// Compile a block with implicit return (the last statement returns its value).
    fn compile_block_with_implicit_return(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty block - return default value
            self.emit_default_return(return_type);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement - it determines the return value
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                let ty = self.compile_expr(expr)?;
                if should_return_as_expected_type(&ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(ty);
                }
            }
            Stmt::Return { value: None, .. } => {
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                let actual_ty = self.compile_expr(expr)?;
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else if should_return_as_expected_type(&actual_ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(actual_ty);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Nested if - recursively handle
                self.compile_if_with_implicit_return(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    return_type,
                )?;
            }
            Stmt::Try { .. } => {
                // Tail-position `try/catch[/else/finally]` returns the executed
                // branch's value rather than the type default (Issue #6223).
                self.compile_try_with_implicit_return(last_stmt, return_type)?;
            }
            Stmt::Block(block) => {
                self.compile_block_tail_or_destructure(block, return_type)?;
            }
            Stmt::FunctionDef { func, .. } => {
                self.compile_stmt(last_stmt)?;
                self.emit(Instr::LoadAny(func.name.clone()));
                self.emit(Instr::ReturnAny);
            }
            Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => {
                // Same rule as the top-level function body (Issue #8976): an
                // assignment in tail position returns the assigned value, not
                // the return type's default. This block-recursion helper is
                // used for `if`/nested-`Block` branches, and also for `global
                // x = value` bodies — those lower to
                // `Stmt::Block([Stmt::Global, Stmt::Assign])`, which recurses
                // here via the `Stmt::Block` arm above (Issue #10023).
                self.compile_assign_as_tail_return(last_stmt, var)?;
            }
            Stmt::DestructuringAssign { .. } => {
                self.compile_destructuring_assign_as_tail_return(last_stmt)?;
            }
            Stmt::IndexAssign { .. } | Stmt::FieldAssign { .. } | Stmt::DictAssign { .. } => {
                // Same rule as `Stmt::Assign`/`Stmt::AddAssign` above, extended
                // to indexed/field/dict targets (Issue #10431). This
                // block-recursion helper is also reached for a `begin ... end`
                // tail nested inside an `if` branch or another block.
                self.compile_assign_stmt_tail_via_temp(last_stmt, return_type)?;
            }
            _ => {
                // Other statements - compile normally and return default
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
        }

        Ok(())
    }

    /// Compile a tail-position `try/catch[/else/finally]` as an implicit
    /// return. The `Stmt::Try` is converted into the same value-producing
    /// `Expr::LetBlock` form used in expression position (Issue #4784), so the
    /// returned value is the last expression of whichever branch executed
    /// instead of the return type's default (Issue #6223). Falls back to plain
    /// statement compilation + default return when the conversion fails (only
    /// possible for a non-`Try` statement, which never reaches here).
    fn compile_try_with_implicit_return(
        &mut self,
        stmt: &Stmt,
        return_type: ValueType,
    ) -> CResult<()> {
        let span = stmt.span();
        match crate::lowering::expr::try_stmt_into_value_expr(stmt.clone(), span) {
            Some(expr) => {
                let actual_ty = self.compile_expr(&expr)?;
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else if should_return_as_expected_type(&actual_ty, &return_type) {
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(actual_ty);
                }
                Ok(())
            }
            None => {
                self.compile_stmt(stmt)?;
                self.emit_default_return(return_type);
                Ok(())
            }
        }
    }

    fn store_module_alias_runtime_value(&mut self, name: &str) {
        if self.explicit_lexical_owner_active(name) {
            self.emit(Instr::StoreAny(name.to_string()));
            return;
        }
        if self.declared_globals.contains(name) {
            self.emit_store_declared_global(name);
            return;
        }
        if !self.strict_undefined_check && self.local_scope_depth == 0 {
            if let Some(module_path) = &self.current_module_path {
                if self
                    .module_constants
                    .get(module_path)
                    .is_some_and(|constants| constants.contains(name))
                {
                    self.emit(Instr::StoreGlobalAny(format!("{module_path}.{name}")));
                    return;
                }
            }
        }
        self.emit(Instr::StoreAny(name.to_string()));
    }

    pub(super) fn compile_stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        self.set_current_span(stmt.span());
        if self.compile_try_stmt(stmt)?.is_some() {
            return Ok(());
        }

        match stmt {
            Stmt::Block(block) => {
                // Inline block: compile all statements in the block
                self.compile_block(block)?;
                Ok(())
            }
            Stmt::Assign { var, value, span } => {
                if self.const_bindings.contains(var)
                    && !self.pending_const_bindings.remove(var)
                    && !self.strict_undefined_check
                    && self.local_scope_depth == 0
                    && !self.explicit_lexical_owner_active(var)
                {
                    self.emit(Instr::PushStr(format!(
                        "invalid assignment to constant Main.{}",
                        var
                    )));
                    self.emit(Instr::ThrowError);
                    return Ok(());
                }
                let was_pending_const = self.pending_const_bindings.remove(var);
                let folded_const_value = if was_pending_const
                    && !self.strict_undefined_check
                    && self.local_scope_depth == 0
                {
                    crate::compile::const_prop::fold_expr_const_value(value, &|name| {
                        self.const_values.get(name).cloned()
                    })
                } else {
                    None
                };
                // Lowering realizes `import M: f as g` as the synthetic
                // assignment `g = M.f`.  Compile that exact assignment from
                // the canonical source recorded during lexical import
                // resolution: a relative import intentionally does not bind
                // its source root (`M`) in this scope. Match target, lowered
                // source path, and span so neither a same-statement conflicting
                // rename nor a later ordinary `g = ...` assignment is mistaken
                // for this import machinery.
                let lowered_source = super::extract_module_path_from_expr(value);
                if self.import_alias_assignments.get(var).is_some_and(|assignments| {
                    assignments
                        .iter()
                        .any(|(candidate_lowered, _, import_span)| {
                            *import_span == *span
                                && lowered_source.as_deref() == Some(candidate_lowered.as_str())
                        })
                }) {
                    // Imports are live aliases of their source binding, not
                    // assignment-time snapshots. The authoritative import
                    // state compiles every read through its canonical source;
                    // suppress both the winning lowered assignment and later
                    // conflicting ones here (Issue #11176).
                    return Ok(());
                }
                // Check for module assignment: S = Statistics, R = Random, etc.
                // Also handle transitive aliases: T = S where S is already a module alias
                if let Expr::Var(module_name, _) = value {
                    let target_is_local = self.explicit_lexical_owner_active(var)
                        || (self.local_scope_depth > 0 && self.locals.contains_key(var));
                    if !target_is_local {
                        // A source-ordered imported submodule is runtime-visible,
                        // but at this assignment point its resolved module is known.
                        // Freeze that value into the new alias just as Julia does
                        // for `const S = Sub` (Issues #11203/#11216 hardening).
                        if let Some(resolved) =
                            self.resolved_active_imported_module_alias(module_name)
                        {
                            self.emit_module_value(&resolved);
                            self.set_resolved_module_alias(var.clone(), resolved);
                            self.locals.insert(var.clone(), ValueType::Module);
                            self.store_module_alias_runtime_value(var);
                            return Ok(());
                        }
                        // Check if it's an existing module alias (transitive alias).
                        if let Some(resolved) =
                            self.resolved_module_alias(module_name).map(str::to_string)
                        {
                            self.emit_module_value(&resolved);
                            self.set_resolved_module_alias(var.clone(), resolved);
                            self.locals.insert(var.clone(), ValueType::Module);
                            self.store_module_alias_runtime_value(var);
                            return Ok(());
                        }
                    }
                    // Check if it's a user-defined module (e.g. `const MA = Mod1`,
                    // Issue #8114). Binding a module to a `const`/variable makes the
                    // binding an alias for that module, so `MA.member` must resolve
                    // the member inside `Mod1` instead of being treated as struct
                    // field access on a `Module` value (which raised
                    // "GetFieldByName: expected struct, got Module").
                    if !target_is_local {
                        let resolved_module = self
                            .module_path_in_current_scope(module_name.as_str())
                            .or_else(|| {
                                (!self.imported_bindings.contains(module_name.as_str()))
                                    .then(|| {
                                        self.resolve_visible_module_path(module_name.as_str())
                                    })
                                    .flatten()
                            });
                        if let Some(resolved_module) = resolved_module {
                            self.emit_module_value(&resolved_module);
                            self.set_resolved_module_alias(var.clone(), resolved_module);
                            self.locals.insert(var.clone(), ValueType::Module);
                            self.store_module_alias_runtime_value(var);
                            return Ok(());
                        }
                    }
                }

                let inferred_julia_type = self.infer_julia_type(value);

                // Check if there's a pre-populated "wider" type for this variable
                // This ensures consistent type usage when a variable starts as I64
                // but later receives F64 values (e.g., sum = 0; sum = sum + f64_val)
                let target_ty = self.locals.get(var).cloned();
                let ty = self.compile_expr(value)?;

                // Check if this is a compound assignment pattern (var = var op mixed_type_var)
                // where the operand is a variable in mixed_type_vars.
                // This only applies when we know the operand is from a mixed I64/F64 variable,
                // NOT when it's an untyped parameter (which could be any type at runtime).
                let is_mixed_type_compound_assignment = match value {
                    Expr::BinaryOp { left, right, .. } => {
                        let is_left_var =
                            matches!(left.as_ref(), Expr::Var(name, _) if name == var);
                        let right_is_mixed = matches!(right.as_ref(), Expr::Var(name, _) if self.mixed_type_vars.contains(name.as_str()));
                        is_left_var && right_is_mixed
                    }
                    _ => false,
                };

                if ty == ValueType::Function {
                    if let Some(alias_target) = self.resolve_function_alias_value(value) {
                        self.function_aliases.insert(var.clone(), alias_target);
                    } else {
                        self.function_aliases.remove(var);
                    }
                } else {
                    self.function_aliases.remove(var);
                }
                self.lexical_function_tables.remove(var);

                let final_ty = match (target_ty, ty.clone()) {
                    // If target is Any AND it's a function parameter with no type annotation,
                    // keep it as Any to use StoreAny/LoadAny for dynamic type handling.
                    (Some(ValueType::Any), _) if self.any_params.contains(var) => ValueType::Any,
                    // If target is Any AND it's a mixed-type variable (F64+I64 in different branches),
                    // use dynamic typing to allow runtime type changes (Julia semantics).
                    (Some(ValueType::Any), ValueType::I64)
                    | (Some(ValueType::Any), ValueType::F64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        ValueType::Any
                    }
                    // Issue #3535/#3536: target Any AND mixed_type_vars contains var
                    // because of incompatible non-numeric reassignment (e.g. Int64
                    // and String, or Struct and Nothing). Keep the slot dynamic so
                    // every assignment compiles to StoreAny.
                    (Some(ValueType::Any), _) if self.mixed_type_vars.contains(var) => {
                        ValueType::Any
                    }
                    (Some(target), incoming)
                        if self.mixed_type_vars.contains(var)
                            && !static_assignment_types_compatible(&target, &incoming) =>
                    {
                        ValueType::Any
                    }
                    // For mixed-type variables (F64+I64 in sequence), use dynamic typing.
                    // This allows `x = 1.0; x = 2` to have typeof(x) == Int64, not Float64.
                    (Some(ValueType::F64), ValueType::I64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        // Use the actual type (I64) for proper dynamic typing
                        ty
                    }
                    (Some(ValueType::I64), ValueType::F64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        // Use the actual type (F64) for proper dynamic typing
                        ty
                    }
                    // If pre-populated type is F64 but compiled type is I64, convert.
                    // This is needed for widening where the type inference determined
                    // that a variable can be both F64 and I64 (e.g., in control flow).
                    // Only applies to non-mixed-type variables (checked above).
                    (Some(ValueType::F64), ValueType::I64) => {
                        self.emit(Instr::ToF64);
                        ValueType::F64
                    }
                    // Compound assignments (x = x op y) where y is a mixed-type variable:
                    // Preserve x's numeric type because y will be numeric at runtime.
                    // This does NOT apply when y is an untyped parameter (could be any type).
                    (Some(ValueType::I64), ValueType::Any) if is_mixed_type_compound_assignment => {
                        self.emit(Instr::DynamicToI64);
                        ValueType::I64
                    }
                    (Some(ValueType::F64), ValueType::Any) if is_mixed_type_compound_assignment => {
                        self.emit(Instr::DynamicToF64);
                        ValueType::F64
                    }
                    // If pre-populated type is Struct but compiled type is Any,
                    // preserve the struct type (compile_binary_op may return Any
                    // for dynamic dispatch but type inference correctly identified the type)
                    (Some(ValueType::Struct(type_id)), ValueType::Any) => {
                        ValueType::Struct(type_id)
                    }
                    // Issue #4827: pre-inference (collect_local_types via
                    // infer_value_type) maps `IOBuffer()` -> IO, but the
                    // compile-time `compile_expr` for the constructor can return
                    // Any when the call is routed through generic base-function
                    // dispatch rather than the IO builtin arm. Preserve the IO
                    // slot type so `infer_expr_type(buf)` reports IO at later
                    // `print(buf, …)` / `println(buf, …)` call sites, enabling the
                    // statically-IO multi-arg user-`show` split (and matching the
                    // global_types IO routing established by Issue #5035). Without
                    // this, the `_ => ty` fallback overwrote the IO slot with Any,
                    // so multi-arg `print(buf, a, x, b)` field-dumped the struct.
                    (Some(ValueType::IO), ValueType::Any) => ValueType::IO,
                    // Note: Complex type conversions are now handled via Pure Julia convert().
                    // Otherwise, use the compiled type.
                    _ => ty,
                };

                if let Some(type_value) = self.resolve_static_datatype_value(value) {
                    self.type_value_aliases.insert(var.clone(), type_value);
                } else {
                    self.type_value_aliases.remove(var);
                }

                // Track JuliaType for parametric types to enable proper dispatch.
                //
                // DESIGN PRINCIPLE: Track based on *inferred type*, not *expression form*.
                // This ensures all sources of parametric types are covered: literals,
                // variable reassignment (t2 = t1), function returns (t3 = make_pair()),
                // conditional expressions (t = if c; (1,2) else (3,4) end), etc.
                //
                // Non-parametric ValueTypes (Tuple, Array) cannot distinguish between
                // Tuple{Int64, Int64} and Tuple{String, Float64}, or Vector{Int64} and
                // Vector{Any}. We store the full JuliaType in `julia_type_locals` so
                // that `infer_julia_type()` can recover the parametric type for method
                // dispatch.
                //
                // If the new assignment does not prove a precise JuliaType, remove any
                // previous precise entry. Otherwise a reused variable such as
                // `arr = [1]; arr = Int8[1]` keeps dispatching as `Vector{Int64}`
                // even though `locals` has moved on to the current array element type.
                //
                // See Issue #1748 (original), #2305 (reassignment), #2319 (conditional),
                // #2352 (VectorOf/MatrixOf dispatch), #5588 (stale reassignment).
                let track_julia_type = matches!(
                    inferred_julia_type,
                    JuliaType::TupleOf(_) | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
                ) || matches!(&inferred_julia_type, JuliaType::Struct(name) if name.starts_with("@NamedTuple{") || is_dict_struct_name(name));
                if track_julia_type {
                    self.julia_type_locals
                        .insert(var.clone(), inferred_julia_type);
                } else {
                    self.julia_type_locals.remove(var);
                }

                // Issue #10267 / #10206: `ValueType::ArrayOf(ArrayElementType::Any,
                // Some(n))` is produced by two structurally different sources —
                // a comprehension whose element type could not be resolved
                // statically (Issue #6817, genuinely UNKNOWN at compile time) and
                // a value that really is `Vector{Any}`/`Matrix{Any}`/`Array{Any,N}`
                // (e.g. `expr.args`, KNOWN exactly). `infer_julia_type`'s
                // `Expr::Var` bridge cannot tell these apart from the `ValueType`
                // alone, so record provenance here at the one point that DOES
                // know which producer ran: only a direct `expr.args`-shaped field
                // access proves the value is genuinely `Any` with a known rank.
                // Every other producer (comprehensions) is intentionally left
                // unmarked, keeping the conservative "unknown, defer to runtime
                // dispatch" bridge behavior by default (see
                // `known_any_rank_array_locals`'s doc comment).
                if matches!(final_ty, ValueType::ArrayOf(ArrayElementType::Any, Some(_)))
                    && is_expr_args_field_access(value)
                {
                    self.known_any_rank_array_locals.insert(var.clone());
                } else {
                    self.known_any_rank_array_locals.remove(var);
                }

                self.store_local(var, final_ty);
                if was_pending_const && !self.strict_undefined_check && self.local_scope_depth == 0
                {
                    self.const_bindings.insert(var.clone());
                    if let Some(value) = folded_const_value {
                        self.const_values.insert(var.clone(), value);
                    } else {
                        self.const_values.remove(var);
                    }
                } else if !self.const_bindings.contains(var) {
                    self.const_values.remove(var);
                }
                Ok(())
            }
            Stmt::AddAssign { var, value, span } => {
                // Route compound assignment through the ordinary binary-op and
                // assignment pipelines. Besides retaining the I64/F64 fast
                // paths, this gives `Any`/BigInt and other dynamic numerics the
                // same dispatch and widening semantics as `x = x + rhs`.
                self.compile_stmt(&Stmt::Assign {
                    var: var.clone(),
                    value: Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var(var.clone().into(), *span)),
                        right: Box::new(value.clone()),
                        span: *span,
                    },
                    span: *span,
                })
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                span,
            } => {
                // Issue #3550: when start/end are typed non-Int64 integers (e.g.
                // `UInt8(1):UInt8(3)`), the optimized I64-specialized path drops
                // the element type. Rewrite the loop to the generic `ForEach`
                // path (with a lazy `Range` value) so iteration produces values
                // of the right type. The default `Int64` case continues using
                // the fast path below.
                let start_ty = self.infer_expr_type(start);
                let end_ty = self.infer_expr_type(end);
                let step_ty = step.as_ref().map(|s| self.infer_expr_type(s));
                // A start/end/step whose inferred type is a non-integer
                // (float / BigFloat) must also divert to the generic ForEach
                // path: the I64 fast path below pins every component to
                // `ValueType::I64`, so a Float-typed step that is not a bare
                // float *literal* — e.g. `0:(2π/12):2π`, where `2π/12` is a
                // `BinaryOp` and so escapes the lowering-time literal check in
                // control_for.rs — gets truncated to 0 and the loop iterates
                // zero times (Issue #7800, follow-up to #3551). `infer_expr_type`
                // resolves `π`, arithmetic, etc., so it catches computed float
                // bounds that the lowering literal heuristic cannot.
                //
                // An `Any`-inferred *step* must also divert (Issue #9291). Since
                // PR #9287 made `/` follow its operands (`has_any → Any`) instead
                // of blanket-inferring `Float64`, a computed float step like
                // `2π/12` can infer `Any` under the harness pipeline (where `π`
                // itself infers `Any`). The I64 fast path below would then
                // `DynamicToI64`-truncate the real float step to 0 and iterate
                // zero times. Routing an `Any` step through the generic `ForEach`
                // path preserves the runtime float value. This is scoped to the
                // step only: an `Any` *bound* on a stepless `start:end` loop
                // (e.g. `for i in 1:n`) stays on the hot I64 path, where
                // `DynamicToI64` matches integer range semantics.
                let needs_typed_range = matches!(
                    start_ty,
                    ValueType::I8
                        | ValueType::I16
                        | ValueType::I32
                        | ValueType::U8
                        | ValueType::U16
                        | ValueType::U32
                        | ValueType::U64
                        | ValueType::Char
                        | ValueType::F64
                        | ValueType::F32
                        | ValueType::F16
                        | ValueType::BigFloat
                        // Issue #9420: a BigInt endpoint promotes the range to
                        // `UnitRange{BigInt}`; the I64 fast path would coerce it
                        // and yield Int64 loop vars.
                        | ValueType::BigInt
                ) || matches!(
                    end_ty,
                    ValueType::I8
                        | ValueType::I16
                        | ValueType::I32
                        | ValueType::U8
                        | ValueType::U16
                        | ValueType::U32
                        | ValueType::U64
                        | ValueType::Char
                        | ValueType::F64
                        | ValueType::F32
                        | ValueType::F16
                        | ValueType::BigFloat
                        | ValueType::BigInt
                ) || matches!(
                    step_ty,
                    Some(ValueType::I8)
                        | Some(ValueType::I16)
                        | Some(ValueType::I32)
                        | Some(ValueType::U8)
                        | Some(ValueType::U16)
                        | Some(ValueType::U32)
                        | Some(ValueType::U64)
                        | Some(ValueType::F64)
                        | Some(ValueType::F32)
                        | Some(ValueType::F16)
                        | Some(ValueType::BigFloat)
                        | Some(ValueType::BigInt)
                        // Issue #9291: an `Any`-inferred explicit step (e.g. a
                        // computed float step whose operands infer `Any`) must
                        // take the generic path, or the I64 fast path truncates
                        // the runtime float step to 0.
                        | Some(ValueType::Any)
                );
                // Char ranges (`for c in 'a':'c'`) take the same generic
                // ForEach path as small-int ranges — the I64 fast path
                // below would store the loop var as Int64 codepoint,
                // bypassing `RangeValue::typed_element` which exists for
                // exactly this purpose (Issue #4796, follow-up to #4795).
                if needs_typed_range {
                    let range_expr = Expr::Range {
                        start: Box::new(start.clone()),
                        step: step.clone().map(Box::new),
                        stop: Box::new(end.clone()),
                        span: *span,
                    };
                    let foreach = Stmt::ForEach {
                        var: var.clone(),
                        iterable: range_expr,
                        body: body.clone(),
                        span: *span,
                    };
                    return self.compile_stmt(&foreach);
                }

                // Issue #10984 / #10903: `var` is a fresh binding for this
                // loop's lifetime, not a reassignment of a same-named outer
                // local. Save the outer value/type state now — BEFORE the
                // unconditional `I64` type pin just below overwrites it —
                // and restore it at the loop's single exit convergence point
                // (in whichever of the two paths below actually runs).
                let shadow = if self.explicit_lexical_scopes {
                    None
                } else {
                    Some(self.shadow_local_enter(var)?)
                };

                // For loop: for var in start:end or start:step:end
                if !self.explicit_lexical_scopes {
                    self.locals.insert(var.clone(), ValueType::I64);
                // Mark the counter initialized: both paths below store it
                // unconditionally before the loop-head test (even a
                // zero-iteration range stores `start` once), so a NESTED
                // same-name shadowing construct inside the body must see a
                // genuine live outer value and emit its guarded save —
                // previously the inner loop skipped the save and clobbered
                // this live counter mid-iteration (Issue #10984 hardening;
                // `shadow_local_exit` restores the pre-enter membership).
                    self.initialized_locals.insert(var.clone());
                }

                // Issue #5166: when the step is a compile-time constant, the per-
                // iteration sign check is redundant — the loop can only ever count in
                // one direction. Detect a constant non-zero step and emit a single-
                // direction exit test plus a constant increment. A constant step of
                // zero falls back to the dynamic path so its (pre-existing) behavior
                // is unchanged.
                if let Some(const_step) = const_int_step(step).filter(|k| *k != 0) {
                    return self.compile_const_step_for(var, start, end, const_step, body, shadow);
                }

                let outer_binding_metadata = self
                    .explicit_lexical_scopes
                    .then(|| self.snapshot_explicit_scope_binding_metadata());
                let stop_var = self.new_temp("stop");
                let step_var = self.new_temp("step");
                let explicit_lexical = self.explicit_lexical_scopes;
                let start_var = explicit_lexical.then(|| self.new_temp("start"));

                if let Some(start_var) = &start_var {
                    self.enter_explicit_lexical_scope(vec![
                        stop_var.clone(),
                        step_var.clone(),
                        start_var.clone(),
                    ]);
                    self.compile_expr_as(start, ValueType::I64)?;
                    self.emit(Instr::StoreI64(start_var.clone()));
                }

                // Compile and store step value first (default 1 if not specified)
                // so the stop coercion below can read its runtime sign.
                if let Some(step_expr) = step {
                    self.compile_expr_as(step_expr, ValueType::I64)?;
                } else {
                    self.emit(Instr::PushI64(1));
                }
                self.emit(Instr::StoreI64(step_var.clone()));

                // Compile and store stop value, then initialize the loop variable.
                //
                // Issue #9321: an `Any`-inferred bound (`for i in 1:k:n`) may
                // arrive as a `Float` at runtime; coerce it to `Int` with upstream
                // range last-element semantics using the runtime step sign, rather
                // than `compile_expr_as`'s `DynamicToI64` truncation toward zero.
                // A statically integer-typed bound keeps the direct path.
                //
                // Issue #9377: `CoerceRangeStopI64` peeks `start` (bottom) and
                // `step` (middle, left below the bound by `LoadI64(step_var)`)
                // beneath the bound to pick the rounding direction and to
                // distinguish the legal empty direction (`1:-Inf` → length 0)
                // from a counting-direction non-finite / out-of-`Int64` bound,
                // which raises the upstream `InexactError` (`1:Inf`, `1:1e30`).
                // The step copy is popped afterward; the `start` operand is
                // reused to initialize the loop variable so `start` is still
                // evaluated exactly once.
                let end_is_any = matches!(self.infer_expr_type(end), ValueType::Any);
                if end_is_any {
                    if let Some(start_var) = &start_var {
                        self.emit(Instr::LoadI64(start_var.clone()));
                    } else {
                        self.compile_expr_as(start, ValueType::I64)?;
                    }
                    self.emit(Instr::LoadI64(step_var.clone()));
                    self.compile_expr(end)?;
                    self.emit(Instr::CoerceRangeStopI64);
                    self.emit(Instr::StoreI64(stop_var.clone()));
                    self.emit(Instr::Pop);
                } else {
                    self.compile_expr_as(end, ValueType::I64)?;
                    self.emit(Instr::StoreI64(stop_var.clone()));
                }
                if explicit_lexical {
                    self.enter_explicit_lexical_scope(vec![var.clone()]);
                    self.locals.insert(var.clone(), ValueType::I64);
                    self.initialized_locals.insert(var.clone());
                    let Some(start_var) = start_var else {
                        return Err(internal_compile_error(
                            "explicit lexical loop start temp must exist",
                        ));
                    };
                    self.emit(Instr::LoadI64(start_var));
                    self.emit(Instr::StoreI64(var.clone()));
                } else if end_is_any {
                    // The coercion path leaves the single evaluated start
                    // value on the stack after discarding its step copy.
                    self.emit(Instr::StoreI64(var.clone()));
                } else {
                    // Initialize loop variable on the legacy frame-local path.
                    self.compile_expr_as(start, ValueType::I64)?;
                    self.emit(Instr::StoreI64(var.clone()));
                }

                let loop_start = self.here();

                // Push loop context for break/continue
                let mut loop_ctx = LoopContext {
                    exit_patches: Vec::new(),
                    continue_patches: Vec::new(),
                };

                // Check loop condition based on step sign:
                // If step > 0: continue while var <= stop (exit when var > stop)
                // If step < 0: continue while var >= stop (exit when var < stop)
                // We check: (step > 0 && var > stop) || (step < 0 && var < stop)

                // Check if step > 0
                self.emit(Instr::LoadI64(step_var.clone()));
                self.emit(Instr::PushI64(0));
                self.emit(Instr::GtI64);
                let j_positive = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // jump to negative check if step <= 0

                // Step is positive: check var > stop
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(stop_var.clone()));
                self.emit(Instr::GtI64);
                let j_exit_pos = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // continue if var <= stop
                let j_to_exit1 = self.here();
                self.emit(Instr::Jump(usize::MAX)); // exit loop
                loop_ctx.exit_patches.push(j_to_exit1);

                // Step is negative: check var < stop
                let negative_check = self.here();
                self.patch_jump(j_positive, negative_check);
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(stop_var.clone()));
                self.emit(Instr::LtI64);
                let j_exit_neg = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // continue if var >= stop
                let j_to_exit2 = self.here();
                self.emit(Instr::Jump(usize::MAX)); // exit loop
                loop_ctx.exit_patches.push(j_to_exit2);

                let body_start = self.here();
                self.patch_jump(j_exit_pos, body_start);
                self.patch_jump(j_exit_neg, body_start);

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                if explicit_lexical {
                    self.scope_cleanup_stack.push(ScopeCleanupContext {
                        names: Vec::new(),
                        shadows: Vec::new(),
                        lexical_scope_count: 2,
                        loop_depth: self.loop_stack.len(),
                        cleanup_on_loop_exit: false,
                        nonlocal_pop_handler: false,
                        nonlocal_pop_caught_exception: false,
                    });
                }
                self.compile_soft_scope_block(body, std::slice::from_ref(var))?;
                if explicit_lexical {
                    self.scope_cleanup_stack.pop();
                }
                let loop_ctx = self.pop_loop_frame()?;

                let continue_target = self.here();

                // Increment by step
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(step_var.clone()));
                self.emit(Instr::AddI64);
                self.emit(Instr::StoreI64(var.clone()));

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Issue #10984 / #10903: restore a shadowed outer local, if
                // any, at the loop's single normal/break-exit convergence
                // point (`exit`).
                if explicit_lexical {
                    self.exit_explicit_lexical_scope();
                    self.exit_explicit_lexical_scope();
                } else if let Some(shadow) = shadow {
                    self.shadow_local_exit(shadow);
                }
                // Patch all exit jumps (from condition and any break statements)
                for patch_pos in loop_ctx.exit_patches {
                    self.patch_jump(patch_pos, exit);
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                if let Some(metadata) = outer_binding_metadata {
                    self.restore_explicit_scope_binding_metadata(metadata);
                }
                self.widen_outer_lexical_assignments_after_loop(
                    body,
                    std::slice::from_ref(var),
                );

                Ok(())
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // ForEach loop: for var in iterable
                // Strategy:
                // 1. Compile and store iterable
                // 2. Call iterate(collection) to get (element, state) or Nothing
                // 3. If Nothing, exit loop
                // 4. Store element in loop variable, execute body
                // 5. Call iterate(collection, state) to get next (element, state) or Nothing
                // 6. If Nothing, exit; otherwise loop back to step 4
                //
                // For custom iterators (struct types), we use Pure Julia iterate methods.
                // For builtin types (Array, Range, Tuple, String), we use VM instructions.

                // Issue #10984 / #10903: `var` is a fresh binding for this
                // loop's lifetime. Save the outer value/type state now,
                // before either path below overwrites it, and restore it at
                // the loop's single exit convergence point.
                let shadow = if self.explicit_lexical_scopes {
                    None
                } else {
                    Some(self.shadow_local_enter(var)?)
                };

                // Check if we should use Pure Julia iterate (for struct types)
                let iterable_ty = self.infer_julia_type(iterable);
                let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

                // Issue #5168: for the builtin (non pure-Julia) iterate path the
                // VM can produce `(element, state)` split across the stack instead
                // of allocating a `(element, state)` tuple every iteration. The
                // pure-Julia path keeps the tuple-based lowering below because its
                // `iterate` methods return real tuples (and may suspend frames).
                if !use_pure_julia_iterate {
                    return self.compile_foreach_split(var, iterable, body, shadow);
                }

                let outer_binding_metadata = self
                    .explicit_lexical_scopes
                    .then(|| self.snapshot_explicit_scope_binding_metadata());
                // Store the iterable
                let iterable_var = self.new_temp("iterable");
                let state_var = self.new_temp("state");
                let iter_result_var = self.new_temp("iter_result");
                let explicit_lexical = self.explicit_lexical_scopes;
                if explicit_lexical {
                    self.enter_explicit_lexical_scope(vec![
                        iterable_var.clone(),
                        state_var.clone(),
                        iter_result_var.clone(),
                    ]);
                }
                self.compile_expr(iterable)?;
                self.emit(Instr::StoreAny(iterable_var.clone()));
                if explicit_lexical {
                    self.enter_explicit_lexical_scope(vec![var.clone()]);
                }

                // Get first iteration result: iterate(collection)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit_iterate_call_1(&iterable_ty)?;
                // Stack: (element, state) or Nothing
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_exit_first = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Continue if NOT Nothing
                let j_to_exit_first = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Exit if Nothing

                let continue_after_check = self.here();
                self.patch_jump(j_exit_first, continue_after_check);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond); // Get state
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst); // Get element

                let loop_start = self.here();

                // Store element in loop variable. When `iterable` is a user
                // struct implementing the Base `iterate` protocol, recover
                // its element type from `iterate`'s inferred return type so
                // the loop variable gets a concrete slot instead of a fully
                // dynamic one (Issue #9124); anything else keeps the
                // existing `Any` fallback.
                let elem_ty = self
                    .infer_foreach_iterate_element_type(&iterable_ty)
                    .unwrap_or(ValueType::Any);
                self.store_local(var, elem_ty);

                // Push loop context for break/continue
                let loop_ctx = LoopContext {
                    exit_patches: vec![j_to_exit_first],
                    continue_patches: Vec::new(),
                };

                // Compile body with loop context
                let inbounds_array_var = proven_inbounds_loop_array_var(iterable);
                if let Some(array_var) = inbounds_array_var {
                    self.push_proven_inbounds_index(array_var, var);
                }
                self.loop_stack.push(loop_ctx);
                if explicit_lexical {
                    self.scope_cleanup_stack.push(ScopeCleanupContext {
                        names: Vec::new(),
                        shadows: Vec::new(),
                        lexical_scope_count: 2,
                        loop_depth: self.loop_stack.len(),
                        cleanup_on_loop_exit: false,
                        nonlocal_pop_handler: false,
                        nonlocal_pop_caught_exception: false,
                    });
                }
                let body_result =
                    self.compile_soft_scope_block(body, std::slice::from_ref(var));
                if explicit_lexical {
                    self.scope_cleanup_stack.pop();
                }
                let loop_ctx = self.pop_loop_frame()?;
                if inbounds_array_var.is_some() {
                    self.pop_proven_inbounds_index();
                }
                body_result?;

                let continue_target = self.here();

                // Get next iteration result: iterate(collection, state)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit(Instr::LoadAny(state_var.clone()));
                self.emit_iterate_call_2(&iterable_ty)?;
                // Stack: (element, state) or Nothing
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_check_loop = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Continue if NOT Nothing
                let j_to_exit_loop = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Exit if Nothing

                let continue_after_check2 = self.here();
                self.patch_jump(j_check_loop, continue_after_check2);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond); // Get state
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst); // Get element

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Issue #10984 / #10903: restore a shadowed outer local, if
                // any, at the loop's single normal/break-exit convergence
                // point (`exit`).
                if explicit_lexical {
                    self.exit_explicit_lexical_scope();
                    self.exit_explicit_lexical_scope();
                } else if let Some(shadow) = shadow {
                    self.shadow_local_exit(shadow);
                }

                // Patch all exit jumps
                self.patch_jump(j_to_exit_first, exit);
                self.patch_jump(j_to_exit_loop, exit);
                for patch_pos in loop_ctx.exit_patches {
                    if patch_pos != j_to_exit_first {
                        self.patch_jump(patch_pos, exit);
                    }
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                if let Some(metadata) = outer_binding_metadata {
                    self.restore_explicit_scope_binding_metadata(metadata);
                }
                self.widen_outer_lexical_assignments_after_loop(
                    body,
                    std::slice::from_ref(var),
                );

                Ok(())
            }
            Stmt::ForEachTuple {
                vars,
                iterable,
                body,
                ..
            } => {
                // ForEachTuple loop: for (a, b) in iterable
                // Similar to ForEach but destructures each element into multiple vars
                //
                // For custom iterators (struct types), we use Pure Julia iterate methods.
                // For builtin types (Array, Range, Tuple, String), we use VM instructions.

                // Issue #10984 / #10903: each destructured var is a fresh
                // binding for this loop's lifetime. Save any shadowed outer
                // locals now, before the destructure below overwrites them,
                // and restore them at the loop's single exit convergence point.
                let shadows: Vec<ShadowedLocal> = if self.explicit_lexical_scopes {
                    Vec::new()
                } else {
                    vars.iter()
                        .map(|v| self.shadow_local_enter(v))
                        .collect::<CResult<_>>()?
                };

                // Check if we should use Pure Julia iterate (for struct types)
                let iterable_ty = self.infer_julia_type(iterable);
                let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

                let outer_binding_metadata = self
                    .explicit_lexical_scopes
                    .then(|| self.snapshot_explicit_scope_binding_metadata());
                let iterable_var = self.new_temp("iterable");
                let state_var = self.new_temp("state");
                let iter_result_var = self.new_temp("iter_result");
                let elem_var = self.new_temp("elem");
                let explicit_lexical = self.explicit_lexical_scopes;
                if explicit_lexical {
                    self.enter_explicit_lexical_scope(vec![
                        iterable_var.clone(),
                        state_var.clone(),
                        iter_result_var.clone(),
                        elem_var.clone(),
                    ]);
                }
                self.compile_expr(iterable)?;
                self.emit(Instr::StoreAny(iterable_var.clone()));
                if explicit_lexical {
                    self.enter_explicit_lexical_scope(vars.clone());
                }

                // Get first iteration result: iterate(collection)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_1(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateFirst);
                }
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_exit_first = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_to_exit_first = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let continue_after_check = self.here();
                self.patch_jump(j_exit_first, continue_after_check);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond);
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst);
                self.emit(Instr::StoreAny(elem_var.clone()));

                let loop_start = self.here();

                // Destructure element tuple into individual variables
                // Element is already a tuple like (1, 10), extract each component
                for (i, var) in vars.iter().enumerate() {
                    self.emit(Instr::LoadAny(elem_var.clone()));
                    self.emit(Instr::PushI64((i + 1) as i64)); // 1-indexed
                    self.emit(Instr::TupleGet);
                    self.emit(Instr::StoreAny(var.clone()));
                    self.locals.insert(var.clone(), ValueType::Any);
                    // Truthful inside the body (the destructure store above
                    // dominates every body run) — lets a nested same-name
                    // shadowing construct emit its guarded save (Issue
                    // #10984 hardening; exit restores pre-enter membership).
                    self.initialized_locals.insert(var.clone());
                }

                // Push loop context for break/continue
                let loop_ctx = LoopContext {
                    exit_patches: vec![j_to_exit_first],
                    continue_patches: Vec::new(),
                };

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                if explicit_lexical {
                    self.scope_cleanup_stack.push(ScopeCleanupContext {
                        names: Vec::new(),
                        shadows: Vec::new(),
                        lexical_scope_count: 2,
                        loop_depth: self.loop_stack.len(),
                        cleanup_on_loop_exit: false,
                        nonlocal_pop_handler: false,
                        nonlocal_pop_caught_exception: false,
                    });
                }
                self.compile_soft_scope_block(body, vars)?;
                if explicit_lexical {
                    self.scope_cleanup_stack.pop();
                }
                let loop_ctx = self.pop_loop_frame()?;

                let continue_target = self.here();

                // Get next iteration result: iterate(collection, state)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit(Instr::LoadAny(state_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_2(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateNext);
                }
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_check_loop = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_to_exit_loop = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let continue_after_check2 = self.here();
                self.patch_jump(j_check_loop, continue_after_check2);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond);
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst);
                self.emit(Instr::StoreAny(elem_var.clone()));

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Issue #10984 / #10903: restore any shadowed outer locals.
                if explicit_lexical {
                    self.exit_explicit_lexical_scope();
                    self.exit_explicit_lexical_scope();
                } else {
                    for shadow in shadows {
                        self.shadow_local_exit(shadow);
                    }
                }

                // Patch all exit jumps
                self.patch_jump(j_to_exit_first, exit);
                self.patch_jump(j_to_exit_loop, exit);
                for patch_pos in loop_ctx.exit_patches {
                    if patch_pos != j_to_exit_first {
                        self.patch_jump(patch_pos, exit);
                    }
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                if let Some(metadata) = outer_binding_metadata {
                    self.restore_explicit_scope_binding_metadata(metadata);
                }
                self.widen_outer_lexical_assignments_after_loop(body, vars);

                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                let outer_binding_metadata = self
                    .explicit_lexical_scopes
                    .then(|| self.snapshot_explicit_scope_binding_metadata());
                let loop_start = self.here();

                // Push loop context for break/continue
                let mut loop_ctx = LoopContext {
                    exit_patches: Vec::new(),
                    continue_patches: Vec::new(),
                };

                // Compile condition in branch context so `&&` / `||` do not
                // materialize a stack Bool before the loop-exit branch.
                loop_ctx
                    .exit_patches
                    .extend(self.compile_condition_false_jumps(condition)?);

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                let narrow_restore = self.apply_then_narrowings(condition);
                self.compile_soft_scope_block(body, &[])?;
                self.restore_then_narrowings(narrow_restore);
                let loop_ctx = self.pop_loop_frame()?;

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Patch all exit jumps (from condition and any break statements)
                for patch_pos in loop_ctx.exit_patches {
                    self.patch_jump(patch_pos, exit);
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, loop_start);
                }
                if let Some(metadata) = outer_binding_metadata {
                    self.restore_explicit_scope_binding_metadata(metadata);
                }
                self.widen_outer_lexical_assignments_after_loop(body, &[]);
                Ok(())
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Dead code elimination: skip provably dead branches.
                // Fires on a bare Bool literal (Issue #3364) and, via the
                // const-bool folder, on any pure const-foldable condition such
                // as `if 1 < 2` or `if true && false` (Issue #5182).
                if let Some(b) = const_bool_condition_with_lookup(condition, &|name| {
                    self.const_values.get(name).cloned()
                }) {
                    if b {
                        // Condition is always true: only compile then-branch
                        self.compile_block(then_branch)?;
                        if let Some(else_block) = else_branch {
                            self.suppress_where_probes_in_eliminated_block(else_block);
                        }
                    } else {
                        self.suppress_where_probes_in_eliminated_block(then_branch);
                        if let Some(else_block) = else_branch {
                            // Condition is always false: only compile else-branch
                            self.compile_block(else_block)?;
                        }
                    }
                    return Ok(());
                }

                // Compile condition in branch context so `&&` / `||` do not
                // materialize a stack Bool before the else-branch jump.
                let condition_false_jumps = self.compile_condition_false_jumps(condition)?;

                // Flow-sensitive local narrowing for `isa`-guarded then-branch
                // (Issue #5181): refine `self.locals` only while compiling the
                // then-branch, then restore so the else-branch / fall-through is
                // unaffected.
                let narrow_restore = self.apply_then_narrowings(condition);
                self.compile_block(then_branch)?;
                self.restore_then_narrowings(narrow_restore);
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let else_start = self.here();
                for patch_pos in condition_false_jumps {
                    self.patch_jump(patch_pos, else_start);
                }

                if let Some(else_block) = else_branch {
                    let else_restore = self.apply_else_narrowings(condition);
                    self.compile_block(else_block)?;
                    self.restore_then_narrowings(else_restore);
                }

                let end = self.here();
                self.patch_jump(j_end, end);
                Ok(())
            }
            Stmt::Return { value, .. } => {
                // Check if there are pending finally blocks
                if self.finally_stack.is_empty() {
                    // No finally blocks - original behavior
                    if let Some(expr) = value {
                        let ty = self.compile_expr(expr)?;
                        self.emit_scope_cleanup_for_return();
                        self.emit(match ty {
                            ValueType::I64 => Instr::ReturnI64,
                            ValueType::F64 => Instr::ReturnF64,
                            ValueType::Array | ValueType::ArrayOf(_, _) => Instr::ReturnArray,
                            ValueType::Str => Instr::ReturnAny,
                            // Use ReturnAny for Nothing to consume the pushed value (Issue #2072)
                            ValueType::Nothing => Instr::ReturnAny,
                            ValueType::Missing => Instr::ReturnAny,
                            ValueType::Struct(_)
                            | ValueType::ComplexF32
                            | ValueType::ComplexF64 => Instr::ReturnStruct,
                            ValueType::Rng => Instr::ReturnRng,
                            ValueType::Range => Instr::ReturnRange,
                            ValueType::Tuple => Instr::ReturnTuple,
                            ValueType::NamedTuple => Instr::ReturnNamedTuple,
                            ValueType::Dict | ValueType::Set => Instr::ReturnDict,
                            ValueType::Generator => Instr::ReturnAny,
                            ValueType::Char => Instr::ReturnAny,
                            ValueType::Any => Instr::ReturnAny,
                            ValueType::DataType => Instr::ReturnAny,
                            ValueType::Module => Instr::ReturnAny,
                            ValueType::BigInt => Instr::ReturnAny,
                            ValueType::BigFloat => Instr::ReturnAny,
                            ValueType::IO => Instr::ReturnAny,
                            ValueType::Function => Instr::ReturnAny,
                            ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::I128 => {
                                Instr::ReturnAny
                            }
                            ValueType::U8
                            | ValueType::U16
                            | ValueType::U32
                            | ValueType::U64
                            | ValueType::U128 => Instr::ReturnAny,
                            ValueType::F32 => Instr::ReturnF32,
                            ValueType::F16 => Instr::ReturnF16,
                            ValueType::Bool => Instr::ReturnAny,
                            ValueType::Symbol
                            | ValueType::Expr
                            | ValueType::QuoteNode
                            | ValueType::LineNumberNode
                            | ValueType::GlobalRef => Instr::ReturnAny,
                            ValueType::Pairs => Instr::ReturnAny,
                            ValueType::Regex | ValueType::RegexMatch => Instr::ReturnAny,
                            ValueType::Enum => Instr::ReturnAny,
                            ValueType::Union(_) => Instr::ReturnAny,
                            ValueType::Memory | ValueType::MemoryOf(_) => Instr::ReturnAny,
                        });
                    } else {
                        self.emit_scope_cleanup_for_return();
                        self.emit(Instr::ReturnNothing);
                    }
                } else {
                    // Has finally blocks - save return value, execute finally, then return
                    let (saved_temp, saved_ty) = if let Some(expr) = value {
                        let ty = self.compile_expr(expr)?;
                        let temp = self.new_temp("return_val");
                        match ty {
                            ValueType::I64 => self.emit(Instr::StoreI64(temp.clone())),
                            ValueType::F64 => self.emit(Instr::StoreF64(temp.clone())),
                            ValueType::Array | ValueType::ArrayOf(_, _) => {
                                self.emit(Instr::StoreArray(temp.clone()))
                            }
                            ValueType::Tuple => self.emit(Instr::StoreTuple(temp.clone())),
                            ValueType::NamedTuple => {
                                self.emit(Instr::StoreNamedTuple(temp.clone()))
                            }
                            ValueType::Dict | ValueType::Set => {
                                self.emit(Instr::StoreDict(temp.clone()))
                            }
                            ValueType::Range => self.emit(Instr::StoreRange(temp.clone())),
                            ValueType::Rng => self.emit(Instr::StoreRng(temp.clone())),
                            ValueType::Struct(_) => self.emit(Instr::StoreStruct(temp.clone())),
                            _ => self.emit(Instr::StoreAny(temp.clone())),
                        }
                        self.locals.insert(temp.clone(), ty.clone());
                        (Some(temp), ty)
                    } else {
                        (None, ValueType::Nothing)
                    };

                    // Execute all pending finally blocks in reverse order
                    let finally_contexts = self.finally_stack.clone();
                    for context in finally_contexts.iter().rev() {
                        self.compile_pending_finally(context)?;
                    }

                    // A pending finally still executes inside the lexical
                    // owners enclosing the return. Close them only after the
                    // finally code has observed its locals, matching the
                    // break/continue transfer ordering (Issue #11569).
                    self.emit_scope_cleanup_for_return();

                    // Load return value and return
                    if let Some(ref temp) = saved_temp {
                        match saved_ty {
                            ValueType::I64 => self.emit(Instr::LoadI64(temp.clone())),
                            ValueType::F64 => self.emit(Instr::LoadF64(temp.clone())),
                            ValueType::Array | ValueType::ArrayOf(_, _) => {
                                self.emit(Instr::LoadArray(temp.clone()))
                            }
                            ValueType::Tuple => self.emit(Instr::LoadTuple(temp.clone())),
                            ValueType::NamedTuple => self.emit(Instr::LoadNamedTuple(temp.clone())),
                            ValueType::Dict | ValueType::Set => {
                                self.emit(Instr::LoadDict(temp.clone()))
                            }
                            ValueType::Range => self.emit(Instr::LoadRange(temp.clone())),
                            ValueType::Rng => self.emit(Instr::LoadRng(temp.clone())),
                            ValueType::Struct(_) => self.emit(Instr::LoadStruct(temp.clone())),
                            _ => self.emit(Instr::LoadAny(temp.clone())),
                        }
                    }
                    self.emit(match saved_ty {
                        ValueType::I64 => Instr::ReturnI64,
                        ValueType::F64 => Instr::ReturnF64,
                        ValueType::Array | ValueType::ArrayOf(_, _) => Instr::ReturnArray,
                        ValueType::Struct(_) => Instr::ReturnStruct,
                        ValueType::Rng => Instr::ReturnRng,
                        ValueType::Range => Instr::ReturnRange,
                        ValueType::Tuple => Instr::ReturnTuple,
                        ValueType::NamedTuple => Instr::ReturnNamedTuple,
                        ValueType::Dict | ValueType::Set => Instr::ReturnDict,
                        // When saved_temp is Some, a Load pushed a value — use ReturnAny
                        // to consume it. When None, no value on stack — use ReturnNothing.
                        // (Issue #2072)
                        ValueType::Nothing => {
                            if saved_temp.is_some() {
                                Instr::ReturnAny
                            } else {
                                Instr::ReturnNothing
                            }
                        }
                        _ => Instr::ReturnAny,
                    });
                }
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                if let Some(var) = const_declaration_marker(expr) {
                    if !self.strict_undefined_check && self.local_scope_depth == 0 {
                        self.pending_const_bindings.insert(var.to_string());
                    }
                    return Ok(());
                }
                let ty = self.compile_expr(expr)?;
                // Pop unused value by storing to dummy variable
                let dummy = self.new_temp("discard");
                match ty {
                    ValueType::I64 => self.emit(Instr::StoreI64(dummy)),
                    ValueType::F64 => self.emit(Instr::StoreF64(dummy)),
                    ValueType::Array | ValueType::ArrayOf(_, _) => self.emit(Instr::StoreArray(dummy)),
                    ValueType::Str => self.emit(Instr::Pop),
                    // `nothing` is a real stack value when it comes from calls like println().
                    // Discard it in statement context so it cannot sit below pending caller args.
                    ValueType::Nothing => self.emit(Instr::Pop),
                    ValueType::Missing => self.emit(Instr::Pop),
                    ValueType::Struct(_) | ValueType::ComplexF32 | ValueType::ComplexF64 => {
                        self.emit(Instr::Pop)
                    }
                    ValueType::Rng => self.emit(Instr::StoreRng(dummy)),
                    ValueType::Range => self.emit(Instr::StoreRange(dummy)),
                    ValueType::Tuple => self.emit(Instr::StoreTuple(dummy)),
                    ValueType::NamedTuple => self.emit(Instr::StoreNamedTuple(dummy)),
                    ValueType::Dict | ValueType::Set => self.emit(Instr::StoreDict(dummy)),
                    ValueType::Generator => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Char => self.emit(Instr::StoreAny(dummy)),
                    ValueType::DataType => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Module => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Any => self.emit(Instr::StoreAny(dummy)),
                    ValueType::BigInt => self.emit(Instr::StoreAny(dummy)),
                    ValueType::BigFloat => self.emit(Instr::StoreAny(dummy)),
                    ValueType::IO => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Function => self.emit(Instr::StoreAny(dummy)),
                    // Narrow integer types use StoreAny which dispatches to the NarrowInt tag.
                    // at runtime, preserving the exact Value type (e.g. I8(42), U32(99)).
                    ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::I128 => {
                        self.emit(Instr::StoreAny(dummy))
                    }
                    ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128 => self.emit(Instr::StoreAny(dummy)),
                    ValueType::F32 => self.emit(Instr::StoreF32(dummy)),
                    ValueType::F16 => self.emit(Instr::StoreF16(dummy)),
                    ValueType::Bool => self.emit(Instr::StoreBool(dummy)),
                    // Macro system types
                    ValueType::Symbol
                    | ValueType::Expr
                    | ValueType::QuoteNode
                    | ValueType::LineNumberNode
                    | ValueType::GlobalRef => self.emit(Instr::StoreAny(dummy)),
                    // Pairs type (for kwargs...)
                    ValueType::Pairs => self.emit(Instr::StoreAny(dummy)),
                    // Regex types
                    ValueType::Regex | ValueType::RegexMatch => self.emit(Instr::StoreAny(dummy)),
                    // Enum type
                    ValueType::Enum => self.emit(Instr::StoreAny(dummy)),
                    // Union type
                    ValueType::Union(_) => self.emit(Instr::StoreAny(dummy)),
                    // Memory type
                    ValueType::Memory | ValueType::MemoryOf(_) => self.emit(Instr::StoreAny(dummy)),
                }
                Ok(())
            }
            Stmt::Meta { .. } | Stmt::LocalDecl { .. } => Ok(()),
            Stmt::Global { names, .. } => {
                // The declaration itself emits no code; it only records that the
                // named bindings are module-level for this scope. `compile_function_body`
                // already pre-scans for these, but record them here too so any
                // path that compiles statements directly stays consistent
                // (Issues #5548, #5549). At module scope the binding is already
                // global, so recording it would only widen its type to `Any` —
                // skip it there (mirrors the pre-scan guard).
                if self.strict_undefined_check || self.local_scope_depth > 0 {
                    for name in names {
                        self.declared_globals.insert(name.clone());
                    }
                }
                Ok(())
            }
            Stmt::Break { .. } => {
                // Jump to the exit of the innermost loop
                if self.loop_stack.is_empty() {
                    return err("break outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();

                // Execute finally blocks inside the current loop
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|ctx| ctx.loop_depth >= current_loop_depth)
                    .cloned()
                    .collect();
                for context in finally_blocks.iter().rev() {
                    self.compile_pending_finally(context)?;
                }

                // Julia runs `finally` before leaving the lexical scopes that
                // enclose the transfer.  In particular, a finally reached by
                // break may still read the current body-local and induction
                // bindings.  Close only the per-body/nested owners afterward;
                // the loop-lifetime owner converges on the ordinary exit path.
                self.emit_scope_cleanup_for_loop_exit(current_loop_depth);

                let j_exit = self.here();
                self.emit(Instr::Jump(usize::MAX));
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.exit_patches.push(j_exit);
                }
                Ok(())
            }
            Stmt::Continue { .. } => {
                // Jump to the entry of the innermost loop
                if self.loop_stack.is_empty() {
                    return err("continue outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();

                // Execute finally blocks inside the current loop
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|ctx| ctx.loop_depth >= current_loop_depth)
                    .cloned()
                    .collect();
                for context in finally_blocks.iter().rev() {
                    self.compile_pending_finally(context)?;
                }

                // Keep the loop-lifetime owner alive across continue, but
                // discard the current iteration's body/nested owners after
                // their pending finally blocks have observed them.
                self.emit_scope_cleanup_for_loop_exit(current_loop_depth);

                let j_continue = self.here();
                self.emit(Instr::Jump(usize::MAX));
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_patches.push(j_continue);
                }
                Ok(())
            }
            Stmt::Test {
                condition, message, ..
            } => {
                self.compile_expr_as(condition, ValueType::Bool)?;
                let msg = message.clone().unwrap_or_default();
                self.emit(Instr::Test(msg));
                Ok(())
            }
            Stmt::TestSet { name, body, .. } => {
                self.emit(Instr::TestSetBegin(name.clone()));
                if self.explicit_lexical_scopes {
                    let outer_binding_metadata =
                        self.snapshot_explicit_scope_binding_metadata();
                    let outer_lexical_scope_locals = self.lexical_scope_locals.clone();
                    let outer_declared_globals = self.declared_globals.clone();
                    let outer_local_scope_depth = self.local_scope_depth;
                    let inventory =
                        crate::lowering::soft_scope::ScopeBindingInventory::collect(body);
                    let mut owned_names: Vec<String> =
                        inventory.binding_names().cloned().collect();
                    owned_names.sort();
                    owned_names.dedup();

                    self.lexical_scope_locals.extend(owned_names.iter().cloned());
                    self.declared_globals
                        .extend(inventory.globals.iter().cloned());
                    for local in &owned_names {
                        self.declared_globals.remove(local);
                        self.locals.insert(local.clone(), ValueType::Any);
                        self.initialized_locals.remove(local);
                        self.julia_type_locals.remove(local);
                        self.known_any_rank_array_locals.remove(local);
                        self.mixed_type_vars.insert(local.clone());
                    }

                    let entered_lexical = self.enter_explicit_lexical_scope(owned_names);
                    if entered_lexical {
                        self.scope_cleanup_stack.push(ScopeCleanupContext {
                            names: Vec::new(),
                            shadows: Vec::new(),
                            lexical_scope_count: 1,
                            loop_depth: self.loop_stack.len(),
                            cleanup_on_loop_exit: true,
                            nonlocal_pop_handler: false,
                            nonlocal_pop_caught_exception: false,
                        });
                    }
                    self.local_scope_depth += 1;
                    let body_result = self.compile_block(body);
                    self.local_scope_depth = outer_local_scope_depth;
                    if entered_lexical {
                        self.scope_cleanup_stack.pop();
                        self.exit_explicit_lexical_scope();
                    }
                    self.restore_explicit_scope_binding_metadata(outer_binding_metadata);
                    self.lexical_scope_locals = outer_lexical_scope_locals;
                    self.declared_globals = outer_declared_globals;
                    body_result?;
                    self.emit(Instr::TestSetEnd);
                    return Ok(());
                }

                let outer_locals = self.locals.clone();
                let outer_julia_type_locals = self.julia_type_locals.clone();
                let outer_known_any_rank_array_locals = self.known_any_rank_array_locals.clone();
                let outer_mixed_type_vars = self.mixed_type_vars.clone();
                let outer_local_scope_depth = self.local_scope_depth;
                self.local_scope_depth += 1;
                let body_result = self.compile_block(body);
                self.local_scope_depth = outer_local_scope_depth;
                body_result?;
                self.locals = outer_locals;
                self.julia_type_locals = outer_julia_type_locals;
                self.known_any_rank_array_locals = outer_known_any_rank_array_locals;
                self.mixed_type_vars = outer_mixed_type_vars;
                self.emit(Instr::TestSetEnd);
                Ok(())
            }
            Stmt::TestThrows {
                exception_type,
                expr,
                ..
            } => {
                // @test_throws ExceptionType expr
                // Uses try/catch pattern: if exception is thrown, it's a pass; if not, it's a fail
                let catch_start = self.here();
                self.emit(Instr::PushHandler(None, None)); // placeholder, will be patched

                // Set up test_throws state
                self.emit(Instr::TestThrowsBegin(exception_type.clone()));

                // Compile the expression that should throw
                self.compile_expr(expr)?;
                self.emit(Instr::Pop);

                // If we reach here, no exception was thrown - that's a failure
                self.emit(Instr::PopHandler);
                self.emit(Instr::TestThrowsEnd); // Will report failure (no exception)
                let jump_to_end = self.here();
                self.emit(Instr::Jump(usize::MAX)); // placeholder

                // Catch block - exception was thrown
                let catch_ip = self.here();
                self.emit(Instr::ClearError);
                self.emit(Instr::TestThrowsEnd); // Will report success

                // Patch the handler to jump to catch
                self.code[catch_start] = Instr::PushHandler(Some(catch_ip), None);

                // Patch the jump to skip catch block
                let end = self.here();
                self.code[jump_to_end] = Instr::Jump(end);

                Ok(())
            }
            Stmt::Timed { body, .. } => {
                self.emit(Instr::TimeNs);
                self.emit(Instr::StoreI64("__time_start".to_string()));

                self.compile_block(body)?;

                self.emit(Instr::TimeNs);
                self.emit(Instr::LoadI64("__time_start".to_string()));
                self.emit(Instr::SubI64);
                self.emit(Instr::ToF64);
                self.emit(Instr::PushF64(1_000_000_000.0));
                self.emit(Instr::DivF64);
                self.emit(Instr::PushStr("  ".to_string()));
                self.emit(Instr::PrintStrNoNewline);
                self.emit(Instr::PrintF64NoNewline);
                self.emit(Instr::PushStr(" seconds".to_string()));
                self.emit(Instr::PrintStr);
                Ok(())
            }
            Stmt::IndexAssign {
                array,
                indices,
                value,
                span,
            } => {
                // `d[k1, k2, ...] = v` on an AbstractDict is sugar for
                // `d[(k1, k2, ...)] = v`: upstream defines
                // `setindex!(t::AbstractDict, v, k1, k2, ks...) =
                // setindex!(t, v, tuple(k1, k2, ks...))` (abstractdict.jl). Without
                // this, a Dict target with 2+ plain indices falls through to native
                // multi-dim `IndexStore(N)`, which errors on a Dict (Issue #6707,
                // sibling of the getindex fix). Rewrite to a single tuple key and
                // dispatch the ordinary one-key setindex!.
                if indices.len() >= 2
                    && !indices
                        .iter()
                        .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }))
                {
                    let target_ty = if self.declared_globals.contains(array) {
                        Some(ValueType::Any)
                    } else {
                        self.locals.get(array).cloned()
                    };
                    let target_julia = self.infer_julia_type(&Expr::Var(array.clone().into(), *span));
                    let target_is_dict_like = matches!(&target_ty, Some(ValueType::Dict))
                        || matches!(&target_ty, Some(ValueType::Struct(type_id))
                            if self
                                .shared_ctx
                                .type_id_to_struct_name
                                .get(type_id)
                                .is_some_and(|name| is_dict_struct_name(name)))
                        || matches!(target_julia, JuliaType::Dict)
                        || matches!(&target_julia, JuliaType::Struct(name) if is_dict_struct_name(name));
                    if target_is_dict_like {
                        let key = Expr::TupleLiteral {
                            elements: indices.clone(),
                            span: *span,
                        };
                        let new_args =
                            vec![Expr::Var(array.clone().into(), *span), value.clone(), key];
                        let ty = self.compile_call("setindex!", &new_args, &[], &[], &[])?;
                        if matches!(ty, ValueType::Nothing) {
                            self.emit(Instr::Pop);
                        } else {
                            let dummy = self.new_temp("discard");
                            self.emit(Instr::StoreAny(dummy));
                        }
                        return Ok(());
                    }
                }

                // Julia-compliant: arr[i] = v is equivalent to setindex!(arr, v, i)
                // We implement this directly with VM instructions for efficiency,
                // and store the modified collection back to the variable.
                let mut setindex_args = Vec::with_capacity(indices.len() + 2);
                setindex_args.push(Expr::Var(array.clone().into(), *span));
                setindex_args.push(value.clone());
                setindex_args.extend(indices.clone());
                let setindex_arg_types: Vec<JuliaType> = setindex_args
                    .iter()
                    .map(|arg| self.infer_julia_type(arg))
                    .collect();
                let target_ty = if self.declared_globals.contains(array) {
                    Some(ValueType::Any)
                } else {
                    self.locals.get(array).cloned()
                };
                let is_struct_backed_dict_target = match &target_ty {
                    Some(ValueType::Struct(type_id)) => self
                        .shared_ctx
                        .type_id_to_struct_name
                        .get(type_id)
                        .is_some_and(|name| is_dict_struct_name(name)),
                    _ => false,
                };
                // A DataType-valued key can only target a Dict, so route the
                // assignment through the `setindex!` builtin (which dispatches to
                // DictSet) rather than the native array-store path below. The
                // array path coerces a numeric scalar value to F64 for an
                // unboxed-target store, which would corrupt a boxed Dict value
                // (e.g. `d[T] = 1` storing `1.0`); DictSet preserves it
                // (Issue #7940).
                let has_datatype_index = indices
                    .iter()
                    .any(|idx| matches!(self.infer_expr_type(idx), ValueType::DataType));
                if is_struct_backed_dict_target
                    || has_datatype_index
                    || self.has_user_dispatch_method_for_arg_types(
                    &["setindex!", "Base.setindex!"],
                    &setindex_arg_types,
                ) {
                    let ty = self.compile_call("setindex!", &setindex_args, &[], &[], &[])?;
                    if matches!(ty, ValueType::Nothing) {
                        self.emit(Instr::Pop);
                    } else {
                        let dummy = self.new_temp("discard");
                        self.emit(Instr::StoreAny(dummy));
                    }
                    return Ok(());
                }

                // Check if this is a global variable (in global_types but not in locals)
                let is_global = self.declared_globals.contains(array)
                    || (target_ty.is_none() && self.shared_ctx.global_types.contains_key(array));
                match target_ty {
                    Some(ValueType::Dict) => {
                        // Dict assignment: setindex!(d, value, key)
                        if indices.len() != 1 {
                            return err("Dict indexing requires exactly one key");
                        }
                        self.emit(Instr::LoadDict(array.clone()));
                        self.compile_expr(&indices[0])?;
                        self.compile_expr(value)?;
                        self.emit(Instr::DictSet);
                        self.emit(Instr::StoreDict(array.clone()));
                        Ok(())
                    }
                    _ => {
                        // Array/struct assignment: setindex!(collection, value, indices...)
                        // Use typed load so StructRef (e.g., SubArray) is supported by IndexStore.
                        self.load_local(array)?;
                        for idx in indices {
                            // Non-integer keys must reach runtime collection dispatch
                            // instead of being coerced to I64. This covers Dict-like
                            // receivers and array-valued WeakKeyDict keys while preserving
                            // integer array stores on the native path (Issue #1814/#10088).
                            // DataType keys are routed to `setindex!`/DictSet above before
                            // reaching here (Issue #7940).
                            let idx_type = self.infer_expr_type(idx);
                            if matches!(
                                idx_type,
                                ValueType::Any
                                    | ValueType::Array
                                    | ValueType::ArrayOf(_, _)
                                    | ValueType::MemoryOf(_)
                                    | ValueType::Bool
                                    | ValueType::Range
                                    | ValueType::Rng
                                    | ValueType::Struct(_)
                                    | ValueType::Tuple
                                    | ValueType::Str
                                    | ValueType::Symbol
                            ) {
                                self.compile_expr(idx)?;
                            } else {
                                self.compile_expr_as(idx, ValueType::I64)?;
                            }
                        }
                        // Compile value without type coercion to support tuples and other types
                        let val_ty = self.compile_expr(value)?;
                        // Only coerce to F64 if it's a numeric type (not Tuple, Struct, etc.).
                        // A zero-index store (`r[] = v`, `IndexStore(0)`) targets a
                        // `Ref` cell, which stores the value VERBATIM at runtime —
                        // coercing there silently corrupted an `Int64` to `Float64`
                        // whenever the target's static type was unknown, e.g. a
                        // module-scope global `Ref` (Issue #10363).
                        if !indices.is_empty()
                            && !target_preserves_boxed_numeric_values(target_ty.as_ref())
                        {
                            match val_ty {
                                ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                                    self.emit(Instr::ToF64);
                                }
                                _ => {}
                            }
                        }
                        let array_expr = Expr::Var(array.clone().into(), *span);
                        if indices.len() == 1
                            && self.is_proven_inbounds_index(&array_expr, &indices[0])
                        {
                            self.emit(Instr::IndexStoreInbounds(indices.len()));
                        } else {
                            self.emit(Instr::IndexStore(indices.len()));
                        }
                        // For global arrays, don't emit StoreArray because:
                        // 1. Arrays are passed by reference - IndexStore modifies in place
                        // 2. StoreArray would create a local slot, shadowing the global
                        // 3. The slotized LoadSlot would then fail to find the value
                        // Instead, just pop the modified array reference from the stack.
                        if is_global {
                            self.emit(Instr::Pop);
                        } else {
                            self.emit(Instr::StoreArray(array.clone()));
                        }
                        Ok(())
                    }
                }
            }
            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                // Qualified assignment through a lexical/static module binding
                // updates that module's global rather than requiring a struct
                // local slot (`P.x = value`, Issue #11219). Keep the receiver on
                // the ordinary SetFieldByName path so runtime module mutation
                // and qualified lookup share one source of truth.
                let module_path = (!self.explicit_lexical_owner_active(object))
                    .then(|| {
                        self.module_path_in_current_scope(object)
                            .or_else(|| self.resolve_visible_module_path(object))
                            .or_else(|| self.resolved_module_alias(object).map(str::to_string))
                    })
                    .flatten();
                if let Some(module_path) = module_path {
                    let mut exports: Vec<String> = self
                        .module_exports
                        .get(&module_path)
                        .map(|values| values.iter().cloned().collect())
                        .unwrap_or_default();
                    exports.sort();
                    self.emit(Instr::PushModule(Box::new(crate::bytecode::ModuleOperands {
                        name: module_path,
                        exports,
                        publics: vec![],
                        base_exports_visible: !self.current_module_is_bare,
                        implicit_standard_bindings: !self.current_module_is_bare,
                    })));
                    self.compile_expr(value)?;
                    self.emit(Instr::SetFieldByName(field.to_string()));
                    self.emit(Instr::Pop);
                    return Ok(());
                }

                // Get the struct type from the local variable
                let obj_ty =
                    self.locals.get(object).cloned().ok_or_else(|| {
                        CompileError::Msg(format!("Unknown variable: {}", object))
                    })?;

                match obj_ty {
                    ValueType::Struct(type_id) => {
                        // Find the struct info and field index
                        let mut field_idx = None;
                        let mut field_ty = ValueType::F64;
                        let mut struct_name = String::new();

                        if let Some((_, name, struct_info)) =
                            self.shared_ctx.struct_table.resolve_type_id(type_id)
                        {
                            struct_name = name.clone();
                            for (idx, (field_name, fty)) in
                                struct_info.fields.iter().enumerate()
                            {
                                if field_name == field {
                                    field_idx = Some(idx);
                                    field_ty = fty.clone();
                                    break;
                                }
                            }
                        }

                        let idx = match field_idx {
                            Some(idx) => idx,
                            None
                                if is_array_wrapper_struct_name(&struct_name)
                                    && is_array_wrapper_compat_field(field) =>
                            {
                                self.emit(Instr::LoadStruct(object.clone()));
                                self.compile_expr(value)?;
                                self.emit(Instr::SetFieldByName(field.to_string()));
                                self.emit(Instr::StoreStruct(object.clone()));
                                return Ok(());
                            }
                            None => {
                                // Issue #10319: a statically-known-bogus field
                                // being assigned (`m.bogus = v` where the
                                // struct has no field `bogus`) must defer to
                                // the same catchable runtime FieldError
                                // upstream Julia raises, not abort compilation
                                // of the whole program. `Instr::SetFieldByName`
                                // already raises `VmError::FieldError` for
                                // exactly this case on the dynamic
                                // (`ValueType::Any`) path below (Issue #10212);
                                // route the statically-typed receiver through
                                // the identical instruction so both paths
                                // share one error site.
                                self.emit(Instr::LoadStruct(object.clone()));
                                self.compile_expr(value)?;
                                self.emit(Instr::SetFieldByName(field.to_string()));
                                self.emit(Instr::StoreStruct(object.clone()));
                                return Ok(());
                            }
                        };

                        // Load the struct
                        self.emit(Instr::LoadStruct(object.clone()));

                        // Compile the new value
                        self.compile_expr_as(value, field_ty)?;

                        // Set the field
                        self.emit(Instr::SetField(idx));

                        // Store the modified struct back
                        self.emit(Instr::StoreStruct(object.clone()));

                        Ok(())
                    }
                    ValueType::Any => {
                        // The receiver type is not statically known here (e.g. a
                        // generic `where T` parameter, or a value typed `Any`). Upstream
                        // Julia resolves such field assignments at runtime: defining the
                        // method does not require the field to exist on every candidate
                        // struct, and a guarded `isdefined(G, :f) || (G.f = ...)` body is
                        // legal even when no in-scope struct declares `f`. So defer the
                        // field lookup to runtime SetFieldByName, which raises if the
                        // actual value lacks the field. Concrete-struct field validation
                        // is still enforced by the ValueType::Struct arm above, which
                        // rejects an unknown field on a statically-known struct
                        // (Issue #7941, builds on Issue #2748).
                        //
                        // Use SetFieldByName for the runtime field lookup to avoid
                        // non-deterministic compile-time struct_table iteration order.
                        self.emit(Instr::LoadAny(object.clone()));

                        // Compile the new value as Any (runtime will handle type)
                        self.compile_expr(value)?;

                        // Set the field by name at runtime (resolves correct index)
                        self.emit(Instr::SetFieldByName(field.to_string()));

                        // Store the modified struct back
                        self.emit(Instr::StoreAny(object.clone()));

                        Ok(())
                    }
                    _ => err("Field assignment requires a struct variable"),
                }
            }
            Stmt::Try { .. } => {
                err("internal: Try statement reached compile_stmt (should be handled by compile_try_stmt)")
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                if let Expr::TupleLiteral { elements, .. } = value {
                    if elements.len() == targets.len() {
                        for (target, element) in targets.iter().zip(elements) {
                            self.compile_stmt(&Stmt::Assign {
                                var: target.clone(),
                                value: element.clone(),
                                span: element.span(),
                            })?;
                        }
                        return Ok(());
                    }
                    return self.compile_iterated_destructuring_assign(targets, value, false);
                }
                self.compile_nonliteral_destructuring_assign(targets, value, false)
            }
            Stmt::DictAssign {
                dict, key, value, ..
            } => {
                // dict[key] = value
                self.emit(Instr::LoadDict(dict.clone()));
                self.compile_expr(key)?;
                self.compile_expr(value)?;
                self.emit(Instr::DictSet);
                self.emit(Instr::StoreDict(dict.clone()));
                Ok(())
            }
            Stmt::Using { module, span } => {
                self.compile_using_alias_activation(module, *span)
            }
            Stmt::Export { .. } => {
                // Export statements are processed at the module level,
                // not during statement compilation. They're already
                // collected in module.exports.
                Ok(())
            }
            Stmt::FunctionDef { func, .. } => {
                // Function definitions inside blocks (e.g., inside @testset, or nested functions).
                // The function has already been compiled during the initial compilation pass.

                // Issue #10396: a declaration-position `where`-bound naming an
                // undefined identifier must raise UndefVarError when the method
                // definition executes (never at parse: the bound may name a
                // type defined earlier at runtime). Probe each unresolvable
                // bound name through the ordinary variable-read path before
                // the definition takes effect — the declaration sibling of the
                // value-position resolution #10226 added. Issue #10582 extends
                // the same probes to parameter-annotation type names
                // (`f(x::SomeUndefName) = 1` raises UndefVarError upstream).
                self.emit_signature_definition_probes(
                    &func.type_params,
                    &func.params,
                    &func.kwparams,
                    func.span.start,
                    func.span.definition_order,
                );

                // Create a qualified function name for disambiguation when multiple parent
                // functions have nested functions with the same name (Issue #1743).
                // Format: "parent_function#nested_function". A function found directly in
                // a module-body `let`/`@testset` (no enclosing named-function parent, so
                // `current_function_name` is `None`, but `current_module_path` is set —
                // Issue #10236) is registered under its module-qualified name
                // (`"Module.path.func"`), matching how `build_method_tables`/
                // `function_indices` register such a lexically-scoped root (see
                // `module_body_scoped_root_indices`): otherwise this bare lookup would
                // either miss entirely or, worse, hit an unrelated same-named root from
                // another module or from Main.
                let qualified_name = if let Some(parent_name) = &self.current_function_name {
                    format!("{}#{}", parent_name, func.name)
                } else if let Some(module_path) = &self.current_module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    func.name.clone()
                };

                // Check if this is a nested function that needs to capture variables
                // from the enclosing scope (closure).
                // This runs at BOTH function level (strict_undefined_check=true) AND
                // module level (strict_undefined_check=false) to support closures defined
                // at top-level or in @testset blocks (Issue #2358).
                // Include both local variables AND captured variables from ancestor scopes
                // to support 3+ levels of closure nesting (Issue #1744)
                // Issue #8118: a nested function in a mutually-recursive closure
                // group that captures an enclosing local uses the authoritative
                // capture set computed up-front by
                // `prescan_mutual_closure_captures` (enclosing-scope data only,
                // sibling function names excluded, sibling captures propagated in).
                // Recomputing free variables here would re-capture sibling names
                // and miss the transitive propagation, breaking reconstruction.
                let free_vars = if let Some(prescanned) =
                    self.mutual_closure_captures.get(&qualified_name).cloned()
                {
                    prescanned
                } else {
                    // Hard-scope prescans predeclare later assignment targets
                    // in `locals` so reads before assignment remain lexical,
                    // but those names do not yet denote capturable outer
                    // values. Offering them to `analyze_free_variables` makes
                    // a nested function's own same-named local assignment look
                    // like a capture whose value does not exist at closure
                    // creation time (Issue #11249).
                    let mut outer_scope_vars: HashSet<String> = self
                        .locals
                        .keys()
                        .filter(|name| {
                            self.initialized_locals.contains(name.as_str())
                                && self.lexical_scope_locals.contains(name.as_str())
                        })
                        .cloned()
                        .collect();
                    outer_scope_vars.extend(self.captured_vars.iter().cloned());
                    analyze_free_variables(func, &outer_scope_vars)
                };

                // The body of a function defined in a module-level `let` scope was
                // already compiled (compile_functions runs before compile_main)
                // against the capture set computed up-front by
                // `collect_let_scope_function_captures` (Issue #11015). Union it in
                // so every name its body loads with `LoadCaptured` is actually
                // captured here.
                let mut free_vars = free_vars;
                if let Some(prescanned) = self.shared_ctx.closure_captures.get(&qualified_name) {
                    free_vars.extend(prescanned.iter().cloned());
                }

                if !free_vars.is_empty() {
                    // This is a closure - store capture info for when the function is compiled
                    // Use qualified name to avoid collision between nested functions with same name
                    self.shared_ctx
                        .closure_captures
                        .insert(qualified_name.clone(), free_vars.clone());

                    // Emit CreateClosure with the QUALIFIED function name
                    // FunctionInfo.name also uses the qualified name for nested functions,
                    // so the runtime lookup will find the correct function (Issue #1743)
                    let mut capture_names: Vec<String> = free_vars.into_iter().collect();
                    // Free variables are collected in a HashSet. The emitted
                    // order is part of cached bytecode, so make it canonical
                    // across independent prelude/Base cache generators
                    // (Issue #11264).
                    capture_names.sort();
                    let candidate_indices =
                        self.imported_generic_candidate_indices(&qualified_name);
                    self.emit_closure_value(
                        &qualified_name,
                        capture_names,
                        candidate_indices,
                    );
                    // Store the closure in the local scope using the ORIGINAL name
                    // (so the local variable `inner` can be accessed normally in user code)
                    // — unless the definition was declared `global` (`global function
                    // f(...) ... end` inside a `let`), in which case the closure binds
                    // to the module-level name so callers outside the block see it
                    // (Issue #11015).
                    if self.declared_globals.contains(&func.name) {
                        self.emit_store_declared_global(&func.name);
                    } else {
                        self.emit(Instr::StoreAny(func.name.clone()));
                        self.locals.insert(func.name.clone(), ValueType::Any);
                        // The closure value now exists in this frame.  Keep
                        // `initialized_locals` in sync with the emitted store
                        // so a subsequently lifted generator/helper may capture
                        // the local function while still excluding merely
                        // prescanned, not-yet-assigned locals (Issue #11249).
                        self.initialized_locals.insert(func.name.clone());
                    }
                    return Ok(());
                }

                // Regular function definition (not a closure). Function bodies
                // need callable locals for nested functions. At module/testset
                // hard-scope level, keep ordinary methods on the direct-dispatch
                // path; only lowering-generated generator helpers are also bound
                // as local values so `Generator(__gen_body_N, iter)` can pass the
                // helper by value in the same scope.
                let lexical_function = self.explicit_lexical_owner_active(&func.name);
                if self.strict_undefined_check
                    || (self.local_scope_depth > 0
                        && is_lifted_generator_helper_name(&func.name))
                    || lexical_function
                {
                    self.emit_function_value(&qualified_name);
                    self.emit(Instr::StoreAny(func.name.clone()));
                    self.locals.insert(func.name.clone(), ValueType::Function);
                    self.initialized_locals.insert(func.name.clone());
                    if lexical_function {
                        self.function_aliases
                            .insert(func.name.clone(), qualified_name.clone());
                        self.lexical_function_tables
                            .insert(func.name.clone(), qualified_name.clone());
                    }
                }

                // Top-level script definitions activate in source order so runtime
                // dispatch can honor world-age visibility for same-name
                // redefinitions (Issue #9650). Name lookup alone is not enough:
                // `function_indices` points to the latest definition, so prefer
                // the exact definition span. Lowering-generated anonymous
                // callables are values consumed by their containing expression,
                // not Julia-visible generic definitions, so they have no marker.
                let markerless_helper =
                    crate::compile::ir_inline::is_markerless_lowered_function(func);
                if !markerless_helper {
                    if !self.strict_undefined_check && self.current_module_path.is_none() {
                        let indices = self
                            .shared_ctx
                            .function_indices_by_span_start
                            .get(&func.span.start)
                            .cloned()
                            .unwrap_or_default();
                        for idx in indices {
                            self.emit_eval_function_activation_once(idx);
                        }
                    } else if let Some(idx) =
                        self.shared_ctx.function_indices.get(&qualified_name)
                    {
                        // This instruction is a no-op at runtime but marks that the
                        // nested/module function definition was executed.
                        self.emit(Instr::DefineFunction(*idx));
                    }
                }
                // Even if not found, this is OK - the function might be defined
                // elsewhere or be a forward reference.
                Ok(())
            }
            Stmt::EvalFunctionDef { func, .. } => {
                // Issue #10396: same definition-time `where`-bound resolution
                // probe as `Stmt::FunctionDef` — upstream raises UndefVarError
                // for `@eval f(x::T) where T<:Undef = x` too. Issue #10582:
                // parameter-annotation names are probed the same way.
                self.emit_signature_definition_probes(
                    &func.type_params,
                    &func.params,
                    &func.kwparams,
                    func.span.start,
                    func.span.definition_order,
                );
                if let Some(idx) = self.shared_ctx.function_indices.get(&func.name) {
                    self.emit_eval_function_activation_once(*idx);
                }
                Ok(())
            }
            Stmt::Label { name, .. } => {
                // Record the label position for @goto to jump to.
                // The label marks the current instruction position.
                let position = self.here();
                self.label_positions.insert(name.clone(), position);
                Ok(())
            }
            Stmt::Goto { name, span } => {
                // Emit a Jump instruction and record it for patching.
                // We use usize::MAX as a placeholder, which will be patched
                // after all labels are collected.
                let patch_position = self.here();
                self.emit(Instr::Jump(usize::MAX));
                self.goto_patches.push((patch_position, name.clone()));
                // Note: The patch will be applied after compilation by patch_goto_jumps()
                let _ = span; // Span is kept for potential future error reporting
                Ok(())
            }
            Stmt::EnumDef {
                enum_def,
                published_members,
                ..
            } => {
                // @enum runtime integration (Issue #5139).
                //
                // 1. Register the type + members in the thread-local runtime enum
                //    registry so display, `Color(v)` construction, and
                //    `instances(Color)` can recover member names / order.
                // 2. Bind each member name to its `Value::Enum` global, so bare
                //    references (`red`) resolve at runtime instead of raising
                //    UndefVarError.
                let type_name = enum_def.name.clone();
                let members: Vec<(String, i64)> = enum_def
                    .members
                    .iter()
                    .map(|m| (m.name.clone(), m.value))
                    .collect();

                self.emit(Instr::RegisterEnum(Box::new(
                    RegisterEnumOperands {
                        type_name: type_name.clone(),
                        members: members.clone(),
                        published_members: published_members.clone(),
                    },
                )));

                for member_index in julia_enum_member_binding_order(&members) {
                    let (member_name, value) = &members[member_index];
                    if published_members
                        .as_ref()
                        .is_some_and(|published| !published.contains(member_name))
                    {
                        continue;
                    }
                    // Mark the member as an Enum type in global_types so loads
                    // and stores use the dynamic (LoadAny/StoreAny) path.
                    self.shared_ctx
                        .global_types
                        .insert(member_name.clone(), ValueType::Enum);
                    self.emit(Instr::PushEnum {
                        type_name: type_name.clone(),
                        value: *value,
                    });
                    self.emit(Instr::StoreAny(member_name.clone()));
                }
                Ok(())
            }
            Stmt::RuntimeNominalDef {
                definition,
                published_members,
                span,
            } => {
                let binding_name = match definition {
                    RuntimeNominalDef::Struct(definition) => definition.name.as_str(),
                    RuntimeNominalDef::AbstractType(definition) => definition.name.as_str(),
                    RuntimeNominalDef::PrimitiveType(definition) => definition.name.as_str(),
                    RuntimeNominalDef::Enum(definition) => definition.name.as_str(),
                };
                self.shared_ctx
                    .runtime_nominal_callable_names
                    .insert(binding_name.to_string());
                self.shared_ctx
                    .runtime_nominal_callable_names
                    .insert(self.runtime_nominal_declared_name(binding_name));
                if let RuntimeNominalDef::Enum(definition) = definition {
                    let info = EnumInfo {
                        base_type: definition.base_type.clone(),
                        members: definition
                            .members
                            .iter()
                            .map(|member| (member.name.clone(), member.value))
                            .collect(),
                    };
                    self.shared_ctx
                        .enum_types
                        .insert(definition.name.clone(), info.clone());
                    self.shared_ctx.enum_types.insert(
                        self.runtime_nominal_declared_name(&definition.name),
                        info,
                    );
                }
                let definition = self.runtime_nominal_def_info(definition)?;
                let coalesce_with_root = self.runtime_nominal_has_compatible_root(&definition);
                let reserved_struct_type_id = match &definition {
                    RuntimeNominalDefInfo::Struct(definition)
                        if !definition.source.inner_constructors.is_empty() =>
                    {
                        self.shared_ctx
                            .struct_defs
                            .iter()
                            .rposition(|reserved| reserved == &definition.layout)
                    }
                    _ => None,
                };
                let coalesce_with_root = coalesce_with_root || reserved_struct_type_id.is_some();
                let constructor_indices = self
                    .shared_ctx
                    .runtime_nominal_constructor_indices
                    .get(&runtime_nominal_site_id(*span))
                    .cloned()
                    .unwrap_or_default();
                self.emit(Instr::DefineRuntimeNominal(Box::new(
                    DefineRuntimeNominalOperands {
                        site_id: runtime_nominal_site_id(*span),
                        span: *span,
                        definition,
                        coalesce_with_root,
                        reserved_struct_type_id,
                        constructor_function_indices: constructor_indices,
                        published_members: published_members.clone(),
                    },
                )));
                Ok(())
            }
        }
    }

    /// Convert a control-flow-owned nominal declaration into inert bytecode
    /// metadata without registering it as an active compile-time type. The VM
    /// installs this payload only if execution reaches its instruction.
    fn runtime_nominal_def_info(
        &mut self,
        definition: &RuntimeNominalDef,
    ) -> CResult<RuntimeNominalDefInfo> {
        match definition {
            RuntimeNominalDef::Struct(definition) => {
                let qualified_name = self.runtime_nominal_declared_name(&definition.name);
                let type_subst = HashMap::new();
                let mut fields = Vec::with_capacity(definition.fields.len());
                let mut field_julia_types = Vec::with_capacity(definition.fields.len());
                for field in &definition.fields {
                    let value_type = self.shared_ctx.substitute_field_type(
                        &field.type_expr,
                        &type_subst,
                        definition.is_base_origin,
                    )?;
                    let julia_type = field
                        .type_expr
                        .as_ref()
                        .map(|type_expr| {
                            self.shared_ctx
                                .resolve_type_expr_recursive(type_expr, &type_subst)
                        })
                        .transpose()?
                        .as_ref()
                        .map(crate::types::TypeExpr::to_julia_type_lossy)
                        .unwrap_or(JuliaType::Any);
                    fields.push((field.name.clone(), value_type));
                    field_julia_types.push(julia_type);
                }
                Ok(RuntimeNominalDefInfo::Struct(RuntimeStructDefInfo {
                    source: Box::new(crate::ir::core::StructDef {
                        name: qualified_name.clone(),
                        parent_type: definition
                            .parent_type
                            .as_deref()
                            .map(|parent| self.runtime_nominal_type_reference(parent)),
                        ..definition.as_ref().clone()
                    }),
                    layout: StructDefInfo {
                        name: qualified_name,
                        is_mutable: definition.is_mutable,
                        fields,
                        field_julia_types,
                        parent_type: definition
                            .parent_type
                            .as_deref()
                            .map(|parent| self.runtime_nominal_type_reference(parent)),
                    },
                }))
            }
            RuntimeNominalDef::AbstractType(definition) => {
                Ok(RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
                    name: self.runtime_nominal_declared_name(&definition.name),
                    parent: definition
                        .parent
                        .as_deref()
                        .map(|parent| self.runtime_nominal_type_reference(parent)),
                    type_params: definition.type_params.clone(),
                }))
            }
            RuntimeNominalDef::PrimitiveType(definition) => {
                Ok(RuntimeNominalDefInfo::PrimitiveType(PrimitiveTypeDefInfo {
                    name: self.runtime_nominal_declared_name(&definition.name),
                    parent: definition
                        .parent
                        .as_deref()
                        .map(|parent| self.runtime_nominal_type_reference(parent)),
                    bits: definition.bits,
                }))
            }
            RuntimeNominalDef::Enum(definition) => Ok(RuntimeNominalDefInfo::Enum(EnumDefInfo {
                name: self.runtime_nominal_declared_name(&definition.name),
                base_type: definition.base_type.clone(),
                members: definition
                    .members
                    .iter()
                    .map(|member| {
                        (
                            self.runtime_nominal_declared_name(&member.name),
                            member.value,
                        )
                    })
                    .collect(),
            })),
        }
    }

    fn runtime_nominal_has_compatible_root(&self, definition: &RuntimeNominalDefInfo) -> bool {
        let name = match definition {
            RuntimeNominalDefInfo::Struct(definition) => definition.layout.name.as_str(),
            RuntimeNominalDefInfo::AbstractType(definition) => definition.name.as_str(),
            RuntimeNominalDefInfo::PrimitiveType(definition) => definition.name.as_str(),
            RuntimeNominalDefInfo::Enum(definition) => definition.name.as_str(),
        };
        if !self.shared_ctx.type_definition_positions.contains_key(name) {
            return false;
        }

        match definition {
            RuntimeNominalDefInfo::Struct(definition) => self
                .shared_ctx
                .struct_defs
                .iter()
                .any(|root| root == &definition.layout),
            RuntimeNominalDefInfo::AbstractType(definition) => self
                .shared_ctx
                .abstract_types
                .iter()
                .any(|root| root == definition),
            RuntimeNominalDefInfo::PrimitiveType(definition) => self
                .shared_ctx
                .primitive_types
                .iter()
                .any(|root| root == definition),
            RuntimeNominalDefInfo::Enum(definition) => {
                self.shared_ctx.enum_types.get(name).is_some_and(|root| {
                    root.base_type == definition.base_type && root.members == definition.members
                })
            }
        }
    }

    fn runtime_nominal_declared_name(&self, name: &str) -> String {
        if name.contains('.') {
            return name.to_string();
        }
        self.current_module_path
            .as_ref()
            .map_or_else(|| name.to_string(), |module| format!("{module}.{name}"))
    }

    fn runtime_nominal_type_reference(&self, name: &str) -> String {
        if name.contains('.') || JuliaType::from_name(name).is_some() {
            return name.to_string();
        }
        self.resolve_visible_type_object_name(name)
            .or_else(|| self.resolve_visible_type_alias(name))
            .unwrap_or_else(|| self.runtime_nominal_declared_name(name))
    }

    // ==========================================================================
    // Iteration Protocol Helpers
    // ==========================================================================

    /// Issue #5168: lower `for var in coll` for the builtin (non pure-Julia)
    /// iterate path without allocating a `(element, state)` tuple per iteration.
    ///
    /// `IterateFirstSplit` / `IterateNextSplit` push `[state, element]` plus a
    /// `Bool(true)` flag when a value is produced, or just `Bool(false)` when the
    /// collection is exhausted. `JumpIfZero` consumes the flag: on exhaustion it
    /// branches to the loop exit (stack already empty); otherwise `[state, element]`
    /// is left on the stack and the element / state are stored directly. This
    /// avoids the per-iteration tuple heap allocation plus the `TupleFirst` /
    /// `TupleSecond` clones of the prior lowering.
    fn compile_foreach_split(
        &mut self,
        var: &str,
        iterable: &Expr,
        body: &Block,
        shadow: Option<ShadowedLocal>,
    ) -> CResult<()> {
        let outer_binding_metadata = self
            .explicit_lexical_scopes
            .then(|| self.snapshot_explicit_scope_binding_metadata());
        let iterable_var = self.new_temp("iterable");
        let state_var = self.new_temp("state");
        let explicit_lexical = self.explicit_lexical_scopes;
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![iterable_var.clone(), state_var.clone()]);
        }

        // Store the iterable.
        self.compile_expr(iterable)?;
        self.emit(Instr::StoreAny(iterable_var.clone()));
        if explicit_lexical {
            self.enter_explicit_lexical_scope(vec![var.to_string()]);
        }

        // First iteration: iterate(collection).
        self.emit(Instr::LoadAny(iterable_var.clone()));
        self.emit(Instr::IterateFirstSplit);
        // Stack: [state, element, Bool(true)] or [Bool(false)].
        let j_to_exit_first = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX)); // Exit if exhausted (flag false).

        // Value present: stack is [state, element]; element on top.
        self.emit(Instr::StoreAny(var.to_string())); // pop element -> loop var
        self.emit(Instr::StoreAny(state_var.clone())); // pop state -> state slot
        self.locals.insert(var.to_string(), ValueType::Any);
        // Truthful inside the body (the element store above dominates every
        // body run), so a nested same-name shadowing construct emits its
        // guarded save instead of clobbering the live element (Issue #10984
        // hardening; `shadow_local_exit` restores pre-enter membership).
        self.initialized_locals.insert(var.to_string());

        let loop_start = self.here();

        // Push loop context for break/continue.
        let loop_ctx = LoopContext {
            exit_patches: vec![j_to_exit_first],
            continue_patches: Vec::new(),
        };
        let inbounds_array_var = proven_inbounds_loop_array_var(iterable);
        if let Some(array_var) = inbounds_array_var {
            self.push_proven_inbounds_index(array_var, var);
        }
        self.loop_stack.push(loop_ctx);
        if explicit_lexical {
            self.scope_cleanup_stack.push(ScopeCleanupContext {
                names: Vec::new(),
                shadows: Vec::new(),
                lexical_scope_count: 2,
                loop_depth: self.loop_stack.len(),
                cleanup_on_loop_exit: false,
                nonlocal_pop_handler: false,
                nonlocal_pop_caught_exception: false,
            });
        }
        let body_result = self.compile_soft_scope_block(body, &[var.to_string()]);
        if explicit_lexical {
            self.scope_cleanup_stack.pop();
        }
        let loop_ctx = self.pop_loop_frame()?;
        if inbounds_array_var.is_some() {
            self.pop_proven_inbounds_index();
        }
        body_result?;

        let continue_target = self.here();

        // Next iteration: iterate(collection, state).
        self.emit(Instr::LoadAny(iterable_var.clone()));
        self.emit(Instr::LoadAny(state_var.clone()));
        self.emit(Instr::IterateNextSplit);
        // Stack: [state, element, Bool(true)] or [Bool(false)].
        let j_to_exit_loop = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX)); // Exit if exhausted (flag false).

        // Value present: stack is [state, element]; element on top.
        self.emit(Instr::StoreAny(var.to_string())); // pop element -> loop var
        self.emit(Instr::StoreAny(state_var.clone())); // pop state -> state slot
        self.emit(Instr::Jump(loop_start));

        let exit = self.here();
        // Issue #10984 / #10903: restore a shadowed outer local, if any.
        if explicit_lexical {
            self.exit_explicit_lexical_scope();
            self.exit_explicit_lexical_scope();
        } else if let Some(shadow) = shadow {
            self.shadow_local_exit(shadow);
        }

        // Patch exit jumps (first/next exhaustion + any break statements).
        self.patch_jump(j_to_exit_first, exit);
        self.patch_jump(j_to_exit_loop, exit);
        for patch_pos in loop_ctx.exit_patches {
            if patch_pos != j_to_exit_first {
                self.patch_jump(patch_pos, exit);
            }
        }
        for patch_pos in loop_ctx.continue_patches {
            self.patch_jump(patch_pos, continue_target);
        }

        if let Some(metadata) = outer_binding_metadata {
            self.restore_explicit_scope_binding_metadata(metadata);
        }
        self.widen_outer_lexical_assignments_after_loop(body, &[var.to_string()]);

        Ok(())
    }

    /// Check if we should use Pure Julia iterate for this type.
    /// Returns true for struct types (custom iterators), false for builtin types.
    pub(in crate::compile) fn should_use_pure_julia_iterate(&self, ty: &JuliaType) -> bool {
        if let Some(result) = static_iterate_strategy(ty) {
            return result;
        }
        // Dynamic fallback: check if there's an iterate method registered
        if let Some(table) = self.method_tables.get("iterate") {
            !table.methods.is_empty()
        } else {
            false
        }
    }

    /// Emit a call to iterate(collection) - 1 argument version.
    /// Looks up the iterate method from method tables and emits a Call instruction.
    pub(in crate::compile) fn emit_iterate_call_1(&mut self, ty: &JuliaType) -> CResult<()> {
        // A native `Value::Generator` must iterate through the VM's lazy
        // generator protocol (`IterateFirst` -> `start_lazy_generator_iterate_call`),
        // NOT the pure-Julia `iterate(g::Generator)` method — the latter accesses
        // `g.f`, which a collapsed FILTERED generator cannot represent (Issue
        // #9200 S3). `IterateFirst` frame-suspends safely for the predicate call.
        if matches!(ty, JuliaType::Generator) {
            self.emit(Instr::IterateFirst);
            return Ok(());
        }
        if let Some(table) = self.method_tables.get("iterate") {
            let arg_types = vec![ty.clone()];
            if let Ok(method) = table.dispatch(&arg_types) {
                self.emit(Instr::Call(method.global_index, 1));
                return Ok(());
            }
            // Try Any dispatch
            let arg_types_any = vec![JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_any) {
                self.emit(Instr::Call(method.global_index, 1));
                return Ok(());
            }
            // For Any type, use IterateDynamic for runtime struct dispatch
            // This handles cases where the collection is a struct type unknown at compile time
            // (e.g., zip(a, b, c) returns Any, but at runtime it's Zip3)
            if matches!(ty, JuliaType::Any) {
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.param_count() == 1)
                    .filter_map(|m| {
                        let ty = m.projected_param_julia_type(0);
                        Self::is_stmt_runtime_iterate_candidate_type(ty.as_ref())
                            .then_some(m.global_index)
                    })
                    .collect();
                if !candidates.is_empty() {
                    self.emit(Instr::IterateDynamic(1, candidates));
                    return Ok(());
                }
            }
        }
        // Fall back to VM instruction - handles Array, Tuple, String, Range at runtime
        self.emit(Instr::IterateFirst);
        Ok(())
    }

    /// Emit a call to iterate(collection, state) - 2 argument version.
    /// Looks up the iterate method from method tables and emits a Call instruction.
    pub(in crate::compile) fn emit_iterate_call_2(&mut self, ty: &JuliaType) -> CResult<()> {
        // See `emit_iterate_call_1`: a native generator iterates via the VM lazy
        // protocol (`IterateNext`), never the pure-Julia `iterate(g::Generator,
        // state)` method (Issue #9200 S3).
        if matches!(ty, JuliaType::Generator) {
            self.emit(Instr::IterateNext);
            return Ok(());
        }
        if let Some(table) = self.method_tables.get("iterate") {
            // Try to find method with (collection_type, Int64) signature
            let arg_types = vec![ty.clone(), JuliaType::Int64];
            if let Ok(method) = table.dispatch(&arg_types) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // Try with Any as second argument
            let arg_types_any = vec![ty.clone(), JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_any) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // Try with both as Any
            let arg_types_both_any = vec![JuliaType::Any, JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_both_any) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // For Any type, use IterateDynamic for runtime struct dispatch
            if matches!(ty, JuliaType::Any) {
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.param_count() == 2)
                    .filter_map(|m| {
                        let ty = m.projected_param_julia_type(0);
                        Self::is_stmt_runtime_iterate_candidate_type(ty.as_ref())
                            .then_some(m.global_index)
                    })
                    .collect();
                if !candidates.is_empty() {
                    self.emit(Instr::IterateDynamic(2, candidates));
                    return Ok(());
                }
            }
        }
        // Fall back to VM instruction - handles Array, Tuple, String, Range at runtime
        self.emit(Instr::IterateNext);
        Ok(())
    }

    fn is_stmt_runtime_iterate_candidate_type(julia_type: &JuliaType) -> bool {
        matches!(
            julia_type,
            JuliaType::Struct(_)
                | JuliaType::Array
                | JuliaType::VectorOf(_)
                | JuliaType::MatrixOf(_)
                // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue
                // #6721); a bare `::Set` (or `::Dict`) iterate method annotation
                // resolves to the native carrier `JuliaType`, but the value is a
                // `StructRef`, so include it as a runtime IterateDynamic candidate
                // (e.g. `for x in itr` where `itr::Any` binds a Set struct inside
                // `union!`).
                | JuliaType::Set
                | JuliaType::Dict
        )
    }
}

/// Collect names declared `global` anywhere in a single local scope (`block`),
/// recursing into nested control-flow blocks but NOT into nested function
/// definitions, which introduce their own scope. See `compile_function_body`
/// (Issues #5548, #5549).
pub(in crate::compile) fn collect_declared_globals(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_declared_globals_in_stmt(stmt, out);
    }
}

fn collect_declared_globals_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Global { names, .. } => {
            for name in names {
                out.insert(name.clone());
            }
        }
        Stmt::Block(block) => collect_declared_globals(block, out),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_declared_globals(then_branch, out);
            if let Some(block) = else_branch {
                collect_declared_globals(block, out);
            }
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_declared_globals(body, out)
        }
        // Loop bodies and try/catch/else/finally clauses have their own local
        // scope. Their `global` declarations are installed only while compiling
        // that scope and must not reclassify its enclosing continuation.
        Stmt::For { .. }
        | Stmt::ForEach { .. }
        | Stmt::ForEachTuple { .. }
        | Stmt::While { .. }
        | Stmt::Try { .. } => {}
        // Other statements never introduce `global` declarations for this scope.
        // `Stmt::FunctionDef` is intentionally skipped: a nested function is a
        // new local scope with its own declarations.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::ValueType;
    use crate::types::JuliaType;

    // ── static_iterate_strategy ───────────────────────────────────────────────

    #[test]
    fn test_static_iterate_strategy_struct_uses_pure_julia() {
        let ty = JuliaType::Struct("Point".to_string());
        assert_eq!(static_iterate_strategy(&ty), Some(true));
    }

    #[test]
    fn test_static_iterate_strategy_cartesian_indices_uses_builtin() {
        let ty = JuliaType::Struct("CartesianIndices".to_string());
        assert_eq!(
            static_iterate_strategy(&ty),
            Some(false),
            "CartesianIndices is special-cased to use VM builtin iterate"
        );
    }

    #[test]
    fn test_static_iterate_strategy_any_uses_pure_julia() {
        assert_eq!(
            static_iterate_strategy(&JuliaType::Any),
            Some(true),
            "Any uses Pure Julia dispatch for runtime struct resolution"
        );
    }

    #[test]
    fn test_static_iterate_strategy_array_types_use_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::Array), Some(false));
        assert_eq!(
            static_iterate_strategy(&JuliaType::VectorOf(Box::new(JuliaType::Int64))),
            Some(false)
        );
        assert_eq!(
            static_iterate_strategy(&JuliaType::MatrixOf(Box::new(JuliaType::Float64))),
            Some(false)
        );
    }

    #[test]
    fn test_static_iterate_strategy_tuple_types_use_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::Tuple), Some(false));
        assert_eq!(
            static_iterate_strategy(&JuliaType::TupleOf(vec![JuliaType::Int64])),
            Some(false)
        );
    }

    #[test]
    fn test_static_iterate_strategy_string_uses_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::String), Some(false));
    }

    #[test]
    fn test_static_iterate_strategy_int64_uses_builtin() {
        // Range-like types use VM builtin iterate
        assert_eq!(static_iterate_strategy(&JuliaType::Int64), Some(false));
    }

    #[test]
    fn test_static_iterate_strategy_unknown_types_return_none() {
        // These types require runtime method-table lookup
        assert_eq!(static_iterate_strategy(&JuliaType::Bool), None);
        assert_eq!(static_iterate_strategy(&JuliaType::Float64), None);
        assert_eq!(static_iterate_strategy(&JuliaType::Dict), None);
    }

    // ── const_bool_condition (Issue #5182) ────────────────────────────────────

    fn sp() -> crate::span::Span {
        crate::span::Span::new(0, 0, 0, 0, 0, 0)
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), sp())
    }

    fn lit_bool(v: bool) -> Expr {
        Expr::Literal(Literal::Bool(v), sp())
    }

    fn binop(op: crate::ir::core::BinaryOp, left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            span: sp(),
        }
    }

    #[test]
    fn test_const_bool_condition_bare_bool_literal() {
        // The trivial Issue #3364 case must still fold.
        assert_eq!(const_bool_condition(&lit_bool(true)), Some(true));
        assert_eq!(const_bool_condition(&lit_bool(false)), Some(false));
    }

    #[test]
    fn test_const_bool_condition_comparisons_are_not_folded() {
        // Comparison/equality operators dispatch to user-overridable methods
        // (Issue #4298), so they must NOT be folded for dead-branch elimination
        // even on constant operands — otherwise `if "a" == "a"` with a user
        // `==(::String,::String)=false` would be eliminated to the wrong branch.
        use crate::ir::core::BinaryOp;
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Lt, lit_int(1), lit_int(2))),
            None
        );
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Gt, lit_int(1), lit_int(2))),
            None
        );
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Eq, lit_int(3), lit_int(3))),
            None
        );
    }

    #[test]
    fn test_const_bool_condition_boolean_algebra() {
        // `true && false` -> false, `false || true` -> true.
        use crate::ir::core::BinaryOp;
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::And, lit_bool(true), lit_bool(false))),
            Some(false)
        );
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Or, lit_bool(false), lit_bool(true))),
            Some(true)
        );
    }

    #[test]
    fn test_const_bool_condition_unary_not() {
        // `!false` -> true (dispatch-free). `!(1 == 2)` wraps a comparison, which
        // is NOT dispatch-free (Issue #4298), so it must NOT fold -> None.
        use crate::ir::core::{BinaryOp, UnaryOp};
        let not_false = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(lit_bool(false)),
            span: sp(),
        };
        assert_eq!(const_bool_condition(&not_false), Some(true));

        let not_eq = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(binop(BinaryOp::Eq, lit_int(1), lit_int(2))),
            span: sp(),
        };
        assert_eq!(const_bool_condition(&not_eq), None);
    }

    #[test]
    fn test_const_bool_condition_nested_expression() {
        // `(1 + 1) < 3 && 2 * 2 == 4` contains comparison operators, which are
        // dispatch-bearing (Issue #4298) — the whole condition must NOT fold even
        // though its operands are constant. Returns None (no dead-branch elim).
        use crate::ir::core::BinaryOp;
        let lhs = binop(
            BinaryOp::Lt,
            binop(BinaryOp::Add, lit_int(1), lit_int(1)),
            lit_int(3),
        );
        let rhs = binop(
            BinaryOp::Eq,
            binop(BinaryOp::Mul, lit_int(2), lit_int(2)),
            lit_int(4),
        );
        assert_eq!(const_bool_condition(&binop(BinaryOp::And, lhs, rhs)), None);
    }

    #[test]
    fn test_const_bool_condition_non_bool_result_is_none() {
        // A const expression that folds to an Int (not Bool) is not a usable
        // condition for branch elimination — must return None.
        use crate::ir::core::BinaryOp;
        assert_eq!(
            const_bool_condition(&binop(BinaryOp::Add, lit_int(1), lit_int(2))),
            None
        );
    }

    #[test]
    fn test_const_bool_condition_variable_is_none() {
        // A runtime variable cannot be folded — DCE must not fire.
        let var = Expr::Var("x".to_string().into(), sp());
        assert_eq!(const_bool_condition(&var), None);
    }

    #[test]
    fn test_const_bool_condition_call_is_none() {
        // An impure / unknown call must never fold (side effects, runtime value).
        let call = Expr::Call {
            function: "f".to_string().into(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: sp(),
        };
        assert_eq!(const_bool_condition(&call), None);
    }

    // ── can_convert_type ──────────────────────────────────────────────────────

    #[test]
    fn test_can_convert_i64_to_f64() {
        assert!(
            can_convert_type(ValueType::I64, ValueType::F64),
            "I64 → F64 conversion should be supported"
        );
    }

    #[test]
    fn test_can_convert_f64_to_i64() {
        assert!(
            can_convert_type(ValueType::F64, ValueType::I64),
            "F64 → I64 conversion should be supported"
        );
    }

    #[test]
    fn test_cannot_convert_same_type() {
        assert!(
            !can_convert_type(ValueType::I64, ValueType::I64),
            "I64 → I64 is not a conversion (same type)"
        );
        assert!(
            !can_convert_type(ValueType::F64, ValueType::F64),
            "F64 → F64 is not a conversion (same type)"
        );
    }

    #[test]
    fn test_cannot_convert_unrelated_types() {
        assert!(
            !can_convert_type(ValueType::Bool, ValueType::I64),
            "Bool → I64 is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::Str, ValueType::Any),
            "Str → Any is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::I64, ValueType::Bool),
            "I64 → Bool is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::F32, ValueType::F64),
            "F32 → F64 is not a direct VM conversion (no dedicated instruction)"
        );
    }

    #[test]
    fn test_cannot_convert_any_to_concrete() {
        assert!(
            !can_convert_type(ValueType::Any, ValueType::I64),
            "Any → I64 is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::Any, ValueType::F64),
            "Any → F64 is not a direct VM conversion"
        );
    }

    #[test]
    fn test_any_return_can_use_declared_primitive_return_opcode() {
        assert!(should_return_as_expected_type(
            &ValueType::Any,
            &ValueType::I64
        ));
        assert!(should_return_as_expected_type(
            &ValueType::Any,
            &ValueType::F64
        ));
        assert!(!should_return_as_expected_type(
            &ValueType::Any,
            &ValueType::Str
        ));
    }
}
