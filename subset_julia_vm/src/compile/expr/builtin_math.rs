//! Math builtin function compilation.
//!
//! Handles compilation of math functions: sqrt, floor, ceil, round, trunc, fma, muladd, etc.
//! Note: sin, cos, tan, asin, acos, atan, exp, log have been migrated to Pure Julia (base/math.jl).

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
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
                // Fall back to builtin F64 sqrt
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::SqrtF64);
                Ok(Some(ValueType::F64))
            }
            "sdiv_int" => {
                // Low-level signed integer division intrinsic
                // Called by div(x::Int64, y::Int64) in int.jl
                // This matches Julia's checked_sdiv_int intrinsic
                if args.len() != 2 {
                    return err(format!("sdiv_int requires 2 arguments, got {}", args.len()));
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
                // Check for user-defined floor method (e.g., floor(::Rational))
                let arg_ty = self.infer_julia_type(&args[0]);
                if matches!(arg_ty, JuliaType::Struct(_)) {
                    if let Some(table) = self.method_tables.get("floor") {
                        let arg_types = vec![arg_ty.clone()];
                        if let Ok(method) = table.dispatch(&arg_types) {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::Call(method.global_index, 1));
                            return Ok(Some(method.return_type.clone()));
                        }
                    }
                }
                // For Any type, use runtime dispatch if struct methods exist
                if matches!(arg_ty, JuliaType::Any) {
                    if let Some(table) = self.method_tables.get("floor") {
                        // Build candidates for runtime dispatch
                        let candidates: Vec<(usize, String)> = table
                            .methods
                            .iter()
                            .filter(|m| {
                                m.params.len() == 1
                                    && matches!(&m.params[0].1, JuliaType::Struct(_))
                            })
                            .map(|m| {
                                let type_name = m.params[0].1.to_string();
                                (m.global_index, type_name)
                            })
                            .collect();
                        if !candidates.is_empty() {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::CallDynamicOrBuiltin(BuiltinId::Floor, candidates));
                            return Ok(Some(ValueType::Any));
                        }
                    }
                }
                // Fall back to builtin F64 floor
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::FloorF64); // Keep as intrinsic (CPU instruction)
                Ok(Some(ValueType::F64))
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
                // Check for user-defined ceil method (e.g., ceil(::Rational))
                let arg_ty = self.infer_julia_type(&args[0]);
                if matches!(arg_ty, JuliaType::Struct(_)) {
                    if let Some(table) = self.method_tables.get("ceil") {
                        let arg_types = vec![arg_ty.clone()];
                        if let Ok(method) = table.dispatch(&arg_types) {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::Call(method.global_index, 1));
                            return Ok(Some(method.return_type.clone()));
                        }
                    }
                }
                // For Any type, use runtime dispatch if struct methods exist
                if matches!(arg_ty, JuliaType::Any) {
                    if let Some(table) = self.method_tables.get("ceil") {
                        // Build candidates for runtime dispatch
                        let candidates: Vec<(usize, String)> = table
                            .methods
                            .iter()
                            .filter(|m| {
                                m.params.len() == 1
                                    && matches!(&m.params[0].1, JuliaType::Struct(_))
                            })
                            .map(|m| {
                                let type_name = m.params[0].1.to_string();
                                (m.global_index, type_name)
                            })
                            .collect();
                        if !candidates.is_empty() {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::CallDynamicOrBuiltin(BuiltinId::Ceil, candidates));
                            return Ok(Some(ValueType::Any));
                        }
                    }
                }
                // Fall back to builtin F64 ceil
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CeilF64); // Keep as intrinsic (CPU instruction)
                Ok(Some(ValueType::F64))
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
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Round, 1));
                Ok(Some(ValueType::F64))
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
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Trunc, 1));
                Ok(Some(ValueType::F64))
            }
            "nextfloat" => {
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::NextFloat, 1));
                Ok(Some(ValueType::F64))
            }
            "prevfloat" => {
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::PrevFloat, 1));
                Ok(Some(ValueType::F64))
            }
            // Bit operations (work on integers, return integers)
            "count_ones" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::CountOnes, 1));
                Ok(Some(ValueType::I64))
            }
            "count_zeros" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::CountZeros, 1));
                Ok(Some(ValueType::I64))
            }
            "leading_zeros" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::LeadingZeros, 1));
                Ok(Some(ValueType::I64))
            }
            "trailing_ones" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::TrailingOnes, 1));
                Ok(Some(ValueType::I64))
            }
            "bitreverse" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Bitreverse, 1));
                Ok(Some(ValueType::I64))
            }
            "leading_ones" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::LeadingOnes, 1));
                Ok(Some(ValueType::I64))
            }
            "trailing_zeros" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::TrailingZeros, 1));
                Ok(Some(ValueType::I64))
            }
            "bitrotate" => {
                // bitrotate(x, k) - rotate bits
                if args.len() != 2 {
                    return err(format!(
                        "bitrotate requires 2 arguments, got {}",
                        args.len()
                    ));
                }
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.compile_expr_as(&args[1], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Bitrotate, 2));
                Ok(Some(ValueType::I64))
            }
            "bswap" => {
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Bswap, 1));
                Ok(Some(ValueType::I64))
            }
            // Float decomposition
            "exponent" => {
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Exponent, 1));
                Ok(Some(ValueType::I64))
            }
            "significand" => {
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Significand, 1));
                Ok(Some(ValueType::F64))
            }
            "frexp" => {
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Frexp, 1));
                Ok(Some(ValueType::Tuple))
            }
            // Float inspection
            "issubnormal" => {
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Issubnormal, 1));
                Ok(Some(ValueType::Bool))
            }
            "maxintfloat" => {
                // maxintfloat() takes no arguments (defaults to Float64)
                self.emit(Instr::CallBuiltin(BuiltinId::Maxintfloat, 0));
                Ok(Some(ValueType::F64))
            }
            // Fused multiply-add
            "fma" => {
                if args.len() != 3 {
                    return err(format!("fma requires 3 arguments, got {}", args.len()));
                }
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.compile_expr_as(&args[1], ValueType::F64)?;
                self.compile_expr_as(&args[2], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Fma, 3));
                Ok(Some(ValueType::F64))
            }
            "muladd" => {
                if args.len() != 3 {
                    return err(format!("muladd requires 3 arguments, got {}", args.len()));
                }
                self.compile_expr_as(&args[0], ValueType::F64)?;
                self.compile_expr_as(&args[1], ValueType::F64)?;
                self.compile_expr_as(&args[2], ValueType::F64)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Muladd, 3));
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
        assert!(matches!(rounding_target_type("Int64"), Some(ValueType::I64)));
        assert!(matches!(rounding_target_type("Int"), Some(ValueType::I64)));
        assert!(matches!(rounding_target_type("UInt8"), Some(ValueType::I64)));
    }

    #[test]
    fn test_rounding_target_type_float_names() {
        assert!(matches!(rounding_target_type("Float64"), Some(ValueType::F64)));
        assert!(matches!(rounding_target_type("Float32"), Some(ValueType::F32)));
        assert!(matches!(rounding_target_type("Float16"), Some(ValueType::F16)));
    }

    #[test]
    fn test_rounding_target_type_bool() {
        assert!(matches!(rounding_target_type("Bool"), Some(ValueType::Bool)));
    }

    #[test]
    fn test_rounding_target_type_unknown_returns_none() {
        assert_eq!(rounding_target_type("String"), None);
        assert_eq!(rounding_target_type(""), None);
        assert_eq!(rounding_target_type("Complex"), None);
    }
}
