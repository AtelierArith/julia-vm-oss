//! User-defined binary operator compilation.
//!
//! Handles compilation of calls to user-defined binary operator methods
//! (e.g., custom `+`, `*` for Complex, Rational types).

use crate::compile::promotion::promote_type;
use crate::ir::core::{BinaryOp, Expr};
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use crate::compile::{julia_type_to_value_type, CResult, CoreCompiler, MethodSig};

/// Convert JuliaType to type name string for promotion.
fn julia_type_to_type_name(ty: &JuliaType) -> String {
    match ty {
        JuliaType::Struct(name) if name.starts_with("Complex{") && name.ends_with("}") => {
            name.clone()
        }
        JuliaType::Float64 => "Float64".to_string(),
        JuliaType::Float32 => "Float32".to_string(),
        JuliaType::Int64 => "Int64".to_string(),
        JuliaType::Int32 => "Int32".to_string(),
        JuliaType::Int16 => "Int16".to_string(),
        JuliaType::Int8 => "Int8".to_string(),
        JuliaType::UInt64 => "UInt64".to_string(),
        JuliaType::UInt32 => "UInt32".to_string(),
        JuliaType::UInt16 => "UInt16".to_string(),
        JuliaType::UInt8 => "UInt8".to_string(),
        JuliaType::Bool => "Bool".to_string(),
        _ => "Float64".to_string(), // Default to Float64 for unknown types
    }
}

/// Promote two operand types involved in Complex arithmetic to get the result type name.
/// Uses the centralized promotion module following Julia's promote_rule/promote_type pattern.
/// e.g., Float64 + Complex{Bool} -> Complex{Float64}
///       Complex{Int64} * Complex{Bool} -> Complex{Int64}
fn promote_complex_operands(left: &JuliaType, right: &JuliaType) -> String {
    let left_name = julia_type_to_type_name(left);
    let right_name = julia_type_to_type_name(right);

    // Use centralized promote_type which handles Complex promotion correctly
    promote_type(&left_name, &right_name)
}

impl CoreCompiler<'_> {
    /// Helper to compile a call to a user-defined binary operator method.
    pub(in crate::compile) fn compile_user_defined_binary_op(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
        method: &MethodSig,
    ) -> CResult<ValueType> {
        // Compile left operand - no conversion needed for abstract or struct types.
        // Abstract numeric types (Number, Real, Integer, etc.) must NOT be converted
        // because the Julia method handles promotion at runtime. For example,
        // +(x::Number, y::Number) receives the original typed values and calls promote().
        let left_param_ty = &method.params[0].1;
        if matches!(
            left_param_ty,
            JuliaType::Any
                | JuliaType::Struct(_)
                | JuliaType::Number
                | JuliaType::Real
                | JuliaType::Integer
                | JuliaType::Signed
                | JuliaType::Unsigned
                | JuliaType::AbstractFloat
        ) {
            self.compile_expr(left)?;
        } else {
            let vt = julia_type_to_value_type(left_param_ty);
            self.compile_expr_as(left, vt)?;
        }
        // Compile right operand
        let right_param_ty = &method.params[1].1;
        if matches!(
            right_param_ty,
            JuliaType::Any
                | JuliaType::Struct(_)
                | JuliaType::Number
                | JuliaType::Real
                | JuliaType::Integer
                | JuliaType::Signed
                | JuliaType::Unsigned
                | JuliaType::AbstractFloat
        ) {
            self.compile_expr(right)?;
        } else {
            let vt = julia_type_to_value_type(right_param_ty);
            self.compile_expr_as(right, vt)?;
        }
        // Call the user-defined operator method
        self.emit(Instr::Call(method.global_index, 2));

        // Fix return type for Complex arithmetic operations
        // When method.return_type is I64 (default from None in JSON) but the operation
        // involves Complex operands, arithmetic operations should return Complex.
        // Comparison operators (==, !=, etc.) return Bool (Julia semantics).
        let is_comparison = matches!(
            op,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge
        );
        if is_comparison {
            // Comparison operators always return Bool (Julia semantics)
            Ok(ValueType::Bool)
        } else {
            let left_is_complex = matches!(left_param_ty, JuliaType::Struct(name) if name == "Complex" || name.starts_with("Complex{"));
            let right_is_complex = matches!(right_param_ty, JuliaType::Struct(name) if name == "Complex" || name.starts_with("Complex{"));
            if (left_is_complex || right_is_complex)
                && !self.is_struct_type_of(method.return_type.clone(), "Complex")
            {
                // Complex arithmetic operation returns Complex with proper type promotion
                // Promote the operand types to get the correct result type
                let promoted_name = promote_complex_operands(left_param_ty, right_param_ty);
                if let Some(info) = self.shared_ctx.struct_table.get(&promoted_name) {
                    Ok(ValueType::Struct(info.type_id))
                } else {
                    // Fallback: try to get Complex{Float64} as a safe default
                    if let Some(info) = self.shared_ctx.struct_table.get("Complex{Float64}") {
                        Ok(ValueType::Struct(info.type_id))
                    } else {
                        // Last resort: return Any for runtime dispatch
                        Ok(ValueType::Any)
                    }
                }
            } else {
                Ok(method.return_type.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── julia_type_to_type_name ───────────────────────────────────────────────

    #[test]
    fn test_julia_type_to_type_name_primitives() {
        assert_eq!(julia_type_to_type_name(&JuliaType::Float64), "Float64");
        assert_eq!(julia_type_to_type_name(&JuliaType::Float32), "Float32");
        assert_eq!(julia_type_to_type_name(&JuliaType::Int64), "Int64");
        assert_eq!(julia_type_to_type_name(&JuliaType::Int32), "Int32");
        assert_eq!(julia_type_to_type_name(&JuliaType::Int16), "Int16");
        assert_eq!(julia_type_to_type_name(&JuliaType::Int8), "Int8");
        assert_eq!(julia_type_to_type_name(&JuliaType::UInt64), "UInt64");
        assert_eq!(julia_type_to_type_name(&JuliaType::UInt32), "UInt32");
        assert_eq!(julia_type_to_type_name(&JuliaType::UInt16), "UInt16");
        assert_eq!(julia_type_to_type_name(&JuliaType::UInt8), "UInt8");
        assert_eq!(julia_type_to_type_name(&JuliaType::Bool), "Bool");
    }

    #[test]
    fn test_julia_type_to_type_name_complex_struct() {
        // Complex{Float64} struct name is preserved as-is
        let complex = JuliaType::Struct("Complex{Float64}".to_string());
        assert_eq!(julia_type_to_type_name(&complex), "Complex{Float64}");
    }

    #[test]
    fn test_julia_type_to_type_name_non_complex_struct_defaults_to_float64() {
        // Non-Complex struct types fall through to the default Float64
        let rational = JuliaType::Struct("Rational{Int64}".to_string());
        assert_eq!(julia_type_to_type_name(&rational), "Float64");
    }

    #[test]
    fn test_julia_type_to_type_name_any_defaults_to_float64() {
        // Unknown/Any types default to Float64
        assert_eq!(julia_type_to_type_name(&JuliaType::Any), "Float64");
    }

    // ── promote_complex_operands ──────────────────────────────────────────────

    #[test]
    fn test_promote_complex_operands_float64_and_complex_bool() {
        // Float64 + Complex{Bool} → Complex{Float64}
        let result = promote_complex_operands(&JuliaType::Float64, &JuliaType::Struct("Complex{Bool}".to_string()));
        assert_eq!(result, "Complex{Float64}");
    }

    #[test]
    fn test_promote_complex_operands_same_types() {
        // Float64 + Float64 → Float64
        let result = promote_complex_operands(&JuliaType::Float64, &JuliaType::Float64);
        assert_eq!(result, "Float64");
    }
}
