//! Utility functions for the compiler.
//!
//! Includes binary op conversion, jump relocation, literal default evaluation,
//! and default type inference for keyword parameters.

#[cfg(test)]
use crate::bytecode::value::ArrayElementType;
#[cfg(test)]
use crate::bytecode::ValueType;
use crate::bytecode::{Instr, SymbolValue, Value};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};

/// Relocate jump targets in cached code from old absolute addresses to new absolute addresses.
/// `old_start` is the original code_start in the cached code array.
/// `new_start` is the new position in the current code array.
pub(in crate::compile) fn relocate_jumps(code: &mut [Instr], old_start: usize, new_start: usize) {
    for instr in code.iter_mut() {
        match instr {
            Instr::Jump(target) => {
                // Convert from absolute-in-old-code to relative, then to absolute-in-new-code
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfZero(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfNeI64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfEqI64(target) => {
                *target = (*target - old_start) + new_start;
            }
            // Directional comparison jumps may be emitted directly by the
            // constant-step `for` loop fast path (Issue #5166), so they too must be
            // relocated. (The peephole pass also produces them, but that runs after
            // relocation.)
            Instr::JumpIfLtI64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfGtI64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfGtI64Slots(_, _, target) => {
                *target = (*target - old_start) + new_start;
            }
            // Fused slot-vs-constant compare-and-branch (Issue #10105). Produced
            // by the peephole pass, so it appears in post-peephole cached code
            // (e.g. Base functions with constant loop guards) that this splice
            // relocation walks; its target must move with the rest.
            Instr::JumpIfCmpI64SlotConst(_, _, _, target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::AddConstI64SlotAndJumpIfLe(_, _, _, target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfLeI64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfGeI64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfEqF64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfNeF64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfNotLtF64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfNotGtF64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfNotLeF64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::JumpIfNotGeF64(target) => {
                *target = (*target - old_start) + new_start;
            }
            Instr::PushHandler(catch_ip, finally_ip) => {
                if let Some(ip) = catch_ip.as_mut() {
                    *ip = (*ip - old_start) + new_start;
                }
                if let Some(ip) = finally_ip.as_mut() {
                    *ip = (*ip - old_start) + new_start;
                }
            }
            _ => {}
        }
    }
}

/// Convert a BinaryOp to its corresponding function name for operator overloading.
pub(crate) fn binary_op_to_function_name(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::IntDiv => "÷",
        BinaryOp::Mod => "%",
        BinaryOp::Pow => "^",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Egal => "===",
        BinaryOp::NotEgal => "!==",
        BinaryOp::Subtype => "<:",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

/// Operator-name string of a unary operator, matching the keys of the shared
/// constant evaluator (`compile::const_prop::eval_const_unary`).
pub(crate) fn unary_op_to_function_name(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::Pos => "+",
    }
}

/// Convert an operator function name to a BinaryOp (inverse of binary_op_to_function_name).
/// Used for Base.:+ syntax.
pub(crate) fn function_name_to_binary_op(name: &str) -> Option<BinaryOp> {
    match name {
        "+" => Some(BinaryOp::Add),
        "-" => Some(BinaryOp::Sub),
        "*" => Some(BinaryOp::Mul),
        "/" => Some(BinaryOp::Div),
        "%" => Some(BinaryOp::Mod),
        "^" => Some(BinaryOp::Pow),
        "<" => Some(BinaryOp::Lt),
        ">" => Some(BinaryOp::Gt),
        "<=" | "≤" => Some(BinaryOp::Le),
        ">=" | "≥" => Some(BinaryOp::Ge),
        "==" => Some(BinaryOp::Eq),
        "!=" | "≠" => Some(BinaryOp::Ne),
        "===" => Some(BinaryOp::Egal),
        "!==" => Some(BinaryOp::NotEgal),
        "<:" => Some(BinaryOp::Subtype),
        "&&" => Some(BinaryOp::And),
        "||" => Some(BinaryOp::Or),
        _ => None,
    }
}

/// Evaluate a literal expression to a Value (for kwparam defaults).
/// Only supports literal values (non-literal defaults not evaluated).
pub(in crate::compile) fn eval_literal_default(expr: &Expr) -> Value {
    match expr {
        Expr::Literal(lit, _) => {
            match lit {
                Literal::Int(v) => Value::I64(*v),
                Literal::Int128(v) => Value::I128(*v),
                Literal::BigInt(s) => Value::BigInt(s.parse().unwrap_or_default()),
                Literal::BigFloat(s) => Value::BigFloat(s.parse().unwrap_or_default()),
                Literal::Float(v) => Value::F64(*v),
                Literal::Float32(v) => Value::F32(*v),
                Literal::Float16(v) => Value::F16(*v),
                Literal::Bool(b) => Value::Bool(*b),
                Literal::Str(s) => Value::str_new(s.clone()),
                Literal::StrBytes(bytes) => Value::str_from_bytes(bytes.clone()),
                Literal::Char(c) => Value::Char(*c),
                Literal::CharMalformed(bits) => Value::CharMalformed(*bits),
                Literal::Nothing => Value::Nothing,
                Literal::Missing => Value::Missing,
                Literal::Undef => Value::Undef, // Required kwarg marker
                Literal::Module(name) => Value::Module(Box::new(
                    crate::bytecode::value::ModuleValue::new(name.clone()),
                )),
                Literal::DataType(name) => {
                    Value::DataType(Box::new(crate::types::JuliaType::from_name_or_struct(name)))
                }
                // Array-literal defaults are re-evaluated per call in the real
                // frame (`lowering/function/kw_defaults.rs`, Issue #6876), so a
                // source `[...]` default (`Expr::ArrayLiteral`) never reaches the
                // pre-evaluated fast path: the kwsorter binds the `Undef`
                // sentinel and the body prologue materializes a fresh
                // `Array{T,N}` wrapper at runtime. These folded `Literal::Array*`
                // arms are therefore unreachable for real kw defaults; returning
                // the body-eval sentinel keeps `eval_literal_default` free of a
                // compile-time native-array carrier (Issue #6807).
                Literal::Array(_, _) | Literal::ArrayI64(_, _) | Literal::ArrayBool(_, _) => {
                    Value::Undef
                }
                // Struct defaults (including Complex{Float64}) handled via struct construction
                Literal::Struct(name, fields) => {
                    if name.starts_with("Complex") && fields.len() == 2 {
                        // Handle Complex as a special case for backwards compatibility
                        if let (Literal::Float(re), Literal::Float(im)) = (&fields[0], &fields[1]) {
                            return Value::new_complex(0, *re, *im);
                        }
                    }
                    Value::Nothing // Other struct defaults not supported in kwparams
                }
                // Metaprogramming literals
                Literal::Symbol(s) => Value::Symbol(SymbolValue::new(s)),
                Literal::Expr { .. } | Literal::QuoteNode(_) | Literal::LineNumberNode { .. } => {
                    Value::Nothing
                }
                // Regex literals
                Literal::Regex { pattern, flags } => {
                    use crate::bytecode::value::RegexValue;
                    match RegexValue::new(pattern, flags) {
                        Ok(regex) => Value::Regex(Box::new(regex)),
                        Err(_) => Value::Nothing,
                    }
                }
                // Enum literals
                Literal::Enum { type_name, value } => Value::Enum {
                    type_name: type_name.clone(),
                    value: *value,
                },
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            if let Some(symbol) = simple_symbol_quote_name(constructor) {
                Value::Symbol(SymbolValue::new(symbol))
            } else {
                Value::I64(0)
            }
        }
        // Bare floating-point special constants (`Inf`/`NaN` family, `pi`/`ℯ`)
        // used as a kwarg default. These resolve as Base global constants in
        // expression position but are not bound runtime globals, so without this
        // arm they fell through to the `Value::I64(0)` fallback (Issue #8078).
        Expr::Var(name, _) => super::float_special_constant_value(name).unwrap_or(Value::I64(0)),
        // Unary `-`/`+` over a pre-evaluable default (e.g. `-Inf`), so the baked
        // constant fallback stays correct even when the runtime mini-interpreter
        // is bypassed (Issue #8078).
        Expr::UnaryOp { op, operand, .. } => match (op, eval_literal_default(operand)) {
            (UnaryOp::Neg, Value::I64(v)) => Value::I64(-v),
            (UnaryOp::Neg, Value::F64(v)) => Value::F64(-v),
            (UnaryOp::Neg, Value::F32(v)) => Value::F32(-v),
            (UnaryOp::Neg, Value::F16(v)) => Value::F16(-v),
            (UnaryOp::Pos, v) => v,
            _ => Value::I64(0),
        },
        // For non-literal defaults, use sensible defaults
        _ => Value::I64(0),
    }
}

fn simple_symbol_quote_name(expr: &Expr) -> Option<&str> {
    let Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args,
        ..
    } = expr
    else {
        return None;
    };
    let [Expr::Literal(Literal::Str(symbol), _)] = args.as_slice() else {
        return None;
    };
    Some(symbol)
}

/// Check if an expression represents a required kwarg (no default)
pub(in crate::compile) fn is_required_kwarg(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Undef, _))
}

/// Infer the value type of an expression (for kwparam defaults).
#[cfg(test)]
pub(in crate::compile) fn infer_default_type(expr: &Expr) -> ValueType {
    match expr {
        Expr::Literal(lit, _) => match lit {
            Literal::Int(_) => ValueType::I64,
            Literal::Bool(_) => ValueType::Bool,
            Literal::Int128(_) => ValueType::I128,
            Literal::BigInt(_) => ValueType::BigInt,
            Literal::BigFloat(_) => ValueType::BigFloat,
            Literal::Nothing => ValueType::Nothing,
            Literal::Missing => ValueType::Missing,
            Literal::Undef => ValueType::Any, // Required kwarg - type determined by annotation or call
            Literal::Module(_) => ValueType::Module,
            Literal::DataType(_) => ValueType::DataType,
            Literal::Float(_) => ValueType::F64,
            Literal::Float32(_) => ValueType::F32,
            Literal::Float16(_) => ValueType::F16,
            Literal::Str(_) | Literal::StrBytes(_) => ValueType::Str,
            Literal::Char(_) | Literal::CharMalformed(_) => ValueType::Char,
            Literal::Array(_, _) => ValueType::ArrayOf(ArrayElementType::F64, None),
            Literal::ArrayI64(_, _) => ValueType::ArrayOf(ArrayElementType::I64, None),
            Literal::ArrayBool(_, _) => ValueType::ArrayOf(ArrayElementType::Bool, None),
            Literal::Struct(_, _) => ValueType::Any, // Struct (including Complex) type_id resolved during compilation
            // Metaprogramming literals
            Literal::Symbol(_) => ValueType::Symbol,
            Literal::Expr { .. } | Literal::QuoteNode(_) | Literal::LineNumberNode { .. } => {
                ValueType::Any
            }
            // Regex literal
            Literal::Regex { .. } => ValueType::Regex,
            // Enum literal
            Literal::Enum { .. } => ValueType::Enum,
        },
        Expr::QuoteLiteral { constructor, .. }
            if simple_symbol_quote_name(constructor).is_some() =>
        {
            ValueType::Symbol
        }
        Expr::BinaryOp {
            op, left, right, ..
        } => match op {
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => ValueType::Bool,
            BinaryOp::Div => ValueType::F64,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::IntDiv | BinaryOp::Mod => {
                let left_ty = infer_default_type(left);
                let right_ty = infer_default_type(right);
                if matches!(left_ty, ValueType::F64 | ValueType::F32 | ValueType::F16)
                    || matches!(right_ty, ValueType::F64 | ValueType::F32 | ValueType::F16)
                {
                    ValueType::F64
                } else {
                    ValueType::I64
                }
            }
            _ => ValueType::I64,
        },
        // A bare float special constant (`Inf`/`NaN` family, `pi`/`ℯ`) carries a
        // precise float type; any other variable reference is resolved at the
        // call site and stays `Any` (Issue #8078).
        Expr::Var(name, _) => match super::float_special_constant_value(name) {
            Some(Value::F32(_)) => ValueType::F32,
            Some(Value::F16(_)) => ValueType::F16,
            Some(_) => ValueType::F64,
            None => ValueType::Any,
        },
        // Unary `-`/`+` preserves the operand's numeric type (so a `-Inf` /
        // `-1.5` default keeps `Float64`, not the old `I64` fallback that
        // mis-typed the slot a `@kwdef`-generated inner constructor dispatches
        // on — Issue #8109, found while fixing #8078). `!` yields `Bool`.
        Expr::UnaryOp { op, operand, .. } => match op {
            UnaryOp::Neg | UnaryOp::Pos => infer_default_type(operand),
            UnaryOp::Not => ValueType::Bool,
        },
        Expr::Call { .. } | Expr::ModuleCall { .. } => ValueType::Any,
        _ => ValueType::I64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{Instr, ValueType};
    use crate::ir::core::{BinaryOp, Expr, Literal, UnaryOp};

    // ── binary_op_to_function_name / function_name_to_binary_op ─────────────

    #[test]
    fn test_binary_op_to_function_name_roundtrip() {
        let ops = [
            BinaryOp::Add,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Mod,
            BinaryOp::Pow,
            BinaryOp::Lt,
            BinaryOp::Gt,
            BinaryOp::Le,
            BinaryOp::Ge,
            BinaryOp::Eq,
            BinaryOp::Ne,
            BinaryOp::Egal,
            BinaryOp::NotEgal,
            BinaryOp::And,
            BinaryOp::Or,
        ];
        for op in &ops {
            let name = binary_op_to_function_name(op);
            let back = function_name_to_binary_op(name);
            assert!(
                back.is_some(),
                "function_name_to_binary_op({name:?}) should succeed"
            );
        }
    }

    #[test]
    fn test_binary_op_function_name_arithmetic() {
        assert_eq!(binary_op_to_function_name(&BinaryOp::Add), "+");
        assert_eq!(binary_op_to_function_name(&BinaryOp::Sub), "-");
        assert_eq!(binary_op_to_function_name(&BinaryOp::Mul), "*");
        assert_eq!(binary_op_to_function_name(&BinaryOp::Div), "/");
    }

    #[test]
    fn test_function_name_to_binary_op_known() {
        assert!(matches!(
            function_name_to_binary_op("+"),
            Some(BinaryOp::Add)
        ));
        assert!(matches!(
            function_name_to_binary_op("*"),
            Some(BinaryOp::Mul)
        ));
        assert!(matches!(
            function_name_to_binary_op("=="),
            Some(BinaryOp::Eq)
        ));
        assert!(matches!(
            function_name_to_binary_op("≤"),
            Some(BinaryOp::Le)
        ));
        assert!(matches!(
            function_name_to_binary_op("≥"),
            Some(BinaryOp::Ge)
        ));
        assert!(matches!(
            function_name_to_binary_op("≠"),
            Some(BinaryOp::Ne)
        ));
        assert!(matches!(
            function_name_to_binary_op("<:"),
            Some(BinaryOp::Subtype)
        ));
    }

    #[test]
    fn test_function_name_to_binary_op_unknown() {
        assert!(function_name_to_binary_op("unknown").is_none());
        assert!(function_name_to_binary_op("").is_none());
        assert!(function_name_to_binary_op("÷").is_none()); // IntDiv is not in the inverse map
    }

    // ── relocate_jumps ───────────────────────────────────────────────────────

    #[test]
    fn test_relocate_jumps_basic() {
        // Code originally at offset 10 is moved to offset 20
        let mut code = vec![
            Instr::Jump(15),       // was at old_start=10, target=15 (relative=5) → new target=25
            Instr::JumpIfZero(12), // relative=2 → new target=22
        ];
        relocate_jumps(&mut code, 10, 20);
        assert!(matches!(code[0], Instr::Jump(25)));
        assert!(matches!(code[1], Instr::JumpIfZero(22)));
    }

    #[test]
    fn test_relocate_jumps_same_offset_is_identity() {
        // Moving from offset 5 to offset 5 should not change targets
        let mut code = vec![Instr::Jump(10), Instr::JumpIfZero(15)];
        relocate_jumps(&mut code, 5, 5);
        assert!(matches!(code[0], Instr::Jump(10)));
        assert!(matches!(code[1], Instr::JumpIfZero(15)));
    }

    #[test]
    fn test_relocate_jumps_push_handler() {
        let mut code = vec![Instr::PushHandler(Some(15), Some(20))];
        relocate_jumps(&mut code, 10, 0);
        // old_start=10, new_start=0: new_target = (target - 10) + 0 = target - 10
        assert!(matches!(code[0], Instr::PushHandler(Some(5), Some(10))));
    }

    #[test]
    fn test_relocate_jumps_push_handler_none() {
        let mut code = vec![Instr::PushHandler(None, None)];
        relocate_jumps(&mut code, 10, 20);
        // None handlers should remain None
        assert!(matches!(code[0], Instr::PushHandler(None, None)));
    }

    #[test]
    fn test_relocate_jumps_non_jump_unchanged() {
        let mut code = vec![Instr::AddI64, Instr::SubI64, Instr::ReturnI64];
        let original = code.clone();
        relocate_jumps(&mut code, 0, 10);
        // Non-jump instructions are not modified
        assert_eq!(code.len(), original.len());
        assert!(matches!(code[0], Instr::AddI64));
    }

    // ── infer_default_type ───────────────────────────────────────────────────

    fn s() -> crate::span::Span {
        crate::span::Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_eval_literal_default_array_literals_defer_to_body_eval() {
        // Array-literal kw defaults are re-evaluated per call in the real frame
        // (Issue #6876), so `eval_literal_default` no longer materializes a
        // compile-time native-array carrier for them (Issue #6807) — it returns
        // the `Undef` body-eval sentinel. (A real source `[...]` default reaches
        // lowering as `Expr::ArrayLiteral` and is marked body-evaluated before
        // this fast path is consulted; these folded `Literal::Array*` forms only
        // appear in hand-built test exprs.)
        let float_expr = Expr::Literal(Literal::Array(vec![1.0, 2.0], vec![2]), s());
        let int_expr = Expr::Literal(Literal::ArrayI64(vec![1, 2], vec![2]), s());
        let bool_expr = Expr::Literal(Literal::ArrayBool(vec![true, false], vec![2]), s());

        assert!(matches!(eval_literal_default(&float_expr), Value::Undef));
        assert!(matches!(eval_literal_default(&int_expr), Value::Undef));
        assert!(matches!(eval_literal_default(&bool_expr), Value::Undef));
    }

    #[test]
    fn test_infer_default_type_literal_int() {
        let expr = Expr::Literal(Literal::Int(42), s());
        assert_eq!(infer_default_type(&expr), ValueType::I64);
    }

    #[test]
    fn test_infer_default_type_literal_float() {
        let expr = Expr::Literal(Literal::Float(1.25), s());
        assert_eq!(infer_default_type(&expr), ValueType::F64);
    }

    #[test]
    fn test_infer_default_type_literal_string() {
        let expr = Expr::Literal(Literal::Str("hello".to_string()), s());
        assert_eq!(infer_default_type(&expr), ValueType::Str);
    }

    #[test]
    fn test_infer_default_type_literal_bool() {
        let expr = Expr::Literal(Literal::Bool(true), s());
        assert!(matches!(eval_literal_default(&expr), Value::Bool(true)));
        assert_eq!(infer_default_type(&expr), ValueType::Bool);
    }

    #[test]
    fn test_issue_4297_quoted_symbol_kw_default_value_and_type() {
        let expr = Expr::QuoteLiteral {
            constructor: Box::new(Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("default".to_string()), s())],
                span: s(),
            }),
            span: s(),
        };

        assert!(matches!(
            eval_literal_default(&expr),
            Value::Symbol(symbol) if symbol.as_str() == "default"
        ));
        assert_eq!(infer_default_type(&expr), ValueType::Symbol);
    }

    #[test]
    fn test_infer_default_type_literal_nothing() {
        let expr = Expr::Literal(Literal::Nothing, s());
        assert_eq!(infer_default_type(&expr), ValueType::Nothing);
    }

    #[test]
    fn test_infer_default_type_non_literal_fallback() {
        // Non-literal defaults are evaluated at call time, so the static slot
        // must not claim stale Int64 precision.
        let expr = Expr::Var("x".to_string().into(), s());
        assert_eq!(infer_default_type(&expr), ValueType::Any);
    }

    // ── Issue #8078: float special constants as kwarg defaults ────────────────

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string().into(), s())
    }

    fn neg(operand: Expr) -> Expr {
        Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand: Box::new(operand),
            span: s(),
        }
    }

    #[test]
    fn test_eval_literal_default_inf_nan_constants_8078() {
        // A bare `Inf`/`NaN` (etc.) kwarg default resolves to the float special
        // constant instead of the old `Value::I64(0)` fallback (Issue #8078).
        assert!(matches!(
            eval_literal_default(&var("Inf")),
            Value::F64(v) if v.is_infinite() && v > 0.0
        ));
        assert!(matches!(
            eval_literal_default(&var("Inf64")),
            Value::F64(v) if v.is_infinite() && v > 0.0
        ));
        assert!(matches!(
            eval_literal_default(&var("Inf32")),
            Value::F32(v) if v.is_infinite() && v > 0.0
        ));
        assert!(matches!(
            eval_literal_default(&var("NaN")),
            Value::F64(v) if v.is_nan()
        ));
        assert!(matches!(
            eval_literal_default(&var("NaN32")),
            Value::F32(v) if v.is_nan()
        ));
        assert!(matches!(
            eval_literal_default(&var("Inf16")),
            Value::F16(v) if v.is_infinite()
        ));
        // A non-constant variable reference still uses the body-eval fallback.
        assert!(matches!(eval_literal_default(&var("x")), Value::I64(0)));
    }

    #[test]
    fn test_eval_literal_default_negated_inf_8078() {
        // `-Inf` / `-1.5` keep their float type and sign in the baked constant
        // fallback (Issue #8078).
        assert!(matches!(
            eval_literal_default(&neg(var("Inf"))),
            Value::F64(v) if v.is_infinite() && v < 0.0
        ));
        assert!(matches!(
            eval_literal_default(&neg(var("Inf32"))),
            Value::F32(v) if v.is_infinite() && v < 0.0
        ));
        assert!(matches!(
            eval_literal_default(&neg(Expr::Literal(Literal::Float(1.5), s()))),
            Value::F64(v) if v == -1.5
        ));
        assert!(matches!(
            eval_literal_default(&neg(Expr::Literal(Literal::Int(3), s()))),
            Value::I64(-3)
        ));
    }

    #[test]
    fn test_infer_default_type_inf_nan_constants_8078() {
        // Float special constants and unary negation preserve a precise float
        // type so a `@kwdef`-generated inner constructor dispatches correctly
        // (Issue #8078).
        assert_eq!(infer_default_type(&var("Inf")), ValueType::F64);
        assert_eq!(infer_default_type(&var("Inf32")), ValueType::F32);
        assert_eq!(infer_default_type(&var("Inf16")), ValueType::F16);
        assert_eq!(infer_default_type(&var("NaN")), ValueType::F64);
        assert_eq!(infer_default_type(&neg(var("Inf"))), ValueType::F64);
        assert_eq!(infer_default_type(&neg(var("Inf32"))), ValueType::F32);
        assert_eq!(
            infer_default_type(&neg(Expr::Literal(Literal::Float(1.5), s()))),
            ValueType::F64
        );
        // Unary minus over an Int default stays Int64.
        assert_eq!(
            infer_default_type(&neg(Expr::Literal(Literal::Int(3), s()))),
            ValueType::I64
        );
    }
}
