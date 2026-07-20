//! Math constants and helper functions for the compiler.
//!
//! This module provides utilities for handling Julia math constants
//! like π, ℯ, and other special values.

use crate::ir::core::{Block, Expr, Function, Stmt};
use crate::types::JuliaType;

/// Check if name is π (pi)
pub(super) fn is_pi_name(name: &str) -> bool {
    matches!(name, "pi" | "\u{03C0}")
}

/// Check if name is Euler's number ℯ
pub(super) fn is_euler_name(name: &str) -> bool {
    // ℯ is U+212F (SCRIPT SMALL E), Julia's Euler constant
    matches!(name, "\u{212F}")
}

/// Check if a name is a MathConstants constant and return its value
pub(super) fn get_math_constant_value(name: &str) -> Option<f64> {
    match name {
        // π and pi (π is U+03C0)
        "π" | "pi" => Some(std::f64::consts::PI),
        // ℯ and e (Euler's number, ℯ is U+212F)
        "ℯ" | "e" => Some(std::f64::consts::E),
        // φ and golden (golden ratio, φ is U+03C6)
        "φ" | "golden" => Some((1.0 + 5.0_f64.sqrt()) / 2.0),
        // γ and eulergamma (Euler-Mascheroni constant, γ is U+03B3)
        "γ" | "eulergamma" => Some(0.5772156649015329),
        // Catalan's constant
        "catalan" => Some(0.915_965_594_177_219),
        // IEEE 754 special values
        "NaN" => Some(f64::NAN),
        "Inf" => Some(f64::INFINITY),
        _ => None,
    }
}

/// Check if a name is a MathConstants export
pub(super) fn is_math_constant(name: &str) -> bool {
    get_math_constant_value(name).is_some()
}

/// Get the value of a constant exported from Base module.
/// Only a subset of MathConstants are exported from Base:
/// - pi, π (pi)
/// - ℯ (Euler's number, U+212F - but NOT ascii 'e')
/// - Inf, NaN
pub(super) fn get_base_exported_constant_value(name: &str) -> Option<f64> {
    match name {
        "π" | "pi" => Some(std::f64::consts::PI),
        "ℯ" => Some(std::f64::consts::E), // Only Unicode ℯ, NOT ascii 'e'
        "NaN" => Some(f64::NAN),
        "Inf" => Some(f64::INFINITY),
        _ => None,
    }
}

/// Check if a function needs Lazy AoT specialization.
/// A function needs specialization if:
/// 1. It has parameters without type annotations, AND
/// 2. It's not just an intrinsic wrapper (Core.Intrinsics.xxx call)
pub fn needs_specialization(func: &Function) -> bool {
    if is_generated_function(func) {
        return false;
    }

    // Must have untyped parameters to benefit from Lazy-AoT specialization at
    // runtime call sites. NOTE: this gates emission of `CallSpecialize`, which
    // bypasses runtime multiple dispatch by binding the call to a single method.
    // It must therefore stay restricted to fully untyped params — extending it
    // to `where`-bound TypeVar params would route multi-method generics (e.g.
    // `promote_rule`) through a single specialized method and break dispatch.
    // For reflection-time inference registration, see `needs_reflection_registration`.
    let has_untyped_params = func.params.iter().any(|p| p.type_annotation.is_none());
    if !has_untyped_params {
        return false;
    }

    // Exclude functions that are just Core.Intrinsics wrappers
    // These are already optimal and don't benefit from specialization
    if is_intrinsic_wrapper(func) {
        return false;
    }

    true
}

pub(crate) fn is_generated_function(func: &Function) -> bool {
    for stmt in &func.body.stmts {
        let Stmt::Meta { annotation, .. } = stmt else {
            return false;
        };
        if annotation.name == "generated" {
            return true;
        }
    }
    false
}

/// Whether a function should be registered in `specializable_functions` so that
/// reflection-time return-type inference can find and re-run its body with the
/// concrete argument types (Issue #5003).
///
/// This is a superset of [`needs_specialization`]: in addition to functions with
/// untyped params, it also covers `where`/value-parametrized methods whose params
/// are annotated with a type variable (`x::T`, `xs::Vector{T}`, `::Type{T}`, ...).
/// Such methods have an "open" concrete type at definition time, so the snapshot
/// return type collapses to `Any`; registering them lets reflection substitute
/// the actual argument type for the type variable. Unlike `needs_specialization`,
/// this does **not** drive `CallSpecialize` emission, so runtime multiple dispatch
/// is preserved.
///
/// `is_user_defined` is `true` for module/user-authored functions — the exact
/// population `compile::effects::propagation::infer_program_effects` already
/// walks unconditionally (`program.functions[base_function_count..]`) — and
/// `false` for Base/Core functions. It is only consulted for the fully-typed,
/// non-generic fallthrough case below (Issues #10145 / #10264): without that
/// branch, a user method whose every parameter has a concrete, non-typevar
/// annotation (e.g. `f(x::Bool)`) is never retained in
/// `specializable_functions`, so `Base.infer_effects` / `infer_exception_type`
/// / return-type reflection composition (`compose_function_effects` et al.)
/// silently fall back to the optimistic all-true `_effects_total()` default —
/// regardless of whether the method's side effects sit at the top level or
/// inside `if`/`elseif`/ternary/`&&`/`||`/loop bodies. The body walker itself
/// (`compute_stmt_effects` / `infer_expr_effects_with_callees`) already joins
/// every control-flow arm correctly (verified for `If`/`While`/`For`/`Try`,
/// `Expr::Ternary`, and `BinaryOp::And`/`Or` short-circuit); the gap was
/// registration eligibility, not control-flow tracking. Base/Core functions
/// keep the narrow gate: they are primarily classified through the curated
/// `_classify_effects` name table (`reflection.jl`), and unconditionally
/// registering the ~5k-function Base corpus here would grow
/// `specializable_functions` (and its embedded precompiled-cache footprint)
/// for no reflection benefit.
///
/// `is_generated_function` is checked unconditionally, up front, for every
/// branch below — not just the new `is_user_defined` fallthrough. A
/// `@generated` method's declared body is a *staged-code generator*, not the
/// method's real per-call semantic body (upstream Julia and sjulia both
/// evaluate it specially, see `BuiltinOp::GeneratedEval`); retaining it in
/// `specializable_functions` would let reflection walk the wrong body for
/// *any* of the pre-existing registration reasons below (a `@generated`
/// `Base.f(...)` extension with a direct `throw`, a `@generated` method with a
/// `Complex`-annotated param, or a `@generated` `where`-bound method) — not
/// only the new fallthrough case (adversarial review, Issues #10145 / #10264).
pub fn needs_reflection_registration(func: &Function, is_user_defined: bool) -> bool {
    if is_generated_function(func) {
        return false;
    }
    if needs_specialization(func) {
        return true;
    }
    if func.is_base_extension && block_contains_direct_throw(&func.body) {
        return true;
    }
    if is_intrinsic_wrapper(func) {
        return false;
    }
    if func
        .params
        .iter()
        .filter_map(|param| param.type_annotation.as_ref())
        .any(param_annotation_is_runtime_open)
    {
        return true;
    }
    let where_params: Vec<&str> = func
        .type_params
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    if func
        .params
        .iter()
        .any(|param| param_annotation_uses_typevar(param, &where_params))
    {
        return true;
    }
    // Issues #10145 / #10264: register the remaining fully-typed user method
    // too (see doc comment above).
    is_user_defined
}

fn param_annotation_is_runtime_open(ty: &JuliaType) -> bool {
    // Bare `Complex` is represented as a concrete struct annotation, but without
    // a concrete element type its slot stays `Any`. Register only the Complex
    // family here; broad user-struct registration changes fixture harness method
    // selection for packages that intentionally rely on generic struct methods.
    matches!(ty, JuliaType::Struct(name) if is_complex_annotation_name(name))
}

fn is_complex_annotation_name(name: &str) -> bool {
    let unqualified = name.rsplit_once('.').map_or(name, |(_, tail)| tail);
    let base = unqualified
        .split_once('{')
        .map_or(unqualified, |(base, _)| base);
    base == "Complex"
}

fn block_contains_direct_throw(block: &Block) -> bool {
    block.stmts.iter().any(stmt_contains_direct_throw)
}

fn stmt_contains_direct_throw(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => block_contains_direct_throw(block),
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::Expr { expr: value, .. }
        | Stmt::Test {
            condition: value, ..
        }
        | Stmt::IndexAssign { value, .. }
        | Stmt::FieldAssign { value, .. }
        | Stmt::DestructuringAssign { value, .. } => expr_contains_direct_throw(value),
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            expr_contains_direct_throw(start)
                || expr_contains_direct_throw(end)
                || step.as_ref().is_some_and(expr_contains_direct_throw)
                || block_contains_direct_throw(body)
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            expr_contains_direct_throw(iterable) || block_contains_direct_throw(body)
        }
        Stmt::While {
            condition, body, ..
        } => expr_contains_direct_throw(condition) || block_contains_direct_throw(body),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_contains_direct_throw(condition)
                || block_contains_direct_throw(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(block_contains_direct_throw)
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_contains_direct_throw(try_block)
                || catch_block
                    .as_ref()
                    .is_some_and(block_contains_direct_throw)
                || else_block.as_ref().is_some_and(block_contains_direct_throw)
                || finally_block
                    .as_ref()
                    .is_some_and(block_contains_direct_throw)
        }
        Stmt::TestThrows { expr, .. } => expr_contains_direct_throw(expr),
        Stmt::DictAssign { key, value, .. } => {
            expr_contains_direct_throw(key) || expr_contains_direct_throw(value)
        }
        Stmt::FunctionDef { .. }
        | Stmt::EvalFunctionDef { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. }
        | Stmt::Global { .. }
        | Stmt::Return { value: None, .. } => false,
    }
}

fn expr_contains_direct_throw(expr: &Expr) -> bool {
    match expr {
        Expr::Call { function, args, .. } => {
            matches!(function.as_str(), "error" | "throw" | "rethrow")
                || args.iter().any(expr_contains_direct_throw)
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            ..
        } => {
            (module == "Base" && matches!(function.as_str(), "error" | "throw" | "rethrow"))
                || args.iter().any(expr_contains_direct_throw)
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_contains_direct_throw(value))
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::Pair {
            key: left,
            value: right,
            ..
        } => expr_contains_direct_throw(left) || expr_contains_direct_throw(right),
        Expr::UnaryOp { operand, .. }
        | Expr::Convert { operand, .. }
        | Expr::QuoteLiteral {
            constructor: operand,
            ..
        }
        | Expr::AssignExpr { value: operand, .. } => expr_contains_direct_throw(operand),
        Expr::ReturnExpr {
            value: Some(value), ..
        } => expr_contains_direct_throw(value),
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            elements.iter().any(expr_contains_direct_throw)
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            args.iter().any(expr_contains_direct_throw)
        }
        Expr::Index { array, indices, .. } => {
            expr_contains_direct_throw(array) || indices.iter().any(expr_contains_direct_throw)
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_contains_direct_throw(start)
                || step
                    .as_ref()
                    .is_some_and(|expr| expr_contains_direct_throw(expr))
                || expr_contains_direct_throw(stop)
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_contains_direct_throw(body)
                || expr_contains_direct_throw(iter)
                || filter
                    .as_ref()
                    .is_some_and(|expr| expr_contains_direct_throw(expr))
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_contains_direct_throw(body)
                || iterations
                    .iter()
                    .any(|(_, iter)| expr_contains_direct_throw(iter))
                || filter
                    .as_ref()
                    .is_some_and(|expr| expr_contains_direct_throw(expr))
        }
        Expr::FieldAccess { object, .. } => expr_contains_direct_throw(object),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_contains_direct_throw(value)),
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
            expr_contains_direct_throw(key) || expr_contains_direct_throw(value)
        }),
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .any(|(_, value)| expr_contains_direct_throw(value))
                || block_contains_direct_throw(body)
        }
        Expr::StringConcat { parts, .. } => parts.iter().any(expr_contains_direct_throw),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_direct_throw(condition)
                || expr_contains_direct_throw(then_expr)
                || expr_contains_direct_throw(else_expr)
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_ref()
                .is_some_and(|expr| expr_contains_direct_throw(expr))
                || type_args.iter().any(expr_contains_direct_throw)
        }
        Expr::Literal(_, _)
        | Expr::Var(_, _)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::ReturnExpr { value: None, .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => false,
    }
}

/// Returns true if the parameter's type annotation references a `where`-bound
/// type variable, e.g. `x::T`, `xs::Vector{T}`, or `::Type{T}` (Issue #5003), or
/// hides one inside a string-spelled parametric type such as `Tuple{Vararg{T,N}}`
/// → `Struct("NTuple{N, T}")` (Issue #4843).
fn param_annotation_uses_typevar(
    param: &crate::ir::core::TypedParam,
    where_params: &[&str],
) -> bool {
    param
        .type_annotation
        .as_ref()
        .is_some_and(|ty| julia_type_contains_typevar(ty, where_params))
}

/// Recursively checks whether a `JuliaType` mentions a `TypeVar`, including
/// `where` parameters spelled inside a `Struct` type-name string (Issue #4843).
fn julia_type_contains_typevar(ty: &crate::types::JuliaType, where_params: &[&str]) -> bool {
    use crate::types::JuliaType;
    match ty {
        JuliaType::TypeVar(_, _) => true,
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            julia_type_contains_typevar(inner, where_params)
        }
        JuliaType::TupleOf(items) | JuliaType::Union(items) => items
            .iter()
            .any(|item| julia_type_contains_typevar(item, where_params)),
        JuliaType::Struct(name) if !where_params.is_empty() => name
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .filter(|token| !token.is_empty())
            .any(|token| where_params.contains(&token)),
        _ => false,
    }
}

/// Check if a function is just a thin wrapper around Core.Intrinsics.
/// These functions don't benefit from specialization.
fn is_intrinsic_wrapper(func: &Function) -> bool {
    use crate::ir::core::{Expr, Stmt};

    // A function is an intrinsic wrapper if:
    // 1. Body has exactly one statement that is a return of a call
    // 2. The call is to Core.Intrinsics.xxx (ModuleCall to Core)

    if func.body.stmts.len() != 1 {
        return false;
    }

    // Check if the statement is a ModuleCall to Core.Intrinsics
    match &func.body.stmts[0] {
        Stmt::Expr { expr, .. }
        | Stmt::Return {
            value: Some(expr), ..
        } => {
            matches!(expr, Expr::ModuleCall { module, .. } if module == "Core" || module.starts_with("Core."))
        }
        _ => false,
    }
}

/// Check if a module name is part of Julia's standard library.
///
/// `pub(crate)` so the vm layer's `util::is_root_module_name` drift-guard test
/// (`check_module_scope_root_list_sync`) can assert its mirror copy stays in
/// sync (Issue #10318). The vm production path uses its own copy to preserve the
/// vm→compile layering separation.
pub(crate) fn is_stdlib_module(name: &str) -> bool {
    crate::module_names::is_root_module_name(name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Function, MetaAnnotation, Stmt, TypedParam};
    use crate::span::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    // === is_pi_name ===

    #[test]
    fn test_is_pi_name_ascii() {
        assert!(is_pi_name("pi"));
    }

    #[test]
    fn test_is_pi_name_unicode() {
        assert!(is_pi_name("\u{03C0}")); // π
    }

    #[test]
    fn test_is_pi_name_rejects_other() {
        assert!(!is_pi_name("PI"));
        assert!(!is_pi_name("Pi"));
        assert!(!is_pi_name("e"));
    }

    // === is_euler_name ===

    #[test]
    fn test_is_euler_name_unicode() {
        assert!(is_euler_name("\u{212F}")); // ℯ
    }

    #[test]
    fn test_is_euler_name_rejects_ascii_e() {
        assert!(!is_euler_name("e"));
        assert!(!is_euler_name("E"));
    }

    // === get_math_constant_value ===

    #[test]
    fn test_math_constant_pi() {
        let val = get_math_constant_value("pi");
        assert_eq!(val, Some(std::f64::consts::PI));
        assert_eq!(
            get_math_constant_value("\u{03C0}"),
            Some(std::f64::consts::PI)
        );
    }

    #[test]
    fn test_math_constant_e() {
        assert_eq!(get_math_constant_value("e"), Some(std::f64::consts::E));
        assert_eq!(
            get_math_constant_value("\u{212F}"),
            Some(std::f64::consts::E)
        );
    }

    #[test]
    fn test_math_constant_golden_ratio() {
        let expected = (1.0 + 5.0_f64.sqrt()) / 2.0;
        assert_eq!(get_math_constant_value("\u{03C6}"), Some(expected));
        assert_eq!(get_math_constant_value("golden"), Some(expected));
    }

    #[test]
    fn test_math_constant_nan() {
        let val = get_math_constant_value("NaN");
        assert!(val.is_some());
        assert!(val.unwrap().is_nan()); // NaN != NaN, so use is_nan()
    }

    #[test]
    fn test_math_constant_inf() {
        assert_eq!(get_math_constant_value("Inf"), Some(f64::INFINITY));
    }

    #[test]
    fn test_math_constant_unknown() {
        assert_eq!(get_math_constant_value("tau"), None);
        assert_eq!(get_math_constant_value(""), None);
    }

    // === is_math_constant ===

    #[test]
    fn test_is_math_constant_known() {
        assert!(is_math_constant("pi"));
        assert!(is_math_constant("NaN"));
        assert!(is_math_constant("catalan"));
    }

    #[test]
    fn test_is_math_constant_unknown() {
        assert!(!is_math_constant("tau"));
        assert!(!is_math_constant("x"));
    }

    // === get_base_exported_constant_value ===

    #[test]
    fn test_base_exported_pi() {
        assert_eq!(
            get_base_exported_constant_value("pi"),
            Some(std::f64::consts::PI)
        );
    }

    #[test]
    fn test_base_exported_euler_unicode_only() {
        // Unicode ℯ IS exported from Base
        assert_eq!(
            get_base_exported_constant_value("\u{212F}"),
            Some(std::f64::consts::E)
        );
        // ASCII 'e' is NOT exported from Base
        assert_eq!(get_base_exported_constant_value("e"), None);
    }

    #[test]
    fn test_base_exported_golden_not_exported() {
        // golden/φ are in MathConstants but NOT exported from Base
        assert_eq!(get_base_exported_constant_value("golden"), None);
        assert_eq!(get_base_exported_constant_value("\u{03C6}"), None);
    }

    // === is_stdlib_module ===

    #[test]
    fn test_is_stdlib_module_core_modules() {
        assert!(is_stdlib_module("Base"));
        assert!(is_stdlib_module("Core"));
        assert!(is_stdlib_module("Main"));
        assert!(is_stdlib_module("Sys"));
    }

    #[test]
    fn test_is_stdlib_module_standard_libraries() {
        assert!(is_stdlib_module("LinearAlgebra"));
        assert!(is_stdlib_module("Statistics"));
        assert!(is_stdlib_module("Random"));
        assert!(is_stdlib_module("Printf"));
        assert!(is_stdlib_module("Test"));
    }

    #[test]
    fn test_is_stdlib_module_rejects_unknown() {
        assert!(!is_stdlib_module("MyModule"));
        assert!(!is_stdlib_module("base")); // case-sensitive
        assert!(!is_stdlib_module(""));
    }

    // === needs_specialization ===

    fn make_func(name: &str, params: Vec<TypedParam>, body: Vec<Stmt>) -> Function {
        Function {
            name: name.to_string(),
            params,
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: body,
                span: span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: span(),
            new_struct_name: None,
        }
    }

    #[test]
    fn test_needs_specialization_untyped_params() {
        // f(x) = x + 1 → needs specialization (untyped param, not intrinsic)
        let func = make_func(
            "f",
            vec![TypedParam::untyped("x".to_string(), span())],
            vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), span())),
                span: span(),
            }],
        );
        assert!(needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_all_typed_params() {
        // f(x::Int64) → does NOT need specialization (all params typed)
        let func = make_func(
            "f",
            vec![TypedParam {
                name: "x".to_string(),
                type_annotation: Some(crate::types::JuliaType::Int64),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), span())),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_no_params() {
        // f() → does NOT need specialization (no untyped params)
        let func = make_func("f", vec![], vec![]);
        assert!(!needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_intrinsic_wrapper() {
        // f(x) = Core.Intrinsics.neg_int(x) → does NOT need specialization
        let func = make_func(
            "neg_int",
            vec![TypedParam::untyped("x".to_string(), span())],
            vec![Stmt::Return {
                value: Some(Expr::ModuleCall {
                    module: "Core".to_string().into(),
                    function: "neg_int".to_string().into(),
                    args: vec![Expr::Var("x".to_string().into(), span())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: span(),
                }),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_generated_untyped_params() {
        // @generated f(x) = x must not use normal value-argument
        // specialization: the generated frame binds x to typeof(x), not x.
        let func = make_func(
            "f",
            vec![TypedParam::untyped("x".to_string(), span())],
            vec![
                Stmt::Meta {
                    annotation: MetaAnnotation {
                        name: "generated".to_string(),
                        args: vec![],
                    },
                    span: span(),
                },
                Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), span())),
                    span: span(),
                },
            ],
        );
        assert!(is_generated_function(&func));
        assert!(!needs_specialization(&func));
    }

    #[test]
    fn test_typevar_param_not_runtime_specialized() {
        // id(x::T) where T = x → must NOT need runtime specialization: routing
        // a generic through a single specialized method would bypass dispatch
        // (Issue #5003). It is registered only for reflection.
        let func = make_func(
            "id",
            vec![TypedParam {
                name: "x".to_string(),
                type_annotation: Some(crate::types::JuliaType::TypeVar("T".to_string(), None)),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), span())),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
        assert!(needs_reflection_registration(&func, false));
        assert!(needs_reflection_registration(&func, true));
    }

    #[test]
    fn test_nested_typevar_param_reflection_registration() {
        // f(xs::Vector{T}) where T → registered for reflection (type variable
        // nested inside a parametric container), not for runtime specialization
        // (Issue #5003).
        let func = make_func(
            "f",
            vec![TypedParam {
                name: "xs".to_string(),
                type_annotation: Some(crate::types::JuliaType::VectorOf(Box::new(
                    crate::types::JuliaType::TypeVar("T".to_string(), None),
                ))),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![Stmt::Return {
                value: Some(Expr::Var("xs".to_string().into(), span())),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
        assert!(needs_reflection_registration(&func, false));
        assert!(needs_reflection_registration(&func, true));
    }

    #[test]
    fn test_typed_non_typevar_param_reflection_registration_by_origin() {
        // f(x::Int64) → never needs runtime specialization: its concrete type
        // is fixed at definition time. Reflection registration now depends on
        // `is_user_defined` (Issues #10145 / #10264): Base/Core functions
        // (`is_user_defined = false`) keep the pre-existing narrow behavior —
        // they are classified through the curated `_classify_effects` name
        // table instead — while module/user-authored functions
        // (`is_user_defined = true`) ARE registered, so `Base.infer_effects` /
        // `infer_exception_type` can walk their body (e.g. a `println` nested
        // inside `if x ... end`) instead of silently defaulting to the
        // optimistic all-true `_effects_total()` summary.
        let func = make_func(
            "f",
            vec![TypedParam {
                name: "x".to_string(),
                type_annotation: Some(crate::types::JuliaType::Int64),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), span())),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
        assert!(!needs_reflection_registration(&func, false));
        assert!(needs_reflection_registration(&func, true));
    }

    #[test]
    fn test_generated_function_never_registered_for_reflection_regardless_of_origin() {
        // A `@generated` method's staged body must never be retained as if it
        // were the method's real semantic body, even for the new
        // `is_user_defined` widening (Issues #10145 / #10264).
        let func = make_func(
            "f",
            vec![TypedParam {
                name: "x".to_string(),
                type_annotation: Some(crate::types::JuliaType::Int64),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![
                Stmt::Meta {
                    annotation: MetaAnnotation {
                        name: "generated".to_string(),
                        args: vec![],
                    },
                    span: span(),
                },
                Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), span())),
                    span: span(),
                },
            ],
        );
        assert!(is_generated_function(&func));
        assert!(!needs_reflection_registration(&func, false));
        assert!(!needs_reflection_registration(&func, true));
    }

    #[test]
    fn test_generated_function_with_typevar_param_never_registered_for_reflection() {
        // A `@generated` method whose param is where-bound (`x::T`) must still
        // never be registered: before the adversarial-review fix (Issues
        // #10145 / #10264), the pre-existing `param_annotation_uses_typevar`
        // branch returned `true` here regardless of `is_generated_function`,
        // which would have let reflection walk a `@generated` method's staged
        // generator body as if it were the method's real per-call body.
        let func = make_func(
            "id",
            vec![TypedParam {
                name: "x".to_string(),
                type_annotation: Some(crate::types::JuliaType::TypeVar("T".to_string(), None)),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![
                Stmt::Meta {
                    annotation: MetaAnnotation {
                        name: "generated".to_string(),
                        args: vec![],
                    },
                    span: span(),
                },
                Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), span())),
                    span: span(),
                },
            ],
        );
        assert!(is_generated_function(&func));
        assert!(!needs_reflection_registration(&func, false));
        assert!(!needs_reflection_registration(&func, true));
    }

    #[test]
    fn test_generated_function_with_complex_param_never_registered_for_reflection() {
        // A `@generated` method with a `Complex`-annotated param must still
        // never be registered — same adversarial-review class as the
        // where-bound-typevar case above: the pre-existing
        // `param_annotation_is_runtime_open` branch is unconditional on its
        // own and must not retain a `@generated` method's staged body.
        let func = make_func(
            "f",
            vec![TypedParam {
                name: "z".to_string(),
                type_annotation: Some(crate::types::JuliaType::Struct(
                    "Complex{Float64}".to_string(),
                )),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![
                Stmt::Meta {
                    annotation: MetaAnnotation {
                        name: "generated".to_string(),
                        args: vec![],
                    },
                    span: span(),
                },
                Stmt::Return {
                    value: Some(Expr::Var("z".to_string().into(), span())),
                    span: span(),
                },
            ],
        );
        assert!(is_generated_function(&func));
        assert!(!needs_reflection_registration(&func, false));
        assert!(!needs_reflection_registration(&func, true));
    }
}
