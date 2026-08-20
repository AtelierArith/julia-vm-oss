use std::collections::HashMap;

use crate::aot::ir::{AotBuiltinOp, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::unsupported;
use super::ops::get;
use super::locals::MathLocals;

const LN_2: f64 = std::f64::consts::LN_2;

pub(super) fn emit_pow(
    body: &mut Function,
    base: &VarRef,
    exponent: &VarRef,
    locals: &HashMap<String, u32>,
    scratch: &MathLocals,
) -> AotResult<()> {
    if base.ty != exponent.ty || !matches!(base.ty, StaticType::F32 | StaticType::F64) {
        return Err(unsupported("Wasm power requires homogeneous Float32 or Float64 arguments"));
    }
    get_as_f64(body, base, locals)?;
    body.instruction(&W::LocalSet(scratch.x));
    get_as_f64(body, exponent, locals)?;
    body.instruction(&W::LocalSet(scratch.y));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::LocalSet(scratch.exponent));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::I32And);
    body.instruction(&W::Select);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalSet(scratch.factor));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Lt);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::F64Trunc);
    body.instruction(&W::F64Ne);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::F64Const(2.0.into()));
    body.instruction(&W::F64Div);
    body.instruction(&W::F64Trunc);
    body.instruction(&W::F64Const(2.0.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::F64Ne);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::F64Const((-1.0).into()));
    body.instruction(&W::LocalSet(scratch.factor));
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Abs);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::End);
    emit_log(body, scratch);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(scratch.x));
    emit_exp(body, scratch);
    body.instruction(&W::LocalGet(scratch.factor));
    body.instruction(&W::F64Mul);
    if base.ty == StaticType::F32 {
        body.instruction(&W::F32DemoteF64);
    }
    Ok(())
}

fn get_as_f64(
    body: &mut Function,
    value: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    get(body, locals, value)?;
    if value.ty == StaticType::F32 {
        body.instruction(&W::F64PromoteF32);
    }
    Ok(())
}

fn emit_log(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Le);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::I64ReinterpretF64);
    body.instruction(&W::I64Const(52));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::I64Const(0x7ff));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Const(1023));
    body.instruction(&W::I64Sub);
    body.instruction(&W::F64ConvertI64S);
    body.instruction(&W::F64Const(LN_2.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(scratch.sum));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::I64ReinterpretF64);
    body.instruction(&W::I64Const(0x000f_ffff_ffff_ffff));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Const(0x3ff0_0000_0000_0000));
    body.instruction(&W::I64Or);
    body.instruction(&W::F64ReinterpretI64);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::F64Sub);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::F64Add);
    body.instruction(&W::F64Div);
    body.instruction(&W::LocalSet(scratch.y));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::LocalSet(scratch.term));
    for denominator in (1..=39).step_by(2) {
        body.instruction(&W::LocalGet(scratch.sum));
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::F64Const((2.0 / f64::from(denominator)).into()));
        body.instruction(&W::F64Mul);
        body.instruction(&W::F64Add);
        body.instruction(&W::LocalSet(scratch.sum));
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::LocalGet(scratch.y));
        body.instruction(&W::F64Mul);
        body.instruction(&W::LocalGet(scratch.y));
        body.instruction(&W::F64Mul);
        body.instruction(&W::LocalSet(scratch.term));
    }
    body.instruction(&W::LocalGet(scratch.sum));
}

fn emit_exp(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(LN_2.into()));
    body.instruction(&W::F64Div);
    body.instruction(&W::F64Nearest);
    body.instruction(&W::LocalSet(scratch.y));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::F64Const(LN_2.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::F64Sub);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalSet(scratch.sum));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalSet(scratch.term));
    for divisor in 1..=20 {
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::LocalGet(scratch.x));
        body.instruction(&W::F64Mul);
        body.instruction(&W::F64Const(f64::from(divisor).into()));
        body.instruction(&W::F64Div);
        body.instruction(&W::LocalSet(scratch.term));
        body.instruction(&W::LocalGet(scratch.sum));
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::F64Add);
        body.instruction(&W::LocalSet(scratch.sum));
    }
    body.instruction(&W::LocalGet(scratch.sum));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::I64TruncSatF64S);
    body.instruction(&W::I64Const(1023));
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(52));
    body.instruction(&W::I64Shl);
    body.instruction(&W::F64ReinterpretI64);
    body.instruction(&W::F64Mul);
}

pub(super) fn emit_math_builtin(
    body: &mut Function,
    op: AotBuiltinOp,
    args: &[VarRef],
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    let ty = args.first().map(|arg| &arg.ty).ok_or_else(|| {
        unsupported(format!("Wasm scalar builtin `{op}` requires an argument"))
    })?;
    match op {
        AotBuiltinOp::Abs
        | AotBuiltinOp::Floor
        | AotBuiltinOp::Ceil
        | AotBuiltinOp::Trunc
        | AotBuiltinOp::Round => emit_native_unary(body, op, &args[0], locals),
        AotBuiltinOp::Sqrt => emit_sqrt(body, &args[0], locals),
        AotBuiltinOp::Min | AotBuiltinOp::Max => {
            emit_native_binary(body, op, args, locals)
        }
        AotBuiltinOp::Clamp => emit_clamp(body, args, locals),
        AotBuiltinOp::Isnan => emit_isnan(body, &args[0], locals),
        AotBuiltinOp::Isinf => emit_isinf(body, &args[0], locals),
        AotBuiltinOp::Isfinite => emit_isfinite(body, &args[0], locals),
        _ => Err(unsupported(format!(
            "Wasm AoT cannot emit scalar builtin `{op}` for `{}`",
            ty.julia_type_name()
        ))),
    }
}

fn emit_native_unary(
    body: &mut Function,
    op: AotBuiltinOp,
    arg: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    get(body, locals, arg)?;
    let instruction = match (&arg.ty, op) {
        (StaticType::F32, AotBuiltinOp::Abs) => W::F32Abs,
        (StaticType::F32, AotBuiltinOp::Floor) => W::F32Floor,
        (StaticType::F32, AotBuiltinOp::Ceil) => W::F32Ceil,
        (StaticType::F32, AotBuiltinOp::Trunc) => W::F32Trunc,
        (StaticType::F32, AotBuiltinOp::Round) => W::F32Nearest,
        (StaticType::F64, AotBuiltinOp::Abs) => W::F64Abs,
        (StaticType::F64, AotBuiltinOp::Floor) => W::F64Floor,
        (StaticType::F64, AotBuiltinOp::Ceil) => W::F64Ceil,
        (StaticType::F64, AotBuiltinOp::Trunc) => W::F64Trunc,
        (StaticType::F64, AotBuiltinOp::Round) => W::F64Nearest,
        _ => return Err(unsupported_float_builtin(op, &arg.ty)),
    };
    body.instruction(&instruction);
    Ok(())
}

fn emit_sqrt(
    body: &mut Function,
    arg: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    get(body, locals, arg)?;
    match arg.ty {
        StaticType::F32 => body.instruction(&W::F32Const(0.0.into())),
        StaticType::F64 => body.instruction(&W::F64Const(0.0.into())),
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Sqrt, &arg.ty)),
    };
    let less_than_zero = match arg.ty {
        StaticType::F32 => W::F32Lt,
        StaticType::F64 => W::F64Lt,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Sqrt, &arg.ty)),
    };
    body.instruction(&less_than_zero);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    get(body, locals, arg)?;
    let sqrt = match arg.ty {
        StaticType::F32 => W::F32Sqrt,
        StaticType::F64 => W::F64Sqrt,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Sqrt, &arg.ty)),
    };
    body.instruction(&sqrt);
    Ok(())
}

fn emit_native_binary(
    body: &mut Function,
    op: AotBuiltinOp,
    args: &[VarRef],
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    let [left, right] = args else {
        return Err(unsupported(format!("Wasm scalar builtin `{op}` requires two arguments")));
    };
    if left.ty != right.ty {
        return Err(unsupported(format!("Wasm scalar builtin `{op}` requires homogeneous arguments")));
    }
    get(body, locals, left)?;
    get(body, locals, right)?;
    let instruction = match (&left.ty, op) {
        (StaticType::F32, AotBuiltinOp::Min) => W::F32Min,
        (StaticType::F32, AotBuiltinOp::Max) => W::F32Max,
        (StaticType::F64, AotBuiltinOp::Min) => W::F64Min,
        (StaticType::F64, AotBuiltinOp::Max) => W::F64Max,
        _ => return Err(unsupported_float_builtin(op, &left.ty)),
    };
    body.instruction(&instruction);
    Ok(())
}

fn emit_clamp(
    body: &mut Function,
    args: &[VarRef],
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    let [value, lower, upper] = args else {
        return Err(unsupported("Wasm scalar builtin `clamp` requires three arguments"));
    };
    if value.ty != lower.ty || value.ty != upper.ty {
        return Err(unsupported("Wasm scalar builtin `clamp` requires homogeneous arguments"));
    }
    get(body, locals, lower)?;
    get(body, locals, upper)?;
    get(body, locals, value)?;
    get(body, locals, value)?;
    get(body, locals, upper)?;
    emit_compare(body, &value.ty, false)?;
    body.instruction(&W::Select);
    get(body, locals, value)?;
    get(body, locals, lower)?;
    emit_compare(body, &value.ty, true)?;
    body.instruction(&W::Select);
    Ok(())
}

fn emit_isnan(body: &mut Function, arg: &VarRef, locals: &HashMap<String, u32>) -> AotResult<()> {
    get(body, locals, arg)?;
    get(body, locals, arg)?;
    emit_compare_ne(body, &arg.ty)
}

fn emit_isinf(body: &mut Function, arg: &VarRef, locals: &HashMap<String, u32>) -> AotResult<()> {
    get(body, locals, arg)?;
    body.instruction(&match arg.ty {
        StaticType::F32 => W::F32Abs,
        StaticType::F64 => W::F64Abs,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Isinf, &arg.ty)),
    });
    emit_infinity(body, &arg.ty)?;
    emit_compare_eq(body, &arg.ty)
}

fn emit_isfinite(body: &mut Function, arg: &VarRef, locals: &HashMap<String, u32>) -> AotResult<()> {
    get(body, locals, arg)?;
    body.instruction(&match arg.ty {
        StaticType::F32 => W::F32Abs,
        StaticType::F64 => W::F64Abs,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Isfinite, &arg.ty)),
    });
    emit_infinity(body, &arg.ty)?;
    emit_compare(body, &arg.ty, true)
}

fn emit_infinity(body: &mut Function, ty: &StaticType) -> AotResult<()> {
    match ty {
        StaticType::F32 => body.instruction(&W::F32Const(f32::INFINITY.into())),
        StaticType::F64 => body.instruction(&W::F64Const(f64::INFINITY.into())),
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Isinf, ty)),
    };
    Ok(())
}

fn emit_compare(body: &mut Function, ty: &StaticType, less: bool) -> AotResult<()> {
    let instruction = match (ty, less) {
        (StaticType::F32, true) => W::F32Lt,
        (StaticType::F32, false) => W::F32Gt,
        (StaticType::F64, true) => W::F64Lt,
        (StaticType::F64, false) => W::F64Gt,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Clamp, ty)),
    };
    body.instruction(&instruction);
    Ok(())
}

fn emit_compare_eq(body: &mut Function, ty: &StaticType) -> AotResult<()> {
    body.instruction(&match ty {
        StaticType::F32 => W::F32Eq,
        StaticType::F64 => W::F64Eq,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Isinf, ty)),
    });
    Ok(())
}

fn emit_compare_ne(body: &mut Function, ty: &StaticType) -> AotResult<()> {
    body.instruction(&match ty {
        StaticType::F32 => W::F32Ne,
        StaticType::F64 => W::F64Ne,
        _ => return Err(unsupported_float_builtin(AotBuiltinOp::Isnan, ty)),
    });
    Ok(())
}

fn unsupported_float_builtin(op: AotBuiltinOp, ty: &StaticType) -> crate::aot::AotError {
    unsupported(format!(
        "Wasm scalar builtin `{op}` requires Float32 or Float64, got `{}`",
        ty.julia_type_name()
    ))
}
