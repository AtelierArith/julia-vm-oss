//! Expression specialization.

use crate::builtins::BuiltinId;
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::vm::{ArrayElementType, Instr, ValueType};

use super::helpers::expr_variant_name;
use super::{FunctionSpecializer, SpecializationError};

impl FunctionSpecializer {
    pub(super) fn compile_expr(&mut self, expr: &Expr) -> Result<ValueType, SpecializationError> {
        match expr {
            Expr::Literal(lit, _) => self.compile_literal(lit),
            Expr::Var(name, _) => self.compile_var(name),
            Expr::BinaryOp {
                op, left, right, ..
            } => self.compile_binary_op(*op, left, right),
            Expr::UnaryOp { op, operand, .. } => self.compile_unary_op(*op, operand),
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => self.compile_call(function, args, kwargs),
            Expr::Builtin { name, args, .. } => self.compile_builtin(*name, args),
            Expr::ArrayLiteral {
                elements, shape, ..
            } => self.compile_array_literal(elements, shape),
            Expr::Index { array, indices, .. } => self.compile_index(array, indices),
            Expr::TupleLiteral { elements, .. } => self.compile_tuple(elements),
            Expr::Range {
                start, step, stop, ..
            } => self.compile_range(start, step.as_deref(), stop),
            _ => Err(SpecializationError::Unsupported(format!(
                "Expression type not yet supported for specialization: {}",
                expr_variant_name(expr)
            ))),
        }
    }

    pub(super) fn compile_literal(&mut self, lit: &Literal) -> Result<ValueType, SpecializationError> {
        match lit {
            Literal::Int(n) => {
                self.emit(Instr::PushI64(*n));
                Ok(ValueType::I64)
            }
            Literal::Float(f) => {
                self.emit(Instr::PushF64(*f));
                Ok(ValueType::F64)
            }
            Literal::Float32(f) => {
                self.emit(Instr::PushF32(*f));
                Ok(ValueType::F32)
            }
            Literal::Float16(f) => {
                self.emit(Instr::PushF16(*f));
                Ok(ValueType::F16)
            }
            Literal::Bool(b) => {
                self.emit(Instr::PushBool(*b));
                Ok(ValueType::Bool)
            }
            Literal::Str(s) => {
                self.emit(Instr::PushStr(s.clone()));
                Ok(ValueType::Str)
            }
            Literal::Nothing => {
                self.emit(Instr::PushNothing);
                Ok(ValueType::Nothing)
            }
            Literal::Missing => {
                self.emit(Instr::PushMissing);
                Ok(ValueType::Missing)
            }
            _ => Err(SpecializationError::Unsupported(
                "Literal type not yet supported".to_string(),
            )),
        }
    }

    pub(super) fn compile_var(&mut self, name: &str) -> Result<ValueType, SpecializationError> {
        // Check for math constants first (before checking locals)
        if !self.locals.contains_key(name) {
            // pi/π constant
            if name == "pi" || name == "\u{03C0}" {
                self.emit(Instr::PushF64(std::f64::consts::PI));
                return Ok(ValueType::F64);
            }
            // ℯ (Euler's number) constant - U+212F SCRIPT SMALL E
            if name == "ℯ" {
                self.emit(Instr::PushF64(std::f64::consts::E));
                return Ok(ValueType::F64);
            }
            // NaN constant
            if name == "NaN" {
                self.emit(Instr::PushF64(f64::NAN));
                return Ok(ValueType::F64);
            }
            // Inf constant
            if name == "Inf" {
                self.emit(Instr::PushF64(f64::INFINITY));
                return Ok(ValueType::F64);
            }
        }

        let ty = self.locals.get(name).cloned().unwrap_or(ValueType::Any);
        match ty {
            ValueType::I64 => self.emit(Instr::LoadI64(name.to_string())),
            ValueType::F64 => self.emit(Instr::LoadF64(name.to_string())),
            ValueType::F32 => self.emit(Instr::LoadF32(name.to_string())),
            ValueType::F16 => self.emit(Instr::LoadF16(name.to_string())),
            ValueType::Str => self.emit(Instr::LoadStr(name.to_string())),
            _ => self.emit(Instr::LoadAny(name.to_string())),
        }
        Ok(ty)
    }

    pub(super) fn compile_binary_op(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        // Handle short-circuit operators specially
        if matches!(op, BinaryOp::And) {
            return self.compile_and_expr(left, right);
        }
        if matches!(op, BinaryOp::Or) {
            return self.compile_or_expr(left, right);
        }

        let lt = self.compile_expr(left)?;
        let rt = self.compile_expr(right)?;

        // Emit typed instruction based on inferred types
        let result_type = self.emit_binary_op(op, lt, rt)?;
        Ok(result_type)
    }

    /// Compile short-circuit AND: left && right
    pub(super) fn compile_and_expr(
        &mut self,
        left: &Expr,
        right: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        // left && right = left ? right : false
        self.compile_expr(left)?;
        let jump_false = self.code.len();
        self.emit(Instr::JumpIfZero(0)); // If left is false, jump to push false

        // Left is true, evaluate right
        self.compile_expr(right)?;
        let jump_end = self.code.len();
        self.emit(Instr::Jump(0)); // Jump to end

        // Left is false, push false
        let false_pos = self.code.len();
        self.code[jump_false] = Instr::JumpIfZero(false_pos);
        self.emit(Instr::PushBool(false));

        let end_pos = self.code.len();
        self.code[jump_end] = Instr::Jump(end_pos);

        Ok(ValueType::Bool)
    }

    /// Compile short-circuit OR: left || right
    pub(super) fn compile_or_expr(
        &mut self,
        left: &Expr,
        right: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        // left || right = left ? true : right
        self.compile_expr(left)?;
        let jump_true = self.code.len();
        self.emit(Instr::JumpIfZero(0)); // If left is false, jump to evaluate right

        // Left is true, push true
        self.emit(Instr::PushBool(true));
        let jump_end = self.code.len();
        self.emit(Instr::Jump(0)); // Jump to end

        // Left is false, evaluate right
        let right_pos = self.code.len();
        self.code[jump_true] = Instr::JumpIfZero(right_pos);
        self.compile_expr(right)?;

        let end_pos = self.code.len();
        self.code[jump_end] = Instr::Jump(end_pos);

        Ok(ValueType::Bool)
    }

    pub(super) fn emit_binary_op(
        &mut self,
        op: BinaryOp,
        lt: ValueType,
        rt: ValueType,
    ) -> Result<ValueType, SpecializationError> {
        // Handle type promotion
        let (promoted_lt, promoted_rt, result_ty) = match (lt.clone(), rt.clone()) {
            (ValueType::I64, ValueType::I64) => (lt, rt, ValueType::I64),
            (ValueType::F64, ValueType::F64) => (lt, rt, ValueType::F64),
            (ValueType::I64, ValueType::F64) => {
                // Convert left I64 to F64
                // Stack: [left_i64, right_f64]
                // Need: [left_f64, right_f64]
                self.emit(Instr::Swap);
                self.emit(Instr::ToF64);
                self.emit(Instr::Swap);
                (ValueType::F64, ValueType::F64, ValueType::F64)
            }
            (ValueType::F64, ValueType::I64) => {
                // Convert right I64 to F64
                // Stack: [left_f64, right_i64]
                // Need: [left_f64, right_f64]
                self.emit(Instr::ToF64);
                (ValueType::F64, ValueType::F64, ValueType::F64)
            }
            _ => {
                // Fall back to dynamic ops for other types
                match op {
                    BinaryOp::Add => self.emit(Instr::DynamicAdd),
                    BinaryOp::Sub => self.emit(Instr::DynamicSub),
                    BinaryOp::Mul => self.emit(Instr::DynamicMul),
                    BinaryOp::Div => self.emit(Instr::DynamicDiv),
                    _ => {
                        return Err(SpecializationError::Unsupported(format!(
                            "Dynamic binary op not yet supported: {:?}",
                            op
                        )))
                    }
                }
                return Ok(ValueType::Any);
            }
        };

        // Emit typed instruction
        match op {
            BinaryOp::Add => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::AddI64);
                } else {
                    self.emit(Instr::AddF64);
                }
            }
            BinaryOp::Sub => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::SubI64);
                } else {
                    self.emit(Instr::SubF64);
                }
            }
            BinaryOp::Mul => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::MulI64);
                } else {
                    self.emit(Instr::MulF64);
                }
            }
            BinaryOp::Div => {
                // Division always produces F64
                if promoted_lt == ValueType::I64 {
                    self.emit(Instr::Swap);
                    self.emit(Instr::ToF64);
                    self.emit(Instr::Swap);
                }
                if promoted_rt == ValueType::I64 {
                    self.emit(Instr::ToF64);
                }
                self.emit(Instr::DivF64);
                return Ok(ValueType::F64);
            }
            BinaryOp::Mod => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::ModI64);
                } else {
                    self.emit(Instr::DynamicMod);
                    return Ok(ValueType::Any);
                }
            }
            BinaryOp::Lt => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::LtI64);
                } else {
                    self.emit(Instr::LtF64);
                }
                return Ok(ValueType::Bool);
            }
            BinaryOp::Gt => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::GtI64);
                } else {
                    self.emit(Instr::GtF64);
                }
                return Ok(ValueType::Bool);
            }
            BinaryOp::Le => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::LeI64);
                } else {
                    self.emit(Instr::LeF64);
                }
                return Ok(ValueType::Bool);
            }
            BinaryOp::Ge => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::GeI64);
                } else {
                    self.emit(Instr::GeF64);
                }
                return Ok(ValueType::Bool);
            }
            BinaryOp::Eq => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::EqI64);
                } else {
                    self.emit(Instr::EqF64);
                }
                return Ok(ValueType::Bool);
            }
            BinaryOp::Ne => {
                if result_ty == ValueType::I64 {
                    self.emit(Instr::NeI64);
                } else {
                    self.emit(Instr::NeF64);
                }
                return Ok(ValueType::Bool);
            }
            BinaryOp::IntDiv => {
                // Integer division with type preservation (Issue #1970)
                // Use DynamicIntDiv for all cases to preserve Float32/Float64 types
                self.emit(Instr::DynamicIntDiv);
                return Ok(ValueType::Any);
            }
            BinaryOp::Pow => {
                // Power operator - use DynamicPow for flexibility
                self.emit(Instr::DynamicPow);
                // Result type depends on inputs but usually F64
                return Ok(ValueType::F64);
            }
            // And/Or are handled by compile_and_expr/compile_or_expr with short-circuit evaluation
            BinaryOp::And | BinaryOp::Or => {
                return Err(SpecializationError::Unsupported(
                    "And/Or should be handled by compile_and_expr/compile_or_expr".to_string(),
                ));
            }
            _ => {
                return Err(SpecializationError::Unsupported(format!(
                    "Binary op not yet supported: {:?}",
                    op
                )));
            }
        }

        Ok(result_ty)
    }

    pub(super) fn compile_unary_op(
        &mut self,
        op: UnaryOp,
        operand: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        let ty = self.compile_expr(operand)?;

        match op {
            UnaryOp::Neg => match ty {
                ValueType::I64 => {
                    self.emit(Instr::NegI64);
                    Ok(ValueType::I64)
                }
                ValueType::F64 => {
                    self.emit(Instr::NegF64);
                    Ok(ValueType::F64)
                }
                _ => {
                    self.emit(Instr::DynamicNeg);
                    Ok(ty)
                }
            },
            UnaryOp::Not => {
                self.emit(Instr::NotBool);
                Ok(ValueType::Bool)
            }
            UnaryOp::Pos => {
                // Unary plus is identity
                Ok(ty)
            }
        }
    }

    pub(super) fn compile_call(
        &mut self,
        function: &str,
        args: &[Expr],
        _kwargs: &[(String, Expr)],
    ) -> Result<ValueType, SpecializationError> {
        // Handle known built-in math functions
        match function {
            // Math functions - single argument
            "sqrt" | "√" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "sqrt requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                if ty == ValueType::I64 {
                    self.emit(Instr::ToF64);
                }
                self.emit(Instr::SqrtF64);
                return Ok(ValueType::F64);
            }
            // Note: abs is now Pure Julia - no specialization needed here
            "floor" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "floor requires 1 argument".to_string(),
                    ));
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::FloorF64);
                return Ok(ValueType::F64);
            }
            "ceil" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "ceil requires 1 argument".to_string(),
                    ));
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CeilF64);
                return Ok(ValueType::F64);
            }
            "round" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "round requires 1 argument".to_string(),
                    ));
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Round, 1));
                return Ok(ValueType::F64);
            }
            // Note: sin, cos, tan, exp, log removed — now Pure Julia (base/math.jl)
            // Type conversion functions
            "Int" | "Int64" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "Int requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                if ty != ValueType::I64 {
                    self.emit(Instr::ToI64);
                }
                return Ok(ValueType::I64);
            }
            "Float64" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "Float64 requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                // If argument is a struct, there might be user-defined Float64 methods.
                // Fall back to generic version to properly dispatch to user methods.
                if matches!(ty, ValueType::Struct(_)) {
                    return Err(SpecializationError::Unsupported(
                        "Float64 on struct may have user-defined method".to_string(),
                    ));
                }
                if ty != ValueType::F64 {
                    self.emit(Instr::ToF64);
                }
                return Ok(ValueType::F64);
            }
            // min/max - implement manually with comparison using temp locals
            "min" => {
                if args.len() != 2 {
                    return Err(SpecializationError::Unsupported(
                        "min requires 2 arguments".to_string(),
                    ));
                }
                let lt = self.compile_expr(&args[0])?;
                let rt = self.compile_expr(&args[1])?;
                // min(a, b) = a < b ? a : b
                // Use temp locals since Rot3 doesn't exist
                match (lt, rt) {
                    (ValueType::I64, ValueType::I64) => {
                        // Stack: [a, b]
                        let temp_b = "__min_b";
                        self.emit(Instr::StoreI64(temp_b.to_string())); // [a], b stored
                        self.emit(Instr::DupI64); // [a, a]
                        self.emit(Instr::LoadI64(temp_b.to_string())); // [a, a, b]
                        self.emit(Instr::LtI64); // [a, (a<b)]
                        let jump_pos = self.code.len();
                        self.emit(Instr::JumpIfZero(0)); // if a >= b, jump to else
                                                         // then: keep a
                        let end_jump = self.code.len();
                        self.emit(Instr::Jump(0));
                        // else: pop a, push b
                        let else_pos = self.code.len();
                        self.code[jump_pos] = Instr::JumpIfZero(else_pos);
                        self.emit(Instr::Pop);
                        self.emit(Instr::LoadI64(temp_b.to_string()));
                        let end_pos = self.code.len();
                        self.code[end_jump] = Instr::Jump(end_pos);
                        return Ok(ValueType::I64);
                    }
                    (ValueType::F64, ValueType::F64) => {
                        let temp_b = "__min_b";
                        self.emit(Instr::StoreF64(temp_b.to_string()));
                        self.emit(Instr::DupF64);
                        self.emit(Instr::LoadF64(temp_b.to_string()));
                        self.emit(Instr::LtF64);
                        let jump_pos = self.code.len();
                        self.emit(Instr::JumpIfZero(0));
                        let end_jump = self.code.len();
                        self.emit(Instr::Jump(0));
                        let else_pos = self.code.len();
                        self.code[jump_pos] = Instr::JumpIfZero(else_pos);
                        self.emit(Instr::Pop);
                        self.emit(Instr::LoadF64(temp_b.to_string()));
                        let end_pos = self.code.len();
                        self.code[end_jump] = Instr::Jump(end_pos);
                        return Ok(ValueType::F64);
                    }
                    _ => {
                        return Err(SpecializationError::Unsupported(
                            "min with mixed or unknown types not yet supported".to_string(),
                        ));
                    }
                }
            }
            "max" => {
                if args.len() != 2 {
                    return Err(SpecializationError::Unsupported(
                        "max requires 2 arguments".to_string(),
                    ));
                }
                let lt = self.compile_expr(&args[0])?;
                let rt = self.compile_expr(&args[1])?;
                // max(a, b) = a > b ? a : b
                match (lt, rt) {
                    (ValueType::I64, ValueType::I64) => {
                        let temp_b = "__max_b";
                        self.emit(Instr::StoreI64(temp_b.to_string()));
                        self.emit(Instr::DupI64);
                        self.emit(Instr::LoadI64(temp_b.to_string()));
                        self.emit(Instr::GtI64);
                        let jump_pos = self.code.len();
                        self.emit(Instr::JumpIfZero(0));
                        let end_jump = self.code.len();
                        self.emit(Instr::Jump(0));
                        let else_pos = self.code.len();
                        self.code[jump_pos] = Instr::JumpIfZero(else_pos);
                        self.emit(Instr::Pop);
                        self.emit(Instr::LoadI64(temp_b.to_string()));
                        let end_pos = self.code.len();
                        self.code[end_jump] = Instr::Jump(end_pos);
                        return Ok(ValueType::I64);
                    }
                    (ValueType::F64, ValueType::F64) => {
                        let temp_b = "__max_b";
                        self.emit(Instr::StoreF64(temp_b.to_string()));
                        self.emit(Instr::DupF64);
                        self.emit(Instr::LoadF64(temp_b.to_string()));
                        self.emit(Instr::GtF64);
                        let jump_pos = self.code.len();
                        self.emit(Instr::JumpIfZero(0));
                        let end_jump = self.code.len();
                        self.emit(Instr::Jump(0));
                        let else_pos = self.code.len();
                        self.code[jump_pos] = Instr::JumpIfZero(else_pos);
                        self.emit(Instr::Pop);
                        self.emit(Instr::LoadF64(temp_b.to_string()));
                        let end_pos = self.code.len();
                        self.code[end_jump] = Instr::Jump(end_pos);
                        return Ok(ValueType::F64);
                    }
                    _ => {
                        return Err(SpecializationError::Unsupported(
                            "max with mixed or unknown types not yet supported".to_string(),
                        ));
                    }
                }
            }
            // println - print each argument and newline
            "println" => {
                for arg in args {
                    self.compile_expr(arg)?;
                    self.emit(Instr::PrintAnyNoNewline);
                }
                self.emit(Instr::PrintNewline);
                return Ok(ValueType::Nothing);
            }
            "print" => {
                for arg in args {
                    self.compile_expr(arg)?;
                    self.emit(Instr::PrintAnyNoNewline);
                }
                return Ok(ValueType::Nothing);
            }
            _ => {
                // Fall through to unsupported error for other function calls
            }
        }

        // User-defined functions or unknown built-ins
        Err(SpecializationError::Unsupported(format!(
            "Function call '{}' not yet supported for specialization",
            function
        )))
    }

    /// Compile built-in operations (Expr::Builtin)
    pub(super) fn compile_builtin(
        &mut self,
        name: BuiltinOp,
        args: &[Expr],
    ) -> Result<ValueType, SpecializationError> {
        match name {
            BuiltinOp::Sqrt => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "sqrt requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                if ty == ValueType::I64 {
                    self.emit(Instr::ToF64);
                }
                self.emit(Instr::SqrtF64);
                Ok(ValueType::F64)
            }
            // Note: BuiltinOp::Abs is removed - abs is now Pure Julia
            BuiltinOp::Zero => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "zero requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                self.emit(Instr::Pop);
                match ty {
                    ValueType::I64 => {
                        self.emit(Instr::PushI64(0));
                        Ok(ValueType::I64)
                    }
                    ValueType::F64 => {
                        self.emit(Instr::PushF64(0.0));
                        Ok(ValueType::F64)
                    }
                    _ => {
                        self.emit(Instr::PushI64(0));
                        Ok(ValueType::I64)
                    }
                }
            }
            BuiltinOp::Length => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "length requires 1 argument".to_string(),
                    ));
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
                Ok(ValueType::I64)
            }
            // Note: BuiltinOp::Sum removed — sum is now Pure Julia (base/array.jl)
            BuiltinOp::Rand => {
                // rand() with no args generates a single random float
                // rand(n) generates an array - not supported in specializer, fall back to generic
                if args.is_empty() {
                    self.emit(Instr::RandF64);
                    Ok(ValueType::F64)
                } else {
                    Err(SpecializationError::Unsupported(
                        "rand(n) array generation not supported in specializer".to_string(),
                    ))
                }
            }
            BuiltinOp::Randn => {
                // Same as rand - randn() generates a single value, randn(n) generates an array
                if args.is_empty() {
                    self.emit(Instr::RandnF64);
                    Ok(ValueType::F64)
                } else {
                    Err(SpecializationError::Unsupported(
                        "randn(n) array generation not supported in specializer".to_string(),
                    ))
                }
            }
            BuiltinOp::TypeOf => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "typeof requires 1 argument".to_string(),
                    ));
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TypeOf, 1));
                Ok(ValueType::DataType)
            }
            _ => Err(SpecializationError::Unsupported(format!(
                "Builtin {:?} not yet supported for specialization",
                name
            ))),
        }
    }

    /// Compile array literal [1, 2, 3]
    pub(super) fn compile_array_literal(
        &mut self,
        elements: &[Expr],
        shape: &[usize],
    ) -> Result<ValueType, SpecializationError> {
        // Infer element type from element expressions
        let array_elem_type = self.infer_array_element_type(elements);

        match array_elem_type {
            ArrayElementType::I64 => {
                self.emit(Instr::NewArrayTyped(ArrayElementType::I64, elements.len()));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::ArrayOf(ArrayElementType::I64))
            }
            ArrayElementType::F64 => {
                self.emit(Instr::NewArrayTyped(ArrayElementType::F64, elements.len()));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::ArrayOf(ArrayElementType::F64))
            }
            ArrayElementType::Bool => {
                self.emit(Instr::NewArrayTyped(ArrayElementType::Bool, elements.len()));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::ArrayOf(ArrayElementType::Bool))
            }
            ArrayElementType::String => {
                self.emit(Instr::NewArrayTyped(
                    ArrayElementType::String,
                    elements.len(),
                ));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::ArrayOf(ArrayElementType::String))
            }
            _ => {
                // Fall back to Any for other types
                self.emit(Instr::NewArrayTyped(ArrayElementType::Any, elements.len()));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::Array)
            }
        }
    }

    /// Infer the array element type from element expressions
    pub(super) fn infer_array_element_type(&self, elements: &[Expr]) -> ArrayElementType {
        if elements.is_empty() {
            return ArrayElementType::Any;
        }

        let mut has_float = false;
        let mut has_int = false;
        let mut has_bool = false;
        let mut has_string = false;
        let mut has_other = false;

        for elem in elements {
            match self.infer_literal_type(elem) {
                Some(ValueType::I64)
                | Some(ValueType::I32)
                | Some(ValueType::I8)
                | Some(ValueType::I16)
                | Some(ValueType::I128)
                | Some(ValueType::U8)
                | Some(ValueType::U16)
                | Some(ValueType::U32)
                | Some(ValueType::U64)
                | Some(ValueType::U128) => has_int = true,
                Some(ValueType::F64) | Some(ValueType::F32) => has_float = true,
                Some(ValueType::Bool) => has_bool = true,
                Some(ValueType::Str) => has_string = true,
                _ => has_other = true,
            }
        }

        // Determine array element type based on element types
        if has_other {
            ArrayElementType::Any
        } else if has_string && !has_int && !has_float && !has_bool {
            ArrayElementType::String
        } else if has_float && !has_string && !has_bool {
            // Float or mixed int/float -> F64
            ArrayElementType::F64
        } else if has_int && !has_float && !has_string && !has_bool {
            // Pure int -> I64
            ArrayElementType::I64
        } else if has_bool && !has_int && !has_float && !has_string {
            // Pure bool -> Bool
            ArrayElementType::Bool
        } else {
            ArrayElementType::Any
        }
    }

    /// Try to infer the type of a literal expression
    pub(super) fn infer_literal_type(&self, expr: &Expr) -> Option<ValueType> {
        match expr {
            Expr::Literal(lit, _) => match lit {
                Literal::Int(_) => Some(ValueType::I64),
                Literal::Float(_) => Some(ValueType::F64),
                Literal::Float32(_) => Some(ValueType::F32),
                Literal::Float16(_) => Some(ValueType::F16),
                Literal::Bool(_) => Some(ValueType::Bool),
                Literal::Str(_) => Some(ValueType::Str),
                _ => None,
            },
            Expr::Var(name, _) => {
                // Check if it's a known local with a specific type
                self.locals.get(name).cloned()
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                // For arithmetic ops, infer from operands
                let lt = self.infer_literal_type(left);
                let rt = self.infer_literal_type(right);
                match (lt, rt, op) {
                    // Division always produces F64
                    (Some(_), Some(_), BinaryOp::Div) => Some(ValueType::F64),
                    // Numeric ops with any float produce float
                    (Some(ValueType::F64), _, _)
                    | (_, Some(ValueType::F64), _)
                    | (Some(ValueType::F32), _, _)
                    | (_, Some(ValueType::F32), _) => Some(ValueType::F64),
                    // Integer ops produce integer
                    (Some(ValueType::I64), Some(ValueType::I64), _) => Some(ValueType::I64),
                    _ => None,
                }
            }
            Expr::UnaryOp { operand, .. } => self.infer_literal_type(operand),
            _ => None,
        }
    }

    /// Compile array indexing arr[i] or arr[i, j]
    pub(super) fn compile_index(
        &mut self,
        array: &Expr,
        indices: &[Expr],
    ) -> Result<ValueType, SpecializationError> {
        let array_type = self.infer_literal_type(array);

        // Check if any index is a Range (slice operation)
        let has_slice = indices
            .iter()
            .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }));

        // Compile array
        self.compile_expr(array)?;

        // Compile indices
        for idx in indices {
            self.compile_expr(idx)?;
        }

        // Emit appropriate index instruction
        if has_slice {
            self.emit(Instr::IndexSlice(indices.len()));
            return Ok(ValueType::Any);
        } else {
            self.emit(Instr::IndexLoad(indices.len()));
        }

        match array_type {
            Some(ValueType::ArrayOf(elem_ty)) | Some(ValueType::MemoryOf(elem_ty)) => {
                Ok(Self::value_type_from_array_element_type(&elem_ty))
            }
            _ => Ok(ValueType::Any),
        }
    }

    /// Compile tuple (a, b, c)
    pub(super) fn compile_tuple(&mut self, elements: &[Expr]) -> Result<ValueType, SpecializationError> {
        // Compile all elements
        for elem in elements {
            self.compile_expr(elem)?;
        }

        // Emit tuple construction
        self.emit(Instr::NewTuple(elements.len()));
        Ok(ValueType::Tuple)
    }

    /// Compile range expression start:stop or start:step:stop
    pub(super) fn compile_range(
        &mut self,
        start: &Expr,
        step: Option<&Expr>,
        stop: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        // MakeRangeLazy expects: start, step, stop on stack
        self.compile_expr(start)?;
        if let Some(step_expr) = step {
            self.compile_expr(step_expr)?;
        } else {
            // Default step is 1
            self.emit(Instr::PushI64(1));
        }
        self.compile_expr(stop)?;
        self.emit(Instr::MakeRangeLazy);
        Ok(ValueType::Range)
    }

    /// Check if an expression might produce Any type (e.g., array indexing).
    /// Used to determine if we need to use dynamic operations to avoid type changes in loops.
    pub(super) fn expr_might_produce_any(&self, expr: &Expr) -> bool {
        match expr {
            // Array indexing always returns Any
            Expr::Index { .. } => true,
            // Variables that are already Any
            Expr::Var(name, _) => self.locals.get(name).cloned() == Some(ValueType::Any),
            // Binary operations with Any operands
            Expr::BinaryOp { left, right, .. } => {
                self.expr_might_produce_any(left) || self.expr_might_produce_any(right)
            }
            // Unary operations on Any values
            Expr::UnaryOp { operand, .. } => self.expr_might_produce_any(operand),
            // Most function calls might return unknown types
            Expr::Call { .. } => true,
            // Literals and other expressions are safe
            _ => false,
        }
    }

    pub(super) fn value_type_from_array_element_type(elem_ty: &ArrayElementType) -> ValueType {
        match elem_ty {
            ArrayElementType::I8 => ValueType::I8,
            ArrayElementType::I16 => ValueType::I16,
            ArrayElementType::I32 => ValueType::I32,
            ArrayElementType::I64 => ValueType::I64,
            ArrayElementType::U8 => ValueType::U8,
            ArrayElementType::U16 => ValueType::U16,
            ArrayElementType::U32 => ValueType::U32,
            ArrayElementType::U64 => ValueType::U64,
            ArrayElementType::F32 => ValueType::F32,
            ArrayElementType::F64 => ValueType::F64,
            ArrayElementType::Bool => ValueType::Bool,
            ArrayElementType::String => ValueType::Str,
            ArrayElementType::Char => ValueType::Char,
            ArrayElementType::StructOf(type_id)
            | ArrayElementType::StructInlineOf(type_id, _) => ValueType::Struct(*type_id),
            ArrayElementType::TupleOf(_) => ValueType::Tuple,
            ArrayElementType::Any
            | ArrayElementType::Struct
            | ArrayElementType::ComplexF32
            | ArrayElementType::ComplexF64 => ValueType::Any,
        }
    }
}
