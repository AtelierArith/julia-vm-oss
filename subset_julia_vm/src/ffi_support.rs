//! Helpers shared with host bindings. Not a stable public API.

use serde_json::{json, Value as JsonValue};
use subset_julia_vm_bytecode::value::{
    is_native_array_value as bytecode_is_native_array_value, RustBigFloat, StructInstance, Value,
};

use crate::plotting::DisplayArtifact;

pub use crate::vm::apply_complex_float_aliases;

pub fn is_native_array_value(value: &Value) -> bool {
    bytecode_is_native_array_value(value)
}

pub fn vm_format_value(value: &Value) -> String {
    // The FFI display path has no `struct_heap` access by design (Issue #8642):
    // a bare `StructRef` reaching here renders as the benign placeholder rather
    // than a resolved struct, so wrap without resolving.
    crate::vm::util::format_value(&crate::vm::util::Resolved::assume_ffi_placeholder(value))
}

pub fn format_bigfloat_julia(value: &RustBigFloat) -> String {
    crate::vm::format_bigfloat_julia(value)
}

pub fn legacy_numeric_result_value(value: &Value) -> f64 {
    match value {
        Value::I64(x) => *x as f64,
        Value::F64(x) => *x,
        Value::Nothing => 0.0,
        val if val.is_complex() => val.as_complex_parts().map(|(re, _)| re).unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

/// Cap on the number of scalar leaves the host result-echo JSON may materialize.
/// The Editor/REPL FFI (`compile_and_run_detailed` → `success_with_value`) turns the
/// program's result value into this typed JSON so the host can display it as text.
/// `gif(@animate ...)` returns an `AnimatedGif` whose `frames::Vector{Plot}` hold the
/// *cumulative* path in every frame — O(frames²) points (~1M for the 9000-step /
/// `every 40` Aizawa sample). Fully serializing that (every point as a JSON node WITH
/// its own `display` string, plus a whole-value `display` at each level) transiently
/// allocated ~4 GB and OOM-killed the iOS Editor (Issue #9237, follow-up to #9218).
/// Values whose capped leaf estimate reaches this bound are echoed as a compact
/// opaque summary instead;
/// the plot itself still renders via the display artifact, which is bounded by the
/// compact growing-path schema (Issue #9206). The estimate is O(bound), never O(data).
const MAX_TYPED_VALUE_JSON_LEAVES: usize = 100_000;

pub fn typed_value_json(value: &Value, struct_heap: &[StructInstance]) -> JsonValue {
    let leaf_estimate =
        crate::repl::value_literal_leaf_estimate(value, struct_heap, MAX_TYPED_VALUE_JSON_LEAVES);
    if leaf_estimate >= MAX_TYPED_VALUE_JSON_LEAVES {
        // Summarize instead of serializing every leaf. `display_type` is O(1) (just the
        // runtime type name); crucially we do NOT call `display_value`/`vm_format_value`
        // here, since `show`-ing the whole value would itself be O(data) (Issue #9218).
        return json!({
            "type": "opaque",
            "julia_type": display_type(value),
            "display": display_type(value),
            "reason": "too-large",
        });
    }
    typed_value_json_inner(value, struct_heap, 0)
}

pub fn typed_value_json_string(value: &Value, struct_heap: &[StructInstance]) -> String {
    typed_value_json(value, struct_heap).to_string()
}

pub fn typed_artifact_json(artifact: &DisplayArtifact) -> JsonValue {
    json!({
        "type": "artifact",
        "mime": artifact.mime.as_str(),
        "data": artifact.data.as_str(),
    })
}

pub fn typed_execution_json(
    value: &Value,
    struct_heap: &[StructInstance],
    artifact: Option<&DisplayArtifact>,
) -> JsonValue {
    json!({
        "value": typed_value_json(value, struct_heap),
        "artifact": artifact.map(typed_artifact_json),
    })
}

fn typed_value_json_inner(
    value: &Value,
    struct_heap: &[StructInstance],
    depth: usize,
) -> JsonValue {
    if depth > 32 {
        return opaque_json(value, struct_heap, "max-depth");
    }

    if value.is_complex() {
        if let Some((real, imag)) = value.as_complex_parts() {
            return json!({
                "type": "complex",
                "julia_type": display_type(value),
                "real": finite_f64_json(real),
                "imag": finite_f64_json(imag),
                "display": display_value(value, struct_heap),
            });
        }
    }

    if let Some(dict) = dict_json(value, struct_heap, depth) {
        return dict;
    }

    if let Some(array) = array_json(value, struct_heap, depth) {
        return array;
    }

    match value {
        Value::I8(v) => int_json("Int8", i64::from(*v), value, struct_heap),
        Value::I16(v) => int_json("Int16", i64::from(*v), value, struct_heap),
        Value::I32(v) => int_json("Int32", i64::from(*v), value, struct_heap),
        Value::I64(v) => int_json("Int64", *v, value, struct_heap),
        Value::I128(v) => json!({
            "type": "int",
            "julia_type": "Int128",
            "value": v.to_string(),
            "display": display_value(value, struct_heap),
        }),
        Value::BigInt(v) => json!({
            "type": "int",
            "julia_type": "BigInt",
            "value": v.to_string(),
            "display": display_value(value, struct_heap),
        }),
        Value::U8(v) => uint_json("UInt8", u64::from(*v), value, struct_heap),
        Value::U16(v) => uint_json("UInt16", u64::from(*v), value, struct_heap),
        Value::U32(v) => uint_json("UInt32", u64::from(*v), value, struct_heap),
        Value::U64(v) => uint_json("UInt64", *v, value, struct_heap),
        Value::U128(v) => json!({
            "type": "uint",
            "julia_type": "UInt128",
            "value": v.to_string(),
            "display": display_value(value, struct_heap),
        }),
        Value::Bool(v) => json!({
            "type": "bool",
            "julia_type": "Bool",
            "value": *v,
            "display": display_value(value, struct_heap),
        }),
        Value::F16(v) => float_json("Float16", v.to_f64(), value, struct_heap),
        Value::F32(v) => float_json("Float32", f64::from(*v), value, struct_heap),
        Value::F64(v) => float_json("Float64", *v, value, struct_heap),
        Value::BigFloat(v) => json!({
            "type": "float",
            "julia_type": "BigFloat",
            "value": format_bigfloat_julia(v),
            "display": display_value(value, struct_heap),
        }),
        Value::Str(v) => json!({
            "type": "string",
            "julia_type": "String",
            "value": v,
            "display": display_value(value, struct_heap),
        }),
        Value::Char(v) => json!({
            "type": "char",
            "julia_type": "Char",
            "value": v.to_string(),
            "display": display_value(value, struct_heap),
        }),
        Value::Nothing => json!({
            "type": "nothing",
            "julia_type": "Nothing",
            "value": null,
            "display": "nothing",
        }),
        Value::Missing => json!({
            "type": "missing",
            "julia_type": "Missing",
            "value": null,
            "display": "missing",
        }),
        Value::Tuple(tuple) => json!({
            "type": "tuple",
            "julia_type": "Tuple",
            "length": tuple.elements.len(),
            "elements": tuple.elements
                .iter()
                .map(|v| typed_value_json_inner(v, struct_heap, depth + 1))
                .collect::<Vec<_>>(),
            "display": display_value(value, struct_heap),
        }),
        Value::NamedTuple(nt) => json!({
            "type": "named_tuple",
            "julia_type": "NamedTuple",
            "length": nt.values.len(),
            "fields": nt.names
                .iter()
                .zip(nt.values.iter())
                .map(|(name, value)| {
                    json!({
                        "name": name,
                        "value": typed_value_json_inner(value, struct_heap, depth + 1),
                    })
                })
                .collect::<Vec<_>>(),
            "display": display_value(value, struct_heap),
        }),
        Value::StructRef(index) => struct_heap
            .get(*index)
            .map(|instance| {
                typed_value_json_inner(&Value::Struct(instance.clone()), struct_heap, depth + 1)
            })
            .unwrap_or_else(|| opaque_json(value, struct_heap, "invalid-struct-ref")),
        Value::Struct(instance) => struct_json(instance, struct_heap, depth, value),
        Value::Symbol(symbol) => json!({
            "type": "symbol",
            "julia_type": "Symbol",
            "value": symbol.as_str(),
            "display": display_value(value, struct_heap),
        }),
        Value::Range(range) => json!({
            "type": "range",
            "julia_type": "AbstractRange",
            "start": finite_f64_json(range.start),
            "step": finite_f64_json(range.step),
            "stop": finite_f64_json(range.stop),
            "length": range.len(),
            "display": display_value(value, struct_heap),
        }),
        Value::Enum {
            type_name,
            value: v,
        } => json!({
            "type": "enum",
            "julia_type": type_name,
            "value": *v,
            "display": display_value(value, struct_heap),
        }),
        _ => opaque_json(value, struct_heap, "unsupported"),
    }
}

fn array_json(value: &Value, struct_heap: &[StructInstance], depth: usize) -> Option<JsonValue> {
    let array = crate::vm::builtins_linalg::linalg_value_to_array_value(
        value.clone(),
        struct_heap,
        "ffi",
        None,
    )
    .ok()?;
    let elements = array
        .to_logical_value_vec()
        .unwrap_or_else(|_| array.to_value_vec())
        .iter()
        .map(|value| typed_value_json_inner(value, struct_heap, depth + 1))
        .collect::<Vec<_>>();
    Some(json!({
        "type": "array",
        "julia_type": format!(
            "Array{{{},{}}}",
            array.element_type().julia_type_name(),
            array.shape.len()
        ),
        "element_type": array.element_type().julia_type_name(),
        "shape": array.shape,
        "length": elements.len(),
        "elements": elements,
        "display": display_value(value, struct_heap),
    }))
}

fn dict_json(value: &Value, struct_heap: &[StructInstance], depth: usize) -> Option<JsonValue> {
    let instance = struct_instance(value, struct_heap)?;
    if !is_dict_struct_name(&instance.struct_name) || instance.values.len() < 5 {
        return None;
    }

    let slots = memory_elements(instance.values.first()?)?;
    let keys = instance.values.get(1)?;
    let vals = instance.values.get(2)?;
    let count = match instance.values.get(4) {
        Some(Value::I64(n)) => usize::try_from(*n).unwrap_or(0),
        _ => 0,
    };

    let mut entries = Vec::with_capacity(count);
    for (index, slot) in slots.iter().enumerate() {
        let filled = match slot {
            Value::U8(v) => (*v & 0x80) != 0,
            Value::I64(v) => (*v & 0x80) != 0,
            _ => false,
        };
        if !filled {
            continue;
        }
        let Some(key) = memory_get(keys, index + 1) else {
            continue;
        };
        let Some(val) = memory_get(vals, index + 1) else {
            continue;
        };
        entries.push(json!({
            "key": typed_value_json_inner(&key, struct_heap, depth + 1),
            "value": typed_value_json_inner(&val, struct_heap, depth + 1),
        }));
    }

    Some(json!({
        "type": "dict",
        "julia_type": instance.struct_name.as_ref(),
        "length": entries.len(),
        "entries": entries,
        "display": display_value(value, struct_heap),
    }))
}

fn struct_json(
    instance: &StructInstance,
    struct_heap: &[StructInstance],
    depth: usize,
    original: &Value,
) -> JsonValue {
    json!({
        "type": "struct",
        "julia_type": instance.struct_name.as_ref(),
        "fields": instance.values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                json!({
                    "index": index,
                    "value": typed_value_json_inner(value, struct_heap, depth + 1),
                })
            })
            .collect::<Vec<_>>(),
        "display": display_value(original, struct_heap),
    })
}

fn struct_instance<'a>(
    value: &'a Value,
    struct_heap: &'a [StructInstance],
) -> Option<&'a StructInstance> {
    match value {
        Value::Struct(instance) => Some(instance),
        Value::StructRef(index) => struct_heap.get(*index),
        _ => None,
    }
}

fn is_dict_struct_name(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .is_some_and(|short| short == "Dict" || short.starts_with("Dict{"))
}

fn memory_elements(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Memory(memory) => {
            let memory = memory.borrow();
            Some(
                (1..=memory.len())
                    .filter_map(|index| memory.get(index).ok())
                    .collect(),
            )
        }
        Value::MemoryRef(memory_ref) => Some(
            (1..=memory_ref.len())
                .filter_map(|index| memory_ref.get(index).ok())
                .collect(),
        ),
        _ => None,
    }
}

fn memory_get(value: &Value, index: usize) -> Option<Value> {
    match value {
        Value::Memory(memory) => memory.borrow().get(index).ok(),
        Value::MemoryRef(memory_ref) => memory_ref.get(index).ok(),
        _ => None,
    }
}

fn int_json(
    julia_type: &str,
    value: i64,
    original: &Value,
    struct_heap: &[StructInstance],
) -> JsonValue {
    json!({
        "type": "int",
        "julia_type": julia_type,
        "value": value,
        "display": display_value(original, struct_heap),
    })
}

fn uint_json(
    julia_type: &str,
    value: u64,
    original: &Value,
    struct_heap: &[StructInstance],
) -> JsonValue {
    json!({
        "type": "uint",
        "julia_type": julia_type,
        "value": value,
        "display": display_value(original, struct_heap),
    })
}

fn float_json(
    julia_type: &str,
    value: f64,
    original: &Value,
    struct_heap: &[StructInstance],
) -> JsonValue {
    json!({
        "type": "float",
        "julia_type": julia_type,
        "value": finite_f64_json(value),
        "display": display_value(original, struct_heap),
    })
}

fn finite_f64_json(value: f64) -> JsonValue {
    if value.is_finite() {
        json!(value)
    } else if value.is_nan() {
        json!("NaN")
    } else if value.is_sign_positive() {
        json!("Inf")
    } else {
        json!("-Inf")
    }
}

fn opaque_json(value: &Value, struct_heap: &[StructInstance], reason: &str) -> JsonValue {
    json!({
        "type": "opaque",
        "julia_type": display_type(value),
        "display": display_value(value, struct_heap),
        "reason": reason,
    })
}

fn display_type(value: &Value) -> String {
    value.runtime_type().to_string()
}

fn display_value(value: &Value, struct_heap: &[StructInstance]) -> String {
    match value {
        Value::StructRef(index) => struct_heap
            .get(*index)
            .map(|instance| vm_format_value(&Value::Struct(instance.clone())))
            .unwrap_or_else(|| "<invalid struct ref>".to_string()),
        _ => vm_format_value(value),
    }
}
