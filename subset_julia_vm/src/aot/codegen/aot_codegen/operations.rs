use super::AotCodeGenerator;
use crate::aot::ir::AotBinOp;
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};

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
            // Julia subtype (`<:`) compares type relationships, not values.
            // Statically resolvable relations are const-folded in the IR
            // converter (Issue #7037); reaching codegen means runtime type
            // values, which stay gated until AoT carries a type-object
            // representation through codegen.
            AotBinOp::Subtype => Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(
                    "AoT codegen does not support subtype operator (<:) on runtime type values; only statically known type names are folded (Issue #7037)",
                )
                .with_workaround(
                    "use statically known type names so the relation can be const-folded, or run this check on the VM",
                ),
            )),

            // Power operation
            AotBinOp::Pow => self.emit_pow(left_str, right_str, left_ty, right_ty, result_ty),

            // Division: Julia preserves concrete float width when a float operand is present.
            AotBinOp::Div => self.emit_div(left_str, right_str, left_ty, right_ty),

            // Integer division: Julia's ÷ returns integer
            AotBinOp::IntDiv => self.emit_intdiv(left_str, right_str, left_ty, right_ty, result_ty),

            // Modulo operation
            AotBinOp::Mod => self.emit_mod(left_str, right_str, left_ty, right_ty, result_ty),

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
        result_ty: &StaticType,
    ) -> AotResult<String> {
        if matches!((left_ty, right_ty), (StaticType::Bool, StaticType::Bool)) {
            return Ok(format!("(!{} || {})", right_str, left_str));
        }

        if matches!(left_ty, StaticType::Bool) && right_ty.is_signed() {
            return Ok(format!(
                "{{ let _sjulia_pow_base = {}; let _sjulia_pow_exp = {}; if _sjulia_pow_exp < {} {{ if !_sjulia_pow_base {{ throw(RuntimeError::custom(format!(\"DomainError with {{}}:\\nCannot raise an integer x to a negative power {{}}.\\nMake x or {{}} a float by adding a zero decimal (e.g., 2.0^-1 or 2^-1.0 instead of 2^-1) or write 1/x^1, float(x)^-1, x^float(-1) or (x//1)^-1.\", _sjulia_pow_exp, _sjulia_pow_exp, _sjulia_pow_exp))) }} else {{ true }} }} else {{ _sjulia_pow_exp == {} || _sjulia_pow_base }} }}",
                left_str,
                right_str,
                Self::zero_literal(right_ty).unwrap_or("0"),
                Self::zero_literal(right_ty).unwrap_or("0")
            ));
        }

        if matches!(left_ty, StaticType::Bool) && right_ty.is_integer() {
            return Ok(format!(
                "{{ let _sjulia_pow_base = {}; let _sjulia_pow_exp = {}; _sjulia_pow_exp == {} || _sjulia_pow_base }}",
                left_str,
                right_str,
                Self::zero_literal(right_ty).unwrap_or("0")
            ));
        }

        if matches!(left_ty, StaticType::Bool) && right_ty.is_float() {
            let target_float = Self::rust_float_type(result_ty).unwrap_or("f64");
            let left = Self::cast_numeric_to_float(left_str, left_ty, target_float);
            let right = Self::cast_numeric_to_float(right_str, right_ty, target_float);
            // `cast_numeric_to_float` already parenthesizes the Bool cast
            // (`(x as u8 as f64)`), so the receiver here is cast-safe as-is.
            return Ok(format!("{}.powf({})", left, right));
        }

        if matches!(right_ty, StaticType::Bool) && left_ty.is_numeric() {
            let Some(one) = Self::one_literal(result_ty) else {
                return Err(AotError::CodegenError(format!(
                    "AoT Bool exponent codegen cannot materialize one for result type {:?}",
                    result_ty
                )));
            };
            return Ok(format!(
                "{{ let _sjulia_pow_base = {}; if {} {{ _sjulia_pow_base }} else {{ {} }} }}",
                left_str, right_str, one
            ));
        }

        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            return Err(AotError::CodegenError(
                "AoT Bool power codegen does not support this operand combination".to_string(),
            ));
        }

        // Non-numeric power is lowered to function dispatch (e.g., Complex ^ Int64).
        if !left_ty.is_numeric() || !right_ty.is_numeric() {
            let resolved = self.resolve_dispatch("^", &[left_ty.clone(), right_ty.clone()])?;
            return Ok(format!("{}({}, {})", resolved, left_str, right_str));
        }

        // Integer base with integer exponent. Parenthesize the receiver so a
        // cast-terminated base (e.g. `length(s)` -> `... .len() as i64`) is not
        // mis-parsed by Rust as `... as (i64.wrapping_pow(..))` — the same
        // "cast cannot be followed by a method call" defect the wrapping_sub
        // receiver had (Issue #8146).
        if left_ty.is_integer() && right_ty.is_integer() {
            Ok(format!("({}).wrapping_pow({} as u32)", left_str, right_str))
        }
        // Float base or mixed types
        else if left_ty.is_float() {
            Ok(format!("({}).powf({})", left_str, right_str))
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
        let target_float = Self::promoted_float_rust_type(left_ty, right_ty);
        let left = Self::cast_numeric_to_float(left_str, left_ty, target_float);
        let right = Self::cast_numeric_to_float(right_str, right_ty, target_float);
        Ok(format!("({} / {})", left, right))
    }

    /// Generate integer division (Julia's ÷)
    fn emit_intdiv(
        &self,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> AotResult<String> {
        // If both are integers, simple division (Rust integer division truncates)
        if left_ty.is_integer() && right_ty.is_integer() {
            Ok(Self::emit_checked_truncating_int_div(
                left_str, right_str, left_ty, right_ty, result_ty,
            ))
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
    // The integer and float branches currently emit the same `%` form, but are kept
    // separate because float modulo is documented to diverge to rem_euclid later.
    #[allow(clippy::if_same_then_else)]
    fn emit_mod(
        &self,
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> AotResult<String> {
        // Integer modulo
        if left_ty.is_integer() && right_ty.is_integer() {
            Ok(Self::emit_checked_int_rem(
                left_str, right_str, left_ty, right_ty, result_ty,
            ))
        }
        // Float or mixed numeric modulo. Full Julia modulo edge semantics are
        // tracked separately; this keeps the emitted operand widths coherent.
        else if left_ty.is_float() || right_ty.is_float() {
            let target_float = Self::promoted_float_rust_type(left_ty, right_ty);
            let left = Self::cast_numeric_to_float(left_str, left_ty, target_float);
            let right = Self::cast_numeric_to_float(right_str, right_ty, target_float);
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

        if left_ty == right_ty
            && result_ty == left_ty
            && Self::uses_wrapping_integer_arithmetic(result_ty)
        {
            if let Some(method) = Self::wrapping_integer_method(op) {
                // Parenthesize the receiver so a left operand that itself ends in a
                // cast (e.g. `time_ns()`'s `... as i64`, or `length(s)`'s `.len() as
                // i64`) is not mis-parsed by Rust as `... as (i64.wrapping_sub(..))`.
                // Rust gives `as` lower precedence than a method call, so the cast
                // must be wrapped: `(<expr> as i64).wrapping_sub(rhs)` (Issue #8146).
                return Ok(format!("({}).{}({})", left_str, method, right_str));
            }
        }

        if op == AotBinOp::Mul
            && matches!(result_ty, StaticType::Str)
            && matches!(left_ty, StaticType::Str | StaticType::Char)
            && matches!(right_ty, StaticType::Str | StaticType::Char)
        {
            return Ok(format!(
                "format!(\"{{}}{{}}\", {}, {})",
                left_str, right_str
            ));
        }

        // Same types: no casting needed
        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            if matches!(result_ty, StaticType::Bool)
                && op == AotBinOp::Mul
                && matches!(left_ty, StaticType::Bool)
                && matches!(right_ty, StaticType::Bool)
            {
                return Ok(format!("({} && {})", left_str, right_str));
            }
            if matches!(result_ty, StaticType::Bool) {
                return Err(AotError::CodegenError(format!(
                    "AoT Bool arithmetic codegen for `{}` with result Bool needs a \
                     Julia-specific lowering rule",
                    op
                )));
            }
            if result_ty.is_numeric() && !matches!(result_ty, StaticType::Bool) {
                let left = Self::cast_bool_for_numeric_result(left_str, left_ty, result_ty);
                let right = Self::cast_bool_for_numeric_result(right_str, right_ty, result_ty);
                return Ok(format!("({} {} {})", left, op_str, right));
            }
        }

        // Same types: no casting needed
        if left_ty == right_ty {
            return Ok(format!("({} {} {})", left_str, op_str, right_str));
        }

        // Result is float: cast operands to the inferred Rust float width.
        if result_ty.is_float() {
            let target_float = Self::rust_float_type(result_ty).unwrap_or("f64");
            let left = Self::cast_numeric_to_float(left_str, left_ty, target_float);
            let right = Self::cast_numeric_to_float(right_str, right_ty, target_float);
            return Ok(format!("({} {} {})", left, op_str, right));
        }

        // Default: no casting
        Ok(format!("({} {} {})", left_str, op_str, right_str))
    }

    pub(super) fn uses_wrapping_integer_arithmetic(ty: &StaticType) -> bool {
        matches!(
            ty,
            StaticType::I64
                | StaticType::I128
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U128
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
        )
    }

    pub(super) fn wrapping_integer_method(op: AotBinOp) -> Option<&'static str> {
        match op {
            AotBinOp::Add => Some("wrapping_add"),
            AotBinOp::Sub => Some("wrapping_sub"),
            AotBinOp::Mul => Some("wrapping_mul"),
            _ => None,
        }
    }

    fn rust_float_type(ty: &StaticType) -> Option<&'static str> {
        match ty {
            StaticType::F64 => Some("f64"),
            StaticType::F32 | StaticType::F16 => Some("f32"),
            _ => None,
        }
    }

    pub(super) fn emit_checked_truncating_int_div(
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> String {
        if matches!(
            (left_ty, right_ty, result_ty),
            (StaticType::Bool, StaticType::Bool, StaticType::Bool)
        ) {
            return format!(
                "{{ if !{} {{ throw(RuntimeError::DivisionByZero) }} else {{ {} }} }}",
                right_str, left_str
            );
        }
        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            let target_integer = Self::rust_integer_type(result_ty).unwrap_or("i64");
            let left = Self::cast_bool_to_integer(left_str, left_ty, target_integer);
            let right = Self::cast_bool_to_integer(right_str, right_ty, target_integer);
            let right_zero = Self::is_zero_expr(right_str, right_ty);
            return format!(
                "{{ if {} {{ throw(RuntimeError::DivisionByZero) }} else {{ {} / {} }} }}",
                right_zero, left, right
            );
        }

        let zero = Self::zero_literal(right_ty).unwrap_or("0");
        if let (Some(min), Some(neg_one)) = (
            Self::signed_min_literal(left_ty),
            Self::minus_one_literal(right_ty),
        ) {
            format!(
                "{{ let _sjulia_div_l = {}; let _sjulia_div_r = {}; if _sjulia_div_r == {} || (_sjulia_div_l == {} && _sjulia_div_r == {}) {{ throw(RuntimeError::DivisionByZero) }} else {{ _sjulia_div_l / _sjulia_div_r }} }}",
                left_str, right_str, zero, min, neg_one
            )
        } else {
            format!(
                "{{ let _sjulia_div_l = {}; let _sjulia_div_r = {}; if _sjulia_div_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ _sjulia_div_l / _sjulia_div_r }} }}",
                left_str, right_str, zero
            )
        }
    }

    pub(super) fn emit_checked_int_rem(
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> String {
        if matches!(
            (left_ty, right_ty, result_ty),
            (StaticType::Bool, StaticType::Bool, StaticType::Bool)
        ) {
            return format!(
                "{{ if !{} {{ throw(RuntimeError::DivisionByZero) }} else {{ false }} }}",
                right_str
            );
        }
        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            let target_integer = Self::rust_integer_type(result_ty).unwrap_or("i64");
            let left = Self::cast_bool_to_integer(left_str, left_ty, target_integer);
            let right = Self::cast_bool_to_integer(right_str, right_ty, target_integer);
            let right_zero = Self::is_zero_expr(right_str, right_ty);
            return format!(
                "{{ if {} {{ throw(RuntimeError::DivisionByZero) }} else {{ {} % {} }} }}",
                right_zero, left, right
            );
        }

        let zero = Self::zero_literal(right_ty).unwrap_or("0");
        let result_zero = Self::zero_literal(result_ty).unwrap_or("0");
        if let (Some(min), Some(neg_one)) = (
            Self::signed_min_literal(left_ty),
            Self::minus_one_literal(right_ty),
        ) {
            format!(
                "{{ let _sjulia_rem_l = {}; let _sjulia_rem_r = {}; if _sjulia_rem_r == {} {{ throw(RuntimeError::DivisionByZero) }} else if _sjulia_rem_l == {} && _sjulia_rem_r == {} {{ {} }} else {{ _sjulia_rem_l % _sjulia_rem_r }} }}",
                left_str, right_str, zero, min, neg_one, result_zero
            )
        } else {
            format!(
                "{{ let _sjulia_rem_l = {}; let _sjulia_rem_r = {}; if _sjulia_rem_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ _sjulia_rem_l % _sjulia_rem_r }} }}",
                left_str, right_str, zero
            )
        }
    }

    pub(super) fn emit_checked_int_mod(
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> String {
        if matches!(
            (left_ty, right_ty, result_ty),
            (StaticType::Bool, StaticType::Bool, StaticType::Bool)
        ) {
            return format!(
                "{{ if !{} {{ throw(RuntimeError::DivisionByZero) }} else {{ false }} }}",
                right_str
            );
        }
        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            let target_integer = Self::rust_integer_type(result_ty).unwrap_or("i64");
            let left = Self::cast_bool_to_integer_for_let(left_str, left_ty, target_integer);
            let right = Self::cast_bool_to_integer_for_let(right_str, right_ty, target_integer);
            let zero = Self::zero_literal(result_ty).unwrap_or("0i64");
            return format!(
                "{{ let _sjulia_mod_l = {}; let _sjulia_mod_r = {}; if _sjulia_mod_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_mod_rem = _sjulia_mod_l % _sjulia_mod_r; if _sjulia_mod_rem != {} && ((_sjulia_mod_rem > {}) != (_sjulia_mod_r > {})) {{ _sjulia_mod_rem + _sjulia_mod_r }} else {{ _sjulia_mod_rem }} }} }}",
                left, right, zero, zero, zero, zero
            );
        }

        let zero = Self::zero_literal(right_ty).unwrap_or("0");
        let compare_zero = Self::zero_literal(result_ty).unwrap_or("0");
        let result_zero = Self::zero_literal(result_ty).unwrap_or("0");
        if let (Some(min), Some(neg_one)) = (
            Self::signed_min_literal(left_ty),
            Self::minus_one_literal(right_ty),
        ) {
            format!(
                "{{ let _sjulia_mod_l = {}; let _sjulia_mod_r = {}; if _sjulia_mod_r == {} {{ throw(RuntimeError::DivisionByZero) }} else if _sjulia_mod_l == {} && _sjulia_mod_r == {} {{ {} }} else {{ let _sjulia_mod_rem = _sjulia_mod_l % _sjulia_mod_r; if _sjulia_mod_rem != {} && ((_sjulia_mod_rem > {}) != (_sjulia_mod_r > {})) {{ _sjulia_mod_rem + _sjulia_mod_r }} else {{ _sjulia_mod_rem }} }} }}",
                left_str, right_str, zero, min, neg_one, result_zero, compare_zero, compare_zero, compare_zero
            )
        } else {
            format!(
                "{{ let _sjulia_mod_l = {}; let _sjulia_mod_r = {}; if _sjulia_mod_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_mod_rem = _sjulia_mod_l % _sjulia_mod_r; if _sjulia_mod_rem != {} && ((_sjulia_mod_rem > {}) != (_sjulia_mod_r > {})) {{ _sjulia_mod_rem + _sjulia_mod_r }} else {{ _sjulia_mod_rem }} }} }}",
                left_str, right_str, zero, compare_zero, compare_zero, compare_zero
            )
        }
    }

    pub(super) fn emit_checked_int_fld(
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> String {
        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            let target_integer = Self::rust_integer_type(result_ty).unwrap_or("i64");
            let left = Self::cast_bool_to_integer_for_let(left_str, left_ty, target_integer);
            let right = Self::cast_bool_to_integer_for_let(right_str, right_ty, target_integer);
            let zero = Self::zero_literal(result_ty).unwrap_or("0i64");
            return format!(
                "{{ let _sjulia_fld_l = {}; let _sjulia_fld_r = {}; if _sjulia_fld_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_fld_q = _sjulia_fld_l / _sjulia_fld_r; let _sjulia_fld_rem = _sjulia_fld_l % _sjulia_fld_r; if _sjulia_fld_rem != {} && ((_sjulia_fld_rem > {}) != (_sjulia_fld_r > {})) {{ _sjulia_fld_q - 1 }} else {{ _sjulia_fld_q }} }} }}",
                left, right, zero, zero, zero, zero
            );
        }

        let zero = Self::zero_literal(right_ty).unwrap_or("0");
        let compare_zero = Self::zero_literal(result_ty).unwrap_or("0");
        if let (Some(min), Some(neg_one)) = (
            Self::signed_min_literal(left_ty),
            Self::minus_one_literal(right_ty),
        ) {
            format!(
                "{{ let _sjulia_fld_l = {}; let _sjulia_fld_r = {}; if _sjulia_fld_r == {} || (_sjulia_fld_l == {} && _sjulia_fld_r == {}) {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_fld_q = _sjulia_fld_l / _sjulia_fld_r; let _sjulia_fld_rem = _sjulia_fld_l % _sjulia_fld_r; if _sjulia_fld_rem != {} && ((_sjulia_fld_rem > {}) != (_sjulia_fld_r > {})) {{ _sjulia_fld_q - 1 }} else {{ _sjulia_fld_q }} }} }}",
                left_str, right_str, zero, min, neg_one, compare_zero, compare_zero, compare_zero
            )
        } else {
            format!(
                "{{ let _sjulia_fld_l = {}; let _sjulia_fld_r = {}; if _sjulia_fld_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_fld_q = _sjulia_fld_l / _sjulia_fld_r; let _sjulia_fld_rem = _sjulia_fld_l % _sjulia_fld_r; if _sjulia_fld_rem != {} && ((_sjulia_fld_rem > {}) != (_sjulia_fld_r > {})) {{ _sjulia_fld_q - 1 }} else {{ _sjulia_fld_q }} }} }}",
                left_str, right_str, zero, compare_zero, compare_zero, compare_zero
            )
        }
    }

    pub(super) fn emit_checked_int_cld(
        left_str: &str,
        right_str: &str,
        left_ty: &StaticType,
        right_ty: &StaticType,
        result_ty: &StaticType,
    ) -> String {
        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            let target_integer = Self::rust_integer_type(result_ty).unwrap_or("i64");
            let left = Self::cast_bool_to_integer_for_let(left_str, left_ty, target_integer);
            let right = Self::cast_bool_to_integer_for_let(right_str, right_ty, target_integer);
            let zero = Self::zero_literal(result_ty).unwrap_or("0i64");
            return format!(
                "{{ let _sjulia_cld_l = {}; let _sjulia_cld_r = {}; if _sjulia_cld_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_cld_q = _sjulia_cld_l / _sjulia_cld_r; let _sjulia_cld_rem = _sjulia_cld_l % _sjulia_cld_r; if _sjulia_cld_rem != {} && ((_sjulia_cld_rem > {}) == (_sjulia_cld_r > {})) {{ _sjulia_cld_q + 1 }} else {{ _sjulia_cld_q }} }} }}",
                left, right, zero, zero, zero, zero
            );
        }

        let zero = Self::zero_literal(right_ty).unwrap_or("0");
        let compare_zero = Self::zero_literal(result_ty).unwrap_or("0");
        if let (Some(min), Some(neg_one)) = (
            Self::signed_min_literal(left_ty),
            Self::minus_one_literal(right_ty),
        ) {
            format!(
                "{{ let _sjulia_cld_l = {}; let _sjulia_cld_r = {}; if _sjulia_cld_r == {} || (_sjulia_cld_l == {} && _sjulia_cld_r == {}) {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_cld_q = _sjulia_cld_l / _sjulia_cld_r; let _sjulia_cld_rem = _sjulia_cld_l % _sjulia_cld_r; if _sjulia_cld_rem != {} && ((_sjulia_cld_rem > {}) == (_sjulia_cld_r > {})) {{ _sjulia_cld_q + 1 }} else {{ _sjulia_cld_q }} }} }}",
                left_str, right_str, zero, min, neg_one, compare_zero, compare_zero, compare_zero
            )
        } else {
            format!(
                "{{ let _sjulia_cld_l = {}; let _sjulia_cld_r = {}; if _sjulia_cld_r == {} {{ throw(RuntimeError::DivisionByZero) }} else {{ let _sjulia_cld_q = _sjulia_cld_l / _sjulia_cld_r; let _sjulia_cld_rem = _sjulia_cld_l % _sjulia_cld_r; if _sjulia_cld_rem != {} && ((_sjulia_cld_rem > {}) == (_sjulia_cld_r > {})) {{ _sjulia_cld_q + 1 }} else {{ _sjulia_cld_q }} }} }}",
                left_str, right_str, zero, compare_zero, compare_zero, compare_zero
            )
        }
    }

    fn rust_integer_type(ty: &StaticType) -> Option<&'static str> {
        match ty {
            StaticType::I64 => Some("i64"),
            StaticType::I128 => Some("i128"),
            StaticType::I32 => Some("i32"),
            StaticType::I16 => Some("i16"),
            StaticType::I8 => Some("i8"),
            StaticType::U64 => Some("u64"),
            StaticType::U128 => Some("u128"),
            StaticType::U32 => Some("u32"),
            StaticType::U16 => Some("u16"),
            StaticType::U8 => Some("u8"),
            _ => None,
        }
    }

    fn signed_min_literal(ty: &StaticType) -> Option<&'static str> {
        match ty {
            StaticType::I64 => Some("i64::MIN"),
            StaticType::I128 => Some("i128::MIN"),
            StaticType::I32 => Some("i32::MIN"),
            StaticType::I16 => Some("i16::MIN"),
            StaticType::I8 => Some("i8::MIN"),
            _ => None,
        }
    }

    fn minus_one_literal(ty: &StaticType) -> Option<&'static str> {
        match ty {
            StaticType::I64 => Some("-1i64"),
            StaticType::I128 => Some("-1i128"),
            StaticType::I32 => Some("-1i32"),
            StaticType::I16 => Some("-1i16"),
            StaticType::I8 => Some("-1i8"),
            _ => None,
        }
    }

    fn zero_literal(ty: &StaticType) -> Option<&'static str> {
        match ty {
            StaticType::Bool => Some("false"),
            StaticType::I64 => Some("0i64"),
            StaticType::I128 => Some("0i128"),
            StaticType::I32 => Some("0i32"),
            StaticType::I16 => Some("0i16"),
            StaticType::I8 => Some("0i8"),
            StaticType::U64 => Some("0u64"),
            StaticType::U128 => Some("0u128"),
            StaticType::U32 => Some("0u32"),
            StaticType::U16 => Some("0u16"),
            StaticType::U8 => Some("0u8"),
            StaticType::F64 => Some("0.0_f64"),
            StaticType::F32 | StaticType::F16 => Some("0.0_f32"),
            _ => None,
        }
    }

    fn one_literal(ty: &StaticType) -> Option<&'static str> {
        match ty {
            StaticType::Bool => Some("true"),
            StaticType::I64 => Some("1i64"),
            StaticType::I128 => Some("1i128"),
            StaticType::I32 => Some("1i32"),
            StaticType::I16 => Some("1i16"),
            StaticType::I8 => Some("1i8"),
            StaticType::U64 => Some("1u64"),
            StaticType::U128 => Some("1u128"),
            StaticType::U32 => Some("1u32"),
            StaticType::U16 => Some("1u16"),
            StaticType::U8 => Some("1u8"),
            StaticType::F64 => Some("1.0_f64"),
            StaticType::F32 | StaticType::F16 => Some("1.0_f32"),
            _ => None,
        }
    }

    fn is_zero_expr(expr: &str, ty: &StaticType) -> String {
        if matches!(ty, StaticType::Bool) {
            format!("!{}", expr)
        } else if let Some(zero) = Self::zero_literal(ty) {
            format!("{} == {}", expr, zero)
        } else {
            "false".to_string()
        }
    }

    fn promoted_float_rust_type(left_ty: &StaticType, right_ty: &StaticType) -> &'static str {
        if matches!(left_ty, StaticType::F64) || matches!(right_ty, StaticType::F64) {
            "f64"
        } else if left_ty.is_float() || right_ty.is_float() {
            "f32"
        } else {
            "f64"
        }
    }

    fn cast_numeric_to_float(expr: &str, ty: &StaticType, target_float: &str) -> String {
        if Self::rust_float_type(ty) == Some(target_float) {
            expr.to_string()
        } else if matches!(ty, StaticType::Bool) {
            format!("({} as u8 as {})", expr, target_float)
        } else if ty.is_numeric() {
            format!("({} as {})", expr, target_float)
        } else {
            expr.to_string()
        }
    }

    fn cast_bool_for_numeric_result(expr: &str, ty: &StaticType, result_ty: &StaticType) -> String {
        if !matches!(ty, StaticType::Bool) {
            return expr.to_string();
        }
        if let Some(target_float) = Self::rust_float_type(result_ty) {
            format!("({} as u8 as {})", expr, target_float)
        } else if let Some(target_integer) = Self::rust_integer_type(result_ty) {
            Self::cast_bool_to_integer(expr, ty, target_integer)
        } else {
            expr.to_string()
        }
    }

    fn cast_bool_to_integer(expr: &str, ty: &StaticType, target_integer: &str) -> String {
        if matches!(ty, StaticType::Bool) {
            format!("({} as u8 as {})", expr, target_integer)
        } else {
            expr.to_string()
        }
    }

    fn cast_bool_to_integer_for_let(expr: &str, ty: &StaticType, target_integer: &str) -> String {
        if matches!(ty, StaticType::Bool) {
            format!("{} as u8 as {}", expr, target_integer)
        } else {
            expr.to_string()
        }
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

        if matches!(left_ty, StaticType::Bool) || matches!(right_ty, StaticType::Bool) {
            if left_ty.is_float() || right_ty.is_float() {
                let target_float = Self::promoted_float_rust_type(left_ty, right_ty);
                let left = Self::cast_numeric_to_float(left_str, left_ty, target_float);
                let right = Self::cast_numeric_to_float(right_str, right_ty, target_float);
                return Ok(format!("({} {} {})", left, op_str, right));
            }
            if left_ty.is_integer() && right_ty.is_integer() {
                let target_integer = match (left_ty, right_ty) {
                    (StaticType::Bool, StaticType::Bool) => "i64",
                    (StaticType::Bool, other) => Self::rust_integer_type(other).unwrap_or("i64"),
                    (other, StaticType::Bool) => Self::rust_integer_type(other).unwrap_or("i64"),
                    _ => "i64",
                };
                let left = Self::cast_bool_to_integer(left_str, left_ty, target_integer);
                let right = Self::cast_bool_to_integer(right_str, right_ty, target_integer);
                return Ok(format!("({} {} {})", left, op_str, right));
            }
        }

        // Mixed numeric types: promote to the widest float present for comparison.
        if left_ty.is_numeric()
            && right_ty.is_numeric()
            && (left_ty.is_float() || right_ty.is_float())
        {
            let target_float = Self::promoted_float_rust_type(left_ty, right_ty);
            let left = Self::cast_numeric_to_float(left_str, left_ty, target_float);
            let right = Self::cast_numeric_to_float(right_str, right_ty, target_float);
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
                _ => {
                    return Err(AotError::InternalError(format!(
                        "emit_identity: unexpected operator {:?} for primitive types",
                        op
                    )))
                }
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
            _ => Err(AotError::InternalError(format!(
                "emit_identity: unexpected operator {:?} for object types",
                op
            ))),
        }
    }

    /// Generate logical operations
    fn emit_logical(&self, op: AotBinOp, left_str: &str, right_str: &str) -> AotResult<String> {
        let op_str = op.to_rust_op();
        Ok(format!("({} {} {})", left_str, op_str, right_str))
    }

    /// Generate bitwise operations
    fn emit_bitwise(&self, op: AotBinOp, left_str: &str, right_str: &str) -> AotResult<String> {
        // Shifts route through Julia-faithful helpers (over-shift → 0 / sign
        // fill, negative amount shifts the other direction) instead of Rust's
        // panicking/masking native `<<`/`>>` (Issue #7057). The shift amount is
        // normalized to `i64`, matching Julia's `Integer`-amount shift methods.
        match op {
            AotBinOp::Shl => Ok(format!("op_lshift({}, ({}) as i64)", left_str, right_str)),
            AotBinOp::Shr => Ok(format!("op_rshift({}, ({}) as i64)", left_str, right_str)),
            // `&` / `|` / `xor` have no over-shift hazard; keep native operators.
            _ => {
                let op_str = op.to_rust_op();
                Ok(format!("({} {} {})", left_str, op_str, right_str))
            }
        }
    }
}
