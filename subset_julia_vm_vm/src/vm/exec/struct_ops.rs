//! Struct operations for the VM.
//!
//! This module handles struct instructions:
//! - NewStruct, NewStructSplat, NewParametricStruct, NewDynamicParametricStruct
//! - LoadStruct, StoreStruct
//! - GetField, GetExprField, SetField
//! - ReturnStruct

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;
use crate::vm::value::is_native_array_value;

use super::super::error::VmError;
use super::super::frame::VarTypeTag;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util;
use super::super::value::{
    is_array_wrapper_struct_name, native_array_ref_value as array_value, native_array_value_ref,
    BindingFieldAccess, BindingValue, MemoryRefValue, RuntimeTypeNameValue, RuntimeTypeVarValue,
    StaticElem, StaticRealValue, StructInstance, SymbolValue, TupleValue, Value,
};
use crate::inference_core::{CoreSubtypeEngine, CoreType};
use crate::types::{JuliaType, TypeExpr, TypeParam};
use crate::vm::splat::SplatPreparation;
use crate::vm::type_objects::RuntimeTypeRegistry;
use std::collections::{HashMap, HashSet};

/// Render a `Symbol` value used as a parametric type parameter, matching
/// upstream Julia's spelling (Issue #5291): an identifier-like symbol renders in
/// colon form (`Val{:up}`, `MIME{:html}`), while a non-identifier symbol keeps
/// the `Symbol("...")` form (`MIME{Symbol("text/plain")}`). Using the same
/// spelling the source-literal parameter uses (`::Val{:up}`) is what lets
/// `isa`/dispatch match the runtime value's type against the method parameter.
fn render_symbol_type_param(sym: &SymbolValue) -> String {
    let s = sym.as_str();
    if crate::vm::builtins_macro::helpers::is_valid_identifier(s) {
        format!(":{}", s)
    } else {
        format!("Symbol(\"{}\")", s)
    }
}

fn render_f64_type_param(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "Inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Inf".to_string()
    } else {
        value.to_string()
    }
}

fn render_f32_type_param(value: f32) -> String {
    if value.is_nan() {
        "Float32(NaN)".to_string()
    } else if value == f32::INFINITY {
        "Float32(Inf)".to_string()
    } else if value == f32::NEG_INFINITY {
        "Float32(-Inf)".to_string()
    } else {
        format!("Float32({})", value)
    }
}

fn render_char_type_param(value: char) -> String {
    let escaped = match value {
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\'' => "\\'".to_string(),
        '\\' => "\\\\".to_string(),
        c => c.to_string(),
    };
    format!("'{escaped}'")
}

fn render_tuple_type_param(tuple: &TupleValue) -> Option<String> {
    let rendered = tuple
        .elements
        .iter()
        .map(render_value_type_param)
        .collect::<Option<Vec<_>>>()?;
    let suffix = if rendered.len() == 1 { "," } else { "" };
    Some(format!("({}{suffix})", rendered.join(", ")))
}

fn render_value_type_param(value: &Value) -> Option<String> {
    Some(match value {
        Value::DataType(jt) => jt.name().to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::U8(n) => format!("0x{n:02x}"),
        Value::U16(n) => format!("0x{n:04x}"),
        Value::U32(n) => format!("0x{n:08x}"),
        Value::U64(n) => format!("0x{n:016x}"),
        Value::U128(n) => format!("0x{n:032x}"),
        Value::F64(n) => render_f64_type_param(*n),
        Value::F32(n) => render_f32_type_param(*n),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => render_char_type_param(*c),
        Value::Symbol(sym) => render_symbol_type_param(sym),
        Value::Tuple(tuple) => render_tuple_type_param(tuple)?,
        _ => return None,
    })
}

/// Render a narrow-width numeric value used as a *parametric type parameter*'s
/// name (Issue #10599). This is the single source of truth shared by
/// `build_parametric_type` (the type-value / `Core.apply_type` path) and
/// `NewDynamicParametricStruct` (the `VP{v}()` runtime-construction path), so
/// the two paths render an identical type name and stay `===`-consistent.
///
/// Signed integers and `Float16` use the round-trippable constructor spelling
/// (`Int8(5)`, `Float16(1.5)`) — mirroring the existing `Float32(...)` form —
/// so the method body recovers the exact value AND its narrow type instead of
/// an `Int64`/`DataType` wrapper. Unsigned integers use the hex spelling
/// upstream Julia displays (`0x05`, `0x0005`, …); the digit count encodes the
/// `UIntN` width and `parse_value_type_param_literal` decodes it back.
fn render_narrow_numeric_type_param(value: &Value) -> Option<String> {
    Some(match value {
        Value::I8(n) => format!("Int8({n})"),
        Value::I16(n) => format!("Int16({n})"),
        Value::I32(n) => format!("Int32({n})"),
        Value::I128(n) => format!("Int128({n})"),
        Value::U8(n) => format!("0x{n:02x}"),
        Value::U16(n) => format!("0x{n:04x}"),
        Value::U32(n) => format!("0x{n:08x}"),
        Value::U64(n) => format!("0x{n:016x}"),
        Value::U128(n) => format!("0x{n:032x}"),
        Value::F16(n) => {
            let v = n.to_f32();
            if v.is_nan() {
                "Float16(NaN)".to_string()
            } else if v == f32::INFINITY {
                "Float16(Inf)".to_string()
            } else if v == f32::NEG_INFINITY {
                "Float16(-Inf)".to_string()
            } else {
                format!("Float16({v})")
            }
        }
        _ => return None,
    })
}

fn render_runtime_typevar_type_param(tv: &RuntimeTypeVarValue) -> String {
    use crate::types::JuliaType;

    if tv.name != "_" {
        return tv.name.clone();
    }

    let has_lower = !matches!(tv.lower_bound, JuliaType::Bottom);
    let has_upper = !matches!(tv.upper_bound, JuliaType::Any);
    match (has_lower, has_upper) {
        (false, false) => tv.name.clone(),
        (false, true) => format!("<:{}", tv.upper_bound.name()),
        (true, false) => format!(">:{}", tv.lower_bound.name()),
        (true, true) => format!(
            "{}<:{}<:{}",
            tv.lower_bound.name(),
            tv.name,
            tv.upper_bound.name()
        ),
    }
}

fn type_arg_value_to_julia_type(value: &Value) -> Option<crate::types::JuliaType> {
    match value {
        Value::DataType(jt) => Some(*jt.clone()),
        Value::RuntimeTypeVar(tv) => tv
            .source_anonymous_projection()
            .or_else(|| Some(tv.projection())),
        Value::I8(n) => Some(JuliaType::Struct(n.to_string())),
        Value::I16(n) => Some(JuliaType::Struct(n.to_string())),
        Value::I32(n) => Some(JuliaType::Struct(n.to_string())),
        Value::I64(n) => Some(JuliaType::Struct(n.to_string())),
        Value::I128(n) => Some(JuliaType::Struct(n.to_string())),
        Value::U8(n) => Some(JuliaType::Struct(n.to_string())),
        Value::U16(n) => Some(JuliaType::Struct(n.to_string())),
        Value::U32(n) => Some(JuliaType::Struct(n.to_string())),
        Value::U64(n) => Some(JuliaType::Struct(n.to_string())),
        Value::U128(n) => Some(JuliaType::Struct(n.to_string())),
        Value::F64(n) => Some(JuliaType::Struct(render_f64_type_param(*n))),
        Value::F32(n) => Some(JuliaType::Struct(render_f32_type_param(*n))),
        Value::Bool(b) => Some(JuliaType::Struct(b.to_string())),
        Value::Char(c) => Some(JuliaType::Struct(render_char_type_param(*c))),
        Value::Symbol(sym) => Some(JuliaType::Struct(render_symbol_type_param(sym))),
        Value::Tuple(tuple) => Some(JuliaType::Struct(render_tuple_type_param(tuple)?)),
        _ => None,
    }
}

fn has_free_runtime_typevar(ty: &JuliaType, bound_ids: &mut Vec<u64>) -> bool {
    match ty {
        JuliaType::RuntimeTypeVar { id, .. } => !bound_ids.contains(id),
        JuliaType::RuntimeUnionAll { var, body } => {
            let JuliaType::RuntimeTypeVar {
                id,
                lower_bound,
                upper_bound,
                ..
            } = var.as_ref()
            else {
                return has_free_runtime_typevar(body, bound_ids);
            };
            if has_free_runtime_typevar(lower_bound, bound_ids)
                || has_free_runtime_typevar(upper_bound, bound_ids)
            {
                return true;
            }
            bound_ids.push(*id);
            let free = has_free_runtime_typevar(body, bound_ids);
            bound_ids.pop();
            free
        }
        JuliaType::RuntimeParametric { params, .. }
        | JuliaType::TupleOf(params)
        | JuliaType::Union(params) => params
            .iter()
            .any(|ty| has_free_runtime_typevar(ty, bound_ids)),
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            has_free_runtime_typevar(inner, bound_ids)
        }
        JuliaType::UnionAll { body, .. } => has_free_runtime_typevar(body, bound_ids),
        _ => false,
    }
}

fn record_runtime_type_binding(
    name: &str,
    actual: &JuliaType,
    inferred: &mut HashMap<String, JuliaType>,
) -> Result<(), VmError> {
    if let Some(existing) = inferred.get(name) {
        if existing == actual {
            return Ok(());
        }
        if *existing != JuliaType::Any && *actual != JuliaType::Any {
            return Err(VmError::TypeError(format!(
                "Inconsistent type inference for {}: {} vs {}",
                name, existing, actual
            )));
        }
        if *actual == JuliaType::Any {
            return Ok(());
        }
    }
    inferred.insert(name.to_string(), actual.clone());
    Ok(())
}

fn bind_runtime_field_type_vars(
    type_expr: &TypeExpr,
    actual: &JuliaType,
    type_params: &[TypeParam],
    inferred: &mut HashMap<String, JuliaType>,
) -> Result<(), VmError> {
    if let TypeExpr::TypeVar(name) = type_expr {
        if type_params.iter().any(|param| param.name == *name) {
            record_runtime_type_binding(name, actual, inferred)?;
        }
        return Ok(());
    }

    let pattern = type_expr.to_julia_type_lossy();
    let Some(bindings) = actual.extract_type_bindings(&pattern, type_params) else {
        return Ok(());
    };
    for (name, ty) in bindings {
        record_runtime_type_binding(&name, &ty, inferred)?;
    }
    Ok(())
}

fn bind_runtime_type_vars_from_param_bounds(
    type_params: &[TypeParam],
    inferred: &mut HashMap<String, JuliaType>,
) -> Result<(), VmError> {
    let mut changed = true;
    while changed {
        changed = false;
        for param in type_params {
            let Some(actual) = inferred.get(&param.name).cloned() else {
                continue;
            };
            let Some(bound_name) = param.get_upper_bound() else {
                continue;
            };
            let bound_pattern = JuliaType::from_name_or_struct(bound_name);
            let Some(bindings) = actual.extract_type_bindings(&bound_pattern, type_params) else {
                continue;
            };
            for (name, ty) in bindings {
                let previous = inferred.get(&name).cloned();
                record_runtime_type_binding(&name, &ty, inferred)?;
                if previous.as_ref() != inferred.get(&name) {
                    changed = true;
                }
            }
        }
    }
    Ok(())
}

fn bitarray_rank_arg(value: &Value) -> Option<usize> {
    match value {
        Value::I8(n) => usize::try_from(*n).ok(),
        Value::I16(n) => usize::try_from(*n).ok(),
        Value::I32(n) => usize::try_from(*n).ok(),
        Value::I64(n) => usize::try_from(*n).ok(),
        Value::I128(n) => usize::try_from(*n).ok(),
        Value::U8(n) => Some(usize::from(*n)),
        Value::U16(n) => Some(usize::from(*n)),
        Value::U32(n) => usize::try_from(*n).ok(),
        Value::U64(n) => usize::try_from(*n).ok(),
        Value::U128(n) => usize::try_from(*n).ok(),
        _ => None,
    }
}
/// Try to produce a compact StaticArray for small <:Real SVector/SMatrix
/// constructions (Issue #7964 Phase 1+3).
///
/// Matches `struct_name` against:
/// - `"SVector{N, T}"` with N ≤ 4, T any supported Real type
/// - `"SMatrix{M, N, T, L}"` with M, N ≤ 4, L == M*N, T any supported Real
///   type — the bundled struct's upstream-shaped fourth length parameter
///   (Issue #11432)
///
/// Expects `values` to contain exactly one element that is a `Value::Tuple`
/// whose elements are all the same Real variant — i.e. the single
/// `data::Tuple` field of the SVector/SMatrix struct.
///
/// Returns `None` for any other struct or unsupported element type, falling
/// back to the normal `StructRef` path.
fn try_make_static_array(struct_name: &str, values: &[Value]) -> Option<Value> {
    // Strip optional module prefix (e.g. "StaticArrays.SVector{...}" → "SVector{...}").
    let bare_name = struct_name
        .strip_prefix("StaticArrays.")
        .unwrap_or(struct_name);

    // SVector{N, T}
    if let Some(rest) = bare_name.strip_prefix("SVector{") {
        if let Some(inner) = rest.strip_suffix('}') {
            let (n_part, elem_name) = inner.rsplit_once(", ")?;
            let n: usize = n_part.parse().ok()?;
            if n > 4 {
                return None;
            }
            let elems = extract_real_tuple(values, n, elem_name)?;
            // Phase 3: prefer zero-allocation inline storage for N≤4.
            if let Some(inline) =
                crate::vm::value::StaticArrayInlineData::try_from_elem(n, 1, &elems)
            {
                return Some(Value::StaticArrayInline(inline));
            }
            return Some(Value::StaticArray(Box::new(StaticRealValue::new_vector(
                struct_name,
                elems,
            ))));
        }
    }

    // SMatrix{M, N, T, L}
    if let Some(rest) = bare_name.strip_prefix("SMatrix{") {
        if let Some(inner) = rest.strip_suffix('}') {
            // inner is "M, N, T, L" (Issue #11432): the bundled struct now
            // declares the upstream fourth length parameter L == M*N, so the
            // instantiated type name always carries all four concrete
            // parameters. Peel L off first, then parse M, N, and the element
            // type name exactly as before.
            let (mnt_part, l_str) = inner.rsplit_once(", ")?;
            let l: usize = l_str.parse().ok()?;
            let (mn_part, elem_name) = mnt_part.rsplit_once(", ")?;
            let (m_str, n_str) = mn_part.split_once(", ")?;
            let m: usize = m_str.parse().ok()?;
            let n: usize = n_str.parse().ok()?;
            if m > 4 || n > 4 || l != m * n {
                return None;
            }
            let elems = extract_real_tuple(values, m * n, elem_name)?;
            // Phase 3: prefer zero-allocation inline storage for M*N≤4.
            // Skip inline for N==1 (column vector): StaticArrayInlineData uses
            // cols==1 to mean SVector, so SMatrix{M,1} would be misreported as
            // SVector{M} — use the StaticArray path which stores the type_name
            // string and reports the correct SMatrix{M,1,T} type (Issue #7964).
            if n > 1 {
                if let Some(inline) =
                    crate::vm::value::StaticArrayInlineData::try_from_elem(m, n, &elems)
                {
                    return Some(Value::StaticArrayInline(inline));
                }
            }
            return Some(Value::StaticArray(Box::new(StaticRealValue::new_matrix(
                struct_name,
                m,
                n,
                elems,
            ))));
        }
    }

    None
}

/// Extract `expected_len` Real values from a single-field struct that holds
/// `values[0] = Value::Tuple(homogeneous-Real elements)`.
/// `elem_type_name` is the Julia element type (e.g. "Float64", "Int64").
fn extract_real_tuple(
    values: &[Value],
    expected_len: usize,
    elem_type_name: &str,
) -> Option<StaticElem> {
    if values.len() != 1 {
        return None;
    }
    let tuple = match &values[0] {
        Value::Tuple(t) => t,
        _ => return None,
    };
    if tuple.elements.len() != expected_len {
        return None;
    }
    let elems = &tuple.elements;
    Some(match elem_type_name {
        "Float64" => StaticElem::F64(
            elems
                .iter()
                .map(|v| {
                    if let Value::F64(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Float32" => StaticElem::F32(
            elems
                .iter()
                .map(|v| {
                    if let Value::F32(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Int64" => StaticElem::I64(
            elems
                .iter()
                .map(|v| {
                    if let Value::I64(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Int32" => StaticElem::I32(
            elems
                .iter()
                .map(|v| {
                    if let Value::I32(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Int16" => StaticElem::I16(
            elems
                .iter()
                .map(|v| {
                    if let Value::I16(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Int8" => StaticElem::I8(
            elems
                .iter()
                .map(|v| if let Value::I8(x) = v { Some(*x) } else { None })
                .collect::<Option<Vec<_>>>()?,
        ),
        "UInt64" => StaticElem::U64(
            elems
                .iter()
                .map(|v| {
                    if let Value::U64(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "UInt32" => StaticElem::U32(
            elems
                .iter()
                .map(|v| {
                    if let Value::U32(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "UInt16" => StaticElem::U16(
            elems
                .iter()
                .map(|v| {
                    if let Value::U16(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "UInt8" => StaticElem::U8(
            elems
                .iter()
                .map(|v| if let Value::U8(x) = v { Some(*x) } else { None })
                .collect::<Option<Vec<_>>>()?,
        ),
        "Bool" => StaticElem::Bool(
            elems
                .iter()
                .map(|v| {
                    if let Value::Bool(x) = v {
                        Some(*x)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None, // unsupported element type → fall back to Struct
    })
}

use super::super::Vm;

const EXPR_FIELD_HEAD_INDEX: usize = 0;
const EXPR_FIELD_ARGS_INDEX: usize = 1;
const LINE_NUMBER_NODE_FIELD_LINE_INDEX: usize = 0;
const LINE_NUMBER_NODE_FIELD_FILE_INDEX: usize = 1;
const GLOBAL_REF_FIELD_MODULE_INDEX: usize = 0;
const GLOBAL_REF_FIELD_NAME_INDEX: usize = 1;
const GLOBAL_REF_FIELD_BINDING_INDEX: usize = 2;

fn array_wrapper_compat_get_field(instance: &StructInstance, field_name: &str) -> Option<Value> {
    if !is_array_wrapper_struct_name(&instance.struct_name) {
        return None;
    }
    let ref_value = instance.values.first()?;
    let size_value = instance.values.get(1)?;
    match field_name {
        "_mem" => match ref_value {
            Value::MemoryRef(memref) => Some(Value::Memory(memref.parent())),
            _ => None,
        },
        "_size" => match ref_value {
            Value::MemoryRef(memref) if memref.memory_index() > 1 => {
                Some(Value::Tuple(TupleValue::new(vec![
                    size_value.clone(),
                    Value::I64(memref.memory_index() as i64),
                ])))
            }
            Value::MemoryRef(_) => Some(size_value.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn array_wrapper_compat_set_field(
    instance: &mut StructInstance,
    field_name: &str,
    value: Value,
) -> Result<bool, VmError> {
    if !is_array_wrapper_struct_name(&instance.struct_name) {
        return Ok(false);
    }
    match field_name {
        "_mem" => {
            let ref_value = match value {
                Value::Memory(mem) => Value::MemoryRef(Box::new(MemoryRefValue::first(mem))),
                Value::MemoryRef(memref) => Value::MemoryRef(memref),
                other => {
                    return Err(VmError::TypeError(format!(
                        "Array wrapper _mem alias expects Memory or MemoryRef, got {:?}",
                        other.value_type()
                    )))
                }
            };
            instance.set_field(0, ref_value)?;
            Ok(true)
        }
        "_size" => {
            let size_value = match value {
                Value::Tuple(t) => match t.elements.first() {
                    Some(Value::Tuple(dims)) => Value::Tuple(dims.clone()),
                    _ => Value::Tuple(t),
                },
                other => other,
            };
            instance.set_field(1, size_value)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn pairs_projected_field(
    pairs: &super::super::value::PairsValue,
    field_name: &str,
) -> Option<Value> {
    match field_name {
        "data" => Some(Value::NamedTuple(pairs.data.clone())),
        // Every current PairsValue origin is pairs(::NamedTuple), whose
        // physical iterator field is `nothing` upstream (Issue #11380).
        "itr" => Some(Value::Nothing),
        _ => None,
    }
}

/// Canonical `convert` target name for a declared field's concrete primitive
/// numeric / bool `JuliaType`.
///
/// Returns `None` for any non-leaf or non-primitive type (`Any`, abstract
/// types, structs, collections, type parameters, ...) so that coercion is never
/// applied where Julia would not insert a numeric `convert` call (or where
/// conversion would be lossy / nonsensical).
///
/// Note: this deliberately uses the precise `JuliaType` (not the lossy
/// `ValueType`, which collapses every signed/unsigned integer to `I64`) so the
/// declared `UInt64` / `Int32` / ... field width is preserved (Issue #4990).
fn primitive_field_convert_name(ty: &crate::types::JuliaType) -> Option<&'static str> {
    use crate::types::JuliaType;
    Some(match ty {
        JuliaType::Int8 => "Int8",
        JuliaType::Int16 => "Int16",
        JuliaType::Int32 => "Int32",
        JuliaType::Int64 => "Int64",
        JuliaType::Int128 => "Int128",
        JuliaType::UInt8 => "UInt8",
        JuliaType::UInt16 => "UInt16",
        JuliaType::UInt32 => "UInt32",
        JuliaType::UInt64 => "UInt64",
        JuliaType::UInt128 => "UInt128",
        JuliaType::Float16 => "Float16",
        JuliaType::Float32 => "Float32",
        JuliaType::Float64 => "Float64",
        JuliaType::Bool => "Bool",
        _ => return None,
    })
}

/// True when `value` is a primitive numeric / bool value that can participate
/// in a numeric `convert`. This guards against trying to convert e.g. a struct
/// or string into a declared numeric field type (which would error).
fn is_primitive_numeric_value(value: &Value) -> bool {
    matches!(
        value,
        Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F16(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::Bool(_)
    )
}

/// Coerce each field value to its declared concrete primitive field type,
/// mirroring Julia's default / `new` constructor `convert(fieldtype, x)` step
/// (Issue #4990).
///
/// Only concrete primitive numeric / bool fields are coerced, and only when
/// the supplied value is itself a primitive numeric value whose runtime type
/// differs from the declared field type. Everything else is left untouched so
/// that `Any`, abstract, struct, and collection fields keep their runtime
/// value.
///
/// The declared field type is taken from `field_julia_types` (the precise
/// `JuliaType`), which—unlike the `fields` `ValueType` list—does not collapse
/// `UInt64` / `Int32` / ... down to a single `I64` representation.
pub(super) fn coerce_fields_to_declared_types(
    struct_defs: &[super::super::types::StructDefInfo],
    struct_heap: &[StructInstance],
    struct_def: Option<&super::super::types::StructDefInfo>,
    values: &mut [Value],
) {
    let Some(def) = struct_def else { return };
    // Coercion requires the precise per-field `JuliaType`. When it is not
    // populated (length mismatch), skip coercion entirely rather than fall back
    // to the lossy `ValueType` list.
    if def.field_julia_types.len() != values.len() {
        return;
    }
    for (idx, value) in values.iter_mut().enumerate() {
        let Some(declared) = def.field_julia_types.get(idx) else {
            continue;
        };
        if coerce_complex_field_to_declared_type(struct_defs, declared, value) {
            continue;
        }
        if coerce_complex_field_ref_to_declared_type(struct_defs, struct_heap, declared, value) {
            continue;
        }
        let Some(target_name) = primitive_field_convert_name(declared) else {
            continue;
        };
        if !is_primitive_numeric_value(value) {
            continue;
        }
        // Already the declared type: nothing to do.
        if util::value_type_name(value) == target_name {
            continue;
        }
        if let Ok(converted) = super::super::convert::convert_value(target_name, value) {
            *value = converted;
        }
    }
}

fn coerce_dynamic_parametric_fields_to_type_args(
    ctx: Option<&crate::bytecode::RuntimeCompileContext>,
    struct_heap: &[StructInstance],
    base_name: &str,
    type_args: &[String],
    values: &mut [Value],
) {
    let Some(ctx) = ctx else { return };
    let Some(parametric) = ctx.parametric_structs.get(base_name).or_else(|| {
        base_name
            .rsplit_once('.')
            .and_then(|(_, short)| ctx.parametric_structs.get(short))
    }) else {
        return;
    };
    if parametric.def.type_params.len() != type_args.len()
        || parametric.def.fields.len() != values.len()
    {
        return;
    }
    let bindings: HashMap<&str, JuliaType> = parametric
        .def
        .type_params
        .iter()
        .zip(type_args.iter())
        .map(|(param, arg)| (param.name.as_str(), JuliaType::from_name_or_struct(arg)))
        .collect();
    for (field, value) in parametric.def.fields.iter().zip(values.iter_mut()) {
        let Some(TypeExpr::TypeVar(name)) = &field.type_expr else {
            continue;
        };
        let Some(target) = bindings.get(name.as_str()) else {
            continue;
        };
        if coerce_complex_field_to_declared_type(&ctx.struct_defs, target, value) {
            continue;
        }
        if coerce_complex_field_ref_to_declared_type(&ctx.struct_defs, struct_heap, target, value) {
            continue;
        }
        let Some(target_name) = primitive_field_convert_name(target) else {
            continue;
        };
        if !is_primitive_numeric_value(value) || util::value_type_name(value) == target_name {
            continue;
        }
        if let Ok(converted) = super::super::convert::convert_value(target_name, value) {
            *value = converted;
        }
    }
}

fn coerce_complex_field_to_declared_type(
    struct_defs: &[super::super::types::StructDefInfo],
    declared: &JuliaType,
    value: &mut Value,
) -> bool {
    let Some((target_name, element_name)) = complex_field_convert_target(declared) else {
        return false;
    };
    let Value::Struct(instance) = value else {
        return false;
    };
    if !instance.is_complex() || instance.values.len() < 2 {
        return false;
    }
    let Ok(re) = super::super::convert::convert_value(&element_name, &instance.values[0]) else {
        return false;
    };
    let Ok(im) = super::super::convert::convert_value(&element_name, &instance.values[1]) else {
        return false;
    };
    let type_id = struct_defs
        .iter()
        .position(|def| def.name == target_name)
        .unwrap_or(instance.type_id);
    *value = Value::Struct(StructInstance::complex_from_storage(
        type_id,
        target_name,
        re,
        im,
    ));
    true
}

fn coerce_complex_field_ref_to_declared_type(
    struct_defs: &[super::super::types::StructDefInfo],
    struct_heap: &[StructInstance],
    declared: &JuliaType,
    value: &mut Value,
) -> bool {
    let Value::StructRef(idx) = value else {
        return false;
    };
    let Some(instance) = struct_heap.get(*idx) else {
        return false;
    };
    if !instance.is_complex() {
        return false;
    }
    let mut owned = Value::Struct(instance.clone());
    if !coerce_complex_field_to_declared_type(struct_defs, declared, &mut owned) {
        return false;
    }
    *value = owned;
    true
}

fn complex_field_convert_target(declared: &JuliaType) -> Option<(String, String)> {
    let JuliaType::Struct(name) = declared else {
        return None;
    };
    match name.as_str() {
        "ComplexF64" => Some(("Complex{Float64}".to_string(), "Float64".to_string())),
        "ComplexF32" => Some(("Complex{Float32}".to_string(), "Float32".to_string())),
        _ if name.starts_with("Complex{") && name.ends_with('}') => {
            let element_name = name["Complex{".len()..name.len() - 1].trim();
            if element_name.is_empty() {
                None
            } else {
                Some((name.clone(), element_name.to_string()))
            }
        }
        _ => None,
    }
}

/// Parse the explicit type arguments carried in a parametric name such as
/// `A.Pt{Float64}` or `Foo{Int64, Float64}` into `JuliaType`s, returning `None`
/// when the name has no `{...}` parameters or supplies more parameters than the
/// type declares. Partially applied names (`Foo{Int64}` for `Foo{T,V}`) return
/// the explicit leading prefix so the runtime constructor can infer the
/// remaining parameters from field values (Issue #8393).
///
/// Constructor path (Issue #9197 S6/S7): the explicit type arguments reach the VM
/// only as a rendered name string here; retiring this parse needs the compiler to
/// lower structured type arguments to the constructor (a lowering-scope change),
/// not the dispatch typemap. Out of scope for S5, which landed the
/// sealed-primitive first-arg gather in `FirstArgIndex` (method_table.rs).
pub(super) fn parse_explicit_parametric_type_args(
    type_name: &str,
    expected: usize,
) -> Option<Vec<crate::types::JuliaType>> {
    let brace_pos = type_name.find('{')?;
    let close_pos = type_name.rfind('}')?;
    if close_pos <= brace_pos + 1 {
        return None;
    }
    let params = split_top_level_type_params(&type_name[brace_pos + 1..close_pos]);
    if params.len() > expected {
        return None;
    }
    Some(
        params
            .iter()
            .map(|p| crate::types::JuliaType::from_name_or_struct(p.trim()))
            .collect(),
    )
}

pub(super) fn infer_runtime_parametric_type_args_with_explicit_prefix(
    def: &crate::ir::core::StructDef,
    base_name: &str,
    arg_types: &[JuliaType],
    explicit_prefix: &[JuliaType],
) -> Result<Vec<JuliaType>, VmError> {
    if arg_types.len() != def.fields.len() {
        return Err(VmError::MethodError(format!(
            "{} constructor expects {} arguments, got {}",
            base_name,
            def.fields.len(),
            arg_types.len()
        )));
    }
    if explicit_prefix.len() > def.type_params.len() {
        return Err(VmError::TypeError(format!(
            "{}{{...}} expects at most {} type parameters, got {}",
            base_name,
            def.type_params.len(),
            explicit_prefix.len()
        )));
    }

    let mut inferred = HashMap::new();
    for (param, explicit) in def.type_params.iter().zip(explicit_prefix.iter()) {
        record_runtime_type_binding(&param.name, explicit, &mut inferred)?;
    }
    for (field, actual) in def.fields.iter().zip(arg_types.iter()) {
        let Some(type_expr) = field.type_expr.as_ref() else {
            continue;
        };
        bind_runtime_field_type_vars(type_expr, actual, &def.type_params, &mut inferred)?;
    }
    bind_runtime_type_vars_from_param_bounds(&def.type_params, &mut inferred)?;

    def.type_params
        .iter()
        .map(|param| {
            inferred.get(&param.name).cloned().ok_or_else(|| {
                VmError::MethodError(format!(
                    "no method matching {}{{...}}({})",
                    base_name,
                    arg_types
                        .iter()
                        .map(|arg| format!("::{}", arg.name()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
        })
        .collect()
}

/// Coerce each field value to the concrete primitive field type obtained by
/// substituting the *explicit* type parameters into the struct's declared field
/// types, mirroring upstream's `Base{T...}(args)` default constructor
/// `convert(fieldtype, arg)` step (Issue #8101). Unlike
/// [`coerce_fields_to_declared_types`], the concrete instantiation row need not
/// exist yet: the declared field type is read from the parametric `StructDef`
/// (`field::T` -> the matching explicit type arg, `field::Int` -> its concrete
/// type). Non-`T`, abstract, struct, and collection fields are left untouched.
pub(super) fn coerce_fields_to_explicit_type_args(
    def: &crate::ir::core::StructDef,
    type_args: &[crate::types::JuliaType],
    values: &mut [Value],
) {
    use crate::types::TypeExpr;
    if def.fields.len() != values.len() {
        return;
    }
    let subst: std::collections::HashMap<&str, &crate::types::JuliaType> = def
        .type_params
        .iter()
        .map(|p| p.name.as_str())
        .zip(type_args.iter())
        .collect();
    for (field, value) in def.fields.iter().zip(values.iter_mut()) {
        let declared = match field.type_expr.as_ref() {
            Some(TypeExpr::TypeVar(name)) => subst.get(name.as_str()).map(|jt| (*jt).clone()),
            Some(TypeExpr::Concrete(jt)) => Some(jt.clone()),
            _ => None,
        };
        let Some(declared) = declared else { continue };
        let Some(target_name) = primitive_field_convert_name(&declared) else {
            continue;
        };
        if !is_primitive_numeric_value(value) {
            continue;
        }
        if util::value_type_name(value) == target_name {
            continue;
        }
        if let Ok(converted) = super::super::convert::convert_value(target_name, value) {
            *value = converted;
        }
    }
}

fn split_top_level_type_params(params: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in params.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                result.push(params[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    result.push(params[start..].trim());
    result
}

fn split_datatype_name(type_name: &str) -> Option<(String, Vec<String>)> {
    let brace_pos = type_name.find('{')?;
    let close_pos = type_name.rfind('}')?;
    if close_pos <= brace_pos || close_pos + 1 != type_name.len() {
        return None;
    }

    let base_name = type_name[..brace_pos].to_string();
    let type_arg_names = split_top_level_type_params(&type_name[brace_pos + 1..close_pos])
        .into_iter()
        .map(str::to_string)
        .collect();
    Some((base_name, type_arg_names))
}

fn runtime_bound_alias_target(
    type_name: &str,
    aliases: &HashMap<String, String>,
) -> Option<String> {
    if let Some(target) = aliases.get(type_name) {
        return Some(target.clone());
    }
    let mut unique_target: Option<&String> = None;
    for (alias, target) in aliases {
        if alias.rsplit('.').next() != Some(type_name) {
            continue;
        }
        if unique_target.is_some_and(|existing| existing != target) {
            return None;
        }
        unique_target = Some(target);
    }
    unique_target.cloned()
}

fn expand_runtime_bound_aliases(
    type_name: &str,
    aliases: &HashMap<String, String>,
    excluded_binders: &HashSet<String>,
    seen: &mut HashSet<String>,
) -> String {
    let type_name = type_name.trim();
    if !excluded_binders.contains(type_name) {
        if let Some(target) = runtime_bound_alias_target(type_name, aliases) {
            if seen.insert(type_name.to_string()) {
                let expanded =
                    expand_runtime_bound_aliases(&target, aliases, excluded_binders, seen);
                seen.remove(type_name);
                return expanded;
            }
        }
    }
    let Some((base, params)) = split_datatype_name(type_name) else {
        return type_name.to_string();
    };
    let expanded_base = expand_runtime_bound_aliases(&base, aliases, excluded_binders, seen);
    let expanded_params = params
        .iter()
        .map(|param| expand_runtime_bound_aliases(param, aliases, excluded_binders, seen))
        .collect::<Vec<_>>();
    format!("{}{{{}}}", expanded_base, expanded_params.join(", "))
}

/// Canonicalize a parametric-struct schema before any runtime wrapper is built.
///
/// Both string-backed and structured wrapper construction route through this
/// authority. `Core.apply_type` then validates the resulting `UnionAll`, while
/// `NewDynamicParametricStruct` only allocates an already-selected concrete
/// instantiation. Keeping alias expansion here prevents validators from
/// comparing a surface alias spelling independently (Issue #11142).
fn expand_runtime_type_params(
    type_params: &[TypeParam],
    aliases: &HashMap<String, String>,
) -> Vec<TypeParam> {
    let excluded_binders: HashSet<String> =
        type_params.iter().map(|param| param.name.clone()).collect();
    type_params
        .iter()
        .map(|param| {
            let mut expanded = param.clone();
            expanded.upper_bound = param.get_upper_bound().map(|bound| {
                expand_runtime_bound_aliases(bound, aliases, &excluded_binders, &mut HashSet::new())
            });
            expanded.bound = expanded.upper_bound.clone();
            expanded.lower_bound = param.lower_bound.as_ref().map(|bound| {
                expand_runtime_bound_aliases(bound, aliases, &excluded_binders, &mut HashSet::new())
            });
            expanded
        })
        .collect()
}

/// The declared type-parameter schema of a builtin parametric family, keyed by
/// its bare name. Each entry is `(param_name, optional upper bound)`. This is the
/// single source of truth for the arity/bounds of a builtin family; both
/// `builtin_runtime_unionall_wrapper` (all params free) and
/// `build_partial_builtin_unionall` (a bound prefix + free suffix) consult it so
/// they agree (Issue #10586).
fn builtin_family_type_params(
    type_name: &str,
) -> Option<&'static [(&'static str, Option<&'static str>)]> {
    let params: &[(&str, Option<&str>)] = match type_name {
        "Array" | "DenseArray" | "AbstractArray" => &[("T", None), ("N", None)],
        "Dict" | "AbstractDict" | "Pair" => &[("K", None), ("V", None)],
        "NTuple" => &[("N", None), ("T", None)],
        "Vector" | "Matrix" | "DenseVector" | "DenseMatrix" | "AbstractVector"
        | "AbstractMatrix" | "Set" | "AbstractSet" | "UnitRange" | "OneTo" | "LinRange" | "Ref"
        | "RefValue" | "Ptr" | "Type" | "Memory" | "MemoryRef" => &[("T", None)],
        "StepRange" => &[("T", None), ("S", None)],
        "NamedTuple" => &[("names", None), ("T", None)],
        "Rational" => &[("T", Some("Integer"))],
        "Complex" => &[("T", Some("Real"))],
        _ => return None,
    };
    Some(params)
}

fn builtin_runtime_unionall_wrapper(type_name: &str) -> Option<JuliaType> {
    let params = builtin_family_type_params(type_name)?;

    let rendered_params = params
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    let mut wrapper = JuliaType::Struct(format!("{type_name}{{{rendered_params}}}"));
    for (name, upper) in params.iter().rev() {
        wrapper = JuliaType::UnionAll {
            var: (*name).to_string(),
            lower_bound: None,
            bound: upper.map(|bound| Box::new(bound.to_string())),
            body: Box::new(wrapper),
        };
    }
    Some(wrapper)
}

/// An under-applied builtin parametric family (`Array{Float64}`, `Dict{String}`)
/// evaluates to the trailing `UnionAll` that keeps the bound prefix and leaves
/// the remaining parameters free, mirroring upstream (`Array{Float64, N} where
/// N`). Returns `None` for a bare name (no braces) or a fully/over-applied
/// instantiation — those are concrete `DataType`s handled by the caller's
/// `from_name_or_struct` fallback (Issue #10586).
fn build_partial_builtin_unionall(base_name: &str, type_arg_names: &[String]) -> Option<JuliaType> {
    let params = builtin_family_type_params(base_name)?;
    if type_arg_names.is_empty() || type_arg_names.len() >= params.len() {
        return None;
    }

    let remaining = &params[type_arg_names.len()..];
    let mut body_params: Vec<String> = type_arg_names.to_vec();
    body_params.extend(remaining.iter().map(|(name, _)| (*name).to_string()));

    let mut ty = JuliaType::Struct(format!("{}{{{}}}", base_name, body_params.join(", ")));
    for (name, upper) in remaining.iter().rev() {
        ty = JuliaType::UnionAll {
            var: (*name).to_string(),
            lower_bound: None,
            bound: upper.map(|bound| Box::new(bound.to_string())),
            body: Box::new(ty),
        };
    }
    Some(ty)
}

/// Structural counterpart of [`build_partial_builtin_unionall`] for a prefix
/// carrying a free runtime TypeVar. Rendering and reparsing that prefix would
/// discard its ID, so retain the prefix nodes and append only the declared
/// trailing binders (Issue #10861).
fn build_structured_partial_builtin_unionall(
    base_name: &str,
    type_args: &[JuliaType],
) -> Option<JuliaType> {
    let params = builtin_family_type_params(base_name)?;
    if type_args.is_empty() || type_args.len() >= params.len() {
        return None;
    }

    let remaining = &params[type_args.len()..];
    let mut body_params = type_args.to_vec();
    body_params.extend(
        remaining
            .iter()
            .map(|(name, _)| JuliaType::TypeVar((*name).to_string(), None)),
    );

    let mut ty = JuliaType::RuntimeParametric {
        base: base_name.to_string(),
        params: body_params,
    };
    for (name, upper) in remaining.iter().rev() {
        ty = JuliaType::UnionAll {
            var: (*name).to_string(),
            lower_bound: None,
            bound: upper.map(|bound| Box::new(bound.to_string())),
            body: Box::new(ty),
        };
    }
    Some(ty)
}

fn partial_unionall_schema_allowed(base_name: &str) -> bool {
    !matches!(
        base_name,
        "Array" | "Vector" | "Matrix" | "Tuple" | "NTuple" | "Val" | "Vararg" | "NamedTuple"
    )
}

fn runtime_parametric_schema_for_family<'a>(
    ctx: &'a crate::bytecode::RuntimeCompileContext,
    family: &str,
) -> Option<&'a crate::runtime_types::struct_info::ParametricStructDef> {
    if let Some(schema) = ctx
        .parametric_structs
        .get(family)
        .or_else(|| ctx.base_parametric_structs.get(family))
    {
        return Some(schema);
    }

    let mut unique = None;
    for (name, schema) in ctx
        .parametric_structs
        .iter()
        .chain(ctx.base_parametric_structs.iter())
    {
        let unqualified = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(_, suffix)| suffix);
        if unqualified != family {
            continue;
        }
        if unique.is_some() {
            return None;
        }
        unique = Some(schema);
    }
    unique
}

fn partial_binders_are_declaration_stable(type_params: &[TypeParam], applied_count: usize) -> bool {
    let applied = &type_params[..applied_count.min(type_params.len())];
    type_params[applied_count.min(type_params.len())..]
        .iter()
        .all(|remaining| {
            remaining
                .lower_bound
                .iter()
                .chain(remaining.get_upper_bound())
                .all(|bound| {
                    applied.iter().all(|applied| {
                        !crate::vm::builtins_types::type_name_references_typevar(
                            bound,
                            &applied.name,
                        )
                    })
                })
        })
}

impl<R: RngLike> Vm<R> {
    pub(in crate::vm) fn datatype_from_name_or_partial_unionall(
        &mut self,
        type_name: &str,
    ) -> JuliaType {
        if let Some((base_name, type_arg_names)) = split_datatype_name(type_name) {
            if partial_unionall_schema_allowed(&base_name) {
                if let Some(ctx) = self.compile_context.as_ref() {
                    if let Some(parametric_def) =
                        runtime_parametric_schema_for_family(ctx, &base_name)
                    {
                        if let Some(partial) = self.build_partial_parametric_unionall(
                            &base_name,
                            &type_arg_names,
                            &parametric_def.def.type_params,
                        ) {
                            let stable = partial_binders_are_declaration_stable(
                                &parametric_def.def.type_params,
                                type_arg_names.len(),
                            );
                            let substitutions = parametric_def
                                .def
                                .type_params
                                .iter()
                                .zip(type_arg_names.iter())
                                .map(|(param, arg)| {
                                    (param.name.clone(), JuliaType::Struct(arg.clone()))
                                })
                                .collect::<Vec<_>>();
                            return self.canonical_partial_unionall(
                                partial,
                                stable,
                                &substitutions,
                            );
                        }
                    }
                }
            }
            // An under-applied builtin family whose bound prefix is concrete
            // (`Array{Float64}`) is a trailing `UnionAll`, not a concrete
            // `DataType` (Issue #10586). A prefix that is still a free type
            // variable (`Array{T}` inside `Array{T} where …`) keeps the bare
            // `Array{T}` schema so the enclosing `where` binds it and prints with
            // upstream's shorthand instead of materialising the elided trailing
            // parameter.
            if type_arg_names
                .iter()
                .all(|name| self.under_applied_prefix_is_bound(name))
            {
                if let Some(partial) = build_partial_builtin_unionall(&base_name, &type_arg_names) {
                    return self.canonical_partial_unionall(partial, true, &[]);
                }
            }
        }

        JuliaType::from_name_or_struct(type_name)
    }

    /// Whether a type-argument name in an under-applied builtin family
    /// (`Array{<name>}`) is a concrete *bound* prefix rather than a free type
    /// variable. Only a concrete prefix turns the family into a specific
    /// trailing `UnionAll` (`Array{Float64, N} where N`); a free variable
    /// (`Array{T}` within a `where`) must keep the bare schema (Issue #10586).
    fn under_applied_prefix_is_bound(&self, name: &str) -> bool {
        let name = name.trim();
        // A value parameter (integer / bool / symbol / char) is a bound prefix.
        if name.parse::<i64>().is_ok()
            || matches!(name, "true" | "false")
            || name.starts_with(':')
            || name.starts_with('\'')
        {
            return true;
        }
        // A compound instantiation (`Vector{Int}`) is bound only when its own
        // base is a known family AND every argument is itself bound — a compound
        // carrying a free variable (`Vector{T}` in a `where`) is not.
        if let Some((base, args)) = split_datatype_name(name) {
            let base_bound = JuliaType::from_name(&base).is_some()
                || builtin_family_type_params(&base).is_some()
                || self.compile_context.as_ref().is_some_and(|ctx| {
                    ctx.struct_table.contains_key(&base)
                        || ctx.parametric_structs.contains_key(&base)
                });
            return base_bound
                && args
                    .iter()
                    .all(|arg| self.under_applied_prefix_is_bound(arg));
        }
        // A qualified name (`Base.Int`) or a builtin type name is bound.
        if name.contains('.') || JuliaType::from_name(name).is_some() {
            return true;
        }
        // A user-declared type name is a concrete prefix; a bare identifier that
        // no static table knows is a free type variable.
        self.compile_context.as_ref().is_some_and(|ctx| {
            ctx.struct_table.contains_key(name)
                || ctx.parametric_structs.contains_key(name)
                || ctx
                    .struct_defs
                    .iter()
                    .any(|d| d.name == name || d.name.split('{').next() == Some(name))
        })
    }

    fn build_partial_parametric_unionall(
        &self,
        base_name: &str,
        type_arg_names: &[String],
        type_params: &[TypeParam],
    ) -> Option<JuliaType> {
        let type_args = type_arg_names
            .iter()
            .map(|name| JuliaType::Struct(name.clone()))
            .collect::<Vec<_>>();
        self.build_structured_partial_parametric_unionall(base_name, &type_args, type_params)
    }

    fn build_structured_partial_parametric_unionall(
        &self,
        base_name: &str,
        type_args: &[JuliaType],
        type_params: &[TypeParam],
    ) -> Option<JuliaType> {
        let expanded_type_params = self.expanded_runtime_type_params(type_params);
        let type_params = expanded_type_params.as_slice();
        if type_args.len() >= type_params.len() {
            return None;
        }

        // Construct the declaration's complete wrapper first, then consume the
        // applied prefix through structural instantiation. Building only the
        // trailing binders loses earlier variables referenced by dependent
        // bounds (`Q{T,N<:T}` -> `Q{Int}.var.ub === Int`) and disconnects the
        // partial value from the declaration's binder graph (Issue #10460).
        let body_params = type_params
            .iter()
            .map(|param| JuliaType::TypeVar(param.name.clone(), None))
            .collect();
        let mut ty = JuliaType::from_structured_parametric(base_name.to_string(), body_params);
        for param in type_params.iter().rev() {
            ty = JuliaType::UnionAll {
                var: param.name.clone(),
                lower_bound: param.lower_bound.clone().map(Box::new),
                bound: param.get_upper_bound().cloned().map(Box::new),
                body: Box::new(ty),
            };
        }
        for type_arg in type_args {
            ty = ty.instantiate(type_arg);
        }
        Some(ty)
    }

    fn expanded_runtime_type_params(&self, type_params: &[TypeParam]) -> Vec<TypeParam> {
        // Runtime type application must validate against the semantic bound,
        // not the surface alias spelling. Expand aliases recursively while
        // protecting this schema's own binder names, so
        // `const E = Union{Integer,String}; F{T<:E}` accepts `Int` under
        // Core.apply_type as upstream does (Issues #11003 and #11142).
        self.compile_context.as_ref().map_or_else(
            || type_params.to_vec(),
            |ctx| expand_runtime_type_params(type_params, &ctx.type_aliases),
        )
    }

    /// Project a declared partial wrapper into the owner-scoped runtime binder
    /// graph used by reflection. Canonical constructors must return this same
    /// graph so `===` never has to equate a source-only wrapper with a freshly
    /// projected one (Issue #10460).
    fn canonical_partial_unionall(
        &mut self,
        partial: JuliaType,
        reuse_declared_binders: bool,
        applied_substitutions: &[(String, JuliaType)],
    ) -> JuliaType {
        if !matches!(partial, JuliaType::UnionAll { .. }) {
            return partial;
        }
        if !reuse_declared_binders {
            return self.project_unionall_binders_with_fresh_owner(&partial, applied_substitutions);
        }
        let outer_var = {
            let registry = RuntimeTypeRegistry::new_with_struct_defs(
                self.compile_context.as_ref(),
                &self.abstract_types,
                &self.struct_defs,
            );
            registry.object(&partial).unionall_var()
        };
        if let Some(var) = outer_var {
            self.runtime_typevar_value_for_unionall_projection(&partial, var);
        }
        self.project_unionall_binders_for_owner(&partial, &partial)
    }

    fn freshen_runtime_unionall_binders(&mut self, ty: JuliaType) -> JuliaType {
        match ty {
            JuliaType::RuntimeUnionAll { var, body } => {
                let JuliaType::RuntimeTypeVar {
                    id,
                    name,
                    lower_bound,
                    upper_bound,
                } = var.as_ref()
                else {
                    return JuliaType::RuntimeUnionAll { var, body };
                };
                let fresh_var = JuliaType::RuntimeTypeVar {
                    id: self.runtime_typevar_counter,
                    name: name.clone(),
                    lower_bound: lower_bound.clone(),
                    upper_bound: upper_bound.clone(),
                };
                self.runtime_typevar_counter += 1;
                let body = body.substitute_runtime_typevar(*id, &fresh_var);
                JuliaType::RuntimeUnionAll {
                    var: Box::new(fresh_var),
                    body: Box::new(self.freshen_runtime_unionall_binders(body)),
                }
            }
            other => other,
        }
    }

    /// Canonical generic wrapper for a `Core.TypeName` symbol — the exact
    /// `DataType`/`UnionAll` value the bare type identifier produces via
    /// `PushDataType`. `typename_symbol()` has already stripped type
    /// parameters and collapsed Base display aliases (`Vector`/`Matrix` ->
    /// `Array`), so resolving the base symbol here reproduces the shared
    /// generic wrapper: `Foo{Int}.name.wrapper === Foo`, with identity stable
    /// across concrete instantiations and `===` the source-level type name
    /// (Issue #10558). Non-parametric concrete/abstract types are their own
    /// wrapper (`Int64.name.wrapper === Int64`); `@enum` types resolve to
    /// `JuliaType::Enum` (identity with `typeof`). Mirroring `PushDataType`
    /// exactly is what unifies these representations — an earlier version used
    /// `build_partial_parametric_unionall`/`builtin_runtime_unionall_wrapper`,
    /// which print the same but are `!==` the bare identifier and return
    /// nothing for non-parametric/enum types. The result is a nested
    /// `UnionAll` chain for parametric types, which `Base.typejoin` walks to
    /// recover each position's declared TypeVar.
    pub(in crate::vm) fn runtime_type_wrapper(&mut self, type_name: &str) -> Option<JuliaType> {
        if crate::vm::value::enum_registry::is_registered_enum(type_name) {
            return Some(JuliaType::Enum(type_name.to_string()));
        }
        Some(self.datatype_from_name_or_partial_unionall(type_name))
    }

    fn resolve_any_type_params_from_values(
        &self,
        struct_name: &str,
        values: &[Value],
    ) -> Option<String> {
        let brace_pos = struct_name.find('{')?;
        let close_pos = struct_name.rfind('}')?;
        if close_pos <= brace_pos {
            return None;
        }

        let base_name = &struct_name[..brace_pos];
        let params = split_top_level_type_params(&struct_name[brace_pos + 1..close_pos]);
        if params.iter().all(|param| param.trim() != "Any") {
            return None;
        }

        let mut changed = false;
        let resolved_params: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                let trimmed = param.trim();
                if trimmed != "Any" {
                    return trimmed.to_string();
                }

                let Some(value) = values.get(idx) else {
                    return trimmed.to_string();
                };
                let resolved = self.get_type_name(value);
                if resolved == "Any" {
                    trimmed.to_string()
                } else {
                    changed = true;
                    resolved
                }
            })
            .collect();

        changed.then(|| format!("{}{{{}}}", base_name, resolved_params.join(", ")))
    }

    fn infer_parametric_struct_name_from_runtime_fields(
        &self,
        base_name: &str,
        values: &[Value],
    ) -> Result<Option<String>, VmError> {
        let Some(ctx) = self.compile_context.as_ref() else {
            return Ok(None);
        };
        let Some(parametric_def) = ctx.parametric_structs.get(base_name) else {
            return Ok(None);
        };
        let def = &parametric_def.def;
        if def.fields.len() != values.len() || def.type_params.is_empty() {
            return Ok(None);
        }

        let mut inferred = HashMap::new();
        for (field, value) in def.fields.iter().zip(values.iter()) {
            let Some(type_expr) = field.type_expr.as_ref() else {
                continue;
            };
            let actual = self.get_value_julia_type(value);
            bind_runtime_field_type_vars(type_expr, &actual, &def.type_params, &mut inferred)?;
        }
        bind_runtime_type_vars_from_param_bounds(&def.type_params, &mut inferred)?;

        let Some(type_args) = def
            .type_params
            .iter()
            .map(|param| inferred.get(&param.name).map(|ty| ty.name().to_string()))
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(None);
        };

        Ok(Some(format!("{}{{{}}}", base_name, type_args.join(", "))))
    }

    /// Build a `Union{...}` value from `Core.apply_type(Union, members...)`
    /// arguments (Issue #10623).
    ///
    /// Mirrors upstream `jl_type_union` (`julia/src/jltypes.c`): members may be
    /// arbitrary types OR runtime `TypeVar`s, deduplicated by runtime IDENTITY,
    /// so two DISTINCT `TypeVar(:F)` build `Union{F, F}` while the same variable
    /// used twice collapses to `F`, order-insensitively (`u1 == u2`).
    ///
    /// This bypasses the generic `build_parametric_type` string round-trip,
    /// which renders every `TypeVar` to its bare name and then loses the
    /// identity distinction in `canonicalize_union`.
    fn build_union_value(&mut self, type_args: &[Value]) -> Value {
        use crate::types::JuliaType;

        let mut members: Vec<(JuliaType, Option<u64>)> = Vec::with_capacity(type_args.len());
        for arg in type_args {
            if let Value::RuntimeTypeVar(tv) = arg {
                members.push((tv.projection(), Some(tv.id)));
            } else if let Some(jt) = type_arg_value_to_julia_type(arg) {
                members.push((jt, None));
            }
            // Non-type args are dropped here; upstream would raise a TypeError,
            // but the subset only constructs Unions from valid type members.
        }

        Value::DataType(Box::new(crate::types::canonicalize_union_with_identity(
            members,
        )))
    }

    /// Whether `value` is acceptable as a parametric-type parameter, mirroring
    /// upstream `jl_valid_type_param` (`julia/src/builtins.c`): a `Type`, a
    /// `TypeVar`, a `Symbol`, a `Module`, or an `isbits` value (`nothing` and
    /// `missing` are isbits singletons). A `Tuple` is valid iff every element
    /// is; a struct instance is valid iff its definition `is_isbits`
    /// (immutable, all-isbits fields) — this accepts `Complex`/`Rational`/plain
    /// isbits user structs (unchanged pre-existing rendering as a bare `Any`
    /// fallback below, since sjulia has no struct-value type-param renderer
    /// yet) while rejecting a non-isbits instance such as `ErrorException`
    /// (holding a non-isbits `String` field) or `BigInt`/`BigFloat` (mutable in
    /// upstream Base). A bare named `Function` (no captures) is always
    /// isbits upstream, as is an `@enum` value (a fixed-width integer); a
    /// `Closure`/`ComposedFunction` is isbits iff every capture / wrapped
    /// function is (checked directly here rather than through the shared
    /// `isbitstype` builtin machinery, which does not yet classify
    /// Function/Closure/ComposedFunction/Enum types at all — a pre-existing,
    /// separate gap tracked as Issue #11589). A `NamedTuple` recurses over its
    /// values exactly like `Tuple`. Anything else (`String`, `Array`, `Dict`,
    /// ...) is likewise invalid (Issue #11555).
    ///
    /// Note: the recursive `Tuple`/`NamedTuple`/`Closure`/`ComposedFunction`
    /// arms accept a `Symbol`/`Module`/`Function` nested *inside* them via
    /// this same predicate, even though upstream additionally requires an
    /// isbits value nested inside a `Tuple` to itself be isbits (a `Symbol`/
    /// `Module` is a valid *standalone* parameter but is not itself isbits,
    /// so e.g. `Vector{(:a,)}` raises upstream). This is a deliberate
    /// over-approximation in the safe direction only: it can accept a
    /// (rare) case upstream would reject, leaving it on the existing
    /// `Any`-fallback rendering exactly as it already behaved before this
    /// fix — never a new false-positive `TypeError` on a value upstream
    /// accepts, which is the regression this predicate must avoid.
    ///
    /// Known remaining gap on the same broken-`isbitstype` root cause as
    /// Issue #11589: a `StaticArray`/`StaticArrayInline` value (SVector /
    /// SMatrix, package-gated behind `using StaticArrays`) is unconditionally
    /// isbits upstream but is rejected here (falls to the catch-all `false`
    /// below) since no base/core fixture reaches it. Once #11589 lands, this
    /// whole hand-written allowlist should collapse to a single delegate
    /// call into the fixed `isbitstype` machinery.
    fn is_valid_type_param_value(&self, value: &Value) -> bool {
        match value {
            Value::DataType(_) | Value::RuntimeTypeVar(_) | Value::Symbol(_) => true,
            Value::Module(_) | Value::Nothing | Value::Missing => true,
            Value::Function(_) | Value::Enum { .. } => true,
            Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F16(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::Bool(_)
            | Value::Char(_) => true,
            Value::Tuple(tuple) => tuple
                .elements
                .iter()
                .all(|elem| self.is_valid_type_param_value(elem)),
            Value::NamedTuple(nt) => nt
                .values
                .iter()
                .all(|val| self.is_valid_type_param_value(val)),
            Value::Closure(cv) => cv
                .captures
                .iter()
                .all(|(_, captured)| self.is_valid_type_param_value(captured)),
            Value::ComposedFunction(cf) => {
                self.is_valid_type_param_value(&cf.outer)
                    && self.is_valid_type_param_value(&cf.inner)
            }
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .and_then(|instance| self.struct_defs.get(instance.type_id))
                .is_some_and(|def| def.is_isbits_with_struct_defs(&self.struct_defs)),
            _ => false,
        }
    }

    /// Build a parametric type `Value` from a base name and a flat list of
    /// already-evaluated type arguments (`DataType`/`TypeVar`/value-parameter
    /// scalars). Shared by `ConstructParametricType` and
    /// `ConstructParametricTypeSplat` so the literal `Tuple{T,Float64}` path
    /// and the splatted `Tuple{xs...}` / `Core.apply_type(...)` path build
    /// identical results (Issue #5112).
    fn build_parametric_type(
        &mut self,
        base_name: &str,
        type_args: Vec<Value>,
    ) -> Result<Value, VmError> {
        use crate::types::JuliaType;

        // `Union` is a distinct applicable family (Issue #10623), NOT a
        // `UnionAll` parameter application: every argument becomes a *member* of
        // the resulting `Union{...}`, members may be arbitrary types including
        // fresh runtime `TypeVar`s, and the result is order-insensitive. Route
        // it to the identity-aware Union builder before the generic
        // string-round-trip path below (which would erase TypeVar identity and
        // collapse `Union{f1, f2}` to `F`).
        if base_name == "Union" {
            return Ok(self.build_union_value(&type_args));
        }

        // Validate every parameter value up front (Issue #11555): upstream
        // `jl_f_apply_type` rejects an invalid parameter with `TypeError`
        // instead of ever constructing a type from it. This is the same
        // `apply_type_to_runtime_base` reaches for its `UnionAll` binder loop
        // (`type_arg_value_to_julia_type`); this literal/compile-time-known-base
        // path was skipping it entirely and silently degrading to `Any`.
        for arg in &type_args {
            if self.is_valid_type_param_value(arg) {
                continue;
            }
            let expected_name = if matches!(arg, Value::BigInt(_) | Value::BigFloat(_)) {
                "Int64"
            } else {
                "Type"
            };
            let got_type_name = self.get_type_name(arg);
            return Err(self.type_error_with_payload(
                format!(
                    "in Type, in parameter, expected {}, got a value of type {}",
                    expected_name, got_type_name
                ),
                Value::Symbol(SymbolValue::new("Type")),
                Value::str_new("parameter".to_string()),
                Value::DataType(Box::new(JuliaType::from_name_or_struct(expected_name))),
                arg.clone(),
            ));
        }

        let requires_structured_encoding = type_args.iter().any(|arg| match arg {
            Value::RuntimeTypeVar(_) => true,
            Value::DataType(ty) => ty.contains_runtime_typevar() || ty.contains_unionall(),
            _ => false,
        });
        if requires_structured_encoding {
            if base_name == "Array" && type_args.len() == 2 {
                if let (Some(element), Value::I64(rank)) =
                    (type_arg_value_to_julia_type(&type_args[0]), &type_args[1])
                {
                    let ty = JuliaType::from_structured_parametric(
                        base_name.to_string(),
                        vec![element, JuliaType::Struct(rank.to_string())],
                    );
                    return Ok(Value::DataType(Box::new(ty)));
                }
            }

            let all_args_are_types = type_args
                .iter()
                .all(|arg| matches!(arg, Value::DataType(_) | Value::RuntimeTypeVar(_)));
            // `JuliaType` does not yet structurally encode value parameters
            // with their exact widths. Keep mixed type/value applications on
            // the existing value-aware rendering path (Issue #10460).
            if all_args_are_types {
                let structured = type_args
                    .iter()
                    .map(type_arg_value_to_julia_type)
                    .collect::<Option<Vec<_>>>();
                if let Some(params) = structured {
                    if let Some(partial) =
                        build_structured_partial_builtin_unionall(base_name, &params)
                    {
                        return Ok(Value::DataType(Box::new(self.canonical_partial_unionall(
                            partial,
                            true,
                            &[],
                        ))));
                    }
                    if partial_unionall_schema_allowed(base_name) {
                        if let Some(ctx) = self.compile_context.as_ref() {
                            if let Some(parametric_def) =
                                runtime_parametric_schema_for_family(ctx, base_name)
                            {
                                if let Some(partial) = self
                                    .build_structured_partial_parametric_unionall(
                                        base_name,
                                        &params,
                                        &parametric_def.def.type_params,
                                    )
                                {
                                    let stable = partial_binders_are_declaration_stable(
                                        &parametric_def.def.type_params,
                                        params.len(),
                                    );
                                    let substitutions = parametric_def
                                        .def
                                        .type_params
                                        .iter()
                                        .zip(params.iter())
                                        .map(|(param, arg)| (param.name.clone(), arg.clone()))
                                        .collect::<Vec<_>>();
                                    return Ok(Value::DataType(Box::new(
                                        self.canonical_partial_unionall(
                                            partial,
                                            stable,
                                            &substitutions,
                                        ),
                                    )));
                                }
                            }
                        }
                    }
                    let ty = JuliaType::from_structured_parametric(base_name.to_string(), params);
                    return Ok(Value::DataType(Box::new(ty)));
                }
            }
        }

        // Convert type arguments to type name strings.
        // Issue #4696: a runtime `TypeVar(:T)` projects to its name
        // ("T"), so `Vector{T}` preserves the TypeVar reference in
        // the parametric type string instead of erasing it to "Any".
        let type_arg_names: Vec<String> = type_args
            .iter()
            .map(|v| match v {
                Value::DataType(jt) => jt.name().to_string(),
                Value::RuntimeTypeVar(tv) => render_runtime_typevar_type_param(tv),
                Value::I64(n) => n.to_string(),
                Value::F64(n) => render_f64_type_param(*n),
                Value::F32(n) => render_f32_type_param(*n),
                Value::Bool(b) => b.to_string(),
                Value::Char(c) => render_char_type_param(*c),
                Value::Symbol(sym) => render_symbol_type_param(sym),
                Value::Tuple(tuple) => {
                    render_tuple_type_param(tuple).unwrap_or_else(|| "Any".to_string())
                }
                other => {
                    render_narrow_numeric_type_param(other).unwrap_or_else(|| "Any".to_string())
                }
            })
            .collect();

        if type_args.len() == 1 {
            if base_name == "BitArray" {
                if let Some(rank) = bitarray_rank_arg(&type_args[0]) {
                    let ty = match rank {
                        1 => JuliaType::Struct("BitVector".to_string()),
                        2 => JuliaType::Struct("BitMatrix".to_string()),
                        n => JuliaType::Struct(format!("BitArray{{{n}}}")),
                    };
                    return Ok(Value::DataType(Box::new(ty)));
                }
            }

            // Issue #4696: accept a fresh `Value::RuntimeTypeVar` as
            // the inner parameter (via its registry projection) so
            // `Vector{T}` keeps `T` as a TypeVar reference instead of
            // collapsing to `Vector{Any}`.
            let inner = match &type_args[0] {
                Value::DataType(jt) => Some(*jt.clone()),
                Value::RuntimeTypeVar(tv) => Some(tv.projection()),
                _ => None,
            };
            if let Some(inner) = inner {
                let constructed = match base_name {
                    "Vector" => Some(JuliaType::VectorOf(Box::new(inner))),
                    "Matrix" => Some(JuliaType::MatrixOf(Box::new(inner))),
                    "Type" => Some(JuliaType::TypeOf(Box::new(inner))),
                    _ => None,
                };
                if let Some(julia_type) = constructed {
                    return Ok(Value::DataType(Box::new(julia_type)));
                }
            }
        }

        // Construct the parametric type name: e.g., "Complex{Float64}"
        if base_name == "Tuple" {
            let tuple_types = type_arg_names
                .iter()
                .map(|name| {
                    JuliaType::from_name(name).unwrap_or_else(|| JuliaType::Struct(name.clone()))
                })
                .collect();
            return Ok(Value::DataType(Box::new(JuliaType::TupleOf(tuple_types))));
        }

        if partial_unionall_schema_allowed(base_name) {
            if let Some(ctx) = self.compile_context.as_ref() {
                if let Some(parametric_def) = runtime_parametric_schema_for_family(ctx, base_name) {
                    if let Some(partial) = self.build_partial_parametric_unionall(
                        base_name,
                        &type_arg_names,
                        &parametric_def.def.type_params,
                    ) {
                        let stable = partial_binders_are_declaration_stable(
                            &parametric_def.def.type_params,
                            type_arg_names.len(),
                        );
                        let substitutions = parametric_def
                            .def
                            .type_params
                            .iter()
                            .zip(type_arg_names.iter())
                            .map(|(param, arg)| {
                                (param.name.clone(), JuliaType::Struct(arg.clone()))
                            })
                            .collect::<Vec<_>>();
                        return Ok(Value::DataType(Box::new(self.canonical_partial_unionall(
                            partial,
                            stable,
                            &substitutions,
                        ))));
                    }
                }
            }
        }

        // An under-applied builtin family built dynamically from a *concrete*
        // prefix (`Core.apply_type(Array, Float64)`, or a runtime `Array{T}`
        // whose `T` is already bound to a concrete type) is the same trailing
        // `UnionAll` as the literal `Array{Float64}` (Issue #10586), so both
        // compare `===` and answer `isa UnionAll` alike. Construction still
        // works: the `::Type{Array{T}}` dispatch binds `T` through the
        // trailing-`UnionAll` fallback in `extract_type_bindings_with_lookup`. A
        // still-free type-variable prefix (`Array{T}` in a `where`) keeps the
        // bare schema so it prints with upstream's shorthand.
        if type_arg_names
            .iter()
            .all(|name| self.under_applied_prefix_is_bound(name))
        {
            if let Some(partial) = build_partial_builtin_unionall(base_name, &type_arg_names) {
                return Ok(Value::DataType(Box::new(self.canonical_partial_unionall(
                    partial,
                    true,
                    &[],
                ))));
            }
        }

        let type_name = if type_arg_names.is_empty() {
            base_name.to_string()
        } else {
            format!("{}{{{}}}", base_name, type_arg_names.join(", "))
        };

        Ok(Value::DataType(Box::new(JuliaType::from_name_or_struct(
            &type_name,
        ))))
    }

    /// Apply a flat parameter list to a base type that was evaluated at
    /// runtime. This mirrors upstream `jl_apply_type`: each argument consumes
    /// one `UnionAll` binder and must satisfy that binder's bounds.
    fn apply_type_to_runtime_base(
        &mut self,
        base_val: Value,
        type_args: Vec<Value>,
    ) -> Result<Value, VmError> {
        // `Union` is a distinct applicable base family (Issue #10623): unlike a
        // `UnionAll` it does not consume one binder per argument — every
        // argument becomes a member of the resulting `Union{...}`, and members
        // may be fresh runtime `TypeVar`s. Route it to the identity-aware Union
        // builder before the `UnionAll` / concrete-base handling below (which
        // would reject `Union` with "expected UnionAll, got Type{Union}").
        if let Value::DataType(jt) = &base_val {
            if matches!(jt.as_ref(), JuliaType::Struct(name) if name == "Union") {
                return Ok(self.build_union_value(&type_args));
            }
        }

        if let Value::DataType(jt) = &base_val {
            if matches!(
                jt.as_ref(),
                JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
            ) {
                let first_declared_var = {
                    let registry = super::super::type_objects::RuntimeTypeRegistry::new(
                        self.compile_context.as_ref(),
                        &self.abstract_types,
                    );
                    registry.object(jt).unionall_var()
                };
                if let Some(var @ JuliaType::TypeVar(..)) = first_declared_var {
                    // Seed the entire owner chain before promotion. Direct
                    // `Core.apply_type(wrapper, ...)` need not have reflected
                    // `.var` first, but promotion still requires the same
                    // owner-scoped IDs as the reflection path (Issue #10261).
                    self.runtime_typevar_value_for_unionall_projection(jt.as_ref(), var);
                }
                let mut instantiated =
                    Box::new(self.project_unionall_binders_for_owner(jt.as_ref(), jt.as_ref()));
                let mut bound_substitutions: HashMap<String, JuliaType> = HashMap::new();
                let mut runtime_bound_substitutions: HashMap<u64, JuliaType> = HashMap::new();
                let mut freshen_remaining_binders = false;
                for arg in &type_args {
                    let Some(arg_type) = type_arg_value_to_julia_type(arg) else {
                        return Err(VmError::TypeError(format!(
                            "Core.apply_type: type parameter must be a type or value parameter, got {:?}",
                            arg.value_type()
                        )));
                    };

                    let (var, declared_lower, declared_upper, runtime_binder_id) =
                        match instantiated.as_ref() {
                            JuliaType::UnionAll {
                                var,
                                lower_bound,
                                bound,
                                ..
                            } => (
                                var.clone(),
                                lower_bound
                                    .as_deref()
                                    .map(|bound| JuliaType::from_name_or_struct(bound.as_str())),
                                bound
                                    .as_deref()
                                    .map(|bound| JuliaType::from_name_or_struct(bound.as_str())),
                                None,
                            ),
                            JuliaType::RuntimeUnionAll { var, .. } => {
                                let JuliaType::RuntimeTypeVar {
                                    id,
                                    name,
                                    lower_bound,
                                    upper_bound,
                                    ..
                                } = var.as_ref()
                                else {
                                    return Err(VmError::ErrorException(
                                        "invalid runtime UnionAll binder".to_string(),
                                    ));
                                };
                                (
                                    name.clone(),
                                    (!matches!(lower_bound.as_ref(), JuliaType::Bottom))
                                        .then(|| lower_bound.as_ref().clone()),
                                    (!matches!(upper_bound.as_ref(), JuliaType::Any))
                                        .then(|| upper_bound.as_ref().clone()),
                                    Some(*id),
                                )
                            }
                            _ => {
                                return Err(VmError::ErrorException(format!(
                                    "too many parameters for type {}",
                                    jt.name()
                                )));
                            }
                        };
                    if let Some(id) = runtime_binder_id {
                        let mut remaining = match instantiated.as_ref() {
                            JuliaType::RuntimeUnionAll { body, .. } => body.as_ref(),
                            _ => instantiated.as_ref(),
                        };
                        while let JuliaType::RuntimeUnionAll { var, body } = remaining {
                            if matches!(
                                var.as_ref(),
                                JuliaType::RuntimeTypeVar {
                                    lower_bound,
                                    upper_bound,
                                    ..
                                } if lower_bound.references_runtime_typevar(id)
                                    || upper_bound.references_runtime_typevar(id)
                            ) {
                                freshen_remaining_binders = true;
                                break;
                            }
                            remaining = body;
                        }
                    }
                    let resolve_bound = |mut resolved: JuliaType| {
                        if runtime_binder_id.is_none() {
                            for (name, replacement) in &bound_substitutions {
                                resolved = resolved.substitute(name, replacement);
                            }
                        }
                        for (id, replacement) in &runtime_bound_substitutions {
                            resolved = resolved.substitute_runtime_typevar(*id, replacement);
                        }
                        resolved
                    };
                    // Upstream `within_typevar` asks whether the argument as
                    // supplied contains a free TypeVar. Do this before replacing
                    // earlier binder IDs for the concrete subtype comparison:
                    // `Vector{A}` remains existential even after the wrapper's
                    // earlier `A` slot was applied to `Int` (Issue #10261).
                    let argument_has_free_typevar =
                        has_free_runtime_typevar(&arg_type, &mut Vec::new());
                    let resolved_arg_type = runtime_bound_substitutions.iter().fold(
                        arg_type.clone(),
                        |resolved, (id, replacement)| {
                            resolved.substitute_runtime_typevar(*id, replacement)
                        },
                    );
                    let actual = CoreType::from(&resolved_arg_type);
                    let subtype = CoreSubtypeEngine::with_hierarchy(&self.struct_hierarchy);
                    let resolved_lower = declared_lower.map(&resolve_bound);
                    let resolved_upper = declared_upper.map(resolve_bound);
                    // User struct schemas reach this loop through one of the
                    // wrapper builders above, which expand visible aliases in
                    // both bounds before constructing the UnionAll (#11142).
                    // Mirror upstream `within_typevar`: a TypeVar argument, or
                    // a declared bound that still contains a free TypeVar, is
                    // accepted without forcing that variable to one envelope
                    // endpoint. Endpoint projection would wrongly turn a
                    // valid existential bound such as `B<:Vector{A}` into the
                    // invariant `B<:Vector{Any}` (Issue #10261).
                    let bound_has_free_typevar = resolved_lower
                        .as_ref()
                        .is_some_and(|lower| has_free_runtime_typevar(lower, &mut Vec::new()))
                        || resolved_upper
                            .as_ref()
                            .is_some_and(|upper| has_free_runtime_typevar(upper, &mut Vec::new()));
                    let defer_bound_validation =
                        argument_has_free_typevar || bound_has_free_typevar;
                    let lower_ok = resolved_lower.as_ref().is_none_or(|lower| {
                        defer_bound_validation
                            || subtype.is_subtype(&CoreType::from(lower), &actual)
                    });
                    let upper_ok = resolved_upper.as_ref().is_none_or(|upper| {
                        defer_bound_validation
                            || subtype.is_subtype(&actual, &CoreType::from(upper))
                    });
                    if !lower_ok || !upper_ok {
                        let expected = match (&resolved_lower, &resolved_upper) {
                            (Some(lower), Some(upper)) => {
                                format!("{}<:{var}<:{}", lower.name(), upper.name())
                            }
                            (Some(lower), None) => format!("{var}>:{}", lower.name()),
                            (None, Some(upper)) => format!("{var}<:{}", upper.name()),
                            (None, None) => var.clone(),
                        };
                        return Err(VmError::TypeError(format!(
                            "in {}, in {}, expected {}, got Type{{{}}}",
                            jt.name(),
                            var,
                            expected,
                            arg_type.name()
                        )));
                    }
                    if let Some(id) = runtime_binder_id {
                        runtime_bound_substitutions.insert(id, arg_type.clone());
                    }
                    bound_substitutions.insert(var.clone(), arg_type.clone());
                    *instantiated = instantiated.instantiate(&arg_type);
                }
                let instantiated = *instantiated;
                let instantiated = if freshen_remaining_binders {
                    self.freshen_runtime_unionall_binders(instantiated)
                } else {
                    instantiated
                };
                return Ok(Value::DataType(Box::new(instantiated)));
            }

            let base_name = jt.name();
            let schema_params = self
                .compile_context
                .as_ref()
                .and_then(|ctx| ctx.parametric_structs.get(base_name.as_ref()))
                .map(|parametric| parametric.def.type_params.clone())
                .or_else(|| {
                    self.abstract_types
                        .iter()
                        .find(|abstract_type| abstract_type.name == base_name.as_ref())
                        .map(|abstract_type| abstract_type.type_params.clone())
                });
            if let Some(type_params) = schema_params {
                if type_args.is_empty() {
                    return Ok(base_val);
                }
                if let Some(wrapper) =
                    self.build_partial_parametric_unionall(base_name.as_ref(), &[], &type_params)
                {
                    return self
                        .apply_type_to_runtime_base(Value::DataType(Box::new(wrapper)), type_args);
                }
            }
            if let Some(wrapper) = builtin_runtime_unionall_wrapper(base_name.as_ref()) {
                return self
                    .apply_type_to_runtime_base(Value::DataType(Box::new(wrapper)), type_args);
            }
        }

        let base_name = match &base_val {
            Value::DataType(jt) => {
                let accepts_parameters = matches!(jt.as_ref(), JuliaType::Tuple)
                    || super::super::type_objects::RuntimeTypeRegistry::new(
                        self.compile_context.as_ref(),
                        &self.abstract_types,
                    )
                    .is_unionall_like(jt);
                if !accepts_parameters {
                    return Err(VmError::TypeError(format!(
                        "in Type{{...}} expression, expected UnionAll, got Type{{{}}}",
                        jt.name()
                    )));
                }
                jt.name().to_string()
            }
            other => {
                // `x{T}` where `x` is a value (not a type): upstream raises a
                // TypeError whose `.func` is `Symbol("Type{...} expression")`,
                // `.expected` is `UnionAll`, and `.got` is the value itself
                // (Issue #11399). Park those so the funnel exposes them
                // instead of `:unknown`/`nothing` placeholders.
                let got = other.clone();
                let msg = format!(
                    "in Type{{...}} expression, expected UnionAll, got {:?}",
                    other
                );
                return Err(self.type_error_with_payload(
                    msg,
                    Value::Symbol(SymbolValue::new("Type{...} expression")),
                    Value::str_new(String::new()),
                    // Construct the nominal `UnionAll` type object directly
                    // rather than reparsing a name string (Issue #10460);
                    // `from_name_or_struct("UnionAll")` returns exactly this.
                    Value::DataType(Box::new(crate::types::JuliaType::Struct(
                        "UnionAll".to_string(),
                    ))),
                    got,
                ));
            }
        };

        self.build_parametric_type(&base_name, type_args)
    }

    /// Execute struct instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_struct(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::NewStruct(type_id, field_count) => {
                if let Some(name) = self.pending_eval_struct_name(*type_id) {
                    let local_name = name.rsplit('.').next().unwrap_or(&name).to_string();
                    self.raise(VmError::UndefVarError(local_name))?;
                    return Ok(DispatchAction::Continue);
                }
                // Get struct definition info
                let mut struct_name = self
                    .struct_defs
                    .get(*type_id)
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                let struct_def = self.struct_defs.get(*type_id).cloned();
                let actual_field_count = struct_def
                    .as_ref()
                    .map(|def| def.fields.len())
                    .unwrap_or(*field_count);

                let mut values = if *field_count == 0 && actual_field_count > 0 {
                    // Partial initialization: new() with no args
                    // Create struct with all fields set to Undef
                    vec![Value::Undef; actual_field_count]
                } else {
                    // Normal case: pop values from stack
                    let mut vals = Vec::with_capacity(*field_count);
                    for _ in 0..*field_count {
                        vals.push(self.stack.pop_value()?);
                    }
                    vals.reverse(); // Restore original order
                    vals
                };

                // Coerce field values to their declared concrete primitive
                // field types, matching Julia's default / `new` constructor
                // `convert(fieldtype, x)` step (Issue #4990).
                coerce_fields_to_declared_types(
                    &self.struct_defs,
                    &self.struct_heap,
                    struct_def.as_ref(),
                    &mut values,
                );

                // If struct has {Any} type parameters, resolve them from actual runtime values.
                if let Some(resolved_name) =
                    self.resolve_any_type_params_from_values(&struct_name, &values)
                {
                    struct_name = resolved_name;
                }

                // Issue #7964 Phase 1: intercept small Float64 SVector/SMatrix.
                if let Some(sv) = try_make_static_array(&struct_name, &values) {
                    self.stack.push(sv);
                    return Ok(DispatchAction::Continue);
                }
                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(*type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(DispatchAction::Continue)
            }

            Instr::NewStructSplat(type_id) => {
                if let Some(name) = self.pending_eval_struct_name(*type_id) {
                    let local_name = name.rsplit('.').next().unwrap_or(&name).to_string();
                    self.raise(VmError::UndefVarError(local_name))?;
                    return Ok(DispatchAction::Continue);
                }
                // Pop a tuple/array and unpack its elements into struct fields
                let val = self.stack.pop_value()?;
                let mut values: Vec<Value> = match val {
                    Value::Tuple(t) => t.elements,
                    // Route the legacy native array carrier through
                    // `native_array_value_ref` so the destructure stays
                    // centralized while #3908 retires the native container in
                    // favor of Memory-first Pure Julia Array wrappers.
                    _ if is_native_array_value(&val) => {
                        match native_array_value_ref(&val) {
                            Some(arr) => {
                                // Convert array elements to Values (works with any array type)
                                let arr_borrow = arr.borrow();
                                (0..arr_borrow.len())
                                    .map(|i| arr_borrow.get_linear(i))
                                    .collect::<Result<Vec<_>, _>>()?
                            }
                            None => vec![],
                        }
                    }
                    Value::Memory(mem) => {
                        let mem_borrow = mem.borrow();
                        (0..mem_borrow.len())
                            .map(|i| mem_borrow.get(i + 1))
                            .collect::<Result<Vec<_>, _>>()?
                    }
                    _ => {
                        self.raise(VmError::type_error_expected(
                            "new(args...)",
                            "a tuple or array",
                            &val,
                        ))?;
                        vec![]
                    }
                };
                // Coerce field values to their declared concrete primitive
                // field types (Issue #4990).
                let struct_def = self.struct_defs.get(*type_id).cloned();
                coerce_fields_to_declared_types(
                    &self.struct_defs,
                    &self.struct_heap,
                    struct_def.as_ref(),
                    &mut values,
                );
                // Get struct name from struct_defs
                let mut struct_name = self
                    .struct_defs
                    .get(*type_id)
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                // If struct has {Any} type parameters, resolve them from actual runtime values.
                if let Some(resolved_name) =
                    self.resolve_any_type_params_from_values(&struct_name, &values)
                {
                    struct_name = resolved_name;
                }
                // Issue #7964 Phase 1: intercept small Float64 SVector/SMatrix.
                if let Some(sv) = try_make_static_array(&struct_name, &values) {
                    self.stack.push(sv);
                    return Ok(DispatchAction::Continue);
                }
                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(*type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(DispatchAction::Continue)
            }

            Instr::NewParametricStruct(ref base_name, field_count) => {
                // Pop field values from stack
                let mut values = Vec::with_capacity(*field_count);
                for _ in 0..*field_count {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();

                // Construct struct name from base name and type bindings
                // e.g., "Rational" with T=Int64 becomes "Rational{Int64}"
                let struct_name = if let Some(frame) = self.frames.last() {
                    if !frame.type_bindings.is_empty() {
                        // Use the struct's declared type-parameter order, not
                        // HashMap iteration order. `new{T,V}` in an inner
                        // constructor must produce `Foo{T,V}`, not a
                        // nondeterministic permutation. (Issue #8341)
                        let ordered_type_args = self.compile_context.as_ref().and_then(|ctx| {
                            let def = &ctx.parametric_structs.get(base_name)?.def;
                            def.type_params
                                .iter()
                                .map(|tp| {
                                    frame
                                        .type_bindings
                                        .get(&tp.name)
                                        .map(|jt| jt.name().to_string())
                                })
                                .collect::<Option<Vec<String>>>()
                        });
                        let type_args: Vec<String> = ordered_type_args.unwrap_or_else(|| {
                            frame
                                .type_bindings
                                .values()
                                .map(|jt| jt.name().to_string())
                                .collect()
                        });
                        format!("{}{{{}}}", base_name, type_args.join(", "))
                    } else {
                        // No type bindings - infer from field values.
                        //
                        // Route a type-inference failure (e.g. a parametric
                        // struct whose shared field type variable cannot unify
                        // two distinct concrete field types, raising
                        // `TypeError("Inconsistent type inference for T ...")`)
                        // through `raise` so it lands in an enclosing
                        // `try`/`catch` instead of propagating as a raw `Err`
                        // out of the whole `run()` loop as an uncatchable abort
                        // (Issue #9524). Struct construction is an instruction,
                        // and the run loop only consults handlers for errors that
                        // went through `raise`; a bare `?` here escaped the
                        // `try`. On a handled error `raise` truncates the stack
                        // to the handler and jumps to `catch_ip`, so we resume by
                        // returning `Continue`.
                        let inferred = match self
                            .infer_parametric_struct_name_from_runtime_fields(base_name, &values)
                        {
                            Ok(inferred) => inferred,
                            Err(err) => {
                                self.raise(err)?;
                                return Ok(DispatchAction::Continue);
                            }
                        };
                        if let Some(runtime_inferred) = inferred {
                            runtime_inferred
                        } else {
                            let scalar_param_name = |v: &Value| -> &'static str {
                                match v {
                                    Value::I64(_) => "Int64",
                                    Value::I32(_) => "Int32",
                                    Value::I16(_) => "Int16",
                                    Value::I8(_) => "Int8",
                                    Value::I128(_) => "Int128",
                                    Value::U64(_) => "UInt64",
                                    Value::U32(_) => "UInt32",
                                    Value::U16(_) => "UInt16",
                                    Value::U8(_) => "UInt8",
                                    Value::U128(_) => "UInt128",
                                    Value::F64(_) => "Float64",
                                    Value::F32(_) => "Float32",
                                    Value::F16(_) => "Float16",
                                    Value::Bool(_) => "Bool",
                                    Value::BigInt(_) => "BigInt",
                                    _ => "Any",
                                }
                            };
                            // Issue #7972: a struct with 2+ type parameters that each
                            // appear as a bare field type (e.g. `P3{A,B}` with `a::A;
                            // b::B`) must report EVERY parameter in `typeof`
                            // (`P3{Int64, Float64}`), not just the first. The frame
                            // carried no `type_bindings` for the inner `new{A,B}(...)`,
                            // so recover each parameter from the matching field's
                            // value. Single-parameter structs and parameters that do
                            // not map to a bare field keep the first-value heuristic.
                            let multi_param_args = self.compile_context.as_ref().and_then(|ctx| {
                                let def = &ctx.parametric_structs.get(base_name)?.def;
                                if def.type_params.len() < 2 {
                                    return None;
                                }
                                def.type_params
                                    .iter()
                                    .map(|tp| {
                                        let idx = def.fields.iter().position(|f| {
                                            matches!(
                                                &f.type_expr,
                                                Some(crate::types::TypeExpr::TypeVar(n))
                                                    if n == &tp.name
                                            )
                                        })?;
                                        Some(scalar_param_name(values.get(idx)?).to_string())
                                    })
                                    .collect::<Option<Vec<String>>>()
                            });
                            match multi_param_args {
                                Some(args) => format!("{}{{{}}}", base_name, args.join(", ")),
                                None => {
                                    let type_arg =
                                        values.first().map_or("Any", |v| scalar_param_name(v));
                                    format!("{}{{{}}}", base_name, type_arg)
                                }
                            }
                        }
                    }
                } else {
                    base_name.clone()
                };

                // Find or create the type_id for this instantiation
                let type_id = self
                    .struct_defs
                    .iter()
                    .position(|d| d.name == struct_name)
                    .unwrap_or(0);

                // Issue #7964 Phase 1: intercept small Float64 SVector/SMatrix.
                if let Some(sv) = try_make_static_array(&struct_name, &values) {
                    self.stack.push(sv);
                    return Ok(DispatchAction::Continue);
                }
                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(DispatchAction::Continue)
            }

            Instr::NewDynamicParametricStruct(ref base_name, field_count, type_param_count) => {
                // Runtime type application / constructor selection owns bound
                // validation. This allocator receives concrete parameters and
                // must not reinterpret raw schema-bound spellings (#11142).
                // Pop type parameters first (they're on top of stack)
                let mut type_params = Vec::with_capacity(*type_param_count);
                for _ in 0..*type_param_count {
                    type_params.push(self.stack.pop_value()?);
                }
                type_params.reverse();

                // Pop field values
                let mut values = Vec::with_capacity(*field_count);
                for _ in 0..*field_count {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();

                // Construct struct name from base name and type parameters
                // Type parameters can be DataType values or Symbol values (for MIME{Symbol(...)} etc.)
                let type_args: Vec<String> = type_params
                    .iter()
                    .map(|v| match v {
                        Value::DataType(jt) => jt.name().to_string(),
                        Value::I64(n) => n.to_string(),
                        Value::F64(n) => render_f64_type_param(*n),
                        Value::F32(n) => render_f32_type_param(*n),
                        Value::Bool(b) => b.to_string(),
                        Value::Char(c) => render_char_type_param(*c),
                        Value::Symbol(sym) => render_symbol_type_param(sym),
                        Value::Tuple(tuple) => {
                            render_tuple_type_param(tuple).unwrap_or_else(|| "Any".to_string())
                        }
                        // Narrow-width numeric value parameters (`VP{Int8(5)}`,
                        // `VP{UInt8(5)}`, `VP{Float16(1.5)}`) previously fell
                        // through to `Any`, erasing the parameter to a
                        // `DataType` wrapper in the method body (Issue #10599).
                        // Render them the same way the type-value path does.
                        other => render_narrow_numeric_type_param(other)
                            .unwrap_or_else(|| "Any".to_string()),
                    })
                    .collect();
                let struct_name = format!("{}{{{}}}", base_name, type_args.join(", "));
                coerce_dynamic_parametric_fields_to_type_args(
                    self.compile_context.as_ref(),
                    &self.struct_heap,
                    base_name,
                    &type_args,
                    &mut values,
                );

                // Find or create the type_id for this instantiation
                let type_id = self
                    .struct_defs
                    .iter()
                    .position(|d| d.name == struct_name)
                    .or_else(|| {
                        if *type_param_count == 0 {
                            return None;
                        }
                        let any_params = vec!["Any"; *type_param_count];
                        let fallback_name = format!("{}{{{}}}", base_name, any_params.join(", "));
                        self.struct_defs
                            .iter()
                            .position(|d| d.name == fallback_name)
                    })
                    .unwrap_or(0);

                // Issue #7964 Phase 1: intercept small Float64 SVector/SMatrix.
                if let Some(sv) = try_make_static_array(&struct_name, &values) {
                    self.stack.push(sv);
                    return Ok(DispatchAction::Continue);
                }
                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(DispatchAction::Continue)
            }

            Instr::ConstructParametricType(ref base_name, num_type_args) => {
                // Pop type arguments from stack (they're on top)
                let mut type_args = Vec::with_capacity(*num_type_args);
                for _ in 0..*num_type_args {
                    type_args.push(self.stack.pop_value()?);
                }
                type_args.reverse();

                let result = self.build_parametric_type(base_name, type_args)?;
                self.stack.push(result);
                Ok(DispatchAction::Continue)
            }

            // Construct a parametric type whose type arguments may be splatted
            // collections (`Tuple{xs...}`, `Core.apply_type(base, args...)`).
            // Stack layout mirrors `CallWithSplat`: the (possibly splatted)
            // arguments are on top, oldest deepest. `splat_mask[i]` flags
            // whether argument `i` is a `...`-splat to be flattened (Issue #5112).
            Instr::ConstructParametricTypeSplat(ref base_name, ref splat_mask) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    let arg_count = splat_mask.len();
                    let mut raw_args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        let value = self.stack.pop_value()?;
                        raw_args.push(self.push_transient_root(value)?);
                    }
                    raw_args.reverse();

                    let type_args = match self.prepare_splat_argument_roots(&raw_args, splat_mask) {
                        Ok(SplatPreparation::Ready(type_args)) => {
                            self.clone_transient_roots(&type_args)?
                        }
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let result = self.build_parametric_type(base_name, type_args)?;
                    self.stack.push(result);
                    Ok(DispatchAction::Continue)
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            // `Core.apply_type(base, params...)` whose base is only known at
            // runtime. Pop `[base, params...]`, derive the base name from the
            // base type value, then build like `ConstructParametricType`
            // (Issue #5112).
            Instr::ApplyTypeDynamic(num_type_args) => {
                let mut type_args = Vec::with_capacity(*num_type_args);
                for _ in 0..*num_type_args {
                    type_args.push(self.stack.pop_value()?);
                }
                type_args.reverse();

                let base_val = self.stack.pop_value()?;
                let result = self.apply_type_to_runtime_base(base_val, type_args)?;
                self.stack.push(result);
                Ok(DispatchAction::Continue)
            }

            // Flatten the complete raw call argument list before choosing the
            // first flattened value as the base (Issues #10191, #10555).
            Instr::ApplyTypeDynamicSplat(ref splat_mask) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    let arg_count = splat_mask.len();
                    let mut raw_args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        let value = self.stack.pop_value()?;
                        raw_args.push(self.push_transient_root(value)?);
                    }
                    raw_args.reverse();

                    let args = match self.prepare_splat_argument_roots(&raw_args, splat_mask) {
                        Ok(SplatPreparation::Ready(args)) => args,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let Some((&base, type_args)) = args.split_first() else {
                        return Err(VmError::TypeError(
                            "Core.apply_type requires a base type argument".to_string(),
                        ));
                    };
                    let base_val = self.clone_transient_root(base)?;
                    let type_args = self.clone_transient_roots(type_args)?;
                    let result = self.apply_type_to_runtime_base(base_val, type_args)?;
                    self.stack.push(result);
                    Ok(DispatchAction::Continue)
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            Instr::LoadStruct(name) => {
                // Get heap index from current frame, fall back to global frame (frame 0).
                // Each frame is probed for a slot binding first, then the name-keyed
                // `locals_any` map: `StoreStruct` writes the latter, and a REPL global
                // that was value-carried (no slot, no assignment in this program — e.g.
                // a seeded ODEProblem, Issue #8260) lives only there. Without the
                // `get_local` fallback such a read raised a spurious UndefVarError.
                let val = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        self.load_slot_value_by_name(frame, name)
                            .or_else(|| frame.get_local(name))
                    })
                    .or_else(|| {
                        // Fall back to global frame for global struct variables
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                self.load_slot_value_by_name(frame, name)
                                    .or_else(|| frame.get_local(name))
                            })
                        } else {
                            None
                        }
                    });
                match val {
                    Some(Value::StructRef(idx)) => self.stack.push(Value::StructRef(idx)),
                    Some(Value::Struct(s)) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        self.stack.push(Value::StructRef(idx));
                    }
                    _ => {
                        // Variable not found - raise error instead of creating invalid reference
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        return Ok(DispatchAction::Continue);
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::StoreStruct(name) => {
                let val = self.stack.pop_value()?;
                if let Some(frame) = self.frames.last_mut() {
                    match val {
                        Value::StructRef(idx) => {
                            // Store the heap index directly
                            frame.locals_any.insert(name.clone(), Value::StructRef(idx));
                            frame.var_types.insert(name.clone(), VarTypeTag::Struct);
                        }
                        Value::Struct(s) => {
                            // Allocate on heap and store index
                            let idx = self.struct_heap.len();
                            self.struct_heap.push(s);
                            frame.locals_any.insert(name.clone(), Value::StructRef(idx));
                            frame.var_types.insert(name.clone(), VarTypeTag::Struct);
                        }
                        other => {
                            return Err(VmError::type_error_expected(
                                "StoreStruct",
                                "struct",
                                &util::value_type_name(&other),
                            ));
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::GetField(field_idx) => {
                let val = self.stack.pop_value()?;
                // Issue #7964: flat StaticArray variants — field 0 is the `.data` tuple.
                // Materialise a real TupleValue so callers that return `.data` directly
                // (e.g. `Base.Tuple(x::SVector) = x.data`) produce the correct type.
                if *field_idx == 0 {
                    if let Value::StaticArray(sv) = &val {
                        self.stack.push(Value::Tuple(sv.to_tuple_value()));
                        return Ok(DispatchAction::Continue);
                    }
                    if let Value::StaticArrayInline(sv) = &val {
                        self.stack.push(Value::Tuple(sv.to_tuple_value()));
                        return Ok(DispatchAction::Continue);
                    }
                } else if matches!(&val, Value::StaticArray(_) | Value::StaticArrayInline(_)) {
                    return Err(VmError::FieldIndexOutOfBounds {
                        index: *field_idx,
                        field_count: 1,
                    });
                }
                if let Value::Pairs(pairs) = &val {
                    let field_name = match *field_idx {
                        0 => "data",
                        1 => "itr",
                        _ => {
                            self.raise(VmError::FieldIndexOutOfBounds {
                                index: *field_idx,
                                field_count: 2,
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let Some(value) = pairs_projected_field(pairs, field_name) else {
                        return Err(VmError::InternalError(format!(
                            "Pairs projection missing field {field_name}"
                        )));
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }
                if let Value::WeakRef(cell) = &val {
                    if *field_idx == 0 {
                        self.stack.push(cell.borrow().clone());
                        return Ok(DispatchAction::Continue);
                    }
                    self.raise(VmError::FieldIndexOutOfBounds {
                        index: *field_idx,
                        field_count: 1,
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                let (field_value, field_count, _struct_name) = match &val {
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            (
                                s.get_field(*field_idx).cloned(),
                                s.values.len(),
                                s.struct_name.clone(),
                            )
                        } else {
                            // INTERNAL: GetField StructRef index is compiler-generated; invalid ref means heap corruption
                            return Err(VmError::InternalError(format!(
                                "GetField: invalid StructRef({}), heap size: {}",
                                idx,
                                self.struct_heap.len()
                            )));
                        }
                    }
                    Value::Struct(s) => (
                        s.get_field(*field_idx).cloned(),
                        s.values.len(),
                        s.struct_name.clone(),
                    ),
                    other => {
                        let frame_name = self
                            .frames
                            .last()
                            .and_then(|frame| frame.func_index)
                            .and_then(|idx| self.functions.get(idx))
                            .map(|func| {
                                let params = func
                                    .param_julia_types
                                    .iter()
                                    .map(|ty| ty.name())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("{}({})", func.name, params)
                            })
                            .unwrap_or_else(|| "<top-level>".to_string());
                        return Err(VmError::TypeError(format!(
                            "GetField({}): expected struct, got {} in {}",
                            field_idx,
                            util::value_type_name(other),
                            frame_name
                        )));
                    }
                };
                let value = match field_value {
                    Some(v) => v,
                    None => {
                        self.raise(VmError::FieldIndexOutOfBounds {
                            index: *field_idx,
                            field_count,
                        })?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                self.stack.push(value);
                Ok(DispatchAction::Continue)
            }

            Instr::GetFieldByName(field_name) => {
                let val = self.stack.pop_value()?;
                // Issue #7964: flat StaticArray variants — "data" is the only field.
                // Materialise a real TupleValue so callers that return `.data` directly
                // (e.g. `Base.Tuple(x::SVector) = x.data`) produce the correct type.
                if matches!(&val, Value::StaticArray(_) | Value::StaticArrayInline(_)) {
                    if field_name == "data" {
                        // Materialise a real TupleValue so callers that return `.data`
                        // directly (e.g. `Base.Tuple(x::SVector) = x.data`) produce the
                        // correct type rather than a raw StaticArrayInline (Issue #7964).
                        let tuple = match &val {
                            Value::StaticArray(sv) => sv.to_tuple_value(),
                            Value::StaticArrayInline(sv) => sv.to_tuple_value(),
                            _ => unreachable!(),
                        };
                        self.stack.push(Value::Tuple(tuple));
                        return Ok(DispatchAction::Continue);
                    }
                    // Issue #10212: nonexistent field -> catchable FieldError,
                    // matching upstream Julia 1.12.
                    self.raise(VmError::FieldError {
                        type_name: val.runtime_type().name().to_string(),
                        field: field_name.clone(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                if let Value::NamedTuple(named) = &val {
                    // Issue #10212: nonexistent field -> catchable FieldError.
                    let value = match named.get_by_name(field_name) {
                        Ok(value) => value.clone(),
                        Err(_) => {
                            self.raise(VmError::FieldError {
                                type_name: "NamedTuple".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Pairs(pairs) = &val {
                    if let Some(value) = pairs_projected_field(pairs, field_name) {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                    // Issue #10212: nonexistent field -> catchable FieldError.
                    self.raise(VmError::FieldError {
                        type_name: "Base.Pairs".to_string(),
                        field: field_name.clone(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Generator(generator) = &val {
                    // Issue #10212: FieldError from the projection helper must be
                    // catchable, matching upstream Julia 1.12.
                    match self.generator_projected_field(generator, field_name) {
                        Ok(value) => self.stack.push(value),
                        Err(err) => {
                            self.raise(err)?;
                        }
                    }
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Module(module) = &val {
                    // Issue #10318: a missing module binding is an UndefVarError
                    // upstream (not a field error), and it is catchable. Carry the
                    // module scope so the message keeps the module name
                    // (`not defined in `Main.<Module>``), matching upstream 1.12.
                    let value = match self.get_module_binding(&module.name, field_name) {
                        Some(value) => value,
                        None => {
                            self.raise(VmError::UndefVarErrorInModule {
                                var: field_name.clone(),
                                scope: util::module_scope_string(&module.name),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Expr(expr) = &val {
                    let value = match field_name.as_str() {
                        "head" => Value::Symbol(expr.head.clone()),
                        "args" => expr.get_args(),
                        _ => {
                            // User-visible: user code can access a nonexistent Expr
                            // field. Issue #10212: catchable FieldError, matching
                            // upstream Julia 1.12.
                            self.raise(VmError::FieldError {
                                type_name: "Expr".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::GlobalRef(gr) = &val {
                    let value = match field_name.as_str() {
                        "mod" => {
                            use crate::vm::value::ModuleValue;
                            Value::Module(Box::new(ModuleValue::new(gr.module.clone())))
                        }
                        "name" => Value::Symbol(gr.name.clone()),
                        "binding" => Value::Binding(Box::new(BindingValue::new(gr.clone()))),
                        _ => {
                            // Issue #10212: catchable FieldError, matching upstream.
                            self.raise(VmError::FieldError {
                                type_name: "GlobalRef".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Binding(binding) = &val {
                    // Issue #10067: distinguish a modeled field (`:globalref`,
                    // `:flags`) from an upstream field that exists but is
                    // unset (`:value`, `:partitions`, `:backedges` ->
                    // UndefRefError) from a name that is not a Core.Binding
                    // field at all (-> FieldError). Shared with
                    // builtins_reflection::mod so the two sites cannot drift.
                    // Issue #10212: both error shapes are catchable, matching
                    // upstream Julia 1.12.
                    let value = match binding.field_by_name(field_name) {
                        BindingFieldAccess::Value(value) => value,
                        BindingFieldAccess::Undef => {
                            self.raise(VmError::UndefRefError)?;
                            return Ok(DispatchAction::Continue);
                        }
                        BindingFieldAccess::NoField => {
                            self.raise(VmError::FieldError {
                                type_name: "Core.Binding".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::QuoteNode(inner) = &val {
                    if field_name == "value" {
                        self.stack.push(inner.as_ref().clone());
                        return Ok(DispatchAction::Continue);
                    }
                    // Issue #10212: catchable FieldError, matching upstream.
                    self.raise(VmError::FieldError {
                        type_name: "QuoteNode".to_string(),
                        field: field_name.clone(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }

                // Issue #10212 follow-up: the Any-typed dynamic dot-access chain
                // (`g(x) = x.line`) reaches `GetFieldByName` for a `LineNumberNode`
                // too, not just the `getfield` builtin. Mirror the other
                // metaprogramming arms here: `.line`/`.file` project the field
                // value, a bogus field raises a catchable FieldError.
                if let Value::LineNumberNode(ln) = &val {
                    let value = match field_name.as_str() {
                        "line" => Value::I64(ln.line),
                        "file" => match &ln.file {
                            Some(file) => Value::Symbol(SymbolValue::new(file)),
                            None => Value::Nothing,
                        },
                        _ => {
                            self.raise(VmError::FieldError {
                                type_name: "LineNumberNode".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::RuntimeTypeVar(tv) = &val {
                    let value = match field_name.as_str() {
                        "name" => {
                            Value::Symbol(crate::vm::value::SymbolValue::new(tv.name.clone()))
                        }
                        "lb" => Value::type_object(tv.lower_bound.clone()),
                        "ub" => Value::type_object(tv.upper_bound.clone()),
                        _ => {
                            // Issue #10212: catchable FieldError, matching upstream.
                            self.raise(VmError::FieldError {
                                type_name: "TypeVar".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                // Issue #3909: type-object field access (e.g. `t.body`, `t.var`)
                // when the compiler could not statically narrow `t` to
                // `ValueType::DataType` (e.g. inside `function unwrap_unionall(t)`
                // where `t` is `Any`). The static `ValueType::DataType` branch
                // in `compile/expr/struct_.rs` already routes these through
                // dedicated builtins; mirror that lookup here for the dynamic
                // path so Pure Julia helpers can iterate over UnionAll bodies.
                if let Value::DataType(jt) = &val {
                    let registry = super::super::type_objects::RuntimeTypeRegistry::new(
                        self.compile_context.as_ref(),
                        &self.abstract_types,
                    );
                    let object = registry.object(jt);
                    // Hoisted before the field match: the `FieldError` arm
                    // below needs the receiver kind after arms whose closures
                    // borrow `self` mutably (Issue #10313).
                    let receiver_is_union =
                        object.kind() == super::super::type_objects::RuntimeTypeObjectKind::Union;
                    let value = match field_name.as_str() {
                        // A `Union` type value exposes its two branch types as
                        // the fields `a`/`b`, matching upstream (Issue #10313).
                        "a" | "b" => object
                            .union_branch_field(field_name)
                            .map(Value::type_object),
                        // Issue #4722: `.parameters` is a `Core.SimpleVector` (svec).
                        // Issue #5162: include integer/value parameters (array dim
                        // `N`, `Val{5}`, ...) so the dynamic path matches both the
                        // static path and upstream Julia exactly.
                        // Issue #10606: Union-kind type values only expose `a`/`b`;
                        // reflection fields are FieldError upstream.
                        "parameters" if !receiver_is_union => {
                            let elements = object
                                .parameters_with_values()
                                .into_iter()
                                .map(|p| self.reflection_parameter_to_value_for_owner(jt, p))
                                .collect();
                            Some(Value::SimpleVector(crate::vm::value::TupleValue {
                                elements,
                            }))
                        }
                        "var" if !receiver_is_union => object.unionall_var().map(|t| {
                            if matches!(t, JuliaType::RuntimeTypeVar { .. }) {
                                Value::type_object(t)
                            } else {
                                self.runtime_typevar_value_for_unionall_projection(jt, t)
                            }
                        }),
                        "body" if !receiver_is_union => {
                            match (object.unionall_body(), object.unionall_var()) {
                                (Some(body), Some(var)) => Some(Value::DataType(Box::new(
                                    self.project_unionall_body_with_identity(jt, body, var),
                                ))),
                                (Some(body), None) => Some(Value::DataType(Box::new(body))),
                                _ => None,
                            }
                        }
                        "name"
                            if object.kind()
                                == super::super::type_objects::RuntimeTypeObjectKind::TypeVar =>
                        {
                            object
                                .typevar_name()
                                .map(|name| Value::Symbol(SymbolValue::new(&name)))
                        }
                        "name" if !receiver_is_union => {
                            Some(Value::RuntimeTypeName(Box::new(RuntimeTypeNameValue {
                                name: object.typename_symbol(),
                                identity: object.typename_identity(),
                            })))
                        }
                        "lb" if !receiver_is_union => {
                            object.typevar_lower_bound().map(Value::type_object)
                        }
                        "ub" if !receiver_is_union => {
                            object.typevar_upper_bound().map(Value::type_object)
                        }
                        _ => None,
                    };
                    match value {
                        Some(v) => {
                            self.stack.push(v);
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            // Issue #10212: catchable FieldError, matching upstream.
                            // A Union-kind receiver reports `Union` as its type
                            // name (`FieldError(Union, :c)` upstream, Issue #10313).
                            let type_name = if receiver_is_union {
                                "Union"
                            } else {
                                "DataType"
                            };
                            self.raise(VmError::FieldError {
                                type_name: type_name.to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }

                if let Value::RuntimeTypeName(type_name) = &val {
                    match field_name.as_str() {
                        "name" => {
                            self.stack
                                .push(Value::Symbol(SymbolValue::new(&type_name.name)));
                            return Ok(DispatchAction::Continue);
                        }
                        "wrapper" => {
                            if let Some(wrapper) = self.runtime_type_wrapper(&type_name.identity) {
                                self.stack.push(Value::DataType(Box::new(wrapper)));
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        _ => {}
                    }
                    // Issue #10212: catchable FieldError, matching upstream.
                    self.raise(VmError::FieldError {
                        type_name: "Core.TypeName".to_string(),
                        field: field_name.clone(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }

                // Base.RefValue{T} field access: `r.x` returns the boxed value
                // (Issue #5130), matching upstream `RefValue.x`.
                if let Value::Ref(cell) = &val {
                    if field_name == "x" {
                        let v = cell.borrow().clone();
                        self.stack.push(v);
                        return Ok(DispatchAction::Continue);
                    }
                    // Issue #10212: catchable FieldError, matching upstream.
                    self.raise(VmError::FieldError {
                        type_name: "Base.RefValue".to_string(),
                        field: field_name.clone(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }

                if let Value::WeakRef(cell) = &val {
                    if field_name == "value" {
                        let v = cell.borrow().clone();
                        self.stack.push(v);
                        return Ok(DispatchAction::Continue);
                    }
                    // Issue #10212: catchable FieldError, matching upstream.
                    self.raise(VmError::FieldError {
                        type_name: "WeakRef".to_string(),
                        field: field_name.clone(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }

                // Migration bridge for Array-as-Memory wrapper semantics (Issue #3908).
                // The Pure Julia Array wrapper methods access `._mem` and `._size`;
                // legacy Rust-backed arrays project those fields so old arrays can
                // execute the same wrapper methods during the migration.
                if let Some(arr) = native_array_value_ref(&val) {
                    let value = match field_name.as_str() {
                        "_mem" => array_value(arr.clone()),
                        "_size" => {
                            let elements = arr
                                .borrow()
                                .shape
                                .iter()
                                .map(|&d| Value::I64(d as i64))
                                .collect();
                            Value::Tuple(crate::vm::value::TupleValue::new(elements))
                        }
                        _ => {
                            // Issue #10212: nonexistent field on a legacy native
                            // array -> catchable FieldError, matching upstream.
                            self.raise(VmError::FieldError {
                                type_name: util::value_type_name(&val).to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                // Handle RegexMatch field access. Projected through the shared
                // `RegexMatchValue::field_by_name` authority (Issue #11382) so
                // this dot-access path, `getfield`, and `fieldnames` cannot
                // drift apart on RegexMatch's five upstream physical fields
                // (`match`, `captures`, `offset`, `offsets`, `regex`).
                if let Value::RegexMatch(m) = &val {
                    let value = match m.field_by_name(field_name)? {
                        Some(value) => value,
                        None => {
                            // User-visible: `m.bogus` on a RegexMatch reaches this
                            // arm. Issue #10212: catchable FieldError, matching
                            // upstream Julia 1.12.
                            self.raise(VmError::FieldError {
                                type_name: "RegexMatch".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                // Handle Regex field access (Issue #10173). Upstream `Regex`
                // exposes `pattern` (plus `compile_options`/`match_options`/
                // `regex`, which sjulia does not model); anything else is a
                // catchable `FieldError`.
                if let Value::Regex(r) = &val {
                    let value = match field_name.as_str() {
                        "pattern" => Value::str_new(r.pattern.clone()),
                        _ => {
                            self.raise(VmError::FieldError {
                                type_name: "Regex".to_string(),
                                field: field_name.clone(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                let (struct_instance, struct_name) = match &val {
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            (s.clone(), s.struct_name.clone())
                        } else {
                            // User-visible: user can access a nonexistent field on a RegexMatch value
                            return Err(VmError::TypeError(format!(
                                "GetFieldByName: invalid StructRef({}), heap size: {}",
                                idx,
                                self.struct_heap.len()
                            )));
                        }
                    }
                    Value::Struct(s) => (s.clone(), s.struct_name.clone()),
                    other => {
                        // Issue #10319: a non-struct receiver reaching this
                        // fallback (Int64, Float64, Bool, Tuple, ...) has no
                        // declared fields. Upstream Julia's default
                        // `getproperty` falls back to `getfield`, which
                        // raises a catchable FieldError for a missing field
                        // on ANY value (not just structs) — verified against
                        // `julia` 1.12 for Int64/Float64/Bool/Char/String/
                        // Symbol/Tuple. Match that shape here instead of the
                        // internal "expected struct" TypeError, which stays
                        // reserved for the genuine VM-invariant failure above
                        // (a dangling `StructRef` into a corrupted heap).
                        self.raise(VmError::FieldError {
                            type_name: util::value_type_name(other).to_string(),
                            field: field_name.clone(),
                        })?;
                        return Ok(DispatchAction::Continue);
                    }
                };

                // Look up the struct definition to find field index by name
                let type_id = struct_instance.type_id;
                let field_idx = if let Some(def) = self.struct_defs.get(type_id) {
                    def.fields.iter().position(|(name, _)| name == field_name)
                } else {
                    None
                };

                // Fallback: if struct_defs lookup failed but this is a Complex struct,
                // resolve "re"/"im" fields directly (Complex always has re=0, im=1).
                // This handles Complex structs returned from interleaved array storage
                // where type_id may not match the runtime struct_defs ordering.
                let field_idx = field_idx.or_else(|| {
                    if struct_instance.is_complex() {
                        match field_name.as_str() {
                            "re" => Some(0),
                            "im" => Some(1),
                            _ => None,
                        }
                    } else {
                        // Try scanning all struct_defs to find correct definition by name
                        for def in &self.struct_defs {
                            if *def.name == *struct_name {
                                if let Some(pos) =
                                    def.fields.iter().position(|(name, _)| name == field_name)
                                {
                                    return Some(pos);
                                }
                            }
                        }
                        None
                    }
                });

                // Fallback for parametric structs whose concrete instantiation is
                // not in `struct_defs` (Issue #7958). A module-qualified parametric
                // *inner* constructor (`Mod.Wrapped(x)` -> `new{T}(...)`) produces an
                // instance whose `struct_name` is the instantiation `Wrapped{Int64}`
                // and whose `type_id` falls back to 0 because that instantiation was
                // never registered; the by-name scan above also misses it (defs hold
                // the base name `Wrapped`, not `Wrapped{Int64}`). The parametric
                // schema in the compile context is keyed by the base name and carries
                // the declared field order, so resolve the index there. `getfield(w, i)`
                // already works because it is positional; this restores named `w.x`.
                let field_idx = field_idx.or_else(|| {
                    let base = struct_name.split('{').next().unwrap_or(&*struct_name);
                    self.compile_context.as_ref().and_then(|ctx| {
                        ctx.parametric_structs
                            .get(base)
                            .and_then(|ps| ps.def.fields.iter().position(|f| &f.name == field_name))
                    })
                });

                match field_idx {
                    Some(idx) => {
                        if let Some(value) = struct_instance.get_field(idx) {
                            self.stack.push(value.clone());
                        } else {
                            self.raise(VmError::FieldIndexOutOfBounds {
                                index: idx,
                                field_count: struct_instance.values.len(),
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    None => {
                        if let Some(value) =
                            array_wrapper_compat_get_field(&struct_instance, field_name)
                        {
                            self.stack.push(value);
                            return Ok(DispatchAction::Continue);
                        }
                        // User-visible: user can access a nonexistent field on a
                        // struct type. Issue #10212: catchable FieldError,
                        // matching upstream Julia 1.12.
                        self.raise(VmError::FieldError {
                            type_name: struct_name.to_string(),
                            field: field_name.clone(),
                        })?;
                        return Ok(DispatchAction::Continue);
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::GetExprField(field_idx) => {
                let val = self.stack.pop_value()?;
                let expr = match val {
                    Value::Expr(e) => e,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetExprField",
                            "Expr",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                let field_val = match *field_idx {
                    EXPR_FIELD_HEAD_INDEX => Value::Symbol(expr.head.clone()),
                    EXPR_FIELD_ARGS_INDEX => expr.get_args(),
                    _ => {
                        // INTERNAL: GetExprField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetExprField: field index {} out of bounds (expected {} or {})",
                            field_idx, EXPR_FIELD_HEAD_INDEX, EXPR_FIELD_ARGS_INDEX
                        )));
                    }
                };
                self.stack.push(field_val);
                Ok(DispatchAction::Continue)
            }

            Instr::GetLineNumberNodeField(field_idx) => {
                let val = self.stack.pop_value()?;
                let ln = match val {
                    Value::LineNumberNode(ln) => ln,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetLineNumberNodeField",
                            "LineNumberNode",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                let field_val = match *field_idx {
                    LINE_NUMBER_NODE_FIELD_LINE_INDEX => Value::I64(ln.line),
                    LINE_NUMBER_NODE_FIELD_FILE_INDEX => match ln.file {
                        Some(file) => Value::Symbol(SymbolValue::new(file)),
                        None => Value::Nothing,
                    },
                    _ => {
                        // INTERNAL: GetLineNumberNodeField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetLineNumberNodeField: field index {} out of bounds (expected {} or {})",
                            field_idx, LINE_NUMBER_NODE_FIELD_LINE_INDEX, LINE_NUMBER_NODE_FIELD_FILE_INDEX
                        )));
                    }
                };
                self.stack.push(field_val);
                Ok(DispatchAction::Continue)
            }

            Instr::GetQuoteNodeValue => {
                let val = self.stack.pop_value()?;
                let inner = match val {
                    Value::QuoteNode(inner) => *inner,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetQuoteNodeValue",
                            "QuoteNode",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                self.stack.push(inner);
                Ok(DispatchAction::Continue)
            }

            Instr::GetGlobalRefField(field_idx) => {
                let val = self.stack.pop_value()?;
                let gr = match val {
                    Value::GlobalRef(gr) => gr,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetGlobalRefField",
                            "GlobalRef",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                let field_val = match *field_idx {
                    GLOBAL_REF_FIELD_MODULE_INDEX => {
                        // .mod returns the Module (we create a Module value from the name)
                        use crate::vm::value::ModuleValue;
                        Value::Module(Box::new(ModuleValue::new(gr.module.clone())))
                    }
                    GLOBAL_REF_FIELD_NAME_INDEX => Value::Symbol(gr.name.clone()),
                    GLOBAL_REF_FIELD_BINDING_INDEX => {
                        Value::Binding(Box::new(BindingValue::new(gr.clone())))
                    }
                    _ => {
                        // INTERNAL: GetGlobalRefField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetGlobalRefField: field index {} out of bounds (expected {}, {}, or {})",
                            field_idx,
                            GLOBAL_REF_FIELD_MODULE_INDEX,
                            GLOBAL_REF_FIELD_NAME_INDEX,
                            GLOBAL_REF_FIELD_BINDING_INDEX
                        )));
                    }
                };
                self.stack.push(field_val);
                Ok(DispatchAction::Continue)
            }

            Instr::SetField(field_idx) => {
                let value = self.stack.pop_value()?;
                let struct_val = self.stack.pop_value()?;

                match struct_val {
                    Value::WeakRef(cell) => {
                        if *field_idx == 0 {
                            *cell.borrow_mut() = value;
                            self.stack.push(Value::WeakRef(cell));
                        } else {
                            self.raise(VmError::FieldIndexOutOfBounds {
                                index: *field_idx,
                                field_count: 1,
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    Value::StructRef(idx) => {
                        // Get type_id from heap
                        let type_id = self.struct_heap.get(idx).map(|s| s.type_id).unwrap_or(0);

                        // Check if struct is mutable
                        let is_mutable = self
                            .struct_defs
                            .get(type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);

                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(DispatchAction::Continue);
                        }

                        // Modify struct in heap directly
                        let set_result = if let Some(s) = self.struct_heap.get_mut(idx) {
                            s.set_field(*field_idx, value)
                        } else {
                            Ok(())
                        };
                        if self.try_or_handle(set_result)?.is_none() {
                            return Ok(DispatchAction::Continue);
                        }
                        // Push the same reference back
                        self.stack.push(Value::StructRef(idx));
                    }
                    Value::Struct(mut s) => {
                        // Check if struct is mutable
                        let is_mutable = self
                            .struct_defs
                            .get(s.type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);

                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(s.type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(DispatchAction::Continue);
                        }

                        if self
                            .try_or_handle(s.set_field(*field_idx, value))?
                            .is_none()
                        {
                            return Ok(DispatchAction::Continue);
                        }
                        // Allocate on heap and push reference
                        let new_idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        self.stack.push(Value::StructRef(new_idx));
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "SetField",
                            "struct",
                            &util::value_type_name(&other),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::SetFieldByName(field_name) => {
                // Runtime field set by name - resolves correct field index at runtime
                // to avoid non-deterministic compile-time struct_table iteration.
                // (Issue #2748)
                let value = self.stack.pop_value()?;
                let struct_val = self.stack.pop_value()?;

                match struct_val {
                    Value::Module(module) => {
                        let qualified = format!("{}.{}", module.name, field_name);
                        self.store_global_value(&qualified, value);
                        self.stack.push(Value::Module(module));
                    }
                    Value::WeakRef(cell) => {
                        if field_name == "value" {
                            *cell.borrow_mut() = value;
                            self.stack.push(Value::WeakRef(cell));
                            return Ok(DispatchAction::Continue);
                        }
                        // Issue #10212: catchable FieldError, matching upstream.
                        self.raise(VmError::FieldError {
                            type_name: "WeakRef".to_string(),
                            field: field_name.clone(),
                        })?;
                        return Ok(DispatchAction::Continue);
                    }
                    Value::StructRef(idx) => {
                        let type_id = self.struct_heap.get(idx).map(|s| s.type_id).unwrap_or(0);

                        // Check mutability
                        let is_mutable = self
                            .struct_defs
                            .get(type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);
                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(DispatchAction::Continue);
                        }

                        // Look up field index by name at runtime
                        let field_idx = self.struct_defs.get(type_id).and_then(|def| {
                            def.fields.iter().position(|(name, _)| name == field_name)
                        });

                        // Fallback: scan struct_defs by struct name
                        let field_idx = field_idx.or_else(|| {
                            let struct_name = self
                                .struct_heap
                                .get(idx)
                                .map(|s| s.struct_name.clone())
                                .unwrap_or_default();
                            for def in &self.struct_defs {
                                if *def.name == *struct_name {
                                    if let Some(pos) =
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    {
                                        return Some(pos);
                                    }
                                }
                            }
                            None
                        });

                        match field_idx {
                            Some(fi) => {
                                let set_result = if let Some(s) = self.struct_heap.get_mut(idx) {
                                    s.set_field(fi, value)
                                } else {
                                    Ok(())
                                };
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                                self.stack.push(Value::StructRef(idx));
                            }
                            None => {
                                if let Some(s) = self.struct_heap.get_mut(idx) {
                                    if array_wrapper_compat_set_field(s, field_name, value)? {
                                        self.stack.push(Value::StructRef(idx));
                                        return Ok(DispatchAction::Continue);
                                    }
                                }
                                // User-visible: user can attempt to set a nonexistent
                                // field on a mutable struct (StructRef path).
                                // Issue #10212: catchable FieldError, matching upstream.
                                let type_name = self
                                    .struct_heap
                                    .get(idx)
                                    .map(|s| s.struct_name.to_string())
                                    .unwrap_or_else(|| "unknown".to_string());
                                self.raise(VmError::FieldError {
                                    type_name,
                                    field: field_name.clone(),
                                })?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    }
                    Value::Struct(mut s) => {
                        let type_id = s.type_id;

                        let is_mutable = self
                            .struct_defs
                            .get(type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);
                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(DispatchAction::Continue);
                        }

                        let field_idx = self.struct_defs.get(type_id).and_then(|def| {
                            def.fields.iter().position(|(name, _)| name == field_name)
                        });

                        // Fallback: scan by struct name
                        let field_idx = field_idx.or_else(|| {
                            for def in &self.struct_defs {
                                if *def.name == *s.struct_name {
                                    if let Some(pos) =
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    {
                                        return Some(pos);
                                    }
                                }
                            }
                            None
                        });

                        match field_idx {
                            Some(fi) => {
                                if self.try_or_handle(s.set_field(fi, value))?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                                let new_idx = self.struct_heap.len();
                                self.struct_heap.push(s);
                                self.stack.push(Value::StructRef(new_idx));
                            }
                            None => {
                                if array_wrapper_compat_set_field(&mut s, field_name, value)? {
                                    let new_idx = self.struct_heap.len();
                                    self.struct_heap.push(s);
                                    self.stack.push(Value::StructRef(new_idx));
                                    return Ok(DispatchAction::Continue);
                                }
                                // User-visible: user can attempt to set a nonexistent
                                // field on a mutable struct (Struct path).
                                // Issue #10212: catchable FieldError, matching upstream.
                                let type_name = s.struct_name.to_string();
                                self.raise(VmError::FieldError {
                                    type_name,
                                    field: field_name.clone(),
                                })?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "SetFieldByName",
                            "struct",
                            &util::value_type_name(&other),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ReturnStruct => {
                let val = self.stack.pop_value()?;
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));
                if is_hof_return && is_value_mode {
                    self.handle_hof_return_value(val)?;
                    Ok(DispatchAction::Continue)
                } else if self.handle_generator_iterate_return(val.clone())?
                    || self.handle_composed_call_return(val.clone())?
                {
                    // A struct value (e.g. a `Pair` produced by `x => x*x`)
                    // returned from a `map`/`collect`/generator mapping function
                    // must be collected by the generator-iterate continuation
                    // rather than leaking past it. Without this the Pair was
                    // returned to the iterate driver verbatim, which then
                    // mis-collected it (only the `.first` field survived),
                    // yielding `Vector{Int64}` instead of `Vector{Pair}`
                    // (Issue #5233, family of #5231).
                    Ok(DispatchAction::Continue)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.pop_call_frame();
                    self.ip = return_ip;
                    // Keep StructRef for internal returns
                    self.stack.push(val);
                    Ok(DispatchAction::Continue)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    // Convert StructRef to Struct for final return
                    match val {
                        Value::StructRef(idx) => {
                            if let Some(s) = self.struct_heap.get(idx) {
                                Ok(DispatchAction::Exit(Value::Struct(s.clone())))
                            } else {
                                Ok(DispatchAction::Exit(Value::Struct(StructInstance::new(
                                    0,
                                    Vec::new(),
                                ))))
                            }
                        }
                        Value::Struct(s) => Ok(DispatchAction::Exit(Value::Struct(s))),
                        other => Err(VmError::type_error_expected(
                            "ReturnStruct",
                            "struct",
                            &util::value_type_name(&other),
                        )),
                    }
                }
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{builtin_family_type_params, expand_runtime_type_params};
    use crate::rng::StableRng;
    use crate::types::TypeParam;
    use crate::vm::types::StructDefInfo;
    use crate::vm::value::{
        new_memory_ref, ArrayElementType, ArrayValue, MemoryValue, StructInstance,
    };
    use crate::vm::{Instr, Value, ValueType, Vm, VmError};
    use std::collections::HashMap;

    fn runtime_bound_aliases(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(name, target)| ((*name).to_string(), (*target).to_string()))
            .collect()
    }

    #[test]
    fn builtin_memory_and_ntuple_family_schemas_cover_default_field_types_11147() {
        assert_eq!(
            builtin_family_type_params("Memory"),
            Some(&[("T", None)][..])
        );
        assert_eq!(
            builtin_family_type_params("MemoryRef"),
            Some(&[("T", None)][..])
        );
        assert_eq!(
            builtin_family_type_params("NTuple"),
            Some(&[("N", None), ("T", None)][..])
        );
    }

    #[test]
    fn runtime_bound_alias_matrix_issue_11142() {
        let aliases = runtime_bound_aliases(&[
            ("Bounds.Exact", "Real"),
            ("Only.Unique", "Integer"),
            ("Left.Shared", "Integer"),
            ("Right.Shared", "String"),
            ("Bounds.Member", "Integer"),
            ("Bounds.Either", "Union{Bounds.Member, String}"),
            ("Bounds.Nested", "Vector{Bounds.Either}"),
        ]);

        let cases = [
            ("Bounds.Exact", "Real"),
            ("Unique", "Integer"),
            ("Shared", "Shared"),
            ("Bounds.Either", "Union{Integer, String}"),
            ("Bounds.Nested", "Vector{Union{Integer, String}}"),
        ];
        for (surface, expected) in cases {
            let expanded = expand_runtime_type_params(
                &[TypeParam::with_upper_bound(
                    "T".to_string(),
                    surface.to_string(),
                )],
                &aliases,
            );
            assert_eq!(
                expanded[0].get_upper_bound().map(String::as_str),
                Some(expected)
            );
        }
    }

    #[test]
    fn runtime_bound_alias_authority_covers_both_bounds_and_binders_issue_11142() {
        let aliases = runtime_bound_aliases(&[
            ("Bounds.Low", "Integer"),
            ("Bounds.High", "Real"),
            ("T", "String"),
        ]);
        let params = [TypeParam::with_both_bounds(
            "T".to_string(),
            "Bounds.Low".to_string(),
            "Bounds.High".to_string(),
        )];

        let expanded = expand_runtime_type_params(&params, &aliases);
        assert_eq!(expanded[0].lower_bound.as_deref(), Some("Integer"));
        assert_eq!(
            expanded[0].get_upper_bound().map(String::as_str),
            Some("Real")
        );

        let binder_bound = [TypeParam::with_upper_bound(
            "T".to_string(),
            "T".to_string(),
        )];
        let expanded_binder = expand_runtime_type_params(&binder_bound, &aliases);
        assert_eq!(
            expanded_binder[0].get_upper_bound().map(String::as_str),
            Some("T")
        );
    }

    #[test]
    fn coerce_fields_preserves_declared_uint64_width_issue_4990() {
        use crate::types::JuliaType;
        let def = StructDefInfo {
            name: "FooUInt64Field".to_string(),
            is_mutable: false,
            fields: vec![("x".to_string(), ValueType::I64)],
            field_julia_types: vec![JuliaType::UInt64],
            parent_type: None,
        };
        // An Int64-tagged value flowing into a `::UInt64` field must be
        // converted to UInt64 (Issue #4990).
        let mut values = vec![Value::I64(2)];
        super::coerce_fields_to_declared_types(&[], &[], Some(&def), &mut values);
        assert!(matches!(values.first(), Some(Value::U64(2))));
    }

    #[test]
    fn coerce_fields_coerces_mixed_integer_and_float_widths_issue_4990() {
        use crate::types::JuliaType;
        let def = StructDefInfo {
            name: "MixedTypedFields".to_string(),
            is_mutable: false,
            fields: vec![
                ("a".to_string(), ValueType::I64),
                ("b".to_string(), ValueType::I64),
                ("c".to_string(), ValueType::F32),
            ],
            field_julia_types: vec![JuliaType::UInt8, JuliaType::Int32, JuliaType::Float32],
            parent_type: None,
        };
        let mut values = vec![Value::I64(1), Value::I64(2), Value::I64(3)];
        super::coerce_fields_to_declared_types(&[], &[], Some(&def), &mut values);
        assert!(matches!(values.first(), Some(Value::U8(1))));
        assert!(matches!(values.get(1), Some(Value::I32(2))));
        assert!(matches!(values.get(2), Some(Value::F32(_))));
    }

    #[test]
    fn coerce_fields_leaves_any_and_struct_fields_untouched_issue_4990() {
        use crate::types::JuliaType;
        let def = StructDefInfo {
            name: "Untyped".to_string(),
            is_mutable: false,
            fields: vec![
                ("x".to_string(), ValueType::Any),
                ("y".to_string(), ValueType::I64),
            ],
            // `Any` field must keep its runtime value; a non-numeric value in a
            // numeric field must not be force-converted.
            field_julia_types: vec![JuliaType::Any, JuliaType::Int64],
            parent_type: None,
        };
        let mut values = vec![Value::U64(7), Value::str_new("hi")];
        super::coerce_fields_to_declared_types(&[], &[], Some(&def), &mut values);
        assert!(matches!(values.first(), Some(Value::U64(7))));
        assert!(matches!(values.get(1), Some(Value::Str(_))));
    }

    #[test]
    fn coerce_fields_converts_declared_complex_field_issue_9381() {
        use crate::types::JuliaType;
        let complex_f64_def = StructDefInfo {
            name: "Complex{Float64}".to_string(),
            is_mutable: false,
            fields: vec![
                ("re".to_string(), ValueType::F64),
                ("im".to_string(), ValueType::F64),
            ],
            field_julia_types: vec![JuliaType::Float64, JuliaType::Float64],
            parent_type: Some("Number".to_string()),
        };
        let holder_def = StructDefInfo {
            name: "HasComplex".to_string(),
            is_mutable: false,
            fields: vec![("z".to_string(), ValueType::Struct(0))],
            field_julia_types: vec![JuliaType::Struct("Complex{Float64}".to_string())],
            parent_type: None,
        };
        let mut values = vec![Value::Struct(StructInstance::complex_from_storage(
            1,
            "Complex{Int64}".to_string(),
            Value::I64(2),
            Value::Bool(true),
        ))];

        super::coerce_fields_to_declared_types(
            &[complex_f64_def, holder_def.clone()],
            &[],
            Some(&holder_def),
            &mut values,
        );

        let Some(Value::Struct(converted)) = values.first() else {
            panic!("expected converted Complex struct, got {values:?}");
        };
        assert_eq!(converted.type_id, 0);
        assert_eq!(&*converted.struct_name, "Complex{Float64}");
        assert!(matches!(converted.values.first(), Some(Value::F64(2.0))));
        assert!(matches!(converted.values.get(1), Some(Value::F64(1.0))));
    }

    #[test]
    fn new_struct_splat_reads_memory_storage_directly() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_defs.push(StructDefInfo {
            name: "Pair".to_string(),
            is_mutable: false,
            fields: vec![
                ("a".to_string(), ValueType::I64),
                ("b".to_string(), ValueType::I64),
            ],
            field_julia_types: vec![
                crate::types::JuliaType::Int64,
                crate::types::JuliaType::Int64,
            ],
            parent_type: None,
        });

        let mut mem = MemoryValue::undef_typed(&ArrayElementType::I64, 2);
        assert!(mem.set(1, Value::I64(10)).is_ok());
        assert!(mem.set(2, Value::I64(20)).is_ok());
        vm.stack.push(Value::Memory(new_memory_ref(mem)));

        assert!(vm.execute_struct(&Instr::NewStructSplat(0)).is_ok());

        match vm.stack.pop() {
            Some(Value::StructRef(idx)) => {
                let instance = &vm.struct_heap[idx];
                assert!(matches!(instance.values.first(), Some(Value::I64(10))));
                assert!(matches!(instance.values.get(1), Some(Value::I64(20))));
                assert_eq!(instance.values.len(), 2);
            }
            other => panic!("expected StructRef from NewStructSplat, got {other:?}"),
        }
    }

    #[test]
    fn construct_parametric_type_preserves_integer_value_params_issue_4644() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack
            .push(Value::DataType(Box::new(crate::types::JuliaType::Int64)));
        vm.stack.push(Value::I64(1));

        assert!(vm
            .execute_struct(&Instr::ConstructParametricType("Array".to_string(), 2))
            .is_ok());

        match vm.stack.pop() {
            Some(Value::DataType(julia_type)) => {
                assert!(
                    julia_type.type_eq(&crate::types::JuliaType::VectorOf(Box::new(
                        crate::types::JuliaType::Int64
                    )))
                );
            }
            other => panic!("expected DataType from ConstructParametricType, got {other:?}"),
        }
    }

    #[test]
    fn construct_parametric_type_splat_expands_array_wrapper_issue_5112() {
        use crate::types::JuliaType;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let args = ArrayValue::any_vector(vec![
            Value::DataType(Box::new(JuliaType::Int64)),
            Value::DataType(Box::new(JuliaType::Real)),
        ]);
        let wrapper = match vm.array_value_to_wrapper(args) {
            Ok(wrapper) => wrapper,
            Err(err) => panic!("test array should convert to an Array wrapper: {err:?}"),
        };
        vm.stack.push(wrapper);

        assert!(vm
            .execute_struct(&Instr::ConstructParametricTypeSplat(
                "Tuple".to_string(),
                vec![true],
            ))
            .is_ok());

        match vm.stack.pop() {
            Some(Value::DataType(julia_type)) => {
                assert_eq!(
                    *julia_type,
                    JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Real])
                );
            }
            other => panic!("expected DataType from ConstructParametricTypeSplat, got {other:?}"),
        }
    }

    #[test]
    fn apply_type_dynamic_instantiates_nested_unionall_issue_5053() {
        use crate::types::JuliaType;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let nested = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                lower_bound: None,
                var: "U".to_string(),
                bound: None,
                body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TupleOf(vec![
                    JuliaType::TypeVar("T".to_string(), None),
                    JuliaType::TypeVar("U".to_string(), None),
                ])))),
            }),
        };
        vm.stack.push(Value::DataType(Box::new(nested)));
        vm.stack.push(Value::DataType(Box::new(JuliaType::Int64)));
        vm.stack.push(Value::DataType(Box::new(JuliaType::String)));

        assert!(vm.execute_struct(&Instr::ApplyTypeDynamic(2)).is_ok());

        match vm.stack.pop() {
            Some(Value::DataType(julia_type)) => {
                assert_eq!(
                    *julia_type,
                    JuliaType::VectorOf(Box::new(JuliaType::TupleOf(vec![
                        JuliaType::Int64,
                        JuliaType::String,
                    ])))
                );
            }
            other => panic!("expected DataType from ApplyTypeDynamic, got {other:?}"),
        }
    }

    #[test]
    fn apply_type_dynamic_splat_expands_and_instantiates_unionall_issue_10191() {
        use crate::types::JuliaType;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let nested = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                lower_bound: None,
                var: "U".to_string(),
                bound: None,
                body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TupleOf(vec![
                    JuliaType::TypeVar("T".to_string(), None),
                    JuliaType::TypeVar("U".to_string(), None),
                ])))),
            }),
        };
        let splatted = ArrayValue::any_vector(vec![Value::DataType(Box::new(JuliaType::String))]);
        let wrapper = match vm.array_value_to_wrapper(splatted) {
            Ok(wrapper) => wrapper,
            Err(err) => panic!("test array should convert to an Array wrapper: {err:?}"),
        };

        vm.stack.push(Value::DataType(Box::new(nested)));
        vm.stack.push(Value::DataType(Box::new(JuliaType::Int64)));
        vm.stack.push(wrapper);

        assert!(vm
            .execute_struct(&Instr::ApplyTypeDynamicSplat(vec![false, false, true]))
            .is_ok());

        match vm.stack.pop() {
            Some(Value::DataType(julia_type)) => {
                assert_eq!(
                    *julia_type,
                    JuliaType::VectorOf(Box::new(JuliaType::TupleOf(vec![
                        JuliaType::Int64,
                        JuliaType::String,
                    ])))
                );
            }
            other => panic!("expected DataType from ApplyTypeDynamicSplat, got {other:?}"),
        }
    }

    #[test]
    fn construct_parametric_type_splat_validates_iterable_11372() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(Value::Nothing);

        let error = match vm.execute_struct(&Instr::ConstructParametricTypeSplat(
            "Tuple".to_string(),
            vec![true],
        )) {
            Err(error) => error,
            Ok(_) => panic!("invalid type splat unexpectedly returned"),
        };
        assert!(matches!(
            error,
            VmError::MethodError(message)
                if message.contains("iterate") && message.contains("Nothing")
        ));
    }

    #[test]
    fn apply_type_dynamic_splat_validates_iterable_11372() {
        use crate::types::JuliaType;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(Value::DataType(Box::new(JuliaType::Tuple)));
        vm.stack.push(Value::Nothing);

        let error = match vm.execute_struct(&Instr::ApplyTypeDynamicSplat(vec![false, true])) {
            Err(error) => error,
            Ok(_) => panic!("invalid apply_type splat unexpectedly returned"),
        };
        assert!(matches!(
            error,
            VmError::MethodError(message)
                if message.contains("iterate") && message.contains("Nothing")
        ));
    }
}

#[cfg(test)]
mod issue_10460_tests {
    use super::*;
    use crate::rng::StableRng;
    use crate::types::JuliaType;

    #[test]
    fn construct_array_preserves_unionall_element_type_issue_10460() {
        let element = JuliaType::UnionAll {
            var: "N".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::Struct("SubArray{Int8, N}".to_string())),
        };
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(Value::DataType(Box::new(element.clone())));
        vm.stack.push(Value::I64(1));

        assert!(vm
            .execute_struct(&Instr::ConstructParametricType("Array".to_string(), 2))
            .is_ok());

        let expected = JuliaType::from_structured_parametric(
            "Array".to_string(),
            vec![element, JuliaType::Struct("1".to_string())],
        );
        assert!(matches!(
            vm.stack.pop(),
            Some(Value::DataType(actual)) if *actual == expected
        ));
    }
}
