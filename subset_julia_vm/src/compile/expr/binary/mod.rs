//! Binary operation compilation.
//!
//! Handles compilation of:
//! - User-defined binary operator overloads
//! - Builtin binary operators (arithmetic, comparison)
//! - Type promotion and intrinsic dispatch

mod builtin;
mod user_defined;

use crate::builtins::BuiltinId;
use crate::intrinsics::Intrinsic;
use crate::ir::core::{BinaryOp, Expr};
use crate::types::{DispatchError, JuliaType};
use crate::vm::{Instr, ValueType};

use crate::compile::{
    binary_op_to_function_name, err, is_float_type, is_numeric_type, is_singleton_type,
    julia_type_to_value_type, CResult, CoreCompiler,
};

/// Determine if both operands are the same small integer type (Issue #2278).
/// In Julia, `Int8(1) + Int8(2)` returns `Int8`, not `Int64`.
/// Returns Some(ValueType) for the preserved type, None if not a same-type small int pair.
pub(super) fn same_small_int_type(left: &ValueType, right: &ValueType) -> Option<ValueType> {
    // Both operands must be the same type
    if left != right {
        return None;
    }
    match left {
        ValueType::I8
        | ValueType::I16
        | ValueType::I32
        | ValueType::U8
        | ValueType::U16
        | ValueType::U32
        | ValueType::U64 => Some(left.clone()),
        _ => None,
    }
}

/// Get the DynamicTo* back-conversion instruction for a small integer ValueType (Issue #2278).
/// Returns the instruction that converts an I64 result back to the original small integer type.
pub(super) fn small_int_back_conversion(ty: &ValueType) -> Option<Instr> {
    match ty {
        ValueType::I8 => Some(Instr::DynamicToI8),
        ValueType::I16 => Some(Instr::DynamicToI16),
        ValueType::I32 => Some(Instr::DynamicToI32),
        ValueType::U8 => Some(Instr::DynamicToU8),
        ValueType::U16 => Some(Instr::DynamicToU16),
        ValueType::U32 => Some(Instr::DynamicToU32),
        ValueType::U64 => Some(Instr::DynamicToU64),
        _ => None,
    }
}

impl CoreCompiler<'_> {
    pub(in crate::compile) fn compile_binary_op(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> CResult<ValueType> {
        // Short-circuit operators are special forms and are not overloadable.
        if matches!(op, BinaryOp::And) {
            return self.compile_and_expr(left, right);
        }
        if matches!(op, BinaryOp::Or) {
            return self.compile_or_expr(left, right);
        }

        // Object identity operators use BuiltinId::Egal
        if matches!(op, BinaryOp::Egal | BinaryOp::NotEgal) {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Egal, 2));
            if matches!(op, BinaryOp::NotEgal) {
                // Negate the boolean result
                self.emit(Instr::NotBool);
            }
            return Ok(ValueType::Bool);
        }

        // Subtype operator uses BuiltinId::Subtype
        if matches!(op, BinaryOp::Subtype) {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Subtype, 2));
            return Ok(ValueType::Bool);
        }

        // Missing propagation for all binary operators (arithmetic and comparison)
        // In Julia, any operation involving missing returns missing (propagation of unknown values).
        // Note: === and !== (identity operators) are handled above and return Bool
        // We only apply this at compile-time for literal `missing` values.
        // For runtime values that might be Missing, the VM handles it appropriately.
        {
            let left_is_missing_lit =
                matches!(left, Expr::Literal(crate::ir::core::Literal::Missing, _));
            let right_is_missing_lit =
                matches!(right, Expr::Literal(crate::ir::core::Literal::Missing, _));
            if left_is_missing_lit || right_is_missing_lit {
                // For missing literals, just push missing as result
                // No need to compile operands since they're literals with no side effects
                self.emit(Instr::PushMissing);
                return Ok(ValueType::Missing);
            }
        }

        // Power operator: use DynamicPow for scalar values to preserve Rational/Complex semantics.
        // Special case: String ^ Int dispatches to repeat(s, n) instead of DynamicPow.
        // Special case: BigInt ^ Int uses PowBigInt intrinsic (Issue #1708).
        if matches!(op, BinaryOp::Pow) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);

            // String ^ Int: dispatch to repeat(s, n) - Julia's string repeat syntax
            if left_ty == ValueType::Str {
                // Look up the repeat function in method tables
                if let Some(table) = self.method_tables.get("repeat") {
                    let arg_types = vec![JuliaType::String, JuliaType::Int64];
                    if let Ok(method) = table.dispatch(&arg_types) {
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;
                        self.emit(Instr::Call(method.global_index, 2));
                        return Ok(ValueType::Str);
                    }
                }
                // Fallback error if repeat method not found
                return err("MethodError: no method matching ^(String, Int64) - repeat function not available");
            }

            // BigInt ^ Int: use PowBigInt intrinsic (Issue #1708)
            let is_bigint_pow = left_ty == ValueType::BigInt || left_ty == ValueType::I128;
            if is_bigint_pow {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallIntrinsic(Intrinsic::PowBigInt));
                return Ok(ValueType::BigInt);
            }

            let left_is_array = matches!(left_ty, ValueType::Array | ValueType::ArrayOf(_));
            let right_is_array = matches!(right_ty, ValueType::Array | ValueType::ArrayOf(_));
            if !(left_is_array || right_is_array) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::DynamicPow);
                return Ok(ValueType::Any);
            }
        }

        // String comparison - handle early before method table dispatch
        // to avoid MethodError for String == String
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            if matches!(left_julia_ty, JuliaType::String)
                && matches!(right_julia_ty, JuliaType::String)
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::EqStr);
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Tuple comparison - handle early before method table dispatch
        // to avoid MethodError for Tuple == Tuple
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            if matches!(left_julia_ty, JuliaType::Tuple | JuliaType::TupleOf(_))
                || matches!(right_julia_ty, JuliaType::Tuple | JuliaType::TupleOf(_))
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isequal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // BigInt/BigFloat operations - handle early before method table dispatch
        // to avoid MethodError when infer_julia_type correctly returns BigInt/BigFloat
        // (Issue #1910: big() type inference now returns precise types)
        // Issue #2497: Only when the other operand is also a primitive numeric type,
        // not a struct like Rational or Complex (which needs promotion-based dispatch).
        {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let has_bigint = matches!(left_julia_ty, JuliaType::BigInt | JuliaType::Int128)
                || matches!(right_julia_ty, JuliaType::BigInt | JuliaType::Int128);
            let has_bigfloat = matches!(left_julia_ty, JuliaType::BigFloat)
                || matches!(right_julia_ty, JuliaType::BigFloat);
            // Skip BigInt/BigFloat intrinsic shortcut if either operand is a struct type
            // (e.g., Rational, Complex) or Any (unknown at compile time).
            // These need full method dispatch via promote(). (Issue #2497)
            let needs_dispatch = matches!(left_julia_ty, JuliaType::Struct(_) | JuliaType::Any)
                || matches!(right_julia_ty, JuliaType::Struct(_) | JuliaType::Any);
            if (has_bigfloat || has_bigint) && !needs_dispatch {
                // Defer to builtin handling below (which checks ValueType and uses
                // BigInt/BigFloat intrinsics). Skip method dispatch to avoid MethodError.
                let left_ty = self.infer_expr_type(left);
                let right_ty = self.infer_expr_type(right);
                let is_bigint_expr = left_ty == ValueType::BigInt
                    || right_ty == ValueType::BigInt
                    || left_ty == ValueType::I128
                    || right_ty == ValueType::I128;
                let is_bigfloat_expr =
                    left_ty == ValueType::BigFloat || right_ty == ValueType::BigFloat;
                if is_bigfloat_expr {
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    let intrinsic = match op {
                        BinaryOp::Add => Intrinsic::AddBigFloat,
                        BinaryOp::Sub => Intrinsic::SubBigFloat,
                        BinaryOp::Mul => Intrinsic::MulBigFloat,
                        BinaryOp::Div => Intrinsic::DivBigFloat,
                        BinaryOp::Lt => Intrinsic::LtBigFloat,
                        BinaryOp::Le => Intrinsic::LeBigFloat,
                        BinaryOp::Gt => Intrinsic::GtBigFloat,
                        BinaryOp::Ge => Intrinsic::GeBigFloat,
                        BinaryOp::Eq => Intrinsic::EqBigFloat,
                        BinaryOp::Ne => Intrinsic::NeBigFloat,
                        _ => {
                            return err(format!("Unsupported BigFloat operation: {:?}", op));
                        }
                    };
                    self.emit(Instr::CallIntrinsic(intrinsic));
                    let result_ty = match op {
                        BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::Eq
                        | BinaryOp::Ne => ValueType::Bool,
                        _ => ValueType::BigFloat,
                    };
                    return Ok(result_ty);
                }
                if is_bigint_expr {
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    let intrinsic = match op {
                        BinaryOp::Add => Intrinsic::AddBigInt,
                        BinaryOp::Sub => Intrinsic::SubBigInt,
                        BinaryOp::Mul => Intrinsic::MulBigInt,
                        BinaryOp::Div | BinaryOp::IntDiv => Intrinsic::DivBigInt,
                        BinaryOp::Mod => Intrinsic::RemBigInt,
                        BinaryOp::Pow => Intrinsic::PowBigInt,
                        BinaryOp::Lt => Intrinsic::LtBigInt,
                        BinaryOp::Le => Intrinsic::LeBigInt,
                        BinaryOp::Gt => Intrinsic::GtBigInt,
                        BinaryOp::Ge => Intrinsic::GeBigInt,
                        BinaryOp::Eq => Intrinsic::EqBigInt,
                        BinaryOp::Ne => Intrinsic::NeBigInt,
                        _ => {
                            return err(format!("Unsupported BigInt operation: {:?}", op));
                        }
                    };
                    self.emit(Instr::CallIntrinsic(intrinsic));
                    let result_ty = match op {
                        BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::Eq
                        | BinaryOp::Ne => ValueType::Bool,
                        _ => ValueType::BigInt,
                    };
                    return Ok(result_ty);
                }
            }
        }

        // Abstract numeric type dispatch - handle early before method table dispatch (Issue #2498)
        // When a parameter has an abstract numeric type annotation (Number, Real, Integer, etc.),
        // the actual runtime value could be BigInt, BigFloat, or any numeric type.
        // We must use runtime dispatch instead of hardcoded intrinsics like AddFloat/AddInt
        // which would fail for BigInt/BigFloat values.
        {
            let has_abstract_numeric = |expr: &Expr| -> bool {
                if let Expr::Var(name, _) = expr {
                    self.abstract_numeric_params.contains(name)
                } else {
                    false
                }
            };
            if has_abstract_numeric(left) || has_abstract_numeric(right) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // For power operations, use DynamicPow which handles I64^I64 -> I64
                if matches!(op, BinaryOp::Pow) {
                    self.emit(Instr::DynamicPow);
                    return Ok(ValueType::Any);
                }

                let fallback_intrinsic = match op {
                    BinaryOp::Add => Intrinsic::AddFloat,
                    BinaryOp::Sub => Intrinsic::SubFloat,
                    BinaryOp::Mul => Intrinsic::MulFloat,
                    BinaryOp::Div => Intrinsic::DivFloat,
                    BinaryOp::IntDiv => Intrinsic::SdivInt,
                    BinaryOp::Pow => return err("internal: Pow should be handled by DynamicPow"),
                    BinaryOp::Lt => Intrinsic::LtFloat,
                    BinaryOp::Le => Intrinsic::LeFloat,
                    BinaryOp::Gt => Intrinsic::GtFloat,
                    BinaryOp::Ge => Intrinsic::GeFloat,
                    BinaryOp::Eq => Intrinsic::EqFloat,
                    BinaryOp::Ne => Intrinsic::NeFloat,
                    BinaryOp::Mod => Intrinsic::SremInt,
                    BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Egal
                    | BinaryOp::NotEgal
                    | BinaryOp::Subtype => Intrinsic::EqInt,
                };

                // Build candidates from method tables for runtime dispatch
                let op_name = binary_op_to_function_name(op);
                let candidates: Vec<(usize, String, String)> =
                    if let Some(table) = self.method_tables.get(op_name) {
                        table
                            .methods
                            .iter()
                            .filter(|m| {
                                m.params.len() == 2 && {
                                    let is_dispatch_candidate = |ty: &JuliaType| {
                                        matches!(
                                            ty,
                                            JuliaType::Struct(_)
                                                | JuliaType::Number
                                                | JuliaType::Real
                                                | JuliaType::Integer
                                                | JuliaType::Signed
                                                | JuliaType::Unsigned
                                                | JuliaType::AbstractFloat
                                        )
                                    };
                                    is_dispatch_candidate(&m.params[0].1)
                                        || is_dispatch_candidate(&m.params[1].1)
                                }
                            })
                            .map(|m| {
                                let left_type = m.params[0].1.to_string();
                                let right_type = m.params[1].1.to_string();
                                (m.global_index, left_type, right_type)
                            })
                            .collect()
                    } else {
                        vec![]
                    };

                self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

                let result_ty = match op {
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Egal
                    | BinaryOp::NotEgal
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Subtype => ValueType::Bool,
                    _ => ValueType::Any,
                };
                return Ok(result_ty);
            }
        }

        // Char arithmetic - handle early before method table dispatch (Issue #2122)
        // Julia: Char + Int → Char, Int + Char → Char, Char - Int → Char, Char - Char → Int
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let left_is_char = left_ty == ValueType::Char;
            let right_is_char = right_ty == ValueType::Char;
            let has_char = left_is_char || right_is_char;

            if has_char {
                // Compile both operands
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // Emit integer intrinsic (Char values are converted to codepoints at runtime)
                let intrinsic = match op {
                    BinaryOp::Add => Intrinsic::AddInt,
                    BinaryOp::Sub => Intrinsic::SubInt,
                    BinaryOp::Lt => Intrinsic::SltInt,
                    BinaryOp::Le => Intrinsic::SleInt,
                    BinaryOp::Gt => Intrinsic::SgtInt,
                    BinaryOp::Ge => Intrinsic::SgeInt,
                    BinaryOp::Eq => Intrinsic::EqInt,
                    BinaryOp::Ne => Intrinsic::NeInt,
                    _ => return err(format!("internal: unexpected Char operation {:?}", op)),
                };

                // For Char arithmetic, the runtime dispatch handles type-correct results.
                // Use CallDynamicBinaryBoth to let runtime handle Char+Int/Char-Char/etc.
                // since the intrinsic execution path already handles Value::Char
                self.emit(Instr::CallDynamicBinaryBoth(intrinsic, vec![]));

                let result_ty = match op {
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    BinaryOp::Sub if left_is_char && right_is_char => ValueType::I64,
                    BinaryOp::Add | BinaryOp::Sub => ValueType::Char,
                    _ => return err(format!("internal: unexpected Char result type for {:?}", op)),
                };
                return Ok(result_ty);
            }
        }

        // Check for user-defined operator overload
        let op_name = binary_op_to_function_name(op);
        if let Some(table) = self.method_tables.get(op_name) {
            // Infer argument types for dispatch
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let arg_types = vec![left_julia_ty.clone(), right_julia_ty.clone()];

            // Check if all methods are Base extensions
            if table.all_base_extensions() {
                // All methods are Base extensions (e.g., `function Base.:+(...)`)
                // Base extensions do NOT shadow builtins for primitive types.
                // Only use Base extension if:
                // 1. At least one operand is a struct type, AND
                // 2. A matching method exists
                let left_is_struct = matches!(left_julia_ty, JuliaType::Struct(_));
                let right_is_struct = matches!(right_julia_ty, JuliaType::Struct(_));
                let left_is_any = matches!(left_julia_ty, JuliaType::Any);
                let right_is_any = matches!(right_julia_ty, JuliaType::Any);

                if left_is_struct || right_is_struct {
                    // IMPORTANT: Check for (Struct, Any) case BEFORE trying static dispatch.
                    // When one operand is Struct and the other is Any, static dispatch might
                    // incorrectly match a method like (Rational{T}, Int64) because `Any` matches
                    // primitive types at compile time. But at runtime, the Any value might
                    // actually be the same struct type, so we need runtime dispatch to check.
                    if (left_is_struct && right_is_any) || (left_is_any && right_is_struct) {
                        // Build candidates for runtime dispatch
                        // We need to find all methods that match the struct operand
                        let check_position = if right_is_any { 1 } else { 0 };
                        let struct_ty = if left_is_struct {
                            &left_julia_ty
                        } else {
                            &right_julia_ty
                        };

                        // Find methods where the struct position matches
                        // For parametric structs, compare base names (e.g., Rational{Int64} matches Rational{T})
                        let struct_base = if let JuliaType::Struct(name) = struct_ty {
                            if let Some(idx) = name.find('{') {
                                &name[..idx]
                            } else {
                                name.as_str()
                            }
                        } else {
                            ""
                        };
                        let candidates: Vec<(usize, String)> = table
                            .methods
                            .iter()
                            .filter(|m| {
                                m.params.len() == 2 && {
                                    let struct_pos = if check_position == 1 { 0 } else { 1 };
                                    // Compare base struct names for parametric types
                                    if let JuliaType::Struct(method_struct_name) =
                                        &m.params[struct_pos].1
                                    {
                                        let method_base =
                                            if let Some(idx) = method_struct_name.find('{') {
                                                &method_struct_name[..idx]
                                            } else {
                                                method_struct_name.as_str()
                                            };
                                        method_base == struct_base
                                    } else {
                                        false
                                    }
                                }
                            })
                            .map(|m| {
                                let any_pos = check_position;
                                let type_name = m.params[any_pos].1.to_string();
                                (m.global_index, type_name)
                            })
                            .collect();
                        if !candidates.is_empty() {
                            // Use runtime dispatch
                            // Compile both arguments
                            self.compile_expr(left)?;
                            self.compile_expr(right)?;

                            // Use first candidate as fallback
                            let fallback = candidates[0].0;
                            self.emit(Instr::CallDynamicBinary(
                                fallback,
                                check_position,
                                candidates,
                            ));

                            // Return the struct type as result (Complex + Any -> Complex)
                            let result_ty = julia_type_to_value_type(struct_ty);
                            return Ok(result_ty);
                        }
                    }

                    // If (Struct, Any) runtime dispatch had no candidates, or this is not a
                    // (Struct, Any) case, try static dispatch
                    match table.dispatch(&arg_types) {
                        Ok(method) => {
                            return self.compile_user_defined_binary_op(op, left, right, method);
                        }
                        Err(_) => {
                            // Static dispatch failed too
                            // Special case: == comparison on structs without user-defined ==
                            // Falls back to field-by-field comparison (Julia default behavior)
                            if matches!(op, BinaryOp::Eq)
                                && left_is_struct
                                && right_is_struct
                                && !left_is_any
                                && !right_is_any
                            {
                                // Emit EqStruct instruction for default struct comparison
                                self.compile_expr(left)?;
                                self.compile_expr(right)?;
                                self.emit(Instr::EqStruct);
                                return Ok(ValueType::Bool);
                            }
                            // Special case: Complex types with primitives - use runtime dispatch
                            // This handles cases like Complex{Bool} / Float64 where type promotion
                            // should happen at runtime (Complex{Bool} + Float64 -> Complex{Float64})
                            let left_is_complex = matches!(&left_julia_ty, JuliaType::Struct(s) if s.starts_with("Complex"));
                            let right_is_complex = matches!(&right_julia_ty, JuliaType::Struct(s) if s.starts_with("Complex"));
                            let left_is_numeric = left_julia_ty.is_builtin_numeric();
                            let right_is_numeric = right_julia_ty.is_builtin_numeric();

                            if (left_is_complex && right_is_numeric)
                                || (left_is_numeric && right_is_complex)
                                || (left_is_complex && right_is_complex)
                            {
                                // Emit runtime dispatch for Complex operations
                                // Build candidates from all Complex-related methods
                                let candidates: Vec<(usize, String, String)> = table.methods
                                    .iter()
                                    .filter(|m| {
                                        m.params.len() == 2 && {
                                            let has_complex_param = m.params.iter().any(|(_, ty)| {
                                                matches!(ty, JuliaType::Struct(s) if s.starts_with("Complex"))
                                            });
                                            has_complex_param
                                        }
                                    })
                                    .map(|m| {
                                        let left_type = m.params[0].1.to_string();
                                        let right_type = m.params[1].1.to_string();
                                        (m.global_index, left_type, right_type)
                                    })
                                    .collect();

                                self.compile_expr(left)?;
                                self.compile_expr(right)?;

                                let fallback_intrinsic = match op {
                                    BinaryOp::Add => Intrinsic::AddFloat,
                                    BinaryOp::Sub => Intrinsic::SubFloat,
                                    BinaryOp::Mul => Intrinsic::MulFloat,
                                    BinaryOp::Div => Intrinsic::DivFloat,
                                    BinaryOp::Lt => Intrinsic::LtFloat,
                                    BinaryOp::Le => Intrinsic::LeFloat,
                                    BinaryOp::Gt => Intrinsic::GtFloat,
                                    BinaryOp::Ge => Intrinsic::GeFloat,
                                    BinaryOp::Eq => Intrinsic::EqFloat,
                                    BinaryOp::Ne => Intrinsic::NeFloat,
                                    _ => {
                                        return err(format!(
                                            "MethodError: no method matching {}({}, {})",
                                            op_name, left_julia_ty, right_julia_ty
                                        ));
                                    }
                                };

                                self.emit(Instr::CallDynamicBinaryBoth(
                                    fallback_intrinsic,
                                    candidates,
                                ));
                                // Return Complex type for Complex operations
                                return Ok(ValueType::Any);
                            }

                            // For Struct+Struct case where dispatch failed (e.g., Complex{Bool} + Complex{Bool}
                            // when no exact method exists), fall through to runtime dispatch.
                            // The actual runtime types may differ from inferred types (Issue #1055).
                            // For non-struct cases with fully known types, this IS a MethodError.
                            // Issue #2127: Allow String*Char and Char*String to fall through to
                            // builtin string concatenation handler.
                            let is_str_char_mul = matches!(op, BinaryOp::Mul)
                                && (matches!(left_julia_ty, JuliaType::String | JuliaType::Char)
                                    || matches!(
                                        right_julia_ty,
                                        JuliaType::String | JuliaType::Char
                                    ));
                            // Issue #2475: Allow (Primitive, Struct) and (Struct, Primitive) to
                            // fall through to runtime dispatch. E.g., Int32 + Rational{Int32}
                            // needs promotion via +(::Number, ::Number) at runtime.
                            let left_is_numeric = left_julia_ty.is_builtin_numeric();
                            let right_is_numeric = right_julia_ty.is_builtin_numeric();
                            let is_primitive_struct_mix = (left_is_numeric && right_is_struct)
                                || (left_is_struct && right_is_numeric);
                            if !(left_is_any
                                || right_is_any
                                || (left_is_struct && right_is_struct)
                                || is_str_char_mul
                                || is_primitive_struct_mix)
                            {
                                return err(format!(
                                    "MethodError: no method matching {}({}, {})",
                                    op_name, left_julia_ty, right_julia_ty
                                ));
                            }
                        }
                    }
                }

                // Handle (Primitive, Any), (Any, Primitive), (Primitive, Struct), and
                // (Struct, Primitive) cases with runtime dispatch.
                // This is needed for cases like `1 + rational(3, 2)` where one operand
                // is a known primitive and the other could be a struct at runtime,
                // or `Int32(1) + Rational{Int32}(1, 2)` where promotion is needed (Issue #2475).
                let left_is_primitive = left_julia_ty.is_builtin_numeric();
                let right_is_primitive = right_julia_ty.is_builtin_numeric();
                let left_is_struct_here = matches!(left_julia_ty, JuliaType::Struct(_));
                let right_is_struct_here = matches!(right_julia_ty, JuliaType::Struct(_));

                if (left_is_primitive && right_is_any)
                    || (left_is_any && right_is_primitive)
                    || (left_is_primitive && right_is_struct_here)
                    || (left_is_struct_here && right_is_primitive)
                {
                    // Build candidates for runtime dispatch
                    let candidates: Vec<(usize, String, String)> = table
                        .methods
                        .iter()
                        .filter(|m| {
                            m.params.len() == 2 && {
                                // Include methods where at least one operand is a struct type
                                // or an abstract numeric type (Number, Real, etc.) for
                                // promotion fallbacks like +(::Number, ::Number)
                                let is_dispatch_candidate = |ty: &JuliaType| {
                                    matches!(
                                        ty,
                                        JuliaType::Struct(_)
                                            | JuliaType::Number
                                            | JuliaType::Real
                                            | JuliaType::Integer
                                            | JuliaType::Signed
                                            | JuliaType::Unsigned
                                            | JuliaType::AbstractFloat
                                    )
                                };
                                is_dispatch_candidate(&m.params[0].1)
                                    || is_dispatch_candidate(&m.params[1].1)
                            }
                        })
                        .map(|m| {
                            let left_type = m.params[0].1.to_string();
                            let right_type = m.params[1].1.to_string();
                            (m.global_index, left_type, right_type)
                        })
                        .collect();

                    if !candidates.is_empty() {
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;

                        // Power operator uses DynamicPow to preserve I64^I64 -> I64
                        if matches!(op, BinaryOp::Pow) {
                            self.emit(Instr::DynamicPow);
                            return Ok(ValueType::Any);
                        }

                        let fallback_intrinsic = match op {
                            BinaryOp::Add => Intrinsic::AddFloat,
                            BinaryOp::Sub => Intrinsic::SubFloat,
                            BinaryOp::Mul => Intrinsic::MulFloat,
                            BinaryOp::Div => Intrinsic::DivFloat,
                            BinaryOp::IntDiv => Intrinsic::SdivInt,
                            BinaryOp::Pow => return err("internal: Pow should be handled by DynamicPow"),
                            BinaryOp::Lt => Intrinsic::LtFloat,
                            BinaryOp::Le => Intrinsic::LeFloat,
                            BinaryOp::Gt => Intrinsic::GtFloat,
                            BinaryOp::Ge => Intrinsic::GeFloat,
                            BinaryOp::Eq => Intrinsic::EqFloat,
                            BinaryOp::Ne => Intrinsic::NeFloat,
                            BinaryOp::Mod => Intrinsic::SremInt,
                            // Logical/special operators don't have intrinsic fallbacks
                            BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Egal
                            | BinaryOp::NotEgal
                            | BinaryOp::Subtype => {
                                return err(format!(
                                    "No method found for operator {:?} with dynamic types",
                                    op
                                ));
                            }
                        };

                        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));
                        return Ok(ValueType::Any);
                    }
                }

                // Handle (Any, Any) and (Struct, Struct) cases with runtime dispatch.
                // For (Struct, Struct), the static dispatch may have failed because the
                // inferred types (e.g., Complex{Bool}) don't match the actual methods
                // (e.g., Complex{Int64}). We need runtime dispatch. (fixes Issue #1055)
                if ((left_is_any && right_is_any) || (left_is_struct && right_is_struct))
                    && !table.methods.is_empty()
                {
                    // Build candidates from methods that take struct or abstract numeric types
                    let candidates: Vec<(usize, String, String)> = table
                        .methods
                        .iter()
                        .filter(|m| {
                            m.params.len() == 2 && {
                                // Include methods where at least one operand is a struct type
                                // or an abstract numeric type (Number, Real, etc.) for
                                // promotion fallbacks like +(::Number, ::Number)
                                let is_dispatch_candidate = |ty: &JuliaType| {
                                    matches!(
                                        ty,
                                        JuliaType::Struct(_)
                                            | JuliaType::Number
                                            | JuliaType::Real
                                            | JuliaType::Integer
                                            | JuliaType::Signed
                                            | JuliaType::Unsigned
                                            | JuliaType::AbstractFloat
                                    )
                                };
                                is_dispatch_candidate(&m.params[0].1)
                                    || is_dispatch_candidate(&m.params[1].1)
                            }
                        })
                        .map(|m| {
                            let left_type = m.params[0].1.to_string();
                            let right_type = m.params[1].1.to_string();
                            (m.global_index, left_type, right_type)
                        })
                        .collect();

                    if !candidates.is_empty() {
                        // Compile both operands
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;

                        // Power operator uses DynamicPow to preserve I64^I64 -> I64
                        if matches!(op, BinaryOp::Pow) {
                            self.emit(Instr::DynamicPow);
                            return Ok(ValueType::Any);
                        }

                        // Determine fallback intrinsic based on operation
                        let fallback_intrinsic = match op {
                            BinaryOp::Add => Intrinsic::AddFloat,
                            BinaryOp::Sub => Intrinsic::SubFloat,
                            BinaryOp::Mul => Intrinsic::MulFloat,
                            BinaryOp::Div => Intrinsic::DivFloat,
                            BinaryOp::IntDiv => Intrinsic::SdivInt,
                            BinaryOp::Pow => return err("internal: Pow should be handled by DynamicPow"),
                            BinaryOp::Lt => Intrinsic::LtFloat,
                            BinaryOp::Le => Intrinsic::LeFloat,
                            BinaryOp::Gt => Intrinsic::GtFloat,
                            BinaryOp::Ge => Intrinsic::GeFloat,
                            BinaryOp::Eq => Intrinsic::EqFloat,
                            BinaryOp::Ne => Intrinsic::NeFloat,
                            BinaryOp::Mod => Intrinsic::SremInt,
                            // Logical/special operators don't have intrinsic fallbacks
                            BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Egal
                            | BinaryOp::NotEgal
                            | BinaryOp::Subtype => {
                                // These should be handled by method dispatch, not intrinsics
                                return err(format!(
                                    "No method found for operator {:?} with dynamic types",
                                    op
                                ));
                            }
                        };

                        // Emit runtime dispatch instruction
                        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

                        // Return Any since we don't know the result type at compile time
                        return Ok(ValueType::Any);
                    }
                }

                // Issue #1759: Try dispatch for ALL types, not just structs.
                // Skip dispatch only when both operands are the SAME primitive numeric type
                // (e.g., Int64 + Int64, Float64 + Float64) — these use intrinsics directly.
                // Mixed-type primitives (e.g., Int64 + Float64) go through Julia's
                // +(::Number, ::Number) → promote() → convert() chain from promotion.jl,
                // matching official Julia behavior.
                let left_is_builtin_numeric = left_julia_ty.is_builtin_numeric();
                let right_is_builtin_numeric = right_julia_ty.is_builtin_numeric();
                let both_same_primitive = left_is_builtin_numeric
                    && right_is_builtin_numeric
                    && left_julia_ty == right_julia_ty;
                if !both_same_primitive {
                    if let Ok(method) = table.dispatch(&arg_types) {
                        return self.compile_user_defined_binary_op(op, left, right, method);
                    }
                }

                // Fall through to builtin handling only if no method matches
            } else {
                // At least one method is NOT a Base extension (regular user-defined operator).
                // Julia semantics: this shadows Base.op completely.
                // However, Base extension methods should still be available for dispatch.

                // IMPORTANT: When any arg is Any and struct-typed methods exist, skip static
                // dispatch (Issue #1055, #1783). Static dispatch with Any incorrectly matches
                // primitive methods (e.g., +(::Float32, ::Int64)) over struct methods
                // (e.g., +(::Rational{T}, ::Int64)) because Any is a subtype of all primitives
                // and Float32 has higher specificity than Rational{T}. At runtime, the Any
                // value could be a struct, so we need runtime dispatch.
                let any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
                let has_struct_methods = any_arg
                    && table.methods.iter().any(|m| {
                        m.params.len() == 2
                            && m.params
                                .iter()
                                .any(|(_, ty)| matches!(ty, JuliaType::Struct(_)))
                    });
                let skip_static_dispatch = any_arg && has_struct_methods;

                // First, try to dispatch to any method (user-defined or Base extension)
                // Skip dispatch when Any args + struct methods exist to avoid wrong matches.
                // Only skip for same-type primitive numerics (e.g., Int64+Int64) which use
                // intrinsics directly. Mixed-type primitives (e.g., Int64+Float64) go through
                // Julia's +(::Number, ::Number) → promote() chain from promotion.jl.
                let both_same_primitive = left_julia_ty.is_builtin_numeric()
                    && right_julia_ty.is_builtin_numeric()
                    && left_julia_ty == right_julia_ty;
                let dispatch_result = if skip_static_dispatch || both_same_primitive {
                    Err(DispatchError::NoMethodFound {
                        name: op_name.to_string(),
                        arg_types: arg_types.clone(),
                    })
                } else {
                    table.dispatch(&arg_types)
                };
                match dispatch_result {
                    Ok(method) => {
                        return self.compile_user_defined_binary_op(op, left, right, method);
                    }
                    Err(_) => {
                        // No matching method found. For primitive types, we should still
                        // allow builtin operators to work. But if the user has defined
                        // a non-Base-extension operator, it shadows the builtin for all types.
                        // However, Base extension methods should still be available.
                        // Check if there are any Base extension methods that could match
                        // (for runtime dispatch cases)
                        let has_base_extensions = table.methods.iter().any(|m| m.is_base_extension);

                        // Handle dynamic dispatch cases (fixes Issue #1055, #1783)
                        // Case 1: Both operands are Any at compile time but could be structs at runtime
                        // Case 2: Both operands are Struct but dispatch failed (e.g., Complex{Bool} vs Complex{Int64})
                        // Case 3: One operand is Any and the other is Primitive (Issue #1783)
                        //         The Any value could be a struct at runtime, so runtime dispatch is needed
                        let left_is_any = matches!(left_julia_ty, JuliaType::Any);
                        let right_is_any = matches!(right_julia_ty, JuliaType::Any);
                        let left_is_struct = matches!(left_julia_ty, JuliaType::Struct(_));
                        let right_is_struct = matches!(right_julia_ty, JuliaType::Struct(_));
                        let left_is_primitive = left_julia_ty.is_builtin_numeric();
                        let right_is_primitive = right_julia_ty.is_builtin_numeric();
                        // Issue #2425: Include (Struct, Any) and (Any, Struct) cases.
                        // When one operand is a known struct type and the other is Any
                        // (e.g., Complex{Float64} * log(z) where log returns Any at compile time),
                        // runtime dispatch is needed to find the correct struct method.
                        // Without these cases, the code falls through to builtin handling
                        // which incorrectly converts the struct to F64 via DynamicToF64.
                        let needs_runtime_dispatch = (left_is_struct || left_is_any)
                            && (right_is_struct || right_is_any)
                            || (left_is_any && right_is_primitive)
                            || (left_is_primitive && right_is_any);
                        if needs_runtime_dispatch && has_base_extensions {
                            // Build candidates from methods that take struct or abstract numeric types
                            let candidates: Vec<(usize, String, String)> = table
                                .methods
                                .iter()
                                .filter(|m| {
                                    m.params.len() == 2 && {
                                        // Include methods where at least one operand is a struct type
                                        // or an abstract numeric type (Number, Real, etc.) for
                                        // promotion fallbacks like +(::Number, ::Number)
                                        let is_dispatch_candidate = |ty: &JuliaType| {
                                            matches!(
                                                ty,
                                                JuliaType::Struct(_)
                                                    | JuliaType::Number
                                                    | JuliaType::Real
                                                    | JuliaType::Integer
                                                    | JuliaType::Signed
                                                    | JuliaType::Unsigned
                                                    | JuliaType::AbstractFloat
                                            )
                                        };
                                        is_dispatch_candidate(&m.params[0].1)
                                            || is_dispatch_candidate(&m.params[1].1)
                                    }
                                })
                                .map(|m| {
                                    let left_type = m.params[0].1.to_string();
                                    let right_type = m.params[1].1.to_string();
                                    (m.global_index, left_type, right_type)
                                })
                                .collect();

                            if !candidates.is_empty() {
                                self.compile_expr(left)?;
                                self.compile_expr(right)?;

                                let fallback_intrinsic = match op {
                                    BinaryOp::Add => Intrinsic::AddFloat,
                                    BinaryOp::Sub => Intrinsic::SubFloat,
                                    BinaryOp::Mul => Intrinsic::MulFloat,
                                    BinaryOp::Div => Intrinsic::DivFloat,
                                    BinaryOp::IntDiv => Intrinsic::SdivInt,
                                    BinaryOp::Pow => Intrinsic::PowFloat,
                                    BinaryOp::Lt => Intrinsic::LtFloat,
                                    BinaryOp::Le => Intrinsic::LeFloat,
                                    BinaryOp::Gt => Intrinsic::GtFloat,
                                    BinaryOp::Ge => Intrinsic::GeFloat,
                                    BinaryOp::Eq => Intrinsic::EqFloat,
                                    BinaryOp::Ne => Intrinsic::NeFloat,
                                    BinaryOp::Mod => Intrinsic::SremInt,
                                    _ => Intrinsic::AddFloat, // Default fallback
                                };

                                self.emit(Instr::CallDynamicBinaryBoth(
                                    fallback_intrinsic,
                                    candidates,
                                ));
                                return Ok(ValueType::Any);
                            }
                        }

                        if has_base_extensions {
                            // There are Base extension methods, but they didn't match at compile time.
                            // This could be a runtime dispatch case. Fall through to builtin handling
                            // for primitive types, which will use builtin operators.
                        } else {
                            // No Base extension methods, and no user-defined method matched.
                            // This is a MethodError - no fallback to builtin.
                            // Issue #2127: Allow String*Char to fall through to string concatenation
                            let is_str_char_mul = matches!(op, BinaryOp::Mul)
                                && (matches!(left_julia_ty, JuliaType::String | JuliaType::Char)
                                    || matches!(
                                        right_julia_ty,
                                        JuliaType::String | JuliaType::Char
                                    ));
                            if !is_str_char_mul {
                                return err(format!(
                                    "MethodError: no method matching {}({}, {})",
                                    op_name, left_julia_ty, right_julia_ty
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Builtin operator handling
        let left_ty = self.infer_expr_type(left);
        let right_ty = self.infer_expr_type(right);

        // Default struct equality comparison (when no user-defined == exists)
        // This handles structs that have no operator methods defined at all
        if matches!(op, BinaryOp::Eq) {
            let left_is_struct = matches!(left_ty, ValueType::Struct(_));
            let right_is_struct = matches!(right_ty, ValueType::Struct(_));
            if left_is_struct && right_is_struct {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::EqStruct);
                return Ok(ValueType::Bool);
            }
        }

        // String comparison: "a" == "b", "a" != "b", "a" < "b", etc.
        if matches!(
            op,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) {
            let left_is_str = matches!(left_ty, ValueType::Str);
            let right_is_str = matches!(right_ty, ValueType::Str);
            if left_is_str && right_is_str {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                    self.emit(Instr::EqStr);
                    if matches!(op, BinaryOp::Ne) {
                        self.emit(Instr::NotBool);
                    }
                } else {
                    // String ordering comparison (lexicographic, Issue #2025)
                    match op {
                        BinaryOp::Lt => self.emit(Instr::LtStr),
                        BinaryOp::Le => self.emit(Instr::LeStr),
                        BinaryOp::Gt => self.emit(Instr::GtStr),
                        BinaryOp::Ge => self.emit(Instr::GeStr),
                        _ => return err(format!("internal: unexpected string comparison operator {:?}", op)),
                    };
                }
                return Ok(ValueType::Bool);
            }
        }

        // Singleton equality comparison: x == nothing, typeof(x) == T, :foo == :bar, 'a' == 'b'
        // For singleton types, equality (==) and identity (===) are semantically equivalent.
        // Uses identity comparison (===) via BuiltinId::Egal for all singleton types.
        // This ensures proper type narrowing: `if x != nothing` works like `if x !== nothing`.
        // SINGLETON_HANDLING: When modifying identity ops, update equality ops too.
        // See also: is_singleton_type() in compile/mod.rs
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_is_singleton = is_singleton_type(&left_ty);
            let right_is_singleton = is_singleton_type(&right_ty);
            // Also check if the literal is nothing (for cases where type inference returns Any)
            let left_is_nothing_lit =
                matches!(left, Expr::Literal(crate::ir::core::Literal::Nothing, _));
            let right_is_nothing_lit =
                matches!(right, Expr::Literal(crate::ir::core::Literal::Nothing, _));
            if left_is_singleton
                || right_is_singleton
                || left_is_nothing_lit
                || right_is_nothing_lit
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Egal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Array comparison: [1,2,3] == [1,2,3]
        // Uses isequal builtin which handles array element comparison
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_is_array = matches!(left_ty, ValueType::Array | ValueType::ArrayOf(_));
            let right_is_array = matches!(right_ty, ValueType::Array | ValueType::ArrayOf(_));
            if left_is_array || right_is_array {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isequal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Matrix/vector multiplication: A * v or A * B
        if matches!(op, BinaryOp::Mul) {
            let left_is_array = matches!(left_ty, ValueType::Array | ValueType::ArrayOf(_));
            let right_is_array = matches!(right_ty, ValueType::Array | ValueType::ArrayOf(_));
            if left_is_array && right_is_array {
                // Compile both operands as arrays
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::MatMul);
                return Ok(ValueType::Array);
            }

            // Scalar * Array or Array * Scalar: use dynamic dispatch
            // This handles cases like Float64 * Complex array or Complex scalar * Float64 array
            let left_is_scalar =
                is_numeric_type(&left_ty) || matches!(left_ty, ValueType::Struct(_));
            let right_is_scalar =
                is_numeric_type(&right_ty) || matches!(right_ty, ValueType::Struct(_));

            if (left_is_scalar && right_is_array) || (left_is_array && right_is_scalar) {
                // Use CallDynamicBinaryBoth with MulFloat fallback
                // Runtime will detect complex arrays and handle appropriately
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // Collect any user-defined * methods for dynamic dispatch
                let candidates: Vec<(usize, String, String)> =
                    if let Some(table) = self.method_tables.get("*") {
                        table
                            .methods
                            .iter()
                            .filter(|m| m.params.len() == 2)
                            .map(|m| {
                                (
                                    m.global_index,
                                    m.params[0].1.to_string(),
                                    m.params[1].1.to_string(),
                                )
                            })
                            .collect()
                    } else {
                        vec![]
                    };

                self.emit(Instr::CallDynamicBinaryBoth(
                    Intrinsic::MulFloat,
                    candidates,
                ));
                return Ok(ValueType::Array);
            }
        }

        // Array / Scalar or Scalar / Array: use dynamic dispatch (Issue #1929)
        // In Julia, v / n is equivalent to v ./ n (element-wise broadcast division)
        if matches!(op, BinaryOp::Div) {
            let left_is_array = matches!(left_ty, ValueType::Array | ValueType::ArrayOf(_));
            let right_is_array = matches!(right_ty, ValueType::Array | ValueType::ArrayOf(_));
            let left_is_scalar =
                is_numeric_type(&left_ty) || matches!(left_ty, ValueType::Struct(_));
            let right_is_scalar =
                is_numeric_type(&right_ty) || matches!(right_ty, ValueType::Struct(_));

            if (left_is_scalar && right_is_array) || (left_is_array && right_is_scalar) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // Collect any user-defined / methods for dynamic dispatch
                let candidates: Vec<(usize, String, String)> =
                    if let Some(table) = self.method_tables.get("/") {
                        table
                            .methods
                            .iter()
                            .filter(|m| m.params.len() == 2)
                            .map(|m| {
                                (
                                    m.global_index,
                                    m.params[0].1.to_string(),
                                    m.params[1].1.to_string(),
                                )
                            })
                            .collect()
                    } else {
                        vec![]
                    };

                self.emit(Instr::CallDynamicBinaryBoth(
                    Intrinsic::DivFloat,
                    candidates,
                ));
                return Ok(ValueType::Array);
            }
        }

        // Array element-wise arithmetic: A + B, A - B, A / B
        // In Julia, + and - on arrays are element-wise, while * is matrix multiplication.
        // These are dispatched through DynamicAdd/Sub/Div which handle Array operands at runtime.
        let left_is_array = matches!(left_ty, ValueType::Array | ValueType::ArrayOf(_));
        let right_is_array = matches!(right_ty, ValueType::Array | ValueType::ArrayOf(_));
        if left_is_array && right_is_array {
            let dynamic_instr = match op {
                BinaryOp::Add => Some(Instr::DynamicAdd),
                BinaryOp::Sub => Some(Instr::DynamicSub),
                BinaryOp::Div => Some(Instr::DynamicDiv),
                _ => None,
            };
            if let Some(instr) = dynamic_instr {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(instr);
                return Ok(ValueType::Array);
            }
        }

        // String concatenation: "a" * "b" => "ab" (Julia uses * for string concatenation)
        // Also handle Any * Str, Str * Any, Str * Char, Char * Str for dynamic dispatch cases
        // Issue #2127: Include Char operands since Julia's * converts Char to String
        let left_could_be_str =
            matches!(left_ty, ValueType::Str | ValueType::Any | ValueType::Char);
        let right_could_be_str =
            matches!(right_ty, ValueType::Str | ValueType::Any | ValueType::Char);
        if matches!(op, BinaryOp::Mul)
            && (left_ty == ValueType::Str
                || right_ty == ValueType::Str
                || (left_could_be_str && right_could_be_str))
        {
            // Compile both operands (they will be converted to strings by StringConcat)
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::StringConcat(2));
            return Ok(ValueType::Str);
        }

        // BigInt/Int128 arithmetic operations - if either operand is BigInt or Int128, use BigInt intrinsics
        // Int128 operations also use BigInt intrinsics since pop_bigint promotes I128 to BigInt
        // Issue #2497: Skip when other operand is a struct (Rational, Complex) or Any
        // (which could be a struct at runtime) — these need promotion-based dispatch.
        let is_bigint = left_ty == ValueType::BigInt
            || right_ty == ValueType::BigInt
            || left_ty == ValueType::I128
            || right_ty == ValueType::I128;
        let other_needs_dispatch = matches!(left_ty, ValueType::Struct(_) | ValueType::Any)
            || matches!(right_ty, ValueType::Struct(_) | ValueType::Any);
        if is_bigint && !other_needs_dispatch {
            // Compile both operands (BigInt intrinsics handle I64 promotion)
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            let intrinsic = match op {
                BinaryOp::Add => Intrinsic::AddBigInt,
                BinaryOp::Sub => Intrinsic::SubBigInt,
                BinaryOp::Mul => Intrinsic::MulBigInt,
                BinaryOp::Div | BinaryOp::IntDiv => Intrinsic::DivBigInt, // ÷ uses same intrinsic for BigInt
                BinaryOp::Mod => Intrinsic::RemBigInt,
                BinaryOp::Pow => Intrinsic::PowBigInt, // Issue #1708: BigInt power with Int64 exponent
                BinaryOp::Lt => Intrinsic::LtBigInt,
                BinaryOp::Le => Intrinsic::LeBigInt,
                BinaryOp::Gt => Intrinsic::GtBigInt,
                BinaryOp::Ge => Intrinsic::GeBigInt,
                BinaryOp::Eq => Intrinsic::EqBigInt,
                BinaryOp::Ne => Intrinsic::NeBigInt,
                _ => return err(format!("Unsupported BigInt operation: {:?}", op)),
            };
            self.emit(Instr::CallIntrinsic(intrinsic));
            // Comparison operations return Bool, others return BigInt
            let result_ty = match op {
                BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne => ValueType::Bool,
                _ => ValueType::BigInt,
            };
            return Ok(result_ty);
        }

        // BigFloat arithmetic operations - if either operand is BigFloat, use BigFloat intrinsics
        // Issue #2497: Skip when other operand is a struct/Any — needs promotion dispatch
        let is_bigfloat = left_ty == ValueType::BigFloat || right_ty == ValueType::BigFloat;
        if is_bigfloat && !other_needs_dispatch {
            // Compile both operands (BigFloat intrinsics handle F64/I64 promotion)
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            let intrinsic = match op {
                BinaryOp::Add => Intrinsic::AddBigFloat,
                BinaryOp::Sub => Intrinsic::SubBigFloat,
                BinaryOp::Mul => Intrinsic::MulBigFloat,
                BinaryOp::Div => Intrinsic::DivBigFloat,
                BinaryOp::Lt => Intrinsic::LtBigFloat,
                BinaryOp::Le => Intrinsic::LeBigFloat,
                BinaryOp::Gt => Intrinsic::GtBigFloat,
                BinaryOp::Ge => Intrinsic::GeBigFloat,
                BinaryOp::Eq => Intrinsic::EqBigFloat,
                BinaryOp::Ne => Intrinsic::NeBigFloat,
                _ => return err(format!("Unsupported BigFloat operation: {:?}", op)),
            };
            self.emit(Instr::CallIntrinsic(intrinsic));
            // Comparison operations return Bool, others return BigFloat
            let result_ty = match op {
                BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne => ValueType::Bool,
                _ => ValueType::BigFloat,
            };
            return Ok(result_ty);
        }

        // Non-primitive power (e.g., Rational, Complex): use DynamicPow for runtime dispatch.
        if matches!(op, BinaryOp::Pow) {
            let left_primitive = matches!(left_ty, ValueType::I64 | ValueType::F64);
            let right_primitive = matches!(right_ty, ValueType::I64 | ValueType::F64);
            if !(left_primitive && right_primitive) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::DynamicPow);
                return Ok(ValueType::Any);
            }
        }

        // Note: Complex operations are handled via Pure Julia method dispatch above.
        // This fallback code is for primitive types only (I64, F64).
        // Complex arithmetic/comparison uses base/complex.jl with Base.:+ etc.

        // Check if either operand is a float type (F16, F32, F64)
        let has_float = is_float_type(&left_ty) || is_float_type(&right_ty);
        // Check if both operands are F32 (for type preservation)
        let both_f32 = left_ty == ValueType::F32 && right_ty == ValueType::F32;
        // Check if one operand is F32 and the other is promotable to F32 (not F64)
        // Issue #1759: Float32 + Bool should return Float32, not Float64
        let has_f64 = left_ty == ValueType::F64 || right_ty == ValueType::F64;
        let has_f32 = left_ty == ValueType::F32 || right_ty == ValueType::F32;
        let one_f32_other_promotable = has_f32 && !has_f64;
        // Check if both operands are F16 or one is F16 with promotable type (Issue #1972)
        let both_f16 = left_ty == ValueType::F16 && right_ty == ValueType::F16;
        let one_f16_other_promotable =
            (left_ty == ValueType::F16 || right_ty == ValueType::F16) && !has_f64 && !has_f32;
        // Issue #2123: Char arithmetic type tracking
        let left_is_char = left_ty == ValueType::Char;
        let right_is_char = right_ty == ValueType::Char;
        let has_char = left_is_char || right_is_char;

        // Check if either operand is Any type (e.g., function call results)
        // When Any is involved, use runtime dispatch to determine the correct operation
        // based on actual runtime types (e.g., real(z) + imag(z) where types are unknown at compile time)
        let has_any = left_ty == ValueType::Any || right_ty == ValueType::Any;

        // Issue #2497: Also need runtime dispatch when mixing BigInt/BigFloat with struct types
        // (e.g., big(2) + Rational{Int64}(1,3)). The early BigInt guard already skips intrinsics
        // for these cases, but we also need to prevent the I64 fallback below from being reached.
        let needs_mixed_dispatch = matches!(
            (&left_ty, &right_ty),
            (
                ValueType::BigInt | ValueType::BigFloat,
                ValueType::Struct(_)
            ) | (
                ValueType::Struct(_),
                ValueType::BigInt | ValueType::BigFloat
            )
        );

        // If either operand is Any or needs mixed dispatch, use runtime dispatch
        if has_any || needs_mixed_dispatch {
            // Compile both operands without type conversion
            self.compile_expr(left)?;
            self.compile_expr(right)?;

            // For power operations, use DynamicPow which correctly handles I64^I64 -> I64
            if matches!(op, BinaryOp::Pow) {
                self.emit(Instr::DynamicPow);
                return Ok(ValueType::Any);
            }

            // Determine fallback intrinsic based on operation
            let fallback_intrinsic = match op {
                BinaryOp::Add => Intrinsic::AddFloat,
                BinaryOp::Sub => Intrinsic::SubFloat,
                BinaryOp::Mul => Intrinsic::MulFloat,
                BinaryOp::Div => Intrinsic::DivFloat,
                BinaryOp::IntDiv => Intrinsic::SdivInt,
                BinaryOp::Pow => return err("internal: Pow should be handled by DynamicPow"),
                BinaryOp::Lt => Intrinsic::LtFloat,
                BinaryOp::Le => Intrinsic::LeFloat,
                BinaryOp::Gt => Intrinsic::GtFloat,
                BinaryOp::Ge => Intrinsic::GeFloat,
                BinaryOp::Eq => Intrinsic::EqFloat,
                BinaryOp::Ne => Intrinsic::NeFloat,
                BinaryOp::Mod => Intrinsic::SremInt, // Runtime dispatch handles BigInt via RemBigInt
                BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Egal
                | BinaryOp::NotEgal
                | BinaryOp::Subtype => Intrinsic::EqInt,
            };

            // Build candidates from struct/abstract-accepting methods for runtime dispatch (fixes Issue #1055)
            // When Any is involved at compile time, operands could be structs at runtime.
            let op_name = binary_op_to_function_name(op);
            let candidates: Vec<(usize, String, String)> =
                if let Some(table) = self.method_tables.get(op_name) {
                    table
                        .methods
                        .iter()
                        .filter(|m| {
                            m.params.len() == 2 && {
                                // Include methods where at least one operand is a struct type
                                // or an abstract numeric type (Number, Real, etc.) for
                                // promotion fallbacks like +(::Number, ::Number)
                                let is_dispatch_candidate = |ty: &JuliaType| {
                                    matches!(
                                        ty,
                                        JuliaType::Struct(_)
                                            | JuliaType::Number
                                            | JuliaType::Real
                                            | JuliaType::Integer
                                            | JuliaType::Signed
                                            | JuliaType::Unsigned
                                            | JuliaType::AbstractFloat
                                    )
                                };
                                is_dispatch_candidate(&m.params[0].1)
                                    || is_dispatch_candidate(&m.params[1].1)
                            }
                        })
                        .map(|m| {
                            let left_type = m.params[0].1.to_string();
                            let right_type = m.params[1].1.to_string();
                            (m.global_index, left_type, right_type)
                        })
                        .collect()
                } else {
                    vec![]
                };

            self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

            // Return Any since we don't know the result type at compile time
            // (except for comparisons which always return Bool)
            let result_ty = match op {
                BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Egal
                | BinaryOp::NotEgal
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Subtype => ValueType::Bool,
                _ => ValueType::Any,
            };
            return Ok(result_ty);
        }

        // Check if both operands are numeric (for type promotion)

        let result_ty = match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32 // Preserve Float32 when F32 + promotable type (Issue #1759)
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Preserve Float16 (Issue #1972)
                } else if has_float {
                    ValueType::F64
                } else if has_char && matches!(op, BinaryOp::Add) {
                    // Issue #2123: Char + Int -> Char, Int + Char -> Char
                    ValueType::Char
                } else if has_char && matches!(op, BinaryOp::Sub) && left_is_char && !right_is_char
                {
                    // Issue #2123: Char - Int -> Char (but Char - Char -> Int)
                    ValueType::Char
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type (e.g., Int8+Int8 -> Int8)
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Div => {
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32 // Float32 / promotable -> Float32 (Issue #1759)
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Float16 / promotable -> Float16 (Issue #1972)
                } else {
                    ValueType::F64
                }
            }
            BinaryOp::Pow => {
                // Power operator: Int^Int -> Int, otherwise -> Float64 (Julia semantics)
                if !has_float {
                    if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                        // Issue #2278: Preserve small integer type
                        small_ty
                    } else {
                        ValueType::I64
                    }
                } else if both_f32 || one_f32_other_promotable {
                    ValueType::F32
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Issue #1972
                } else {
                    ValueType::F64
                }
            }
            BinaryOp::IntDiv => {
                // Integer division: preserve float type, otherwise I64
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Issue #1972
                } else if has_float {
                    ValueType::F64
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Mod => {
                // Modulo: preserve float type, otherwise I64
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Issue #1972
                } else if has_float {
                    ValueType::F64
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

        // For comparisons, use the operand types, not the result type (which is always Bool)
        // Note: has_any cases are handled above with CallDynamicBinaryBoth
        // Issue #1759: Use F64 operand type for all float operations because intrinsics only
        // support I64 and F64. For F32 results, we convert F64 back to F32 at the end.
        let operand_ty = match op {
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            // For division: always use F64 for computation
            BinaryOp::Div => ValueType::F64,
            // For multiplication with floats, use F64
            BinaryOp::Mul => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            // For Add/Sub with floats, use F64
            BinaryOp::Add | BinaryOp::Sub => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            // Mod with floats: use F64 operand type for computation, result converted back at end
            BinaryOp::Mod => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            _ => result_ty.clone(),
        };

        self.compile_expr_as(left, operand_ty.clone())?;
        self.compile_expr_as(right, operand_ty.clone())?;

        // Map (BinaryOp, ValueType) to Intrinsic, using CallIntrinsic for Julia-like semantics.
        // This mirrors Julia's design where `1 + 2` calls `Base.add_int(1, 2)`.
        let intrinsic_opt = match (op, operand_ty) {
            // Integer arithmetic -> add_int, sub_int, mul_int, sdiv_int, srem_int
            (BinaryOp::Add, ValueType::I64) => Some(Intrinsic::AddInt),
            (BinaryOp::Sub, ValueType::I64) => Some(Intrinsic::SubInt),
            (BinaryOp::Mul, ValueType::I64) => Some(Intrinsic::MulInt),
            (BinaryOp::IntDiv, ValueType::I64) => Some(Intrinsic::SdivInt),
            (BinaryOp::Mod, ValueType::I64) => Some(Intrinsic::SremInt),
            // Float mod: use DynamicMod for fmod semantics and type preservation (Issue #1762)
            (BinaryOp::Mod, ValueType::F64) => None, // Will use DynamicMod below
            // Float int div: use DynamicIntDiv for type preservation (Issue #1970)
            (BinaryOp::IntDiv, ValueType::F64) => None, // Will use DynamicIntDiv below
            // Power: Use DynamicPow for all cases to preserve integer arithmetic when appropriate
            (BinaryOp::Pow, ValueType::I64) => None, // Will use DynamicPow below
            (BinaryOp::Pow, ValueType::F64) => None, // Will use DynamicPow below

            // Float64 arithmetic -> add_float, sub_float, mul_float, div_float, pow_float
            // Note: Float32 operations also use these intrinsics with F64 operand_ty,
            // and the result is converted back to F32 at the end (Issue #1759).
            (BinaryOp::Add, ValueType::F64) => Some(Intrinsic::AddFloat),
            (BinaryOp::Sub, ValueType::F64) => Some(Intrinsic::SubFloat),
            (BinaryOp::Mul, ValueType::F64) => Some(Intrinsic::MulFloat),
            (BinaryOp::Div, ValueType::F64) => Some(Intrinsic::DivFloat),

            // Integer comparisons -> eq_int, ne_int, slt_int, sle_int, sgt_int, sge_int
            (BinaryOp::Eq, ValueType::I64) => Some(Intrinsic::EqInt),
            (BinaryOp::Ne, ValueType::I64) => Some(Intrinsic::NeInt),
            (BinaryOp::Lt, ValueType::I64) => Some(Intrinsic::SltInt),
            (BinaryOp::Le, ValueType::I64) => Some(Intrinsic::SleInt),
            (BinaryOp::Gt, ValueType::I64) => Some(Intrinsic::SgtInt),
            (BinaryOp::Ge, ValueType::I64) => Some(Intrinsic::SgeInt),

            // Float64 comparisons -> eq_float, ne_float, lt_float, le_float, gt_float, ge_float
            // Note: Float32 comparisons also use these intrinsics with F64 operand_ty (Issue #1759).
            (BinaryOp::Eq, ValueType::F64) => Some(Intrinsic::EqFloat),
            (BinaryOp::Ne, ValueType::F64) => Some(Intrinsic::NeFloat),
            (BinaryOp::Lt, ValueType::F64) => Some(Intrinsic::LtFloat),
            (BinaryOp::Le, ValueType::F64) => Some(Intrinsic::LeFloat),
            (BinaryOp::Gt, ValueType::F64) => Some(Intrinsic::GtFloat),
            (BinaryOp::Ge, ValueType::F64) => Some(Intrinsic::GeFloat),

            // Note: Complex operations use Pure Julia with method dispatch (base/complex.jl)
            // Division always uses F64 intrinsics for primitives
            (BinaryOp::Div, _) => Some(Intrinsic::DivFloat),
            // Power operator: use DynamicPow for all cases to preserve integer arithmetic
            (BinaryOp::Pow, _) => None, // Will use DynamicPow in special cases block
            (BinaryOp::Mod, _) => Some(Intrinsic::SremInt),
            // Integer division (÷) - use DynamicIntDiv for float types (Issue #1970)
            (BinaryOp::IntDiv, _) => None, // Will use DynamicIntDiv below

            // Logical AND: use mul_int (both must be non-zero)
            (BinaryOp::And, _) => Some(Intrinsic::MulInt),

            // Logical OR: special handling required
            (BinaryOp::Or, _) => None,

            _ => None,
        };

        match intrinsic_opt {
            Some(intrinsic) => {
                self.emit(Instr::CallIntrinsic(intrinsic));
            }
            None => {
                // Special cases that need custom handling
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
                    // a || b: if (a + b) != 0 then 1 else 0
                    return self.compile_or_expr(left, right);
                } else {
                    // If we reach here, it means dispatch failed - report error
                    return err(format!("Unsupported binary op: {:?}", op));
                }
            }
        }

        // Issue #1759: Convert F64 result back to F32 if needed for arithmetic operations.
        // Intrinsics only operate on F64, so when result should be F32 we need to convert.
        if result_ty == ValueType::F32 {
            self.emit(Instr::DynamicToF32);
        }
        // Issue #1975: Convert F64 result back to F16 if needed for arithmetic operations.
        if result_ty == ValueType::F16 {
            self.emit(Instr::DynamicToF16);
        }
        // Issue #2123: Convert I64 result back to Char for Char+Int/Int+Char/Char-Int arithmetic.
        if result_ty == ValueType::Char {
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::IntToChar, 1));
        }
        // Issue #2278: Convert I64 result back to small integer type
        if let Some(back_conv) = small_int_back_conversion(&result_ty) {
            self.emit(back_conv);
        }

        // Comparisons return Bool (Julia semantics)
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
