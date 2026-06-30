//! Conditional type narrowing for abstract interpretation.
//!
//! This module implements type narrowing in conditional branches,
//! enabling more precise type inference through control flow analysis.
//!
//! # Implementation Approach
//!
//! This module uses **environment splitting** rather than generating Conditional types
//! in the lattice. This approach:
//!
//! - **Splits the type environment** into separate environments for then/else branches
//! - **Applies type narrowing** to each branch's environment
//! - **Merges environments** after analyzing both branches
//!
//! This is functionally equivalent to using Conditional types but simpler to implement
//! and maintain. The design document (TYPE_INFERENCE_ENHANCEMENT.md) describes Conditional
//! types, but the actual implementation uses environment splitting for practical reasons.
//!
//! # Design Trade-offs
//!
//! **Environment Splitting (Current Implementation)**:
//! - ✅ Simpler implementation
//! - ✅ Works correctly for all tested cases
//! - ✅ Lower maintenance burden
//! - ❌ Conditional information is lost after merge
//! - ❌ Cannot represent conditional types in function signatures
//!
//! **Conditional Types (Design Specification)**:
//! - ✅ Preserves conditional information in the lattice
//! - ✅ Better optimization potential
//! - ✅ Consistent with Julia's approach
//! - ❌ More complex implementation
//! - ❌ Requires updates to all lattice operations
//!
//! The current implementation prioritizes correctness and simplicity over perfect
//! alignment with the design specification. Conditional types could be added as
//! a future enhancement if optimization becomes critical.
//!
//! # MustAlias-style refinements (Issue #3504)
//!
//! In addition to plain variable narrowing, this module records refinements
//! against simple field identities: an `isa(obj.f, T)` (or
//! `obj.f !== nothing`) guard stores a structured `(root=obj, field=f)`
//! refinement in the type environment. The corresponding `Expr::FieldAccess`
//! arm in the inference engine consults that side table before falling back to
//! the declared field type.
//! This is a deliberately lightweight subset of Julia's `MustAlias` (see
//! `julia/Compiler/src/typelattice.jl`) — we track no aliasing graph and
//! no SSA versioning. The structured root+field identity is enough for the common
//! `if obj.f isa T; obj.f.something; end` shape.
//!
//! Path refinements are invalidated by the inference engine when the
//! underlying storage is rebound:
//! - `Stmt::Assign { var, .. }` drops every `var.*` and `var[*]` path.
//! - `Stmt::FieldAssign { object, field, .. }` drops the precise
//!   `object.field` key (sibling fields keep their refinements — single-
//!   field writes can't disturb them).
//! - `Stmt::IndexAssign { array, indices, .. }` drops `array[N]` when the
//!   index is a constant we recognize (`Literal::Int`/`Literal::Bool`),
//!   otherwise drops every `array[*]` path conservatively.
//! - `Stmt::DictAssign { dict, .. }` drops every `dict[*]` path.
//! - `Stmt::DestructuringAssign { targets, .. }` drops paths under each
//!   rebound target.
//!
//! ## Differences from Julia's `MustAlias`
//!
//! - **No alias graph.** We don't know that `a` and `b` may refer to the
//!   same mutable object; mutating `a.f` doesn't invalidate `b.f` even
//!   when the runtime would. Aliasing-driven invalidation is intentionally
//!   left to a follow-up PR.
//! - **Root-scoped field identities.** Nested field access like `a.b.c` is
//!   refinement-tracked under the root variable `a`, but arbitrary object
//!   expressions such as `arr[i].field` are not represented as refinement
//!   paths.
//! - **No SSA versioning.** A loop body that rebinds `var` once per
//!   iteration loses its path refinements at the rebind, like a regular
//!   straight-line assignment. We rely on the loop fixpoint widening
//!   to converge to a sound type for the bare binding.
//! - **No indexed access narrowing.** Julia's `MustAlias` machinery forms
//!   aliases for eligible `getfield` loads, not mutable container `getindex`
//!   loads. `getfield(obj, :field)` is normalized into the same `obj.field`
//!   path key as `Expr::FieldAccess`.

use crate::compile::abstract_interp::struct_info::StructTypeInfo;
use crate::compile::abstract_interp::TypeEnv;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Function, Literal, Stmt, UnaryOp};
use std::collections::HashMap;

/// Result of splitting an environment by a condition.
#[derive(Debug)]
pub struct SplitEnv {
    /// Environment for the then-branch (condition is true)
    pub then_env: TypeEnv,
    /// Environment for the else-branch (condition is false)
    pub else_env: TypeEnv,
}

/// Splits an environment based on a conditional expression.
///
/// This function analyzes the condition and narrows types accordingly:
/// - `isa(val, Type)`: Narrows `val` to `Type` in then-branch
/// - `val === nothing`: Narrows `val` to `Nothing` in then-branch
/// - `val !== nothing`: Narrows `val` to exclude `Nothing` in then-branch
///
/// # Example
/// ```text
/// if val isa Int
///     # then-branch: val is Int
/// else
///     # else-branch: val is not Int (uses subtract)
/// end
/// ```
pub fn split_env_by_condition(env: &TypeEnv, condition: &Expr) -> SplitEnv {
    split_env_by_condition_impl(env, condition, None)
}

fn split_env_by_condition_impl(
    env: &TypeEnv,
    condition: &Expr,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> SplitEnv {
    match condition {
        // isa(val, Type) pattern
        Expr::Call { function, args, .. } if function == "isa" && args.len() == 2 => {
            handle_isa_condition(env, &args[0], &args[1], struct_table)
        }

        // isa builtin: isa(val, Type)
        Expr::Builtin { name, args, .. } if matches!(name, BuiltinOp::Isa) && args.len() == 2 => {
            handle_isa_condition(env, &args[0], &args[1], struct_table)
        }

        // typeof(val) === T / typeof(val) == T (and the reversed operand order).
        //
        // For a concrete type `T` this is equivalent to `val isa T` for the
        // purpose of narrowing: in the then-branch `val` is exactly `T`, and in
        // the else-branch `T` is subtracted. Both `===` (BinaryOp::Egal) and
        // `==` (BinaryOp::Eq) are sound here because `==` on `DataType` values
        // falls back to identity (`===`); unlike narrowing against the `nothing`
        // *value* (Issue #3522), there is no user-overloadable `==` ambiguity for
        // the result of `typeof`. (Issue #5140)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Egal | BinaryOp::Eq)
            && (extract_typeof_value(left).is_some() || extract_typeof_value(right).is_some()) =>
        {
            if let Some(val_expr) = extract_typeof_value(left) {
                handle_isa_condition(env, val_expr, right, struct_table)
            } else if let Some(val_expr) = extract_typeof_value(right) {
                handle_isa_condition(env, val_expr, left, struct_table)
            } else {
                split_env_no_narrow(env)
            }
        }

        // typeof(val) !== T / typeof(val) != T (and reversed): negate the
        // equality case by swapping the then/else environments. (Issue #5140)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::NotEgal | BinaryOp::Ne)
            && (extract_typeof_value(left).is_some() || extract_typeof_value(right).is_some()) =>
        {
            let split = if let Some(val_expr) = extract_typeof_value(left) {
                handle_isa_condition(env, val_expr, right, struct_table)
            } else if let Some(val_expr) = extract_typeof_value(right) {
                handle_isa_condition(env, val_expr, left, struct_table)
            } else {
                split_env_no_narrow(env)
            };
            SplitEnv {
                then_env: split.else_env,
                else_env: split.then_env,
            }
        }

        // val === nothing (using === operator - Egal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Egal) && is_nothing_literal(right) => {
            handle_nothing_check(env, left, true, struct_table)
        }

        // nothing === val (using === operator - Egal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Egal) && is_nothing_literal(left) => {
            handle_nothing_check(env, right, true, struct_table)
        }

        // val !== nothing (using !== operator - NotEgal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::NotEgal) && is_nothing_literal(right) => {
            handle_nothing_check(env, left, false, struct_table)
        }

        // nothing !== val (using !== operator - NotEgal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::NotEgal) && is_nothing_literal(left) => {
            handle_nothing_check(env, right, false, struct_table)
        }

        // Note: `==` and `!=` are intentionally NOT used for narrowing against `nothing`.
        // In Julia, `==` is a generic, overloadable function. A user-defined `==(x, ::Nothing)`
        // can return `true` for non-`Nothing` values, so narrowing on `==`/`!=` is unsound.
        // Use `===` / `!==` (BinaryOp::Egal / BinaryOp::NotEgal) for identity-based narrowing.
        // (Issue #3522)

        // !cond (logical NOT) - swap then and else branches
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
            ..
        } => {
            let inner_split = split_env_by_condition_impl(env, operand, struct_table);
            // Negate: swap then and else environments
            SplitEnv {
                then_env: inner_split.else_env,
                else_env: inner_split.then_env,
            }
        }

        // cond1 && cond2 (logical AND)
        // then: both conditions are true (apply both narrowings)
        // else: at least one condition is false (join of possible false cases)
        Expr::BinaryOp {
            op: BinaryOp::And,
            left,
            right,
            ..
        } => {
            // First, split by cond1
            let split1 = split_env_by_condition_impl(env, left, struct_table);
            // Then, split cond1's then-branch by cond2
            let split2 = split_env_by_condition_impl(&split1.then_env, right, struct_table);

            // then: cond1 true AND cond2 true
            let then_env = split2.then_env;

            // else: cond1 false OR (cond1 true AND cond2 false)
            // This is the join of split1.else_env and split2.else_env
            let mut else_env = split1.else_env.clone();
            else_env.merge(&split2.else_env);

            SplitEnv { then_env, else_env }
        }

        // cond1 || cond2 (logical OR)
        // then: at least one condition is true (join of possible true cases)
        // else: both conditions are false (apply both narrowings)
        Expr::BinaryOp {
            op: BinaryOp::Or,
            left,
            right,
            ..
        } => {
            // First, split by cond1
            let split1 = split_env_by_condition_impl(env, left, struct_table);
            // Then, split cond1's else-branch by cond2
            let split2 = split_env_by_condition_impl(&split1.else_env, right, struct_table);

            // then: cond1 true OR (cond1 false AND cond2 true)
            // This is the join of split1.then_env and split2.then_env
            let mut then_env = split1.then_env.clone();
            then_env.merge(&split2.then_env);

            // else: cond1 false AND cond2 false
            let else_env = split2.else_env;

            SplitEnv { then_env, else_env }
        }

        // Unhandled condition: no narrowing
        _ => SplitEnv {
            then_env: env.clone(),
            else_env: env.clone(),
        },
    }
}

/// Splits an environment by a condition, including a narrow interprocedural
/// predicate subset. This recognizes one-argument functions whose whole body is
/// a direct predicate over that argument (`isa(x, T)`, `x === nothing`, or
/// `x !== nothing`) and applies the resulting refinement to the caller's
/// actual argument.
pub fn split_env_by_condition_with_predicates(
    env: &TypeEnv,
    condition: &Expr,
    function_table: &HashMap<String, Function>,
) -> SplitEnv {
    split_env_by_condition_with_predicates_impl(env, condition, function_table, None)
}

/// Variant used by the production inference engine when user-defined struct
/// identities are available. This lets `x isa MyStruct` narrow to the concrete
/// struct lattice type instead of failing to resolve the right-hand type name.
pub fn split_env_by_condition_with_predicates_and_structs(
    env: &TypeEnv,
    condition: &Expr,
    function_table: &HashMap<String, Function>,
    struct_table: &HashMap<String, StructTypeInfo>,
) -> SplitEnv {
    split_env_by_condition_with_predicates_impl(env, condition, function_table, Some(struct_table))
}

fn split_env_by_condition_with_predicates_impl(
    env: &TypeEnv,
    condition: &Expr,
    function_table: &HashMap<String, Function>,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> SplitEnv {
    if let Some(inlined_condition) = predicate_call_condition(condition, function_table) {
        return split_env_by_condition_impl(env, &inlined_condition, struct_table);
    }

    split_env_by_condition_impl(env, condition, struct_table)
}

fn predicate_call_condition(
    condition: &Expr,
    function_table: &HashMap<String, Function>,
) -> Option<Expr> {
    let Expr::Call {
        function,
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        ..
    } = condition
    else {
        return None;
    };

    if args.len() != 1
        || !kwargs.is_empty()
        || splat_mask.iter().any(|is_splat| *is_splat)
        || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
    {
        return None;
    }

    let func = function_table.get(function)?;
    if func.params.len() != 1 || func.params[0].is_varargs || !func.kwparams.is_empty() {
        return None;
    }

    let param_name = &func.params[0].name;
    let predicate_expr = single_return_expr(func)?;
    rewrite_single_arg_predicate(predicate_expr, param_name, &args[0])
}

fn single_return_expr(func: &Function) -> Option<&Expr> {
    if func.body.stmts.len() != 1 {
        return None;
    }

    match &func.body.stmts[0] {
        Stmt::Return {
            value: Some(expr), ..
        }
        | Stmt::Expr { expr, .. } => Some(expr),
        _ => None,
    }
}

fn rewrite_single_arg_predicate(expr: &Expr, param_name: &str, actual: &Expr) -> Option<Expr> {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } if function == "isa"
            && args.len() == 2
            && kwargs.is_empty()
            && !splat_mask.iter().any(|is_splat| *is_splat)
            && !kwargs_splat_mask.iter().any(|is_splat| *is_splat)
            && is_param_var(&args[0], param_name) =>
        {
            Some(Expr::Call {
                function: function.clone(),
                args: vec![actual.clone(), args[1].clone()],
                kwargs: vec![],
                splat_mask: vec![false, false],
                kwargs_splat_mask: vec![],
                span: *span,
            })
        }
        Expr::Builtin {
            name: BuiltinOp::Isa,
            args,
            span,
        } if args.len() == 2 && is_param_var(&args[0], param_name) => Some(Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![actual.clone(), args[1].clone()],
            span: *span,
        }),
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } if matches!(op, BinaryOp::Egal | BinaryOp::NotEgal)
            && is_param_var(left, param_name)
            && is_nothing_literal(right) =>
        {
            Some(Expr::BinaryOp {
                op: *op,
                left: Box::new(actual.clone()),
                right: Box::new((**right).clone()),
                span: *span,
            })
        }
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } if matches!(op, BinaryOp::Egal | BinaryOp::NotEgal)
            && is_nothing_literal(left)
            && is_param_var(right, param_name) =>
        {
            Some(Expr::BinaryOp {
                op: *op,
                left: Box::new((**left).clone()),
                right: Box::new(actual.clone()),
                span: *span,
            })
        }
        _ => None,
    }
}

fn is_param_var(expr: &Expr, param_name: &str) -> bool {
    matches!(expr, Expr::Var(name, _) if name == param_name)
}

/// Extracts a path string for a `var.field` access if the object has a
/// narrowable root path. Used by both conditional narrowing and
/// `Expr::FieldAccess` inference (Issues #3520/#5862).
pub fn extract_field_narrow_path(object: &Expr, field: &str) -> Option<String> {
    let object_path = extract_field_object_narrow_path(object)?;
    Some(format!("{}.{}", object_path, field))
}

fn extract_field_object_narrow_path(object: &Expr) -> Option<String> {
    match object {
        Expr::Var(obj_name, _) => Some(obj_name.clone()),
        Expr::FieldAccess { object, field, .. } => extract_field_narrow_path(object, field),
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } => extract_getfield_narrow_path(function, args, kwargs, splat_mask, kwargs_splat_mask),
        _ => None,
    }
}

/// Extracts a path string for a `getfield(var, :field)` call if the object is
/// a simple variable and the field is a literal Symbol/String. Returns `None`
/// otherwise. This deliberately maps to the same key as `var.field`, so the
/// existing refinement and invalidation machinery stays shared (Issue #3716).
pub fn extract_getfield_narrow_path(
    function: &str,
    args: &[Expr],
    kwargs: &[(String, Expr)],
    splat_mask: &[bool],
    kwargs_splat_mask: &[bool],
) -> Option<String> {
    if function != "getfield"
        || args.len() != 2
        || !kwargs.is_empty()
        || splat_mask.iter().any(|is_splat| *is_splat)
        || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
    {
        return None;
    }

    let field = extract_static_field_name(&args[1])?;
    extract_field_narrow_path(&args[0], field)
}

/// Extracts a statically known field name used by `getfield(obj, field)`.
///
/// The parser lowers `:value` as `QuoteLiteral(SymbolNew("value"))`, while
/// tests and a few synthetic IR producers may use `Literal::Symbol` directly.
/// Treat both as the same static field key.
pub fn extract_static_field_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Literal(Literal::Symbol(s), _) | Expr::Literal(Literal::Str(s), _) => {
            Some(s.as_str())
        }
        Expr::QuoteLiteral { constructor, .. } => match &**constructor {
            Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args,
                ..
            } if args.len() == 1 => match &args[0] {
                Expr::Literal(Literal::Str(s), _) => Some(s.as_str()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Extracts a path string for an `arr[i]` access where `arr` is a simple
/// variable and `i` is a constant literal. Returns `None` otherwise. Used by
/// both conditional narrowing and `Expr::Index` inference (Issue #3521).
pub fn extract_index_narrow_path(array: &Expr, indices: &[Expr]) -> Option<String> {
    if indices.len() != 1 {
        return None;
    }
    if let Expr::Var(arr_name, _) = array {
        if let Some(idx_str) = extract_constant_index(&indices[0]) {
            return Some(format!("{}[{}]", arr_name, idx_str));
        }
    }
    None
}

/// Extracts a narrowable path from an expression.
///
/// This handles:
/// - `Expr::Var(name)` -> `Some("name")`
/// - `Expr::FieldAccess { object: Expr::Var(obj), field }` -> `Some("obj.field")`
/// - nested field paths such as `a.b.c` -> `Some("a.b.c")`
///
/// Returns `None` for complex expressions that cannot be tracked in the environment.
fn extract_narrowable_path(expr: &Expr) -> Option<String> {
    match expr {
        // Simple variable
        Expr::Var(name, _) => Some(name.clone()),

        // Field access: obj.field or nested obj.inner.field
        Expr::FieldAccess { object, field, .. } => extract_field_narrow_path(object, field),

        // Call-form field access: getfield(obj, :field)
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } => extract_getfield_narrow_path(function, args, kwargs, splat_mask, kwargs_splat_mask),

        _ => None,
    }
}

fn extract_narrowable_parent(expr: &Expr) -> Option<String> {
    let path = extract_narrowable_path(expr)?;
    let end = path.find(['.', '[']).unwrap_or(path.len());
    Some(path[..end].to_string())
}

fn set_narrowed_path(env: &mut TypeEnv, expr: &Expr, path: &str, ty: LatticeType) {
    match expr {
        Expr::Var(_, _) => env.set(path, ty),
        Expr::FieldAccess { .. } | Expr::Index { .. } | Expr::Call { .. } => {
            if let Some(parent) = extract_narrowable_parent(expr) {
                env.set_refinement(&parent, path, ty);
            } else {
                env.set(path, ty);
            }
        }
        _ => env.set(path, ty),
    }
}

/// Extracts a constant index value as a string.
fn extract_constant_index(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(Literal::Int(i), _) => Some(i.to_string()),
        Expr::Literal(Literal::Bool(b), _) => Some(if *b { "true" } else { "false" }.to_string()),
        // Could extend to support Symbol literals in the future
        _ => None,
    }
}

/// Handles `isa(val, Type)` conditional narrowing.
fn handle_isa_condition(
    env: &TypeEnv,
    val_expr: &Expr,
    type_expr: &Expr,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> SplitEnv {
    // Extract narrowable path from val_expr (supports Var, FieldAccess,
    // nested FieldAccess, and getfield calls).
    let path = match extract_narrowable_path(val_expr) {
        Some(p) => p,
        None => {
            // Complex expression: cannot narrow
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Extract target type from type_expr
    let target_type = match extract_type_from_expr_with_structs(type_expr, struct_table) {
        Some(ty) => ty,
        None => {
            // Cannot determine type: no narrowing
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Get current type of the path. If the path has no prior refinement,
    // use declared struct-field information for simple `obj.field` /
    // `getfield(obj, :field)` shapes when the engine provided a struct table.
    let current_type = match current_narrowable_type(env, &path, val_expr, struct_table) {
        Some(ty) => ty,
        None => {
            // Path not in environment: no narrowing
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Then-branch: narrow to target type (intersection)
    let then_type = current_type.meet(&target_type);
    let mut then_env = env.clone();
    set_narrowed_path(&mut then_env, val_expr, &path, then_type);

    // Else-branch: exclude target type (subtraction)
    let else_type = current_type.subtract(&target_type);
    let mut else_env = env.clone();
    set_narrowed_path(&mut else_env, val_expr, &path, else_type);

    SplitEnv { then_env, else_env }
}

/// Handles `val === nothing` or `val !== nothing` conditional narrowing.
fn handle_nothing_check(
    env: &TypeEnv,
    val_expr: &Expr,
    is_equality: bool,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> SplitEnv {
    // Extract narrowable path (supports Var, FieldAccess, nested FieldAccess,
    // and getfield calls).
    let path = match extract_narrowable_path(val_expr) {
        Some(p) => p,
        None => {
            // Complex expression: cannot narrow
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Get current type of the path. If the path has no prior refinement,
    // use declared struct-field information for simple `obj.field` /
    // `getfield(obj, :field)` shapes when the engine provided a struct table.
    let current_type = match current_narrowable_type(env, &path, val_expr, struct_table) {
        Some(ty) => ty,
        None => {
            // Path not in environment: no narrowing
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    let nothing_type = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Nothing,
    )));

    let (then_type, else_type) = if is_equality {
        // val === nothing:
        // - then-branch: val is Nothing
        // - else-branch: val is not Nothing
        (
            current_type.meet(&nothing_type),
            current_type.subtract(&nothing_type),
        )
    } else {
        // val !== nothing:
        // - then-branch: val is not Nothing
        // - else-branch: val is Nothing
        (
            current_type.subtract(&nothing_type),
            current_type.meet(&nothing_type),
        )
    };

    let mut then_env = env.clone();
    set_narrowed_path(&mut then_env, val_expr, &path, then_type);

    let mut else_env = env.clone();
    set_narrowed_path(&mut else_env, val_expr, &path, else_type);

    SplitEnv { then_env, else_env }
}

fn current_narrowable_type(
    env: &TypeEnv,
    path: &str,
    expr: &Expr,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> Option<LatticeType> {
    env.get_refinement(path)
        .cloned()
        .or_else(|| env.get(path).cloned())
        .or_else(|| declared_field_type_for_expr(env, expr, struct_table))
}

fn declared_field_type_for_expr(
    env: &TypeEnv,
    expr: &Expr,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> Option<LatticeType> {
    let (object, field) = match expr {
        Expr::FieldAccess { object, field, .. } => (&**object, field.as_str()),
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if function == "getfield"
            && args.len() == 2
            && kwargs.is_empty()
            && !splat_mask.iter().any(|is_splat| *is_splat)
            && !kwargs_splat_mask.iter().any(|is_splat| *is_splat) =>
        {
            let field = extract_static_field_name(&args[1])?;
            (&args[0], field)
        }
        _ => return None,
    };

    let object_ty = match object {
        Expr::Var(root, _) => env.get(root)?.clone(),
        _ => declared_field_type_for_expr(env, object, struct_table)?,
    };
    let LatticeType::Concrete(ConcreteType::Struct { name, .. }) = object_ty else {
        return None;
    };
    struct_table?
        .get(&name)
        .and_then(|info| info.get_field_type(field))
        .cloned()
}

/// Checks if an expression is the `nothing` literal.
fn is_nothing_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Nothing, _))
}

/// A `SplitEnv` that applies no narrowing (both branches keep the input env).
fn split_env_no_narrow(env: &TypeEnv) -> SplitEnv {
    SplitEnv {
        then_env: env.clone(),
        else_env: env.clone(),
    }
}

/// If `expr` is `typeof(val)` (either the `Call`-form `typeof(val)` or the
/// `Builtin { TypeOf, [val] }` form), returns the inner `val` expression.
/// Returns `None` for any other shape (Issue #5140).
fn extract_typeof_value(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args,
            ..
        } if args.len() == 1 => Some(&args[0]),
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if function == "typeof"
            && args.len() == 1
            && kwargs.is_empty()
            && !splat_mask.iter().any(|is_splat| *is_splat)
            && !kwargs_splat_mask.iter().any(|is_splat| *is_splat) =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

/// Extracts a LatticeType from a type expression.
///
/// This handles simple type expressions like:
/// - Variable names: `Int`, `Float64`, `String`
/// - All numeric types: Int8, Int16, Int32, Int64, Int128, UInt8-UInt128, Float32, Float64
/// - `Union{T1, T2, ...}` lowered as `Builtin::TypeOf("Union{...}")` (Issue #3523)
#[cfg(test)]
fn extract_type_from_expr(expr: &Expr) -> Option<LatticeType> {
    extract_type_from_expr_with_structs(expr, None)
}

fn extract_type_from_expr_with_structs(
    expr: &Expr,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> Option<LatticeType> {
    match expr {
        Expr::Var(name, _) => name_to_lattice_type(name, struct_table),

        // Parametric/Union types are lowered as
        // `Builtin { name: TypeOf, args: [Literal::Str("Union{...}")] }`
        // (see lowering/expr/mod.rs::lower_parametrized_type_expr).
        // We only handle simple Union{...} of recognized type names; everything
        // else stays conservative (Issue #3523).
        Expr::Builtin { name, args, .. }
            if matches!(name, BuiltinOp::TypeOf) && args.len() == 1 =>
        {
            if let Expr::Literal(Literal::Str(s), _) = &args[0] {
                parse_static_type_string(s, struct_table)
            } else {
                None
            }
        }

        _ => None,
    }
}

/// Map a bare type name to a `LatticeType`.
fn name_to_lattice_type(
    name: &str,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> Option<LatticeType> {
    match name {
        // Signed integers
        "Int8" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int8),
        ))),
        "Int16" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int16),
        ))),
        "Int32" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int32),
        ))),
        "Int" if crate::types::native_int_type_name() == "Int32" => Some(LatticeType::Concrete(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        )),
        "Int" | "Int64" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))),
        "Int128" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int128),
        ))),
        "BigInt" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::BigInt),
        ))),

        // Unsigned integers
        "UInt8" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt8),
        ))),
        "UInt16" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt16),
        ))),
        "UInt32" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt32),
        ))),
        "UInt" if crate::types::native_uint_type_name() == "UInt32" => Some(LatticeType::Concrete(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
        )),
        "UInt" | "UInt64" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt64),
        ))),
        "UInt128" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt128),
        ))),

        // Floating point
        "Float16" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float16),
        ))),
        "Float32" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))),
        "Float" | "Float64" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))),
        "BigFloat" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::BigFloat),
        ))),

        // Other concrete types
        "Bool" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Bool),
        ))),
        "String" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String),
        ))),
        "Char" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Char),
        ))),
        "Nothing" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Nothing),
        ))),
        "Missing" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Missing),
        ))),
        "Symbol" => Some(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Symbol),
        ))),

        // Top
        "Any" => Some(LatticeType::Top),

        _ => struct_table.and_then(|table| {
            table.get(name).map(|info| {
                LatticeType::Concrete(ConcreteType::Struct {
                    name: name.to_string(),
                    type_id: info.type_id,
                })
            })
        }),
    }
}

/// Parse a textual type expression that the lowering layer stored as a string
/// inside `Builtin::TypeOf`. Currently understands `Union{T1, T2, ...}` for
/// recognized leaf types; all other forms return `None` so the caller stays
/// conservative.
fn parse_static_type_string(
    s: &str,
    struct_table: Option<&HashMap<String, StructTypeInfo>>,
) -> Option<LatticeType> {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("Union{") {
        let inner = rest.strip_suffix('}')?;
        let mut variants: std::collections::BTreeSet<ConcreteType> =
            std::collections::BTreeSet::new();
        let mut result: Option<LatticeType> = None;
        for part in inner.split(',') {
            let leaf = part.trim();
            // Recursively parse, but we only flatten one level for now.
            let lt = name_to_lattice_type(leaf, struct_table)
                .or_else(|| parse_static_type_string(leaf, struct_table))?;
            match lt {
                LatticeType::Concrete(ct) => {
                    variants.insert(ct);
                }
                LatticeType::Union(set) => {
                    for ct in set {
                        variants.insert(ct);
                    }
                }
                LatticeType::Top => {
                    // `Union{Any, ...}` is just `Any`.
                    result = Some(LatticeType::Top);
                }
                _ => return None,
            }
        }
        if matches!(result, Some(LatticeType::Top)) {
            return Some(LatticeType::Top);
        }
        return Some(match variants.len() {
            0 => LatticeType::Bottom,
            1 => LatticeType::Concrete(variants.into_iter().next()?),
            _ => LatticeType::Union(variants),
        });
    }
    // Non-Union parametric forms (e.g. `Vector{Int}`, `Complex{Float64}`)
    // are not yet supported — stay conservative.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_split_env_isa_narrows_then_branch() {
        let mut env = TypeEnv::new();
        // val has type Any (Top)
        env.set("val", LatticeType::Top);

        // Condition: isa(val, Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );

        // Else-branch: val should be Top (can't subtract from Top)
        assert_eq!(split.else_env.get("val"), Some(&LatticeType::Top));
    }

    #[test]
    fn test_split_env_isa_with_union() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: isa(val, Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );

        // Else-branch: val should be String (Union - Int64)
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_split_env_nothing_check_equality() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: val === nothing (uses Egal operator)
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Nothing
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );

        // Else-branch: val should be Int64 (Union - Nothing)
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_split_env_nothing_check_inequality() {
        let mut env = TypeEnv::new();
        // val has type Union{String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: val !== nothing (uses NotEgal operator)
        let condition = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be String (not nothing)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );

        // Else-branch: val should be Nothing
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );
    }

    #[test]
    fn test_split_env_unhandled_condition() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        // Condition: x > 5 (not a type narrowing condition)
        let condition = Expr::BinaryOp {
            op: BinaryOp::Gt,
            left: Box::new(Expr::Var("x".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Int(5), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Both branches should have the same environment (no narrowing)
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_is_nothing_literal() {
        assert!(is_nothing_literal(&Expr::Literal(
            Literal::Nothing,
            dummy_span()
        )));
        assert!(!is_nothing_literal(&Expr::Literal(
            Literal::Int(42),
            dummy_span()
        )));
        assert!(!is_nothing_literal(&Expr::Var(
            "x".to_string(),
            dummy_span()
        )));
    }

    #[test]
    fn test_extract_type_from_expr() {
        // Basic types
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int64".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Float64".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("String".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UnknownType".to_string(), dummy_span())),
            None
        );
    }

    #[test]
    fn test_extract_type_from_expr_all_numeric_types() {
        // Signed integers
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int8".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int8)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int16".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int16)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int32".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int32)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int128".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int128)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("BigInt".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::BigInt)
            )))
        );

        // Unsigned integers
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt8".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt8)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt16".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt16)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt32".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt32)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt64)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt64".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt64)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt128".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt128)
            )))
        );

        // Floating point
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Float32".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float32)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Float".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("BigFloat".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::BigFloat)
            )))
        );

        // Other types
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Bool".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Char".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Char)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Nothing".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Missing".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Missing)
            )))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Symbol".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Symbol)
            )))
        );
    }

    // ====== Tests for compound boolean conditions (&&, ||, !) ======

    #[test]
    fn test_split_env_not_operator() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: !(val === nothing)
        // This is equivalent to: val !== nothing
        let inner_condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };
        let condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(inner_condition),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64 (NOT nothing)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );

        // Else-branch: val should be Nothing
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );
    }

    #[test]
    fn test_split_env_and_operator_both_narrow() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: val !== nothing && isa(val, Int64)
        // After &&: val should be Int64 in then-branch
        let cond1 = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };
        let cond2 = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let condition = Expr::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(cond1),
            right: Box::new(cond2),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64 (both conditions satisfied)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );

        // Else-branch: val could be Nothing or String (not narrowed to a single type)
        // The else branch is the join of:
        // - cond1 false (val is Nothing)
        // - cond1 true but cond2 false (val is String)
        // Result should be Union{Nothing, String}
        let else_type = split.else_env.get("val").unwrap();
        assert!(
            matches!(else_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            else_type
        );
        if let LatticeType::Union(types) = else_type {
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))));
            assert!(!types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
        }
    }

    #[test]
    fn test_split_env_or_operator_either_narrow() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String, Bool}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        env.set("val", LatticeType::Union(union_types));

        // Condition: isa(val, Int64) || isa(val, String)
        // Then-branch: val could be Int64 or String
        // Else-branch: val should be Bool
        let cond1 = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let cond2 = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("String".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let condition = Expr::BinaryOp {
            op: BinaryOp::Or,
            left: Box::new(cond1),
            right: Box::new(cond2),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val could be Int64 or String (join of both true branches)
        let then_type = split.then_env.get("val").unwrap();
        assert!(
            matches!(then_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            then_type
        );
        if let LatticeType::Union(types) = then_type {
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))));
        }

        // Else-branch: val should be Bool (both conditions false)
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
    }

    #[test]
    fn test_split_env_nested_not_not() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: !!(val === nothing)
        // Double negation should give same result as val === nothing
        let inner_condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };
        let not_condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(inner_condition),
            span: dummy_span(),
        };
        let condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(not_condition),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Nothing (same as val === nothing)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );

        // Else-branch: val should be Int64
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_split_env_not_isa() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        env.set("val", LatticeType::Union(union_types));

        // Condition: !(val isa Int64)
        // then: val is NOT Int64 → String
        // else: val is Int64
        let isa_condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(isa_condition),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be String (NOT Int64)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );

        // Else-branch: val should be Int64
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    // ====== Tests for typeof(x) === T narrowing (Issue #5140) ======

    fn typeof_call(val: Expr) -> Expr {
        Expr::Call {
            function: "typeof".to_string(),
            args: vec![val],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }
    }

    fn union_int_string() -> LatticeType {
        let mut set = std::collections::BTreeSet::new();
        set.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        set.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        LatticeType::Union(set)
    }

    #[test]
    fn test_split_env_typeof_egal_narrows_then_branch() {
        let mut env = TypeEnv::new();
        env.set("x", union_int_string());

        // Condition: typeof(x) === Int64
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(typeof_call(Expr::Var("x".to_string(), dummy_span()))),
            right: Box::new(Expr::Var("Int64".to_string(), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // then: x is Int64
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        // else: x is String (Union - Int64)
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_split_env_typeof_eq_narrows_then_branch() {
        let mut env = TypeEnv::new();
        env.set("x", union_int_string());

        // Condition: typeof(x) == Int64 (Eq, not Egal). For types == falls back
        // to === so narrowing is sound (Issue #5140).
        let condition = Expr::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(typeof_call(Expr::Var("x".to_string(), dummy_span()))),
            right: Box::new(Expr::Var("Int64".to_string(), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_split_env_typeof_reversed_operands() {
        let mut env = TypeEnv::new();
        env.set("x", union_int_string());

        // Condition: Int64 === typeof(x) (type on the left)
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("Int64".to_string(), dummy_span())),
            right: Box::new(typeof_call(Expr::Var("x".to_string(), dummy_span()))),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_split_env_typeof_builtin_form() {
        let mut env = TypeEnv::new();
        env.set("x", union_int_string());

        // Condition: typeof(x) === String using the Builtin TypeOf form
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Builtin {
                name: BuiltinOp::TypeOf,
                args: vec![Expr::Var("x".to_string(), dummy_span())],
                span: dummy_span(),
            }),
            right: Box::new(Expr::Var("String".to_string(), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_split_env_typeof_notegal_swaps_branches() {
        let mut env = TypeEnv::new();
        env.set("x", union_int_string());

        // Condition: typeof(x) !== Int64
        // then: x is NOT Int64 -> String; else: x is Int64
        let condition = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(typeof_call(Expr::Var("x".to_string(), dummy_span()))),
            right: Box::new(Expr::Var("Int64".to_string(), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_split_env_typeof_unknown_type_no_narrow() {
        let mut env = TypeEnv::new();
        env.set("x", union_int_string());

        // Condition: typeof(x) === SomeUnknownType -> no narrowing
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(typeof_call(Expr::Var("x".to_string(), dummy_span()))),
            right: Box::new(Expr::Var("SomeUnknownType".to_string(), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        assert_eq!(split.then_env.get("x"), Some(&union_int_string()));
        assert_eq!(split.else_env.get("x"), Some(&union_int_string()));
    }

    // ====== Tests for field/index access type narrowing (Issue #1641) ======

    #[test]
    fn test_extract_narrowable_path_var() {
        let expr = Expr::Var("x".to_string(), dummy_span());
        assert_eq!(extract_narrowable_path(&expr), Some("x".to_string()));
    }

    #[test]
    fn test_extract_narrowable_path_field_access() {
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
            field: "field".to_string(),
            span: dummy_span(),
        };
        assert_eq!(
            extract_narrowable_path(&expr),
            Some("obj.field".to_string())
        );
    }

    #[test]
    fn test_extract_narrowable_path_getfield_call() {
        let expr = Expr::Call {
            function: "getfield".to_string(),
            args: vec![
                Expr::Var("obj".to_string(), dummy_span()),
                Expr::Literal(Literal::Symbol("field".to_string()), dummy_span()),
            ],
            kwargs: vec![],
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        };
        assert_eq!(
            extract_narrowable_path(&expr),
            Some("obj.field".to_string())
        );
    }

    #[test]
    fn test_extract_narrowable_path_index_returns_none_issue_4270() {
        let expr = Expr::Index {
            array: Box::new(Expr::Var("arr".to_string(), dummy_span())),
            indices: vec![Expr::Literal(Literal::Int(1), dummy_span())],
            span: dummy_span(),
        };
        assert_eq!(extract_narrowable_path(&expr), None);
    }

    #[test]
    fn test_extract_narrowable_path_nested_field_issue_5862() {
        let inner = Expr::FieldAccess {
            object: Box::new(Expr::Var("a".to_string(), dummy_span())),
            field: "b".to_string(),
            span: dummy_span(),
        };
        let expr = Expr::FieldAccess {
            object: Box::new(inner),
            field: "c".to_string(),
            span: dummy_span(),
        };
        assert_eq!(extract_narrowable_path(&expr), Some("a.b.c".to_string()));
    }

    #[test]
    fn test_split_env_isa_field_access() {
        let mut env = TypeEnv::new();
        // obj.field has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("obj.field", LatticeType::Union(union_types));

        // Condition: isa(obj.field, Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::FieldAccess {
                    object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
                    field: "field".to_string(),
                    span: dummy_span(),
                },
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: obj.field should be Int64
        assert_eq!(
            split.then_env.get_refinement("obj.field"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );

        // Else-branch: obj.field should be Nothing
        assert_eq!(
            split.else_env.get_refinement("obj.field"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );
    }

    #[test]
    fn test_split_env_nothing_check_field_access() {
        let mut env = TypeEnv::new();
        // obj.value has type Union{String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("obj.value", LatticeType::Union(union_types));

        // Condition: obj.value !== nothing
        let condition = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
                field: "value".to_string(),
                span: dummy_span(),
            }),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: obj.value should be String (not nothing)
        assert_eq!(
            split.then_env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );

        // Else-branch: obj.value should be Nothing
        assert_eq!(
            split.else_env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );
    }

    #[test]
    fn test_split_env_nothing_check_getfield_call() {
        let mut env = TypeEnv::new();
        // obj.value has type Union{String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("obj.value", LatticeType::Union(union_types));

        // Condition: getfield(obj, :value) !== nothing
        let condition = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::Call {
                function: "getfield".to_string(),
                args: vec![
                    Expr::Var("obj".to_string(), dummy_span()),
                    Expr::Literal(Literal::Symbol("value".to_string()), dummy_span()),
                ],
                kwargs: vec![],
                splat_mask: vec![false, false],
                kwargs_splat_mask: vec![],
                span: dummy_span(),
            }),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: obj.value should be String (not nothing)
        assert_eq!(
            split.then_env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );

        // Else-branch: obj.value should be Nothing
        assert_eq!(
            split.else_env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            )))
        );
    }

    #[test]
    fn test_split_env_isa_getfield_call() {
        let mut env = TypeEnv::new();
        // obj.value has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        env.set("obj.value", LatticeType::Union(union_types));

        // Condition: isa(getfield(obj, :value), Int64)
        let condition = Expr::Call {
            function: "isa".to_string(),
            args: vec![
                Expr::Call {
                    function: "getfield".to_string(),
                    args: vec![
                        Expr::Var("obj".to_string(), dummy_span()),
                        Expr::Literal(Literal::Symbol("value".to_string()), dummy_span()),
                    ],
                    kwargs: vec![],
                    splat_mask: vec![false, false],
                    kwargs_splat_mask: vec![],
                    span: dummy_span(),
                },
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            kwargs: vec![],
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: obj.value should be Int64
        assert_eq!(
            split.then_env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );

        // Else-branch: obj.value should be String
        assert_eq!(
            split.else_env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_split_env_isa_index_access_does_not_narrow_issue_4270() {
        let mut env = TypeEnv::new();
        // tup[1] has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        env.set("tup[1]", LatticeType::Union(union_types));

        // Condition: isa(tup[1], Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Index {
                    array: Box::new(Expr::Var("tup".to_string(), dummy_span())),
                    indices: vec![Expr::Literal(Literal::Int(1), dummy_span())],
                    span: dummy_span(),
                },
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Indexed loads are not Julia MustAlias fields. Keep the existing
        // element path unchanged instead of refining it branch-locally.
        assert_eq!(
            split.then_env.get("tup[1]"),
            Some(&LatticeType::Union({
                let mut expected = std::collections::BTreeSet::new();
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                )));
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String,
                )));
                expected
            }))
        );

        assert_eq!(
            split.else_env.get("tup[1]"),
            Some(&LatticeType::Union({
                let mut expected = std::collections::BTreeSet::new();
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                )));
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String,
                )));
                expected
            }))
        );
    }

    #[test]
    fn test_split_env_nothing_check_index_access_does_not_narrow_issue_4270() {
        let mut env = TypeEnv::new();
        // arr[0] has type Union{Float64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        env.set("arr[0]", LatticeType::Union(union_types));

        // Condition: arr[0] === nothing
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Index {
                array: Box::new(Expr::Var("arr".to_string(), dummy_span())),
                indices: vec![Expr::Literal(Literal::Int(0), dummy_span())],
                span: dummy_span(),
            }),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Indexed loads from mutable containers are not MustAlias fields.
        assert_eq!(
            split.then_env.get("arr[0]"),
            Some(&LatticeType::Union({
                let mut expected = std::collections::BTreeSet::new();
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                )));
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                )));
                expected
            }))
        );

        assert_eq!(
            split.else_env.get("arr[0]"),
            Some(&LatticeType::Union({
                let mut expected = std::collections::BTreeSet::new();
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                )));
                expected.insert(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                )));
                expected
            }))
        );
    }
}
