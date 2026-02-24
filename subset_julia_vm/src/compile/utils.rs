//! Utility functions for the compiler.
//!
//! Includes binary op conversion, jump relocation, literal default evaluation,
//! and default type inference for keyword parameters.

use crate::ir::core::{BinaryOp, Expr, Literal};
use crate::vm::value::ArrayElementType;
use crate::vm::{new_array_ref, ArrayValue, Instr, SymbolValue, Value, ValueType};

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
        "<=" => Some(BinaryOp::Le),
        ">=" => Some(BinaryOp::Ge),
        "==" => Some(BinaryOp::Eq),
        "!=" => Some(BinaryOp::Ne),
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
                Literal::Bool(b) => Value::I64(if *b { 1 } else { 0 }),
                Literal::Str(s) => Value::Str(s.clone()),
                Literal::Char(c) => Value::Char(*c),
                Literal::Nothing => Value::Nothing,
                Literal::Missing => Value::Missing,
                Literal::Undef => Value::Undef, // Required kwarg marker
                Literal::Module(name) => {
                    Value::Module(Box::new(crate::vm::value::ModuleValue::new(name.clone())))
                }
                Literal::Array(data, shape) => Value::Array(new_array_ref(ArrayValue::from_f64(
                    data.clone(),
                    shape.clone(),
                ))),
                Literal::ArrayI64(data, shape) => Value::Array(new_array_ref(
                    ArrayValue::from_i64(data.clone(), shape.clone()),
                )),
                Literal::ArrayBool(data, shape) => Value::Array(new_array_ref(
                    ArrayValue::from_bool(data.clone(), shape.clone()),
                )),
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
                    use crate::vm::value::RegexValue;
                    match RegexValue::new(pattern, flags) {
                        Ok(regex) => Value::Regex(regex),
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
        // For non-literal defaults, use sensible defaults
        _ => Value::I64(0),
    }
}

/// Check if an expression represents a required kwarg (no default)
pub(in crate::compile) fn is_required_kwarg(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Undef, _))
}

/// Infer the value type of an expression (for kwparam defaults).
pub(in crate::compile) fn infer_default_type(expr: &Expr) -> ValueType {
    match expr {
        Expr::Literal(lit, _) => match lit {
            Literal::Int(_) | Literal::Bool(_) => ValueType::I64,
            Literal::Int128(_) => ValueType::I128,
            Literal::BigInt(_) => ValueType::BigInt,
            Literal::BigFloat(_) => ValueType::BigFloat,
            Literal::Nothing => ValueType::Nothing,
            Literal::Missing => ValueType::Missing,
            Literal::Undef => ValueType::Any, // Required kwarg - type determined by annotation or call
            Literal::Module(_) => ValueType::Module,
            Literal::Float(_) => ValueType::F64,
            Literal::Float32(_) => ValueType::F32,
            Literal::Float16(_) => ValueType::F16,
            Literal::Str(_) => ValueType::Str,
            Literal::Char(_) => ValueType::Char,
            Literal::Array(_, _) => ValueType::ArrayOf(ArrayElementType::F64),
            Literal::ArrayI64(_, _) => ValueType::ArrayOf(ArrayElementType::I64),
            Literal::ArrayBool(_, _) => ValueType::ArrayOf(ArrayElementType::Bool),
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
        _ => ValueType::I64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Expr, Literal};
    use crate::vm::{Instr, ValueType};

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
            assert!(back.is_some(), "function_name_to_binary_op({name:?}) should succeed");
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
        assert!(matches!(function_name_to_binary_op("+"), Some(BinaryOp::Add)));
        assert!(matches!(function_name_to_binary_op("*"), Some(BinaryOp::Mul)));
        assert!(matches!(function_name_to_binary_op("=="), Some(BinaryOp::Eq)));
        assert!(matches!(function_name_to_binary_op("<:"), Some(BinaryOp::Subtype)));
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
            Instr::Jump(15),      // was at old_start=10, target=15 (relative=5) → new target=25
            Instr::JumpIfZero(12), // relative=2 → new target=22
        ];
        relocate_jumps(&mut code, 10, 20);
        assert!(matches!(code[0], Instr::Jump(25)));
        assert!(matches!(code[1], Instr::JumpIfZero(22)));
    }

    #[test]
    fn test_relocate_jumps_same_offset_is_identity() {
        // Moving from offset 5 to offset 5 should not change targets
        let mut code = vec![
            Instr::Jump(10),
            Instr::JumpIfZero(15),
        ];
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
        assert_eq!(infer_default_type(&expr), ValueType::I64); // Bool stored as I64
    }

    #[test]
    fn test_infer_default_type_literal_nothing() {
        let expr = Expr::Literal(Literal::Nothing, s());
        assert_eq!(infer_default_type(&expr), ValueType::Nothing);
    }

    #[test]
    fn test_infer_default_type_non_literal_fallback() {
        // Non-literal expressions default to I64
        let expr = Expr::Var("x".to_string(), s());
        assert_eq!(infer_default_type(&expr), ValueType::I64);
    }
}
