//! Expression specialization.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
// Issue #8192: typed scalar binary-op instruction selection is shared with the
// main compiler so the two codegen paths cannot diverge. See the helper's doc
// comment and `docs/vm/BINARY_DISPATCH.md` ("Two binary-op codegen paths").
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::vm::value::is_array_wrapper_struct_name;
use crate::vm::{ArrayElementType, Instr, ValueType};
use subset_julia_vm_bytecode::typed_scalar_binary_instr;

use super::helpers::expr_variant_name;
use super::{
    specialize_function_with_callees, FunctionSpecializer, SpecializableCallee, SpecializationError,
};

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
/// `k * b.x * dt` as an `Expr::Call { function: "*".into(), args: [k, b.x, dt] }`
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

/// True when both sides are the same variable (`z * z`), so ComplexF64 mul can
/// use the square path instead of evaluating the operand twice.
fn same_complex_f64_var(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Var(l, _), Expr::Var(r, _)) => l == r,
        _ => false,
    }
}

fn unqualified_type_name(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(_, tail)| tail)
}

fn is_range_struct_name(name: &str) -> bool {
    let base = name
        .split('{')
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or(name);
    matches!(
        base,
        "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange" | "OneTo" | "LogRange"
    )
}

fn is_proven_scalar_index_value_type(value_type: &ValueType) -> bool {
    matches!(
        value_type,
        ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I64
            | ValueType::I128
            | ValueType::BigInt
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
    )
}

fn conversion_target_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(name, _) => Some(unqualified_type_name(name)),
        _ => None,
    }
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

/// Recognize `<x> * im` / `im * <x>` in EITHER shape the frontend can produce:
/// an `Expr::BinaryOp { op: Mul, .. }` node, or the n-ary call form
/// `Expr::Call { function: "*", args: [x, im] }` that the parser emits for a
/// chained/variadic `*` (Julia's `*` is variadic — the same reason
/// `binary_op_from_operator_name` exists, Issue #6346).
///
/// Handling only the `BinaryOp` shape made `cr + ci * im` (as written in
/// `mandel_count`) miss the ComplexF64 construction fast path and fall into
/// `emit_binary_op` with an `Add (F64, ComplexF64)` pair, which has no typed
/// instruction — aborting the whole specialization (Issue #10749).
fn imaginary_product_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::BinaryOp {
            op: BinaryOp::Mul,
            left,
            right,
            ..
        } => imaginary_multiplier_expr(left, right),
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if function == "*"
            && args.len() == 2
            && kwargs.is_empty()
            && !splat_mask.iter().any(|s| *s)
            && !kwargs_splat_mask.iter().any(|s| *s) =>
        {
            imaginary_multiplier_expr(&args[0], &args[1])
        }
        _ => None,
    }
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
                splat_mask,
                kwargs_splat_mask,
                ..
            } => self.compile_call(function, args, kwargs, splat_mask, kwargs_splat_mask),
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
            return Err(SpecializationError::Unsupported(format!(
                "free variable `{}` is resolved by the generic bytecode path",
                name
            )));
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
            ValueType::ComplexF64 => {
                if self.is_complex_split(name) {
                    self.emit_load_complex_split(name);
                } else {
                    self.emit(Instr::LoadAny(name.to_string()));
                }
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
        // `im` is only the imaginary unit when it is NOT shadowed by a local
        // (a parameter or assignment named `im`); Julia resolves the local
        // first, exactly as `compile_call` does for a local callable
        // (Issue #10146). Without this guard `x + im * 2` in a function whose
        // parameter is named `im` would silently compile as a Complex literal.
        let im_is_shadowed = self.locals.contains_key("im");
        if matches!(op, BinaryOp::Mul) && !im_is_shadowed {
            if let Some(imag_expr) = imaginary_multiplier_expr(left, right) {
                return self.compile_imaginary_multiple_f64(imag_expr).map(Some);
            }
        }

        if matches!(op, BinaryOp::Add | BinaryOp::Sub) && !im_is_shadowed {
            if let Some((real_expr, imag_expr, imag_sign)) =
                real_plus_imaginary_components(op, left, right)
            {
                return self
                    .compile_complex_from_real_imag_f64(real_expr, imag_expr, imag_sign)
                    .map(Some);
            }
        }

        // `z^2` and `z * z` are the same ComplexF64 square. Mandelbrot-style
        // kernels write both spellings; the square path evaluates the operand
        // once and reuses (re, im) temps instead of the full four-temp binary
        // mul (Issue #10704 untyped broadcast remaining gap vs typed SROA).
        if matches!(op, BinaryOp::Pow)
            && matches!(right, Expr::Literal(Literal::Int(2), _))
            && self
                .infer_literal_type(left)
                .as_ref()
                .is_some_and(is_complex_f64_type)
        {
            return self.compile_complex_square_f64(left).map(Some);
        }
        if matches!(op, BinaryOp::Mul)
            && same_complex_f64_var(left, right)
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

    fn ensure_real_unary_operand(
        function: &str,
        ty: ValueType,
    ) -> Result<ValueType, SpecializationError> {
        if matches!(
            ty,
            ValueType::F64
                | ValueType::F32
                | ValueType::F16
                | ValueType::Bool
                | ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
        ) {
            Ok(unary_float_preserving_result_type(ty))
        } else {
            Err(SpecializationError::Unsupported(format!(
                "{function} on non-real operand {:?} is not specialized",
                ty
            )))
        }
    }

    /// Emit a primitive `Int64` conversion for `ty`, but only when it is
    /// provably *exact and method-free*: an integer type strictly narrower
    /// than `Int64` (widening it can never overflow), `Bool`, or `Char`.
    ///
    /// Every other `ValueType` returns `Err(Unsupported)` instead of emitting
    /// anything, so the caller's whole-function specialization attempt fails
    /// and execution falls back to the generic/dynamic dispatch path — the
    /// same path the direct (non-specialized) call already uses. That
    /// fallback is required because these operands can either:
    ///   - lose precision and must raise `InexactError` when they do
    ///     (`Float64`/`Float32`/`Float16` truncation, `Int128`/`UInt64`/
    ///     `UInt128`/`BigInt`/`BigFloat` overflow — Issue #11198, and the
    ///     `convert(Int64, x)` sibling, Issue #11487), or
    ///   - be a `Struct`/`Any` value that may carry a user-defined
    ///     `Int64(::T)` method, which must take dispatch priority over the
    ///     primitive conversion (Issue #11215).
    ///
    /// Shared by both `Int`/`Int64` constructor calls (`compile_call`) and
    /// `convert(Int64, x)` (`compile_convert_call`) so the two codegen paths
    /// cannot silently diverge on this correctness invariant again.
    fn emit_exact_to_i64(
        &mut self,
        ty: ValueType,
        context: &str,
    ) -> Result<(), SpecializationError> {
        match ty {
            ValueType::I64 => Ok(()),
            ValueType::Bool => {
                self.emit(Instr::BoolToI64);
                Ok(())
            }
            ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::Char => {
                self.emit(Instr::ToI64);
                Ok(())
            }
            other => Err(SpecializationError::Unsupported(format!(
                "{context}(::{:?}) requires a checked conversion or method dispatch",
                other
            ))),
        }
    }

    fn emit_convert_to_i64(&mut self, ty: ValueType) -> Result<(), SpecializationError> {
        self.emit_exact_to_i64(ty, "convert(Int64, _)")
    }

    fn emit_convert_to_f64(&mut self, ty: ValueType) -> Result<(), SpecializationError> {
        match ty {
            ValueType::F64 => Ok(()),
            ValueType::I64 => {
                self.emit(Instr::ToF64);
                Ok(())
            }
            ValueType::Bool => {
                self.emit(Instr::BoolToI64);
                self.emit(Instr::ToF64);
                Ok(())
            }
            ValueType::Any
            | ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::F16
            | ValueType::F32
            | ValueType::BigInt
            | ValueType::BigFloat => {
                self.emit(Instr::DynamicToF64);
                Ok(())
            }
            other => Err(SpecializationError::Unsupported(format!(
                "convert(Float64, ::{:?}) is not specialized",
                other
            ))),
        }
    }

    fn emit_convert_to_bool(&mut self, ty: ValueType) -> Result<(), SpecializationError> {
        match ty {
            ValueType::Bool => Ok(()),
            ValueType::I64 => {
                self.emit(Instr::I64ToBool);
                Ok(())
            }
            ValueType::Any => {
                self.emit(Instr::DynamicToBool);
                Ok(())
            }
            other => Err(SpecializationError::Unsupported(format!(
                "convert(Bool, ::{:?}) is not specialized",
                other
            ))),
        }
    }

    fn compile_convert_call(&mut self, args: &[Expr]) -> Result<ValueType, SpecializationError> {
        if args.len() != 2 {
            return Err(SpecializationError::Unsupported(
                "convert requires 2 arguments".to_string(),
            ));
        }
        let Some(target_name) = conversion_target_name(&args[0]) else {
            return Err(SpecializationError::Unsupported(
                "convert target is not a simple type object".to_string(),
            ));
        };

        let ty = self.compile_expr(&args[1])?;
        match target_name {
            "Any" => Ok(ty),
            "Int" | "Int64" => {
                self.emit_convert_to_i64(ty)?;
                Ok(ValueType::I64)
            }
            "Float64" => {
                self.emit_convert_to_f64(ty)?;
                Ok(ValueType::F64)
            }
            "Bool" => {
                self.emit_convert_to_bool(ty)?;
                Ok(ValueType::Bool)
            }
            "Complex" => match ty {
                ValueType::ComplexF64 | ValueType::ComplexF32 => Ok(ty),
                other => Err(SpecializationError::Unsupported(format!(
                    "convert({target_name}, ::{:?}) is not specialized",
                    other
                ))),
            },
            "ComplexF64" | "Complex{Float64}" => match ty {
                ValueType::ComplexF64 => Ok(ValueType::ComplexF64),
                other => Err(SpecializationError::Unsupported(format!(
                    "convert({target_name}, ::{:?}) is not specialized",
                    other
                ))),
            },
            "ComplexF32" | "Complex{Float32}" => match ty {
                ValueType::ComplexF32 => Ok(ValueType::ComplexF32),
                other => Err(SpecializationError::Unsupported(format!(
                    "convert({target_name}, ::{:?}) is not specialized",
                    other
                ))),
            },
            _ => Err(SpecializationError::Unsupported(format!(
                "convert target '{}' not yet supported for specialization",
                target_name
            ))),
        }
    }

    fn emit_load_complex_split(&mut self, name: &str) {
        let (re, im) = self
            .complex_splits
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.cx_slot_names(name));
        self.emit(Instr::LoadF64(re));
        self.emit(Instr::LoadF64(im));
    }

    pub(super) fn emit_store_complex_split(&mut self, name: &str) {
        let (re, im) = self
            .complex_splits
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.cx_slot_names(name));
        self.emit(Instr::StoreF64(im));
        self.emit(Instr::StoreF64(re));
    }

    pub(super) fn emit_materialize_complex_f64(&mut self) {
        // stack: [re, im]
        self.emit(Instr::NewParametricStruct("Complex".to_string(), 2));
    }

    fn compile_imaginary_multiple_f64(
        &mut self,
        imag_expr: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        self.emit(Instr::PushF64(0.0));
        self.compile_numeric_as_f64(imag_expr)?;
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
        // The expression may have produced an unboxed [re, im] pair; materialize
        // it into a boxed ComplexF64 before storing into the generic Any slot.
        self.emit_materialize_complex_f64();
        self.emit(Instr::StoreAny(name.to_string()));
        Ok(())
    }

    fn load_complex_f64_temp_field(&mut self, name: &str, field: usize) {
        self.emit(Instr::LoadAny(name.to_string()));
        self.emit(Instr::GetField(field));
    }

    fn compile_complex_abs2_f64(&mut self, expr: &Expr) -> Result<ValueType, SpecializationError> {
        if let Expr::Var(name, _) = expr {
            if self.is_complex_split(name)
                || self
                    .locals
                    .get(name.as_str())
                    .is_some_and(|ty| *ty == ValueType::ComplexF64)
            {
                // Split-local path: load each field ONCE (not via
                // `emit_load_complex_split`, which pushes both at once and
                // would force a spill to reorder them) and square it in
                // place — `LoadF64(re); DupF64; MulF64` immediately followed
                // by the same shape for `im` is exactly the adjacent window
                // the shared Instr-level peephole
                // (`subset_julia_vm_bytecode::peephole`) fuses into
                // `LoadSquareF64Slot`, and the typed-loop predecoder's
                // `fuse_typed_loop_ops` then fuses the two
                // `LoadSquareF64Slot`s + `AddF64` into ONE
                // `PushSumSquaresF64Slots` op — matching the static
                // compiler's SROA'd `abs2` shape exactly (Issue #10799).
                // Each field is read exactly once here, so this cannot
                // reintroduce the double-counted-`im` bug fixed by Issue
                // #10567 (that bug came from reusing one loaded value for
                // both squares instead of loading each field separately).
                let (re, im) = self.cx_slot_names(name.as_str());
                self.emit(Instr::LoadF64(re));
                self.emit(Instr::DupF64);
                self.emit(Instr::MulF64);
                self.emit(Instr::LoadF64(im));
                self.emit(Instr::DupF64);
                self.emit(Instr::MulF64);
                self.emit(Instr::AddF64);
                return Ok(ValueType::F64);
            }
        }

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
        // Issue #10799: when the operand is a bare split-local var (the
        // overwhelmingly common `z*z`/`z^2` shape in Mandelbrot-style
        // recurrences), read its (re, im) fields directly via their
        // PERSISTENT slot names instead of compiling the operand expression
        // and spilling the result to fresh temp names. This lets the shared
        // Instr-level peephole and `fuse_typed_loop_ops` (both unchanged by
        // this Issue) fuse the resulting op window into
        // `PushDiffSquaresF64Slots`/`PushMulF64Slots` superinstructions, the
        // same ones the static compiler's SROA'd `z*z` gets. This path does
        // NOT emit the `AddF64SlotStore`/`CopyF64Slots` shape
        // `fuse_complex_mul_add_assign` requires (that fusion stays specific
        // to the static compiler's temp-then-copy-back codegen; verified via
        // codex review this path never reaches it), so it does not further
        // collapse into a single `ComplexMulAddAssign` — only the smaller
        // per-multiply/per-square fusions apply. This is purely an
        // op-count/fusion optimization — the arithmetic is unchanged, so any
        // non-bare-var operand (an
        // arbitrary sub-expression) keeps using the general path below.
        if let Expr::Var(name, _) = expr {
            if self.is_complex_split(name)
                || self
                    .locals
                    .get(name.as_str())
                    .is_some_and(|ty| *ty == ValueType::ComplexF64)
            {
                let (re, im) = self.cx_slot_names(name.as_str());

                // real = re*re - im*im
                self.emit(Instr::LoadF64(re.clone()));
                self.emit(Instr::DupF64);
                self.emit(Instr::MulF64);
                self.emit(Instr::LoadF64(im.clone()));
                self.emit(Instr::DupF64);
                self.emit(Instr::MulF64);
                self.emit(Instr::SubF64);

                // imag = re*im + im*re (NOT `2*re*im`: the doubled-multiply
                // form matches the two-`LoadF64Slot`-pair adjacency
                // `fuse_typed_loop_ops`'s `PushMulF64Slots` window expects;
                // a constant-2.0 scale does not fuse at all).
                self.emit(Instr::LoadF64(re.clone()));
                self.emit(Instr::LoadF64(im.clone()));
                self.emit(Instr::MulF64);
                self.emit(Instr::LoadF64(im));
                self.emit(Instr::LoadF64(re));
                self.emit(Instr::MulF64);
                self.emit(Instr::AddF64);

                return Ok(ValueType::ComplexF64);
            }
        }

        let ty = self.compile_expr(expr)?;
        if !is_complex_f64_type(&ty) {
            return Err(SpecializationError::Unsupported(format!(
                "ComplexF64 fast path expected ComplexF64, got {:?}",
                ty
            )));
        }
        // Stack: [re, im].  Spill both parts to temps so we can use each twice.
        let re_temp = self.temp_name("cx_re");
        let im_temp = self.temp_name("cx_im");
        self.emit(Instr::StoreF64(im_temp.clone()));
        self.emit(Instr::StoreF64(re_temp.clone()));

        // real = re*re - im*im
        self.emit(Instr::LoadF64(re_temp.clone()));
        self.emit(Instr::DupF64);
        self.emit(Instr::MulF64);
        self.emit(Instr::LoadF64(im_temp.clone()));
        self.emit(Instr::DupF64);
        self.emit(Instr::MulF64);
        self.emit(Instr::SubF64);

        // imag = 2*re*im
        self.emit(Instr::PushF64(2.0));
        self.emit(Instr::LoadF64(re_temp.clone()));
        self.emit(Instr::LoadF64(im_temp.clone()));
        self.emit(Instr::MulF64);
        self.emit(Instr::MulF64);

        Ok(ValueType::ComplexF64)
    }

    /// If `expr` is a bare local var already known to hold a split-slot
    /// `ComplexF64` (or typed `ComplexF64`), return its persistent `(re, im)`
    /// slot names. Used to skip compiling+spilling an operand that already
    /// lives in addressable, reusable slots (Issue #10799).
    fn complex_var_slot_names(&self, expr: &Expr) -> Option<(String, String)> {
        let Expr::Var(name, _) = expr else {
            return None;
        };
        if self.is_complex_split(name)
            || self
                .locals
                .get(name.as_str())
                .is_some_and(|ty| *ty == ValueType::ComplexF64)
        {
            Some(self.cx_slot_names(name.as_str()))
        } else {
            None
        }
    }

    fn compile_complex_binary_f64(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<ValueType, SpecializationError> {
        // Issue #10799: `z*z + c`-shaped recurrences call this with `right`
        // a bare split-local var (`c`) far more often than with an arbitrary
        // sub-expression. When it is, skip compiling+spilling `right`
        // entirely (its fields are already addressable by persistent slot
        // name) and only spill LEFT's imag part — needed because the
        // arithmetic must read the OLD `l_re` (TOS after popping `l_im`)
        // before combining, then reload `l_im` for the second component.
        // This drops the operand setup from 4 spilled temps (+2 ops to
        // compile `right`) to 1.
        if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            if let Some((c_re, c_im)) = self.complex_var_slot_names(right) {
                let l_ty = self.compile_expr(left)?;
                if !is_complex_f64_type(&l_ty) {
                    return Err(SpecializationError::Unsupported(format!(
                        "ComplexF64 binary fast path expected ComplexF64 left operand, got {:?}",
                        l_ty
                    )));
                }
                // Stack: [l_re, l_im].
                let lim = self.temp_name("cx_lim");
                self.emit(Instr::StoreF64(lim.clone()));
                // Stack: [l_re]. real = l_re +/- c_re.
                self.emit(Instr::LoadF64(c_re));
                if matches!(op, BinaryOp::Add) {
                    self.emit(Instr::AddF64);
                } else {
                    self.emit(Instr::SubF64);
                }
                // imag = l_im +/- c_im.
                self.emit(Instr::LoadF64(lim));
                self.emit(Instr::LoadF64(c_im));
                if matches!(op, BinaryOp::Add) {
                    self.emit(Instr::AddF64);
                } else {
                    self.emit(Instr::SubF64);
                }
                return Ok(ValueType::ComplexF64);
            }
        }

        let l_ty = self.compile_expr(left)?;
        if !is_complex_f64_type(&l_ty) {
            return Err(SpecializationError::Unsupported(format!(
                "ComplexF64 binary fast path expected ComplexF64 left operand, got {:?}",
                l_ty
            )));
        }
        let r_ty = self.compile_expr(right)?;
        if !is_complex_f64_type(&r_ty) {
            return Err(SpecializationError::Unsupported(format!(
                "ComplexF64 binary fast path expected ComplexF64 right operand, got {:?}",
                r_ty
            )));
        }
        // Stack: [l_re, l_im, r_re, r_im].  Spill all four parts to temps.
        let lre = self.temp_name("cx_lre");
        let lim = self.temp_name("cx_lim");
        let rre = self.temp_name("cx_rre");
        let rim = self.temp_name("cx_rim");
        self.emit(Instr::StoreF64(rim.clone()));
        self.emit(Instr::StoreF64(rre.clone()));
        self.emit(Instr::StoreF64(lim.clone()));
        self.emit(Instr::StoreF64(lre.clone()));

        match op {
            BinaryOp::Add | BinaryOp::Sub => {
                // real = l_re +/- r_re
                self.emit(Instr::LoadF64(lre.clone()));
                self.emit(Instr::LoadF64(rre.clone()));
                if matches!(op, BinaryOp::Add) {
                    self.emit(Instr::AddF64);
                } else {
                    self.emit(Instr::SubF64);
                }
                // imag = l_im +/- r_im
                self.emit(Instr::LoadF64(lim.clone()));
                self.emit(Instr::LoadF64(rim.clone()));
                if matches!(op, BinaryOp::Add) {
                    self.emit(Instr::AddF64);
                } else {
                    self.emit(Instr::SubF64);
                }
            }
            BinaryOp::Mul => {
                // real = l_re*r_re - l_im*r_im
                self.emit(Instr::LoadF64(lre.clone()));
                self.emit(Instr::LoadF64(rre.clone()));
                self.emit(Instr::MulF64);
                self.emit(Instr::LoadF64(lim.clone()));
                self.emit(Instr::LoadF64(rim.clone()));
                self.emit(Instr::MulF64);
                self.emit(Instr::SubF64);

                // imag = l_re*r_im + l_im*r_re
                self.emit(Instr::LoadF64(lre.clone()));
                self.emit(Instr::LoadF64(rim.clone()));
                self.emit(Instr::MulF64);
                self.emit(Instr::LoadF64(lim.clone()));
                self.emit(Instr::LoadF64(rre.clone()));
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

        Ok(ValueType::ComplexF64)
    }

    /// Emit Complex arithmetic for operands ALREADY on the stack, in the
    /// split-slot representation (a `ComplexF64` value occupies two stack
    /// entries, `[re, im]`) — Issue #10749.
    ///
    /// The real/complex mixed forms deliberately mirror upstream's NARROW
    /// methods from `base/complex.jl` (`*(x::Real, z::Complex) =
    /// Complex(x*real(z), x*imag(z))`), NOT the full complex product with a
    /// zero imaginary part. The two differ on non-finite operands — e.g.
    /// `2.0 * (Inf + 1.0im)` is `Inf + 2.0im` under the narrow method but
    /// `Inf + NaN*im` under the full formula (the `0 * Inf` cross-term) — so
    /// using the full formula here would silently change results relative to
    /// the generic (pure-Julia) path this specialization replaces.
    ///
    /// Only `+`, `-`, `*` are handled. `/` is deliberately NOT: upstream uses
    /// a scaled (Smith's-algorithm) complex division whose rounding this
    /// naive formula would not reproduce bit-for-bit. Anything else returns
    /// `Unsupported` and falls back to generic dispatch.
    fn emit_complex_mixed_binary_op(
        &mut self,
        op: BinaryOp,
        lt: &ValueType,
        rt: &ValueType,
    ) -> Result<ValueType, SpecializationError> {
        let unsupported = |op| {
            SpecializationError::Unsupported(format!("unsupported Complex binary op: {:?}", op))
        };
        match (lt, rt) {
            (ValueType::ComplexF64, ValueType::ComplexF64) => {
                // Stack: [l_re, l_im, r_re, r_im]
                let lre = self.temp_name("cxm_lre");
                let lim = self.temp_name("cxm_lim");
                let rre = self.temp_name("cxm_rre");
                let rim = self.temp_name("cxm_rim");
                self.emit(Instr::StoreF64(rim.clone()));
                self.emit(Instr::StoreF64(rre.clone()));
                self.emit(Instr::StoreF64(lim.clone()));
                self.emit(Instr::StoreF64(lre.clone()));
                match op {
                    BinaryOp::Add | BinaryOp::Sub => {
                        let add = matches!(op, BinaryOp::Add);
                        self.emit(Instr::LoadF64(lre));
                        self.emit(Instr::LoadF64(rre));
                        self.emit(if add { Instr::AddF64 } else { Instr::SubF64 });
                        self.emit(Instr::LoadF64(lim));
                        self.emit(Instr::LoadF64(rim));
                        self.emit(if add { Instr::AddF64 } else { Instr::SubF64 });
                    }
                    BinaryOp::Mul => {
                        // real = l_re*r_re - l_im*r_im
                        self.emit(Instr::LoadF64(lre.clone()));
                        self.emit(Instr::LoadF64(rre.clone()));
                        self.emit(Instr::MulF64);
                        self.emit(Instr::LoadF64(lim.clone()));
                        self.emit(Instr::LoadF64(rim.clone()));
                        self.emit(Instr::MulF64);
                        self.emit(Instr::SubF64);
                        // imag = l_re*r_im + l_im*r_re
                        self.emit(Instr::LoadF64(lre));
                        self.emit(Instr::LoadF64(rim));
                        self.emit(Instr::MulF64);
                        self.emit(Instr::LoadF64(lim));
                        self.emit(Instr::LoadF64(rre));
                        self.emit(Instr::MulF64);
                        self.emit(Instr::AddF64);
                    }
                    _ => return Err(unsupported(op)),
                }
                Ok(ValueType::ComplexF64)
            }
            (ValueType::ComplexF64, ValueType::F64 | ValueType::I64) => {
                // Stack: [l_re, l_im, x]
                if *rt == ValueType::I64 {
                    self.emit(Instr::ToF64);
                }
                let x = self.temp_name("cxm_x");
                let lre = self.temp_name("cxm_lre");
                let lim = self.temp_name("cxm_lim");
                self.emit(Instr::StoreF64(x.clone()));
                self.emit(Instr::StoreF64(lim.clone()));
                self.emit(Instr::StoreF64(lre.clone()));
                match op {
                    // +(z, x) = Complex(real(z) + x, imag(z))
                    // -(z, x) = Complex(real(z) - x, imag(z))
                    BinaryOp::Add | BinaryOp::Sub => {
                        self.emit(Instr::LoadF64(lre));
                        self.emit(Instr::LoadF64(x));
                        self.emit(if matches!(op, BinaryOp::Add) {
                            Instr::AddF64
                        } else {
                            Instr::SubF64
                        });
                        self.emit(Instr::LoadF64(lim));
                    }
                    // *(z, x) = Complex(real(z)*x, imag(z)*x)
                    BinaryOp::Mul => {
                        self.emit(Instr::LoadF64(lre));
                        self.emit(Instr::LoadF64(x.clone()));
                        self.emit(Instr::MulF64);
                        self.emit(Instr::LoadF64(lim));
                        self.emit(Instr::LoadF64(x));
                        self.emit(Instr::MulF64);
                    }
                    _ => return Err(unsupported(op)),
                }
                Ok(ValueType::ComplexF64)
            }
            (ValueType::F64 | ValueType::I64, ValueType::ComplexF64) => {
                // Stack: [x, r_re, r_im] — the real operand is UNDER the pair,
                // so spill the pair first, then widen/spill `x`.
                let rre = self.temp_name("cxm_rre");
                let rim = self.temp_name("cxm_rim");
                let x = self.temp_name("cxm_x");
                self.emit(Instr::StoreF64(rim.clone()));
                self.emit(Instr::StoreF64(rre.clone()));
                if *lt == ValueType::I64 {
                    self.emit(Instr::ToF64);
                }
                self.emit(Instr::StoreF64(x.clone()));
                match op {
                    // +(x, z) = Complex(x + real(z), imag(z))
                    BinaryOp::Add => {
                        self.emit(Instr::LoadF64(x));
                        self.emit(Instr::LoadF64(rre));
                        self.emit(Instr::AddF64);
                        self.emit(Instr::LoadF64(rim));
                    }
                    // -(x, z) = Complex(x - real(z), -imag(z))
                    BinaryOp::Sub => {
                        self.emit(Instr::LoadF64(x));
                        self.emit(Instr::LoadF64(rre));
                        self.emit(Instr::SubF64);
                        self.emit(Instr::LoadF64(rim));
                        self.emit(Instr::NegF64);
                    }
                    // *(x, z) = Complex(x*real(z), x*imag(z))
                    BinaryOp::Mul => {
                        self.emit(Instr::LoadF64(x.clone()));
                        self.emit(Instr::LoadF64(rre));
                        self.emit(Instr::MulF64);
                        self.emit(Instr::LoadF64(x));
                        self.emit(Instr::LoadF64(rim));
                        self.emit(Instr::MulF64);
                    }
                    _ => return Err(unsupported(op)),
                }
                Ok(ValueType::ComplexF64)
            }
            _ => Err(unsupported(op)),
        }
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
            // Complex arithmetic on the split-slot (SROA) representation
            // (Issue #10749). `try_compile_complex_binary_op` only fires when
            // it can pattern-match the *syntax* of both operands, so a Complex
            // operand that arrives here as a plain value — most importantly a
            // LICM-hoisted loop-invariant temp such as
            // `__sjulia_licm_0 = ci * im` in `mandel_count` — previously
            // aborted the whole specialization. Handle the value-level cases.
            (ValueType::ComplexF64, ValueType::ComplexF64)
            | (ValueType::ComplexF64, ValueType::F64 | ValueType::I64)
            | (ValueType::F64 | ValueType::I64, ValueType::ComplexF64)
                if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) =>
            {
                return self.emit_complex_mixed_binary_op(op, &lt, &rt);
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
        // the shared `bytecode::typed_scalar_binary_instr` table the main compiler also
        // uses, so the two binary-op codegen paths cannot drift apart. Ops
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
                return Ok(result_ty);
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
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> Result<ValueType, SpecializationError> {
        // Julia resolves the callee through local scope before considering
        // global methods or builtin constructors. Runtime specialization must
        // preserve that order: a parameter named `Float64` is a callable value
        // in `Float64(2)`, not the builtin Float64 constructor (Issue #10146).
        if self.locals.contains_key(function) {
            if !kwargs.is_empty()
                || splat_mask.iter().any(|is_splat| *is_splat)
                || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
            {
                return Err(SpecializationError::Unsupported(
                    "local callable splat/keyword calls are not specialized".to_string(),
                ));
            }
            for arg in args {
                let ty = self.compile_expr(arg)?;
                // A split ComplexF64 pair must be boxed before escaping to a
                // callee that expects a single boxed value (Issue #10567).
                if ty == ValueType::ComplexF64 {
                    self.emit_materialize_complex_f64();
                }
            }
            self.compile_var(function)?;
            self.emit(Instr::CallFunctionVariable(args.len()));
            return Ok(ValueType::Any);
        }

        // Handle known built-in math functions
        match function {
            "convert" => return self.compile_convert_call(args),
            // Direct Complex{Float64} construction (Issue #9654): the SROA
            // value-position materialization rewrites provably-ComplexF64
            // expressions (`0.0 + 0.0im`, `cr + ci*im`, …) into explicit
            // `Complex{Float64}(re, im)` constructor calls, so specialized
            // bodies must compile them (previously `Unsupported`, which failed
            // the whole specialization and dropped Complex-annotated functions
            // to the dynamic path). Non-numeric args still bail to the generic
            // fallback via `compile_numeric_as_f64`.
            // NOTE: no `Complex64` here — upstream removed that alias in 0.7
            // (`Complex64` meant Complex{Float32}); it is `UndefVarError` in
            // both julia and sjulia user code.
            "Complex{Float64}" | "ComplexF64" => {
                if args.len() == 2 {
                    return self.compile_complex_from_real_imag_f64(&args[0], &args[1], 1.0);
                }
                if args.len() == 1 {
                    // Complex{Float64}(x) == Complex{Float64}(x, 0.0)
                    self.compile_numeric_as_f64(&args[0])?;
                    self.emit(Instr::PushF64(0.0));
                    self.emit(Instr::NewParametricStruct("Complex".to_string(), 2));
                    return Ok(ValueType::ComplexF64);
                }
                return Err(SpecializationError::Unsupported(
                    "Complex{Float64} constructor arity not specialized".to_string(),
                ));
            }
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
                // `SqrtF64` is the *real* sqrt: a Complex (or otherwise
                // non-real) operand must fall back to generic dispatch (the
                // pure-Julia `sqrt(z::Complex{Float64})` method). Reachable
                // since the Complex{Float64} constructor arm above made
                // Complex-typed operands compilable (Issue #9654): the
                // `sqrt(Complex{Float64}(z.re, z.im))` promotion wrapper in
                // base/complex.jl previously failed specialization at the
                // ctor, never reaching this blind `SqrtF64` emit.
                let result_ty = Self::ensure_real_unary_operand("sqrt", ty)?;
                self.emit(Instr::SqrtF64);
                return Ok(result_ty);
            }
            // Note: abs is now Pure Julia - no specialization needed here
            "floor" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "floor requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                let result_ty = Self::ensure_real_unary_operand("floor", ty)?;
                self.emit(Instr::FloorF64);
                return Ok(result_ty);
            }
            "ceil" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "ceil requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                let result_ty = Self::ensure_real_unary_operand("ceil", ty)?;
                self.emit(Instr::CeilF64);
                return Ok(result_ty);
            }
            "round" => {
                if args.len() != 1 {
                    return Err(SpecializationError::Unsupported(
                        "round requires 1 argument".to_string(),
                    ));
                }
                let ty = self.compile_expr(&args[0])?;
                let result_ty = Self::ensure_real_unary_operand("round", ty)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Round, 1));
                return Ok(result_ty);
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
                // Issues #11198/#11215: don't silently truncate a lossy
                // source or bypass a user-defined `Int64(::T)` method — see
                // `emit_exact_to_i64` for the full invariant this enforces.
                self.emit_exact_to_i64(ty, "Int64")?;
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
            if args.len() == 2 {
                // Exactly 2 args: delegate to `compile_binary_op` so an
                // operator written in this n-ary `Call` shape gets the SAME
                // fast paths as an `Expr::BinaryOp` node — notably the
                // real+imaginary `Complex{Float64}` construction pattern
                // (`cr + ci*im`), which only matches a literal `Mul` shape
                // and was unreachable through this entry point before (Issue
                // #10749: newly exercised once cross-function specialization
                // stopped bailing out before reaching code shaped like this).
                return self.compile_binary_op(op, &args[0], &args[1]);
            }
            if args.len() > 2 {
                let mut acc_ty = self.compile_expr(&args[0])?;
                for arg in &args[1..] {
                    let rhs_ty = self.compile_expr(arg)?;
                    acc_ty = self.emit_binary_op(op, acc_ty, rhs_ty)?;
                }
                return Ok(acc_ty);
            }
        }

        // A call to another user-defined function that is itself eligible for
        // runtime specialization (Issue #10749). `self.callable_registry` maps
        // a bare (or module-qualified) name to the callee's own
        // `spec_func_index` when that name resolves to exactly one method —
        // ambiguous names (multiple dispatch on the same bare name) are never
        // registered, so they fall through to the `Unsupported` below exactly
        // as before this issue.
        //
        // Emitting `Instr::CallSpecialize` here is deliberate reuse: it is the
        // SAME instruction (and the SAME runtime path,
        // `execute_call_specialize_with_args`) the main compiler emits for an
        // ordinary call site to this callee, so calling it from inside a
        // runtime-specialized body needs no new execution machinery — only
        // the compile-time recognition below.
        if let Some(callee) = self.resolve_callable_callee(function) {
            if !kwargs.is_empty()
                || splat_mask.iter().any(|is_splat| *is_splat)
                || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
            {
                return Err(SpecializationError::Unsupported(
                    "specialized calls to user functions do not support kwargs/splat yet"
                        .to_string(),
                ));
            }
            let min_args = callee
                .param_count
                .saturating_sub(usize::from(callee.has_vararg));
            let arity_ok = if callee.has_vararg {
                args.len() >= min_args
            } else {
                args.len() == callee.param_count
            };
            if !arity_ok {
                return Err(SpecializationError::Unsupported(format!(
                    "call to '{}' does not match its specializable arity",
                    function
                )));
            }

            let mut arg_types = Vec::with_capacity(args.len());
            for arg in args {
                let ty = self.compile_expr(arg)?;
                // A split ComplexF64 pair must be boxed before escaping to a
                // callee frame that expects a single boxed value, same as the
                // local-callable path above (Issue #10567).
                if ty == ValueType::ComplexF64 {
                    self.emit_materialize_complex_f64();
                }
                arg_types.push(ty);
            }

            // Do NOT guess the callee's return type: only trust it once a
            // (bounded, cycle-safe) nested specialization attempt against the
            // callee's own IR, with these EXACT argument types, has actually
            // succeeded. A wrong guess here would let downstream instructions
            // in THIS function assume the wrong type — silent data
            // corruption is worse than a missed optimization.
            let return_type = self.infer_callee_return_type(&callee, &arg_types)?;
            self.emit(Instr::CallSpecialize(callee.spec_func_index, args.len()));
            if return_type == ValueType::ComplexF64 {
                // `CallSpecialize` always yields a single boxed value on the
                // stack (`emit_return` materializes any ComplexF64 result
                // before returning), but every OTHER ComplexF64-producing
                // expression path in this file leaves a split `[re, im]` F64
                // pair instead (Issue #10567's SROA convention — see
                // `emit_store`'s `ComplexF64` arm). Unbox here so this call
                // site's result matches that convention, the same way the
                // parameter-hoisting preamble in `specialize_function` unboxes
                // a ComplexF64 parameter at function entry. Skipping this
                // would leave a single boxed value where every consumer
                // (`emit_store`, `emit_binary_op`'s complex paths, …) expects
                // two F64s — silent stack corruption, not just a missed
                // optimization.
                //
                // Use a FRESH temp name per call site (`temp_name` keys on the
                // current code length) rather than one shared constant, so two
                // Complex-returning calls can never alias each other's spill
                // slot (codex review of Issue #10749).
                let result_temp = self.temp_name("cx_call_result");
                self.emit(Instr::StoreAny(result_temp.clone()));
                self.emit(Instr::LoadAny(result_temp.clone()));
                self.emit(Instr::GetField(0));
                self.emit(Instr::LoadAny(result_temp));
                self.emit(Instr::GetField(1));
            }
            return Ok(return_type);
        }

        // User-defined functions or unknown built-ins
        Err(SpecializationError::Unsupported(format!(
            "Function call '{}' not yet supported for specialization",
            function
        )))
    }

    /// Resolve `function` (as written at the call site) against
    /// `self.callable_registry`.
    ///
    /// Resolution order is MODULE-QUALIFIED FIRST, then the bare name: inside
    /// `module M`, a bare call to `f` means `M.f` whenever `M` defines one,
    /// even if a same-named top-level `f` also exists. Trying the bare name
    /// first would bind the *global* `f` and silently specialize the call to
    /// the wrong function (codex review of Issue #10749).
    ///
    /// A name bound to a LOCAL is never resolved here — `compile_call`'s
    /// local-callable path (Issue #10146) returns before reaching this — but
    /// the check is repeated defensively so this helper is safe to call from
    /// anywhere.
    ///
    /// Returns an owned clone (cheap: the only heap field is an `Arc`) so the
    /// caller can immediately follow up with `&mut self` calls like
    /// `compile_expr` without fighting the borrow checker.
    fn resolve_callable_callee(&self, function: &str) -> Option<SpecializableCallee> {
        if self.locals.contains_key(function) {
            return None;
        }
        if let Some(module_path) = self.current_module_path.as_ref() {
            let qualified = format!("{}.{}", module_path, function);
            if let Some(callee) = self.callable_registry.get(&qualified) {
                return Some(callee.clone());
            }
        }
        self.callable_registry.get(function).cloned()
    }

    /// Infer a callee's return type for this exact call site's concrete
    /// argument types via a nested, bounded, cycle-safe `specialize_function`
    /// call (Issue #10749). Returns `Unsupported` (never a guess) when the
    /// nested attempt fails — including when it is rejected outright by the
    /// shared recursion/depth guard for a direct or mutual recursive cycle.
    ///
    /// Also rejects a callee whose return sites do NOT all agree on one type
    /// (`return_type_consistent == false`): the reported `return_type` is
    /// last-write-wins, so for a body like `x > 0 ? x * 2 : "neg"` it names
    /// only one of the two possible result types. Trusting it would let this
    /// caller type its downstream instructions for a value the callee does not
    /// always produce (codex review of Issue #10749).
    fn infer_callee_return_type(
        &self,
        callee: &SpecializableCallee,
        arg_types: &[ValueType],
    ) -> Result<ValueType, SpecializationError> {
        let result = specialize_function_with_callees(
            &callee.ir,
            arg_types,
            self.struct_defs,
            self.type_object_names,
            callee.module_path.as_deref(),
            self.disable_array_index_fast_path,
            self.disable_field_access,
            self.callable_registry,
            self.recursion_guard,
            Some(callee.spec_func_index),
        )
        .map_err(|_| {
            SpecializationError::Unsupported(
                "callee return type not resolvable for specialization".to_string(),
            )
        })?;
        if !result.return_type_consistent {
            return Err(SpecializationError::Unsupported(
                "callee has multiple return types; not specialized (Issue #10749)".to_string(),
            ));
        }
        Ok(result.return_type)
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
                let result_ty = Self::ensure_real_unary_operand("sqrt", ty)?;
                self.emit(Instr::SqrtF64);
                Ok(result_ty)
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
                self.locals.get(name.as_str()).cloned()
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
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                // N-ary `+` / `-` / `*` folded from the parser.
                if let Some(op) = binary_op_from_operator_name(function) {
                    if args.len() >= 2
                        && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
                        && kwargs.is_empty()
                    {
                        let mut all_complex_f64 = true;
                        for arg in args {
                            if !self
                                .infer_literal_type(arg)
                                .is_some_and(|t| t == ValueType::ComplexF64)
                            {
                                all_complex_f64 = false;
                                break;
                            }
                        }
                        if all_complex_f64 {
                            return Some(ValueType::ComplexF64);
                        }
                    }
                }
                // `abs2(z::ComplexF64)` is common in escape-test kernels.
                if function == "abs2"
                    && args.len() == 1
                    && kwargs.is_empty()
                    && self
                        .infer_literal_type(&args[0])
                        .as_ref()
                        .is_some_and(is_complex_f64_type)
                {
                    return Some(ValueType::F64);
                }
                None
            }
            Expr::UnaryOp { operand, .. } => self.infer_literal_type(operand),
            _ => None,
        }
    }

    /// Compile array indexing arr[i] or arr[i, j]
    /// Typed-array-literal build for `T[a, b, ...]`, mirroring the main
    /// compiler's `getindex` literal arm instruction-for-instruction
    /// (`NewMemory` → per-element `PushI64`/value/`MemorySet` → `FinalizeArray`,
    /// with the same per-element `convert` routing) so the two codegen paths
    /// cannot diverge (Issue #10746).
    pub(super) fn compile_typed_array_literal(
        &mut self,
        target: &Expr,
        element_type: ArrayElementType,
        values: &[Expr],
    ) -> Result<ValueType, SpecializationError> {
        self.compile_expr(target)?;
        let target_temp = self.temp_name("typed_literal_target");
        self.locals.insert(target_temp.clone(), ValueType::DataType);
        self.emit(Instr::StoreAny(target_temp.clone()));
        self.emit(Instr::NewMemory(element_type.clone(), values.len()));
        for (index, value) in values.iter().enumerate() {
            self.emit(Instr::PushI64((index + 1) as i64));
            self.emit(Instr::LoadAny(target_temp.clone()));
            self.compile_expr(value)?;
            self.emit(Instr::CallBuiltin(BuiltinId::Convert, 2));
            self.emit(Instr::MemorySet);
        }
        self.emit(Instr::FinalizeArray(vec![values.len()]));
        Ok(ValueType::ArrayOf(element_type, None))
    }

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

        // Issues #10566/#10746: `T[a, b]` (a type-object-prefixed *array
        // literal*, e.g. `Any[x]`) parses as an `Expr::Index` whose receiver is
        // a type object, not as an index into a collection. Emitting a scalar
        // `IndexLoad` against the `DataType` receiver produced `MethodError: no
        // method matching getindex(DataType)` at runtime. For bare-identifier
        // element types the specializer now emits the same literal build as the
        // main compiler's `getindex` arm (shared element-type map, Issue
        // #10746); receivers the shared map does not cover (user structs,
        // parametric spellings) still refuse the whole specialization so the
        // function runs its generic body, which builds the literal correctly.
        if let Expr::Var(name, _) = array {
            if self.resolve_type_object_name(name).is_some() {
                let literal_indices_only = !indices
                    .iter()
                    .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }));
                if literal_indices_only {
                    if let Some(element_type) =
                        subset_julia_vm_bytecode::bare_type_name_array_element_type(name)
                    {
                        return self.compile_typed_array_literal(array, element_type, indices);
                    }
                }
                return Err(SpecializationError::Unsupported(format!(
                    "`{}[...]` is a typed array literal with an element type the \
                     specializer's literal build does not cover (Issues #10566/#10746)",
                    name
                )));
            }
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
            let idx_type = self.infer_literal_type(idx);
            let is_range_struct = match &idx_type {
                Some(ValueType::Struct(type_id)) => self
                    .struct_defs
                    .get(*type_id)
                    .is_some_and(|def| is_range_struct_name(&def.name)),
                _ => false,
            };
            matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. })
                || is_range_struct
                || matches!(
                    idx_type,
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

        // A lossy runtime-specialization tag such as `Struct(type_id)` is not
        // authoritative enough to prove scalar cardinality: the id may belong
        // to the runtime value's local type namespace rather than this
        // specializer's `struct_defs`. Such a value can be a CartesianIndex or
        // an AbstractRange. Keep the result dynamic unless every index is a
        // proven integer scalar, so a gathered array cannot flow into a typed
        // `ReturnI64`/`ReturnF64` continuation (Issue #10970).
        let indices_are_proven_scalar = indices.iter().all(|index| {
            self.infer_literal_type(index)
                .as_ref()
                .is_some_and(is_proven_scalar_index_value_type)
        });
        if !indices_are_proven_scalar {
            return Ok(ValueType::Any);
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
        let Some(elem_types) = self.tuple_element_types.get(name.as_str()).cloned() else {
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
            Expr::Var(name, _) => self.locals.get(name.as_str()).cloned() == Some(ValueType::Any),
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
