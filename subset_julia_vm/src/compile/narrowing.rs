//! Flow-sensitive local type narrowing for branch codegen (Issue #5181).
//!
//! The abstract-interpretation engine already computes conditional narrowing
//! facts (`compile/abstract_interp/conditional.rs`), but those facts only feed
//! return-type inference — the *codegen* path (`CoreCompiler::self.locals`,
//! keyed on [`ValueType`]) never saw them. As a result, a `then`-branch guarded
//! by `x isa Int64` still loaded `x` through `LoadAny` and dispatched its
//! arithmetic dynamically, even though the guard proves a concrete type.
//!
//! This module extracts the subset of guard facts that map cleanly onto a
//! concrete [`ValueType`] so the branch codegen can temporarily refine
//! `self.locals` (saving/restoring around the branch). The refinement is a pure,
//! conservative *then*-branch analysis:
//!
//! - `x isa T` (as `isa(x, T)` call, `Builtin(Isa)`, or the `x isa T`
//!   surface form) and `typeof(x) === T` / `typeof(x) == T` where `T` names a
//!   concrete type that has a [`ValueType`] representation narrow `x` to that
//!   type inside the `then` branch.
//! - `x === nothing` / `x !== nothing` narrow nullable two-member unions in the
//!   corresponding branch.
//! - `c1 && c2` accumulates the narrowings of both operands (both must hold in
//!   the `then` branch).
//! - `x isa T`, `typeof(x) === T`, and `x === nothing` also narrow the `else`
//!   branch when the current codegen type is a two-member `Union{T,U}`. In that
//!   case the negation is exactly `U`, so the branch can use typed
//!   loads/arithmetic without changing control flow (Issue #5077).
//!
//! Soundness rests on the VM's typed loads (`LoadI64`/`LoadF64`/...): each falls
//! back to reading the variable's actual slot via `load_slot_value_by_name`, so
//! a typed load only succeeds when the runtime value genuinely has that type —
//! which the `isa` guard guarantees for code reachable inside the branch.
//!
//! We deliberately narrow negated guards only for the exact two-member union
//! case above. The negation of `isa` over `Any`, wider unions, or `&&` generally
//! cannot be expressed as a single concrete `ValueType`, so attempting it would
//! be unsound or useless. `===`/`!== nothing` narrowing is left to the inference
//! engine for statement-level return inference, but expression-position ternary
//! codegen also needs the two-member-union opposite branch (`x === nothing ? a :
//! use(x)`) so the non-`Nothing` arm can use typed loads (Issue #5589).

use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::vm::ValueType;

/// Map a concrete Julia type *name* to the [`ValueType`] used for typed loads
/// and arithmetic specialization. Returns `None` for abstract types, unknown
/// names, or types whose `ValueType` offers no codegen specialization over
/// `Any` (so callers can cheaply skip them).
///
/// `struct_id_for` resolves a user struct name to its `type_id`, enabling
/// `x isa MyStruct` to narrow to `ValueType::Struct(id)`.
pub(super) fn value_type_for_type_name(
    name: &str,
    struct_id_for: impl Fn(&str) -> Option<usize>,
) -> Option<ValueType> {
    let vt = match name {
        "Int8" => ValueType::I8,
        "Int16" => ValueType::I16,
        "Int32" => ValueType::I32,
        "Int" if crate::types::native_int_type_name() == "Int32" => ValueType::I32,
        "Int64" | "Int" => ValueType::I64,
        "Int128" => ValueType::I128,
        "UInt8" => ValueType::U8,
        "UInt16" => ValueType::U16,
        "UInt32" => ValueType::U32,
        "UInt" if crate::types::native_uint_type_name() == "UInt32" => ValueType::U32,
        "UInt64" | "UInt" => ValueType::U64,
        "UInt128" => ValueType::U128,
        "Float16" => ValueType::F16,
        "Float32" => ValueType::F32,
        "Float64" => ValueType::F64,
        "Bool" => ValueType::Bool,
        "Char" => ValueType::Char,
        "String" => ValueType::Str,
        _ => {
            return struct_id_for(name).map(ValueType::Struct);
        }
    };
    Some(vt)
}

/// Extract the type-name operand of an `isa` guard, returning `(var_name, type_name)`
/// when the condition is exactly `x isa T` (in any of its IR spellings) with a
/// bare variable on the left and a bare type name on the right.
fn isa_guard_parts(condition: &Expr) -> Option<(&str, &str)> {
    let (val, ty) = match condition {
        // `isa(x, T)` lowered as an ordinary call.
        Expr::Call { function, args, .. } if function == "isa" && args.len() == 2 => {
            (&args[0], &args[1])
        }
        // `x isa T` / `isa(x, T)` lowered to the `Isa` builtin.
        Expr::Builtin { name, args, .. } if matches!(name, BuiltinOp::Isa) && args.len() == 2 => {
            (&args[0], &args[1])
        }
        _ => return None,
    };

    let var_name = match val {
        Expr::Var(name, _) => name.as_str(),
        _ => return None,
    };
    let type_name = match ty {
        Expr::Var(name, _) => name.as_str(),
        _ => return None,
    };
    Some((var_name, type_name))
}

fn typeof_guard_parts(condition: &Expr) -> Option<(&str, &str, bool)> {
    if let Expr::UnaryOp {
        op: UnaryOp::Not,
        operand,
        ..
    } = condition
    {
        let (var, type_name, type_holds_in_then) = typeof_guard_parts(operand)?;
        return Some((var, type_name, !type_holds_in_then));
    }

    let Expr::BinaryOp {
        op, left, right, ..
    } = condition
    else {
        return None;
    };

    let type_holds_in_then = match op {
        BinaryOp::Egal | BinaryOp::Eq => true,
        BinaryOp::NotEgal | BinaryOp::Ne => false,
        _ => return None,
    };

    if let Some(var_name) = typeof_value_var(left) {
        let type_name = type_name_expr(right)?;
        return Some((var_name, type_name, type_holds_in_then));
    }
    if let Some(var_name) = typeof_value_var(right) {
        let type_name = type_name_expr(left)?;
        return Some((var_name, type_name, type_holds_in_then));
    }
    None
}

fn nothing_guard_parts(condition: &Expr) -> Option<(&str, bool)> {
    if let Expr::UnaryOp {
        op: UnaryOp::Not,
        operand,
        ..
    } = condition
    {
        let (var, nothing_holds_in_then) = nothing_guard_parts(operand)?;
        return Some((var, !nothing_holds_in_then));
    }

    let Expr::BinaryOp {
        op, left, right, ..
    } = condition
    else {
        return None;
    };

    let nothing_holds_in_then = match op {
        BinaryOp::Egal => true,
        BinaryOp::NotEgal => false,
        _ => return None,
    };

    if is_nothing_expr(left) {
        if let Expr::Var(name, _) = right.as_ref() {
            return Some((name.as_str(), nothing_holds_in_then));
        }
    }
    if is_nothing_expr(right) {
        if let Expr::Var(name, _) = left.as_ref() {
            return Some((name.as_str(), nothing_holds_in_then));
        }
    }
    None
}

fn is_nothing_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Nothing, _))
        || matches!(expr, Expr::Var(name, _) if name == "nothing")
}

fn typeof_value_var(expr: &Expr) -> Option<&str> {
    let value = match expr {
        Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args,
            ..
        } if args.len() == 1 => args.first()?,
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
            && splat_mask.iter().all(|is_splat| !*is_splat)
            && kwargs_splat_mask.iter().all(|is_splat| !*is_splat) =>
        {
            args.first()?
        }
        _ => return None,
    };
    match value {
        Expr::Var(name, _) => Some(name.as_str()),
        _ => None,
    }
}

fn type_name_expr(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(name, _) => Some(name.as_str()),
        _ => None,
    }
}

/// Compute the `then`-branch local refinements implied by a branch condition.
///
/// Returns `(var_name, narrowed_value_type)` pairs to overlay onto
/// `CoreCompiler::self.locals` for the duration of the `then` branch. The list
/// is empty when the condition carries no codegen-actionable narrowing.
///
/// `&&` chains accumulate; later facts win on the same variable (Julia evaluates
/// left-to-right, so a rightmost guard on the same name is the most refined).
#[cfg(test)]
fn then_branch_narrowings(
    condition: &Expr,
    struct_id_for: &impl Fn(&str) -> Option<usize>,
) -> Vec<(String, ValueType)> {
    then_branch_narrowings_with_current(condition, &|_| None, struct_id_for)
}

pub(super) fn then_branch_narrowings_with_current(
    condition: &Expr,
    current_type_for: &impl Fn(&str) -> Option<ValueType>,
    struct_id_for: &impl Fn(&str) -> Option<usize>,
) -> Vec<(String, ValueType)> {
    let mut out = Vec::new();
    collect_then_narrowings(condition, current_type_for, struct_id_for, &mut out);
    out
}

/// Compute `else`-branch local refinements implied by a branch condition.
///
/// This is intentionally narrower than [`then_branch_narrowings`]. We only
/// produce facts for a direct `x isa T` guard when the current local type is a
/// two-member union and removing `T` leaves exactly one concrete `ValueType`.
/// That mirrors the first safe codegen slice of union splitting (Issue #5077).
pub(super) fn else_branch_narrowings(
    condition: &Expr,
    current_type_for: &impl Fn(&str) -> Option<ValueType>,
    struct_id_for: &impl Fn(&str) -> Option<usize>,
) -> Vec<(String, ValueType)> {
    if let Some((var, type_name)) = isa_guard_parts(condition) {
        return positive_guard_else_narrowings(var, type_name, current_type_for, struct_id_for);
    }

    if let Some((var, nothing_holds_in_then)) = nothing_guard_parts(condition) {
        return if nothing_holds_in_then {
            let current = current_type_for(var);
            current
                .as_ref()
                .and_then(|ty| two_member_union_remainder(ty, &ValueType::Nothing))
                .map(|remaining| vec![(var.to_string(), remaining)])
                .unwrap_or_default()
        } else {
            current_type_for(var)
                .as_ref()
                .and_then(|ty| union_contains_member(ty, &ValueType::Nothing))
                .map(|()| vec![(var.to_string(), ValueType::Nothing)])
                .unwrap_or_default()
        };
    }

    let Some((var, type_name, type_holds_in_then)) = typeof_guard_parts(condition) else {
        return Vec::new();
    };
    if type_holds_in_then {
        positive_guard_else_narrowings(var, type_name, current_type_for, struct_id_for)
    } else {
        type_name_else_narrowing(var, type_name, struct_id_for)
    }
}

pub(super) fn const_nothing_guard_bool(
    condition: &Expr,
    current_type_for: &impl Fn(&str) -> Option<ValueType>,
) -> Option<bool> {
    let (var, nothing_holds_in_then) = nothing_guard_parts(condition)?;
    let current = current_type_for(var)?;
    let is_nothing = match current {
        ValueType::Nothing => true,
        ValueType::Any | ValueType::Union(_) => return None,
        _ => false,
    };
    Some(if nothing_holds_in_then {
        is_nothing
    } else {
        !is_nothing
    })
}

fn positive_guard_else_narrowings(
    var: &str,
    type_name: &str,
    current_type_for: &impl Fn(&str) -> Option<ValueType>,
    struct_id_for: &impl Fn(&str) -> Option<usize>,
) -> Vec<(String, ValueType)> {
    let Some(target) = value_type_for_type_name(type_name, struct_id_for) else {
        return Vec::new();
    };
    positive_value_else_narrowings(var, &target, current_type_for)
}

fn positive_value_else_narrowings(
    var: &str,
    target: &ValueType,
    current_type_for: &impl Fn(&str) -> Option<ValueType>,
) -> Vec<(String, ValueType)> {
    let Some(current) = current_type_for(var) else {
        return Vec::new();
    };
    let Some(remaining) = two_member_union_remainder(&current, target) else {
        return Vec::new();
    };
    vec![(var.to_string(), remaining)]
}

fn type_name_else_narrowing(
    var: &str,
    type_name: &str,
    struct_id_for: &impl Fn(&str) -> Option<usize>,
) -> Vec<(String, ValueType)> {
    value_type_for_type_name(type_name, struct_id_for)
        .map(|ty| vec![(var.to_string(), ty)])
        .unwrap_or_default()
}

fn two_member_union_remainder(current: &ValueType, excluded: &ValueType) -> Option<ValueType> {
    let ValueType::Union(types) = current else {
        return None;
    };
    if types.len() != 2 || !types.iter().any(|ty| ty == excluded) {
        return None;
    }
    let remaining = types.iter().find(|ty| *ty != excluded)?.clone();
    match remaining {
        ValueType::Any | ValueType::Union(_) => None,
        ty => Some(ty),
    }
}

fn union_contains_member(current: &ValueType, member: &ValueType) -> Option<()> {
    let ValueType::Union(types) = current else {
        return None;
    };
    types.iter().any(|ty| ty == member).then_some(())
}

fn collect_then_narrowings(
    condition: &Expr,
    current_type_for: &impl Fn(&str) -> Option<ValueType>,
    struct_id_for: &impl Fn(&str) -> Option<usize>,
    out: &mut Vec<(String, ValueType)>,
) {
    match condition {
        Expr::BinaryOp {
            op: BinaryOp::And,
            left,
            right,
            ..
        } => {
            // `c1 && c2`: both hold in the then-branch.
            collect_then_narrowings(left, current_type_for, struct_id_for, out);
            collect_then_narrowings(right, current_type_for, struct_id_for, out);
        }
        _ => {
            if let Some((var, type_name)) = isa_guard_parts(condition) {
                if let Some(vt) = value_type_for_type_name(type_name, struct_id_for) {
                    // Replace any earlier fact for the same variable (rightmost wins).
                    out.retain(|(name, _)| name != var);
                    out.push((var.to_string(), vt));
                }
            } else if let Some((var, type_name, true)) = typeof_guard_parts(condition) {
                if let Some(vt) = value_type_for_type_name(type_name, struct_id_for) {
                    out.retain(|(name, _)| name != var);
                    out.push((var.to_string(), vt));
                }
            } else if let Some((var, nothing_holds_in_then)) = nothing_guard_parts(condition) {
                let narrowed = if nothing_holds_in_then {
                    current_type_for(var)
                        .as_ref()
                        .and_then(|ty| union_contains_member(ty, &ValueType::Nothing))
                        .map(|()| ValueType::Nothing)
                } else {
                    current_type_for(var)
                        .as_ref()
                        .and_then(|ty| two_member_union_remainder(ty, &ValueType::Nothing))
                };
                if let Some(vt) = narrowed {
                    out.retain(|(name, _)| name != var);
                    out.push((var.to_string(), vt));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::test_helpers::{call_expr, var_expr, zero_span};
    use crate::ir::core::Expr;

    fn no_structs(_: &str) -> Option<usize> {
        None
    }

    fn isa_call(var: &str, ty: &str) -> Expr {
        call_expr("isa", vec![var_expr(var), var_expr(ty)])
    }

    fn typeof_expr(var: &str) -> Expr {
        Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args: vec![var_expr(var)],
            span: zero_span(),
        }
    }

    fn typeof_cmp(var: &str, ty: &str, op: BinaryOp) -> Expr {
        Expr::BinaryOp {
            op,
            left: Box::new(typeof_expr(var)),
            right: Box::new(var_expr(ty)),
            span: zero_span(),
        }
    }

    fn reversed_typeof_cmp(ty: &str, var: &str, op: BinaryOp) -> Expr {
        Expr::BinaryOp {
            op,
            left: Box::new(var_expr(ty)),
            right: Box::new(typeof_expr(var)),
            span: zero_span(),
        }
    }

    fn and_expr(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(left),
            right: Box::new(right),
            span: zero_span(),
        }
    }

    #[test]
    fn isa_int64_narrows_var_to_i64() {
        let cond = isa_call("x", "Int64");
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn isa_float64_narrows_var_to_f64() {
        let cond = isa_call("y", "Float64");
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("y".to_string(), ValueType::F64)]);
    }

    #[test]
    fn isa_string_narrows_var_to_str() {
        let cond = isa_call("s", "String");
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("s".to_string(), ValueType::Str)]);
    }

    #[test]
    fn isa_builtin_form_is_recognized() {
        let cond = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![var_expr("x"), var_expr("Int")],
            span: zero_span(),
        };
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn typeof_egal_narrows_then_branch_to_concrete_type() {
        let cond = typeof_cmp("x", "Int64", BinaryOp::Egal);
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn typeof_eq_narrows_then_branch_to_concrete_type() {
        let cond = typeof_cmp("x", "Float64", BinaryOp::Eq);
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::F64)]);
    }

    #[test]
    fn reversed_typeof_egal_narrows_then_branch() {
        let cond = reversed_typeof_cmp("String", "x", BinaryOp::Egal);
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::Str)]);
    }

    #[test]
    fn typeof_noteq_does_not_narrow_then_branch_without_current_union() {
        let cond = typeof_cmp("x", "Int64", BinaryOp::NotEgal);
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert!(facts.is_empty());
    }

    #[test]
    fn and_of_two_isa_guards_accumulates() {
        let cond = and_expr(isa_call("x", "Int64"), isa_call("y", "Float64"));
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(
            facts,
            vec![
                ("x".to_string(), ValueType::I64),
                ("y".to_string(), ValueType::F64),
            ]
        );
    }

    #[test]
    fn rightmost_guard_on_same_var_wins() {
        let cond = and_expr(isa_call("x", "Int64"), isa_call("x", "Float64"));
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::F64)]);
    }

    #[test]
    fn abstract_type_name_yields_no_narrowing() {
        // `Number` / `Real` are abstract — no concrete ValueType, no narrowing.
        let cond = isa_call("x", "Number");
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert!(facts.is_empty());
    }

    #[test]
    fn non_guard_condition_yields_no_narrowing() {
        // `x > 0` is not an isa/identity guard.
        let cond = Expr::BinaryOp {
            op: BinaryOp::Gt,
            left: Box::new(var_expr("x")),
            right: Box::new(Expr::Literal(crate::ir::core::Literal::Int(0), zero_span())),
            span: zero_span(),
        };
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert!(facts.is_empty());
    }

    #[test]
    fn isa_with_non_var_subject_is_ignored() {
        // `isa(f(x), Int64)` — subject is not a bare variable, can't refine a local.
        let cond = call_expr(
            "isa",
            vec![call_expr("f", vec![var_expr("x")]), var_expr("Int64")],
        );
        let facts = then_branch_narrowings(&cond, &no_structs);
        assert!(facts.is_empty());
    }

    #[test]
    fn struct_type_narrows_to_struct_value_type() {
        let structs = |name: &str| if name == "MyStruct" { Some(42) } else { None };
        let cond = isa_call("p", "MyStruct");
        let facts = then_branch_narrowings(&cond, &structs);
        assert_eq!(facts, vec![("p".to_string(), ValueType::Struct(42))]);
    }

    #[test]
    fn else_branch_narrows_two_member_union_to_remaining_type() {
        let cond = isa_call("x", "Int64");
        let current = |name: &str| {
            if name == "x" {
                Some(ValueType::Union(vec![ValueType::I64, ValueType::Str]))
            } else {
                None
            }
        };
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::Str)]);
    }

    #[test]
    fn typeof_egal_else_branch_narrows_two_member_union_to_remaining_type() {
        let cond = typeof_cmp("x", "Int64", BinaryOp::Egal);
        let current = |name: &str| {
            if name == "x" {
                Some(ValueType::Union(vec![ValueType::I64, ValueType::Str]))
            } else {
                None
            }
        };
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::Str)]);
    }

    #[test]
    fn nothing_egal_else_branch_narrows_two_member_union_to_remaining_type_issue_5589() {
        let cond = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(var_expr("x")),
            right: Box::new(Expr::Literal(
                crate::ir::core::Literal::Nothing,
                zero_span(),
            )),
            span: zero_span(),
        };
        let current = |name: &str| {
            if name == "x" {
                Some(ValueType::Union(vec![ValueType::I64, ValueType::Nothing]))
            } else {
                None
            }
        };
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn nothing_identifier_else_branch_narrows_two_member_union_issue_5589() {
        let cond = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(var_expr("x")),
            right: Box::new(var_expr("nothing")),
            span: zero_span(),
        };
        let current = |name: &str| {
            if name == "x" {
                Some(ValueType::Union(vec![ValueType::I64, ValueType::Nothing]))
            } else {
                None
            }
        };
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn nothing_notegal_then_branch_narrows_nullable_union_to_remaining_type() {
        let cond = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(var_expr("x")),
            right: Box::new(var_expr("nothing")),
            span: zero_span(),
        };
        let current = |name: &str| {
            if name == "x" {
                Some(ValueType::Union(vec![ValueType::I64, ValueType::Nothing]))
            } else {
                None
            }
        };
        let facts = then_branch_narrowings_with_current(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn nothing_egal_then_branch_does_not_narrow_non_nullable_union() {
        let cond = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(var_expr("x")),
            right: Box::new(var_expr("nothing")),
            span: zero_span(),
        };
        let current = |_name: &str| Some(ValueType::Any);
        let facts = then_branch_narrowings_with_current(&cond, &current, &no_structs);
        assert!(facts.is_empty());
    }

    #[test]
    fn nothing_guard_const_bool_from_current_concrete_type() {
        let cond = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(var_expr("x")),
            right: Box::new(var_expr("nothing")),
            span: zero_span(),
        };
        let current_nothing = |_name: &str| Some(ValueType::Nothing);
        let current_i64 = |_name: &str| Some(ValueType::I64);
        let current_any = |_name: &str| Some(ValueType::Any);
        assert_eq!(
            const_nothing_guard_bool(&cond, &current_nothing),
            Some(true)
        );
        assert_eq!(const_nothing_guard_bool(&cond, &current_i64), Some(false));
        assert_eq!(const_nothing_guard_bool(&cond, &current_any), None);
    }

    #[test]
    fn typeof_noteq_else_branch_narrows_to_compared_type() {
        let cond = typeof_cmp("x", "Int64", BinaryOp::NotEgal);
        let current = |_name: &str| Some(ValueType::Any);
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn not_typeof_egal_else_branch_narrows_to_compared_type() {
        let cond = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(typeof_cmp("x", "Int64", BinaryOp::Egal)),
            span: zero_span(),
        };
        let current = |_name: &str| Some(ValueType::Any);
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert_eq!(facts, vec![("x".to_string(), ValueType::I64)]);
    }

    #[test]
    fn else_branch_narrowing_skips_wider_unions() {
        let cond = isa_call("x", "Int64");
        let current = |name: &str| {
            if name == "x" {
                Some(ValueType::Union(vec![
                    ValueType::I64,
                    ValueType::Str,
                    ValueType::Bool,
                ]))
            } else {
                None
            }
        };
        let facts = else_branch_narrowings(&cond, &current, &no_structs);
        assert!(facts.is_empty());
    }

    #[test]
    fn else_branch_narrowing_skips_any_or_non_union_current_type() {
        let cond = isa_call("x", "Int64");
        let any_current = |name: &str| {
            if name == "x" {
                Some(ValueType::Any)
            } else {
                None
            }
        };
        assert!(else_branch_narrowings(&cond, &any_current, &no_structs).is_empty());

        let i64_current = |name: &str| {
            if name == "x" {
                Some(ValueType::I64)
            } else {
                None
            }
        };
        assert!(else_branch_narrowings(&cond, &i64_current, &no_structs).is_empty());
    }

    #[test]
    fn else_branch_narrows_struct_union_to_remaining_type() {
        let structs = |name: &str| if name == "MyStruct" { Some(42) } else { None };
        let cond = isa_call("p", "MyStruct");
        let current = |name: &str| {
            if name == "p" {
                Some(ValueType::Union(vec![
                    ValueType::Struct(42),
                    ValueType::I64,
                ]))
            } else {
                None
            }
        };
        let facts = else_branch_narrowings(&cond, &current, &structs);
        assert_eq!(facts, vec![("p".to_string(), ValueType::I64)]);
    }
}
