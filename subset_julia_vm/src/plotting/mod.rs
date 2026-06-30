//! Plot rendering utilities for REPL display artifacts.
//!
//! Detects Plot struct values returned by the VM and generates display artifacts
//! automatically, without requiring the user to call a plot-display function
//! explicitly. All plots — 2D (`:line`, `:scatter`, `:bar`, `:heatmap`) and 3D
//! (`:path3d`, `:scatter3d`, `:surface`) — produce Plotly JSON, so the host
//! renders every plot with the same interactive viewer (Issue #5283).

pub mod jsxgraph;
pub mod plotly;

use crate::vm::{StructInstance, Value};

/// A display artifact produced by a REPL evaluation (e.g., a Plotly plot spec).
#[derive(Debug, Clone)]
pub struct DisplayArtifact {
    /// MIME type, currently always "application/vnd.plotly+json".
    pub mime: String,
    /// Artifact data (UTF-8 text)
    pub data: String,
}

/// Try to generate a display artifact from a VM value.
/// Returns `Some(artifact)` when `value` is a recognized renderable type: a `Plot`
/// (2D or 3D, static Plotly JSON), an `AnimatedGif` (Issue #6355, a Plotly frames
/// animation), or an Interact `Manipulate` (Issue #7275, a static figure with a
/// `updatemenus` dropdown that toggles `visible` between per-choice trace groups).
/// All use the same `application/vnd.plotly+json` MIME; an animation is
/// distinguished by a `frames` key, and a manipulate figure by a `"type":"dropdown"`
/// `updatemenus` entry.
pub fn try_value_to_artifact(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Option<DisplayArtifact> {
    if let Some(json) = plotly::generate_plotly_json(value, struct_heap)
        .or_else(|| plotly::generate_plotly_animation_json(value, struct_heap))
        .or_else(|| plotly::generate_plotly_manipulate_json(value, struct_heap))
    {
        return Some(DisplayArtifact {
            mime: plotly::MIME.to_string(),
            data: json,
        });
    }
    if let Some(json) = jsxgraph::generate_jsxgraph_json(value, struct_heap) {
        return Some(DisplayArtifact {
            mime: jsxgraph::MIME.to_string(),
            data: json,
        });
    }
    None
}

// --- shared coordinate extraction (used by both svg.rs and plotly.rs) ---

fn numeric_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::F64(x) => Some(*x),
        Value::F32(x) => Some(*x as f64),
        Value::I64(x) => Some(*x as f64),
        Value::I32(x) => Some(*x as f64),
        Value::I16(x) => Some(*x as f64),
        Value::I8(x) => Some(*x as f64),
        Value::U64(x) => Some(*x as f64),
        Value::U32(x) => Some(*x as f64),
        _ => None,
    }
}

fn linrange_to_f64_vec(v: &Value, heap: &[StructInstance]) -> Option<Vec<f64>> {
    let instance = match v {
        Value::Struct(s) => s,
        Value::StructRef(idx) => heap.get(*idx)?,
        _ => return None,
    };
    let short_name = instance
        .struct_name
        .rsplit('.')
        .next()
        .unwrap_or(&instance.struct_name);
    if !short_name.starts_with("LinRange") || instance.values.len() < 4 {
        return None;
    }

    let start = numeric_to_f64(&instance.values[0])?;
    let stop = numeric_to_f64(&instance.values[1])?;
    let len = match instance.values[2] {
        Value::I64(n) if n > 0 => usize::try_from(n).ok()?,
        Value::I64(_) => return Some(Vec::new()),
        _ => return None,
    };
    let lendiv = match instance.values[3] {
        Value::I64(n) if n > 0 => n as f64,
        _ => return None,
    };

    if len == 1 {
        return Some(vec![start]);
    }

    Some(
        (0..len)
            .map(|i| {
                let t = (i as f64) / lendiv;
                (1.0 - t) * start + t * stop
            })
            .collect(),
    )
}

/// Extract plot coordinates from any array-shaped `Value` into a flat `Vec<f64>`.
///
/// Handles every representation a Plots series field can hold: the transitional
/// native-array compatibility carrier (e.g. `rand(10)`), the Pure-Julia `Array{T}`
/// wrapper struct backed by a `Memory` buffer (e.g. broadcast results like
/// `cos.(0:0.01:π)`), and `Range`. Returns an empty vec for non-array or
/// non-numeric values. (Issue #5271 follow-up: broadcast/range-backed arrays
/// were previously dropped, leaving 2D and 3D plots blank.)
pub(crate) fn value_to_f64_vec(v: &Value, heap: &[StructInstance]) -> Vec<f64> {
    if let Value::Range(r) = v {
        let n = r.len();
        return (0..n).map(|i| r.start + (i as f64) * r.step).collect();
    }
    if let Some(values) = linrange_to_f64_vec(v, heap) {
        return values;
    }
    let Ok(arr) =
        crate::vm::builtins_linalg::linalg_value_to_array_value(v.clone(), heap, "plot", None)
    else {
        return Vec::new();
    };
    if let Ok(vec) = arr.try_as_f64_vec() {
        return vec;
    }
    // Heterogeneous (`Any`) arrays of numbers: keep the numeric elements.
    arr.to_value_vec()
        .iter()
        .filter_map(numeric_to_f64)
        .collect()
}

/// Extract a 2D array-shaped `Value` as row-major nested rows (`z[iy][ix]`).
///
/// VM arrays are stored column-major (element `(i,j)` at `flat[j*nrows + i]`),
/// so we transpose-on-read to produce the row-major nesting Plotly's `surface`
/// `z` expects, with `size(z) == (length(y), length(x))`.
pub(crate) fn value_to_f64_matrix(v: &Value, heap: &[StructInstance]) -> Option<Vec<Vec<f64>>> {
    let arr =
        crate::vm::builtins_linalg::linalg_value_to_array_value(v.clone(), heap, "surface", None)
            .ok()?;
    if arr.shape.len() != 2 {
        return None;
    }
    let nrows = arr.shape[0];
    let ncols = arr.shape[1];
    let flat = arr.try_as_f64_vec().ok()?;
    if flat.len() != nrows * ncols {
        return None;
    }
    let matrix = (0..nrows)
        .map(|i| (0..ncols).map(|j| flat[j * nrows + i]).collect())
        .collect();
    Some(matrix)
}
