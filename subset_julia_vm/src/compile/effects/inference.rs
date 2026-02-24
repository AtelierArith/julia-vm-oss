//! Effect inference for built-in operations and expressions.
//!
//! This module implements effect inference rules for built-in functions,
//! operators, and expression types.

use super::{EffectBit, Effects};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, UnaryOp};

/// Infer effects for a built-in operation by name and argument effects.
pub fn infer_builtin_effects(name: &str, arg_effects: &[Effects]) -> Effects {
    match name {
        // Pure arithmetic operations
        "+" | "-" | "*" | "/" | "^" | "//" | "div" | "rem" | "mod" | "%" => {
            infer_arithmetic_effects(arg_effects)
        }

        // Comparison operations (pure)
        "==" | "!=" | "<" | "<=" | ">" | ">=" | "===" | "!==" | "isequal" | "isless" => {
            Effects::pure_arithmetic()
        }

        // Boolean operations (pure)
        "!" | "&&" | "||" | "&" | "|" | "xor" => Effects::pure_arithmetic(),

        // Bitwise operations (pure)
        "<<" | ">>" | ">>>" => Effects::pure_arithmetic(),

        // Math functions (pure, but may throw for domain errors)
        // IMPORTANT: This list is an effect inference hint, independent of whether the
        // function is a Rust builtin or Pure Julia. Do NOT remove entries during
        // builtin migration. See Issue #2634 and docs/vm/BUILTIN_REMOVAL.md Layer 5.
        "sqrt" | "sin" | "cos" | "tan" | "exp" | "log" | "log10" | "log2" | "abs" | "sign"
        | "floor" | "ceil" | "round" | "trunc" | "isnan" | "isinf" | "isfinite" => {
            Effects::pure_arithmetic()
        }

        // Array indexing (getindex) - may throw BoundsError
        "getindex" => Effects::array_getindex(),

        // Array mutation (setindex!) - side effects
        "setindex!" => Effects::array_setindex(),

        // Array construction (pure)
        "zeros" | "ones" | "fill" | "similar" | "copy" => Effects::pure_arithmetic(),

        // Array properties (pure)
        "length" | "size" | "ndims" | "eltype" | "axes" | "eachindex" => Effects::pure_arithmetic(),

        // IO operations (side effects)
        "println" | "print" | "show" | "display" | "write" => Effects::with_side_effects(),

        // String operations (pure)
        "string" | "isempty" | "startswith" | "endswith" | "contains" | "replace" | "split"
        | "join" | "uppercase" | "lowercase" | "strip" | "lstrip" | "rstrip" => {
            Effects::pure_arithmetic()
        }

        // Type operations (pure)
        "typeof" | "isa" | "convert" | "promote_type" => Effects::pure_arithmetic(),

        // Tuple/NamedTuple operations (pure)
        "tuple" | "getfield" | "fieldnames" => Effects::pure_arithmetic(),

        // Collection operations (pure)
        "push!" | "pop!" | "append!" | "deleteat!" => Effects::array_setindex(), // Mutation

        // Iteration operations (pure)
        "iterate" | "first" | "last" | "collect" => Effects::pure_arithmetic(),

        // Range operations (pure)
        "range" | ":" | "step" => Effects::pure_arithmetic(),

        // Random number generation (side effects, modifies RNG state)
        "rand" | "randn" | "randexp" => Effects::with_side_effects(),

        // Default: conservative (arbitrary effects)
        _ => {
            // For unknown functions, merge argument effects conservatively
            merge_arg_effects(arg_effects)
        }
    }
}

/// Infer effects for arithmetic operations.
/// Arithmetic is pure unless arguments have side effects.
fn infer_arithmetic_effects(arg_effects: &[Effects]) -> Effects {
    if arg_effects.is_empty() {
        return Effects::pure_arithmetic();
    }

    // Merge all argument effects
    let mut result = Effects::pure_arithmetic();
    for arg_eff in arg_effects {
        result = result.merge(arg_eff);
    }
    result
}

/// Merge argument effects conservatively.
fn merge_arg_effects(arg_effects: &[Effects]) -> Effects {
    if arg_effects.is_empty() {
        return Effects::arbitrary();
    }

    let mut result = arg_effects[0];
    for arg_eff in &arg_effects[1..] {
        result = result.merge(arg_eff);
    }
    result
}

/// Merge a base effect with all argument effects conservatively.
fn merge_with_args(base: Effects, arg_effects: &[Effects]) -> Effects {
    arg_effects.iter().fold(base, |acc, arg| acc.merge(arg))
}

/// Infer effects for IR builtins (`Expr::Builtin`) by opcode.
fn infer_builtin_op_effects(op: &BuiltinOp, arg_effects: &[Effects]) -> Effects {
    let base = match op {
        // RNG/time and runtime-eval operations are effectful.
        BuiltinOp::Rand
        | BuiltinOp::Randn
        | BuiltinOp::Seed
        | BuiltinOp::TimeNs
        | BuiltinOp::Eval
        | BuiltinOp::EvalFile
        | BuiltinOp::IncludeString
        | BuiltinOp::MacroExpandBang
        | BuiltinOp::TestRecord
        | BuiltinOp::TestRecordBroken
        | BuiltinOp::TestSetBegin
        | BuiltinOp::TestSetEnd => Effects::with_side_effects(),

        // Mutating collection operations.
        BuiltinOp::Push
        | BuiltinOp::Pop
        | BuiltinOp::PushFirst
        | BuiltinOp::PopFirst
        | BuiltinOp::Insert
        | BuiltinOp::DeleteAt
        | BuiltinOp::DictDelete
        | BuiltinOp::DictMergeBang
        | BuiltinOp::DictEmpty
        | BuiltinOp::DictGetBang => Effects::array_setindex(),

        // Conservative default for non-mutating builtins.
        _ => Effects::pure_arithmetic(),
    };
    merge_with_args(base, arg_effects)
}

/// Infer effects for a binary operation.
pub fn infer_binary_op_effects(op: &BinaryOp, left: &Effects, right: &Effects) -> Effects {
    let merged = left.merge(right);

    match op {
        // Arithmetic operations - pure if operands are pure
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::IntDiv
        | BinaryOp::Mod
        | BinaryOp::Pow => {
            Effects {
                consistent: merged.consistent,
                effect_free: merged.effect_free,
                nothrow: merged.nothrow, // Division may throw DivideError
                terminates: true,
                notaskstate: merged.notaskstate,
                inaccessiblememonly: merged.inaccessiblememonly,
                noub: merged.noub,
                nonoverlayed: merged.nonoverlayed,
                nortcall: merged.nortcall,
            }
        }

        // Comparison operations - pure
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Le
        | BinaryOp::Gt
        | BinaryOp::Ge
        | BinaryOp::Egal    // === (object identity)
        | BinaryOp::NotEgal // !== (not object identity)
        | BinaryOp::Subtype => Effects::pure_arithmetic(), // <: (subtype check)

        // Boolean operations - pure
        BinaryOp::And | BinaryOp::Or => merged,
    }
}

/// Infer effects for a unary operation.
pub fn infer_unary_op_effects(op: &UnaryOp, operand: &Effects) -> Effects {
    match op {
        // Arithmetic negation - pure
        UnaryOp::Neg | UnaryOp::Pos => *operand,

        // Boolean negation - pure
        UnaryOp::Not => *operand,
    }
}

/// Infer effects for an expression.
pub fn infer_expr_effects(expr: &Expr) -> Effects {
    match expr {
        // Literals are pure and total
        Expr::Literal(_, _) => Effects::total(),

        // Variable references are pure (assuming no global mutation tracking)
        Expr::Var { .. } => Effects::pure_arithmetic(),

        // Binary operations
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let left_eff = infer_expr_effects(left);
            let right_eff = infer_expr_effects(right);
            infer_binary_op_effects(op, &left_eff, &right_eff)
        }

        // Unary operations
        Expr::UnaryOp { op, operand, .. } => {
            let operand_eff = infer_expr_effects(operand);
            infer_unary_op_effects(op, &operand_eff)
        }

        // Function calls
        Expr::Call { function, args, .. } => {
            let arg_effects: Vec<Effects> = args.iter().map(infer_expr_effects).collect();
            infer_builtin_effects(function, &arg_effects)
        }

        // Built-in calls
        Expr::Builtin { name, args, .. } => {
            let arg_effects: Vec<Effects> = args.iter().map(infer_expr_effects).collect();
            infer_builtin_op_effects(name, &arg_effects)
        }

        // Array literal - pure construction
        Expr::ArrayLiteral { elements, .. } => {
            let mut result = Effects::pure_arithmetic();
            for elem in elements {
                result = result.merge(&infer_expr_effects(elem));
            }
            result
        }

        // Tuple literal - pure construction
        Expr::TupleLiteral { elements, .. } => {
            let mut result = Effects::pure_arithmetic();
            for elem in elements {
                result = result.merge(&infer_expr_effects(elem));
            }
            result
        }

        // NamedTuple literal - pure construction
        Expr::NamedTupleLiteral { fields, .. } => {
            let mut result = Effects::pure_arithmetic();
            for (_, expr) in fields {
                result = result.merge(&infer_expr_effects(expr));
            }
            result
        }

        // Range - pure construction
        Expr::Range {
            start, stop, step, ..
        } => {
            let mut result = infer_expr_effects(start);
            result = result.merge(&infer_expr_effects(stop));
            if let Some(step_expr) = step {
                result = result.merge(&infer_expr_effects(step_expr));
            }
            result
        }

        // Let blocks - effects of the body
        Expr::LetBlock { .. } => {
            // Conservative: assume body may have arbitrary effects
            Effects::arbitrary()
        }

        // Function references - pure
        Expr::FunctionRef { .. } => Effects::pure_arithmetic(),

        // Index access - may throw BoundsError
        Expr::Index { array, indices, .. } => {
            let mut result = infer_expr_effects(array);
            for idx in indices {
                result = result.merge(&infer_expr_effects(idx));
            }
            // Add bounds check effect
            Effects {
                consistent: result.consistent,
                effect_free: result.effect_free,
                nothrow: false, // May throw BoundsError
                terminates: result.terminates,
                notaskstate: result.notaskstate,
                inaccessiblememonly: result.inaccessiblememonly,
                noub: false, // Out-of-bounds is undefined
                nonoverlayed: result.nonoverlayed,
                nortcall: result.nortcall,
            }
        }

        // Field access - pure
        Expr::FieldAccess { object, .. } => infer_expr_effects(object),

        // Comprehension - effects of the body
        Expr::Comprehension { body, .. } | Expr::MultiComprehension { body, .. } => {
            // Comprehension creates new array, so it's mostly pure
            // but body may have side effects
            infer_expr_effects(body)
        }

        // Generator - conservative (may iterate with effects)
        Expr::Generator { .. } => Effects::arbitrary(),

        // Ternary operator - merge all branches
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let mut result = infer_expr_effects(condition);
            result = result.merge(&infer_expr_effects(then_expr));
            result = result.merge(&infer_expr_effects(else_expr));
            result
        }

        // String concatenation - pure
        Expr::StringConcat { parts, .. } => {
            let mut result = Effects::pure_arithmetic();
            for part in parts {
                result = result.merge(&infer_expr_effects(part));
            }
            result
        }

        // Assignment expression - side effect
        Expr::AssignExpr { value, .. } => {
            let value_eff = infer_expr_effects(value);
            Effects {
                consistent: EffectBit::AlwaysFalse,
                effect_free: EffectBit::AlwaysFalse,
                nothrow: value_eff.nothrow,
                terminates: value_eff.terminates,
                notaskstate: value_eff.notaskstate,
                inaccessiblememonly: false, // Mutates variable
                noub: value_eff.noub,
                nonoverlayed: value_eff.nonoverlayed,
                nortcall: value_eff.nortcall,
            }
        }

        // Default: conservative
        _ => Effects::arbitrary(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::BuiltinOp;
    use crate::span::Span;

    fn test_span() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    #[test]
    fn test_infer_builtin_arithmetic() {
        let effects = infer_builtin_effects("+", &[]);
        assert!(effects.is_pure());
        assert!(effects.is_foldable());
    }

    #[test]
    fn test_infer_builtin_println() {
        let effects = infer_builtin_effects("println", &[]);
        assert!(!effects.is_pure());
        assert!(!effects.is_foldable());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_infer_builtin_getindex() {
        let effects = infer_builtin_effects("getindex", &[]);
        assert!(!effects.nothrow); // May throw BoundsError
        assert!(effects.consistent.is_always_true());
    }

    #[test]
    fn test_infer_builtin_setindex() {
        let effects = infer_builtin_effects("setindex!", &[]);
        assert!(!effects.is_pure());
        assert!(!effects.is_foldable());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_infer_binary_op_add() {
        let left = Effects::pure_arithmetic();
        let right = Effects::pure_arithmetic();
        let effects = infer_binary_op_effects(&BinaryOp::Add, &left, &right);
        assert!(effects.is_pure());
    }

    #[test]
    fn test_infer_unary_op_neg() {
        let operand = Effects::pure_arithmetic();
        let effects = infer_unary_op_effects(&UnaryOp::Neg, &operand);
        assert!(effects.is_pure());
    }

    #[test]
    fn test_merge_arg_effects() {
        let pure = Effects::pure_arithmetic();
        let side_effect = Effects::with_side_effects();
        let merged = merge_arg_effects(&[pure, side_effect]);
        assert!(!merged.is_pure());
    }

    #[test]
    fn test_expr_builtin_push_is_not_pure() {
        let expr = Expr::Builtin {
            name: BuiltinOp::Push,
            args: vec![],
            span: test_span(),
        };
        let effects = infer_expr_effects(&expr);
        assert!(!effects.is_pure());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_expr_builtin_rand_is_not_pure() {
        let expr = Expr::Builtin {
            name: BuiltinOp::Rand,
            args: vec![],
            span: test_span(),
        };
        let effects = infer_expr_effects(&expr);
        assert!(!effects.is_pure());
        assert!(effects.effect_free.is_always_false());
    }
}
