//! Math builtin function compilation.
//!
//! Handles compilation of math functions: sqrt, floor, ceil, round, trunc, fma, muladd, etc.
//! Note: sin, cos, tan, asin, acos, atan, exp, log have been migrated to Pure Julia (base/math.jl).

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

// Note: next_prev_float_result_type removed — nextfloat/prevfloat are now
// Pure Julia (base/float.jl, Issue #6740).

fn unary_float_preserving_result_type(arg: ValueType) -> ValueType {
    match arg {
        ValueType::F16 | ValueType::F32 | ValueType::F64 => arg,
        ValueType::Any => ValueType::Any,
        _ => ValueType::F64,
    }
}

impl CoreCompiler<'_> {
    fn compile_unary_rounding_struct_dispatch(
        &mut self,
        name: &str,
        builtin_id: BuiltinId,
        arg: &Expr,
    ) -> CResult<Option<ValueType>> {
        let arg_ty = self.infer_julia_type(arg);
        if matches!(arg_ty, JuliaType::Struct(_)) {
            if let Some(table) = self.method_tables.get(name) {
                let arg_types = vec![arg_ty.clone()];
                if let Ok(method) = table.dispatch(&arg_types) {
                    self.compile_expr(arg)?;
                    self.emit(Instr::Call(method.global_index, 1));
                    return Ok(Some(method.return_type.clone()));
                }
            }
        }

        // For Any type, use runtime dispatch if struct methods exist.
        if matches!(arg_ty, JuliaType::Any) {
            if let Some(table) = self.method_tables.get(name) {
                // Issue #6496: index-only payload; the runtime derives the
                // expected first-parameter type name from FunctionInfo.
                // Struct-spelling probe sourced from the canonical
                // `core_signature` projection (Issue #6495, stages 7a/7c-ii);
                // `params.len()` is an arity read.
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| {
                        m.param_count() == 1
                            && m.param_matches_at(0, super::binary::core_param_is_struct_spelling)
                    })
                    .map(|m| m.global_index)
                    .collect();
                if !candidates.is_empty() {
                    self.compile_expr(arg)?;
                    self.emit(Instr::CallDynamicOrBuiltin(builtin_id, candidates));
                    return Ok(Some(ValueType::Any));
                }
            }
        }

        Ok(None)
    }

    /// Compile math builtin functions.
    /// Returns `Ok(Some(result))` if handled, `Ok(None)` if not a math function.
    pub(in super::super) fn compile_builtin_math(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "rand" => {
                if args.is_empty() {
                    self.emit(Instr::RandF64);
                    Ok(Some(ValueType::F64))
                } else {
                    // Check if first argument is a type identifier (Int, Int64, Float64)
                    let (dims, is_int_array) = if let Some(first) = args.first() {
                        match first {
                            Expr::Var(name, _) if name == "Int" || name == "Int64" => {
                                // rand(Int, dims...) or rand(Int64, dims...)
                                (&args[1..], true)
                            }
                            Expr::Var(name, _) if name == "Float64" => {
                                // rand(Float64, dims...) - same as rand(dims...)
                                (&args[1..], false)
                            }
                            _ => (args, false),
                        }
                    } else {
                        (args, false)
                    };

                    for dim in dims {
                        self.compile_expr_as(dim, ValueType::I64)?;
                    }

                    if is_int_array {
                        self.emit(Instr::RandIntArray(dims.len()));
                    } else {
                        self.emit(Instr::RandArray(dims.len()));
                    }
                    Ok(Some(ValueType::Array))
                }
            }
            "sqrt" => {
                // Check for user-defined sqrt method (e.g., sqrt(::Complex{Float64}))
                let arg_ty = self.infer_julia_type(&args[0]);
                if matches!(arg_ty, JuliaType::Struct(_)) {
                    if let Some(table) = self.method_tables.get("sqrt") {
                        let arg_types = vec![arg_ty.clone()];
                        if let Ok(method) = table.dispatch(&arg_types) {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::Call(method.global_index, 1));
                            return Ok(Some(method.return_type.clone()));
                        }
                    }
                }
                // Keep Float16/Float32 width; integer-like inputs still produce Float64.
                let arg_ty = self.compile_expr(&args[0])?;
                self.emit(Instr::SqrtF64);
                Ok(Some(unary_float_preserving_result_type(arg_ty)))
            }
            "sdiv_int" => {
                // Low-level signed integer division intrinsic
                // Called by div(x::Int64, y::Int64) in int.jl
                // This matches Julia's checked_sdiv_int intrinsic
                if args.len() != 2 {
                    return err(format!("sdiv_int requires 2 arguments, got {}", args.len()));
                }
                // Issue #3694: keep Int128 operands as I128 so the extended SdivInt
                // intrinsic preserves the type. div(::Int128, ::Int128) in int.jl
                // relies on this to return Int128 instead of Float64.
                // Issue #3696: same for UInt128 with unsigned division semantics.
                let left_ty = self.infer_expr_type(&args[0]);
                let right_ty = self.infer_expr_type(&args[1]);
                let both_i128 = left_ty == ValueType::I128 && right_ty == ValueType::I128;
                let both_u128 = left_ty == ValueType::U128 && right_ty == ValueType::U128;
                if both_i128 {
                    self.compile_expr_as(&args[0], ValueType::I128)?;
                    self.compile_expr_as(&args[1], ValueType::I128)?;
                    self.emit(Instr::CallIntrinsic(crate::intrinsics::Intrinsic::SdivInt));
                    return Ok(Some(ValueType::I128));
                }
                if both_u128 {
                    self.compile_expr_as(&args[0], ValueType::U128)?;
                    self.compile_expr_as(&args[1], ValueType::U128)?;
                    self.emit(Instr::CallIntrinsic(crate::intrinsics::Intrinsic::SdivInt));
                    return Ok(Some(ValueType::U128));
                }
                // Issue #3701: same for UInt64. Cast-through-I64 wraps for
                // values above i64::MAX, so route through the native U64 arm
                // of SdivInt instead.
                let both_u64 = left_ty == ValueType::U64 && right_ty == ValueType::U64;
                if both_u64 {
                    self.compile_expr_as(&args[0], ValueType::U64)?;
                    self.compile_expr_as(&args[1], ValueType::U64)?;
                    self.emit(Instr::CallIntrinsic(crate::intrinsics::Intrinsic::SdivInt));
                    return Ok(Some(ValueType::U64));
                }
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.compile_expr_as(&args[1], ValueType::I64)?;
                self.emit(Instr::CallIntrinsic(crate::intrinsics::Intrinsic::SdivInt));
                Ok(Some(ValueType::I64))
            }
            // Note: sin, cos, tan, asin, acos, atan, exp, log removed — now Pure Julia (base/math.jl)

            // === Rounding functions (also Builtins, not CPU-level) ===
            "floor" => {
                // floor(T, x) - floor and convert to type T (Issue #2028)
                if args.len() == 2 {
                    if let Expr::Var(type_name, _) = &args[0] {
                        if let Some(vt) = rounding_target_type(type_name) {
                            self.compile_expr_as(&args[1], ValueType::F64)?;
                            self.emit(Instr::FloorF64);
                            self.emit_rounding_conversion(&vt);
                            return Ok(Some(vt));
                        }
                    }
                }
                if let Some(result) = self.compile_unary_rounding_struct_dispatch(
                    "floor",
                    BuiltinId::Floor,
                    &args[0],
                )? {
                    return Ok(Some(result));
                }
                // Keep Float16/Float32 width; integer-like inputs still produce Float64.
                let arg_ty = self.compile_expr(&args[0])?;
                self.emit(Instr::FloorF64); // Keep as intrinsic (CPU instruction)
                Ok(Some(unary_float_preserving_result_type(arg_ty)))
            }
            "ceil" => {
                // ceil(T, x) - ceil and convert to type T (Issue #2028)
                if args.len() == 2 {
                    if let Expr::Var(type_name, _) = &args[0] {
                        if let Some(vt) = rounding_target_type(type_name) {
                            self.compile_expr_as(&args[1], ValueType::F64)?;
                            self.emit(Instr::CeilF64);
                            self.emit_rounding_conversion(&vt);
                            return Ok(Some(vt));
                        }
                    }
                }
                if let Some(result) =
                    self.compile_unary_rounding_struct_dispatch("ceil", BuiltinId::Ceil, &args[0])?
                {
                    return Ok(Some(result));
                }
                // Keep Float16/Float32 width; integer-like inputs still produce Float64.
                let arg_ty = self.compile_expr(&args[0])?;
                self.emit(Instr::CeilF64); // Keep as intrinsic (CPU instruction)
                Ok(Some(unary_float_preserving_result_type(arg_ty)))
            }
            "round" => {
                // round(T, x) - round and convert to type T (Issue #2028)
                if args.len() == 2 {
                    if let Expr::Var(type_name, _) = &args[0] {
                        if let Some(vt) = rounding_target_type(type_name) {
                            self.compile_expr_as(&args[1], ValueType::F64)?;
                            self.emit(Instr::CallBuiltin(BuiltinId::Round, 1));
                            self.emit_rounding_conversion(&vt);
                            return Ok(Some(vt));
                        }
                    }
                }
                if let Some(result) = self.compile_unary_rounding_struct_dispatch(
                    "round",
                    BuiltinId::Round,
                    &args[0],
                )? {
                    return Ok(Some(result));
                }
                let arg_ty = self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Round, 1));
                Ok(Some(unary_float_preserving_result_type(arg_ty)))
            }
            "trunc" => {
                // trunc(T, x) - truncate and convert to type T (Issue #2028)
                if args.len() == 2 {
                    if let Expr::Var(type_name, _) = &args[0] {
                        if let Some(vt) = rounding_target_type(type_name) {
                            self.compile_expr_as(&args[1], ValueType::F64)?;
                            self.emit(Instr::CallBuiltin(BuiltinId::Trunc, 1));
                            self.emit_rounding_conversion(&vt);
                            return Ok(Some(vt));
                        }
                    }
                }
                if let Some(result) = self.compile_unary_rounding_struct_dispatch(
                    "trunc",
                    BuiltinId::Trunc,
                    &args[0],
                )? {
                    return Ok(Some(result));
                }
                let arg_ty = self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Trunc, 1));
                Ok(Some(unary_float_preserving_result_type(arg_ty)))
            }
            // nextfloat / prevfloat removed - pure Julia (Issue #6740)
            // Bit operations: low-level CPU intrinsics called by the pure-Julia
            // public functions count_ones/leading_zeros/trailing_zeros/bitreverse/
            // bswap (base/int.jl, Issue #6741). Issue #4785: do NOT coerce the
            // input to I64 — the runtime dispatches on the actual integer variant
            // so the bit width is preserved.
            "_ctpop_int" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::CountOnes, 1));
                Ok(Some(ValueType::I64))
            }
            "_ctlz_int" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::LeadingZeros, 1));
                Ok(Some(ValueType::I64))
            }
            "_bitreverse_int" => {
                // bitreverse preserves element type (UInt8 → UInt8).
                let t = self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Bitreverse, 1));
                Ok(Some(t))
            }
            "_cttz_int" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TrailingZeros, 1));
                Ok(Some(ValueType::I64))
            }
            "_bswap_int" => {
                // Issue #4787: preserve the original integer element
                // type so bswap respects the actual bit width.
                let t = self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Bswap, 1));
                Ok(Some(t))
            }
            // Float decomposition: exponent / significand / frexp removed - pure Julia (Issue #6740)
            // Float inspection: issubnormal removed - pure Julia (Issue #6740)
            // Note: maxintfloat is now Pure Julia (base/floatfuncs.jl) — Issue #3732.
            // Note: muladd is now Pure Julia (base/math.jl) — Issue #3732.
            // Note: public fma is Pure Julia (base/math.jl); the Pure Julia
            // wrapper calls the internal `_fma` intrinsic for IEEE fused
            // semantics on Float64. The public name no longer reaches a Rust
            // builtin route here.
            "_fma" => {
                if args.len() != 3 {
                    return err(format!("_fma requires 3 arguments, got {}", args.len()));
                }
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.compile_expr_as(&args[1], ValueType::F64)?;
                self.compile_expr_as(&args[2], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Fma, 3));
                Ok(Some(ValueType::F64))
            }
            // Number theory functions
            // Note: gcd, lcm, factorial removed - now Pure Julia (base/intfuncs.jl)
            "sleep" => {
                // Validate argument count
                if args.is_empty() {
                    return err("sleep() requires one argument");
                }
                if args.len() > 1 {
                    return err("sleep() takes exactly one argument");
                }

                // Infer argument type and emit appropriate instruction
                let arg_ty = self.infer_expr_type(&args[0]);
                match arg_ty {
                    ValueType::I64 => {
                        self.compile_expr_as(&args[0], ValueType::I64)?;
                        self.emit(Instr::SleepI64);
                    }
                    _ => {
                        // Default to F64 (matches Julia's type coercion)
                        self.compile_expr_as(&args[0], ValueType::F64)?;
                        self.emit(Instr::SleepF64);
                    }
                }

                Ok(Some(ValueType::Nothing))
            }
            _ => Ok(None),
        }
    }

    /// Emit conversion instruction after a rounding operation to convert F64 to target type (Issue #2028).
    fn emit_rounding_conversion(&mut self, target: &ValueType) {
        match target {
            ValueType::I64 => {
                self.emit(Instr::DynamicToI64);
            }
            ValueType::F32 => {
                self.emit(Instr::DynamicToF32);
            }
            ValueType::F16 => {
                self.emit(Instr::DynamicToF16);
            }
            ValueType::Bool => {
                self.emit(Instr::DynamicToBool);
            }
            // F64 needs no conversion (rounding already produces F64)
            _ => {}
        }
    }
}

/// Map a type name to the ValueType for rounding target type conversion (Issue #2028).
/// Returns None if the name is not a recognized numeric type.
pub(super) fn rounding_target_type(type_name: &str) -> Option<ValueType> {
    match type_name {
        "Int" | "Int64" | "Int32" | "Int16" | "Int8" | "Int128" | "UInt64" | "UInt32"
        | "UInt16" | "UInt8" | "UInt128" => Some(ValueType::I64),
        "Float64" => Some(ValueType::F64),
        "Float32" => Some(ValueType::F32),
        "Float16" => Some(ValueType::F16),
        "Bool" => Some(ValueType::Bool),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rounding_target_type_integer_names() {
        assert!(matches!(
            rounding_target_type("Int64"),
            Some(ValueType::I64)
        ));
        assert!(matches!(rounding_target_type("Int"), Some(ValueType::I64)));
        assert!(matches!(
            rounding_target_type("UInt8"),
            Some(ValueType::I64)
        ));
    }

    #[test]
    fn test_rounding_target_type_float_names() {
        assert!(matches!(
            rounding_target_type("Float64"),
            Some(ValueType::F64)
        ));
        assert!(matches!(
            rounding_target_type("Float32"),
            Some(ValueType::F32)
        ));
        assert!(matches!(
            rounding_target_type("Float16"),
            Some(ValueType::F16)
        ));
    }

    #[test]
    fn test_rounding_target_type_bool() {
        assert!(matches!(
            rounding_target_type("Bool"),
            Some(ValueType::Bool)
        ));
    }

    #[test]
    fn test_rounding_target_type_unknown_returns_none() {
        assert_eq!(rounding_target_type("String"), None);
        assert_eq!(rounding_target_type(""), None);
        assert_eq!(rounding_target_type("Complex"), None);
    }
}
