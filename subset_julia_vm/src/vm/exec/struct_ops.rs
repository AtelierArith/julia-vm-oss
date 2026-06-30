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
    MemoryRefValue, RuntimeTypeNameValue, RuntimeTypeVarValue, StaticElem, StaticRealValue,
    StructInstance, SymbolValue, TupleValue, Value,
};
use crate::types::{JuliaType, TypeExpr, TypeParam};
use std::collections::HashMap;

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
        Value::RuntimeTypeVar(tv) => Some(tv.projection()),
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
/// - `"SMatrix{M, N, T}"` with M, N ≤ 4, T any supported Real type
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

    // SMatrix{M, N, T}
    if let Some(rest) = bare_name.strip_prefix("SMatrix{") {
        if let Some(inner) = rest.strip_suffix('}') {
            // inner is "M, N, T"
            let (mn_part, elem_name) = inner.rsplit_once(", ")?;
            let (m_str, n_str) = mn_part.split_once(", ")?;
            let m: usize = m_str.parse().ok()?;
            let n: usize = n_str.parse().ok()?;
            if m > 4 || n > 4 {
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
        "itr" => {
            let elements = pairs
                .data
                .names
                .iter()
                .map(|name| Value::Symbol(SymbolValue::new(name)))
                .collect();
            Some(Value::Tuple(super::super::value::TupleValue::new(elements)))
        }
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

/// Parse the explicit type arguments carried in a parametric name such as
/// `A.Pt{Float64}` or `Foo{Int64, Float64}` into `JuliaType`s, returning `None`
/// when the name has no `{...}` parameters or supplies more parameters than the
/// type declares. Partially applied names (`Foo{Int64}` for `Foo{T,V}`) return
/// the explicit leading prefix so the runtime constructor can infer the
/// remaining parameters from field values (Issue #8393).
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

impl<R: RngLike> Vm<R> {
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

    /// Build a parametric type `Value` from a base name and a flat list of
    /// already-evaluated type arguments (`DataType`/`TypeVar`/value-parameter
    /// scalars). Shared by `ConstructParametricType` and
    /// `ConstructParametricTypeSplat` so the literal `Tuple{T,Float64}` path
    /// and the splatted `Tuple{xs...}` / `Core.apply_type(...)` path build
    /// identical results (Issue #5112).
    fn build_parametric_type(&mut self, base_name: &str, type_args: Vec<Value>) -> Value {
        use crate::types::JuliaType;

        // Issue #4698: remember the identity of any fresh `TypeVar`
        // arguments. Projecting a `RuntimeTypeVar` to a
        // `JuliaType::TypeVar(name, upper)` (below) drops its unique
        // `id`, so stash the original id-bearing value keyed by the
        // same (name, upper) the projection produces. Reflection
        // (`Vector{T}.parameters[1]`) can then hand back the *same*
        // TypeVar object, keeping `parameters[1] === T` true.
        for arg in &type_args {
            if let Value::RuntimeTypeVar(tv) = arg {
                let upper = match &tv.upper_bound {
                    crate::types::JuliaType::Any => None,
                    other => Some(other.name().to_string()),
                };
                self.runtime_typevar_identities
                    .insert((tv.name.clone(), upper), (**tv).clone());
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
                Value::I8(n) => format!("Int8({n})"),
                Value::I16(n) => format!("Int16({n})"),
                Value::I32(n) => format!("Int32({n})"),
                Value::I64(n) => n.to_string(),
                Value::I128(n) => format!("Int128({n})"),
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
                Value::Tuple(tuple) => {
                    render_tuple_type_param(tuple).unwrap_or_else(|| "Any".to_string())
                }
                _ => "Any".to_string(),
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
                    return Value::DataType(Box::new(ty));
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
                    return Value::DataType(Box::new(julia_type));
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
            return Value::DataType(Box::new(JuliaType::TupleOf(tuple_types)));
        }

        let type_name = if type_arg_names.is_empty() {
            base_name.to_string()
        } else {
            format!("{}{{{}}}", base_name, type_arg_names.join(", "))
        };

        Value::DataType(Box::new(JuliaType::from_name_or_struct(&type_name)))
    }

    /// Execute struct instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_struct(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::NewStruct(type_id, field_count) => {
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
                coerce_fields_to_declared_types(struct_def.as_ref(), &mut values);

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
                                    .filter_map(|i| arr_borrow.get_linear(i).ok())
                                    .collect()
                            }
                            None => vec![],
                        }
                    }
                    Value::Memory(mem) => {
                        let mem_borrow = mem.borrow();
                        (0..mem_borrow.len())
                            .filter_map(|i| mem_borrow.data.get_value(i))
                            .collect()
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
                coerce_fields_to_declared_types(struct_def.as_ref(), &mut values);
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
                        if let Some(runtime_inferred) = self
                            .infer_parametric_struct_name_from_runtime_fields(base_name, &values)?
                        {
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
                        _ => "Any".to_string(),
                    })
                    .collect();
                let struct_name = format!("{}{{{}}}", base_name, type_args.join(", "));

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

                let result = self.build_parametric_type(base_name, type_args);
                self.stack.push(result);
                Ok(DispatchAction::Continue)
            }

            // Construct a parametric type whose type arguments may be splatted
            // collections (`Tuple{xs...}`, `Core.apply_type(base, args...)`).
            // Stack layout mirrors `CallWithSplat`: the (possibly splatted)
            // arguments are on top, oldest deepest. `splat_mask[i]` flags
            // whether argument `i` is a `...`-splat to be flattened (Issue #5112).
            Instr::ConstructParametricTypeSplat(ref base_name, ref splat_mask) => {
                let arg_count = splat_mask.len();
                let mut raw_args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    raw_args.push(self.stack.pop_value()?);
                }
                raw_args.reverse();

                let type_args = crate::vm::splat::expand_splat_arguments(raw_args, splat_mask);
                let result = self.build_parametric_type(base_name, type_args);
                self.stack.push(result);
                Ok(DispatchAction::Continue)
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
                if let Value::DataType(jt) = &base_val {
                    if matches!(jt.as_ref(), crate::types::JuliaType::UnionAll { .. }) {
                        let mut instantiated = jt.clone();
                        for arg in &type_args {
                            let Some(arg_type) = type_arg_value_to_julia_type(arg) else {
                                return Err(VmError::TypeError(format!(
                                    "Core.apply_type: type parameter must be a type or value parameter, got {:?}",
                                    arg.value_type()
                                )));
                            };
                            instantiated = Box::new(instantiated.instantiate(&arg_type));
                        }
                        self.stack.push(Value::DataType(instantiated));
                        return Ok(DispatchAction::Continue);
                    }
                }

                let base_name = match &base_val {
                    // Strip any `{...}` parameters: `Box{Int}` -> `Box`,
                    // `Vector{Int}` -> `Vector` (preserving the display alias
                    // rather than collapsing onto `Array`).
                    Value::DataType(jt) => {
                        let name = jt.name();
                        name.split_once('{')
                            .map(|(base, _)| base.to_string())
                            .unwrap_or_else(|| name.to_string())
                    }
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Core.apply_type: base must be a type, got {:?}",
                            other
                        )));
                    }
                };

                let result = self.build_parametric_type(&base_name, type_args);
                self.stack.push(result);
                Ok(DispatchAction::Continue)
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
                    return Err(VmError::TypeError(format!(
                        "type {} has no field {}",
                        val.runtime_type().name(),
                        field_name
                    )));
                }
                if let Value::NamedTuple(named) = &val {
                    let value = named.get_by_name(field_name).map_err(|e| {
                        VmError::TypeError(format!("NamedTuple has no field {}: {}", field_name, e))
                    })?;
                    self.stack.push(value.clone());
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Pairs(pairs) = &val {
                    if let Some(value) = pairs_projected_field(pairs, field_name) {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                    return Err(VmError::TypeError(format!(
                        "type Base.Pairs has no field {field_name}"
                    )));
                }

                if let Value::Generator(generator) = &val {
                    let value = self.generator_projected_field(generator, field_name)?;
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if let Value::Module(module) = &val {
                    let value = self
                        .get_module_binding(&module.name, field_name)
                        .ok_or_else(|| {
                            VmError::TypeError(format!(
                                "Module {} has no binding named {}",
                                module.name, field_name
                            ))
                        })?;
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
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "type GlobalRef has no field {}",
                                field_name
                            )));
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
                    return Err(VmError::TypeError(format!(
                        "type QuoteNode has no field {field_name}"
                    )));
                }

                if let Value::RuntimeTypeVar(tv) = &val {
                    let value = match field_name.as_str() {
                        "name" => {
                            Value::Symbol(crate::vm::value::SymbolValue::new(tv.name.clone()))
                        }
                        "lb" => Value::DataType(Box::new(tv.lower_bound.clone())),
                        "ub" => Value::DataType(Box::new(tv.upper_bound.clone())),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "type TypeVar has no field {}",
                                field_name
                            )));
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
                    // Issue #4722: `.parameters` is a `Core.SimpleVector` (svec).
                    // Issue #5162: include integer/value parameters (array dim
                    // `N`, `Val{5}`, ...) so the dynamic path matches both the
                    // static path and upstream Julia exactly.
                    let value = match field_name.as_str() {
                        "parameters" => {
                            let elements = object
                                .parameters_with_values()
                                .into_iter()
                                .map(|p| self.reflection_parameter_to_value(p))
                                .collect();
                            Some(Value::SimpleVector(crate::vm::value::TupleValue {
                                elements,
                            }))
                        }
                        "var" => object.unionall_var().map(|t| Value::DataType(Box::new(t))),
                        "body" => object.unionall_body().map(|t| Value::DataType(Box::new(t))),
                        "name" => Some(Value::RuntimeTypeName(Box::new(RuntimeTypeNameValue {
                            name: object.typename_symbol(),
                        }))),
                        "lb" => object
                            .typevar_lower_bound()
                            .map(|t| Value::DataType(Box::new(t))),
                        "ub" => object
                            .typevar_upper_bound()
                            .map(|t| Value::DataType(Box::new(t))),
                        _ => None,
                    };
                    match value {
                        Some(v) => {
                            self.stack.push(v);
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            return Err(VmError::TypeError(format!(
                                "type DataType has no field {}",
                                field_name
                            )));
                        }
                    }
                }

                if let Value::RuntimeTypeName(type_name) = &val {
                    if field_name == "name" {
                        self.stack
                            .push(Value::Symbol(SymbolValue::new(&type_name.name)));
                        return Ok(DispatchAction::Continue);
                    }
                    return Err(VmError::TypeError(format!(
                        "type Core.TypeName has no field {}",
                        field_name
                    )));
                }

                // Base.RefValue{T} field access: `r.x` returns the boxed value
                // (Issue #5130), matching upstream `RefValue.x`.
                if let Value::Ref(cell) = &val {
                    if field_name == "x" {
                        let v = cell.borrow().clone();
                        self.stack.push(v);
                        return Ok(DispatchAction::Continue);
                    }
                    return Err(VmError::TypeError(format!(
                        "type Base.RefValue has no field {}",
                        field_name
                    )));
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
                            return Err(VmError::TypeError(format!(
                                "GetFieldByName({field_name}): expected struct, got {}",
                                util::value_type_name(&val)
                            )));
                        }
                    };
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                // Handle RegexMatch field access
                if let Value::RegexMatch(m) = &val {
                    let value = match field_name.as_str() {
                        "match" => Value::Str(m.match_str.clone()),
                        "captures" => {
                            let elements: Vec<Value> = m
                                .captures
                                .iter()
                                .map(|c| match c {
                                    Some(s) => Value::Str(s.clone()),
                                    None => Value::Nothing,
                                })
                                .collect();
                            Value::Tuple(crate::vm::value::TupleValue::new(elements))
                        }
                        "offset" => Value::I64(m.offset),
                        "offsets" => {
                            let elements: Vec<Value> =
                                m.offsets.iter().map(|&o| Value::I64(o)).collect();
                            Value::Tuple(crate::vm::value::TupleValue::new(elements))
                        }
                        _ => {
                            // INTERNAL: GetFieldByName StructRef index is compiler-generated; invalid ref means heap corruption
                            return Err(VmError::InternalError(format!(
                                "type RegexMatch has no field {}",
                                field_name
                            )));
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
                        return Err(VmError::type_error_expected(
                            "GetFieldByName",
                            "struct",
                            &util::value_type_name(other),
                        ));
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
                        // User-visible: user can access a nonexistent field on a struct type
                        return Err(VmError::TypeError(format!(
                            "type {} has no field {}",
                            struct_name, field_name
                        )));
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
                    _ => {
                        // INTERNAL: GetGlobalRefField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetGlobalRefField: field index {} out of bounds (expected {} or {})",
                            field_idx, GLOBAL_REF_FIELD_MODULE_INDEX, GLOBAL_REF_FIELD_NAME_INDEX
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
                                // User-visible: user can attempt to set a nonexistent field on a mutable struct (StructRef path)
                                return Err(VmError::TypeError(format!(
                                    "SetFieldByName: no field '{}' on struct",
                                    field_name
                                )));
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
                                // User-visible: user can attempt to set a nonexistent field on a mutable struct (Struct path)
                                return Err(VmError::TypeError(format!(
                                    "SetFieldByName: no field '{}' on struct",
                                    field_name
                                )));
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
    use crate::rng::StableRng;
    use crate::vm::types::StructDefInfo;
    use crate::vm::value::{new_memory_ref, ArrayElementType, MemoryValue};
    use crate::vm::{Instr, Value, ValueType, Vm};

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
        super::coerce_fields_to_declared_types(Some(&def), &mut values);
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
        super::coerce_fields_to_declared_types(Some(&def), &mut values);
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
        let mut values = vec![Value::U64(7), Value::Str("hi".into())];
        super::coerce_fields_to_declared_types(Some(&def), &mut values);
        assert!(matches!(values.first(), Some(Value::U64(7))));
        assert!(matches!(values.get(1), Some(Value::Str(_))));
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
}
