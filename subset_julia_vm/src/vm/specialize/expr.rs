//! Expression specialization.

use crate::builtins::BuiltinId;
// Issue #8192: typed scalar binary-op instruction selection is shared with the
// main compiler so the two codegen paths cannot diverge. See the helper's doc
// comment and `docs/vm/BINARY_DISPATCH.md` ("Two binary-op codegen paths").
use crate::compile::typed_scalar_binary_instr;
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::vm::value::is_array_wrapper_struct_name;
use crate::vm::{ArrayElementType, Instr, ValueType};

use super::helpers::expr_variant_name;
use super::{FunctionSpecializer, SpecializationError};

fn unary_float_preserving_result_type(arg: ValueType) -> ValueType {
    match arg {
        ValueType::F16 | ValueType::F32 | ValueType::F64 => arg,
        ValueType::Any => ValueType::Any,
        _ => ValueType::F64,
    }
}

/// Map an arithmetic/comparison operator *function name* to its [`BinaryOp`].
///
/// The parser emits operator applications such as the n-ary product
/// `k * b.x * dt` as an `Expr::Call { function: "*", args: [k, b.x, dt] }`
/// rather than nested `Expr::BinaryOp`s (Julia's `*`/`+` are variadic). Without
/// this, any statement containing a chained `*`/`+` aborted specialization. The
/// operator is folded left-associatively through [`FunctionSpecializer::emit_binary_op`],
/// which keeps the existing primitive fast paths and falls back to the generic
/// VM body for non-numeric operands. (Issue #6346)
fn binary_op_from_operator_name(name: &str) -> Option<BinaryOp> {
    Some(match name {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Mod,
        "^" => BinaryOp::Pow,
        "÷" => BinaryOp::IntDiv,
        "<" => BinaryOp::Lt,
        ">" => BinaryOp::Gt,
        "<=" => BinaryOp::Le,
        ">=" => BinaryOp::Ge,
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        _ => return None,
    })
}

fn is_complex_f64_type(ty: &ValueType) -> bool {
    matches!(ty, ValueType::ComplexF64)
}

fn is_imaginary_unit_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::Var(name, _) if name == "im")
}

fn imaginary_multiplier_expr<'a>(left: &'a Expr, right: &'a Expr) -> Option<&'a Expr> {
    if is_imaginary_unit_expr(right) {
        Some(left)
    } else if is_imaginary_unit_expr(left) {
        Some(right)
    } else {
        None
    }
}

fn imaginary_product_expr(expr: &Expr) -> Option<&Expr> {
    let Expr::BinaryOp {
        op, left, right, ..
    } = expr
    else {
        return None;
    };
    if !matches!(op, BinaryOp::Mul) {
        return None;
    }
    imaginary_multiplier_expr(left, right)
}

fn real_plus_imaginary_components<'a>(
    op: BinaryOp,
    left: &'a Expr,
    right: &'a Expr,
) -> Option<(&'a Expr, &'a Expr, f64)> {
    match op {
        BinaryOp::Add => imaginary_product_expr(right)
            .map(|imag_expr| (left, imag_expr, 1.0))
            .or_else(|| imaginary_product_expr(left).map(|imag_expr| (right, imag_expr, 1.0))),
        BinaryOp::Sub => imaginary_product_expr(right).map(|imag_expr| (left, imag_expr, -1.0)),
        _ => None,
    }
}

impl FunctionSpecializer<'_> {
    fn module_private_type_object_name(&self, name: &str) -> Option<String> {
        if self.locals.contains_key(name) || name.contains('.') {
            return None;
        }
        let module_path = self.current_module_path.as_ref()?;
        let qualified = format!("{}.{}", module_path, name);
        self.struct_defs
            .iter()
            .any(|def| def.name == qualified)
            .then_some(qualified)
    }

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
            Expr::FieldAccess { object, field, .. } => self.compile_field_access(object, field),
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

    pub(super) fn compile_literal(
        &mut self,
        lit: &Literal,
    ) -> Result<ValueType, SpecializationError> {
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
            if let Some(qualified) = self.module_private_type_object_name(name) {
                self.emit(Instr::PushDataType(qualified));
                return Ok(ValueType::DataType);
            }
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
            if let Some(type_name) = self.resolve_type_object_name(name) {
                self.emit(Instr::PushDataType(type_name));
                return Ok(ValueType::DataType);
            }
        }

        let ty = self.locals.get(name).cloned().unwrap_or(ValueType::Any);
        match ty {
            ValueType::I64 => self.emit(Instr::LoadI64(name.to_string())),
            ValueType::F64 => self.emit(Instr::LoadF64(name.to_string())),
            ValueType::F32 => self.emit(Instr::LoadF32(name.to_string())),
            ValueType::F16 => self.emit(Instr::LoadF16(name.to_string())),
            ValueType::Str => self.emit(Instr::LoadStr(name.to_string())),
            ValueType::Array | ValueType::ArrayOf(_, _) => {
                self.emit(Instr::LoadArray(name.to_string()))
            }
            _ => self.emit(Instr::LoadAny(name.to_string())),
        }
        Ok(ty)
    }

    fn resolve_type_object_name(&self, name: &str) -> Option<String> {
        if let Some(module_path) = self.module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.type_object_names.contains(&qualified) {
                return Some(qualified);
            }
        }

        self.type_object_names
            .contains(name)
            .then(|| name.to_string())
    }

    /// Runtime arg-type specialized binary-op codegen (Issue #8167). This is the
    /// *second* of the two binary-op codegen paths (Issue #8192): the main
    /// ahead-of-time path is `compile::CoreCompiler::compile_binary_op`. Typed
    /// `Int64`/`Float64` instruction selection is shared via
    /// [`typed_scalar_binary_instr`]; promotion must stay `Swap`-free on the hot
    /// path so the native typed-loop recognizer (`vm::executable`) keeps matching
    /// (the #8183 footgun). See `docs/vm/BINARY_DISPATCH.md`
    /// ("Two binary-op codegen paths").
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
        if let Some(ty) = self.try_compile_complex_binary_op(op, left, right)? {
            return Ok(ty);
        }
        if matches!(op, BinaryOp::Pow) && matches!(right, Expr::Literal(Literal::Int(2), _)) {
            let lt = self.compile_expr(left)?;
            match lt {
                ValueType::I64 => {
                    self.emit(Instr::DupI64);
                    self.emit(Instr::MulI64);
                    return Ok(ValueType::I64);
                }
                ValueType::F64 => {
                    self.emit(Instr::DupF64);
                    self.emit(Instr::MulF64);
                    return Ok(ValueType::F64);
                }
                _ => {
                    self.emit(Instr::PushI64(2));
                    self.emit(Instr::DynamicPow);
                    return Ok(ValueType::Any);
                }
            }
        }

        // Issue #8183 / #8192: keep scalar `Int64`/`Float64` arithmetic and
        // division on typed `…ToF64; <op>F64` instructions. The default
        // `compile_left; compile_right; emit_binary_op` path promotes an I64
        // operand that needs widening with `Swap; ToF64; Swap` once both operands
        // are on the stack, and that stray `Swap` aborts native typed-loop
        // recognition (it has no place on the predecoder's split typed stacks —
        // the #8183 footgun). Coercing each operand to Float64 *as it is compiled*
        // keeps the body on the instructions the recognizer matches and needs no
        // Swap. This is output-identical: Julia promotes an int-and-float pair to
        // the float, and `/` always yields Float64. Scoped to I64/F64 so the
        // coerced operands stay on the predecoder's I64/F64 stacks; other widths
        // fall through to the (correct, if unrecognized) generic path.
        //
        // Covered here, Swap-free: mixed `Int/Float` `+ - * /`, *and* any
        // `Int64 / Int64` division (which also forces Float64). Same-type `+ - *`
        // stay on their native I64/F64 ops via `emit_binary_op` (no Swap there).
        if let (Some(lt), Some(rt)) = (
            self.infer_literal_type(left),
            self.infer_literal_type(right),
        ) {
            let both_i64_f64 = matches!(lt, ValueType::I64 | ValueType::F64)
                && matches!(rt, ValueType::I64 | ValueType::F64);
            let needs_f64_promotion = matches!(op, BinaryOp::Div)
                || (matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) && lt != rt);
            // `typed_scalar_binary_instr(.., true)` is `Some` for Add/Sub/Mul/Div;
            // `if let` keeps this panic-free (a `None` simply falls through to the
            // generic path below) per the VM's no-panic policy (Issue #2193).
            if both_i64_f64 && needs_f64_promotion {
                if let Some(instr) = typed_scalar_binary_instr(op, true) {
                    self.compile_numeric_as_f64(left)?;
                    self.compile_numeric_as_f64(right)?;
                    self.emit(instr);
                    return Ok(ValueType::F64);
                }
            }
        }

        let lt = self.compile_expr(left)?;
        let rt = self.compile_expr(right)?;

        // Emit typed instruction based on inferred types
        let result_type = self.emit_binary_op(op, lt, rt)?;
        Ok(result_type)
    }

    fn try_compile_complex_binary_op(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<Option<ValueType>, SpecializationError> {
        if matches!(op, BinaryOp::Mul) {
            if let Some(imag_expr) = imaginary_multiplier_expr(left, right) {
                return self.compile_imaginary_multiple_f64(imag_expr).map(Some);
            }
        }

        if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            if let Some((real_expr, imag_expr, imag_sign)) =
                real_plus_imaginary_components(op, left, right)
            {
                return self
                    .compile_complex_from_real_imag_f64(real_expr, imag_expr, imag_sign)
                    .map(Some);
            }
        }

        if matches!(op, BinaryOp::Pow)
            && matches!(right, Expr::Literal(Literal::Int(2), _))
            && self
                .infer_literal_type(left)
                .as_ref()
                .is_some_and(is_complex_f64_type)
        {
            return self.compile_complex_square_f64(left).map(Some);
        }

        if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
            && self
                .infer_literal_type(left)
                .as_ref()
                .is_some_and(is_complex_f64_type)
            && self
                .infer_literal_type(right)
                .as_ref()
                .is_some_and(is_complex_f64_type)
        {
            return self.compile_complex_binary_f64(op, left, right).map(Some);
        }

        Ok(None)
    }

    fn temp_name(&self, prefix: &str) -> String {
        format!("__sjulia_spec_{}_{}", prefix, self.code.len())
    }

    fn compile_numeric_as_f64(&mut self, expr: &Expr) -> Result<(), SpecializationError> {
        let ty = self.compile_expr(expr)?;
        match ty {
            ValueType::F64 => Ok(()),
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
            | ValueType::F16
            | ValueType::F32
            | ValueType::Bool => {
                self.emit(Instr::ToF64);
                Ok(())
            }
            other => Err(SpecializationError::Unsupported(format!(
                "ComplexF64 fast path requires a real numeric operand, got {:?}",
                other
            ))),
        }
    }

    fn compile_imaginary_multiple_f64(
        &mut self,
        imag_expr: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        self.emit(Instr::PushF64(0.0));
        self.compile_numeric_as_f64(imag_expr)?;
        self.emit(Instr::NewParametricStruct("Complex".to_string(), 2));
        Ok(ValueType::ComplexF64)
    }

    fn compile_complex_from_real_imag_f64(
        &mut self,
        real_expr: &Expr,
        imag_expr: &Expr,
        imag_sign: f64,
    ) -> Result<ValueType, SpecializationError> {
        self.compile_numeric_as_f64(real_expr)?;
        self.compile_numeric_as_f64(imag_expr)?;
        if imag_sign < 0.0 {
            self.emit(Instr::NegF64);
        }
        self.emit(Instr::NewParametricStruct("Complex".to_string(), 2));
        Ok(ValueType::ComplexF64)
    }

    fn store_complex_f64_temp(
        &mut self,
        expr: &Expr,
        name: &str,
    ) -> Result<(), SpecializationError> {
        let ty = self.compile_expr(expr)?;
        if !is_complex_f64_type(&ty) {
            return Err(SpecializationError::Unsupported(format!(
                "ComplexF64 fast path expected ComplexF64, got {:?}",
                ty
            )));
        }
        self.emit(Instr::StoreAny(name.to_string()));
        Ok(())
    }

    fn load_complex_f64_temp_field(&mut self, name: &str, field: usize) {
        self.emit(Instr::LoadAny(name.to_string()));
        self.emit(Instr::GetField(field));
    }

    fn compile_complex_abs2_f64(&mut self, expr: &Expr) -> Result<ValueType, SpecializationError> {
        let temp = self.temp_name("abs2");
        self.store_complex_f64_temp(expr, &temp)?;
        self.load_complex_f64_temp_field(&temp, 0);
        self.emit(Instr::DupF64);
        self.emit(Instr::MulF64);
        self.load_complex_f64_temp_field(&temp, 1);
        self.emit(Instr::DupF64);
        self.emit(Instr::MulF64);
        self.emit(Instr::AddF64);
        Ok(ValueType::F64)
    }

    fn compile_complex_square_f64(
        &mut self,
        expr: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        let temp = self.temp_name("square");
        self.store_complex_f64_temp(expr, &temp)?;

        self.load_complex_f64_temp_field(&temp, 0);
        self.emit(Instr::DupF64);
        self.emit(Instr::MulF64);
        self.load_complex_f64_temp_field(&temp, 1);
        self.emit(Instr::DupF64);
        self.emit(Instr::MulF64);
        self.emit(Instr::SubF64);

        self.load_complex_f64_temp_field(&temp, 0);
        self.load_complex_f64_temp_field(&temp, 1);
        self.emit(Instr::MulF64);
        self.emit(Instr::PushF64(2.0));
        self.emit(Instr::MulF64);

        self.emit(Instr::NewParametricStruct("Complex".to_string(), 2));
        Ok(ValueType::ComplexF64)
    }

    fn compile_complex_binary_f64(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        let left_temp = self.temp_name("left");
        self.store_complex_f64_temp(left, &left_temp)?;
        let right_temp = self.temp_name("right");
        self.store_complex_f64_temp(right, &right_temp)?;

        match op {
            BinaryOp::Add | BinaryOp::Sub => {
                self.load_complex_f64_temp_field(&left_temp, 0);
                self.load_complex_f64_temp_field(&right_temp, 0);
                if matches!(op, BinaryOp::Add) {
                    self.emit(Instr::AddF64);
                } else {
                    self.emit(Instr::SubF64);
                }
                self.load_complex_f64_temp_field(&left_temp, 1);
                self.load_complex_f64_temp_field(&right_temp, 1);
                if matches!(op, BinaryOp::Add) {
                    self.emit(Instr::AddF64);
                } else {
                    self.emit(Instr::SubF64);
                }
            }
            BinaryOp::Mul => {
                self.load_complex_f64_temp_field(&left_temp, 0);
                self.load_complex_f64_temp_field(&right_temp, 0);
                self.emit(Instr::MulF64);
                self.load_complex_f64_temp_field(&left_temp, 1);
                self.load_complex_f64_temp_field(&right_temp, 1);
                self.emit(Instr::MulF64);
                self.emit(Instr::SubF64);

                self.load_complex_f64_temp_field(&left_temp, 0);
                self.load_complex_f64_temp_field(&right_temp, 1);
                self.emit(Instr::MulF64);
                self.load_complex_f64_temp_field(&left_temp, 1);
                self.load_complex_f64_temp_field(&right_temp, 0);
                self.emit(Instr::MulF64);
                self.emit(Instr::AddF64);
            }
            _ => {
                return Err(SpecializationError::Unsupported(format!(
                    "unsupported ComplexF64 binary op: {:?}",
                    op
                )));
            }
        }

        self.emit(Instr::NewParametricStruct("Complex".to_string(), 2));
        Ok(ValueType::ComplexF64)
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
                if let Some(instr) = dynamic_struct_binary_instr(op, &lt, &rt) {
                    self.emit(instr);
                    return Ok(dynamic_struct_binary_result_type(op, &lt, &rt));
                }

                // Other non-primitive binary operations must keep the generic VM body.
                // The generic compiler can emit CallDynamicBinaryBoth with method
                // candidates, so user methods such as `+(::Vector{Int64}, ...)`
                // win before fallback. The specialization compiler only emits Dynamic*
                // above for struct scalar arithmetic, where VM Dynamic* performs Julia
                // method lookup before falling back.
                return Err(SpecializationError::Unsupported(format!(
                    "Dynamic binary op requires generic dispatch: {:?} ({:?}, {:?})",
                    op, lt, rt
                )));
            }
        };

        // Emit the typed instruction. Issue #8192: resolve the I64/F64 op through
        // the shared `compile::typed_scalar_binary_instr` table the main compiler
        // also uses, so the two binary-op codegen paths cannot drift apart. Ops
        // with no single typed instruction (`÷`, `^`, float `%`) keep the bespoke
        // dynamic lowering below. `result_ty` here is already promoted to I64 or
        // F64 by the block above, so `result_is_float` selects the right variant.
        let result_is_float = result_ty == ValueType::F64;
        // `typed_scalar_binary_instr` is `Some` for these ops; resolve it
        // fallibly (no panic on `None`) so the VM stays panic-free (Issue #2193):
        // an unexpected `None` degrades to a recoverable specialization failure.
        let typed_instr = |op| {
            typed_scalar_binary_instr(op, result_is_float).ok_or_else(|| {
                SpecializationError::Unsupported(format!("no typed scalar instruction for {op:?}"))
            })
        };
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                self.emit(typed_instr(op)?);
            }
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => {
                self.emit(typed_instr(op)?);
                return Ok(ValueType::Bool);
            }
            BinaryOp::Div => {
                // Division always produces F64. The promotion block above leaves
                // the operands either both I64 or both F64, so coerce any I64
                // operands up to F64 before `DivF64`. (The hot-loop entry through
                // `compile_binary_op` avoids this on-stack `Swap` by coercing each
                // operand as it is compiled — see #8183 / #8192. This fallback,
                // reached e.g. by the n-ary `/(a, b, c)` fold, stays correct.)
                if promoted_lt == ValueType::I64 {
                    self.emit(Instr::Swap);
                    self.emit(Instr::ToF64);
                    self.emit(Instr::Swap);
                }
                if promoted_rt == ValueType::I64 {
                    self.emit(Instr::ToF64);
                }
                self.emit(typed_instr(op)?);
                return Ok(ValueType::F64);
            }
            BinaryOp::Mod => match typed_scalar_binary_instr(op, result_is_float) {
                Some(instr) => self.emit(instr), // ModI64 for the integer case
                None => {
                    // Float `%` uses fmod semantics (Issue #1762).
                    self.emit(Instr::DynamicMod);
                    return Ok(ValueType::Any);
                }
            },
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
            "abs2" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "abs2 requires 1 argument".to_string(),
                    ));
                }
                if self
                    .infer_literal_type(&args[0])
                    .as_ref()
                    .is_some_and(is_complex_f64_type)
                {
                    return self.compile_complex_abs2_f64(&args[0]);
                }
            }
            // Math functions - single argument
            "sqrt" | "√" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "sqrt requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                self.emit(Instr::SqrtF64);
                return Ok(unary_float_preserving_result_type(ty));
            }
            // Note: abs is now Pure Julia - no specialization needed here
            "floor" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "floor requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                self.emit(Instr::FloorF64);
                return Ok(unary_float_preserving_result_type(ty));
            }
            "ceil" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "ceil requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                self.emit(Instr::CeilF64);
                return Ok(unary_float_preserving_result_type(ty));
            }
            "round" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "round requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Round, 1));
                return Ok(unary_float_preserving_result_type(ty));
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
            // println - print each argument and newline (stdout only).
            //
            // Issue #5035: this fast path writes every argument to stdout, which
            // is WRONG for `println(io, ...)` / `print(io, ...)` where the first
            // argument is an IO sink (IOBuffer/stdout/stderr/devnull). The
            // generic compiler routes those through IOPrint, but the specializer
            // has no IO type info (a top-level `buf = IOBuffer()` is a global, not
            // a typed local), so it would dump the buffer value to stdout instead
            // of writing to the sink. Only take this fast path when the first
            // argument is *definitely* a non-IO primitive; otherwise bail to the
            // generic bytecode which handles IO routing correctly.
            "println" if self.first_arg_is_definitely_not_io(args) => {
                for arg in args {
                    self.compile_expr(arg)?;
                    self.emit(Instr::PrintAnyNoNewline);
                }
                self.emit(Instr::PrintNewline);
                return Ok(ValueType::Nothing);
            }
            "print" if self.first_arg_is_definitely_not_io(args) => {
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

        // Operator applications the parser emits as a call (notably the n-ary
        // `*(a, b, c)` / `+(a, b, c)` forms). Fold left-associatively through the
        // typed binary-op path; `emit_binary_op` keeps the I64/F64 fast paths and
        // returns `Unsupported` for non-numeric operands, so user-overloaded
        // operators still fall back to generic dispatch. (Issue #6346)
        if let Some(op) = binary_op_from_operator_name(function) {
            if args.len() >= 2 {
                let mut acc_ty = self.compile_expr(&args[0])?;
                for arg in &args[1..] {
                    let rhs_ty = self.compile_expr(arg)?;
                    acc_ty = self.emit_binary_op(op, acc_ty, rhs_ty)?;
                }
                return Ok(acc_ty);
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
                self.emit(Instr::SqrtF64);
                Ok(unary_float_preserving_result_type(ty))
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
                Ok(ValueType::ArrayOf(ArrayElementType::I64, None))
            }
            ArrayElementType::F64 => {
                self.emit(Instr::NewArrayTyped(ArrayElementType::F64, elements.len()));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::ArrayOf(ArrayElementType::F64, None))
            }
            ArrayElementType::Bool => {
                self.emit(Instr::NewArrayTyped(ArrayElementType::Bool, elements.len()));
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.emit(Instr::PushElemTyped);
                }
                self.emit(Instr::FinalizeArrayTyped(shape.to_vec()));
                Ok(ValueType::ArrayOf(ArrayElementType::Bool, None))
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
                Ok(ValueType::ArrayOf(ArrayElementType::String, None))
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

    /// Whether the first argument of a `print`/`println` call is *definitely*
    /// not an IO sink, so the stdout-only specializer fast path is safe to use.
    ///
    /// Only the first argument can be an IO stream in `print(io, ...)` /
    /// `println(io, ...)`. The specializer fast path writes every argument to
    /// stdout, which silently corrupts IO routing, so we restrict it to calls
    /// whose first argument is a statically-known non-IO primitive
    /// (Int/Float/Bool/String). Anything else — an `Any`-typed local/global, a
    /// call result, `stdout`/`stderr`, etc. — bails to the generic bytecode,
    /// which dispatches IO correctly (Issue #5035). A zero-arg `println()` is
    /// trivially safe (no IO sink).
    pub(super) fn first_arg_is_definitely_not_io(&self, args: &[Expr]) -> bool {
        match args.first() {
            None => true,
            Some(first) => matches!(
                self.infer_literal_type(first),
                Some(
                    ValueType::I64
                        | ValueType::F64
                        | ValueType::F32
                        | ValueType::F16
                        | ValueType::Bool
                        | ValueType::Str
                )
            ),
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
                    (Some(ValueType::ComplexF64), Some(ValueType::I64), BinaryOp::Pow)
                        if matches!(right.as_ref(), Expr::Literal(Literal::Int(2), _)) =>
                    {
                        Some(ValueType::ComplexF64)
                    }
                    (
                        Some(ValueType::ComplexF64),
                        Some(ValueType::ComplexF64),
                        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul,
                    ) => Some(ValueType::ComplexF64),
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
        // Issue #6561: a constant index into a tracked tuple-literal temporary
        // (`a = __tuple_tmp[1]`, the desugared form of `a, b = b, a % b`) keeps
        // the destructured binding type-stable instead of widening to `Any`.
        if let Some(ty) = self.try_compile_tracked_tuple_index(array, indices)? {
            return Ok(ty);
        }

        let array_type = self.infer_literal_type(array);

        // Issue #6657: when a user `getindex` override exists on a native array
        // receiver, the native-indexing fast path below would bypass it. Bail so
        // the generic body's runtime `getindex` dispatch is used instead. Scoped
        // to array-like receivers (and unknown `Any` receivers) so tuple/string
        // constant-index specialization is unaffected.
        if self.disable_array_index_fast_path
            && matches!(
                array_type,
                Some(ValueType::Array)
                    | Some(ValueType::ArrayOf(_, _))
                    | Some(ValueType::MemoryOf(_))
                    | None
            )
        {
            return Err(SpecializationError::Unsupported(
                "scalar getindex with a user array override must use generic dispatch (#6657)"
                    .to_string(),
            ));
        }

        // Check if any index is a slice operation. As well as a literal range /
        // `:` (`a[2:3]`, `a[:]`), an index whose SPECIALIZED type is a runtime
        // `Range`/`AbstractVector` selects a sub-array — e.g. `f(a, k) = a[k]`
        // specialized for `f(arr, 2:3)` binds `k` to `ValueType::Range`. Without
        // this the specializer emitted a scalar `IndexLoad` and inferred the
        // array element type (I64), so `ReturnI64` later coerced/rejected the
        // sub-array result (Issue #5747).
        let has_slice = indices.iter().any(|idx| {
            matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. })
                || matches!(
                    self.infer_literal_type(idx),
                    Some(ValueType::Range)
                        | Some(ValueType::Array)
                        | Some(ValueType::ArrayOf(_, _))
                        | Some(ValueType::MemoryOf(_))
                )
        });

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
            Some(ValueType::ArrayOf(elem_ty, _)) | Some(ValueType::MemoryOf(elem_ty)) => {
                Ok(elem_ty.to_value_type())
            }
            _ => Ok(ValueType::Any),
        }
    }

    /// Try to compile a constant index into a tracked tuple-literal temporary
    /// as a type-stable read (Issue #6561).
    ///
    /// Returns `Ok(Some(element_type))` when `array` is a `Var` whose tuple
    /// element types were recorded by `Stmt::Assign` (see
    /// [`tuple_element_types`](FunctionSpecializer::tuple_element_types)), the
    /// single index is a constant integer literal in bounds, and that element's
    /// type is one the specializer can store with a typed `Store*` (`I64`/`F64`).
    ///
    /// The recorded element type is the type the specializer *itself* emitted
    /// for that tuple element, so the `Value` pushed by `IndexLoad` carries
    /// exactly that tag — no dynamic coercion is needed (unlike the interpreter's
    /// `compile/stmt.rs` arm, which sees an approximate static `infer_julia_type`
    /// and coerces defensively). The emitted sequence is just `LoadAny(temp)` /
    /// `PushI64(k)` / `IndexLoad(1)`; the caller's `emit_store` then picks the
    /// typed `StoreI64`/`StoreF64` from the returned type. Every other case
    /// returns `Ok(None)` to fall through to the generic `compile_index` path
    /// (which yields `Any`).
    fn try_compile_tracked_tuple_index(
        &mut self,
        array: &Expr,
        indices: &[Expr],
    ) -> Result<Option<ValueType>, SpecializationError> {
        if indices.len() != 1 {
            return Ok(None);
        }
        let Expr::Var(name, _) = array else {
            return Ok(None);
        };
        let Some(elem_types) = self.tuple_element_types.get(name).cloned() else {
            return Ok(None);
        };
        let Expr::Literal(Literal::Int(k), _) = &indices[0] else {
            return Ok(None);
        };
        let k = *k;
        // 1-based Julia index; `checked_sub` + `try_from` reject any
        // non-positive (or overflowing) `k - 1` rather than panicking.
        let Some(idx0) = k.checked_sub(1) else {
            return Ok(None);
        };
        let Ok(idx) = usize::try_from(idx0) else {
            return Ok(None);
        };
        let Some(elem_ty) = elem_types.get(idx).cloned() else {
            return Ok(None);
        };
        // Only sharpen the primitive numerics the specializer can store with a
        // typed `Store*`; everything else stays on the generic `Any` path so
        // load/store representations remain consistent.
        if !matches!(elem_ty, ValueType::I64 | ValueType::F64) {
            return Ok(None);
        }
        self.emit(Instr::LoadAny(name.to_string()));
        self.emit(Instr::PushI64(k));
        self.emit(Instr::IndexLoad(1));
        Ok(Some(elem_ty))
    }

    /// Compile a struct field read `object.field` (Issue #6346).
    ///
    /// Mirrors the interpreter's `compile_field_access` typed-struct path: the
    /// object is compiled, and when its specialized type is a known
    /// `ValueType::Struct(type_id)` the field index and type are resolved
    /// statically from the borrowed `struct_defs`, emitting a direct
    /// `GetField(idx)`. Any other operand (an `Any`-typed value, a module path,
    /// or a Complex/Expr builtin field) is left to the interpreter fallback by
    /// returning `Unsupported`.
    pub(super) fn compile_field_access(
        &mut self,
        object: &Expr,
        field: &str,
    ) -> Result<ValueType, SpecializationError> {
        // Issue #8127: when the program defines a user `getproperty` override, a
        // direct `GetField` would bypass it. Abandon specialization so the read
        // runs through the interpreter's `getproperty` dispatch (which compiles
        // `obj.field` to a `getproperty(obj, :field)` call).
        if self.disable_field_access {
            return Err(SpecializationError::Unsupported(
                "FieldAccess specialization disabled by a user getproperty override (Issue #8127)"
                    .to_string(),
            ));
        }
        let obj_ty = self.compile_expr(object)?;
        let ValueType::Struct(type_id) = obj_ty else {
            return Err(SpecializationError::Unsupported(format!(
                "FieldAccess specialization requires a known struct operand, got {:?}",
                obj_ty
            )));
        };
        // Issue #6804: array values are observed as the faithful `Array{T,N}`
        // wrapper struct at runtime, but its fields are parametric/special
        // (`ref::MemoryRef{T}`, `size::NTuple{N,Int}`) and `resolve_struct_field`
        // can mis-type them (e.g. `size` as the element type rather than the
        // tuple), so a statically-typed `GetField` would wrongly coerce the
        // result. Leave array field access to the interpreter, whose
        // `GetFieldByName` returns the correct values (`a.size`, `a.ref`).
        if self
            .struct_defs
            .get(type_id)
            .is_some_and(|def| is_array_wrapper_struct_name(&def.name))
        {
            return Err(SpecializationError::Unsupported(
                "FieldAccess on an array wrapper struct is left to the interpreter (Issue #6804)"
                    .to_string(),
            ));
        }
        let (idx, field_ty) = self.resolve_struct_field(type_id, field)?;
        self.emit(Instr::GetField(idx));
        Ok(field_ty)
    }

    /// Compile tuple (a, b, c)
    pub(super) fn compile_tuple(
        &mut self,
        elements: &[Expr],
    ) -> Result<ValueType, SpecializationError> {
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
        // MakeRangeLazy/MakeStepRangeLazy expect: start, step, stop on stack. An
        // explicit step (`a:s:b`) yields a `StepRange` even for step 1 (Issue #5667).
        let explicit_step = step.is_some();
        self.compile_expr(start)?;
        if let Some(step_expr) = step {
            self.compile_expr(step_expr)?;
        } else {
            // Default step is 1
            self.emit(Instr::PushI64(1));
        }
        self.compile_expr(stop)?;
        self.emit(if explicit_step {
            Instr::MakeStepRangeLazy
        } else {
            Instr::MakeRangeLazy
        });
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
}

fn dynamic_struct_binary_instr(op: BinaryOp, lt: &ValueType, rt: &ValueType) -> Option<Instr> {
    if !matches!(lt, ValueType::Struct(_)) && !matches!(rt, ValueType::Struct(_)) {
        return None;
    }

    match op {
        BinaryOp::Add => Some(Instr::DynamicAdd),
        BinaryOp::Sub => Some(Instr::DynamicSub),
        BinaryOp::Mul => Some(Instr::DynamicMul),
        BinaryOp::Div => Some(Instr::DynamicDiv),
        BinaryOp::Mod => Some(Instr::DynamicMod),
        BinaryOp::IntDiv => Some(Instr::DynamicIntDiv),
        BinaryOp::Pow => Some(Instr::DynamicPow),
        _ => None,
    }
}

fn dynamic_struct_binary_result_type(op: BinaryOp, lt: &ValueType, rt: &ValueType) -> ValueType {
    match op {
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::IntDiv
        | BinaryOp::Pow => match (lt, rt) {
            (ValueType::Struct(type_id), _) | (_, ValueType::Struct(type_id)) => {
                ValueType::Struct(*type_id)
            }
            _ => ValueType::Any,
        },
        _ => ValueType::Any,
    }
}
