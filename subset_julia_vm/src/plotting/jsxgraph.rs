//! JSXGraph JSON spec generation for display artifacts.
//!
//! Detects `JSXGraph.Board` values returned by the VM and emits a
//! `application/vnd.jsxgraph+json` artifact. The JSON carries board-level options
//! plus an ordered list of elements, each with an integer `id`, a JSXGraph
//! `type`, `parents`, and `attrs`. Element references in `parents` are encoded as
//! `{"ref": id}` so frontends can resolve them after creating objects.

use crate::vm::builtins_linalg::linalg_value_to_array_value;
use crate::vm::{StructInstance, Value};
use serde_json::{json, Map, Number, Value as JsonValue};

pub const MIME: &str = "application/vnd.jsxgraph+json";

pub fn generate_jsxgraph_json(value: &Value, struct_heap: &[StructInstance]) -> Option<String> {
    let board = resolve_struct(value, struct_heap, "Board")?;
    let options = struct_field_array(board, 1, struct_heap)?;
    let elements = struct_field_array(board, 0, struct_heap)?;

    let mut options_json = pairs_to_json(&options, struct_heap);
    let mut elems_json = Vec::new();
    for el_val in elements {
        elems_json.push(element_to_json(&el_val, struct_heap)?);
    }

    // Issue #7592: a board carrying a 3D view must rotate (not pan) on a
    // single-finger drag. JSXGraph's board defaults to single-finger
    // origin-move (`pan.needTwoFingers = false`); on touch (iOS) that calls
    // `initMoveOrigin` on pointerdown and sets `BOARD_MODE_MOVE_ORIGIN`, which
    // blocks the `View3D` rotation handler — it only starts while
    // `board.mode === BOARD_MODE_NONE`. So a single-finger drag pans the scene
    // instead of rotating it. Requiring two fingers to pan frees the
    // single-finger drag to rotate the view (two-finger drag still pans, pinch
    // still zooms; mouse keeps `needShift` via JSXGraph's attribute merge).
    // Only inject when a view3d is actually present and the user has not set
    // `pan` explicitly.
    if elems_json.iter().any(json_contains_view3d) {
        if let JsonValue::Object(ref mut map) = options_json {
            map.entry("pan".to_string())
                .or_insert_with(|| json!({"needTwoFingers": true}));
        }
    }

    let board_json = json!({"options": options_json, "elements": elems_json});
    serde_json::to_string(&board_json).ok()
}

/// True if `el` (or, recursively, any nested element) is a JSXGraph 3D view.
fn json_contains_view3d(el: &JsonValue) -> bool {
    if el.get("type").and_then(JsonValue::as_str) == Some("view3d") {
        return true;
    }
    el.get("elements")
        .and_then(JsonValue::as_array)
        .is_some_and(|children| children.iter().any(json_contains_view3d))
}

fn element_to_json(value: &Value, heap: &[StructInstance]) -> Option<JsonValue> {
    if let Some(view) = resolve_struct(value, heap, "View3D") {
        let id = struct_field_i64(view, 0)?;
        let parents = struct_field_array(view, 1, heap)?;
        let attrs = struct_field_array(view, 2, heap)?;
        let children = struct_field_array(view, 3, heap)?;
        let nested = children
            .iter()
            .map(|child| element_to_json(child, heap))
            .collect::<Option<Vec<_>>>()?;

        return Some(json!({
            "id": id,
            "type": "view3d",
            "parents": parents_to_json(&parents, heap),
            "attrs": pairs_to_json(&attrs, heap),
            "elements": nested,
        }));
    }

    let el = resolve_struct(value, heap, "JSXElement")?;
    let id = struct_field_i64(el, 0)?;
    let type_name = struct_field_symbol(el, 1)?;
    let parents = struct_field_array(el, 2, heap)?;
    let attrs = struct_field_array(el, 3, heap)?;

    Some(json!({
        "id": id,
        "type": type_name,
        "parents": parents_to_json(&parents, heap),
        "attrs": pairs_to_json(&attrs, heap),
    }))
}

fn resolve_struct<'a>(
    value: &'a Value,
    heap: &'a [StructInstance],
    short_name: &str,
) -> Option<&'a StructInstance> {
    let instance = match value {
        Value::Struct(s) => s,
        Value::StructRef(idx) => heap.get(*idx)?,
        _ => return None,
    };
    let name = instance
        .struct_name
        .rsplit('.')
        .next()
        .unwrap_or(&instance.struct_name);
    if name == short_name
        || name.starts_with(&format!("{}{{", short_name))
        || *instance.struct_name == format!("JSXGraph.{}", short_name)
    {
        Some(instance)
    } else {
        None
    }
}

fn struct_field_i64(instance: &StructInstance, idx: usize) -> Option<i64> {
    match instance.values.get(idx)? {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        _ => None,
    }
}

fn struct_field_symbol(instance: &StructInstance, idx: usize) -> Option<String> {
    match instance.values.get(idx)? {
        Value::Symbol(sym) => Some(sym.as_str().to_string()),
        _ => None,
    }
}

fn struct_field_array(
    instance: &StructInstance,
    idx: usize,
    heap: &[StructInstance],
) -> Option<Vec<Value>> {
    let field = instance.values.get(idx)?;
    let arr = linalg_value_to_array_value(field.clone(), heap, "jsxgraph", None).ok()?;
    Some(arr.to_value_vec())
}

fn parents_to_json(parents: &[Value], heap: &[StructInstance]) -> JsonValue {
    JsonValue::Array(
        parents
            .iter()
            .map(|p| value_to_jsx_parent(p, heap))
            .collect(),
    )
}

fn value_to_jsx_parent(value: &Value, heap: &[StructInstance]) -> JsonValue {
    if let Some(jsfunc) = js_function_to_json(value, heap) {
        return jsfunc;
    }
    if let Some(id) = element_id(value, heap) {
        return json!({"ref": id});
    }
    match value {
        Value::F64(x) => JsonValue::Number(Number::from_f64(*x).unwrap_or_else(|| Number::from(0))),
        Value::F32(x) => {
            JsonValue::Number(Number::from_f64(*x as f64).unwrap_or_else(|| Number::from(0)))
        }
        Value::I64(x) => JsonValue::Number(Number::from(*x)),
        Value::I32(x) => JsonValue::Number(Number::from(*x)),
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Str(s) => JsonValue::String(s.clone()),
        Value::Symbol(sym) => JsonValue::String(sym.as_str().to_string()),
        Value::Tuple(t) => JsonValue::Array(
            t.elements
                .iter()
                .map(|v| value_to_jsx_parent(v, heap))
                .collect(),
        ),
        Value::Range(r) => JsonValue::Array(vec![
            JsonValue::Number(Number::from_f64(r.start).unwrap_or_else(|| Number::from(0))),
            JsonValue::Number(
                Number::from_f64(r.last().unwrap_or(r.start)).unwrap_or_else(|| Number::from(0)),
            ),
        ]),
        Value::Struct(_) | Value::StructRef(_) => {
            // Nested Array wrapper (e.g. curve xs/ys sampled as Vector{Float64}).
            if let Ok(arr) = linalg_value_to_array_value(value.clone(), heap, "jsxgraph", None) {
                JsonValue::Array(
                    arr.to_value_vec()
                        .iter()
                        .map(|v| value_to_jsx_parent(v, heap))
                        .collect(),
                )
            } else {
                JsonValue::Null
            }
        }
        _ => JsonValue::Null,
    }
}

fn element_id(value: &Value, heap: &[StructInstance]) -> Option<i64> {
    resolve_struct(value, heap, "JSXElement")
        .or_else(|| resolve_struct(value, heap, "View3D"))
        .and_then(|el| struct_field_i64(el, 0))
}

fn js_function_to_json(value: &Value, heap: &[StructInstance]) -> Option<JsonValue> {
    let jsfunc = resolve_struct(value, heap, "JSFunction")?;
    let code = match jsfunc.values.first()? {
        Value::Str(s) => s.clone(),
        _ => return None,
    };
    let var = struct_field_symbol(jsfunc, 1)?;
    // A non-empty second variable marks a multi-argument function (e.g. the
    // `(u, v)` coordinate maps of a parametricsurface3d); emit it as `vars` so
    // the renderer builds a function of all parameters. Otherwise stay on the
    // single-argument `var` form used by curve3d.
    match struct_field_symbol(jsfunc, 2).filter(|s| !s.is_empty()) {
        Some(var2) => Some(json!({"jsfunc": code, "vars": [var, var2]})),
        None => Some(json!({"jsfunc": code, "var": var})),
    }
}

fn pairs_to_json(pairs: &[Value], heap: &[StructInstance]) -> JsonValue {
    let mut map = Map::new();
    for pair in pairs {
        if let Some((k, v)) = pair_to_kv(pair, heap) {
            map.insert(k, value_to_jsx_parent(&v, heap));
        }
    }
    JsonValue::Object(map)
}

fn pair_to_kv(pair: &Value, heap: &[StructInstance]) -> Option<(String, Value)> {
    let pair_struct = resolve_struct(pair, heap, "Pair")?;
    let first = pair_struct.values.first()?;
    let second = pair_struct.values.get(1)?.clone();
    let key = match first {
        Value::Symbol(sym) => sym.as_str().to_string(),
        Value::Str(s) => s.clone(),
        _ => return None,
    };
    Some((key, second))
}
