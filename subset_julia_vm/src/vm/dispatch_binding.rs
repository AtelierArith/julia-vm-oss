//! Runtime dispatch type-parameter binding helpers (Issue #6334: extracted
//! from `vm/mod.rs`): parsing `Val{...}` type parameters into values and
//! binding value-level type params (array ranks, `Val` payloads) into the
//! callee frame.
//!
//! The binding/checking of `where` type vars against runtime argument types
//! moved to `inference_core::dispatch_resolver`
//! (`runtime_value_type_matches_param_with_bindings`, Issue #5915).

use super::{frame, Frame, FunctionInfo, SymbolValue, TupleValue, Value};
use half::f16;

pub(super) fn parse_val_char_parameter(type_arg: &str) -> Option<char> {
    if !(type_arg.starts_with('\'') && type_arg.ends_with('\'')) {
        return None;
    }

    let content = &type_arg[1..type_arg.len() - 1];
    let bytes = content.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    if bytes[0] != b'\\' {
        let mut chars = content.chars();
        let first = chars.next()?;
        return chars.next().is_none().then_some(first);
    }

    if bytes.len() < 2 {
        return None;
    }

    match bytes[1] {
        b'n' if bytes.len() == 2 => Some('\n'),
        b'r' if bytes.len() == 2 => Some('\r'),
        b't' if bytes.len() == 2 => Some('\t'),
        b'\\' if bytes.len() == 2 => Some('\\'),
        b'\'' if bytes.len() == 2 => Some('\''),
        b'"' if bytes.len() == 2 => Some('"'),
        b'a' if bytes.len() == 2 => Some('\x07'),
        b'b' if bytes.len() == 2 => Some('\x08'),
        b'f' if bytes.len() == 2 => Some('\x0c'),
        b'v' if bytes.len() == 2 => Some('\x0b'),
        b'e' if bytes.len() == 2 => Some('\x1b'),
        b'$' if bytes.len() == 2 => Some('$'),
        b'x' => {
            let hex_part = &content[2..];
            if hex_part.is_empty() || hex_part.len() > 2 {
                return None;
            }
            if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex_part, 16)
                .ok()
                .and_then(char::from_u32)
        }
        b'u' => {
            let hex_part = &content[2..];
            if hex_part.is_empty() || hex_part.len() > 4 {
                return None;
            }
            if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex_part, 16)
                .ok()
                .and_then(char::from_u32)
        }
        b'U' => {
            let hex_part = &content[2..];
            if hex_part.is_empty() || hex_part.len() > 8 {
                return None;
            }
            if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex_part, 16)
                .ok()
                .and_then(char::from_u32)
        }
        b'0'..=b'7' => {
            let oct_part = &content[1..];
            if oct_part.is_empty() || oct_part.len() > 3 {
                return None;
            }
            if !oct_part.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                return None;
            }
            u32::from_str_radix(oct_part, 8)
                .ok()
                .and_then(char::from_u32)
        }
        _ => None,
    }
}

pub(super) fn parse_val_tuple_parameter(type_arg: &str) -> Option<TupleValue> {
    if !(type_arg.starts_with('(') && type_arg.ends_with(')')) {
        return None;
    }

    let inner = &type_arg[1..type_arg.len() - 1];
    if inner.trim().is_empty() {
        return Some(TupleValue::new(Vec::new()));
    }

    let mut elements = Vec::new();
    for part in split_val_tuple_elements(inner) {
        let value = parse_val_tuple_element(part)?;
        elements.push(value);
    }
    Some(TupleValue::new(elements))
}

fn split_val_tuple_elements(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut brace_depth = 0;
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }

        match c {
            '\'' | '"' => quote = Some(c),
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                let part = s[start..i].trim();
                if !part.is_empty() {
                    result.push(part);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let part = s[start..].trim();
    if !part.is_empty() {
        result.push(part);
    }
    result
}

fn parse_val_tuple_element(type_arg: &str) -> Option<Value> {
    if let Some(value) = parse_val_constructor_parameter(type_arg) {
        return Some(value);
    }
    if let Ok(int_val) = type_arg.parse::<i64>() {
        return Some(Value::I64(int_val));
    }
    if let Ok(float_val) = type_arg.parse::<f64>() {
        return Some(Value::F64(float_val));
    }
    if type_arg == "true" {
        return Some(Value::Bool(true));
    }
    if type_arg == "false" {
        return Some(Value::Bool(false));
    }
    if let Some(char_val) = parse_val_char_parameter(type_arg) {
        return Some(Value::Char(char_val));
    }
    if type_arg.starts_with(':') {
        return Some(Value::Symbol(SymbolValue::new(
            type_arg.trim_start_matches(':'),
        )));
    }
    parse_val_tuple_parameter(type_arg).map(Value::Tuple)
}

pub(super) fn parse_val_constructor_parameter(type_arg: &str) -> Option<Value> {
    let (type_name, rest) = type_arg.split_once('(')?;
    let type_name = type_name.trim();
    let inner = rest.strip_suffix(')')?.trim();

    match type_name {
        "Int8" => inner
            .parse::<i64>()
            .ok()
            .and_then(|v| i8::try_from(v).ok())
            .map(Value::I8),
        "Int16" => inner
            .parse::<i64>()
            .ok()
            .and_then(|v| i16::try_from(v).ok())
            .map(Value::I16),
        "Int32" => inner
            .parse::<i64>()
            .ok()
            .and_then(|v| i32::try_from(v).ok())
            .map(Value::I32),
        "Int" if crate::types::native_int_type_name() == "Int32" => inner
            .parse::<i64>()
            .ok()
            .and_then(|v| i32::try_from(v).ok())
            .map(Value::I32),
        "Int64" | "Int" => inner.parse::<i64>().ok().map(Value::I64),
        "Int128" => inner.parse::<i128>().ok().map(Value::I128),
        "UInt8" => inner
            .parse::<u128>()
            .ok()
            .and_then(|v| u8::try_from(v).ok())
            .map(Value::U8),
        "UInt16" => inner
            .parse::<u128>()
            .ok()
            .and_then(|v| u16::try_from(v).ok())
            .map(Value::U16),
        "UInt32" => inner
            .parse::<u128>()
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .map(Value::U32),
        "UInt" if crate::types::native_uint_type_name() == "UInt32" => inner
            .parse::<u128>()
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .map(Value::U32),
        "UInt64" | "UInt" => inner.parse::<u64>().ok().map(Value::U64),
        "UInt128" => inner.parse::<u128>().ok().map(Value::U128),
        "Float16" => inner
            .parse::<f32>()
            .ok()
            .map(|v| Value::F16(f16::from_f32(v))),
        "Float32" => inner.parse::<f32>().ok().map(Value::F32),
        "Float64" | "Float" => inner.parse::<f64>().ok().map(Value::F64),
        "Bool" => match inner {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            "0" => Some(Value::Bool(false)),
            "1" => Some(Value::Bool(true)),
            _ => None,
        },
        _ => None,
    }
}

/// Split a type-parameter argument list on its first top-level comma, treating
/// `{...}` braces as nesting so that `N,NTuple{M,T}` splits into `N` and
/// `NTuple{M,T}` rather than at the inner comma (Issue #4842).
pub(super) fn split_top_level_comma(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

pub(super) fn bind_array_rank_type_param(
    frame: &mut Frame,
    param_jtype: &crate::types::JuliaType,
    arg_jtype: &crate::types::JuliaType,
    func: &FunctionInfo,
) {
    let crate::types::JuliaType::Struct(param_type_name) = param_jtype else {
        return;
    };
    let base = param_type_name
        .find('{')
        .map_or(param_type_name.as_str(), |brace_idx| {
            &param_type_name[..brace_idx]
        });
    let base = base.rsplit('.').next().unwrap_or(base);
    if !matches!(base, "Array" | "AbstractArray") {
        return;
    }

    let params = crate::vm::util::parse_parametric_params(param_type_name);
    let Some(rank_arg) = params.get(1).map(|arg| arg.trim()) else {
        return;
    };
    if !func.type_params.iter().any(|tp| tp.name == rank_arg) {
        return;
    }
    let Some(rank) = arg_jtype.array_type_ndims() else {
        return;
    };
    let rank = i64::try_from(rank).unwrap_or(i64::MAX);
    bind_val_parameter_value(frame, rank_arg, Value::I64(rank));
}

pub(super) fn bind_val_parameter_value(frame: &mut Frame, name: &str, value: Value) {
    match value {
        Value::I64(v) => {
            frame.locals_any.insert(name.to_string(), Value::I64(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::I64);
        }
        Value::F64(v) => {
            frame.locals_any.insert(name.to_string(), Value::F64(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::F64);
        }
        Value::F32(v) => {
            frame.locals_any.insert(name.to_string(), Value::F32(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::F32);
        }
        Value::F16(v) => {
            frame.locals_any.insert(name.to_string(), Value::F16(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::F16);
        }
        Value::Bool(v) => {
            frame.locals_any.insert(name.to_string(), Value::Bool(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::Bool);
        }
        Value::Char(v) => {
            frame.locals_any.insert(name.to_string(), Value::Char(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::Char);
        }
        Value::Tuple(v) => {
            frame.locals_any.insert(name.to_string(), Value::Tuple(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::Tuple);
        }
        Value::Symbol(v) => {
            frame.locals_any.insert(name.to_string(), Value::Symbol(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::ValSymbol);
        }
        Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I128(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_) => {
            frame.locals_any.insert(name.to_string(), value);
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::NarrowInt);
        }
        other => {
            frame.locals_any.insert(name.to_string(), other);
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::Any);
        }
    }
}

/// Expand a function's declared parameter `JuliaType`s into the per-call
/// signature for `arg_len` arguments (repeating the vararg element type),
/// returning `None` when the function cannot accept that arity.
///
/// This is the runtime-side source of truth for candidate signatures in the
/// #6336/#6496 structured-payload migration: `Instr` payloads carry only
/// candidate function indices, and dispatch handlers derive the historical
/// type-name signature from each candidate's `FunctionInfo` through this
/// expansion instead of deserializing baked name strings.
pub(crate) fn expanded_param_types_for_call(
    func: &FunctionInfo,
    arg_len: usize,
) -> Option<Vec<crate::types::JuliaType>> {
    let Some(vararg_idx) = func.vararg_param_index else {
        return (func.param_julia_types.len() == arg_len).then(|| func.param_julia_types.clone());
    };

    if arg_len < vararg_idx {
        return None;
    }
    if let Some(fixed_count) = func.vararg_fixed_count {
        if arg_len != vararg_idx + fixed_count {
            return None;
        }
    }

    let vararg_ty = func
        .param_julia_types
        .get(vararg_idx)
        .cloned()
        .unwrap_or(crate::types::JuliaType::Any);
    let vararg_ty = vararg_dispatch_element_type(vararg_ty);
    let mut expanded: Vec<_> = func
        .param_julia_types
        .iter()
        .take(vararg_idx)
        .cloned()
        .collect();
    for _ in vararg_idx..arg_len {
        expanded.push(vararg_ty.clone());
    }
    Some(expanded)
}

/// Derive the runtime type-name signature (one rendered type name per
/// argument) for calling `func` with `arity` arguments.
///
/// Equality with the canonical compile-time `MethodSig` projection is pinned
/// by the Base-corpus gate
/// `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495`
/// in `compile/cache.rs`.
#[cfg(test)]
pub(crate) fn derived_runtime_signature(func: &FunctionInfo, arity: usize) -> Option<Vec<String>> {
    Some(
        expanded_param_types_for_call(func, arity)?
            .iter()
            .map(render_runtime_candidate_type)
            .collect(),
    )
}

fn render_runtime_candidate_type(ty: &crate::types::JuliaType) -> String {
    let core = crate::inference_core::CoreType::from(ty);
    crate::inference_core::core_type_to_julia_type(&core).to_string()
}

fn method_type_param_declares(name: &str, type_params: &[crate::types::TypeParam]) -> bool {
    type_params.iter().any(|tp| tp.name == name)
}

fn concrete_leaf_struct_core(name: &str) -> crate::inference_core::CoreType {
    crate::inference_core::CoreType::Struct {
        name: name.to_string(),
        params: vec![],
    }
}

fn candidate_leaf_struct_core(
    ty: &crate::types::JuliaType,
    base_core: &crate::inference_core::CoreType,
    type_params: &[crate::types::TypeParam],
) -> Option<crate::inference_core::CoreType> {
    use crate::inference_core::CoreType;
    use crate::types::JuliaType;

    let name = match ty {
        JuliaType::Struct(name) if !name.contains('{') => name.as_str(),
        JuliaType::TypeVar(name, None) => name.as_str(),
        _ => return None,
    };
    if method_type_param_declares(name, type_params) {
        return None;
    }
    matches!(
        base_core,
        CoreType::TypeVar(var)
            if var.name == name && var.upper_bound.is_none() && var.lower_bound.is_none()
    )
    .then(|| concrete_leaf_struct_core(name))
}

fn runtime_candidate_slot_core_type(
    ty: &crate::types::JuliaType,
    rendered: &str,
    type_params: &[crate::types::TypeParam],
) -> crate::inference_core::CoreType {
    let base_core =
        crate::inference_core::dispatch_resolver::runtime_candidate_core_type(ty, rendered);
    candidate_leaf_struct_core(ty, &base_core, type_params).unwrap_or(base_core)
}

pub(crate) fn runtime_actual_core_type(
    ty: &crate::types::JuliaType,
) -> crate::inference_core::CoreType {
    use crate::inference_core::CoreType;
    use crate::types::JuliaType;

    let base_core = crate::inference_core::CoreType::from(ty);
    if let JuliaType::Struct(name) = ty {
        if !name.contains('{')
            && matches!(
                &base_core,
                CoreType::TypeVar(var)
                    if var.name == *name && var.upper_bound.is_none() && var.lower_bound.is_none()
            )
        {
            return concrete_leaf_struct_core(name);
        }
    }
    base_core
}

/// Structured runtime candidate signature for the `core_signature`-based
/// dynamic dispatch path (Issue #6502 slice 2).
///
/// `rendered` keeps the historical per-slot rendered type names (still used
/// by the VM representation fences and MethodError display); `slots` carries
/// the structured per-slot [`CoreType`]s with `where` bounds embedded; and
/// `signature` carries the full `core_signature`-shaped form when the method
/// has `where` parameters (used as the cross-slot consistency gate,
/// Issue #6536).
#[derive(Debug, Clone)]
pub(crate) struct RuntimeCandidateCoreSignature {
    pub rendered: Vec<String>,
    pub slots: Vec<crate::inference_core::CoreType>,
    pub signature: Option<crate::inference_core::CoreType>,
}

/// Build a [`RuntimeCandidateCoreSignature`] from a method's declared per-call
/// parameter types and `where` parameters.
pub(crate) fn build_runtime_candidate_core_signature(
    param_types: &[crate::types::JuliaType],
    type_params: &[crate::types::TypeParam],
) -> RuntimeCandidateCoreSignature {
    use crate::inference_core::dispatch_resolver::{
        embed_type_param_bounds, runtime_core_signature,
    };

    let rendered: Vec<String> = param_types
        .iter()
        .map(render_runtime_candidate_type)
        .collect();
    let slots: Vec<crate::inference_core::CoreType> = param_types
        .iter()
        .zip(rendered.iter())
        .map(|(jt, name)| {
            embed_type_param_bounds(
                runtime_candidate_slot_core_type(jt, name, type_params),
                type_params,
            )
        })
        .collect();
    let signature = (!type_params.is_empty()).then(|| runtime_core_signature(&slots, type_params));
    RuntimeCandidateCoreSignature {
        rendered,
        slots,
        signature,
    }
}

pub(super) fn vararg_dispatch_element_type(ty: crate::types::JuliaType) -> crate::types::JuliaType {
    match ty {
        crate::types::JuliaType::TupleOf(elements) if elements.len() == 1 => elements
            .into_iter()
            .next()
            .unwrap_or(crate::types::JuliaType::Any),
        crate::types::JuliaType::Struct(name) if name.starts_with("Tuple{") => {
            let params = crate::vm::util::parse_parametric_params(&name);
            if params.len() == 1 {
                crate::types::JuliaType::from_name_or_struct(params[0].trim())
            } else {
                crate::types::JuliaType::Struct(name)
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_runtime_candidate_core_signature, runtime_actual_core_type};
    use crate::inference_core::{CoreType, CoreTypeVar};
    use crate::types::{JuliaType, TypeParam};

    #[test]
    fn runtime_signature_keeps_undeclared_typevar_like_struct_concrete_issue_5314() {
        let sig = build_runtime_candidate_core_signature(
            &[
                JuliaType::Struct("Q5314".to_string()),
                JuliaType::TypeVar("Q5314".to_string(), None),
            ],
            &[],
        );

        assert_eq!(
            sig.slots,
            vec![
                CoreType::Struct {
                    name: "Q5314".to_string(),
                    params: vec![],
                },
                CoreType::Struct {
                    name: "Q5314".to_string(),
                    params: vec![],
                },
            ]
        );
        assert_eq!(
            runtime_actual_core_type(&JuliaType::Struct("Q5314".to_string())),
            CoreType::Struct {
                name: "Q5314".to_string(),
                params: vec![],
            }
        );
    }

    #[test]
    fn runtime_signature_preserves_declared_where_typevar() {
        let sig = build_runtime_candidate_core_signature(
            &[JuliaType::TypeVar("T".to_string(), None)],
            &[TypeParam::new("T".to_string())],
        );

        assert_eq!(
            sig.slots,
            vec![CoreType::TypeVar(CoreTypeVar {
                name: "T".to_string(),
                lower_bound: None,
                upper_bound: None,
            })]
        );
    }
}
