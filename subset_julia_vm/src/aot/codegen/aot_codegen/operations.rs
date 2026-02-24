use super::AotCodeGenerator;
use crate::aot::ir::AotBinOp;
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};

impl AotCodeGenerator {
    // ========== Arithmetic Operation Generation ==========

    /// Generate a binary operation with proper type handling
    pub(super) fn emit_binop(
        &self,
        op: AotBinOp,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> AotResult<String> {
        match op {
            // Power operation
            AotBinOp::Pow => self.emit_pow(left_str, right_str, left_ty, right_ty),

            // Division: Julia's / always returns Float64
            AotBinOp::Div => self.emit_div(left_str, right_str, left_ty, right_ty),

            // Integer division: Julia's ÷ returns integer
            AotBinOp::IntDiv => self.emit_intdiv(left_str, right_str, left_ty, right_ty),

            // Modulo operation
            AotBinOp::Mod => self.emit_mod(left_str, right_str, left_ty, right_ty),

            // Standard arithmetic with type promotion
            AotBinOp::Add | AotBinOp::Sub | AotBinOp::Mul => {
                self.emit_arithmetic(op, left_str, right_str, left_ty, right_ty, result_ty)
            }

            // Comparison operations
            AotBinOp::Lt
            | AotBinOp::Gt
            | AotBinOp::Le
            | AotBinOp::Ge
            | AotBinOp::Eq
            | AotBinOp::Ne => self.emit_comparison(op, left_str, right_str, left_ty, right_ty),

            // Identity comparisons (=== and !==)
            AotBinOp::Egal | AotBinOp::NotEgal => {
                self.emit_identity(op, left_str, right_str, left_ty, right_ty)
            }

            // Logical operations
            AotBinOp::And | AotBinOp::Or => self.emit_logical(op, left_str, right_str),

            // Bitwise operations
            AotBinOp::BitAnd
            | AotBinOp::BitOr
            | AotBinOp::BitXor
            | AotBinOp::Shl
            | AotBinOp::Shr => self.emit_bitwise(op, left_str, right_str),
        }
    }

    /// Generate power operation
    fn emit_pow(
        &self,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
    ) -> AotResult<String> {
        // Non-numeric power is lowered to function dispatch (e.g., Complex ^ Int64).
        if !left_ty.is_numeric() || !right_ty.is_numeric() {
            let resolved = self.resolve_dispatch("^", &[left_ty.clone(), right_ty.clone()]);
            return Ok(format!("{}({}, {})", resolved, left_str, right_str));
        }

        // Integer base with integer exponent
        if left_ty.is_integer() && right_ty.is_integer() {
            Ok(format!("{}.pow({} as u32)", left_str, right_str))
        }
        // Float base or mixed types
        else if left_ty.is_float() {
            Ok(format!("{}.powf({})", left_str, right_str))
        }
        // Integer base with float exponent -> convert to float
        else if left_ty.is_integer() && right_ty.is_float() {
            Ok(format!("({} as f64).powf({})", left_str, right_str))
        }
        // Default to float power
        else {
            Ok(format!("({} as f64).powf({} as f64)", left_str, right_str))
        }
    }

    /// Generate float division (Julia's /)
    fn emit_div(
        &self,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
    ) -> AotResult<String> {
        // If both are floats, simple division
        if left_ty.is_float() && right_ty.is_float() {
            Ok(format!("({} / {})", left_str, right_str))
        }
        // If left is float, cast right
        else if left_ty.is_float() {
            Ok(format!("({} / {} as f64)", left_str, right_str))
        }
        // If right is float, cast left
        else if right_ty.is_float() {
            Ok(format!("({} as f64 / {})", left_str, right_str))
        }
        // Both integers: cast both to f64 for Julia division semantics
        else {
            Ok(format!("({} as f64 / {} as f64)", left_str, right_str))
        }
    }

    /// Generate integer division (Julia's ÷)
    fn emit_intdiv(
        &self,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
    ) -> AotResult<String> {
        // If both are integers, simple division (Rust integer division truncates)
        if left_ty.is_integer() && right_ty.is_integer() {
            Ok(format!("({} / {})", left_str, right_str))
        }
        // If floats involved, convert to integer first, then divide
        else if left_ty.is_float() && right_ty.is_float() {
            Ok(format!("(({} as i64) / ({} as i64))", left_str, right_str))
        } else if left_ty.is_float() {
            Ok(format!("(({} as i64) / {})", left_str, right_str))
        } else if right_ty.is_float() {
            Ok(format!("({} / ({} as i64))", left_str, right_str))
        } else {
            Ok(format!("({} / {})", left_str, right_str))
        }
    }

    /// Generate modulo operation
    fn emit_mod(
        &self,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
    ) -> AotResult<String> {
        // Integer modulo
        if left_ty.is_integer() && right_ty.is_integer() {
            Ok(format!("({} % {})", left_str, right_str))
        }
        // Float modulo (uses rem_euclid for Julia-like behavior, but % works too)
        else if left_ty.is_float() && right_ty.is_float() {
            Ok(format!("({} % {})", left_str, right_str))
        }
        // Mixed: promote to float
        else if left_ty.is_float() || right_ty.is_float() {
            let left = if left_ty.is_integer() {
                format!("{} as f64", left_str)
            } else {
                left_str.to_string()
            };
            let right = if right_ty.is_integer() {
                format!("{} as f64", right_str)
            } else {
                right_str.to_string()
            };
            Ok(format!("({} % {})", left, right))
        } else {
            Ok(format!("({} % {})", left_str, right_str))
        }
    }

    /// Generate standard arithmetic with type promotion
    fn emit_arithmetic(
        &self,
        op: AotBinOp,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> AotResult<String> {
        let op_str = op.to_rust_op();

        // Same types: no casting needed
        if left_ty == right_ty {
            return Ok(format!("({} {} {})", left_str, op_str, right_str));
        }

        // Result is float but operands include integers: need casts
        if result_ty.is_float() {
            let left = if left_ty.is_integer() {
                format!("({} as f64)", left_str)
            } else {
                left_str.to_string()
            };
            let right = if right_ty.is_integer() {
                format!("({} as f64)", right_str)
            } else {
                right_str.to_string()
            };
            return Ok(format!("({} {} {})", left, op_str, right));
        }

        // Default: no casting
        Ok(format!("({} {} {})", left_str, op_str, right_str))
    }

    // ========== Comparison Operation Generation ==========

    /// Generate comparison operations with proper type handling
    fn emit_comparison(
        &self,
        op: AotBinOp,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
    ) -> AotResult<String> {
        let op_str = op.to_rust_op();

        // Same types: direct comparison
        if left_ty == right_ty {
            return Ok(format!("({} {} {})", left_str, op_str, right_str));
        }

        // Mixed numeric types: promote to float for comparison
        if (left_ty.is_integer() && right_ty.is_float())
            || (left_ty.is_float() && right_ty.is_integer())
        {
            let left = if left_ty.is_integer() {
                format!("({} as f64)", left_str)
            } else {
                left_str.to_string()
            };
            let right = if right_ty.is_integer() {
                format!("({} as f64)", right_str)
            } else {
                right_str.to_string()
            };
            return Ok(format!("({} {} {})", left, op_str, right));
        }

        // Default: direct comparison
        Ok(format!("({} {} {})", left_str, op_str, right_str))
    }

    /// Generate identity comparison (=== and !==)
    fn emit_identity(
        &self,
        op: AotBinOp,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
    ) -> AotResult<String> {
        // For primitive types, === is the same as ==
        if left_ty.is_primitive() && right_ty.is_primitive() {
            let rust_op = match op {
                AotBinOp::Egal => "==",
                AotBinOp::NotEgal => "!=",
                _ => return Err(AotError::InternalError(
                    format!("emit_identity: unexpected operator {:?} for primitive types", op),
                )),
            };
            return Ok(format!("({} {} {})", left_str, rust_op, right_str));
        }

        // For objects, use std::ptr::eq for identity comparison
        match op {
            AotBinOp::Egal => Ok(format!(
                "std::ptr::eq(&{} as *const _, &{} as *const _)",
                left_str, right_str
            )),
            AotBinOp::NotEgal => Ok(format!(
                "!std::ptr::eq(&{} as *const _, &{} as *const _)",
                left_str, right_str
            )),
            _ => Err(AotError::InternalError(
                format!("emit_identity: unexpected operator {:?} for object types", op),
            )),
        }
    }

    /// Generate logical operations
    fn emit_logical(&self, op: AotBinOp, left_str: &str, right_str: &str) -> AotResult<String> {
        let op_str = op.to_rust_op();
        Ok(format!("({} {} {})", left_str, op_str, right_str))
    }

    /// Generate bitwise operations
    fn emit_bitwise(&self, op: AotBinOp, left_str: &str, right_str: &str) -> AotResult<String> {
        let op_str = op.to_rust_op();
        Ok(format!("({} {} {})", left_str, op_str, right_str))
    }
}
