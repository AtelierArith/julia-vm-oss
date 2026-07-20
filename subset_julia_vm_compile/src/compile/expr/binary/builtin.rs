//! Builtin binary operator compilation.
//!
//! Handles compilation of binary operations using only builtin/intrinsic operators,
//! bypassing user-defined operator overloads. Used for `Base.:+` syntax.

use crate::bytecode::{Instr, ValueType};
use crate::intrinsics::Intrinsic;
use crate::ir::core::{BinaryOp, Expr};

use crate::compile::{err, CResult, CoreCompiler};

use super::{
    binary_compare_check, same_small_int_type, small_int_back_conversion, typed_instr_for_intrinsic,
};

impl CoreCompiler<'_> {
    /// Compile a binary operation using builtin operators only.
    /// This is used for Base.:+ syntax to bypass user-defined operator overloads.
    pub(in crate::compile) fn compile_builtin_binary_op(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> CResult<ValueType> {
        if matches!(op, BinaryOp::And) {
            return self.compile_and_expr(left, right);
        }
        if matches!(op, BinaryOp::Or) {
            return self.compile_or_expr(left, right);
        }

        let left_ty = self.infer_expr_type(left);
        let right_ty = self.infer_expr_type(right);

        // Compare-mode annotation (Issue #8620): compile-time chose UniqueBuiltin for
        // this call site.  Log if the resolver disagrees.
        binary_compare_check(op, &left_ty, &right_ty, "UniqueBuiltin");

        // Issue #9343: Bool "strong zero" multiply. `*(::Bool, ::AbstractFloat)`
        // returns `copysign(zero(y), y)` when the Bool is `false`, so
        // `false * Inf == 0.0` (not NaN). The typed specialization below widens
        // the Bool to a float (`BoolToI64; ToF64; MulF64`), computing
        // `0.0 * Inf == NaN`. Route Bool × machine-float multiply to dynamic
        // dispatch, where the VM's strong-zero arm preserves the correct value.
        let is_machine_float =
            |t: &ValueType| matches!(t, ValueType::F16 | ValueType::F32 | ValueType::F64);
        if matches!(op, BinaryOp::Mul)
            && ((left_ty == ValueType::Bool && is_machine_float(&right_ty))
                || (right_ty == ValueType::Bool && is_machine_float(&left_ty)))
        {
            self.emit_dynamic_binary_both_op(op, left, right)?;
            // Result is the float operand's type (Bool promotes to the float).
            let float_ty = if left_ty == ValueType::Bool {
                right_ty
            } else {
                left_ty
            };
            return Ok(float_ty);
        }
        if matches!(op, BinaryOp::Add)
            && ((left_ty == ValueType::Bool && is_machine_float(&right_ty))
                || (right_ty == ValueType::Bool && is_machine_float(&left_ty)))
        {
            self.emit_dynamic_binary_both_op(op, left, right)?;
            let float_ty = if left_ty == ValueType::Bool {
                right_ty
            } else {
                left_ty
            };
            return Ok(float_ty);
        }

        // Matrix/vector multiplication: A * v or A * B
        if matches!(op, BinaryOp::Mul)
            && (left_ty == ValueType::Array || right_ty == ValueType::Array)
        {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::MatMul);
            return Ok(ValueType::Array);
        }

        // String concatenation: "a" * "b" => "ab"
        // Issue #2127: Include Char operands since Julia's * converts Char to String
        if matches!(op, BinaryOp::Mul)
            && (left_ty == ValueType::Str
                || right_ty == ValueType::Str
                || (matches!(left_ty, ValueType::Str | ValueType::Char)
                    && matches!(right_ty, ValueType::Str | ValueType::Char)))
        {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::StringConcat(2));
            return Ok(ValueType::Str);
        }

        // Note: This function handles builtin operators for primitive types only.
        // Complex operations use Pure Julia method dispatch (base/complex.jl).
        //
        // Issue #1759: Float32 type preservation - when one operand is Float32 and the
        // other is not Float64, the result should be Float32 (following Julia semantics).
        let has_f64 = left_ty == ValueType::F64 || right_ty == ValueType::F64;
        let has_f32 = left_ty == ValueType::F32 || right_ty == ValueType::F32;
        let has_f16 = left_ty == ValueType::F16 || right_ty == ValueType::F16;
        let left_is_char = left_ty == ValueType::Char;
        let right_is_char = right_ty == ValueType::Char;
        let has_char = left_is_char || right_is_char;
        let is_fixed_width_integer = |ty: &ValueType| {
            matches!(
                ty,
                ValueType::I8
                    | ValueType::I16
                    | ValueType::I32
                    | ValueType::I64
                    | ValueType::I128
                    | ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128
            )
        };

        let result_ty = match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                if has_f64 {
                    ValueType::F64
                } else if has_f32 {
                    // Issue #1759: Float32 + Bool/Int -> Float32
                    ValueType::F32
                } else if has_f16 {
                    ValueType::F16 // Issue #1972
                } else if has_char && matches!(op, BinaryOp::Add) {
                    // Issue #2122: Char + Int -> Char, Int + Char -> Char
                    ValueType::Char
                } else if has_char && matches!(op, BinaryOp::Sub) && left_is_char && !right_is_char
                {
                    // Issue #2122: Char - Int -> Char
                    ValueType::Char
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type (e.g., Int8+Int8 -> Int8)
                    small_ty
                } else {
                    // Char - Char -> Int (both are Char, result is Int)
                    ValueType::I64
                }
            }
            BinaryOp::Div => {
                // Division always returns float, but preserve Float32 if no Float64
                if has_f64 {
                    ValueType::F64
                } else if has_f32 {
                    ValueType::F32
                } else if has_f16 {
                    ValueType::F16 // Issue #1972
                } else {
                    ValueType::F64
                }
            }
            BinaryOp::Pow => {
                // Power operator: Int^Int -> Int, otherwise -> Float64 (Julia semantics)
                if has_f64 {
                    ValueType::F64
                } else if has_f32 {
                    ValueType::F32
                } else if has_f16 {
                    ValueType::F16 // Issue #1972
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::IntDiv | BinaryOp::Mod => {
                // Integer division / modulo: preserve float type, otherwise I64
                if has_f64 {
                    ValueType::F64
                } else if has_f32 {
                    ValueType::F32
                } else if has_f16 {
                    ValueType::F16 // Issue #1972
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Egal
            | BinaryOp::NotEgal
            | BinaryOp::Subtype => {
                ValueType::Bool // Comparisons return Bool (Julia semantics)
            }
            BinaryOp::And | BinaryOp::Or => ValueType::Bool, // Logical operators return Bool
        };

        if result_ty == ValueType::F16
            && !has_f32
            && !has_f64
            && ((left_ty == ValueType::F16 && is_fixed_width_integer(&right_ty))
                || (right_ty == ValueType::F16 && is_fixed_width_integer(&left_ty)))
            && matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
            )
        {
            // Issue #9717: Float16 mixed with fixed-width integers promotes the
            // integer operand to Float16 before arithmetic. The generic typed
            // path below converts operands directly to F64, which misses
            // overflow-to-Inf and rounding edges for large integers.
            self.compile_expr_as(left, ValueType::F16)?;
            self.emit(Instr::DynamicToF64);
            self.compile_expr_as(right, ValueType::F16)?;
            self.emit(Instr::DynamicToF64);
            let intrinsic = match op {
                BinaryOp::Add => Intrinsic::DynamicAdd,
                BinaryOp::Sub => Intrinsic::DynamicSub,
                BinaryOp::Mul => Intrinsic::DynamicMul,
                BinaryOp::Div => Intrinsic::DynamicDiv,
                _ => unreachable!(),
            };
            if let Some(instr) = typed_instr_for_intrinsic(intrinsic) {
                self.emit(instr);
            } else {
                self.emit(Instr::CallIntrinsic(intrinsic));
            }
            self.emit(Instr::DynamicToF16);
            return Ok(ValueType::F16);
        }

        // Issue #1759: For Float32/Float16 operations, we compute in F64 then convert back
        // because intrinsics only support I64 and F64. (Issue #1975: include has_f16)
        let has_any_float = has_f64 || has_f32 || has_f16;
        let operand_ty = match op {
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                if has_any_float {
                    ValueType::F64 // Use F64 intrinsics for F32/F16 comparisons too
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if has_any_float {
                    ValueType::F64
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Div => ValueType::F64,
            BinaryOp::Mul | BinaryOp::Add | BinaryOp::Sub | BinaryOp::Pow => {
                if has_any_float {
                    ValueType::F64 // Compute in F64, convert back to F32/F16 if needed
                } else {
                    ValueType::I64
                }
            }
            _ => {
                // For other ops, use the result type or I64
                if result_ty == ValueType::F32 || result_ty == ValueType::F16 {
                    ValueType::F64 // Compute in F64
                } else {
                    result_ty.clone()
                }
            }
        };

        self.compile_expr_as(left, operand_ty.clone())?;
        self.compile_expr_as(right, operand_ty.clone())?;

        let intrinsic_opt = match (op, operand_ty.clone()) {
            (BinaryOp::Add, ValueType::I64) => Some(Intrinsic::AddInt),
            (BinaryOp::Sub, ValueType::I64) => Some(Intrinsic::SubInt),
            (BinaryOp::Mul, ValueType::I64) => Some(Intrinsic::MulInt),
            (BinaryOp::Mod, ValueType::I64) => Some(Intrinsic::SremInt),
            (BinaryOp::IntDiv, ValueType::I64) => Some(Intrinsic::SdivInt),
            // Float mod/int div: use Dynamic* for type preservation (Issue #1762, #1970)
            (BinaryOp::Mod, ValueType::F64) => None,
            (BinaryOp::IntDiv, ValueType::F64) => None,
            // Power: use DynamicPow to preserve integer arithmetic
            (BinaryOp::Pow, ValueType::I64) => None,
            (BinaryOp::Add, ValueType::F64) => Some(Intrinsic::DynamicAdd),
            (BinaryOp::Sub, ValueType::F64) => Some(Intrinsic::DynamicSub),
            (BinaryOp::Mul, ValueType::F64) => Some(Intrinsic::DynamicMul),
            (BinaryOp::Div, ValueType::F64) => Some(Intrinsic::DynamicDiv),
            // Power: use DynamicPow to preserve type semantics
            (BinaryOp::Pow, ValueType::F64) => None,
            (BinaryOp::Eq, ValueType::I64) => Some(Intrinsic::EqInt),
            (BinaryOp::Ne, ValueType::I64) => Some(Intrinsic::NeInt),
            (BinaryOp::Lt, ValueType::I64) => Some(Intrinsic::SltInt),
            (BinaryOp::Le, ValueType::I64) => Some(Intrinsic::SleInt),
            (BinaryOp::Gt, ValueType::I64) => Some(Intrinsic::SgtInt),
            (BinaryOp::Ge, ValueType::I64) => Some(Intrinsic::SgeInt),
            (BinaryOp::Eq, ValueType::F64) => Some(Intrinsic::EqFloat),
            (BinaryOp::Ne, ValueType::F64) => Some(Intrinsic::NeFloat),
            (BinaryOp::Lt, ValueType::F64) => Some(Intrinsic::LtFloat),
            (BinaryOp::Le, ValueType::F64) => Some(Intrinsic::LeFloat),
            (BinaryOp::Gt, ValueType::F64) => Some(Intrinsic::GtFloat),
            (BinaryOp::Ge, ValueType::F64) => Some(Intrinsic::GeFloat),
            // Note: Complex operations now use Pure Julia with method dispatch
            (BinaryOp::Div, _) => Some(Intrinsic::DynamicDiv),
            // Power: use DynamicPow to preserve type semantics
            (BinaryOp::Pow, _) => None,
            (BinaryOp::Mod, _) => None, // Will use DynamicMod below (Issue #1762)
            (BinaryOp::IntDiv, _) => None, // Will use DynamicIntDiv below (Issue #1970)
            (BinaryOp::And, _) => Some(Intrinsic::MulInt),
            (BinaryOp::Or, _) => None,
            _ => None,
        };

        match intrinsic_opt {
            Some(intrinsic) => {
                if let Some(instr) = typed_instr_for_intrinsic(intrinsic) {
                    self.emit(instr);
                } else {
                    self.emit(Instr::CallIntrinsic(intrinsic));
                }
            }
            None => {
                if matches!(op, BinaryOp::Pow) {
                    // Power operator: use DynamicPow to preserve integer arithmetic
                    self.emit(Instr::DynamicPow);
                } else if matches!(op, BinaryOp::Mod) {
                    // Float mod: use DynamicMod for fmod semantics (Issue #1762)
                    self.emit(Instr::DynamicMod);
                } else if matches!(op, BinaryOp::IntDiv) {
                    // Float int div: use DynamicIntDiv for type preservation (Issue #1970)
                    self.emit(Instr::DynamicIntDiv);
                } else if matches!(op, BinaryOp::Or) {
                    return self.compile_or_expr(left, right);
                } else {
                    // Note: Complex Pow now uses Pure Julia with method dispatch
                    return err(format!("Unsupported builtin binary op: {:?}", op));
                }
            }
        }

        // Issue #1759: Convert F64 result back to F32 if needed
        if result_ty == ValueType::F32 && operand_ty == ValueType::F64 {
            self.emit(Instr::DynamicToF32);
        }
        // Issue #1975: Convert F64 result back to F16 if needed
        if result_ty == ValueType::F16 && operand_ty == ValueType::F64 {
            self.emit(Instr::DynamicToF16);
        }
        // Issue #2122: Convert I64 result back to Char for Char arithmetic
        if result_ty == ValueType::Char {
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::IntToChar, 1));
        }
        // Issue #2278: Convert I64 result back to small integer type
        if let Some(back_conv) = small_int_back_conversion(&result_ty) {
            if operand_ty == ValueType::I64 {
                self.emit(back_conv);
            }
        }

        match op {
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => Ok(ValueType::Bool),
            _ => Ok(result_ty),
        }
    }
}
