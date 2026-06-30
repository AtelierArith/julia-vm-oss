//! Unary operation compilation.
//!
//! Handles compilation of:
//! - Unary operators (negation, not, positive)
//! - Short-circuit logical operators (&&, ||)

use crate::builtins::BuiltinId;
use crate::intrinsics::Intrinsic;
use crate::ir::core::{Expr, UnaryOp};
use crate::lowering::expr::make_broadcasted_call;
use crate::span::Span;
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::super::type_helpers::short_circuit_result_type;
use super::super::{err, is_base_function, is_random_function, CResult, CoreCompiler};

/// Check if an expression is a call to a function that never returns normally
/// (e.g., `throw(...)`, `error(...)`, `rethrow(...)`).
/// These are equivalent to Julia's `Union{}` (bottom type) — they always throw
/// an exception, so their "return type" is irrelevant in short-circuit contexts.
/// See Issue #2598.
fn is_never_returning_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call { function, .. } => {
            matches!(function.as_str(), "throw" | "error" | "rethrow")
        }
        _ => false,
    }
}

impl CoreCompiler<'_> {
    pub(in super::super) fn compile_unary_op(
        &mut self,
        op: &UnaryOp,
        operand: &Expr,
        span: Span,
    ) -> CResult<ValueType> {
        if matches!(op, UnaryOp::Not) && self.compile_unary_not_operand_is_callable(operand) {
            self.emit(Instr::PushFunction("!".to_string()));
            self.compile_expr(operand)?;
            self.emit(Instr::CallBuiltin(BuiltinId::Compose, 2));
            return Ok(ValueType::Function);
        }

        // Handle unary operations on missing literals
        // In Julia, -missing returns missing (propagation of unknown value)
        if matches!(op, UnaryOp::Neg | UnaryOp::Pos) {
            let is_missing_lit =
                matches!(operand, Expr::Literal(crate::ir::core::Literal::Missing, _));
            if is_missing_lit {
                // For missing literals, just push missing as result
                self.emit(Instr::PushMissing);
                return Ok(ValueType::Missing);
            }
        }

        // Check for user-defined unary operator (like -z for Complex)
        if matches!(op, UnaryOp::Neg) {
            let operand_julia_ty = self.infer_julia_type(operand);
            if matches!(operand_julia_ty, JuliaType::Struct(_)) {
                // Try to find a matching unary - method
                if let Some(table) = self.method_tables.get("-") {
                    let arg_types = vec![operand_julia_ty.clone()];
                    if let Ok(method) = table.dispatch(&arg_types) {
                        // Compile operand and call the Pure Julia method
                        self.compile_expr(operand)?;
                        self.emit(Instr::Call(method.global_index, 1));
                        return Ok(method.return_type.clone());
                    }
                }
            }
        }

        if matches!(op, UnaryOp::Neg)
            && matches!(
                self.infer_expr_type(operand),
                ValueType::Array | ValueType::ArrayOf(_, _)
            )
        {
            let broadcasted_neg = make_broadcasted_call("-", vec![operand.clone()], span);
            return self.compile_expr(&broadcasted_neg);
        }

        let ty = self.compile_expr(operand)?;
        match (op, &ty) {
            // Negation -> neg_int, neg_float (returns same type as operand)
            (UnaryOp::Neg, ValueType::I64) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegInt));
                Ok(ValueType::I64)
            }
            (UnaryOp::Neg, ValueType::F64) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegFloat));
                Ok(ValueType::F64)
            }
            (UnaryOp::Neg, ValueType::F32) => {
                // Use NegAny which handles F32 and preserves the type
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::F32)
            }
            (UnaryOp::Neg, ValueType::F16) => {
                // Use NegAny which handles F16 and preserves the type (Issue #1972)
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::F16)
            }
            // Issue #3705: narrow integer and 128-bit types must dispatch through
            // NegAny which now preserves each type. Without these arms the match
            // fell into the wildcard error case below.
            (UnaryOp::Neg, ValueType::I8) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::I8)
            }
            (UnaryOp::Neg, ValueType::I16) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::I16)
            }
            (UnaryOp::Neg, ValueType::I32) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::I32)
            }
            (UnaryOp::Neg, ValueType::I128) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::I128)
            }
            (UnaryOp::Neg, ValueType::U8) => {
                // Julia: -UInt8(5) == UInt8(251) (two's-complement wrap, preserves type)
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::U8)
            }
            (UnaryOp::Neg, ValueType::U16) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::U16)
            }
            (UnaryOp::Neg, ValueType::U32) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::U32)
            }
            (UnaryOp::Neg, ValueType::U64) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::U64)
            }
            (UnaryOp::Neg, ValueType::U128) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::U128)
            }
            (UnaryOp::Neg, ValueType::BigInt) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::BigInt)
            }
            (UnaryOp::Neg, ValueType::BigFloat) => {
                self.emit(Instr::CallIntrinsic(Intrinsic::NegAny));
                Ok(ValueType::BigFloat)
            }
            // For Any type, use DynamicNeg which handles both primitives and structs.
            // DynamicNeg tries Julia dispatch for struct types (e.g., Rational, Complex),
            // falling back to inline negation for primitives.
            (UnaryOp::Neg, ValueType::Any) => {
                self.emit(Instr::DynamicNeg);
                Ok(ValueType::Any)
            }
            // Bool negation: convert Bool to I64, negate, convert back
            // Julia: -true == -1, -false == 0
            (UnaryOp::Neg, ValueType::Bool) => {
                self.emit(Instr::BoolToI64);
                self.emit(Instr::CallIntrinsic(Intrinsic::NegInt));
                Ok(ValueType::I64)
            }
            // For Struct types, the dispatch should have been handled above at line 1404-1417
            // If we get here, it means dispatch failed
            (UnaryOp::Neg, ValueType::Struct(_)) => {
                err("Unsupported struct negation - no Base.:- method found".to_string())
            }
            // NOT operations always return Bool
            (UnaryOp::Not, ValueType::Bool) => {
                self.emit(Instr::NotBool);
                Ok(ValueType::Bool)
            }
            (UnaryOp::Not, ValueType::I64) => {
                // NOT on I64: !x = (x == 0) -> use eq_int
                self.emit(Instr::PushI64(0));
                self.emit(Instr::CallIntrinsic(Intrinsic::EqInt));
                Ok(ValueType::Bool)
            }
            (UnaryOp::Not, ValueType::Any) => {
                // For Any type (e.g., user-defined function returning Bool),
                // convert to Bool first, then apply NotBool
                self.emit(Instr::DynamicToBool);
                self.emit(Instr::NotBool);
                Ok(ValueType::Bool)
            }
            (UnaryOp::Not, _) => {
                // Fallback for other types: treat as I64
                self.emit(Instr::PushI64(0));
                self.emit(Instr::CallIntrinsic(Intrinsic::EqInt));
                Ok(ValueType::Bool)
            }
            // Unary + is a no-op, preserves operand type
            (UnaryOp::Pos, _) => Ok(ty),
            _ => err(format!("Unsupported unary op: {:?}", op)),
        }
    }

    fn compile_unary_not_operand_is_callable(&mut self, operand: &Expr) -> bool {
        if self.infer_expr_type(operand) == ValueType::Function {
            return true;
        }

        match operand {
            Expr::FunctionRef { .. } => true,
            Expr::Var(name, _) if !self.locals.contains_key(name) => {
                self.method_tables.contains_key(name)
                    || is_base_function(name)
                    || self.function_aliases.contains_key(name)
                    || (self.usings.contains("Random") && is_random_function(name))
            }
            _ => false,
        }
    }

    pub(in super::super) fn compile_and_expr(
        &mut self,
        left: &Expr,
        right: &Expr,
    ) -> CResult<ValueType> {
        // a && b: evaluate left; if false, return false; else evaluate right.
        // The left operand is a *condition*: compile it with its natural type
        // (no `I64ToBool` coercion) so `JumpIfZero` enforces Julia's Bool-only
        // boolean-context rule and raises `TypeError: non-boolean (T) used in
        // boolean context` for a non-Bool operand such as `1 && true`. Branch
        // position is already strict via `compile_condition_false_jumps`; this
        // closes the value-position gap (Issue #6162).
        self.compile_expr(left)?;
        let j_false = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));

        // Left was true, evaluate right
        // For control flow expressions (return/break/continue) and never-returning
        // calls (throw/error/rethrow), don't require Bool conversion.
        // These expressions don't "return" - control transfers away. (Issue #2598)
        let is_control_flow = matches!(
            right,
            Expr::ReturnExpr { .. } | Expr::BreakExpr { .. } | Expr::ContinueExpr { .. }
        ) || is_never_returning_call(right);
        // The right operand is in *value* position: Julia's `a && b` returns `b`
        // as-is when `a` is true (and `false` otherwise), so compile it with its
        // natural type instead of coercing to Bool. When that type isn't Bool the
        // whole expression widens to Any, since the false path contributes a Bool
        // (Issue #6278; follow-up to #6162).
        let locals_before_right = self.locals.clone();
        let julia_type_locals_before_right = self.julia_type_locals.clone();
        let then_restore = self.apply_then_narrowings(left);
        let right_result = if is_control_flow {
            self.compile_expr(right).map(|_| {
                // Control flow executed - we won't reach here at runtime, but push a
                // dummy value to keep the stack consistent with the false path.
                self.emit(Instr::PushBool(false));
                ValueType::Bool
            })
        } else {
            self.compile_expr(right).map(short_circuit_result_type)
        };
        self.restore_short_circuit_narrowings(then_restore);
        self.locals = locals_before_right;
        self.julia_type_locals = julia_type_locals_before_right;
        let result_ty = right_result?;
        let j_end = self.here();
        self.emit(Instr::Jump(usize::MAX));

        // Left was false, push false
        let false_start = self.here();
        self.patch_jump(j_false, false_start);
        self.emit(Instr::PushBool(false));

        let end = self.here();
        self.patch_jump(j_end, end);
        Ok(result_ty)
    }

    pub(in super::super) fn compile_or_expr(
        &mut self,
        left: &Expr,
        right: &Expr,
    ) -> CResult<ValueType> {
        // a || b: evaluate left; if true, return true; else evaluate right.
        // The left operand is a *condition*: compile it with its natural type
        // (no `I64ToBool` coercion) so `JumpIfZero` enforces Julia's Bool-only
        // boolean-context rule and raises `TypeError: non-boolean (T) used in
        // boolean context` for a non-Bool operand such as `1 || false`
        // (Issue #6162).
        self.compile_expr(left)?;
        let j_eval_right = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));

        // Left was true, push true
        self.emit(Instr::PushBool(true));
        let j_end = self.here();
        self.emit(Instr::Jump(usize::MAX));

        // Left was false, evaluate right
        // For control flow expressions and never-returning calls, don't require Bool conversion
        // (Issue #2598)
        let right_start = self.here();
        self.patch_jump(j_eval_right, right_start);
        let is_control_flow = matches!(
            right,
            Expr::ReturnExpr { .. } | Expr::BreakExpr { .. } | Expr::ContinueExpr { .. }
        ) || is_never_returning_call(right);
        // The right operand is in *value* position: Julia's `a || b` returns `b`
        // as-is when `a` is false (and `true` otherwise), so compile it with its
        // natural type instead of coercing to Bool. When that type isn't Bool the
        // whole expression widens to Any, since the true path contributes a Bool
        // (Issue #6278; follow-up to #6162).
        let locals_before_right = self.locals.clone();
        let julia_type_locals_before_right = self.julia_type_locals.clone();
        let else_restore = self.apply_else_narrowings(left);
        let right_result = if is_control_flow {
            self.compile_expr(right).map(|_| {
                self.emit(Instr::PushBool(false)); // Dummy value for stack consistency
                ValueType::Bool
            })
        } else {
            self.compile_expr(right).map(short_circuit_result_type)
        };
        self.restore_short_circuit_narrowings(else_restore);
        self.locals = locals_before_right;
        self.julia_type_locals = julia_type_locals_before_right;
        let result_ty = right_result?;

        let end = self.here();
        self.patch_jump(j_end, end);
        Ok(result_ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::Literal;
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn make_call(name: &str) -> Expr {
        Expr::Call {
            function: name.to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: s(),
        }
    }

    // ── is_never_returning_call ───────────────────────────────────────────────

    #[test]
    fn test_throw_call_is_never_returning() {
        assert!(is_never_returning_call(&make_call("throw")));
    }

    #[test]
    fn test_error_call_is_never_returning() {
        assert!(is_never_returning_call(&make_call("error")));
    }

    #[test]
    fn test_rethrow_call_is_never_returning() {
        assert!(is_never_returning_call(&make_call("rethrow")));
    }

    #[test]
    fn test_regular_call_is_not_never_returning() {
        assert!(!is_never_returning_call(&make_call("println")));
    }

    #[test]
    fn test_literal_is_not_never_returning() {
        let lit = Expr::Literal(Literal::Int(42), s());
        assert!(!is_never_returning_call(&lit));
    }

    #[test]
    fn test_var_is_not_never_returning() {
        let var = Expr::Var("x".to_string(), s());
        assert!(!is_never_returning_call(&var));
    }
}
