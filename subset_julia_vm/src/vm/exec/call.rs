//! Function call instructions.
//!
//! Handles: Call, CallWithKwargs, CallWithSplat, CallSpecialize, CallIntrinsic, CallBuiltin
//!
//! ## Kwargs Binding Pattern (Issue #2397)
//!
//! Kwargs binding is centralized in two helper functions to avoid divergence:
//! - `bind_kwargs_defaults()`: Binds all kwargs to defaults (no kwargs provided at call site)
//! - `bind_kwargs_with_map()`: Binds kwargs using provided map (kwargs provided at call site)
//!
//! All call instruction handlers MUST use these helpers instead of inline kwargs logic.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::slot::slotize_code;
use super::util::bind_value_to_slot;
use super::DispatchAction;
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::rng::RngLike;
use crate::types::{JuliaType, TypeExpr};
use crate::vm::value::{
    native_array_ref_from_value, native_array_ref_value, FunctionValue, SymbolValue,
};
use std::collections::HashMap;
use std::rc::Rc;

struct KwDefaultEvalCtx<'a> {
    code: &'a [Instr],
    functions: &'a [Rc<FunctionInfo>],
    global_frame: Option<&'a Frame>,
    global_slot_map: &'a HashMap<String, usize>,
}

/// A single kw-default function call to evaluate (its index/info plus the
/// positional and keyword arguments). Bundled so the kw-default entry point
/// keeps a small parameter list (Issue #6832).
struct KwDefaultCallRequest<'a> {
    func_index: usize,
    func: &'a FunctionInfo,
    args: &'a [Value],
    kwargs: &'a HashMap<String, Value>,
}

fn eval_literal_kw_default(expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Literal(lit, _) => match lit {
            Literal::Int(v) => Some(Value::I64(*v)),
            Literal::Float(v) => Some(Value::F64(*v)),
            Literal::Float32(v) => Some(Value::F32(*v)),
            Literal::Float16(v) => Some(Value::F16(*v)),
            Literal::Bool(v) => Some(Value::Bool(*v)),
            Literal::Str(v) => Some(Value::Str(v.clone())),
            Literal::Char(v) => Some(Value::Char(*v)),
            Literal::Nothing => Some(Value::Nothing),
            Literal::Missing => Some(Value::Missing),
            Literal::Symbol(v) => Some(Value::Symbol(SymbolValue::new(v))),
            Literal::DataType(v) => Some(Value::DataType(Box::new(
                crate::types::JuliaType::from_name_or_struct(v),
            ))),
            Literal::Undef => Some(Value::Undef),
            _ => None,
        },
        Expr::QuoteLiteral { constructor, .. } => simple_symbol_quote_name(constructor)
            .map(|symbol| Value::Symbol(SymbolValue::new(symbol))),
        _ => None,
    }
}

fn simple_symbol_quote_name(expr: &Expr) -> Option<&str> {
    let Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args,
        ..
    } = expr
    else {
        return None;
    };
    let [Expr::Literal(Literal::Str(symbol), _)] = args.as_slice() else {
        return None;
    };
    Some(symbol)
}

fn bind_static_parametric_call_bindings(frame: &mut Frame, bindings: &[StaticParamBinding]) {
    for binding in bindings {
        match &binding.value {
            TypeExpr::Concrete(jt) => {
                frame.type_bindings.insert(binding.name.clone(), jt.clone());
            }
            TypeExpr::Parameterized { .. } => {
                frame
                    .type_bindings
                    .insert(binding.name.clone(), binding.value.to_julia_type_lossy());
            }
            TypeExpr::TypeVar(name) | TypeExpr::RuntimeExpr(name) => {
                if let Some(value) = parse_static_value_type_param(name) {
                    bind_val_parameter_value(frame, &binding.name, value);
                } else {
                    frame
                        .type_bindings
                        .insert(binding.name.clone(), JuliaType::from_name_or_struct(name));
                }
            }
        }
    }
}

fn parse_static_value_type_param(name: &str) -> Option<Value> {
    if let Some(value) = parse_val_constructor_parameter(name) {
        return Some(value);
    }
    if let Ok(value) = name.parse::<i64>() {
        return Some(Value::I64(value));
    }
    if let Ok(value) = name.parse::<f64>() {
        return Some(Value::F64(value));
    }
    if name == "true" {
        return Some(Value::Bool(true));
    }
    if name == "false" {
        return Some(Value::Bool(false));
    }
    if let Some(value) = parse_val_char_parameter(name) {
        return Some(Value::Char(value));
    }
    if let Some(value) = parse_val_tuple_parameter(name) {
        return Some(Value::Tuple(value));
    }
    name.strip_prefix(':')
        .map(|symbol| Value::Symbol(SymbolValue::new(symbol)))
}

fn value_from_bound_name(
    ctx: &KwDefaultEvalCtx<'_>,
    func: &FunctionInfo,
    frame: &Frame,
    name: &str,
) -> Option<Value> {
    if let Some(slot) = func
        .slot_names
        .iter()
        .position(|slot_name| slot_name == name)
    {
        if let Some(Some(value)) = frame.locals_slots.get(slot) {
            return Some(value.clone());
        }
    }
    frame
        .get_local(name)
        .or_else(|| {
            let global_frame = ctx.global_frame?;
            if let Some(slot) = ctx.global_slot_map.get(name) {
                if let Some(Some(value)) = global_frame.locals_slots.get(*slot) {
                    return Some(value.clone());
                }
            }
            global_frame.get_local(name)
        })
        // A bare `Inf`/`NaN`/`pi` (etc.) kwarg default is a Base global constant
        // the compiler emits as a float literal in expression position, but it is
        // not a bound runtime global, so the bound-name lookups above miss it.
        // Resolve it here (bound names take precedence) so `f(; a = Inf)` yields ∞
        // instead of the `Value::I64(0)` baked-default fallback (Issue #8078).
        .or_else(|| crate::compile::float_special_constant_value(name))
        .or_else(|| {
            ctx.functions
                .iter()
                .find(|candidate| function_name_matches(&candidate.name, name))
                .map(|candidate| Value::Function(FunctionValue::new(candidate.name.clone())))
        })
}

/// Exact value-based relational result for a mixed integer/float pair (Issue
/// #8187, generalized to every `Int*`/`UInt*` × `Float16`/`Float32`/`Float64`
/// width in #8199), or `None` for a non-relational op or non-mixed pair. Mirrors
/// the VM dynamic path's `exact_int_float_comparison` so both bypass the lossy
/// promote-to-float used for arithmetic.
fn exact_mixed_int_float_relational(op: &BinaryOp, left: &Value, right: &Value) -> Option<bool> {
    use crate::vm::numeric_identity::mixed_int_float_ordering;
    use std::cmp::Ordering;
    let ord = mixed_int_float_ordering(left, right)?; // `None` -> not a mixed pair
    Some(match op {
        BinaryOp::Eq => ord == Some(Ordering::Equal),
        BinaryOp::Ne => ord != Some(Ordering::Equal),
        BinaryOp::Lt => ord == Some(Ordering::Less),
        BinaryOp::Le => matches!(ord, Some(Ordering::Less | Ordering::Equal)),
        BinaryOp::Gt => ord == Some(Ordering::Greater),
        BinaryOp::Ge => matches!(ord, Some(Ordering::Greater | Ordering::Equal)),
        _ => return None,
    })
}

fn eval_numeric_binary_default(op: &BinaryOp, left: Value, right: Value) -> Option<Value> {
    // Mixed integer/float *relational* ops are value-based (Issue #8187/#8199,
    // the integer is NOT rounded to the float type); arithmetic still promotes
    // through the same-type arms below.
    if let Some(result) = exact_mixed_int_float_relational(op, &left, &right) {
        return Some(Value::Bool(result));
    }
    match (left, right) {
        (Value::I64(a), Value::I64(b)) => match op {
            BinaryOp::Add => Some(Value::I64(a.wrapping_add(b))),
            BinaryOp::Sub => Some(Value::I64(a.wrapping_sub(b))),
            BinaryOp::Mul => Some(Value::I64(a.wrapping_mul(b))),
            BinaryOp::Div => Some(Value::F64(a as f64 / b as f64)),
            BinaryOp::IntDiv => Some(Value::I64(a / b)),
            BinaryOp::Mod => Some(Value::I64(a % b)),
            BinaryOp::Eq => Some(Value::Bool(a == b)),
            BinaryOp::Ne => Some(Value::Bool(a != b)),
            BinaryOp::Lt => Some(Value::Bool(a < b)),
            BinaryOp::Le => Some(Value::Bool(a <= b)),
            BinaryOp::Gt => Some(Value::Bool(a > b)),
            BinaryOp::Ge => Some(Value::Bool(a >= b)),
            _ => None,
        },
        (Value::F64(a), Value::F64(b)) => match op {
            BinaryOp::Add => Some(Value::F64(a + b)),
            BinaryOp::Sub => Some(Value::F64(a - b)),
            BinaryOp::Mul => Some(Value::F64(a * b)),
            BinaryOp::Div => Some(Value::F64(a / b)),
            BinaryOp::Eq => Some(Value::Bool(a == b)),
            BinaryOp::Ne => Some(Value::Bool(a != b)),
            BinaryOp::Lt => Some(Value::Bool(a < b)),
            BinaryOp::Le => Some(Value::Bool(a <= b)),
            BinaryOp::Gt => Some(Value::Bool(a > b)),
            BinaryOp::Ge => Some(Value::Bool(a >= b)),
            _ => None,
        },
        // Mixed Int64/Float64 arithmetic promotes to Float64 (relational ops were
        // already handled value-based at the top of this function, Issue #8187).
        (Value::I64(a), Value::F64(b)) => {
            eval_numeric_binary_default(op, Value::F64(a as f64), Value::F64(b))
        }
        (Value::F64(a), Value::I64(b)) => {
            eval_numeric_binary_default(op, Value::F64(a), Value::F64(b as f64))
        }
        (Value::Bool(a), Value::Bool(b)) => match op {
            BinaryOp::Eq => Some(Value::Bool(a == b)),
            BinaryOp::Ne => Some(Value::Bool(a != b)),
            _ => None,
        },
        _ => None,
    }
}

fn eval_intrinsic_binary_default(
    intrinsic: &crate::intrinsics::Intrinsic,
    left: Value,
    right: Value,
) -> Option<Value> {
    use crate::intrinsics::Intrinsic;

    let both_int = matches!((&left, &right), (Value::I64(_), Value::I64(_)));
    let op = match (intrinsic, both_int) {
        (Intrinsic::AddFloat, true) | (Intrinsic::AddInt, _) => BinaryOp::Add,
        (Intrinsic::SubFloat, true) | (Intrinsic::SubInt, _) => BinaryOp::Sub,
        (Intrinsic::MulFloat, true) | (Intrinsic::MulInt, _) => BinaryOp::Mul,
        (Intrinsic::DivFloat, _) | (Intrinsic::SdivInt, _) => BinaryOp::Div,
        (Intrinsic::SremInt, _) => BinaryOp::Mod,
        (Intrinsic::EqFloat, true) | (Intrinsic::EqInt, _) => BinaryOp::Eq,
        (Intrinsic::NeFloat, true) | (Intrinsic::NeInt, _) => BinaryOp::Ne,
        (Intrinsic::LtFloat, true) | (Intrinsic::SltInt, _) => BinaryOp::Lt,
        (Intrinsic::LeFloat, true) | (Intrinsic::SleInt, _) => BinaryOp::Le,
        (Intrinsic::GtFloat, true) | (Intrinsic::SgtInt, _) => BinaryOp::Gt,
        (Intrinsic::GeFloat, true) | (Intrinsic::SgeInt, _) => BinaryOp::Ge,
        (Intrinsic::AddFloat, false) => BinaryOp::Add,
        (Intrinsic::SubFloat, false) => BinaryOp::Sub,
        (Intrinsic::MulFloat, false) => BinaryOp::Mul,
        (Intrinsic::EqFloat, false) => BinaryOp::Eq,
        (Intrinsic::NeFloat, false) => BinaryOp::Ne,
        (Intrinsic::LtFloat, false) => BinaryOp::Lt,
        (Intrinsic::LeFloat, false) => BinaryOp::Le,
        (Intrinsic::GtFloat, false) => BinaryOp::Gt,
        (Intrinsic::GeFloat, false) => BinaryOp::Ge,
        _ => return None,
    };
    eval_numeric_binary_default(&op, left, right)
}

fn eval_kw_default_expr(
    ctx: &KwDefaultEvalCtx<'_>,
    func: &FunctionInfo,
    frame: &Frame,
    expr: &Expr,
    depth: usize,
) -> Option<Value> {
    if depth > 4 {
        return None;
    }
    match expr {
        Expr::Literal(..) | Expr::QuoteLiteral { .. } => eval_literal_kw_default(expr),
        Expr::Var(name, _) => value_from_bound_name(ctx, func, frame, name),
        Expr::UnaryOp { op, operand, .. } => {
            let value = eval_kw_default_expr(ctx, func, frame, operand, depth)?;
            match (op, value) {
                (UnaryOp::Neg, Value::I64(v)) => Some(Value::I64(-v)),
                (UnaryOp::Neg, Value::F64(v)) => Some(Value::F64(-v)),
                (UnaryOp::Not, Value::Bool(v)) => Some(Value::Bool(!v)),
                (UnaryOp::Pos, value) => Some(value),
                _ => None,
            }
        }
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let left = eval_kw_default_expr(ctx, func, frame, left, depth)?;
            let right = eval_kw_default_expr(ctx, func, frame, right, depth)?;
            eval_numeric_binary_default(op, left, right)
        }
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if kwargs.is_empty()
            && splat_mask.iter().all(|is_splat| !*is_splat)
            && kwargs_splat_mask.is_empty() =>
        {
            let arg_values = eval_kw_default_args(ctx, func, frame, args, depth + 1)?;
            eval_kw_default_call(ctx, function, &arg_values, &HashMap::new(), depth + 1)
        }
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if splat_mask.iter().all(|is_splat| !*is_splat)
            && kwargs_splat_mask.iter().all(|is_splat| !*is_splat) =>
        {
            let arg_values = eval_kw_default_args(ctx, func, frame, args, depth + 1)?;
            let kwarg_values = eval_kw_default_kwargs(ctx, func, frame, kwargs, depth + 1)?;
            eval_kw_default_call(ctx, function, &arg_values, &kwarg_values, depth + 1)
        }
        _ => None,
    }
}

fn eval_kw_default_args(
    ctx: &KwDefaultEvalCtx<'_>,
    func: &FunctionInfo,
    frame: &Frame,
    args: &[Expr],
    depth: usize,
) -> Option<Vec<Value>> {
    let mut arg_values = Vec::with_capacity(args.len());
    for arg in args {
        arg_values.push(eval_kw_default_expr(ctx, func, frame, arg, depth)?);
    }
    Some(arg_values)
}

fn eval_kw_default_kwargs(
    ctx: &KwDefaultEvalCtx<'_>,
    func: &FunctionInfo,
    frame: &Frame,
    kwargs: &[(String, Expr)],
    depth: usize,
) -> Option<HashMap<String, Value>> {
    let mut kwarg_values = HashMap::with_capacity(kwargs.len());
    for (name, expr) in kwargs {
        let value = eval_kw_default_expr(ctx, func, frame, expr, depth)?;
        kwarg_values.insert(name.clone(), value);
    }
    Some(kwarg_values)
}

fn function_name_matches(actual: &str, requested: &str) -> bool {
    actual == requested
        || actual
            .rsplit_once('.')
            .is_some_and(|(_, short_name)| short_name == requested)
}

fn eval_kw_default_call(
    ctx: &KwDefaultEvalCtx<'_>,
    function: &str,
    args: &[Value],
    kwargs: &HashMap<String, Value>,
    depth: usize,
) -> Option<Value> {
    ctx.functions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, candidate)| {
            function_name_matches(&candidate.name, function)
                && candidate.params.len() == args.len()
                && candidate.vararg_param_index.is_none()
                && kwargs.keys().all(|name| {
                    candidate
                        .kwparams
                        .iter()
                        .any(|kwparam| !kwparam.is_varargs && &kwparam.name == name)
                })
        })
        .and_then(|(func_index, candidate)| {
            eval_simple_kw_default_function(
                ctx,
                &KwDefaultCallRequest {
                    func_index,
                    func: candidate,
                    args,
                    kwargs,
                },
                depth,
            )
        })
}

fn eval_simple_kw_default_function(
    ctx: &KwDefaultEvalCtx<'_>,
    request: &KwDefaultCallRequest<'_>,
    depth: usize,
) -> Option<Value> {
    let func = request.func;
    if depth > 4
        || func.params.len() != request.args.len()
        || func.vararg_param_index.is_some()
        || func.kwparams.iter().any(|kwparam| kwparam.is_varargs)
    {
        return None;
    }

    let mut frame = Frame::new_with_slots(func.local_slot_count, Some(request.func_index));
    for (idx, value) in request.args.iter().enumerate() {
        let slot = *func.param_slots.get(idx)?;
        if !frame.set_slot_value(slot, value.clone()) {
            return None;
        }
    }
    for kwparam in &func.kwparams {
        let value = if let Some(value) = request.kwargs.get(&kwparam.name) {
            value.clone()
        } else if kwparam.required {
            return None;
        } else {
            kwparam
                .default_expr
                .as_ref()
                .and_then(|expr| eval_kw_default_expr(ctx, func, &frame, expr, depth + 1))
                .unwrap_or_else(|| kwparam.default.clone())
        };
        if !frame.set_slot_value(kwparam.slot, value) {
            return None;
        }
    }
    run_kw_default_body(ctx, func, frame, depth)
}

/// Run the bounded (≤64-step) mini interpreter over a kw-default function body's
/// bytecode, with the function's positional arguments and keyword defaults
/// already bound into `frame`. Extracted from `eval_simple_kw_default_function`
/// so the binding phases and the interpreter loop are independently testable
/// (Issue #6832).
fn run_kw_default_body(
    ctx: &KwDefaultEvalCtx<'_>,
    func: &FunctionInfo,
    mut frame: Frame,
    depth: usize,
) -> Option<Value> {
    let mut stack: Vec<Value> = Vec::new();
    let mut ip = func.entry;
    let end = func.code_end.min(ctx.code.len());
    let mut steps = 0usize;
    while ip < end && steps < 64 {
        steps += 1;
        let instr = ctx.code.get(ip)?;
        ip += 1;
        match instr {
            Instr::PushI64(v) => stack.push(Value::I64(*v)),
            Instr::PushF64(v) => stack.push(Value::F64(*v)),
            Instr::PushF32(v) => stack.push(Value::F32(*v)),
            Instr::PushF16(v) => stack.push(Value::F16(*v)),
            Instr::PushBool(v) => stack.push(Value::Bool(*v)),
            Instr::PushStr(v) => stack.push(Value::Str(v.clone())),
            Instr::PushChar(v) => stack.push(Value::Char(*v)),
            Instr::PushNothing => stack.push(Value::Nothing),
            Instr::PushMissing => stack.push(Value::Missing),
            Instr::PushUndef => stack.push(Value::Undef),
            Instr::PushSymbol(v) => stack.push(Value::Symbol(SymbolValue::new(v))),
            Instr::PushDataType(v) => stack.push(Value::DataType(Box::new(
                crate::types::JuliaType::from_name_or_struct(v),
            ))),
            Instr::LoadSlot(slot) => {
                let value = frame.locals_slots.get(*slot)?.as_ref()?.clone();
                stack.push(value);
            }
            Instr::StoreSlot(slot) => {
                let value = stack.pop()?;
                if !frame.set_slot_value(*slot, value) {
                    return None;
                }
            }
            Instr::LoadSlotArray(slot) => {
                if let Some(v) = frame.slot_array(*slot) {
                    stack.push(native_array_ref_value(v.clone()));
                } else {
                    // The non-typed slot already holds the value verbatim; cloning
                    // it preserves the native-array carrier (a cheap `Rc` bump)
                    // without an explicit carrier-variant match (Issue #6806).
                    stack.push(frame.locals_slots.get(*slot)?.as_ref()?.clone());
                }
            }
            Instr::StoreSlotArray(slot) => {
                let value = stack.pop()?;
                let ok = match native_array_ref_from_value(value) {
                    Ok(arr) => frame.set_slot_array(*slot, arr),
                    Err(other) => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotTuple(slot) => {
                if let Some(v) = frame.slot_tuple(*slot) {
                    stack.push(Value::Tuple(v.clone()));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Tuple(v) => stack.push(Value::Tuple(v.clone())),
                        v => stack.push(v.clone()),
                    }
                }
            }
            Instr::StoreSlotTuple(slot) => {
                let value = stack.pop()?;
                let ok = match value {
                    Value::Tuple(tuple) => frame.set_slot_tuple(*slot, tuple),
                    other => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotNamedTuple(slot) => {
                if let Some(v) = frame.slot_named_tuple(*slot) {
                    stack.push(Value::NamedTuple(v.clone()));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::NamedTuple(v) => stack.push(Value::NamedTuple(v.clone())),
                        v => stack.push(v.clone()),
                    }
                }
            }
            Instr::StoreSlotNamedTuple(slot) => {
                let value = stack.pop()?;
                let ok = match value {
                    Value::NamedTuple(named_tuple) => {
                        frame.set_slot_named_tuple(*slot, named_tuple)
                    }
                    other => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotDict(slot) => {
                // `Value::Dict` retired (Issue #6731): a Dict local is a StructRef
                // value loaded through the generic slot path.
                let v = frame.locals_slots.get(*slot)?.as_ref()?;
                stack.push(v.clone());
            }
            Instr::StoreSlotDict(slot) => {
                let value = stack.pop()?;
                if !frame.set_slot_value(*slot, value) {
                    return None;
                }
            }
            Instr::LoadSlotSet(slot) => {
                // `Value::Set` retired (Issue #6732): a Set local is a StructRef
                // value loaded through the generic slot path.
                let v = frame.locals_slots.get(*slot)?.as_ref()?;
                stack.push(v.clone());
            }
            Instr::StoreSlotSet(slot) => {
                let value = stack.pop()?;
                if !frame.set_slot_value(*slot, value) {
                    return None;
                }
            }
            Instr::LoadSlotStruct(slot) => {
                if let Some(v) = frame.slot_struct(*slot) {
                    stack.push(Value::StructRef(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::StructRef(v) => stack.push(Value::StructRef(*v)),
                        Value::Struct(v) => stack.push(Value::Struct(v.clone())),
                        v => stack.push(v.clone()),
                    }
                }
            }
            Instr::StoreSlotStruct(slot) => {
                let value = stack.pop()?;
                let ok = match value {
                    Value::StructRef(idx) => frame.set_slot_struct_ref(*slot, idx),
                    other => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotRange(slot) => {
                if let Some(v) = frame.slot_range(*slot) {
                    stack.push(Value::Range(v.clone()));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Range(v) => stack.push(Value::Range(v.clone())),
                        v => stack.push(v.clone()),
                    }
                }
            }
            Instr::StoreSlotRange(slot) => {
                let value = stack.pop()?;
                let ok = match value {
                    Value::Range(range) => frame.set_slot_range(*slot, range),
                    other => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotRng(slot) => {
                if let Some(v) = frame.slot_rng(*slot) {
                    stack.push(Value::Rng(v.clone()));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Rng(v) => stack.push(Value::Rng(v.clone())),
                        v => stack.push(v.clone()),
                    }
                }
            }
            Instr::StoreSlotRng(slot) => {
                let value = stack.pop()?;
                let ok = match value {
                    Value::Rng(rng) => frame.set_slot_rng(*slot, rng),
                    other => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotGenerator(slot) => {
                if let Some(v) = frame.slot_generator(*slot) {
                    stack.push(Value::Generator(Box::new(v.clone())));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Generator(v) => stack.push(Value::Generator(v.clone())),
                        v => stack.push(v.clone()),
                    }
                }
            }
            Instr::StoreSlotGenerator(slot) => {
                let value = stack.pop()?;
                let ok = match value {
                    Value::Generator(generator) => frame.set_slot_generator(*slot, generator),
                    other => frame.set_slot_value(*slot, other),
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadSlotI64(slot) => {
                if let Some(v) = frame.slot_i64(*slot) {
                    stack.push(Value::I64(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        value @ (Value::I64(_)
                        | Value::Bool(_)
                        | Value::I32(_)
                        | Value::I16(_)
                        | Value::I8(_)
                        | Value::I128(_)
                        | Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::U64(_)
                        | Value::U128(_)
                        | Value::F16(_)
                        | Value::F32(_)
                        | Value::F64(_)) => stack.push(value.clone()),
                        _ => return None,
                    }
                }
            }
            Instr::LoadSlotI64ToF64(slot) => {
                if let Some(v) = frame.slot_i64(*slot) {
                    stack.push(Value::F64(v as f64));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::I64(v) => stack.push(Value::F64(*v as f64)),
                        Value::Bool(v) => stack.push(Value::F64(if *v { 1.0 } else { 0.0 })),
                        Value::I32(v) => stack.push(Value::F64(*v as f64)),
                        Value::I16(v) => stack.push(Value::F64(*v as f64)),
                        Value::I8(v) => stack.push(Value::F64(*v as f64)),
                        Value::I128(v) => stack.push(Value::F64(*v as f64)),
                        Value::U8(v) => stack.push(Value::F64(*v as f64)),
                        Value::U16(v) => stack.push(Value::F64(*v as f64)),
                        Value::U32(v) => stack.push(Value::F64(*v as f64)),
                        Value::U64(v) => stack.push(Value::F64(*v as f64)),
                        Value::U128(v) => stack.push(Value::F64(*v as f64)),
                        Value::F16(v) => stack.push(Value::F64(v.to_f64())),
                        Value::F32(v) => stack.push(Value::F64(*v as f64)),
                        Value::F64(v) => stack.push(Value::F64(*v)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotI64(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::I64(v) => v,
                    _ => return None,
                };
                if !frame.set_slot_i64(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotF64(slot) => {
                if let Some(v) = frame.slot_f64(*slot) {
                    stack.push(Value::F64(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::F64(v) => stack.push(Value::F64(*v)),
                        value @ (Value::F16(_) | Value::F32(_)) => stack.push(value.clone()),
                        Value::I64(v) => stack.push(Value::F64(*v as f64)),
                        Value::Bool(v) => stack.push(Value::F64(if *v { 1.0 } else { 0.0 })),
                        Value::I8(v) => stack.push(Value::F64(*v as f64)),
                        Value::I16(v) => stack.push(Value::F64(*v as f64)),
                        Value::I32(v) => stack.push(Value::F64(*v as f64)),
                        Value::I128(v) => stack.push(Value::F64(*v as f64)),
                        Value::U8(v) => stack.push(Value::F64(*v as f64)),
                        Value::U16(v) => stack.push(Value::F64(*v as f64)),
                        Value::U32(v) => stack.push(Value::F64(*v as f64)),
                        Value::U64(v) => stack.push(Value::F64(*v as f64)),
                        Value::U128(v) => stack.push(Value::F64(*v as f64)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotF64(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::F64(v) => v,
                    Value::I64(v) => v as f64,
                    _ => return None,
                };
                if !frame.set_slot_f64(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotBool(slot) => {
                if let Some(v) = frame.slot_bool(*slot) {
                    stack.push(Value::Bool(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Bool(v) => stack.push(Value::Bool(*v)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotBool(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::Bool(v) => v,
                    _ => return None,
                };
                if !frame.set_slot_bool(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotF32(slot) => {
                if let Some(v) = frame.slot_f32(*slot) {
                    stack.push(Value::F32(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::F32(v) => stack.push(Value::F32(*v)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotF32(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::F32(v) => v,
                    Value::F64(v) => v as f32,
                    Value::I64(v) => v as f32,
                    _ => return None,
                };
                if !frame.set_slot_f32(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotF16(slot) => {
                if let Some(v) = frame.slot_f16(*slot) {
                    stack.push(Value::F16(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::F16(v) => stack.push(Value::F16(*v)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotF16(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::F16(v) => v,
                    Value::F32(v) => half::f16::from_f32(v),
                    Value::F64(v) => half::f16::from_f64(v),
                    Value::I64(v) => half::f16::from_f64(v as f64),
                    _ => return None,
                };
                if !frame.set_slot_f16(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotStr(slot) => {
                if let Some(v) = frame.slot_str(*slot) {
                    stack.push(Value::Str(v.clone()));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Str(v) => stack.push(Value::Str(v.clone())),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotStr(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::Str(v) => v,
                    _ => return None,
                };
                if !frame.set_slot_str(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotChar(slot) => {
                if let Some(v) = frame.slot_char(*slot) {
                    stack.push(Value::Char(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Char(v) => stack.push(Value::Char(*v)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotChar(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::Char(v) => v,
                    _ => return None,
                };
                if !frame.set_slot_char(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotSymbol(slot) => {
                if let Some(v) = frame.slot_symbol(*slot) {
                    stack.push(Value::Symbol(v.clone()));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Symbol(v) => stack.push(Value::Symbol(v.clone())),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotSymbol(slot) => {
                let value = stack.pop()?;
                let v = match value {
                    Value::Symbol(v) => v,
                    _ => return None,
                };
                if !frame.set_slot_symbol(*slot, v) {
                    return None;
                }
            }
            Instr::LoadSlotNarrowInt(slot) => {
                if let Some(v) = frame.slot_narrow_int(*slot) {
                    stack.push(v.clone());
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        v @ (Value::I8(_)
                        | Value::I16(_)
                        | Value::I32(_)
                        | Value::I128(_)
                        | Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::U64(_)
                        | Value::U128(_)) => stack.push(v.clone()),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotNarrowInt(slot) => {
                let value = stack.pop()?;
                match value {
                    v @ (Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
                    | Value::I128(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)) => {
                        if !frame.set_slot_narrow_int(*slot, v) {
                            return None;
                        }
                    }
                    other => {
                        if !frame.set_slot_value(*slot, other) {
                            return None;
                        }
                    }
                }
            }
            Instr::LoadSlotNothing(slot) => {
                if frame.slot_nothing(*slot) {
                    stack.push(Value::Nothing);
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Nothing => stack.push(Value::Nothing),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotNothing(slot) => {
                let value = stack.pop()?;
                let ok = if matches!(value, Value::Nothing) {
                    frame.set_slot_nothing(*slot)
                } else {
                    frame.set_slot_value(*slot, value)
                };
                if !ok {
                    return None;
                }
            }
            Instr::LoadAny(name)
            | Instr::LoadI64(name)
            | Instr::LoadF64(name)
            | Instr::LoadF32(name)
            | Instr::LoadF16(name)
            | Instr::LoadBool(name)
            | Instr::LoadStr(name) => {
                let value = value_from_bound_name(ctx, func, &frame, name)?;
                stack.push(value);
            }
            Instr::DynamicAdd => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Add, left, right)?);
            }
            Instr::DynamicSub => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Sub, left, right)?);
            }
            Instr::DynamicMul => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Mul, left, right)?);
            }
            Instr::DynamicDiv => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Div, left, right)?);
            }
            Instr::AddI64 => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Add, left, right)?);
            }
            Instr::SubI64 => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Sub, left, right)?);
            }
            Instr::MulI64 => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_numeric_binary_default(&BinaryOp::Mul, left, right)?);
            }
            Instr::CallIntrinsic(intrinsic) | Instr::CallDynamicBinaryBoth(intrinsic, _) => {
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(eval_intrinsic_binary_default(intrinsic, left, right)?);
            }
            Instr::Call(target_index, arg_count)
            | Instr::CallInbounds(target_index, arg_count)
            | Instr::CallResolved(target_index, arg_count) => {
                let mut call_args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    call_args.push(stack.pop()?);
                }
                call_args.reverse();
                let target = ctx.functions.get(*target_index)?;
                let value = eval_simple_kw_default_function(
                    ctx,
                    &KwDefaultCallRequest {
                        func_index: *target_index,
                        func: target,
                        args: &call_args,
                        kwargs: &HashMap::new(),
                    },
                    depth + 1,
                )?;
                stack.push(value);
            }
            Instr::CallBuiltin(crate::builtins::BuiltinId::SymbolNew, 1) => {
                let Value::Str(symbol) = stack.pop()? else {
                    return None;
                };
                stack.push(Value::Symbol(SymbolValue::new(symbol)));
            }
            Instr::ReturnAny
            | Instr::ReturnI64
            | Instr::ReturnF64
            | Instr::ReturnF32
            | Instr::ReturnF16
            | Instr::ReturnArray
            | Instr::ReturnTuple
            | Instr::ReturnNamedTuple
            | Instr::ReturnRange
            | Instr::ReturnStruct
            | Instr::ReturnDict
            | Instr::ReturnSet
            | Instr::ReturnRef
            | Instr::ReturnMemory
            | Instr::ReturnRng => return stack.pop(),
            Instr::ReturnNothing => return Some(Value::Nothing),
            _ => return None,
        }
    }
    None
}

fn kwparam_default_value(
    ctx: &KwDefaultEvalCtx<'_>,
    func: &FunctionInfo,
    frame: &Frame,
    kwparam: &KwParamInfo,
) -> Value {
    kwparam
        .default_expr
        .as_ref()
        .and_then(|expr| eval_kw_default_expr(ctx, func, frame, expr, 0))
        .unwrap_or_else(|| kwparam.default.clone())
}

/// Bind all keyword arguments to their defaults (no kwargs provided at call site).
///
/// Used by: `Call`, `CallWithSplat`
///
/// This function handles:
/// - Required kwargs: Returns error if any required kwarg has no value
/// - kwargs varargs: Binds to empty `Pairs` (NOT `Nothing`)
/// - Regular kwargs: Binds to their default values
pub(super) fn bind_kwargs_defaults(
    func: &FunctionInfo,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    global_frame: Option<&Frame>,
    global_slot_map: &HashMap<String, usize>,
) -> Result<(), VmError> {
    let ctx = KwDefaultEvalCtx {
        code,
        functions,
        global_frame,
        global_slot_map,
    };
    for kwparam in &func.kwparams {
        if kwparam.required {
            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
        }
        let default_value = if kwparam.is_varargs {
            // kwargs varargs with no kwargs passed: bind empty Pairs (not Nothing)
            Value::Pairs(PairsValue {
                data: NamedTupleValue {
                    names: vec![],
                    values: vec![],
                },
            })
        } else {
            kwparam_default_value(&ctx, func, frame, kwparam)
        };
        bind_value_to_slot(frame, kwparam.slot, default_value, struct_heap);
    }
    Ok(())
}

/// Build the `MethodError` for an unsupported keyword argument the method does
/// not accept (Issue #5121), or `None` if every supplied keyword is accepted.
///
/// A keyword is accepted when it names a declared keyword parameter, or when
/// `func` has a `kwargs...` varargs collector (which absorbs any extra
/// keyword). This mirrors upstream Julia, which raises `MethodError`
/// ("unsupported keyword argument") instead of silently dropping unknown
/// keywords. The returned error is routed through the VM's exception handling
/// (so it is catchable) by `Vm::reject_unknown_kwargs_or_raise`.
pub(super) fn unknown_kwarg_error(
    func: &FunctionInfo,
    kwargs_map: &HashMap<String, Value>,
) -> Option<VmError> {
    if func.kwparams.iter().any(|kp| kp.is_varargs) {
        return None;
    }
    for key in kwargs_map.keys() {
        let is_declared = func
            .kwparams
            .iter()
            .any(|kp| !kp.is_varargs && &kp.name == key);
        if !is_declared {
            return Some(VmError::MethodError(format!(
                "no method matching {}(; {}::...): unsupported keyword argument \"{}\"",
                func.name, key, key
            )));
        }
    }
    None
}

/// Bind keyword arguments using provided kwargs map (kwargs provided at call site).
///
/// Used by: `CallWithKwargs`, `CallWithKwargsSplat`
///
/// This function handles:
/// - kwargs varargs: Collects remaining kwargs not matched to named kwparams
/// - Provided kwargs: Uses value from map
/// - Required kwargs: Returns error if not provided
/// - Regular kwargs: Falls back to default value
pub(super) fn bind_kwargs_with_map(
    func: &FunctionInfo,
    kwargs_map: &HashMap<String, Value>,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    global_frame: Option<&Frame>,
    global_slot_map: &HashMap<String, usize>,
) -> Result<(), VmError> {
    // Reject unknown keyword arguments (Issue #5121). The hot named-function
    // call paths (`CallWithKwargs` / `CallWithKwargsSplat`) pre-check via
    // `reject_unknown_kwargs_or_raise` so the error is *catchable* and they never
    // reach this point with an unknown keyword. This guard covers the remaining
    // function-variable / dynamic call paths that bind through here directly, so
    // an unknown keyword always errors rather than being silently dropped.
    if let Some(err) = unknown_kwarg_error(func, kwargs_map) {
        return Err(err);
    }

    let ctx = KwDefaultEvalCtx {
        code,
        functions,
        global_frame,
        global_slot_map,
    };
    for kwparam in &func.kwparams {
        if kwparam.is_varargs {
            // This is a kwargs varargs parameter (kwargs...)
            // Collect remaining kwargs that weren't matched to specific kwparams
            let mut remaining: Vec<(String, Value)> = Vec::with_capacity(kwargs_map.len());
            for (k, v) in kwargs_map.iter() {
                // Check if this key is a named kwparam (not the varargs one)
                let is_named_kwparam = func
                    .kwparams
                    .iter()
                    .any(|kp| !kp.is_varargs && &kp.name == k);
                if !is_named_kwparam {
                    remaining.push((k.clone(), v.clone()));
                }
            }
            // Create a Pairs from remaining kwargs (Julia's Base.Pairs type)
            let names: Vec<String> = remaining.iter().map(|(k, _)| k.clone()).collect();
            let values: Vec<Value> = remaining.into_iter().map(|(_, v)| v).collect();
            let pairs = Value::Pairs(PairsValue {
                data: NamedTupleValue { names, values },
            });
            bind_value_to_slot(frame, kwparam.slot, pairs, struct_heap);
        } else if let Some(val) = kwargs_map.get(&kwparam.name) {
            bind_value_to_slot(frame, kwparam.slot, val.clone(), struct_heap);
        } else if kwparam.required {
            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
        } else {
            let default_value = kwparam_default_value(&ctx, func, frame, kwparam);
            bind_value_to_slot(frame, kwparam.slot, default_value, struct_heap);
        }
    }
    Ok(())
}

impl<R: RngLike> Vm<R> {
    pub(crate) fn try_specialized_entry_for_runtime_call(
        &mut self,
        fallback_index: usize,
        args: &[Value],
    ) -> Option<usize> {
        let spec_func_index = self
            .specializable_functions
            .iter()
            .position(|func| func.fallback_index == fallback_index)?;
        let spec_func = self.specializable_functions.get(spec_func_index)?.clone();
        let fallback_func = self.functions.get(fallback_index)?.clone();
        let arg_types: Vec<ValueType> = args
            .iter()
            .map(|value| self.get_value_type(value))
            .collect();
        let key = SpecializationKey {
            func_index: spec_func_index,
            arg_types: arg_types.clone(),
        };

        if let Some(cached) = self.specialization_cache.get(&key) {
            return Some(cached.entry);
        }
        self.compile_context.as_ref()?;

        let type_object_names = specialize::collect_type_object_names(
            &self.struct_defs,
            self.compile_context.as_ref(),
            &self.abstract_types,
        );
        let module_path = specialize::module_path_from_function_name(&fallback_func.name);
        let result = specialize::specialize_function(
            &spec_func.ir,
            &arg_types,
            &self.struct_defs,
            &type_object_names,
            module_path,
            self.disable_array_getindex_specialization(),
            self.disable_field_access_specialization(),
        )
        .ok()?;
        let (entry_point, appended_len) =
            self.install_specialized_body(result.code, &fallback_func);
        self.specialization_cache.insert(
            key,
            SpecializedCode {
                entry: entry_point,
                return_type: result.return_type,
                code_len: appended_len,
            },
        );
        Some(entry_point)
    }

    /// Finalize a freshly specialized function body and append it to the running
    /// program, returning its `(entry_point, appended_len)`.
    ///
    /// Both runtime specialization sites — [`Self::try_specialized_entry_for_runtime_call`]
    /// and the dispatch-loop specializer — funnel through here so the
    /// *finalization* of specialized codegen (slotize → peephole → relocate →
    /// append) lives in exactly one place (Issue #8192: the binary-op / fusion
    /// codegen had been duplicated across paths).
    ///
    /// The post-slotize `peephole::optimize` pass is what gives a specialized
    /// body the same fused superinstructions (`LoadMulF64Slot`,
    /// `JumpIfGtI64Slots`, `AddConstI64SlotAndJumpIfLe`, …) the main compiler
    /// emits. Without it an *untyped* function specialized for concrete args ran
    /// an unfused body (~1.5x more ops per iteration) than its typed twin, so the
    /// untyped Aizawa / GCD loops were slower despite reaching the typed-loop
    /// fast path (Issue #8205). The pass self-consistently remaps the body's
    /// internal jump targets, so the `+ entry_point` relocation only has to shift
    /// them to absolute program addresses.
    fn install_specialized_body(
        &mut self,
        specialized_code: Vec<Instr>,
        fallback_func: &FunctionInfo,
    ) -> (usize, usize) {
        let entry_point = self.code.len();
        let mut specialized_code = specialized_code;
        let slot_map: HashMap<String, usize> = fallback_func
            .slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect();
        slotize_code(&mut specialized_code, &slot_map, &fallback_func.slot_types);
        let (specialized_code, _peephole_index_mapping) =
            crate::compile::peephole::optimize(specialized_code);

        let code = std::rc::Rc::make_mut(&mut self.code);
        for instr in specialized_code {
            let relocated = match instr {
                Instr::Jump(target) => Instr::Jump(target + entry_point),
                Instr::JumpIfZero(target) => Instr::JumpIfZero(target + entry_point),
                Instr::JumpIfNeI64(target) => Instr::JumpIfNeI64(target + entry_point),
                Instr::JumpIfEqI64(target) => Instr::JumpIfEqI64(target + entry_point),
                Instr::JumpIfLtI64(target) => Instr::JumpIfLtI64(target + entry_point),
                Instr::JumpIfGtI64(target) => Instr::JumpIfGtI64(target + entry_point),
                Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target) => {
                    Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target + entry_point)
                }
                Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, target) => {
                    Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, target + entry_point)
                }
                Instr::JumpIfLeI64(target) => Instr::JumpIfLeI64(target + entry_point),
                Instr::JumpIfGeI64(target) => Instr::JumpIfGeI64(target + entry_point),
                Instr::JumpIfEqF64(target) => Instr::JumpIfEqF64(target + entry_point),
                Instr::JumpIfNeF64(target) => Instr::JumpIfNeF64(target + entry_point),
                Instr::JumpIfNotLtF64(target) => Instr::JumpIfNotLtF64(target + entry_point),
                Instr::JumpIfNotGtF64(target) => Instr::JumpIfNotGtF64(target + entry_point),
                Instr::JumpIfNotLeF64(target) => Instr::JumpIfNotLeF64(target + entry_point),
                Instr::JumpIfNotGeF64(target) => Instr::JumpIfNotGeF64(target + entry_point),
                other => other,
            };
            code.push(relocated);
        }
        let appended_len = code.len() - entry_point;
        self.executable
            .append_bytecode(code, entry_point, code.len());
        (entry_point, appended_len)
    }

    /// Reject unknown keyword arguments before a keyworded call binds its frame
    /// (Issue #5121). Returns `Ok(true)` when an unsupported keyword was found
    /// and the resulting `MethodError` was caught by an active handler (the
    /// caller must then return `DispatchAction::Continue` so the dispatch loop
    /// resumes at the catch target); `Ok(false)` when all keywords are accepted
    /// and the call should proceed; or `Err` when the error was not caught and
    /// must propagate. Routing through `raise` (rather than returning `Err`
    /// directly from the binding code) makes the error catchable via
    /// `try`/`catch` / `@test_throws`, matching upstream Julia.
    pub(super) fn reject_unknown_kwargs_or_raise(
        &mut self,
        func: &FunctionInfo,
        kwargs_map: &HashMap<String, Value>,
    ) -> Result<bool, VmError> {
        if let Some(err) = unknown_kwarg_error(func, kwargs_map) {
            self.raise(err)?;
            return Ok(true);
        }
        Ok(false)
    }

    #[inline]
    fn execute_direct_call_fast(
        &mut self,
        func_index: usize,
        arg_count: usize,
        inbounds_context: bool,
    ) -> Result<Option<DispatchAction>, VmError> {
        let Some(func) = self.functions.get(func_index) else {
            let result: Result<(), VmError> = Err(VmError::InternalError(format!(
                "Function index {} out of bounds (have {} functions)",
                func_index,
                self.functions.len()
            )));
            self.try_or_handle(result)?;
            return Ok(Some(DispatchAction::Continue));
        };

        if func.is_generated
            || func.vararg_param_index.is_some()
            || !func.kwparams.is_empty()
            || !func.type_params.is_empty()
            || func.params.len() != arg_count
            || func.param_slots.len() != arg_count
        {
            crate::vm::profiler::record_event("CallDirectFastMiss");
            return Ok(None);
        }

        let local_slot_count = func.local_slot_count;
        let target_entry = func.entry;
        let target_end = func.code_end;
        let i64_fast_candidate = self
            .peek_i64_call_args(arg_count)
            .map(|args| (func.param_slots.clone(), args));

        if let Some((param_slots, i64_args)) = i64_fast_candidate {
            if let Some(result) = self.try_execute_direct_i64_function_call_i64_args(
                target_entry,
                target_end,
                &param_slots,
                &i64_args,
                Some(arg_count),
            ) {
                return Ok(Some(result));
            }
        }

        // Reuse a retired frame from the pool to avoid per-call map allocation
        // on this hottest direct-call path (Issue #5172).
        let mut frame = self.acquire_frame(local_slot_count, Some(func_index));
        frame.inbounds_context = inbounds_context;

        for idx in (0..arg_count).rev() {
            let slot = self.functions[func_index].param_slots[idx];
            let val = self.stack.pop_value()?;
            bind_value_to_slot(&mut frame, slot, val, &mut self.struct_heap);
        }

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = target_entry;
        crate::vm::profiler::record_event("CallDirectFastHit");
        Ok(Some(DispatchAction::Continue))
    }

    #[inline]
    fn try_execute_direct_i64_function_call_i64_args(
        &mut self,
        target_entry: usize,
        target_end: usize,
        param_slots: &[usize],
        i64_args: &[i64],
        stack_arg_count: Option<usize>,
    ) -> Option<DispatchAction> {
        let i64_value = self
            .try_execute_euclidean_modulo_i64_function_call_i64_args(
                target_entry,
                target_end,
                param_slots,
                i64_args,
            )
            .or_else(|| {
                self.try_execute_i64_function_call_i64_args(
                    target_entry,
                    target_end,
                    param_slots,
                    i64_args,
                )
            })?;

        if let Some(arg_count) = stack_arg_count {
            self.pop_call_args(arg_count);
        }
        crate::vm::profiler::record_event("CallDirectFastI64FunctionHit");
        if self.try_consume_i64_eq_branch(i64_value) {
            return Some(DispatchAction::Continue);
        }
        self.stack.push(Value::I64(i64_value));
        Some(DispatchAction::Continue)
    }

    fn peek_i64_call_args(&self, arg_count: usize) -> Option<Vec<i64>> {
        let start = self.stack.len().checked_sub(arg_count)?;
        self.stack[start..]
            .iter()
            .map(|value| match value {
                Value::I64(value) => Some(*value),
                _ => None,
            })
            .collect()
    }

    fn pop_call_args(&mut self, arg_count: usize) {
        let new_len = self.stack.len() - arg_count;
        self.stack.truncate(new_len);
    }

    fn execute_direct_call_with_args(
        &mut self,
        func_index: usize,
        args: Vec<Value>,
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        let func = match self.get_function_cloned_or_raise(func_index)? {
            Some(f) => f,
            None => return Ok(DispatchAction::Continue),
        };
        self.execute_direct_call_with_func_args(func_index, func, args, inbounds_context)
    }

    fn execute_direct_call_with_func_args(
        &mut self,
        func_index: usize,
        func: Rc<FunctionInfo>,
        args: Vec<Value>,
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        self.execute_direct_call_with_func_args_and_static_bindings(
            func_index,
            func,
            args,
            inbounds_context,
            &[],
        )
    }

    fn execute_direct_call_with_func_args_and_static_bindings(
        &mut self,
        func_index: usize,
        func: Rc<FunctionInfo>,
        args: Vec<Value>,
        inbounds_context: bool,
        static_bindings: &[StaticParamBinding],
    ) -> Result<DispatchAction, VmError> {
        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));
        frame.inbounds_context = inbounds_context;

        bind_static_parametric_call_bindings(&mut frame, static_bindings);

        // Extract type parameter bindings from arguments (Issue #2468)
        self.bind_type_params(&func, &args, &mut frame);

        // Bind positional arguments
        if let Some(vararg_idx) = func.vararg_param_index {
            // Function has varargs: bind args[0..vararg_idx] normally,
            // then collect remaining args into a Tuple for the vararg param
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
            // Collect remaining args into a Tuple
            let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: vararg_values,
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            // No varargs: bind 1-to-1
            for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
        }

        // Bind keyword arguments with their defaults (no kwargs provided)
        bind_kwargs_defaults(
            &func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        if let Some(result) =
            self.try_eval_cached_generated_expr(func_index, &func, &args, &frame)?
        {
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        let generated_eval_frame = func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(&func, &args, &mut frame);

        // Issue #6868: a `where`-parametric method (`type_params` non-empty) is
        // excluded from the typed direct-call fast path (`execute_direct_call_fast`
        // bails on `!func.type_params.is_empty()`) and from `CallSpecialize`
        // (`needs_specialization` requires an untyped param), so without this it
        // would run its fully generic body with every parameter bound to `Any`,
        // dynamically dispatching the inner operators — measurably slower than
        // both the untyped-generic and concrete-typed forms (#6846 profiling).
        // Specialize the body for the concrete runtime argument types (cached on
        // `(spec_idx, arg_types)`) and enter there instead. The method's bounds
        // (`T<:Real`, ...) were already validated by the static dispatch that
        // resolved this `CallResolved` to a single method, so per-argument-type
        // specialization is sound, and the type variable `T` stays bound in the
        // frame via `bind_type_params` above for bodies that reference it.
        let target_entry = if !func.type_params.is_empty() && !func.is_generated {
            self.try_specialized_entry_for_runtime_call(func_index, &args)
                .unwrap_or(func.entry)
        } else {
            func.entry
        };

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            &func,
            func_index,
            &args,
            generated_eval_frame,
        );
        self.ip = target_entry;
        Ok(DispatchAction::Continue)
    }

    /// Execute function call instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not a call operation.
    #[inline]
    pub(super) fn execute_call(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::Call(func_index, arg_count)
            | Instr::CallInbounds(func_index, arg_count)
            | Instr::CallResolved(func_index, arg_count) => {
                let inbounds_context = matches!(instr, Instr::CallInbounds(_, _));
                if let Some(result) =
                    self.execute_direct_call_fast(*func_index, *arg_count, inbounds_context)?
                {
                    return Ok(result);
                }

                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                self.execute_direct_call_with_func_args(*func_index, func, args, inbounds_context)
            }

            Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => self
                .execute_direct_call_i64_slots(
                    operands,
                    matches!(instr, Instr::CallInboundsI64Slots(_)),
                ),

            Instr::CallStaticParametric(operands) => {
                let func = match self.get_function_cloned_or_raise(operands.func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };
                let mut args = Vec::with_capacity(operands.arg_count);
                for _ in 0..operands.arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                self.execute_direct_call_with_func_args_and_static_bindings(
                    operands.func_index,
                    func,
                    args,
                    false,
                    &operands.bindings,
                )
            }

            Instr::CallWithKwargs(func_index, pos_arg_count, ref kwarg_names) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };

                // Pop kwarg values from stack (they were pushed last)
                let mut kwarg_values: Vec<Value> = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                // Build kwargs map
                let mut kwargs_map: HashMap<String, Value> = HashMap::new();
                for (name, value) in kwarg_names.iter().zip(kwarg_values) {
                    kwargs_map.insert(name.clone(), value);
                }

                // Reject unknown keyword arguments (Issue #5121). Done before any
                // positional args are popped / the frame is built so the
                // (catchable) MethodError is raised against the caller's handlers.
                if self.reject_unknown_kwargs_or_raise(&func, &kwargs_map)? {
                    return Ok(DispatchAction::Continue);
                }

                // Pop positional args
                let mut pos_args = Vec::with_capacity(*pos_arg_count);
                for _ in 0..*pos_arg_count {
                    pos_args.push(self.stack.pop_value()?);
                }
                pos_args.reverse();

                let mut frame = self.acquire_frame(func.local_slot_count, Some(*func_index));

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &pos_args, &mut frame);

                // Bind positional args (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = pos_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword args (use provided value or default)
                bind_kwargs_with_map(
                    &func,
                    &kwargs_map,
                    &mut frame,
                    &mut self.struct_heap,
                    &self.code,
                    &self.functions,
                    self.frames.first(),
                    &self.global_slot_map,
                )?;

                if let Some(result) =
                    self.try_eval_cached_generated_expr(*func_index, &func, &pos_args, &frame)?
                {
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }

                let generated_eval_frame = func.is_generated.then(|| frame.clone());
                self.bind_generated_body_arg_types(&func, &pos_args, &mut frame);
                self.return_ips.push(self.ip);
                self.try_push_call_frame(frame)?;
                self.remember_current_generated_expr_cache_key(
                    &func,
                    *func_index,
                    &pos_args,
                    generated_eval_frame,
                );
                self.ip = func.entry;
                Ok(DispatchAction::Continue)
            }

            Instr::CallWithKwargsSplat(
                func_index,
                pos_arg_count,
                ref kwarg_names,
                ref kwargs_splat_mask,
            ) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };

                // Pop kwarg values from stack (they were pushed last)
                let mut kwarg_values: Vec<Value> = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                // Build kwargs map, expanding splatted values
                let mut kwargs_map: HashMap<String, Value> = HashMap::new();
                for (idx, (name, value)) in kwarg_names.iter().zip(kwarg_values).enumerate() {
                    if kwargs_splat_mask.get(idx).copied().unwrap_or(false) {
                        // This is a splatted kwarg - expand NamedTuple/Dict into key-value pairs
                        match &value {
                            Value::NamedTuple(named_tuple) => {
                                for (k, v) in
                                    named_tuple.names.iter().zip(named_tuple.values.iter())
                                {
                                    kwargs_map.insert(k.clone(), v.clone());
                                }
                            }
                            Value::Pairs(pairs) => {
                                for (k, v) in pairs.data.names.iter().zip(pairs.data.values.iter())
                                {
                                    kwargs_map.insert(k.clone(), v.clone());
                                }
                            }
                            Value::Tuple(tuple) => {
                                // Tuple of pairs: ((k1, v1), (k2, v2), ...)
                                for elem in &tuple.elements {
                                    let Value::Tuple(pair) = elem else { continue };
                                    if pair.elements.len() != 2 {
                                        continue;
                                    }
                                    let Value::Symbol(key) = &pair.elements[0] else {
                                        continue;
                                    };
                                    kwargs_map
                                        .insert(key.as_str().to_string(), pair.elements[1].clone());
                                }
                            }
                            _ => {
                                // Unknown type - ignore silently
                            }
                        }
                    } else {
                        // Regular kwarg - add directly
                        kwargs_map.insert(name.clone(), value);
                    }
                }

                // Reject unknown keyword arguments after splat expansion (Issue
                // #5121), before any positional args are popped / the frame is
                // built, so the (catchable) MethodError targets the caller.
                if self.reject_unknown_kwargs_or_raise(&func, &kwargs_map)? {
                    return Ok(DispatchAction::Continue);
                }

                // Pop positional args
                let mut pos_args = Vec::with_capacity(*pos_arg_count);
                for _ in 0..*pos_arg_count {
                    pos_args.push(self.stack.pop_value()?);
                }
                pos_args.reverse();

                let mut frame = self.acquire_frame(func.local_slot_count, Some(*func_index));

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &pos_args, &mut frame);

                // Bind positional args (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = pos_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword args (use provided value or default)
                bind_kwargs_with_map(
                    &func,
                    &kwargs_map,
                    &mut frame,
                    &mut self.struct_heap,
                    &self.code,
                    &self.functions,
                    self.frames.first(),
                    &self.global_slot_map,
                )?;

                if let Some(result) =
                    self.try_eval_cached_generated_expr(*func_index, &func, &pos_args, &frame)?
                {
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }

                let generated_eval_frame = func.is_generated.then(|| frame.clone());
                self.bind_generated_body_arg_types(&func, &pos_args, &mut frame);
                self.return_ips.push(self.ip);
                self.try_push_call_frame(frame)?;
                self.remember_current_generated_expr_cache_key(
                    &func,
                    *func_index,
                    &pos_args,
                    generated_eval_frame,
                );
                self.ip = func.entry;
                Ok(DispatchAction::Continue)
            }

            Instr::CallWithSplat(func_index, arg_count, ref splat_mask) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };

                // Pop arguments from stack
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Expand splatted arguments
                let expanded_args = super::super::splat::expand_splat_arguments_with_heap(
                    args,
                    splat_mask,
                    &self.struct_heap,
                )?;

                let mut frame = self.acquire_frame(func.local_slot_count, Some(*func_index));

                // Bind type parameters from expanded arguments (Issue #5936).
                self.bind_type_params(&func, &expanded_args, &mut frame);

                // Bind expanded arguments to parameters (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = expanded_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining expanded args into a Tuple
                    let vararg_values: Vec<Value> = expanded_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = expanded_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword arguments with their defaults (no kwargs provided via splat)
                bind_kwargs_defaults(
                    &func,
                    &mut frame,
                    &mut self.struct_heap,
                    &self.code,
                    &self.functions,
                    self.frames.first(),
                    &self.global_slot_map,
                )?;

                if let Some(result) =
                    self.try_eval_cached_generated_expr(*func_index, &func, &expanded_args, &frame)?
                {
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }

                let generated_eval_frame = func.is_generated.then(|| frame.clone());
                self.bind_generated_body_arg_types(&func, &expanded_args, &mut frame);
                self.return_ips.push(self.ip);
                self.try_push_call_frame(frame)?;
                self.remember_current_generated_expr_cache_key(
                    &func,
                    *func_index,
                    &expanded_args,
                    generated_eval_frame,
                );
                self.ip = func.entry;
                Ok(DispatchAction::Continue)
            }

            // Lazy AoT call: specialize function based on runtime argument types
            Instr::CallSpecialize(spec_func_index, arg_count)
            | Instr::CallSpecializeInbounds(spec_func_index, arg_count) => {
                let args = self.pop_specialize_args(*arg_count)?;
                self.execute_call_specialize_with_args(
                    *spec_func_index,
                    args,
                    matches!(instr, Instr::CallSpecializeInbounds(_, _)),
                )
            }

            Instr::CallSpecializeI64Slots(operands)
            | Instr::CallSpecializeInboundsI64Slots(operands) => self
                .execute_call_specialize_i64_slots(
                    operands,
                    matches!(instr, Instr::CallSpecializeInboundsI64Slots(_)),
                ),

            Instr::CallIntrinsic(intrinsic) => {
                if let Err(err) = self.execute_intrinsic(*intrinsic) {
                    self.raise(err)?;
                    return Ok(DispatchAction::Continue);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::CallBuiltin(builtin_id, argc) => {
                if let Err(err) = self.execute_builtin(*builtin_id, *argc) {
                    self.raise(err)?;
                    return Ok(DispatchAction::Continue);
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    fn pop_specialize_args(&mut self, arg_count: usize) -> Result<Vec<Value>, VmError> {
        let mut args = Vec::with_capacity(arg_count);
        for _ in 0..arg_count {
            args.push(self.stack.pop_value()?);
        }
        args.reverse();
        Ok(args)
    }

    fn load_i64_slot_specialize_args(
        &mut self,
        slots: &[usize],
    ) -> Result<Option<Vec<Value>>, VmError> {
        let mut args = Vec::with_capacity(slots.len());
        for slot in slots {
            match self.load_i64_slot_specialize_arg(*slot) {
                Ok(Some(value)) => args.push(value),
                Ok(None) => {
                    for arg in args {
                        self.stack.push(arg);
                    }
                    return Ok(None);
                }
                Err(err) => {
                    for arg in args {
                        self.stack.push(arg);
                    }
                    return Err(err);
                }
            }
        }
        Ok(Some(args))
    }

    fn load_i64_slot_specialize_arg(&mut self, slot: usize) -> Result<Option<Value>, VmError> {
        let Some(frame) = self.frames.last() else {
            self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
            return Ok(None);
        };

        if let Some(value) = frame.slot_i64(slot) {
            return Ok(Some(Value::I64(value)));
        }

        match frame.locals_slots.get(slot) {
            Some(Some(
                value @ (Value::I64(_)
                | Value::Bool(_)
                | Value::I32(_)
                | Value::I16(_)
                | Value::I8(_)
                | Value::I128(_)
                | Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::U128(_)
                | Value::F16(_)
                | Value::F32(_)
                | Value::F64(_)),
            )) => Ok(Some(value.clone())),
            Some(Some(value)) => {
                let name = self.slot_name_for_frame(frame, slot);
                Err(VmError::InternalError(format!(
                    "LoadSlotI64: expected numeric in {}, got {:?}",
                    name, value
                )))
            }
            Some(None) => {
                let name = self.slot_name_for_frame(frame, slot);
                self.raise(VmError::UndefVarError(name))?;
                Ok(None)
            }
            None => Err(VmError::InternalError(format!(
                "LoadSlotI64: slot out of bounds: {}",
                slot
            ))),
        }
    }

    fn load_i64_slot_specialize_values(
        &mut self,
        slots: &[usize],
    ) -> Result<Option<Vec<i64>>, VmError> {
        let Some(frame) = self.frames.last() else {
            self.raise(VmError::UndefVarError("slot".to_string()))?;
            return Ok(None);
        };

        let mut args = Vec::with_capacity(slots.len());
        for slot in slots {
            match frame.slot_i64(*slot) {
                Some(value) => args.push(value),
                _ => return Ok(None),
            }
        }
        Ok(Some(args))
    }

    fn execute_direct_call_i64_slots(
        &mut self,
        operands: &CallDirectSlots,
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        let func_index = operands.func_index;
        let arg_count = operands.slots.len();
        let Some(func) = self.functions.get(func_index) else {
            let result: Result<(), VmError> = Err(VmError::InternalError(format!(
                "Function index {} out of bounds (have {} functions)",
                func_index,
                self.functions.len()
            )));
            self.try_or_handle(result)?;
            return Ok(DispatchAction::Continue);
        };

        if func.is_generated
            || func.vararg_param_index.is_some()
            || !func.kwparams.is_empty()
            || !func.type_params.is_empty()
            || func.params.len() != arg_count
            || func.param_slots.len() != arg_count
        {
            let Some(args) = self.load_i64_slot_specialize_args(&operands.slots)? else {
                return Ok(DispatchAction::Continue);
            };
            return self.execute_direct_call_with_args(func_index, args, inbounds_context);
        }

        let local_slot_count = func.local_slot_count;
        let target_entry = func.entry;
        let target_end = func.code_end;
        let param_slots = func.param_slots.clone();

        let Some(i64_args) = self.load_i64_slot_specialize_values(&operands.slots)? else {
            let Some(args) = self.load_i64_slot_specialize_args(&operands.slots)? else {
                return Ok(DispatchAction::Continue);
            };
            return self.execute_direct_call_with_args(func_index, args, inbounds_context);
        };

        if let Some(result) = self.try_execute_direct_i64_function_call_i64_args(
            target_entry,
            target_end,
            &param_slots,
            &i64_args,
            None,
        ) {
            return Ok(result);
        }

        let mut frame = self.acquire_frame(local_slot_count, Some(func_index));
        frame.inbounds_context = inbounds_context;
        for (idx, slot) in param_slots.iter().enumerate() {
            if let Some(value) = i64_args.get(idx) {
                if !frame.set_slot_i64(*slot, *value) {
                    return Err(VmError::InternalError(format!(
                        "CallResolvedI64Slots: slot out of bounds: {}",
                        slot
                    )));
                }
            }
        }

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = target_entry;
        crate::vm::profiler::record_event("CallDirectFastHit");
        Ok(DispatchAction::Continue)
    }

    fn execute_call_specialize_i64_slots(
        &mut self,
        operands: &CallSpecializeSlots,
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        let Some(i64_args) = self.load_i64_slot_specialize_values(&operands.slots)? else {
            let Some(args) = self.load_i64_slot_specialize_args(&operands.slots)? else {
                return Ok(DispatchAction::Continue);
            };
            return self.execute_call_specialize_with_args(
                operands.spec_func_index,
                args,
                inbounds_context,
            );
        };

        if let Some(result) = self.try_execute_cached_i64_slot_specialize_call(
            operands.spec_func_index,
            &i64_args,
            inbounds_context,
        )? {
            return Ok(result);
        }

        let args = i64_args.into_iter().map(Value::I64).collect();
        self.execute_call_specialize_with_args(operands.spec_func_index, args, inbounds_context)
    }

    fn try_execute_cached_i64_slot_specialize_call(
        &mut self,
        spec_func_index: usize,
        i64_args: &[i64],
        inbounds_context: bool,
    ) -> Result<Option<DispatchAction>, VmError> {
        // Cheap monomorphic fast path (Issue #8167): a previously-resolved
        // all-`I64` specialization is keyed by `(spec_func_index, arity)`, so the
        // dispatch avoids rebuilding and hashing a `Vec<ValueType>` key and
        // cloning the callee's `param_slots` on every call. The record is
        // populated by `record_i64_spec_dispatch` on the first call once the
        // specialization (and the callee's fast-path eligibility) is known; a
        // miss falls through to `execute_call_specialize_with_args`, which both
        // compiles/looks up the specialization and populates this cache.
        let Some(dispatch) = self
            .specialization_i64_cache
            .get(&(spec_func_index, i64_args.len()))
            .cloned()
        else {
            return Ok(None);
        };
        crate::vm::profiler::record_event("SpecializeI64DispatchCacheHit");

        let entry = dispatch.entry;
        let code_end = dispatch.code_end;

        if let Some(i64_value) = self.try_execute_euclidean_modulo_i64_function_call_i64_args(
            entry,
            code_end,
            &dispatch.param_slots,
            i64_args,
        ) {
            if self.try_consume_i64_eq_branch(i64_value) {
                return Ok(Some(DispatchAction::Continue));
            }
            self.stack.push(Value::I64(i64_value));
            return Ok(Some(DispatchAction::Continue));
        }

        if let Some(i64_value) = self.try_execute_i64_function_call_i64_args(
            entry,
            code_end,
            &dispatch.param_slots,
            i64_args,
        ) {
            if self.try_consume_i64_eq_branch(i64_value) {
                return Ok(Some(DispatchAction::Continue));
            }
            self.stack.push(Value::I64(i64_value));
            return Ok(Some(DispatchAction::Continue));
        }

        let mut frame =
            self.acquire_frame(dispatch.local_slot_count, Some(dispatch.fallback_index));
        frame.inbounds_context = inbounds_context;
        for (idx, slot) in dispatch.param_slots.iter().enumerate() {
            if let Some(value) = i64_args.get(idx) {
                if !frame.set_slot_i64(*slot, *value) {
                    return Err(VmError::InternalError(format!(
                        "CallSpecializeI64Slots: slot out of bounds: {}",
                        slot
                    )));
                }
            }
        }

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = entry;
        Ok(Some(DispatchAction::Continue))
    }

    /// Populate the cheap `(spec_func_index, arity)` dispatch cache for the
    /// all-`I64` specialize hot path (Issue #8167). No-op unless every argument
    /// is `I64` and the callee qualifies for the slot-based fast path (the same
    /// eligibility predicate the slow path used to re-check on every call).
    fn record_i64_spec_dispatch(
        &mut self,
        spec_func_index: usize,
        fallback_index: usize,
        arg_types: &[ValueType],
        fallback_func: &FunctionInfo,
        specialized: &SpecializedCode,
    ) {
        let arity = arg_types.len();
        if !arg_types.iter().all(|t| matches!(t, ValueType::I64)) {
            return;
        }
        if fallback_func.is_generated
            || fallback_func.vararg_param_index.is_some()
            || !fallback_func.kwparams.is_empty()
            || !fallback_func.type_params.is_empty()
            || fallback_func.params.len() != arity
            || fallback_func.param_slots.len() != arity
        {
            return;
        }
        let dispatch = I64SpecDispatch {
            entry: specialized.entry,
            code_end: specialized.entry + specialized.code_len,
            fallback_index,
            local_slot_count: fallback_func.local_slot_count,
            param_slots: Rc::from(fallback_func.param_slots.as_slice()),
        };
        self.specialization_i64_cache
            .insert((spec_func_index, arity), dispatch);
    }

    fn execute_call_specialize_with_args(
        &mut self,
        spec_func_index: usize,
        args: Vec<Value>,
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        let spec_func = match self.specializable_functions.get(spec_func_index) {
            Some(f) => f.clone(),
            None => {
                self.raise(VmError::InternalError(format!(
                    "unknown specializable function index: {}",
                    spec_func_index
                )))?;
                return Ok(DispatchAction::Continue);
            }
        };
        let fallback_func = match self.get_function_cloned_or_raise(spec_func.fallback_index)? {
            Some(f) => f,
            None => return Ok(DispatchAction::Continue),
        };

        let arg_types: Vec<ValueType> = args.iter().map(|v| self.get_value_type(v)).collect();

        let key = SpecializationKey {
            func_index: spec_func_index,
            arg_types: arg_types.clone(),
        };

        let specialized = if let Some(cached) = self.specialization_cache.get(&key) {
            Some(cached.clone())
        } else if self.compile_context.is_some() {
            let type_object_names = specialize::collect_type_object_names(
                &self.struct_defs,
                self.compile_context.as_ref(),
                &self.abstract_types,
            );
            let module_path = specialize::module_path_from_function_name(&fallback_func.name);
            match specialize::specialize_function(
                &spec_func.ir,
                &arg_types,
                &self.struct_defs,
                &type_object_names,
                module_path,
                self.disable_array_getindex_specialization(),
                self.disable_field_access_specialization(),
            ) {
                Ok(result) => {
                    let (entry_point, appended_len) =
                        self.install_specialized_body(result.code, &fallback_func);
                    let specialized = SpecializedCode {
                        entry: entry_point,
                        return_type: result.return_type,
                        code_len: appended_len,
                    };
                    self.specialization_cache.insert(key, specialized.clone());
                    Some(specialized)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Issue #8167: record the resolved all-`I64` specialization so the next
        // call at a `CallSpecializeI64Slots` site takes the cheap direct path in
        // `try_execute_cached_i64_slot_specialize_call` instead of rebuilding and
        // hashing a `Vec`-keyed `SpecializationKey` each time.
        if let Some(specialized_code) = &specialized {
            self.record_i64_spec_dispatch(
                spec_func_index,
                spec_func.fallback_index,
                &arg_types,
                &fallback_func,
                specialized_code,
            );
        }

        let target_entry = if let Some(specialized_code) = &specialized {
            specialized_code.entry
        } else {
            fallback_func.entry
        };

        if let Some(specialized_code) = &specialized {
            if let Some(value) = self.try_execute_euclidean_modulo_i64_function_call(
                specialized_code.entry,
                specialized_code.entry + specialized_code.code_len,
                &fallback_func.param_slots,
                &args,
            ) {
                match value {
                    Value::I64(i64_value) => {
                        if self.try_consume_i64_eq_branch(i64_value) {
                            return Ok(DispatchAction::Continue);
                        }
                        self.stack.push(Value::I64(i64_value));
                        return Ok(DispatchAction::Continue);
                    }
                    other => {
                        self.stack.push(other);
                        return Ok(DispatchAction::Continue);
                    }
                }
            }
            if let Some(i64_value) = self.try_execute_i64_function_call(
                specialized_code.entry,
                specialized_code.entry + specialized_code.code_len,
                &fallback_func.param_slots,
                &args,
            ) {
                if self.try_consume_i64_eq_branch(i64_value) {
                    return Ok(DispatchAction::Continue);
                }
                self.stack.push(Value::I64(i64_value));
                return Ok(DispatchAction::Continue);
            }
        }

        let mut frame = self.acquire_frame(
            fallback_func.local_slot_count,
            Some(spec_func.fallback_index),
        );
        frame.inbounds_context = inbounds_context;

        // `CallSpecialize` still executes in the original method's frame.
        // Bind `where` parameters from the fallback signature before jumping
        // into the specialized body so signatures like
        // `f(g, s::NTuple{N,T}) where {N,T}` can read `T`/`N` in the body
        // (Issue #8325).
        self.bind_type_params(&fallback_func, &args, &mut frame);

        if let Some(vararg_idx) = fallback_func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = fallback_func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
            let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: vararg_values,
            });
            if let Some(slot) = fallback_func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in fallback_func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        // Evaluate each omitted keyword's default *expression* in the real
        // call frame so a default that references a free name (a global /
        // const-global, e.g. `g(a; x=G)`) resolves to that binding instead
        // of the baked `kwparam.default` literal — which is `I64(0)` for any
        // non-foldable default. Specialized bodies still execute in the
        // fallback frame and read the same keyword slots, so defaults (including
        // the empty `kwargs...` Pairs collector) must be bound on both the
        // specialized and fallback paths (Issues #7774, #8375).
        bind_kwargs_defaults(
            &fallback_func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        if let Some(result) = self.try_eval_cached_generated_expr(
            spec_func.fallback_index,
            &fallback_func,
            &args,
            &frame,
        )? {
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        let generated_eval_frame = fallback_func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(&fallback_func, &args, &mut frame);
        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            &fallback_func,
            spec_func.fallback_index,
            &args,
            generated_eval_frame,
        );
        self.ip = target_entry;
        Ok(DispatchAction::Continue)
    }
}

#[cfg(test)]
mod kw_default_interpreter_tests {
    use super::{run_kw_default_body, KwDefaultEvalCtx};
    use crate::vm::frame::Frame;
    use crate::vm::instr::Instr;
    use crate::vm::types::FunctionInfo;
    use crate::vm::value::{Value, ValueType};
    use std::collections::HashMap;
    use std::rc::Rc;

    /// Minimal `FunctionInfo` for the mini interpreter: only `entry`/`code_end`/
    /// `local_slot_count` matter to `run_kw_default_body`.
    fn minimal_func(entry: usize, code_end: usize, local_slot_count: usize) -> FunctionInfo {
        FunctionInfo {
            name: String::new(),
            params: Vec::new(),
            kwparams: Vec::new(),
            entry,
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            min_world: 1,
            type_params: Vec::new(),
            param_julia_types: Vec::new(),
            code_start: entry,
            code_end,
            slot_names: Vec::new(),
            slot_types: Vec::new(),
            local_slot_count,
            param_slots: Vec::new(),
            vararg_param_index: None,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            def_line: 0,
        }
    }

    fn run(code: &[Instr], func: &FunctionInfo, frame: Frame) -> Option<Value> {
        let functions: Vec<Rc<FunctionInfo>> = Vec::new();
        let global_slot_map: HashMap<String, usize> = HashMap::new();
        let ctx = KwDefaultEvalCtx {
            code,
            functions: &functions,
            global_frame: None,
            global_slot_map: &global_slot_map,
        };
        run_kw_default_body(&ctx, func, frame, 0)
    }

    #[test]
    fn push_then_return_yields_pushed_value() {
        let code = [Instr::PushI64(7), Instr::ReturnAny];
        let func = minimal_func(0, 2, 0);
        assert!(matches!(
            run(&code, &func, Frame::new_with_slots(0, None)),
            Some(Value::I64(7))
        ));
    }

    #[test]
    fn load_slot_returns_bound_value() {
        let code = [Instr::LoadSlot(0), Instr::ReturnAny];
        let func = minimal_func(0, 2, 1);
        let mut frame = Frame::new_with_slots(1, None);
        frame.set_slot_value(0, Value::I64(99));
        assert!(matches!(run(&code, &func, frame), Some(Value::I64(99))));
    }

    #[test]
    fn return_nothing_yields_nothing() {
        let code = [Instr::ReturnNothing];
        let func = minimal_func(0, 1, 0);
        assert!(matches!(
            run(&code, &func, Frame::new_with_slots(0, None)),
            Some(Value::Nothing)
        ));
    }

    #[test]
    fn body_without_return_yields_none() {
        // Reaching `code_end` without a `Return*` instruction fails the eval.
        let code = [Instr::PushI64(1)];
        let func = minimal_func(0, 1, 0);
        assert!(run(&code, &func, Frame::new_with_slots(0, None)).is_none());
    }

    #[test]
    fn unhandled_instruction_bails_to_none() {
        // The interpreter handles only a constant-folding subset; control-flow
        // ops such as `Jump` are not in it and fall through to a `None` bail
        // (the call site then defers to the full VM).
        let code = [Instr::Jump(0)];
        let func = minimal_func(0, 1, 0);
        assert!(run(&code, &func, Frame::new_with_slots(0, None)).is_none());
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod specialized_body_peephole_8205_tests {
    //! Issue #8205: when a function with an *untyped* parameter is called with a
    //! concrete type, the VM appends a runtime-specialized body. That body must
    //! pass through the same post-slotize peephole fuser the main compiler uses
    //! ([`super::Vm::install_specialized_body`]); otherwise it runs an *unfused*
    //! hot loop (`LoadSlotF64; LoadSlotF64; MulF64` instead of `LoadMulF64Slot`)
    //! and is ~1.4x slower than its fully typed twin even though both reach the
    //! typed-loop fast path.
    use crate::compile::compile_with_cache;
    use crate::lowering::Lowering;
    use crate::parser::Parser;
    use crate::rng::StableRng;
    use crate::vm::{CompiledProgram, Instr, Vm};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn count_load_mul_f64_slot(code: &[Instr]) -> usize {
        code.iter()
            .filter(|instr| matches!(instr, Instr::LoadMulF64Slot(_)))
            .count()
    }

    /// The runtime specialization of an untyped-argument F64 loop is peephole
    /// fused: running the program appends at least one `LoadMulF64Slot` that the
    /// statically compiled program did not contain. Before the #8205 fix the
    /// specialized body was emitted unfused, so this count did not grow.
    #[test]
    fn untyped_arg_specialized_body_is_peephole_fused_8205() {
        // `hot8205`'s F64 locals are inferred; only the untyped bound `n` forces
        // a runtime specialization for `n::Int64`. `a * b` (two distinct slots)
        // fuses to `LoadMulF64Slot` once the body is peephole-optimized.
        let source = r#"
function hot8205(n)
    s = 0.0
    a = 1.5
    b = 2.5
    i = 0
    while i < n
        s = s + a * b
        i = i + 1
    end
    s
end
hot8205(4)
"#;
        let compiled = compile_source(source);
        let before = count_load_mul_f64_slot(&compiled.code);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        vm.run().expect("run hot8205");
        let after = count_load_mul_f64_slot(&vm.code);

        assert!(
            after > before,
            "runtime-specialized untyped-arg body must be peephole-fused into \
             LoadMulF64Slot (Issue #8205): before={before} after={after}"
        );
    }
}
