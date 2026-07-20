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
use super::util::{bind_value_to_slot, strip_module_prefix};
use super::DispatchAction;
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};
use crate::rng::RngLike;
use crate::types::{JuliaType, TypeExpr};
use crate::vm::executable::ResolvedSpecF64Callee;
use crate::vm::specialize::{CallableRegistry, SpecializableCallee, SpecializationRecursionGuard};
use crate::vm::splat::{KwargsMap, SplatPreparation};
use crate::vm::value::{
    is_complex_type_name, native_array_ref_from_value, native_array_ref_value, FunctionValue,
    SymbolValue,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

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

/// Resolve a keyword's declared annotation through the selected method's
/// positional/static `where` bindings. Both caller-supplied assertions and
/// omitted-default assertions must use this authority so `k::T` means the
/// same concrete type at both boundaries (Issues #11024, #11135).
fn resolve_kw_declared_type(
    declared: &JuliaType,
    type_bindings: &HashMap<String, JuliaType>,
) -> JuliaType {
    let mut resolved = declared.clone();
    for (name, replacement) in type_bindings {
        resolved = resolved.substitute(name, replacement);
    }
    resolved
}

fn eval_literal_kw_default(expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Literal(lit, _) => match lit {
            Literal::Int(v) => Some(Value::I64(*v)),
            Literal::Float(v) => Some(Value::F64(*v)),
            Literal::Float32(v) => Some(Value::F32(*v)),
            Literal::Float16(v) => Some(Value::F16(*v)),
            Literal::Bool(v) => Some(Value::Bool(*v)),
            Literal::Str(v) => Some(Value::str_new(v.clone())),
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

fn bind_static_parametric_call_bindings(
    frame: &mut Frame,
    bindings: &[StaticParamBinding],
    caller_type_bindings: Option<&HashMap<String, JuliaType>>,
) {
    for binding in bindings {
        match &binding.value {
            TypeExpr::Concrete(jt) => {
                let mut resolved = jt.clone();
                if let Some(caller_bindings) = caller_type_bindings {
                    for (name, replacement) in caller_bindings {
                        resolved = resolved.substitute(name, replacement);
                    }
                }
                frame.type_bindings.insert(binding.name.clone(), resolved);
            }
            TypeExpr::Parameterized { .. } => {
                let mut resolved = binding.value.to_julia_type_lossy();
                if let Some(caller_bindings) = caller_type_bindings {
                    for (name, replacement) in caller_bindings {
                        resolved = resolved.substitute(name, replacement);
                    }
                }
                frame.type_bindings.insert(binding.name.clone(), resolved);
            }
            TypeExpr::TypeVar(name) | TypeExpr::RuntimeExpr(name) => {
                if let Some(value) = parse_static_value_type_param(name) {
                    bind_val_parameter_value(frame, &binding.name, value);
                } else if let Some(caller_type) =
                    caller_type_bindings.and_then(|bindings| bindings.get(name))
                {
                    // A static-parametric instruction can forward a type
                    // parameter from its caller, e.g. an outer constructor
                    // `Foo(x::T) where T` calling the inner `Foo{T}(x)`.
                    // Preserve the caller's concrete binding instead of
                    // materializing the literal placeholder type `T`
                    // (Issues #10959, #10967).
                    frame
                        .type_bindings
                        .insert(binding.name.clone(), caller_type.clone());
                } else {
                    frame
                        .type_bindings
                        .insert(binding.name.clone(), JuliaType::from_name_or_struct(name));
                }
            }
        }
    }
}

/// Render a runtime type-argument value for a `MethodError` message
/// (`Foo{String}(...)`). Type parameters are `DataType`s in the common case and
/// plain values (`Val`-style parameters) otherwise (Issue #10998).
pub(in crate::vm::exec) fn runtime_type_binding_display(value: &Value) -> String {
    match value {
        Value::DataType(jt) => jt.to_string(),
        Value::I64(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Symbol(sym) => format!(":{}", sym.as_str()),
        other => format!("{other:?}"),
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
        .or_else(|| crate::runtime_constants::float_special_constant_value(name))
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
            // checked_div / wrapping_rem avoid the i64::MIN with -1 overflow
            // panics; div overflow (and b == 0) falls back to the slow path,
            // which raises DivideError, and rem(typemin, -1) == 0 (Issue #9429).
            BinaryOp::IntDiv => a.checked_div(b).map(Value::I64),
            BinaryOp::Mod => {
                if b == 0 {
                    None
                } else {
                    Some(Value::I64(a.wrapping_rem(b)))
                }
            }
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
        (Intrinsic::DynamicAdd, true) | (Intrinsic::AddInt, _) => BinaryOp::Add,
        (Intrinsic::DynamicSub, true) | (Intrinsic::SubInt, _) => BinaryOp::Sub,
        (Intrinsic::DynamicMul, true) | (Intrinsic::MulInt, _) => BinaryOp::Mul,
        (Intrinsic::DynamicDiv, _) | (Intrinsic::SdivInt, _) => BinaryOp::Div,
        (Intrinsic::SremInt, _) => BinaryOp::Mod,
        (Intrinsic::EqFloat, true) | (Intrinsic::EqInt, _) => BinaryOp::Eq,
        (Intrinsic::NeFloat, true) | (Intrinsic::NeInt, _) => BinaryOp::Ne,
        (Intrinsic::LtFloat, true) | (Intrinsic::SltInt, _) => BinaryOp::Lt,
        (Intrinsic::LeFloat, true) | (Intrinsic::SleInt, _) => BinaryOp::Le,
        (Intrinsic::GtFloat, true) | (Intrinsic::SgtInt, _) => BinaryOp::Gt,
        (Intrinsic::GeFloat, true) | (Intrinsic::SgeInt, _) => BinaryOp::Ge,
        (Intrinsic::DynamicAdd, false) => BinaryOp::Add,
        (Intrinsic::DynamicSub, false) => BinaryOp::Sub,
        (Intrinsic::DynamicMul, false) => BinaryOp::Mul,
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
    kwargs: &[(crate::ir::core::InternedStr, Expr)],
    depth: usize,
) -> Option<HashMap<String, Value>> {
    let mut kwarg_values = HashMap::with_capacity(kwargs.len());
    for (name, expr) in kwargs {
        let value = eval_kw_default_expr(ctx, func, frame, expr, depth)?;
        kwarg_values.insert(name.to_string(), value);
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
            Instr::PushStr(v) => stack.push(Value::str_new(v.clone())),
            Instr::PushStrBytes(v) => stack.push(Value::str_from_bytes(v.clone())),
            Instr::PushChar(v) => stack.push(Value::Char(*v)),
            Instr::PushCharMalformed(v) => stack.push(Value::CharMalformed(*v)),
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
            Instr::TakeSlot(slot) => {
                // Destructive slot load (Issue #10107): move the value out.
                let value = frame.locals_slots.get_mut(*slot)?.take()?;
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
                if let Some(v) = frame.slot_string_value(*slot) {
                    stack.push(v.clone());
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        v if v.string_bytes().is_some() => stack.push(v.clone()),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotStr(slot) => {
                let value = stack.pop()?;
                value.string_bytes()?;
                if !frame.set_slot_string_value(*slot, value) {
                    return None;
                }
            }
            Instr::LoadSlotChar(slot) => {
                if let Some(v) = frame.slot_char(*slot) {
                    stack.push(Value::Char(v));
                } else {
                    match frame.locals_slots.get(*slot)?.as_ref()? {
                        Value::Char(v) => stack.push(Value::Char(*v)),
                        Value::CharMalformed(v) => stack.push(Value::CharMalformed(*v)),
                        _ => return None,
                    }
                }
            }
            Instr::StoreSlotChar(slot) => {
                // Accept malformed Chars too (Issue #8995).
                let value = stack.pop()?;
                if !matches!(value, Value::Char(_) | Value::CharMalformed(_)) {
                    return None;
                }
                if !frame.set_slot_char_value(*slot, value) {
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
pub(in crate::vm) fn bind_kwargs_defaults(
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
    kwargs_map: &KwargsMap<Value>,
) -> Option<VmError> {
    if func.kwparams.iter().any(|kp| kp.is_varargs) {
        return None;
    }
    // Report the first unsupported keyword in call-site/splat-merge order
    // (Issue #11383): `kwargs_map` is now an insertion-ordered accumulator, so
    // this matches upstream Julia, which names the first-supplied unsupported
    // keyword rather than a hash-order-dependent one (previously worked around
    // by sorting, Issue #8658 — no longer needed once the map has real order).
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
    kwargs_map: &KwargsMap<Value>,
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
            if matches!(val, Value::Undef) && !kwparam.required {
                // Positional-default forwarding stubs preserve an omitted
                // keyword as the raw sentinel. A literal-default full method
                // materializes its own default here; a body-evaluated default
                // keeps the sentinel for its callee prologue (Issue #11135).
                let default_value = kwparam_default_value(&ctx, func, frame, kwparam);
                bind_value_to_slot(frame, kwparam.slot, default_value, struct_heap);
            } else {
                bind_value_to_slot(frame, kwparam.slot, val.clone(), struct_heap);
            }
        } else if kwparam.required {
            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
        } else {
            let default_value = kwparam_default_value(&ctx, func, frame, kwparam);
            bind_value_to_slot(frame, kwparam.slot, default_value, struct_heap);
        }
    }
    Ok(())
}

fn direct_call_runtime_specialization_candidate(func: &FunctionInfo) -> bool {
    if func.is_generated {
        return false;
    }
    if !func.type_params.is_empty() {
        return true;
    }
    func.param_julia_types
        .iter()
        .zip(func.params.iter())
        .any(|(julia_type, (_, value_type))| {
            matches!(value_type, ValueType::Any) && is_complex_runtime_annotation(julia_type)
        })
}

fn is_complex_runtime_annotation(ty: &JuliaType) -> bool {
    let JuliaType::Struct(name) = ty else {
        return false;
    };
    let unqualified = name
        .rsplit_once('.')
        .map_or(name.as_str(), |(_, tail)| tail);
    is_complex_type_name(unqualified)
}

fn runtime_specialization_supported_for_function(
    func: &FunctionInfo,
    arg_types: &[ValueType],
    code: &[Instr],
) -> bool {
    if !arg_types.iter().any(|ty| matches!(ty, ValueType::Function)) {
        return true;
    }
    // Function values carry candidate-method metadata in generic bytecode.
    // The runtime specializer currently tracks only the erased `Function` type,
    // so a body that materializes a resolved function value can recompile that
    // value into a bare global load and lose the binding (Issue #10423). Plain
    // higher-order functions such as `all(f, range)` still need specialization
    // for precise range element storage, so do not reject every `Function`
    // argument signature.
    !function_body_materializes_resolved_function_value(func, code)
}

fn function_body_materializes_resolved_function_value(func: &FunctionInfo, code: &[Instr]) -> bool {
    code.get(func.code_start..func.code_end)
        .is_some_and(|body| {
            body.iter()
                .any(|instr| matches!(instr, Instr::PushResolvedFunction(_)))
        })
}

/// Build the name -> callee lookup consulted by the runtime specializer's
/// `compile_call` (Issue #10749). A name is only registered when it resolves
/// to EXACTLY ONE method anywhere in `functions` — Julia multiple dispatch on
/// an ambiguous bare name has no sound resolution at this layer (the
/// specializer does not do argument-type-driven method resolution), so such
/// names are simply excluded; calls to them keep falling back to the
/// pre-existing `Unsupported` path.
pub(crate) fn build_specializable_callable_registry(
    functions: &[Rc<FunctionInfo>],
    specializable_functions: &[SpecializableFunction],
) -> CallableRegistry {
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for f in functions {
        *name_counts.entry(f.name.as_str()).or_insert(0) += 1;
    }
    let mut registry = CallableRegistry::new();
    for (idx, spec) in specializable_functions.iter().enumerate() {
        let Some(fallback) = functions.get(spec.fallback_index) else {
            continue;
        };
        if name_counts
            .get(fallback.name.as_str())
            .copied()
            .unwrap_or(0)
            != 1
        {
            continue;
        }
        if !fallback.type_params.is_empty()
            || fallback
                .param_julia_types
                .iter()
                .any(|ty| !matches!(ty, JuliaType::Any))
        {
            // Runtime specialization direct-calls a unique callable by name
            // from erased ValueType argument keys; it does not perform Julia
            // method applicability. Typed methods must therefore stay on the
            // generic dispatch path: `::Int` must reject strings,
            // `Vector{Number}` must reject `Vector{Int64}`, diagonal `where`
            // constraints must compare bindings, and `Type{T}` exactness must
            // not collapse to `ValueType::DataType` (Issue #10782).
            continue;
        }
        registry.insert(
            fallback.name.clone(),
            SpecializableCallee {
                spec_func_index: idx,
                ir: Arc::clone(&spec.ir),
                param_count: fallback.params.len(),
                has_vararg: fallback.vararg_param_index.is_some(),
                module_path: specialize::module_path_from_function_name(&fallback.name)
                    .map(str::to_string),
            },
        );
    }
    registry
}

impl<R: RngLike> Vm<R> {
    /// Cached wrapper around [`build_specializable_callable_registry`]. The
    /// registry only needs to be rebuilt when new functions or specializable
    /// entries have been registered since the last build (append-only Lazy
    /// AoT bytecode model), tracked via the two table lengths at build time.
    pub(crate) fn specializable_callable_registry(&mut self) -> Rc<CallableRegistry> {
        let functions_len = self.functions.len();
        let spec_len = self.specializable_functions.len();
        if let Some((cached_functions_len, cached_spec_len, cached)) =
            &self.specializable_callable_registry_cache
        {
            if *cached_functions_len == functions_len && *cached_spec_len == spec_len {
                return Rc::clone(cached);
            }
        }
        let registry = Rc::new(build_specializable_callable_registry(
            &self.functions,
            &self.specializable_functions,
        ));
        self.specializable_callable_registry_cache =
            Some((functions_len, spec_len, Rc::clone(&registry)));
        registry
    }
}

impl<R: RngLike> Vm<R> {
    /// Try to resolve (and possibly install) a runtime specialization for a
    /// direct call to `fallback_index` with the given concrete `args`.
    ///
    /// Returns `Some((entry_ip, local_slot_count))` when a specialization is
    /// available.  The caller must size the callee frame to `local_slot_count`
    /// rather than `fallback_func.local_slot_count`, because runtime
    /// specialization may introduce extra split slots (e.g. ComplexF64 SROA).
    pub(crate) fn try_specialized_entry_for_runtime_call(
        &mut self,
        fallback_index: usize,
        args: &[Value],
    ) -> Option<(usize, usize)> {
        self.try_specialized_body_for_runtime_call(fallback_index, args)
            .map(|(entry, _end, local_slot_count)| (entry, local_slot_count))
    }

    /// Try to resolve (and possibly install) a runtime-specialized body for a
    /// direct call, returning `(entry_ip, code_end, local_slot_count)`.
    pub(crate) fn try_specialized_body_for_runtime_call(
        &mut self,
        fallback_index: usize,
        args: &[Value],
    ) -> Option<(usize, usize, usize)> {
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
        if !runtime_specialization_supported_for_function(&fallback_func, &arg_types, &self.code) {
            return None;
        }
        let key = SpecializationKey {
            func_index: spec_func_index,
            arg_types: arg_types.clone(),
        };

        if let Some(cached) = self.specialization_cache.get(&key) {
            return Some((
                cached.entry,
                cached.entry + cached.code_len,
                cached.local_slot_count,
            ));
        }
        // Negative cache hit (Issue #8603): this signature already failed to
        // specialize; skip the (expensive) re-attempt.
        if self.specialization_failure_cache.contains(&key) {
            crate::vm::profiler::record_event("SpecializeFailureCacheHit");
            return None;
        }
        self.compile_context.as_ref()?;

        let type_object_names = specialize::collect_type_object_names(
            &self.struct_defs,
            self.compile_context.as_ref(),
            &self.abstract_types,
        );
        let module_path = specialize::module_path_from_function_name(&fallback_func.name);
        let callable_registry = self.specializable_callable_registry();
        let recursion_guard = RefCell::new(SpecializationRecursionGuard::new());
        let result = match specialize::specialize_function_with_callees(
            &spec_func.ir,
            &arg_types,
            &self.struct_defs,
            &type_object_names,
            module_path,
            self.disable_array_getindex_specialization(),
            self.disable_field_access_specialization(),
            &callable_registry,
            &recursion_guard,
            Some(spec_func_index),
        ) {
            Ok(result) => result,
            Err(_) => {
                // Remember the failure so later calls with the same signature
                // skip straight to the fallback (Issue #8603).
                self.specialization_failure_cache.insert(key);
                self.enforce_specialization_failure_cache_limit();
                return None;
            }
        };
        let (entry_point, appended_len, local_slot_count) =
            self.install_specialized_body(result.code, &fallback_func, &arg_types);
        self.specialization_cache.insert(
            key,
            SpecializedCode {
                entry: entry_point,
                return_type: result.return_type,
                code_len: appended_len,
                local_slot_count,
            },
        );
        self.enforce_specialization_cache_limit();
        Some((entry_point, entry_point + appended_len, local_slot_count))
    }

    /// Finalize a freshly specialized function body and append it to the running
    /// program, returning its `(entry_point, appended_len, local_slot_count)`.
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
        arg_types: &[ValueType],
    ) -> (usize, usize, usize) {
        let entry_point = self.code.len();
        let mut specialized_code = specialized_code;
        let mut slot_map: HashMap<String, usize> = fallback_func
            .slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect();
        // Issue #10567: runtime specialization may introduce fresh local names
        // (ComplexF64 SROA split slots like `__sjulia_cx_re_*`, spill temps,
        // etc.) that are not present in the fallback function's slot map.  Give
        // each such name a new slot index so `slotize_code` can turn the named
        // loads/stores into typed slot ops — required for the typed-loop
        // recognizer and the `ComplexMulAddAssign` superinstruction.
        //
        // Issue #10407: where-clause type parameters are installed into
        // `frame.type_bindings` (or value locals for Val{N}-style params) by
        // `bind_type_params`, NOT into slots. Specialized bodies that load them
        // via `LoadAny("T")` (so the local-callee path shadows a same-named
        // builtin) must keep that name-based load — assigning a fresh empty
        // slot turns `LoadAny` into `LoadSlot` and raises
        // `UndefVarError: slot N` because nothing ever stores into that slot.
        let type_param_names: HashSet<&str> = fallback_func
            .type_params
            .iter()
            .map(|tp| tp.name.as_str())
            .collect();
        let mut next_slot = fallback_func.local_slot_count;
        for instr in &specialized_code {
            let name = match instr {
                Instr::LoadI64(n)
                | Instr::LoadF64(n)
                | Instr::LoadAny(n)
                | Instr::LoadStr(n)
                | Instr::LoadBool(n)
                | Instr::LoadStruct(n)
                | Instr::StoreI64(n)
                | Instr::StoreF64(n)
                | Instr::StoreAny(n)
                | Instr::StoreStr(n)
                | Instr::StoreBool(n)
                | Instr::StoreStruct(n)
                | Instr::IncVarI64(n)
                | Instr::DecVarI64(n) => Some(n.as_str()),
                _ => None,
            };
            if let Some(name) = name {
                if type_param_names.contains(name) {
                    continue;
                }
                if !slot_map.contains_key(name) {
                    slot_map.insert(name.to_string(), next_slot);
                    next_slot += 1;
                }
            }
        }
        // Issue #10491: slotize against the SPECIALIZED slot types, not the
        // fallback's. The unspecialized fallback tags arg-dependent locals
        // `unknown`, which degrades the specialized body's typed name-based
        // loads/stores (`StoreF64(name)`, …) to generic `LoadSlot`/`StoreSlot`
        // — blocking the frame-less typed predecoders. Deriving the tags from
        // the concrete argument types (dispatch keys the body on exactly these
        // types) plus the specialized body's own stores is sound: the same
        // conflict-poisoning merge as compile time widens any slot whose
        // stores disagree back to `unknown`. Bodies outside the simple
        // positional shape keep the previous fallback-tag behavior.
        let simple_positional_shape = fallback_func.vararg_param_index.is_none()
            && fallback_func.type_params.is_empty()
            && fallback_func.params.len() == arg_types.len();
        let specialized_slot_types = if simple_positional_shape {
            let params: Vec<(String, ValueType)> = fallback_func
                .params
                .iter()
                .zip(arg_types.iter())
                .map(|((name, _), ty)| (name.clone(), ty.clone()))
                .collect();
            let kwparams: Vec<subset_julia_vm_bytecode::slot::SlotParamInfo> = fallback_func
                .kwparams
                .iter()
                .map(|kw| subset_julia_vm_bytecode::slot::SlotParamInfo {
                    name: kw.name.clone(),
                    ty: kw.ty.clone(),
                })
                .collect();
            Some(
                subset_julia_vm_bytecode::slot::build_specialized_slot_types(
                    &params,
                    &kwparams,
                    &specialized_code,
                    &slot_map,
                    next_slot,
                ),
            )
        } else {
            None
        };
        let fallback_slot_types: Vec<_> = if fallback_func.slot_types.len() >= next_slot {
            fallback_func.slot_types.to_vec()
        } else {
            let mut extended = fallback_func.slot_types.to_vec();
            extended.resize(next_slot, None);
            extended
        };
        subset_julia_vm_bytecode::slot::slotize_code(
            &mut specialized_code,
            &slot_map,
            specialized_slot_types
                .as_deref()
                .unwrap_or(&fallback_slot_types),
        );
        let (specialized_code, _peephole_index_mapping) =
            subset_julia_vm_bytecode::peephole::optimize(specialized_code);

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
                // Fused slot-vs-constant compare-and-branch (Issue #10105): the
                // `peephole::optimize` above can produce it for the specialized
                // body's constant loop guards, so its target must be shifted to
                // the absolute program address like every other branch here.
                Instr::JumpIfCmpI64SlotConst(slot, konst, cmp, target) => {
                    Instr::JumpIfCmpI64SlotConst(slot, konst, cmp, target + entry_point)
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
        self.executable.append_bytecode(
            code,
            &self.functions,
            self.base_function_count,
            entry_point,
            code.len(),
        );
        (entry_point, appended_len, next_slot)
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
        kwargs_map: &KwargsMap<Value>,
    ) -> Result<bool, VmError> {
        if let Some(err) = unknown_kwarg_error(func, kwargs_map) {
            self.raise(err)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn bind_kwargs_defaults_or_handle(
        &mut self,
        func: &FunctionInfo,
        frame: &mut Frame,
    ) -> Result<bool, VmError> {
        let result = bind_kwargs_defaults(
            func,
            frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        );
        Ok(self.try_or_handle(result)?.is_none())
    }

    /// Assert every SUPPLIED keyword argument against its DECLARED type,
    /// matching upstream Julia's keyword-argument semantics (Issue #11024).
    ///
    /// Upstream treats a keyword annotation as an ASSERTION, not a conversion:
    ///
    /// ```text
    /// k(; x::Int64 = 1) = x;  k(x = 2.0)
    ///   -> TypeError: in keyword argument x, expected Int64, got a value of type Float64
    /// h(; x::Real  = 1) = x;  h(x = 2.5)  -> 2.5   (abstract annotation accepts)
    /// ```
    ///
    /// The check runs through the same runtime `check_subtype` engine `isa`
    /// uses, against the precise `declared_type` (`KwParamInfo.ty` is the lossy
    /// slot `ValueType` and cannot express `Real`). An unannotated keyword has
    /// no `declared_type` and is unconstrained, as before.
    pub(super) fn check_supplied_kwarg_types(
        &mut self,
        func: &FunctionInfo,
        kwargs_map: &KwargsMap<Value>,
        type_bindings: &HashMap<String, JuliaType>,
    ) -> Result<(), VmError> {
        for kwparam in &func.kwparams {
            if kwparam.is_varargs {
                continue;
            }
            let Some(declared) = kwparam.declared_type.as_ref() else {
                continue;
            };
            let Some(value) = kwargs_map.get(&kwparam.name) else {
                continue;
            };
            // Issue #11124: `Value::Undef` in a keyword slot is the VM's
            // NOT-SUPPLIED sentinel, never a user value — Julia surface syntax
            // cannot pass `#undef`. It marks both a required keyword
            // (`compile/utils.rs`, `Literal::Undef` => "Required kwarg marker")
            // and a body-evaluated default (Issue #5121): the kwsorter binds the
            // sentinel and the callee's prologue overwrites it with the real
            // default (`k === Undef ? k = <default expr> : k`).
            //
            // A reduced-arity positional-default stub (`g(y, x=2; k::Integer=...)`
            // lowers to `g(y) = g(y, 2)`) has no such prologue: it forwards its own
            // raw `k` slot verbatim via `CallWithKwargs`, so the sentinel arrives
            // here as an EXPLICITLY PRESENT map entry. Asserting it reported
            // `TypeError: in keyword argument k, expected Integer, got a value of
            // type #undef` for a keyword the caller never supplied — breaking every
            // annotated keyword whose default is a CALL, whenever the function also
            // had a defaulted positional argument.
            //
            // Skipping the sentinel restores this function to its stated contract
            // (assert every SUPPLIED keyword): the callee's prologue still
            // materializes the default, and an omitted REQUIRED keyword still
            // raises `UndefKeywordError` from `bind_kwargs_with_map`, which owns
            // that check.
            if matches!(value, Value::Undef) {
                continue;
            }
            let expected_type = resolve_kw_declared_type(declared, type_bindings);
            let expected = expected_type.name();
            // Resolve heap-backed carriers (notably `StructRef`) to their Julia
            // type name before subtype checking. The shallow carrier label made
            // a valid `k::UserStruct = UserStruct(...)` supplied value fail the
            // #11024 assertion as `StructRef` (Issues #11024, #11135).
            let actual = self.get_type_name(value);
            if self.check_subtype(&actual, expected.as_ref()) {
                continue;
            }
            return Err(VmError::TypeError(format!(
                "in keyword argument {}, expected {}, got a value of type {}",
                kwparam.name, expected, actual
            )));
        }
        Ok(())
    }

    /// Assert the first body-entry store for an annotated optional keyword.
    ///
    /// Lowering places that store after the keyword's default guard and before
    /// the user body. This models upstream's typed inner keyword method: a bad
    /// DEFAULT is a MethodError, distinct from the TypeError emitted above for
    /// a caller-supplied value (Issue #11135). Returns `false` when the error
    /// was caught and the caller must skip the store and resume at the handler.
    pub(in crate::vm) fn validate_pending_kw_default_store(
        &mut self,
        slot: usize,
        value: &Value,
    ) -> Result<bool, VmError> {
        let Some(frame) = self.frames.last() else {
            return Ok(true);
        };
        if !frame.pending_kw_default_type_checks.contains_key(&slot) {
            return Ok(true);
        }
        let Some(func_index) = frame.func_index else {
            return Ok(true);
        };
        let Some(func) = self.functions.get(func_index) else {
            return Ok(true);
        };
        let Some(kwparam) = func
            .kwparams
            .iter()
            .find(|kwparam| kwparam.slot == slot && kwparam.declared_type.is_some())
        else {
            return Ok(true);
        };
        let Some(declared_type) = kwparam.declared_type.as_ref() else {
            return Ok(true);
        };
        let expected = resolve_kw_declared_type(declared_type, &frame.type_bindings);
        let function_name = func.name.clone();
        let keyword_name = kwparam.name.clone();
        let expected_name = expected.name().into_owned();
        let actual_name = self.get_type_name(value);

        if !self.check_subtype(&actual_name, &expected_name) {
            self.raise(VmError::MethodError(format!(
                "no method matching keyword sorter for {}({}::{}) (expected {})",
                function_name, keyword_name, actual_name, expected_name
            )))?;
            return Ok(false);
        }

        if let Some(frame) = self.frames.last_mut() {
            frame.pending_kw_default_type_checks.remove(&slot);
        }
        Ok(true)
    }

    pub(super) fn bind_kwargs_with_map_or_handle(
        &mut self,
        func: &FunctionInfo,
        kwargs_map: &KwargsMap<Value>,
        frame: &mut Frame,
    ) -> Result<bool, VmError> {
        // Issue #11024: a declared keyword type is an assertion on the supplied
        // value; raise the upstream `TypeError` through the VM's exception
        // handling (so it stays catchable) before any binding happens.
        let assertion = self.check_supplied_kwarg_types(func, kwargs_map, &frame.type_bindings);
        if self.try_or_handle(assertion)?.is_none() {
            return Ok(true);
        }
        let result = bind_kwargs_with_map(
            func,
            kwargs_map,
            frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        );
        Ok(self.try_or_handle(result)?.is_none())
    }

    fn try_execute_resolved_generator_iterator_size(
        &mut self,
        func_index: usize,
        arg_count: usize,
    ) -> Result<Option<DispatchAction>, VmError> {
        if arg_count != 1 {
            return Ok(None);
        }
        let Some(func) = self.functions.get(func_index) else {
            return Ok(None);
        };
        if func.name != "IteratorSize" && func.name != "Base.IteratorSize" {
            return Ok(None);
        }

        let is_filtered_generator = match self.stack.last() {
            Some(Value::Generator(generator)) => self.generator_is_filtered(generator),
            _ => false,
        };
        if !is_filtered_generator {
            return Ok(None);
        }

        self.stack.pop_value()?;
        let result = self.zero_field_struct_value("SizeUnknown")?;
        self.stack.push(result);
        Ok(Some(DispatchAction::Continue))
    }

    fn try_execute_native_range_direct_call(
        &mut self,
        func_index: usize,
        arg_count: usize,
    ) -> Result<Option<DispatchAction>, VmError> {
        if func_index >= self.base_function_count {
            return Ok(None);
        }
        let Some(func) = self.functions.get(func_index) else {
            return Ok(None);
        };
        let name = strip_module_prefix(func.name.as_str());
        if !matches!(
            name,
            "first" | "last" | "step" | "length" | "iterate" | "collect"
        ) {
            return Ok(None);
        }
        let Some(start) = self.stack.len().checked_sub(arg_count) else {
            return Ok(None);
        };
        let Some(Value::Range(range)) = self.stack.get(start) else {
            return Ok(None);
        };
        let range = range.clone();
        let range_arg = Value::Range(range.clone());
        let state_arg = self.stack.get(start + 1).cloned();

        let result = match (name, arg_count) {
            ("first", 1) => range
                .first_value()
                .ok_or_else(|| VmError::TypeError("first: collection is empty".to_string()))?,
            ("last", 1) => range
                .last_value()
                .ok_or_else(|| VmError::TypeError("last: collection is empty".to_string()))?,
            ("step", 1) => range.typed_step(),
            ("length", 1) => range.length_value(),
            ("iterate", 1) => self.iterate_first(&range_arg)?,
            ("iterate", 2) => {
                let Some(state) = state_arg.as_ref() else {
                    return Ok(None);
                };
                self.iterate_next(&range_arg, state)?
            }
            ("collect", 1) => self.array_wrapper_value(range.collect())?,
            _ => return Ok(None),
        };

        self.pop_call_args(arg_count);
        self.stack.push(result);
        Ok(Some(DispatchAction::Continue))
    }

    fn try_native_range_call_value(
        &mut self,
        func_index: usize,
        func: &FunctionInfo,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        if func_index >= self.base_function_count {
            return Ok(None);
        }
        let name = strip_module_prefix(func.name.as_str());
        if !matches!(
            name,
            "first" | "last" | "step" | "length" | "iterate" | "collect"
        ) {
            return Ok(None);
        }
        let Some(Value::Range(range)) = args.first() else {
            return Ok(None);
        };

        let result = match (name, args.len()) {
            ("first", 1) => range
                .first_value()
                .ok_or_else(|| VmError::TypeError("first: collection is empty".to_string()))?,
            ("last", 1) => range
                .last_value()
                .ok_or_else(|| VmError::TypeError("last: collection is empty".to_string()))?,
            ("step", 1) => range.typed_step(),
            ("length", 1) => range.length_value(),
            ("iterate", 1) => self.iterate_first(&args[0])?,
            ("iterate", 2) => self.iterate_next(&args[0], &args[1])?,
            ("collect", 1) => self.array_wrapper_value(range.collect())?,
            _ => return Ok(None),
        };

        Ok(Some(result))
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
            || direct_call_runtime_specialization_candidate(func)
            || func.params.len() != arg_count
            || func.param_slots.len() != arg_count
        {
            crate::vm::profiler::record_event("CallDirectFastMiss");
            return Ok(None);
        }

        let local_slot_count = func.local_slot_count;
        let target_entry = func.entry;
        let target_end = func.code_end;
        let param_slots = func.param_slots.clone();
        let i64_fast_candidate = self
            .peek_i64_call_args(arg_count)
            .map(|args| (param_slots.clone(), args));

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

        let f64_fast_candidate = self
            .peek_f64_call_args(arg_count)
            .map(|args| (param_slots.clone(), args));

        if let Some((param_slots, f64_args)) = f64_fast_candidate {
            if let Some(result) = self.try_execute_direct_f64_function_call_f64_args(
                target_entry,
                target_end,
                &param_slots,
                &f64_args,
                Some(arg_count),
            ) {
                return Ok(Some(result));
            }
        }

        // Frame-less typed scalar function block (Issue #9693): small typed
        // functions (i64/f64/ComplexF64 params, scalar body, Return* exits)
        // execute directly from the argument stack — no frame, no argument
        // slot binding, no per-instruction dispatch, no return routing.
        if let Some(result) = self.try_execute_typed_scalar_function_call(
            func_index,
            target_entry,
            target_end,
            arg_count,
        ) {
            return Ok(Some(result));
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
        let i64_value = self.try_execute_i64_function_call_i64_args(
            target_entry,
            target_end,
            param_slots,
            i64_args,
        )?;

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

    #[inline]
    fn try_execute_direct_f64_function_call_f64_args(
        &mut self,
        target_entry: usize,
        target_end: usize,
        param_slots: &[usize],
        f64_args: &[f64],
        stack_arg_count: Option<usize>,
    ) -> Option<DispatchAction> {
        let f64_value = self.try_execute_f64_function_call_f64_args(
            target_entry,
            target_end,
            param_slots,
            f64_args,
        )?;

        if let Some(arg_count) = stack_arg_count {
            self.pop_call_args(arg_count);
        }
        crate::vm::profiler::record_event("CallDirectFastF64FunctionHit");
        if self.try_consume_f64_eq_branch(f64_value) {
            return Some(DispatchAction::Continue);
        }
        self.stack.push(Value::F64(f64_value));
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

    fn peek_f64_call_args(&self, arg_count: usize) -> Option<Vec<f64>> {
        let start = self.stack.len().checked_sub(arg_count)?;
        self.stack[start..]
            .iter()
            .map(|value| match value {
                Value::F64(value) => Some(*value),
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
        let result =
            self.execute_direct_call_with_func_args(func_index, func, &args, inbounds_context);
        // Issue #10103: reclaim the owned scratch vector into the pool.
        self.release_arg_vec(args);
        result
    }

    // `pub(in crate::vm)`: also entered by the register VM gate's call
    // trampoline (`vm::register_gate`, Issue #8558).
    pub(in crate::vm) fn execute_direct_call_with_func_args(
        &mut self,
        func_index: usize,
        func: Rc<FunctionInfo>,
        args: &[Value],
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        self.execute_direct_call_with_func_args_and_static_bindings(
            func_index,
            func,
            args,
            inbounds_context,
            &[],
            false,
            false,
            None,
            &[],
        )
    }

    // Issue #10103: `args` is borrowed, not owned. Every positional value is
    // *cloned* into the callee frame's slots (or copied via `to_vec()` for
    // varargs), so the argument vector is pure scratch. Taking it by shared
    // slice lets the direct-call fill sites recycle the backing `Vec` through
    // `arg_vec_pool` instead of allocating a fresh one per call.
    /// Does a runtime type argument satisfy the declared bounds of the callee's
    /// matching `where` binder? `Foo{T}(x) where {T<:Number}` must reject
    /// `Foo{String}("a")` exactly like upstream, including when the type
    /// argument is only known at runtime (Issue #10998).
    ///
    /// A bound may depend on a sibling binder (`V<:AbstractVector{T}`), so the
    /// other runtime bindings of the same call are substituted into it first. A
    /// bound that still mentions an unresolved binder after substitution is not
    /// enforced here — argument-type validation covers those.
    fn runtime_type_binding_satisfies_bounds(
        &self,
        func: &FunctionInfo,
        name: &str,
        actual_type: &JuliaType,
        runtime_bindings: &[(String, Value)],
    ) -> bool {
        let Some(type_param) = func.type_params.iter().find(|tp| tp.name == name) else {
            return true;
        };
        let actual = actual_type.to_string();
        let resolve_bound = |bound: &str| -> Option<String> {
            let mut resolved = JuliaType::from_name_or_struct(bound);
            for (binder, value) in runtime_bindings {
                if let Value::DataType(jt) = value {
                    resolved = resolved.substitute(binder, jt.as_ref());
                }
            }
            let rendered = resolved.to_string();
            let mentions_unresolved_binder = func.type_params.iter().any(|tp| {
                rendered
                    .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                    .any(|token| token == tp.name)
            });
            (!mentions_unresolved_binder).then_some(rendered)
        };
        if let Some(upper) = type_param.get_upper_bound() {
            if let Some(bound) = resolve_bound(upper) {
                if !self.check_subtype(&actual, &bound) {
                    return false;
                }
            }
        }
        if let Some(lower) = &type_param.lower_bound {
            if let Some(bound) = resolve_bound(lower) {
                if !self.check_subtype(&bound, &actual) {
                    return false;
                }
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::vm::exec) fn execute_direct_call_with_func_args_and_static_bindings(
        &mut self,
        func_index: usize,
        func: Rc<FunctionInfo>,
        args: &[Value],
        inbounds_context: bool,
        static_bindings: &[StaticParamBinding],
        forward_caller_type_bindings: bool,
        validate_argument_types: bool,
        validation_fallback: Option<&StaticParametricFallback>,
        runtime_bindings: &[(String, Value)],
    ) -> Result<DispatchAction, VmError> {
        // A compiled forwarding call can carry an enclosing binder by name
        // (`Foo{T}(...)` inside another `where T` method). Resolve that name
        // against the caller frames before allocating the callee frame;
        // otherwise the callee receives the nominal type `T` instead of the
        // caller's concrete binding (Issue #10959, default-argument stubs).
        let mut resolved_static_bindings = Vec::with_capacity(static_bindings.len());
        let mut forwarded_value_bindings = Vec::new();
        for binding in static_bindings {
            let TypeExpr::TypeVar(name) = &binding.value else {
                resolved_static_bindings.push(binding.clone());
                continue;
            };
            let resolved = self
                .frames
                .iter()
                .enumerate()
                .rev()
                .find_map(|(frame_index, frame)| {
                    let carries_static_parameter = frame
                        .func_index
                        .and_then(|index| self.functions.get(index))
                        .is_some_and(|caller| {
                            caller.type_params.iter().any(|param| param.name == *name)
                        });
                    if !carries_static_parameter {
                        return None;
                    }
                    self.get_value_from_frame(name, frame_index)
                        .map(|value| match value {
                            Value::DataType(julia_type) => Ok(*julia_type),
                            other => Err(other),
                        })
                })
                .or_else(|| {
                    self.frames
                        .iter()
                        .rev()
                        .find_map(|frame| frame.type_bindings.get(name).cloned().map(Ok))
                });
            match resolved {
                Some(Ok(bound_type)) => resolved_static_bindings.push(StaticParamBinding {
                    name: binding.name.clone(),
                    value: TypeExpr::Concrete(bound_type),
                }),
                Some(Err(value)) => {
                    forwarded_value_bindings.push((binding.name.clone(), value));
                }
                None => resolved_static_bindings.push(binding.clone()),
            }
        }
        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));
        frame.inbounds_context = inbounds_context;

        let caller_type_bindings = if forward_caller_type_bindings {
            self.frames.last().map(|caller| &caller.type_bindings)
        } else {
            None
        };
        bind_static_parametric_call_bindings(
            &mut frame,
            &resolved_static_bindings,
            caller_type_bindings,
        );
        for (name, value) in forwarded_value_bindings {
            bind_val_parameter_value(&mut frame, &name, value);
        }

        // Runtime type arguments (`Foo{typeof(x)}(x)`, `Foo{t}(x)`): the binder
        // value only exists as a stack value, so it cannot be a literal
        // `StaticParamBinding`. Install it here and enforce the binder's
        // declared bounds — upstream raises a `MethodError` when the runtime
        // type argument is outside the inner constructor's `where` bound
        // (Issue #10998).
        for (name, value) in runtime_bindings {
            match value {
                Value::DataType(jt) => {
                    if !self.runtime_type_binding_satisfies_bounds(
                        &func,
                        name,
                        jt.as_ref(),
                        runtime_bindings,
                    ) {
                        self.release_unpushed_frame(frame);
                        return Err(VmError::MethodError(format!(
                            "no method matching {}{{{}}}({})",
                            func.name,
                            runtime_bindings
                                .iter()
                                .map(|(_, value)| runtime_type_binding_display(value))
                                .collect::<Vec<_>>()
                                .join(", "),
                            args.iter()
                                .map(|arg| format!("::{}", self.dispatch_julia_type_for_value(arg)))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )));
                    }
                    frame.type_bindings.insert(name.clone(), (**jt).clone());
                }
                other => bind_val_parameter_value(&mut frame, name, other.clone()),
            }
        }

        if validate_argument_types {
            let mut bindings = frame.type_bindings.clone();
            let arguments_match = crate::vm::expanded_param_types_for_call(&func, args.len())
                .is_some_and(|param_types| {
                    args.iter().zip(param_types.iter()).all(|(arg, param_ty)| {
                        self.value_matches_param_with_bindings(
                            arg,
                            param_ty,
                            &func.type_params,
                            &mut bindings,
                        )
                    })
                });
            if !arguments_match {
                self.release_unpushed_frame(frame);
                if let Some(fallback) = validation_fallback {
                    let Some(fallback_func) =
                        self.get_function_cloned_or_raise(fallback.func_index)?
                    else {
                        return Ok(DispatchAction::Continue);
                    };
                    return self.execute_direct_call_with_func_args_and_static_bindings(
                        fallback.func_index,
                        fallback_func,
                        args,
                        inbounds_context,
                        &fallback.bindings,
                        forward_caller_type_bindings,
                        true,
                        None,
                        runtime_bindings,
                    );
                }
                return Err({
                    let message = format!(
                        "no method matching {}({})",
                        func.name,
                        args.iter()
                            .map(|arg| self.dispatch_julia_type_for_value(arg).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    self.method_error_with_payload(message, &func.name, args)
                });
            }
        }

        // Extract type parameter bindings from arguments (Issue #2468)
        self.bind_type_params(&func, args, &mut frame);

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
        if self.bind_kwargs_defaults_or_handle(&func, &mut frame)? {
            return Ok(DispatchAction::Continue);
        }

        if let Some(result) =
            self.try_eval_cached_generated_expr(func_index, &func, args, &frame)?
        {
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        let generated_eval_frame = func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(&func, args, &mut frame);

        // Issue #6868: `where`-parametric direct calls need a concrete-runtime
        // specialization because `CallSpecialize` only covers untyped params.
        // Issue #8796 extends the same bridge to runtime-open annotations such
        // as `c::Complex`: static dispatch has already selected this method, but
        // the method body still has `Any`-like slots until we specialize it for
        // the concrete argument values.
        let target_entry = if direct_call_runtime_specialization_candidate(&func) {
            if let Some((entry, slot_count)) =
                self.try_specialized_entry_for_runtime_call(func_index, args)
            {
                if slot_count > frame.locals_slots.len() {
                    frame.locals_slots.resize(slot_count, None);
                }
                entry
            } else {
                func.entry
            }
        } else {
            func.entry
        };

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            &func,
            func_index,
            args,
            generated_eval_frame,
        );
        self.ip = target_entry;
        Ok(DispatchAction::Continue)
    }

    fn execute_generator_trait_direct_call_fast(
        &mut self,
        func_index: usize,
        arg_count: usize,
    ) -> Result<Option<DispatchAction>, VmError> {
        if arg_count != 1 {
            return Ok(None);
        }

        let Some(func) = self.functions.get(func_index) else {
            return Ok(None);
        };
        let trait_name = func
            .name
            .rsplit('.')
            .next()
            .unwrap_or(func.name.as_str())
            .to_string();
        if !matches!(trait_name.as_str(), "IteratorSize" | "IteratorEltype") {
            return Ok(None);
        }

        let stack_arg = self.stack.last().cloned();
        match stack_arg {
            Some(Value::Generator(g)) => {
                self.stack.pop_value()?;
                let result = match trait_name.as_str() {
                    "IteratorSize" if self.generator_is_filtered(&g) => {
                        self.zero_field_struct_value("SizeUnknown")?
                    }
                    "IteratorSize" => {
                        self.iterator_size_value_for_native_generator_iter(g.iter.as_ref())?
                    }
                    "IteratorEltype" => self.zero_field_struct_value("EltypeUnknown")?,
                    _ => return Ok(None),
                };
                self.stack.push(result);
                Ok(Some(DispatchAction::Continue))
            }
            Some(value) if self.pure_generator_iter_value(&value).is_some() => {
                let iter = self
                    .pure_generator_iter_value(&value)
                    .ok_or_else(|| VmError::InternalError("Generator iter vanished".to_string()))?;
                self.stack.pop_value()?;
                let result = match trait_name.as_str() {
                    "IteratorSize" if self.value_is_filter_struct(&iter) => {
                        self.zero_field_struct_value("SizeUnknown")?
                    }
                    "IteratorSize" => self.iterator_size_value_for_native_generator_iter(&iter)?,
                    "IteratorEltype" => self.zero_field_struct_value("EltypeUnknown")?,
                    _ => return Ok(None),
                };
                self.stack.push(result);
                Ok(Some(DispatchAction::Continue))
            }
            Some(Value::DataType(julia_type)) => {
                let Some(iter_type) =
                    super::call_dynamic::generator_iter_type_name(julia_type.as_ref())
                else {
                    return Ok(None);
                };
                self.stack.pop_value()?;
                let result = match trait_name.as_str() {
                    "IteratorSize" => {
                        self.iterator_size_value_for_generator_iter_type_name(&iter_type)?
                    }
                    "IteratorEltype" => self.zero_field_struct_value("EltypeUnknown")?,
                    _ => return Ok(None),
                };
                self.stack.push(result);
                Ok(Some(DispatchAction::Continue))
            }
            _ => Ok(None),
        }
    }

    /// Reject a statically indexed call when its method has not reached the
    /// current world yet. Dynamic dispatch already applies the same visibility
    /// fence; direct bytecode must not bypass source-ordered top-level method
    /// activation (Issues #9784 and #11477).
    fn direct_function_visible_or_raise(&mut self, func_index: usize) -> Result<bool, VmError> {
        let Some(func_name) = self
            .functions
            .get(func_index)
            .map(|function| function.name.clone())
        else {
            // Preserve the existing out-of-range InternalError path in the
            // concrete call handler below.
            return Ok(true);
        };
        if self.function_visible_in_world(func_index, self.current_dispatch_world()) {
            return Ok(true);
        }
        self.raise(VmError::UndefVarError(func_name))?;
        Ok(false)
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
                if !self.direct_function_visible_or_raise(*func_index)? {
                    return Ok(DispatchAction::Continue);
                }
                let inbounds_context = matches!(instr, Instr::CallInbounds(_, _));
                if matches!(instr, Instr::CallResolved(_, _)) {
                    if let Some(action) =
                        self.try_execute_resolved_generator_iterator_size(*func_index, *arg_count)?
                    {
                        return Ok(action);
                    }
                }
                // Register VM prototype gate (Issue #8558): behind
                // SJULIA_REGISTER_VM=1, eligible function bodies execute on
                // the side-by-side register VM. One `Option` check when off.
                if self.register_gate_enabled() {
                    if let Some(action) =
                        self.try_register_vm_call(*func_index, *arg_count, inbounds_context)?
                    {
                        return Ok(action);
                    }
                }
                if let Some(action) =
                    self.execute_generator_trait_direct_call_fast(*func_index, *arg_count)?
                {
                    return Ok(action);
                }
                if let Some(action) =
                    self.try_execute_native_range_direct_call(*func_index, *arg_count)?
                {
                    return Ok(action);
                }
                if let Some(result) =
                    self.execute_direct_call_fast(*func_index, *arg_count, inbounds_context)?
                {
                    return Ok(result);
                }

                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };
                // Issue #10103: recycle the scratch argument vector through the
                // pool instead of allocating one per call (hot for tight
                // recursion such as `fib(n)`).
                let mut args = self.acquire_arg_vec();
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                let result = self.execute_direct_call_with_func_args(
                    *func_index,
                    func,
                    &args,
                    inbounds_context,
                );
                self.release_arg_vec(args);
                result
            }

            Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => {
                if !self.direct_function_visible_or_raise(operands.func_index)? {
                    return Ok(DispatchAction::Continue);
                }
                self.execute_direct_call_i64_slots(
                    operands,
                    matches!(instr, Instr::CallInboundsI64Slots(_)),
                )
            }

            Instr::CallStaticParametric(operands) => {
                if !self.direct_function_visible_or_raise(operands.func_index)? {
                    return Ok(DispatchAction::Continue);
                }
                let func = match self.get_function_cloned_or_raise(operands.func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };
                // Runtime type arguments are pushed above the positional args,
                // so they pop first (Issue #10998).
                let mut runtime_bindings: Vec<(String, Value)> =
                    Vec::with_capacity(operands.runtime_binding_names.len());
                for name in operands.runtime_binding_names.iter().rev() {
                    runtime_bindings.push((name.clone(), self.stack.pop_value()?));
                }
                runtime_bindings.reverse();
                // Issue #10103: recycle the scratch argument vector via the pool.
                let mut args = self.acquire_arg_vec();
                for _ in 0..operands.arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                let result = self.execute_direct_call_with_func_args_and_static_bindings(
                    operands.func_index,
                    func,
                    &args,
                    false,
                    &operands.bindings,
                    operands.forward_caller_type_bindings,
                    operands.validate_argument_types,
                    operands.validation_fallback.as_ref(),
                    &runtime_bindings,
                );
                self.release_arg_vec(args);
                result
            }

            Instr::CallWithKwargs(func_index, pos_arg_count, ref kwarg_names) => {
                if !self.direct_function_visible_or_raise(*func_index)? {
                    return Ok(DispatchAction::Continue);
                }
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

                // Build kwargs map, preserving first-occurrence insertion order
                // (Issue #11383).
                let mut kwargs_map: KwargsMap<Value> = KwargsMap::new();
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
                if self.bind_kwargs_with_map_or_handle(&func, &kwargs_map, &mut frame)? {
                    return Ok(DispatchAction::Continue);
                }

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
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    if !self.direct_function_visible_or_raise(*func_index)? {
                        return Ok(DispatchAction::Continue);
                    }
                    let func = match self.get_function_cloned_or_raise(*func_index)? {
                        Some(f) => f,
                        None => return Ok(DispatchAction::Continue),
                    };

                    // Pop kwarg values from stack (they were pushed last)
                    let mut kwarg_values = Vec::with_capacity(kwarg_names.len());
                    for _ in 0..kwarg_names.len() {
                        let value = self.stack.pop_value()?;
                        kwarg_values.push(self.push_transient_root(value)?);
                    }
                    kwarg_values.reverse();

                    let kwargs_roots = match self.prepare_kwarg_value_roots(
                        kwarg_names,
                        kwargs_splat_mask,
                        &kwarg_values,
                    ) {
                        Ok(SplatPreparation::Ready(kwargs_map)) => kwargs_map,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let kwargs_map = kwargs_roots
                        .iter()
                        .map(|(name, &value)| Ok((name.clone(), self.clone_transient_root(value)?)))
                        .collect::<Result<KwargsMap<_>, VmError>>()?;

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
                            bind_value_to_slot(
                                &mut frame,
                                *slot,
                                vararg_tuple,
                                &mut self.struct_heap,
                            );
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
                    if self.bind_kwargs_with_map_or_handle(&func, &kwargs_map, &mut frame)? {
                        return Ok(DispatchAction::Continue);
                    }

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
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            Instr::CallWithSplat(func_index, arg_count, ref splat_mask) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    if !self.direct_function_visible_or_raise(*func_index)? {
                        return Ok(DispatchAction::Continue);
                    }
                    let func = match self.get_function_cloned_or_raise(*func_index)? {
                        Some(f) => f,
                        None => return Ok(DispatchAction::Continue),
                    };

                    // Pop arguments from stack
                    let mut args = Vec::with_capacity(*arg_count);
                    for _ in 0..*arg_count {
                        let value = self.stack.pop_value()?;
                        args.push(self.push_transient_root(value)?);
                    }
                    args.reverse();

                    let expanded_roots = match self.prepare_splat_argument_roots(&args, splat_mask)
                    {
                        Ok(SplatPreparation::Ready(expanded_args)) => expanded_args,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let expanded_args = self.clone_transient_roots(&expanded_roots)?;

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
                            bind_value_to_slot(
                                &mut frame,
                                *slot,
                                vararg_tuple,
                                &mut self.struct_heap,
                            );
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
                    if self.bind_kwargs_defaults_or_handle(&func, &mut frame)? {
                        return Ok(DispatchAction::Continue);
                    }

                    if let Some(result) = self.try_eval_cached_generated_expr(
                        *func_index,
                        &func,
                        &expanded_args,
                        &frame,
                    )? {
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
                })();
                self.end_transient_root_frame(root_base);
                result
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

            Instr::CallSpecializeF64Slots(operands)
            | Instr::CallSpecializeInboundsF64Slots(operands) => self
                .execute_call_specialize_f64_slots(
                    operands,
                    matches!(instr, Instr::CallSpecializeInboundsF64Slots(_)),
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
                let ctx = self.slot_debug_context_for_frame(frame, slot);
                Err(VmError::InternalError(format!(
                    "LoadSlotI64: expected numeric in {}, got {:?} [{}]",
                    name, value, ctx
                )))
            }
            Some(None) => {
                let name = self.slot_name_for_frame(frame, slot);
                self.raise(VmError::UndefVarError(name))?;
                Ok(None)
            }
            None => Err(super::slot_out_of_bounds("LoadSlotI64", slot)),
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

    /// Load I64 argument values from the current frame's typed slots into a
    /// caller-provided fixed-size stack buffer. Avoids the per-call `Vec`
    /// allocation of [`load_i64_slot_specialize_values`] for the common small
    /// arity case (the hot path in numeric loops such as `calc_pi`). Returns
    /// `Ok(Some(()))` on success, `Ok(None)` if a slot is not a typed I64 (caller
    /// should fall back to the generic `Value` path), and propagates frame errors.
    pub(in crate::vm) fn load_i64_slot_specialize_values_into<const N: usize>(
        &self,
        slots: &[usize],
        out: &mut [i64; N],
    ) -> Result<Option<()>, VmError> {
        if slots.len() > N {
            return Ok(None);
        }
        let Some(frame) = self.frames.last() else {
            return Err(VmError::UndefVarError("slot".to_string()));
        };
        for (i, slot) in slots.iter().enumerate() {
            match frame.slot_i64(*slot) {
                Some(value) => out[i] = value,
                _ => return Ok(None),
            }
        }
        Ok(Some(()))
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

        // Fast stack-buffer path for small-arity I64 calls; avoids a per-call
        // Vec allocation on this very hot path.
        let mut i64_args_buf = [0_i64; 8];
        if let Some(()) =
            self.load_i64_slot_specialize_values_into(&operands.slots, &mut i64_args_buf)?
        {
            let i64_args = &i64_args_buf[..operands.slots.len()];
            if let Some(result) = self.try_execute_direct_i64_function_call_i64_args(
                target_entry,
                target_end,
                &param_slots,
                i64_args,
                None,
            ) {
                return Ok(result);
            }

            let mut frame = self.acquire_frame(local_slot_count, Some(func_index));
            frame.inbounds_context = inbounds_context;
            for (idx, slot) in param_slots.iter().enumerate() {
                if let Some(value) = i64_args.get(idx) {
                    if !frame.set_slot_i64(*slot, *value) {
                        return Err(super::slot_out_of_bounds("CallResolvedI64Slots", slot));
                    }
                }
            }

            self.return_ips.push(self.ip);
            self.try_push_call_frame(frame)?;
            self.ip = target_entry;
            crate::vm::profiler::record_event("CallDirectFastHit");
            return Ok(DispatchAction::Continue);
        }

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
                    return Err(super::slot_out_of_bounds("CallResolvedI64Slots", slot));
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
        // Fast stack-buffer path for small-arity all-I64 specialize calls; avoids
        // a per-call Vec allocation on this very hot path.
        let mut i64_args_buf = [0_i64; 8];
        if let Some(()) =
            self.load_i64_slot_specialize_values_into(&operands.slots, &mut i64_args_buf)?
        {
            let i64_args = &i64_args_buf[..operands.slots.len()];
            if let Some(result) = self.try_execute_cached_i64_slot_specialize_call(
                operands.spec_func_index,
                i64_args,
                inbounds_context,
            )? {
                return Ok(result);
            }

            let args = i64_args.iter().copied().map(Value::I64).collect();
            return self.execute_call_specialize_with_args(
                operands.spec_func_index,
                args,
                inbounds_context,
            );
        }

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

    // ---- Float64 mirrors of the I64 slot-specialize path (Issue #10491) ----

    fn load_f64_slot_specialize_args(
        &mut self,
        slots: &[usize],
    ) -> Result<Option<Vec<Value>>, VmError> {
        let mut args = Vec::with_capacity(slots.len());
        for slot in slots {
            match self.load_f64_slot_specialize_arg(*slot) {
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

    fn load_f64_slot_specialize_arg(&mut self, slot: usize) -> Result<Option<Value>, VmError> {
        let Some(frame) = self.frames.last() else {
            self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
            return Ok(None);
        };

        if let Some(value) = frame.slot_f64(slot) {
            return Ok(Some(Value::F64(value)));
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
                let ctx = self.slot_debug_context_for_frame(frame, slot);
                Err(VmError::InternalError(format!(
                    "LoadSlotF64: expected numeric in {}, got {:?} [{}]",
                    name, value, ctx
                )))
            }
            Some(None) => {
                let name = self.slot_name_for_frame(frame, slot);
                self.raise(VmError::UndefVarError(name))?;
                Ok(None)
            }
            None => Err(super::slot_out_of_bounds("LoadSlotF64", slot)),
        }
    }

    fn load_f64_slot_specialize_values(
        &mut self,
        slots: &[usize],
    ) -> Result<Option<Vec<f64>>, VmError> {
        let Some(frame) = self.frames.last() else {
            self.raise(VmError::UndefVarError("slot".to_string()))?;
            return Ok(None);
        };

        let mut args = Vec::with_capacity(slots.len());
        for slot in slots {
            match frame.slot_f64(*slot) {
                Some(value) => args.push(value),
                _ => return Ok(None),
            }
        }
        Ok(Some(args))
    }

    /// Float64 mirror of [`Self::load_i64_slot_specialize_values_into`]
    /// (Issue #10491): load F64 argument values from the current frame's typed
    /// slots into a caller-provided fixed-size stack buffer.
    pub(in crate::vm) fn load_f64_slot_specialize_values_into<const N: usize>(
        &self,
        slots: &[usize],
        out: &mut [f64; N],
    ) -> Result<Option<()>, VmError> {
        if slots.len() > N {
            return Ok(None);
        }
        let Some(frame) = self.frames.last() else {
            return Err(VmError::UndefVarError("slot".to_string()));
        };
        for (i, slot) in slots.iter().enumerate() {
            match frame.slot_f64(*slot) {
                Some(value) => out[i] = value,
                _ => return Ok(None),
            }
        }
        Ok(Some(()))
    }

    /// Float64 mirror of [`Self::execute_call_specialize_i64_slots`]
    /// (Issue #10491).
    fn execute_call_specialize_f64_slots(
        &mut self,
        operands: &CallSpecializeSlots,
        inbounds_context: bool,
    ) -> Result<DispatchAction, VmError> {
        // Fast stack-buffer path for small-arity all-F64 specialize calls.
        let mut f64_args_buf = [0.0_f64; 8];
        if let Some(()) =
            self.load_f64_slot_specialize_values_into(&operands.slots, &mut f64_args_buf)?
        {
            let f64_args = &f64_args_buf[..operands.slots.len()];
            if let Some(result) = self.try_execute_cached_f64_slot_specialize_call(
                operands.spec_func_index,
                f64_args,
                inbounds_context,
            )? {
                return Ok(result);
            }

            let args = f64_args.iter().copied().map(Value::F64).collect();
            return self.execute_call_specialize_with_args(
                operands.spec_func_index,
                args,
                inbounds_context,
            );
        }

        let Some(f64_args) = self.load_f64_slot_specialize_values(&operands.slots)? else {
            let Some(args) = self.load_f64_slot_specialize_args(&operands.slots)? else {
                return Ok(DispatchAction::Continue);
            };
            return self.execute_call_specialize_with_args(
                operands.spec_func_index,
                args,
                inbounds_context,
            );
        };

        if let Some(result) = self.try_execute_cached_f64_slot_specialize_call(
            operands.spec_func_index,
            &f64_args,
            inbounds_context,
        )? {
            return Ok(result);
        }

        let args = f64_args.into_iter().map(Value::F64).collect();
        self.execute_call_specialize_with_args(operands.spec_func_index, args, inbounds_context)
    }

    /// Float64 mirror of [`Self::try_execute_cached_i64_slot_specialize_call`]
    /// (Issue #10491).
    fn try_execute_cached_f64_slot_specialize_call(
        &mut self,
        spec_func_index: usize,
        f64_args: &[f64],
        inbounds_context: bool,
    ) -> Result<Option<DispatchAction>, VmError> {
        let dispatch = if let Some(Some(entry)) =
            self.specialization_f64_fast_cache.get(spec_func_index)
        {
            if entry.arity != f64_args.len() {
                return Ok(None);
            }
            match entry.predecoded.as_ref() {
                Some(Some(ResolvedSpecF64Callee::F64(block))) => {
                    if let Some(f64_value) = Self::execute_f64_function_block(block, f64_args) {
                        crate::vm::profiler::record_event("SpecializeF64DispatchCacheHit");
                        if self.try_consume_f64_eq_branch(f64_value) {
                            return Ok(Some(DispatchAction::Continue));
                        }
                        self.stack.push(Value::F64(f64_value));
                        return Ok(Some(DispatchAction::Continue));
                    }
                }
                Some(Some(ResolvedSpecF64Callee::Typed(block))) => {
                    // Disjoint field borrows: `block` borrows the fast
                    // cache immutably while the callee runs against
                    // `self.rng` (the body is effect-free, so the rng is
                    // never actually advanced).
                    if let Some(value) =
                        Self::run_typed_scalar_block_with_f64_args(block, f64_args, &mut self.rng)
                    {
                        crate::vm::profiler::record_event("SpecializeF64DispatchCacheHit");
                        if let Value::F64(v) = value {
                            if self.try_consume_f64_eq_branch(v) {
                                return Ok(Some(DispatchAction::Continue));
                            }
                            self.stack.push(Value::F64(v));
                        } else {
                            self.stack.push(value);
                        }
                        return Ok(Some(DispatchAction::Continue));
                    }
                }
                _ => {}
            }
            entry.dispatch.clone()
        } else {
            let Some(dispatch) = self
                .specialization_f64_cache
                .get(&(spec_func_index, f64_args.len()))
                .cloned()
            else {
                return Ok(None);
            };
            dispatch
        };
        crate::vm::profiler::record_event("SpecializeF64DispatchCacheHit");

        let entry_ip = dispatch.entry;
        let code_end = dispatch.code_end;

        if let Some(f64_value) = self.try_execute_f64_function_call_f64_args(
            entry_ip,
            code_end,
            &dispatch.param_slots,
            f64_args,
        ) {
            if self.try_consume_f64_eq_branch(f64_value) {
                return Ok(Some(DispatchAction::Continue));
            }
            self.stack.push(Value::F64(f64_value));
            return Ok(Some(DispatchAction::Continue));
        }

        // Mixed-type body (e.g. an I64 loop counter): frame-less typed block.
        if let Some(value) = self.try_execute_typed_scalar_function_call_f64_args(
            entry_ip,
            code_end,
            &dispatch.param_slots,
            f64_args,
        ) {
            if let Value::F64(v) = value {
                if self.try_consume_f64_eq_branch(v) {
                    return Ok(Some(DispatchAction::Continue));
                }
                self.stack.push(Value::F64(v));
            } else {
                self.stack.push(value);
            }
            return Ok(Some(DispatchAction::Continue));
        }

        let mut frame =
            self.acquire_frame(dispatch.local_slot_count, Some(dispatch.fallback_index));
        frame.inbounds_context = inbounds_context;
        for (idx, slot) in dispatch.param_slots.iter().enumerate() {
            if let Some(value) = f64_args.get(idx) {
                if !frame.set_slot_f64(*slot, *value) {
                    return Err(super::slot_out_of_bounds("CallSpecializeF64Slots", slot));
                }
            }
        }

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = entry_ip;
        Ok(Some(DispatchAction::Continue))
    }

    /// Float64 mirror of [`Self::record_i64_spec_dispatch`] (Issue #10491):
    /// populate the cheap `(spec_func_index, arity)` dispatch cache for the
    /// all-`F64` specialize hot path. No-op unless every argument is `F64` and
    /// the callee qualifies for the slot-based fast path.
    fn record_f64_spec_dispatch(
        &mut self,
        spec_func_index: usize,
        fallback_index: usize,
        arg_types: &[ValueType],
        fallback_func: &FunctionInfo,
        specialized: &SpecializedCode,
    ) {
        let arity = arg_types.len();
        if !arg_types.iter().all(|t| matches!(t, ValueType::F64)) {
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
            local_slot_count: specialized.local_slot_count,
            param_slots: Rc::from(fallback_func.param_slots.as_slice()),
        };
        self.specialization_f64_cache
            .insert((spec_func_index, arity), dispatch.clone());
        if spec_func_index >= self.specialization_f64_fast_cache.len() {
            self.specialization_f64_fast_cache
                .resize_with(spec_func_index + 1, || None);
        }
        // Predecode once: prefer the pure-F64 block, fall back to the
        // mixed-type frame-less typed block (Issue #10491).
        let predecoded = Some(
            try_predecode_f64_function(
                self.code.as_ref(),
                &self.functions,
                self.base_function_count,
                dispatch.entry,
                dispatch.code_end,
                &dispatch.param_slots,
            )
            .map(ResolvedSpecF64Callee::F64)
            .or_else(|| {
                crate::vm::executable::try_predecode_typed_scalar_function(
                    self.code.as_ref(),
                    &self.functions,
                    dispatch.entry,
                    dispatch.code_end,
                    self.base_function_count,
                    &dispatch.param_slots,
                )
                .map(ResolvedSpecF64Callee::Typed)
            }),
        );
        self.specialization_f64_fast_cache[spec_func_index] = Some(F64SpecFastCacheEntry {
            arity,
            dispatch,
            predecoded,
        });
        self.enforce_specialization_f64_cache_limit();
    }

    /// Narrow mixed-arg mirror of [`Self::record_i64_spec_dispatch`] /
    /// [`Self::record_f64_spec_dispatch`] (Issue #10567 round 2): populate
    /// the `(spec_func_index, arg_types)` dispatch cache a mixed-type-argument
    /// specialize call site's loop-mode fast path
    /// (`TypedLoopOp::CallSpecializeComplexI64Function`) resolves against.
    /// No-op whenever the argument types ARE uniformly I64 or F64 — those
    /// signatures are the two recorders above's exclusive territory — or the
    /// callee does not qualify for the slot-based fast path at all (same
    /// eligibility predicate as the other two recorders). Keyed by the FULL
    /// `arg_types`, not just arity — see the field doc on
    /// `Vm::specialization_mixed_cache` for why an arity-only key is unsound
    /// here. Unlike the I64/F64 recorders, this does not eagerly predecode:
    /// `Vm::resolve_specialize_complex_i64_callee` predecodes lazily (once
    /// per typed-loop block entry, not once per call — the hot path this
    /// narrow op targets already amortizes it), through the shared
    /// `typed_function_cache`.
    fn record_mixed_spec_dispatch(
        &mut self,
        spec_func_index: usize,
        fallback_index: usize,
        arg_types: &[ValueType],
        fallback_func: &FunctionInfo,
        specialized: &SpecializedCode,
    ) {
        let arity = arg_types.len();
        if arg_types.iter().all(|t| matches!(t, ValueType::I64))
            || arg_types.iter().all(|t| matches!(t, ValueType::F64))
        {
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
            local_slot_count: specialized.local_slot_count,
            param_slots: Rc::from(fallback_func.param_slots.as_slice()),
        };
        self.specialization_mixed_cache
            .insert((spec_func_index, arg_types.to_vec()), dispatch);
        self.enforce_specialization_mixed_cache_limit();
    }

    fn try_execute_cached_i64_slot_specialize_call(
        &mut self,
        spec_func_index: usize,
        i64_args: &[i64],
        inbounds_context: bool,
    ) -> Result<Option<DispatchAction>, VmError> {
        // Ultra-cheap monomorphic fast path: a direct Vec lookup by
        // `spec_func_index` with the arity stored inline. This avoids the HashMap
        // lookup on the hot `CallSpecializeI64Slots` path for numeric loops.
        //
        // Run the predecoded block by reference while the cache entry is borrowed;
        // only the cheap `I64SpecDispatch` (param_slots is an Rc) is cloned.
        let dispatch =
            if let Some(Some(entry)) = self.specialization_i64_fast_cache.get(spec_func_index) {
                if entry.arity != i64_args.len() {
                    return Ok(None);
                }
                if let Some(Some(block)) = entry.predecoded.as_ref() {
                    if let Some(i64_value) = Self::execute_i64_function_block(block, i64_args) {
                        crate::vm::profiler::record_event("SpecializeI64DispatchCacheHit");
                        if self.try_consume_i64_eq_branch(i64_value) {
                            return Ok(Some(DispatchAction::Continue));
                        }
                        self.stack.push(Value::I64(i64_value));
                        return Ok(Some(DispatchAction::Continue));
                    }
                }
                entry.dispatch.clone()
            } else {
                // Fall back to the HashMap cache (Issue #8167). A miss there falls
                // through to `execute_call_specialize_with_args`, which populates
                // both caches on the first all-`I64` call.
                let Some(dispatch) = self
                    .specialization_i64_cache
                    .get(&(spec_func_index, i64_args.len()))
                    .cloned()
                else {
                    return Ok(None);
                };
                dispatch
            };
        crate::vm::profiler::record_event("SpecializeI64DispatchCacheHit");

        let entry_ip = dispatch.entry;
        let code_end = dispatch.code_end;

        if let Some(i64_value) = self.try_execute_i64_function_call_i64_args(
            entry_ip,
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
                    return Err(super::slot_out_of_bounds("CallSpecializeI64Slots", slot));
                }
            }
        }

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = entry_ip;
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
            local_slot_count: specialized.local_slot_count,
            param_slots: Rc::from(fallback_func.param_slots.as_slice()),
        };
        self.specialization_i64_cache
            .insert((spec_func_index, arity), dispatch.clone());
        // Also populate the direct-index Vec cache used by the hot path.
        if spec_func_index >= self.specialization_i64_fast_cache.len() {
            self.specialization_i64_fast_cache
                .resize(spec_func_index + 1, None);
        }
        // Predecode once per specialized body and cache the block alongside the
        // dispatch metadata, saving a HashMap probe on every hot-path call.
        let predecoded = Some(try_predecode_i64_function(
            self.code.as_ref(),
            &self.functions,
            self.base_function_count,
            dispatch.entry,
            dispatch.code_end,
            &dispatch.param_slots,
        ));
        self.specialization_i64_fast_cache[spec_func_index] = Some(I64SpecFastCacheEntry {
            arity,
            dispatch,
            predecoded,
        });
        self.enforce_specialization_i64_cache_limit();
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
        if let Some(value) =
            self.try_native_range_call_value(spec_func.fallback_index, &fallback_func, &args)?
        {
            self.stack.push(value);
            return Ok(DispatchAction::Continue);
        }

        let arg_types: Vec<ValueType> = args.iter().map(|v| self.get_value_type(v)).collect();

        let specialization_supported =
            runtime_specialization_supported_for_function(&fallback_func, &arg_types, &self.code)
                && !self
                    .repl_world_sensitive_specializable_indices
                    .contains(&spec_func_index);
        let key = SpecializationKey {
            func_index: spec_func_index,
            arg_types: arg_types.clone(),
        };

        let specialized = if !specialization_supported {
            None
        } else if let Some(cached) = self.specialization_cache.get(&key) {
            Some(cached.clone())
        } else if self.specialization_failure_cache.contains(&key) {
            // Negative cache hit (Issue #8603): this signature already failed to
            // specialize; skip the (expensive) re-attempt and run the fallback.
            crate::vm::profiler::record_event("SpecializeFailureCacheHit");
            None
        } else if self.compile_context.is_some() {
            let type_object_names = specialize::collect_type_object_names(
                &self.struct_defs,
                self.compile_context.as_ref(),
                &self.abstract_types,
            );
            let module_path = specialize::module_path_from_function_name(&fallback_func.name);
            let callable_registry = self.specializable_callable_registry();
            let recursion_guard = RefCell::new(SpecializationRecursionGuard::new());
            match specialize::specialize_function_with_callees(
                &spec_func.ir,
                &arg_types,
                &self.struct_defs,
                &type_object_names,
                module_path,
                self.disable_array_getindex_specialization(),
                self.disable_field_access_specialization(),
                &callable_registry,
                &recursion_guard,
                Some(spec_func_index),
            ) {
                Ok(result) => {
                    let (entry_point, appended_len, local_slot_count) =
                        self.install_specialized_body(result.code, &fallback_func, &arg_types);
                    let specialized = SpecializedCode {
                        entry: entry_point,
                        return_type: result.return_type,
                        code_len: appended_len,
                        local_slot_count,
                    };
                    self.specialization_cache.insert(key, specialized.clone());
                    self.enforce_specialization_cache_limit();
                    Some(specialized)
                }
                Err(_) => {
                    // Remember the failure so later calls with the same
                    // signature skip straight to the fallback (Issue #8603).
                    self.specialization_failure_cache.insert(key);
                    self.enforce_specialization_failure_cache_limit();
                    None
                }
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
            // Issue #10491: F64 mirror — the arg-type predicates make the two
            // recorders mutually exclusive, so at most one populates its cache.
            self.record_f64_spec_dispatch(
                spec_func_index,
                spec_func.fallback_index,
                &arg_types,
                &fallback_func,
                specialized_code,
            );
            // Issue #10567 (round 2): narrow mixed-arg mirror — mutually
            // exclusive with both recorders above by construction (their
            // arg-type predicates partition into "all I64" / "all F64" /
            // "neither").
            self.record_mixed_spec_dispatch(
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
            if let Some(f64_value) = self.try_execute_f64_function_call(
                specialized_code.entry,
                specialized_code.entry + specialized_code.code_len,
                &fallback_func.param_slots,
                &args,
            ) {
                crate::vm::profiler::record_event("CallDirectFastF64FunctionHit");
                if self.try_consume_f64_eq_branch(f64_value) {
                    return Ok(DispatchAction::Continue);
                }
                self.stack.push(Value::F64(f64_value));
                return Ok(DispatchAction::Continue);
            }
            // Issue #10567/#10704: neither fast path above fires for a
            // genuinely mixed-type argument list (e.g. a boxed `ComplexF64`
            // struct plus an `Int64` counter, as at `mandel_point(c,
            // maxiter)`'s call site) since both require every argument to be
            // the same scalar `Value` variant. Try the frame-less typed
            // scalar block path, which binds mixed I64/F64/ComplexF64
            // arguments directly (no boxing round-trip) when the specialized
            // body predecodes as a `TypedScalarFunctionBlock`.
            if let Some(value) = self.try_execute_typed_scalar_function_call_from_values(
                specialized_code.entry,
                specialized_code.entry + specialized_code.code_len,
                &fallback_func.param_slots,
                &args,
            ) {
                self.stack.push(value);
                return Ok(DispatchAction::Continue);
            }
        }

        let frame_slot_count = if let Some(specialized_code) = &specialized {
            specialized_code.local_slot_count
        } else {
            fallback_func.local_slot_count
        };
        let mut frame = self.acquire_frame(frame_slot_count, Some(spec_func.fallback_index));
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
        if self.bind_kwargs_defaults_or_handle(&fallback_func, &mut frame)? {
            return Ok(DispatchAction::Continue);
        }

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
            is_lowering_helper: false,
            definition_order: 0,
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
            suppress_short_name_alias: false,
            shared_plan: None,
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
mod transient_root_scope_tests {
    use crate::rng::StableRng;
    use crate::test_runtime::compile_source_with_cache;
    use crate::vm::{Instr, Value, Vm};

    #[test]
    fn ordinary_keyword_calls_do_not_leak_transient_roots_11372() {
        let source = r#"
f11372(x; y = 0) = x + y

function run11372()
    total = 0
    for i in 1:8
        total += f11372(i; y = 1)
    end
    total
end

run11372()
"#;
        let compiled = compile_source_with_cache(source);
        assert!(
            compiled
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::CallWithKwargs(..))),
            "regression must exercise the ordinary keyword-call opcode"
        );

        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run ordinary keyword-call loop");
        assert!(matches!(result, Value::I64(44)));
        assert!(
            vm.transient_roots.is_empty(),
            "ordinary non-splat calls must not retain transient GC roots"
        );
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
    use crate::rng::StableRng;
    use crate::test_runtime::compile_source_with_cache;
    use crate::vm::{Instr, Vm};
    use subset_julia_vm_bytecode::CompiledProgram;

    fn compile_source(source: &str) -> CompiledProgram {
        compile_source_with_cache(source)
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
