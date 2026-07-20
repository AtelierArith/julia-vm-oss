//! Plotly JSON generation from Plot struct values (2D and 3D).
//!
//! All Plots series — 2D (`:line`, `:scatter`, `:bar`, `:heatmap`, `:contour`) and 3D
//! (`:path3d`, `:scatter3d`, `:surface`) — render through Plotly.js. 2D
//! line/scatter series become `scatter` traces (`mode:"lines"` /
//! `mode:"markers"`), histograms use `bar`, matrix-valued 2D plots use
//! `heatmap` / `contour`, and 3D series become `scatter3d` / `surface` traces. The host
//! (iOS / Web) draws every plot with the same interactive Plotly viewer
//! (Issue #5283).

use super::{value_to_f64_matrix, value_to_f64_vec};
use subset_julia_vm_bytecode::value::{StructInstance, Value};

pub const MIME: &str = "application/vnd.plotly+json";

/// Catppuccin accent palette, cycled per series.
const COLORS: [&str; 5] = ["#89b4fa", "#a6e3a1", "#fab387", "#f38ba8", "#cba6f7"];

/// Generate a Plotly JSON string from a Plot value, or None if not a Plot.
pub fn generate_plotly_json(value: &Value, struct_heap: &[StructInstance]) -> Option<String> {
    let plot = resolve_struct(value, struct_heap)?;
    if !is_plots_type(&plot.struct_name, "Plot") {
        return None;
    }

    let series_val = plot.values.first()?;
    let series_list = extract_series(series_val, struct_heap);
    let aspect_ratio = plot
        .values
        .get(2)
        .map(|v| extract_aspect_ratio(v, struct_heap))
        .unwrap_or(AspectRatio::None);
    let title = extract_title(plot);
    let xlims = extract_xlims(plot);
    let ylims = extract_ylims(plot);
    let hlines = extract_ref_lines(plot, 6, struct_heap);
    let vlines = extract_ref_lines(plot, 7, struct_heap);

    // Return None only when there is nothing to render (no series and no shapes).
    if series_list.is_empty() && hlines.is_empty() && vlines.is_empty() {
        return None;
    }

    Some(render_plotly_json(
        &series_list,
        aspect_ratio,
        &title,
        xlims,
        ylims,
        &hlines,
        &vlines,
    ))
}

// --- helpers ---

fn resolve_struct<'a>(v: &'a Value, heap: &'a [StructInstance]) -> Option<&'a StructInstance> {
    match v {
        Value::Struct(s) => Some(s),
        Value::StructRef(idx) => heap.get(*idx),
        _ => None,
    }
}

// `using Plots` exposes `Plots.Plot` / `Plots.Series` as the struct_name, while
// code inside the `Plots` module sees the bare names. Accept either form so the
// REPL auto-display path works whether the user wrote `using Plots` or referenced
// the types fully qualified.
fn is_plots_type(name: &str, expected: &str) -> bool {
    name == expected || name == format!("Plots.{}", expected)
}

// `using Interact` exposes `Interact.Manipulate`, while code inside the `Interact`
// module sees the bare `Manipulate`. Accept either form (mirrors `is_plots_type`).
fn is_interact_type(name: &str, expected: &str) -> bool {
    name == expected || name == format!("Interact.{}", expected)
}

#[derive(Clone, Copy, PartialEq)]
enum SeriesKind {
    Line,
    Scatter,
    Bar,
    Heatmap,
    Contour,
    Path3d,
    Scatter3d,
    Surface,
}

impl SeriesKind {
    fn is_3d(self) -> bool {
        matches!(
            self,
            SeriesKind::Path3d | SeriesKind::Scatter3d | SeriesKind::Surface
        )
    }
}

struct SeriesData {
    x: Vec<f64>,
    y: Vec<f64>,
    /// Flat z for `:path3d` / `:scatter3d`. Empty for 2D and matrix z series.
    z: Vec<f64>,
    /// Only for matrix-valued `:surface`, `:heatmap`, and `:contour`.
    z_matrix: Option<Vec<Vec<f64>>>,
    kind: SeriesKind,
    /// Per-series legend label, extracted from `Series.label` (field 3).
    label: Option<String>,
    /// Per-series contour levels, extracted from `Series.levels` (field 5).
    contour_levels: ContourLevels,
}

#[derive(Clone, Debug, PartialEq)]
enum ContourLevels {
    Auto,
    Count(i64),
    Values(Vec<f64>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AspectRatio {
    None,
    Equal,
    Ratio(f64),
}

impl AspectRatio {
    fn is_fixed(self) -> bool {
        !matches!(self, AspectRatio::None)
    }

    fn scaleratio(self) -> Option<f64> {
        match self {
            AspectRatio::None => None,
            AspectRatio::Equal => Some(1.0),
            AspectRatio::Ratio(r) if r.is_finite() && r > 0.0 => Some(r),
            AspectRatio::Ratio(_) => None,
        }
    }
}

fn extract_aspect_ratio(value: &Value, heap: &[StructInstance]) -> AspectRatio {
    match value {
        Value::Symbol(sym) if sym.as_str() == "equal" => AspectRatio::Equal,
        Value::Symbol(sym) if sym.as_str() == "none" || sym.as_str() == "auto" => AspectRatio::None,
        Value::Bool(true) => AspectRatio::Ratio(1.0),
        Value::Bool(false) => AspectRatio::Ratio(0.0),
        Value::F64(v) => AspectRatio::Ratio(*v),
        Value::F32(v) => AspectRatio::Ratio(*v as f64),
        Value::I64(v) => AspectRatio::Ratio(*v as f64),
        Value::I32(v) => AspectRatio::Ratio(*v as f64),
        Value::I16(v) => AspectRatio::Ratio(*v as f64),
        Value::I8(v) => AspectRatio::Ratio(*v as f64),
        Value::U64(v) => AspectRatio::Ratio(*v as f64),
        Value::U32(v) => AspectRatio::Ratio(*v as f64),
        Value::Struct(s) => rational_aspect_ratio(s),
        Value::StructRef(idx) => heap
            .get(*idx)
            .map(rational_aspect_ratio)
            .unwrap_or(AspectRatio::None),
        _ => AspectRatio::None,
    }
}

/// Read the `title` field (4th field, `values[3]`) of a `Plot` struct (Issue #7030).
/// Empty (or non-string) → no title.
fn extract_title(plot: &StructInstance) -> String {
    match plot.values.get(3) {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Symbol(s)) => s.as_str().to_string(),
        _ => String::new(),
    }
}

/// Extract `(lo, hi)` from a `nothing`/`(lo, hi)` Julia value (Issue #7850).
/// `nothing` → `None`; a 2-element tuple of numerics → `Some((lo, hi))`.
fn extract_lims_from_value(value: &Value) -> Option<(f64, f64)> {
    match value {
        Value::Nothing => None,
        Value::Tuple(t) if t.elements.len() >= 2 => {
            let lo = scalar_f64(&t.elements[0])?;
            let hi = scalar_f64(&t.elements[1])?;
            Some((lo, hi))
        }
        _ => None,
    }
}

/// Read `xlims` from `Plot.values[4]` (Issue #7850).
fn extract_xlims(plot: &StructInstance) -> Option<(f64, f64)> {
    plot.values.get(4).and_then(extract_lims_from_value)
}

/// Read `ylims` from `Plot.values[5]` (Issue #7850).
fn extract_ylims(plot: &StructInstance) -> Option<(f64, f64)> {
    plot.values.get(5).and_then(extract_lims_from_value)
}

/// Read reference-line values from `Plot.values[field_idx]` (Issue #7850).
/// Returns an empty vec when the field is absent, `nothing`, or not a numeric array.
fn extract_ref_lines(plot: &StructInstance, field_idx: usize, heap: &[StructInstance]) -> Vec<f64> {
    match plot.values.get(field_idx) {
        None | Some(Value::Nothing) => vec![],
        Some(v) => super::value_to_f64_vec(v, heap),
    }
}

/// Escape a string for embedding in a JSON string literal.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// `"title":{...}` layout key-value (no surrounding braces, no leading comma), or
/// `None` when the title is empty.
fn title_json_kv(title: &str) -> Option<String> {
    if title.is_empty() {
        return None;
    }
    Some(format!(
        r##""title":{{"text":"{}","font":{{"color":"#cdd6f4"}}}}"##,
        json_escape(title)
    ))
}

/// Extract a string label from a `Series.label` value. Symbols and strings are
/// accepted; `nothing` / other values leave the series unlabeled.
fn extract_series_label(value: &Value) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.to_string()),
        Value::Symbol(s) => Some(s.as_str().to_string()),
        _ => None,
    }
}

/// `"name":"..."` trace key-value fragment (including the leading comma), or an
/// empty string when the series has no label.
fn trace_name_fragment(label: &Option<String>) -> String {
    match label {
        Some(s) => format!(r#","name":"{}""#, json_escape(s)),
        None => String::new(),
    }
}

fn integer_i64(value: &Value) -> Option<i64> {
    match value {
        Value::I64(v) => Some(*v),
        Value::I32(v) => Some(i64::from(*v)),
        Value::I16(v) => Some(i64::from(*v)),
        Value::I8(v) => Some(i64::from(*v)),
        Value::U32(v) => Some(i64::from(*v)),
        Value::U64(v) => i64::try_from(*v).ok(),
        _ => None,
    }
}

fn extract_contour_levels(value: Option<&Value>, heap: &[StructInstance]) -> ContourLevels {
    let Some(value) = value else {
        return ContourLevels::Auto;
    };
    if matches!(value, Value::Nothing) {
        return ContourLevels::Auto;
    }
    if let Some(n) = integer_i64(value) {
        return if n > 0 {
            ContourLevels::Count(n)
        } else {
            ContourLevels::Auto
        };
    }
    let values = value_to_f64_vec(value, heap);
    if values.len() >= 2 {
        ContourLevels::Values(values)
    } else {
        ContourLevels::Auto
    }
}

fn contour_level_fragments(levels: &ContourLevels) -> (String, String) {
    match levels {
        ContourLevels::Auto => (String::new(), String::new()),
        // Match upstream Plots' Plotly backend: integer levels become `ncontours`
        // with two endpoint contours included.
        ContourLevels::Count(n) => (String::new(), format!(r#","ncontours":{}"#, n + 2)),
        ContourLevels::Values(values) if values.len() >= 2 => {
            let start = values[0];
            let end = *values.last().unwrap_or(&start);
            let size = (end - start) / ((values.len() - 1) as f64);
            if size.is_finite() && size != 0.0 {
                (
                    format!(
                        r#","start":{},"end":{},"size":{}"#,
                        f64_to_json(start),
                        f64_to_json(end),
                        f64_to_json(size)
                    ),
                    String::new(),
                )
            } else {
                (String::new(), String::new())
            }
        }
        ContourLevels::Values(_) => (String::new(), String::new()),
    }
}

fn rational_aspect_ratio(s: &StructInstance) -> AspectRatio {
    let Some((num, den)) = s.as_rational_parts_f64() else {
        return AspectRatio::None;
    };
    if den == 0.0 {
        AspectRatio::None
    } else {
        AspectRatio::Ratio(num / den)
    }
}

fn extract_series(series_val: &Value, heap: &[StructInstance]) -> Vec<SeriesData> {
    let Ok(arr) = crate::vm::builtins_linalg::linalg_value_to_array_value(
        series_val.clone(),
        heap,
        "plot",
        None,
    ) else {
        return vec![];
    };
    let arr = arr.to_value_vec();

    arr.iter()
        .filter_map(|elem| {
            let s = resolve_struct(elem, heap)?;
            if !is_plots_type(&s.struct_name, "Series") {
                return None;
            }
            // Series fields: (x, y, z, label, seriestype).
            let label = s.values.get(3).and_then(extract_series_label);
            let seriestype = match s.values.get(4) {
                Some(Value::Symbol(sym)) => sym.as_str(),
                _ => "line",
            };
            let kind = match seriestype {
                "scatter" => SeriesKind::Scatter,
                "bar" => SeriesKind::Bar,
                "heatmap" => SeriesKind::Heatmap,
                "contour" => SeriesKind::Contour,
                "path3d" => SeriesKind::Path3d,
                "scatter3d" => SeriesKind::Scatter3d,
                "surface" => SeriesKind::Surface,
                // "line", "path", and anything else fall back to a line.
                _ => SeriesKind::Line,
            };
            let contour_levels = if kind == SeriesKind::Contour {
                extract_contour_levels(s.values.get(5), heap)
            } else {
                ContourLevels::Auto
            };
            let x = s
                .values
                .first()
                .map(|v| value_to_f64_vec(v, heap))
                .unwrap_or_default();
            let y = s
                .values
                .get(1)
                .map(|v| value_to_f64_vec(v, heap))
                .unwrap_or_default();
            if x.is_empty() || y.is_empty() {
                return None;
            }
            match kind {
                SeriesKind::Surface | SeriesKind::Heatmap | SeriesKind::Contour => {
                    let z_matrix = value_to_f64_matrix(s.values.get(2)?, heap)?;
                    Some(SeriesData {
                        x,
                        y,
                        z: vec![],
                        z_matrix: Some(z_matrix),
                        kind,
                        label,
                        contour_levels,
                    })
                }
                SeriesKind::Path3d | SeriesKind::Scatter3d => {
                    let z = value_to_f64_vec(s.values.get(2)?, heap);
                    if z.is_empty() {
                        return None;
                    }
                    Some(SeriesData {
                        x,
                        y,
                        z,
                        z_matrix: None,
                        kind,
                        label,
                        contour_levels,
                    })
                }
                SeriesKind::Line | SeriesKind::Scatter | SeriesKind::Bar => Some(SeriesData {
                    x,
                    y,
                    z: vec![],
                    z_matrix: None,
                    kind,
                    label,
                    contour_levels,
                }),
            }
        })
        .collect()
}

fn f64_to_json(v: f64) -> String {
    if v.is_nan() || v.is_infinite() {
        "null".to_string()
    } else {
        format!("{}", v)
    }
}

fn vec_to_json_array(v: &[f64]) -> String {
    let inner: Vec<String> = v.iter().map(|x| f64_to_json(*x)).collect();
    format!("[{}]", inner.join(","))
}

fn matrix_to_json_array(m: &[Vec<f64>]) -> String {
    let rows: Vec<String> = m.iter().map(|row| vec_to_json_array(row)).collect();
    format!("[{}]", rows.join(","))
}

fn render_trace(sd: &SeriesData, color: &str) -> String {
    let name_frag = trace_name_fragment(&sd.label);
    match sd.kind {
        SeriesKind::Line => format!(
            r##"{{"type":"scatter","mode":"lines","x":{},"y":{}{},"line":{{"color":"{}","width":2}}}}"##,
            vec_to_json_array(&sd.x),
            vec_to_json_array(&sd.y),
            name_frag,
            color,
        ),
        SeriesKind::Scatter => format!(
            r##"{{"type":"scatter","mode":"markers","x":{},"y":{}{},"marker":{{"color":"{}","size":6}}}}"##,
            vec_to_json_array(&sd.x),
            vec_to_json_array(&sd.y),
            name_frag,
            color,
        ),
        SeriesKind::Bar => format!(
            r##"{{"type":"bar","x":{},"y":{}{},"marker":{{"color":"{}"}}}}"##,
            vec_to_json_array(&sd.x),
            vec_to_json_array(&sd.y),
            name_frag,
            color,
        ),
        SeriesKind::Heatmap => {
            let z_json = sd
                .z_matrix
                .as_deref()
                .map(matrix_to_json_array)
                .unwrap_or_else(|| "[]".to_string());
            format!(
                r#"{{"type":"heatmap","x":{},"y":{},"z":{}{},"colorscale":"Viridis","showscale":true}}"#,
                vec_to_json_array(&sd.x),
                vec_to_json_array(&sd.y),
                z_json,
                name_frag,
            )
        }
        SeriesKind::Contour => {
            let z_json = sd
                .z_matrix
                .as_deref()
                .map(matrix_to_json_array)
                .unwrap_or_else(|| "[]".to_string());
            let (contours_frag, trace_frag) = contour_level_fragments(&sd.contour_levels);
            format!(
                r#"{{"type":"contour","x":{},"y":{},"z":{}{},"contours":{{"coloring":"lines"{}}},"colorscale":"Viridis","showscale":true{}}}"#,
                vec_to_json_array(&sd.x),
                vec_to_json_array(&sd.y),
                z_json,
                name_frag,
                contours_frag,
                trace_frag,
            )
        }
        SeriesKind::Path3d => format!(
            r##"{{"type":"scatter3d","mode":"lines","x":{},"y":{},"z":{}{},"line":{{"color":"{}","width":3}}}}"##,
            vec_to_json_array(&sd.x),
            vec_to_json_array(&sd.y),
            vec_to_json_array(&sd.z),
            name_frag,
            color,
        ),
        SeriesKind::Scatter3d => format!(
            r##"{{"type":"scatter3d","mode":"markers","x":{},"y":{},"z":{}{},"marker":{{"color":"{}","size":4}}}}"##,
            vec_to_json_array(&sd.x),
            vec_to_json_array(&sd.y),
            vec_to_json_array(&sd.z),
            name_frag,
            color,
        ),
        SeriesKind::Surface => {
            let z_json = sd
                .z_matrix
                .as_deref()
                .map(matrix_to_json_array)
                .unwrap_or_else(|| "[]".to_string());
            format!(
                r#"{{"type":"surface","x":{},"y":{},"z":{}{},"colorscale":"Viridis","showscale":true}}"#,
                vec_to_json_array(&sd.x),
                vec_to_json_array(&sd.y),
                z_json,
                name_frag,
            )
        }
    }
}

fn render_scene_layout(aspect_ratio: AspectRatio, title: &str, showlegend: bool) -> String {
    let aspectmode = if aspect_ratio.is_fixed() {
        r#","aspectmode":"data""#
    } else {
        ""
    };
    let title_frag = title_json_kv(title)
        .map(|kv| format!(",{}", kv))
        .unwrap_or_default();
    let showlegend = if showlegend { "true" } else { "false" };
    format!(
        r##"{{"paper_bgcolor":"#1e1e2e","plot_bgcolor":"#1e1e2e","font":{{"color":"#cdd6f4"}}{},"scene":{{"xaxis":{{"gridcolor":"#45475a","color":"#cdd6f4"}},"yaxis":{{"gridcolor":"#45475a","color":"#cdd6f4"}},"zaxis":{{"gridcolor":"#45475a","color":"#cdd6f4"}},"bgcolor":"#1e1e2e"{}}},"margin":{{"l":0,"r":0,"t":40,"b":0}},"showlegend":{}}}"##,
        title_frag, aspectmode, showlegend
    )
}

/// Render Plotly shapes for horizontal (`hlines`) and vertical (`vlines`) reference
/// lines (Issue #7850).  Uses a dashed Catppuccin overlay line consistent with the
/// theme's zerolinecolor.
fn render_shapes(hlines: &[f64], vlines: &[f64]) -> String {
    let mut shapes: Vec<String> = Vec::new();
    for &y in hlines {
        shapes.push(format!(
            r##"{{"type":"line","xref":"paper","x0":0,"x1":1,"yref":"y","y0":{y},"y1":{y},"line":{{"color":"#585b70","width":1,"dash":"dash"}}}}"##,
            y = f64_to_json(y)
        ));
    }
    for &x in vlines {
        shapes.push(format!(
            r##"{{"type":"line","yref":"paper","y0":0,"y1":1,"xref":"x","x0":{x},"x1":{x},"line":{{"color":"#585b70","width":1,"dash":"dash"}}}}"##,
            x = f64_to_json(x)
        ));
    }
    shapes.join(",")
}

fn render_axis_layout_2d(
    aspect_ratio: AspectRatio,
    title: &str,
    showlegend: bool,
    xlims: Option<(f64, f64)>,
    ylims: Option<(f64, f64)>,
    hlines: &[f64],
    vlines: &[f64],
) -> String {
    let yaxis_aspect = aspect_ratio
        .scaleratio()
        .map(|ratio| format!(r#","scaleanchor":"x","scaleratio":{}"#, f64_to_json(ratio)))
        .unwrap_or_default();
    let title_frag = title_json_kv(title)
        .map(|kv| format!(",{}", kv))
        .unwrap_or_default();
    let showlegend = if showlegend { "true" } else { "false" };
    let xrange_frag = xlims
        .map(|(lo, hi)| format!(r#","range":[{},{}]"#, f64_to_json(lo), f64_to_json(hi)))
        .unwrap_or_default();
    let yrange_frag = ylims
        .map(|(lo, hi)| format!(r#","range":[{},{}]"#, f64_to_json(lo), f64_to_json(hi)))
        .unwrap_or_default();
    let shapes_frag = if hlines.is_empty() && vlines.is_empty() {
        String::new()
    } else {
        format!(r#","shapes":[{}]"#, render_shapes(hlines, vlines))
    };
    format!(
        r##"{{"paper_bgcolor":"#1e1e2e","plot_bgcolor":"#1e1e2e","font":{{"color":"#cdd6f4"}}{},"xaxis":{{"gridcolor":"#45475a","color":"#cdd6f4","zerolinecolor":"#585b70"{}}},"yaxis":{{"gridcolor":"#45475a","color":"#cdd6f4","zerolinecolor":"#585b70"{}{}}},"margin":{{"l":48,"r":20,"t":40,"b":40}},"showlegend":{}{}}}"##,
        title_frag, xrange_frag, yrange_frag, yaxis_aspect, showlegend, shapes_frag
    )
}

// --- Animation (Issue #6355) ---

/// Generate a Plotly frames-animation JSON from an `AnimatedGif` value, or `None`.
///
/// Each frame is an in-memory `Plot` snapshot; we render every frame's series with
/// exactly the same trace builders the static path uses (`extract_series` +
/// `render_trace`), so an animated plot looks identical to its static counterpart.
/// On top of the per-frame traces we assemble Plotly's native `frames` array plus
/// Play/Pause `updatemenus` and a frame `slider`, so the iOS/Web host auto-plays
/// and loops the animation. The MIME is unchanged (`application/vnd.plotly+json`);
/// the host branches on the presence of a `frames` key.
pub fn generate_plotly_animation_json(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Option<String> {
    let agif = resolve_struct(value, struct_heap)?;
    if !is_plots_type(&agif.struct_name, "AnimatedGif") {
        return None;
    }

    // AnimatedGif fields: (frames, fps); `frames` is a Vector{Plot}.
    let frames_val = agif.values.first()?;
    let frames_arr = crate::vm::builtins_linalg::linalg_value_to_array_value(
        frames_val.clone(),
        struct_heap,
        "gif",
        None,
    )
    .ok()?;
    let frame_plots = frames_arr.to_value_vec();
    if frame_plots.is_empty() {
        return None;
    }

    let fps = agif
        .values
        .get(1)
        .and_then(scalar_f64)
        .filter(|f| *f > 0.0)
        .unwrap_or(20.0);
    let frame_duration = (1000.0 / fps).round().max(1.0) as i64;

    // Collect each frame's series data *structurally* (not yet rendered to JSON) so
    // we can both detect the common growing-path case and render it compactly.
    // Axis limits and reference lines are read from the first frame only (they are
    // global animation-level settings, not per-frame data — Issue #7850).
    let mut frame_series: Vec<Vec<SeriesData>> = Vec::new();
    let mut frame_titles: Vec<String> = Vec::new();
    let mut has_3d = false;
    let mut aspect_ratio = AspectRatio::None;
    let mut xlims: Option<(f64, f64)> = None;
    let mut ylims: Option<(f64, f64)> = None;
    let mut hlines: Vec<f64> = Vec::new();
    let mut vlines: Vec<f64> = Vec::new();
    let mut any_label = false;
    for (frame_index, plot_val) in frame_plots.iter().enumerate() {
        let Some(plot) = resolve_struct(plot_val, struct_heap) else {
            continue;
        };
        if !is_plots_type(&plot.struct_name, "Plot") {
            continue;
        }
        let Some(series_val) = plot.values.first() else {
            continue;
        };
        let series_list = extract_series(series_val, struct_heap);
        if frame_index == 0 {
            aspect_ratio = plot
                .values
                .get(2)
                .map(|v| extract_aspect_ratio(v, struct_heap))
                .unwrap_or(AspectRatio::None);
            xlims = extract_xlims(plot);
            ylims = extract_ylims(plot);
            hlines = extract_ref_lines(plot, 6, struct_heap);
            vlines = extract_ref_lines(plot, 7, struct_heap);
        }
        if series_list.iter().any(|s| s.kind.is_3d()) {
            has_3d = true;
        }
        if series_list.iter().any(|s| s.label.is_some()) {
            any_label = true;
        }
        frame_series.push(series_list);
        frame_titles.push(extract_title(plot));
    }
    if frame_series.is_empty() {
        return None;
    }

    let any_title = frame_titles.iter().any(|t| !t.is_empty());

    // Reuse the static layout (with frame 0's title), then merge in the controls.
    let base_title = frame_titles.first().map(String::as_str).unwrap_or("");
    let base_layout = if has_3d {
        render_scene_layout(aspect_ratio, base_title, any_label)
    } else {
        render_axis_layout_2d(
            aspect_ratio,
            base_title,
            any_label,
            xlims,
            ylims,
            &hlines,
            &vlines,
        )
    };
    let trimmed = base_layout.trim();
    let base_inner = &trimmed[1..trimmed.len() - 1];

    let updatemenus = render_animation_updatemenus(frame_duration);
    let sliders = render_animation_slider(frame_series.len(), frame_duration);

    // `_frameDuration` is read by the iOS/Web auto-play call; Plotly ignores it.
    let layout = format!(
        r#"{{{},"_frameDuration":{},"updatemenus":[{}],"sliders":[{}]}}"#,
        base_inner, frame_duration, updatemenus, sliders
    );

    // Growing-path fast schema (Issue #9206): a `plot3d(1)` + `push!` + `@animate`
    // animation snapshots the *cumulative* path in every frame, so the naive
    // "full trace data per frame" encoding is O(frames²) in size — ~61 MB for the
    // stock 9000-step Aizawa sample, which dominates iOS/Web render time (the 61 MB
    // string crosses the FFI boundary and is JSON.parse'd + rendered by Plotly.js).
    // When every trace across frames is an exact, non-decreasing prefix of the final
    // frame's arrays, emit the full arrays ONCE plus per-frame point counts; the
    // host (`expandCompactFrames` in REPLEntryView.swift / web/app.js) reconstructs
    // byte-identical Plotly frames by slicing. Non-prefix animations (e.g. a 2D
    // `plot()`-per-frame sine wave) fail the check and fall back to the full schema.
    if let Some(counts) = detect_prefix_nested(&frame_series) {
        let full = frame_series.last()?;
        let full_json = render_frame_traces(full);
        let initial = render_frame_traces(&frame_series[0]);
        let counts_json = format!(
            "[{}]",
            counts
                .iter()
                .map(|row| format!(
                    "[{}]",
                    row.iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let titles_frag = if any_title {
            let ts: Vec<String> = frame_titles
                .iter()
                .map(|t| format!("\"{}\"", json_escape(t)))
                .collect();
            format!(r#","titles":[{}]"#, ts.join(","))
        } else {
            String::new()
        };
        return Some(format!(
            r#"{{"traces":{},"layout":{},"framesCompact":{{"full":{},"counts":{}{}}}}}"#,
            initial, layout, full_json, counts_json, titles_frag
        ));
    }

    // Fallback: native Plotly `frames`, each carrying its own full trace `data`.
    // frames: [{"name":"0","data":[...],"layout":{...}}, ...]. Each frame carries
    // its own `layout.title` so the per-frame title updates as the animation plays
    // (Issue #7030). If ANY frame is titled, every frame emits an explicit title
    // (empty string for untitled frames) so a later untitled frame *clears* a prior
    // title instead of Plotly retaining it (PR #7132 review). If no frame is titled,
    // emit a bare `{}` layout.
    let frame_trace_arrays: Vec<String> = frame_series
        .iter()
        .map(|f| render_frame_traces(f))
        .collect();
    let frames_json: Vec<String> = frame_trace_arrays
        .iter()
        .enumerate()
        .map(|(i, traces)| {
            let frame_layout = if any_title {
                // title_json_kv returns None for an empty title; force an explicit
                // empty title so this frame overrides any previously-shown one.
                let kv = title_json_kv(&frame_titles[i]).unwrap_or_else(|| {
                    r##""title":{"text":"","font":{"color":"#cdd6f4"}}"##.to_string()
                });
                format!("{{{}}}", kv)
            } else {
                "{}".to_string()
            };
            format!(
                r#"{{"name":"{}","data":{},"layout":{}}}"#,
                i, traces, frame_layout
            )
        })
        .collect();
    let frames_json = format!("[{}]", frames_json.join(","));

    Some(format!(
        r#"{{"traces":{},"layout":{},"frames":{}}}"#,
        frame_trace_arrays[0], layout, frames_json
    ))
}

/// Render one frame's series list to a `[trace, ...]` JSON array (the same trace
/// builders the static path uses, so an animated plot looks identical to its
/// static counterpart).
fn render_frame_traces(series: &[SeriesData]) -> String {
    let traces: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(i, sd)| render_trace(sd, COLORS[i % COLORS.len()]))
        .collect();
    format!("[{}]", traces.join(","))
}

/// Detect the growing-path animation shape (Issue #9206): every frame has the same
/// trace count, and for each trace the `x`/`y`/`z` arrays are an exact,
/// non-decreasing prefix of the *final* frame's arrays (same `kind`/`label`).
/// Returns per-frame, per-trace point counts when compactible, else `None` (surface
/// / heatmap / contour / bar, matrix-z, ragged, or non-prefix frames all fall back).
fn detect_prefix_nested(frames: &[Vec<SeriesData>]) -> Option<Vec<Vec<usize>>> {
    // A single frame gains nothing from compaction; the full schema is fine.
    if frames.len() < 2 {
        return None;
    }
    let last = frames.last()?;
    let n_traces = last.len();
    if n_traces == 0 {
        return None;
    }
    for s in last {
        if s.z_matrix.is_some()
            || matches!(
                s.kind,
                SeriesKind::Surface | SeriesKind::Heatmap | SeriesKind::Contour | SeriesKind::Bar
            )
        {
            return None;
        }
    }
    let mut counts: Vec<Vec<usize>> = Vec::with_capacity(frames.len());
    for (fi, frame) in frames.iter().enumerate() {
        if frame.len() != n_traces {
            return None;
        }
        let mut row = Vec::with_capacity(n_traces);
        for t in 0..n_traces {
            let s = &frame[t];
            let full = &last[t];
            if s.kind != full.kind || s.label != full.label {
                return None;
            }
            let n = s.x.len();
            // x and y must have equal length; n must fit within the final arrays;
            // lengths must not shrink from the previous frame (monotonic growth).
            if s.y.len() != n || n > full.x.len() || (fi > 0 && n < counts[fi - 1][t]) {
                return None;
            }
            if s.x[..] != full.x[..n] || s.y[..] != full.y[..n] {
                return None;
            }
            // z: both flat-empty (2D), or both length n with a matching prefix (3D).
            if full.z.is_empty() {
                if !s.z.is_empty() {
                    return None;
                }
            } else if s.z.len() != n || s.z[..] != full.z[..n] {
                return None;
            }
            row.push(n);
        }
        counts.push(row);
    }
    Some(counts)
}

fn scalar_f64(v: &Value) -> Option<f64> {
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

// --- Manipulate dropdown (Issue #7275) ---

/// Generate a Plotly dropdown JSON from an Interact `Manipulate` value, or `None`.
///
/// `@manipulate for var = choices … end` (Interact MVP, Issue #7275) evaluates the
/// body once per discrete choice and stores the per-choice `Plot` snapshots plus
/// their labels in a `Manipulate` struct. This is the dropdown counterpart of
/// `generate_plotly_animation_json`: instead of native Plotly `frames`, every
/// choice's traces are emitted up-front into one trace list — visible for the first
/// choice, hidden for the rest — and an `updatemenus` dropdown toggles the `visible`
/// arrays between choice groups. The MIME is unchanged
/// (`application/vnd.plotly+json`); the host renders it as a normal static figure
/// with a working dropdown (no reactive runtime needed).
pub fn generate_plotly_manipulate_json(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Option<String> {
    let manip = resolve_struct(value, struct_heap)?;
    if !is_interact_type(&manip.struct_name, "Manipulate") {
        return None;
    }

    // Manipulate fields: (plots, labels). `plots` is a Vector{Plot}; `labels` is a
    // Vector{String}, one per choice.
    let plots_val = manip.values.first()?;
    let plots_arr = crate::vm::builtins_linalg::linalg_value_to_array_value(
        plots_val.clone(),
        struct_heap,
        "manipulate",
        None,
    )
    .ok()?;
    let choice_plots = plots_arr.to_value_vec();
    if choice_plots.is_empty() {
        return None;
    }

    let labels = manip
        .values
        .get(1)
        .map(|v| extract_label_strings(v, struct_heap))
        .unwrap_or_default();

    // Render each choice's series. `choice_trace_counts[i]` is how many traces choice
    // `i` contributed, so the dropdown buttons can build `visible` arrays that toggle
    // whole choice groups. Choice 0's traces are visible; the rest start hidden.
    let mut all_traces: Vec<String> = Vec::new();
    let mut choice_trace_counts: Vec<usize> = Vec::new();
    let mut has_3d = false;
    let mut aspect_ratio = AspectRatio::None;
    let mut base_title = String::new();
    let mut any_label = false;
    let mut color_index = 0usize;
    let mut manip_xlims: Option<(f64, f64)> = None;
    let mut manip_ylims: Option<(f64, f64)> = None;
    let mut manip_hlines: Vec<f64> = Vec::new();
    let mut manip_vlines: Vec<f64> = Vec::new();
    for (choice_index, plot_val) in choice_plots.iter().enumerate() {
        let Some(plot) = resolve_struct(plot_val, struct_heap) else {
            choice_trace_counts.push(0);
            continue;
        };
        if !is_plots_type(&plot.struct_name, "Plot") {
            choice_trace_counts.push(0);
            continue;
        }
        let Some(series_val) = plot.values.first() else {
            choice_trace_counts.push(0);
            continue;
        };
        let series_list = extract_series(series_val, struct_heap);
        if choice_index == 0 {
            aspect_ratio = plot
                .values
                .get(2)
                .map(|v| extract_aspect_ratio(v, struct_heap))
                .unwrap_or(AspectRatio::None);
            base_title = extract_title(plot);
            manip_xlims = extract_xlims(plot);
            manip_ylims = extract_ylims(plot);
            manip_hlines = extract_ref_lines(plot, 6, struct_heap);
            manip_vlines = extract_ref_lines(plot, 7, struct_heap);
        }
        if series_list.iter().any(|s| s.kind.is_3d()) {
            has_3d = true;
        }
        if series_list.iter().any(|s| s.label.is_some()) {
            any_label = true;
        }
        let visible = choice_index == 0;
        for sd in &series_list {
            let trace = render_trace(sd, COLORS[color_index % COLORS.len()]);
            all_traces.push(with_visibility(&trace, visible));
            color_index += 1;
        }
        choice_trace_counts.push(series_list.len());
    }
    let total_traces: usize = choice_trace_counts.iter().sum();
    if total_traces == 0 {
        return None;
    }

    let base_layout = if has_3d {
        render_scene_layout(aspect_ratio, &base_title, any_label)
    } else {
        render_axis_layout_2d(
            aspect_ratio,
            &base_title,
            any_label,
            manip_xlims,
            manip_ylims,
            &manip_hlines,
            &manip_vlines,
        )
    };
    let trimmed = base_layout.trim();
    let base_inner = &trimmed[1..trimmed.len() - 1];

    // Control kind: an `AbstractRange` choice renders as a continuous slider
    // (upstream `widget()` `AbstractRange → slider`, Issue #7338); everything else
    // keeps the discrete dropdown (≈ upstream `togglebuttons`). The control is stored
    // as the third `Manipulate` field (a `Symbol`); default to dropdown when absent.
    let is_slider = manip
        .values
        .get(2)
        .map(|v| matches!(v, Value::Symbol(s) if s.as_str() == "slider"))
        .unwrap_or(false);

    let layout = if is_slider {
        let slider =
            render_manipulate_slider(&choice_trace_counts, total_traces, &labels, &base_title);
        format!(r#"{{{},"sliders":[{}]}}"#, base_inner, slider)
    } else {
        let dropdown =
            render_manipulate_dropdown(&choice_trace_counts, total_traces, &labels, &base_title);
        format!(r#"{{{},"updatemenus":[{}]}}"#, base_inner, dropdown)
    };

    Some(format!(
        r#"{{"traces":[{}],"layout":{}}}"#,
        all_traces.join(","),
        layout
    ))
}

/// Splice a `"visible":true|false` key into an already-rendered trace JSON object.
/// Plotly traces start with `{`; we inject the key right after it so the dropdown
/// can toggle whole choice groups via `visible` arrays.
fn with_visibility(trace: &str, visible: bool) -> String {
    let vis = if visible { "true" } else { "false" };
    // `trace` is always a JSON object literal starting with `{"type":...`.
    if let Some(rest) = trace.strip_prefix('{') {
        format!(r#"{{"visible":{},{}"#, vis, rest)
    } else {
        trace.to_string()
    }
}

/// Extract the dropdown labels from the `Manipulate.labels` field (a Vector of
/// strings/symbols). Falls back to an empty vec for non-array values; callers
/// supply positional defaults (`choice N`) when a label is missing.
fn extract_label_strings(value: &Value, heap: &[StructInstance]) -> Vec<String> {
    let Ok(arr) = crate::vm::builtins_linalg::linalg_value_to_array_value(
        value.clone(),
        heap,
        "manipulate",
        None,
    ) else {
        return vec![];
    };
    arr.to_value_vec()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.to_string(),
            Value::Symbol(s) => s.as_str().to_string(),
            other => scalar_f64(other).map(f64_to_json).unwrap_or_default(),
        })
        .collect()
}

/// Build the `"dropdown"` `updatemenus` entry: one button per choice. Each button
/// `restyle`s `visible` so only that choice's contiguous trace group is shown, and
/// `relayout`s the title to `<base_title> (<label>)` (or just the label when there is
/// no base title). The default-active button is choice 0.
fn render_manipulate_dropdown(
    choice_trace_counts: &[usize],
    total_traces: usize,
    labels: &[String],
    base_title: &str,
) -> String {
    let mut trace_start = 0usize;
    let buttons: Vec<String> = choice_trace_counts
        .iter()
        .enumerate()
        .map(|(choice_index, &count)| {
            // visible[t] = true iff trace t belongs to this choice's group.
            let visible: Vec<String> = (0..total_traces)
                .map(|t| {
                    if t >= trace_start && t < trace_start + count {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                })
                .collect();
            trace_start += count;

            let label = labels
                .get(choice_index)
                .cloned()
                .unwrap_or_else(|| format!("choice {}", choice_index + 1));
            let title_text = if base_title.is_empty() {
                label.clone()
            } else {
                format!("{} ({})", base_title, label)
            };
            format!(
                r##"{{"label":"{}","method":"update","args":[{{"visible":[{}]}},{{"title":{{"text":"{}","font":{{"color":"#cdd6f4"}}}}}}]}}"##,
                json_escape(&label),
                visible.join(","),
                json_escape(&title_text),
            )
        })
        .collect();
    format!(
        // A dropdown anchored at the top-left, above the plot, styled to match the
        // Catppuccin theme the static/animation layouts use.
        r##"{{"type":"dropdown","direction":"down","showactive":true,"active":0,"x":0,"y":1.15,"xanchor":"left","yanchor":"top","pad":{{"t":10,"r":10}},"bgcolor":"#313244","bordercolor":"#45475a","font":{{"color":"#cdd6f4"}},"buttons":[{}]}}"##,
        buttons.join(",")
    )
}

/// Build a `sliders` entry for a range-valued `@manipulate` (Issue #7338): one
/// slider step per choice, mirroring `render_manipulate_dropdown`'s buttons. Each
/// step `update`s `visible` so only that choice's contiguous trace group is shown
/// and `relayout`s the title; the default-active step is choice 0. Unlike the
/// animation slider this uses `method:"update"` (the traces are pre-rendered with
/// visibility toggles — there are no Plotly `frames`), matching upstream's
/// `AbstractRange → slider` widget for a static, no-reactive-runtime rendering.
fn render_manipulate_slider(
    choice_trace_counts: &[usize],
    total_traces: usize,
    labels: &[String],
    base_title: &str,
) -> String {
    let mut trace_start = 0usize;
    let steps: Vec<String> = choice_trace_counts
        .iter()
        .enumerate()
        .map(|(choice_index, &count)| {
            let visible: Vec<String> = (0..total_traces)
                .map(|t| {
                    if t >= trace_start && t < trace_start + count {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                })
                .collect();
            trace_start += count;

            let label = labels
                .get(choice_index)
                .cloned()
                .unwrap_or_else(|| format!("choice {}", choice_index + 1));
            let title_text = if base_title.is_empty() {
                label.clone()
            } else {
                format!("{} ({})", base_title, label)
            };
            format!(
                r##"{{"label":"{}","method":"update","args":[{{"visible":[{}]}},{{"title":{{"text":"{}","font":{{"color":"#cdd6f4"}}}}}}]}}"##,
                json_escape(&label),
                visible.join(","),
                json_escape(&title_text),
            )
        })
        .collect();
    format!(
        // Full-width track below the plot, styled to match the Catppuccin theme.
        r##"{{"active":0,"x":0,"y":0,"len":1,"pad":{{"t":60,"b":10}},"currentvalue":{{"visible":true,"prefix":"","xanchor":"right","font":{{"color":"#cdd6f4"}}}},"font":{{"color":"#cdd6f4"}},"steps":[{}]}}"##,
        steps.join(",")
    )
}

fn render_animation_updatemenus(frame_duration: i64) -> String {
    format!(
        // Buttons sit on their own row just below the plot (left-aligned); the
        // slider gets a full-width row below them (Issue: avoid Play/Pause
        // overlapping the slider track on narrow mobile screens).
        r##"{{"type":"buttons","direction":"left","showactive":false,"x":0,"y":0,"xanchor":"left","yanchor":"top","pad":{{"t":50,"r":10}},"bgcolor":"#313244","bordercolor":"#45475a","font":{{"color":"#cdd6f4"}},"buttons":[{{"label":"▶ Play","method":"animate","args":[null,{{"frame":{{"duration":{frame_duration},"redraw":true}},"transition":{{"duration":0}},"fromcurrent":true,"mode":"immediate"}}]}},{{"label":"❚❚ Pause","method":"animate","args":[[null],{{"frame":{{"duration":0,"redraw":false}},"mode":"immediate","transition":{{"duration":0}}}}]}}]}}"##
    )
}

fn render_animation_slider(n_frames: usize, frame_duration: i64) -> String {
    let steps: Vec<String> = (0..n_frames)
        .map(|i| {
            format!(
                r#"{{"label":"{i}","method":"animate","args":[["{i}"],{{"frame":{{"duration":{frame_duration},"redraw":true}},"mode":"immediate","transition":{{"duration":0}}}}]}}"#
            )
        })
        .collect();
    format!(
        // Full-width track on a row below the Play/Pause buttons; the
        // "frame: N" readout is right-anchored so it never sits under them.
        r##"{{"active":0,"x":0,"y":0,"len":1,"pad":{{"t":100,"b":10}},"currentvalue":{{"visible":true,"prefix":"frame: ","xanchor":"right","font":{{"color":"#cdd6f4"}}}},"font":{{"color":"#cdd6f4"}},"steps":[{}]}}"##,
        steps.join(",")
    )
}

fn render_plotly_json(
    series: &[SeriesData],
    aspect_ratio: AspectRatio,
    title: &str,
    xlims: Option<(f64, f64)>,
    ylims: Option<(f64, f64)>,
    hlines: &[f64],
    vlines: &[f64],
) -> String {
    let has_3d = series.iter().any(|s| s.kind.is_3d());
    let showlegend = series.iter().any(|s| s.label.is_some());
    let traces: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(i, sd)| render_trace(sd, COLORS[i % COLORS.len()]))
        .collect();

    let layout = if has_3d {
        render_scene_layout(aspect_ratio, title, showlegend)
    } else {
        render_axis_layout_2d(
            aspect_ratio,
            title,
            showlegend,
            xlims,
            ylims,
            hlines,
            vlines,
        )
    };

    format!(r#"{{"traces":[{}],"layout":{}}}"#, traces.join(","), layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series_2d(kind: SeriesKind) -> SeriesData {
        SeriesData {
            x: vec![0.0, 1.0, 2.0],
            y: vec![0.0, 1.0, 0.0],
            z: vec![],
            z_matrix: None,
            kind,
            label: None,
            contour_levels: ContourLevels::Auto,
        }
    }

    #[test]
    fn test_f64_to_json_finite() {
        assert_eq!(f64_to_json(1.5), "1.5");
        assert_eq!(f64_to_json(0.0), "0");
    }

    #[test]
    fn test_f64_to_json_non_finite() {
        assert_eq!(f64_to_json(f64::NAN), "null");
        assert_eq!(f64_to_json(f64::INFINITY), "null");
        assert_eq!(f64_to_json(f64::NEG_INFINITY), "null");
    }

    #[test]
    fn test_vec_to_json_array() {
        assert_eq!(vec_to_json_array(&[1.0, 2.0, 3.0]), "[1,2,3]");
    }

    #[test]
    fn test_matrix_to_json_array() {
        let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        assert_eq!(matrix_to_json_array(&m), "[[1,2],[3,4]]");
    }

    #[test]
    fn test_render_plotly_json_2d_line() {
        // 2D line → flat scatter trace + 2D (non-scene) layout.
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::None,
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(json.contains(r#""type":"scatter""#));
        assert!(json.contains(r#""mode":"lines""#));
        assert!(!json.contains("scatter3d"));
        assert!(
            !json.contains("\"scene\""),
            "2D plot must not use a scene layout"
        );
        assert!(json.contains("\"xaxis\"") && json.contains("\"yaxis\""));
    }

    #[test]
    fn test_render_plotly_json_2d_scatter() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Scatter)],
            AspectRatio::None,
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(json.contains(r#""type":"scatter""#));
        assert!(json.contains(r#""mode":"markers""#));
        assert!(json.contains("\"marker\""));
    }

    #[test]
    fn test_render_plotly_json_2d_bar() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Bar)],
            AspectRatio::None,
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(json.contains(r#""type":"bar""#));
        assert!(!json.contains(r#""mode":"lines""#));
        assert!(!json.contains("\"scene\""));
    }

    #[test]
    fn test_render_plotly_json_2d_heatmap() {
        let series = vec![SeriesData {
            x: vec![10.0, 20.0],
            y: vec![1.0, 2.0, 3.0],
            z: vec![],
            z_matrix: Some(vec![
                vec![110.0, 120.0],
                vec![210.0, 220.0],
                vec![310.0, 320.0],
            ]),
            kind: SeriesKind::Heatmap,
            label: None,
            contour_levels: ContourLevels::Auto,
        }];
        let json = render_plotly_json(&series, AspectRatio::None, "", None, None, &[], &[]);
        assert!(json.contains(r#""type":"heatmap""#));
        assert!(json.contains("[[110,120],[210,220],[310,320]]"));
        assert!(!json.contains("\"scene\""));
    }

    #[test]
    fn test_render_plotly_json_2d_contour() {
        let series = vec![SeriesData {
            x: vec![10.0, 20.0],
            y: vec![1.0, 2.0, 3.0],
            z: vec![],
            z_matrix: Some(vec![
                vec![110.0, 120.0],
                vec![210.0, 220.0],
                vec![310.0, 320.0],
            ]),
            kind: SeriesKind::Contour,
            label: None,
            contour_levels: ContourLevels::Count(5),
        }];
        let json = render_plotly_json(&series, AspectRatio::None, "", None, None, &[], &[]);
        assert!(json.contains(r#""type":"contour""#));
        assert!(json.contains("[[110,120],[210,220],[310,320]]"));
        assert!(json.contains(r#""contours":{"coloring":"lines"}"#));
        assert!(json.contains(r#""ncontours":7"#));
        assert!(!json.contains("\"scene\""));
    }

    #[test]
    fn test_render_plotly_json_2d_contour_range_levels() {
        let series = vec![SeriesData {
            x: vec![0.0, 1.0],
            y: vec![0.0, 1.0],
            z: vec![],
            z_matrix: Some(vec![vec![-1.0, 0.0], vec![0.0, 1.0]]),
            kind: SeriesKind::Contour,
            label: None,
            contour_levels: ContourLevels::Values(vec![-1.0, -0.5, 0.0, 0.5, 1.0]),
        }];
        let json = render_plotly_json(&series, AspectRatio::None, "", None, None, &[], &[]);
        assert!(json.contains(r#""start":-1"#));
        assert!(json.contains(r#""end":1"#));
        assert!(json.contains(r#""size":0.5"#));
    }

    #[test]
    fn test_render_plotly_json_path3d() {
        let series = vec![SeriesData {
            x: vec![0.0, 1.0],
            y: vec![0.0, 1.0],
            z: vec![0.0, 1.0],
            z_matrix: None,
            kind: SeriesKind::Path3d,
            label: None,
            contour_levels: ContourLevels::Auto,
        }];
        let json = render_plotly_json(&series, AspectRatio::None, "", None, None, &[], &[]);
        assert!(json.contains("\"type\":\"scatter3d\""));
        assert!(json.contains("\"mode\":\"lines\""));
        assert!(
            json.contains("\"scene\""),
            "3D plot must use a scene layout"
        );
        assert!(json.contains("traces"));
        assert!(json.contains("layout"));
    }

    #[test]
    fn test_render_plotly_json_surface_z_orientation() {
        // surface z: size(z) == (length(y), length(x)), i.e. row=y, col=x.
        // For x=[0,1], y=[0,1,2], z is a 3x2 matrix.
        let z_matrix = vec![vec![0.0, 1.0], vec![2.0, 3.0], vec![4.0, 5.0]];
        let series = vec![SeriesData {
            x: vec![0.0, 1.0],
            y: vec![0.0, 1.0, 2.0],
            z: vec![],
            z_matrix: Some(z_matrix),
            kind: SeriesKind::Surface,
            label: None,
            contour_levels: ContourLevels::Auto,
        }];
        let json = render_plotly_json(&series, AspectRatio::None, "", None, None, &[], &[]);
        assert!(json.contains("\"type\":\"surface\""));
        // z should be a 3x2 nested array
        assert!(json.contains("[[0,1],[2,3],[4,5]]"));
    }

    #[test]
    fn test_json_escape() {
        assert_eq!(json_escape("t=1"), "t=1");
        assert_eq!(json_escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(json_escape("a\\b"), "a\\\\b");
        assert_eq!(json_escape("a\nb"), "a\\nb");
    }

    #[test]
    fn test_render_plotly_json_with_title() {
        // Issue #7030: a non-empty title becomes a `layout.title.text`.
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::None,
            "t=1.5",
            None,
            None,
            &[],
            &[],
        );
        assert!(
            json.contains(r#""title":{"text":"t=1.5""#),
            "expected title in layout; got: {json}"
        );
    }

    #[test]
    fn test_render_plotly_json_without_title_omits_title_key() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::None,
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(
            !json.contains(r#""title""#),
            "empty title must not emit a title key; got: {json}"
        );
    }

    #[test]
    fn test_render_axis_layout_2d_escapes_title() {
        let layout =
            render_axis_layout_2d(AspectRatio::None, r#"a"b"#, false, None, None, &[], &[]);
        assert!(layout.contains(r#""text":"a\"b""#));
    }

    #[test]
    fn test_render_plotly_json_2d_equal_aspect_ratio() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::Equal,
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(json.contains(r#""scaleanchor":"x""#));
        assert!(json.contains(r#""scaleratio":1"#));
    }

    #[test]
    fn test_render_plotly_json_2d_numeric_aspect_ratio() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::Ratio(2.0),
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(json.contains(r#""scaleanchor":"x""#));
        assert!(json.contains(r#""scaleratio":2"#));
    }

    #[test]
    fn test_animation_updatemenus_has_play_and_pause() {
        // fps=20 → 50ms per frame. Issue #6355.
        let menus = render_animation_updatemenus(50);
        assert!(menus.contains(r#""method":"animate""#));
        assert!(menus.contains("Play") && menus.contains("Pause"));
        assert!(
            menus.contains(r#""duration":50"#),
            "play button should carry the frame duration; got: {menus}"
        );
    }

    #[test]
    fn test_animation_slider_has_one_step_per_frame() {
        let slider = render_animation_slider(3, 50);
        // One step per frame, labelled/named by index.
        assert_eq!(slider.matches(r#""method":"animate""#).count(), 3);
        assert!(slider.contains(r#"["0"]"#));
        assert!(slider.contains(r#"["2"]"#));
        assert!(slider.contains(r#""prefix":"frame: ""#));
    }

    #[test]
    fn test_with_visibility_injects_key() {
        let trace = r#"{"type":"scatter","x":[1,2]}"#;
        assert_eq!(
            with_visibility(trace, true),
            r#"{"visible":true,"type":"scatter","x":[1,2]}"#
        );
        assert_eq!(
            with_visibility(trace, false),
            r#"{"visible":false,"type":"scatter","x":[1,2]}"#
        );
    }

    #[test]
    fn test_manipulate_dropdown_one_button_per_choice() {
        // Two choices, each with one trace (Issue #7275).
        let dropdown =
            render_manipulate_dropdown(&[1, 1], 2, &["some".to_string(), "other".to_string()], "");
        assert!(dropdown.contains(r#""type":"dropdown""#));
        assert_eq!(dropdown.matches(r#""method":"update""#).count(), 2);
        assert!(dropdown.contains(r#""label":"some""#));
        assert!(dropdown.contains(r#""label":"other""#));
        // Choice 0 shows trace 0 only; choice 1 shows trace 1 only.
        assert!(dropdown.contains(r#""visible":[true,false]"#));
        assert!(dropdown.contains(r#""visible":[false,true]"#));
        assert!(dropdown.contains(r#""active":0"#));
    }

    #[test]
    fn test_manipulate_dropdown_multi_trace_groups() {
        // Choice 0 has 2 traces, choice 1 has 1 trace → visible arrays span the
        // contiguous groups (Issue #7275).
        let dropdown =
            render_manipulate_dropdown(&[2, 1], 3, &["a".to_string(), "b".to_string()], "");
        assert!(dropdown.contains(r#""visible":[true,true,false]"#));
        assert!(dropdown.contains(r#""visible":[false,false,true]"#));
    }

    #[test]
    fn test_manipulate_dropdown_label_fallback() {
        // Missing labels fall back to a positional `choice N` (Issue #7275).
        let dropdown = render_manipulate_dropdown(&[1, 1], 2, &[], "");
        assert!(dropdown.contains(r#""label":"choice 1""#));
        assert!(dropdown.contains(r#""label":"choice 2""#));
    }

    #[test]
    fn test_manipulate_dropdown_title_combines_base() {
        // A base title is suffixed with the choice label; the per-button title is set
        // via the `relayout` half of the `update` args (Issue #7275).
        let dropdown = render_manipulate_dropdown(&[1], 1, &["some".to_string()], "Data");
        assert!(dropdown.contains(r#""text":"Data (some)""#));
    }

    #[test]
    fn test_render_plotly_json_3d_equal_aspect_ratio() {
        let series = vec![SeriesData {
            x: vec![0.0, 1.0],
            y: vec![0.0, 1.0],
            z: vec![0.0, 1.0],
            z_matrix: None,
            kind: SeriesKind::Path3d,
            label: None,
            contour_levels: ContourLevels::Auto,
        }];
        let json = render_plotly_json(&series, AspectRatio::Equal, "", None, None, &[], &[]);
        assert!(json.contains(r#""aspectmode":"data""#));
    }

    #[test]
    fn test_render_trace_with_label_emits_plotly_name_7998() {
        let mut sd = series_2d(SeriesKind::Line);
        sd.label = Some("quadratic".to_string());
        let trace = render_trace(&sd, "#89b4fa");
        assert!(
            trace.contains(r#""name":"quadratic""#),
            "labeled series must emit a Plotly name; got: {trace}"
        );
    }

    #[test]
    fn test_render_trace_label_is_json_escaped_7998() {
        let mut sd = series_2d(SeriesKind::Scatter);
        sd.label = Some(r#"a"b"#.to_string());
        let trace = render_trace(&sd, "#89b4fa");
        assert!(
            trace.contains(r#""name":"a\"b""#),
            "label quotes must be JSON-escaped; got: {trace}"
        );
    }

    #[test]
    fn test_render_plotly_json_with_label_enables_legend_7998() {
        let series = vec![SeriesData {
            x: vec![0.0, 1.0],
            y: vec![0.0, 1.0],
            z: vec![],
            z_matrix: None,
            kind: SeriesKind::Line,
            label: Some("My Line".to_string()),
            contour_levels: ContourLevels::Auto,
        }];
        let json = render_plotly_json(&series, AspectRatio::None, "", None, None, &[], &[]);
        assert!(
            json.contains(r#""name":"My Line""#),
            "labeled trace must carry name; got: {json}"
        );
        assert!(
            json.contains(r#""showlegend":true"#),
            "any labeled series must enable the legend; got: {json}"
        );
    }

    #[test]
    fn test_render_plotly_json_without_label_disables_legend_7998() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::None,
            "",
            None,
            None,
            &[],
            &[],
        );
        assert!(
            json.contains(r#""showlegend":false"#),
            "unlabeled plots must keep legend disabled; got: {json}"
        );
    }

    // --- Issue #7850: xlims, ylims, hlines, vlines ---

    #[test]
    fn test_render_axis_layout_2d_xlims_injects_range() {
        let layout = render_axis_layout_2d(
            AspectRatio::None,
            "",
            false,
            Some((0.0, 5.0)),
            None,
            &[],
            &[],
        );
        assert!(
            layout.contains(r#""range":[0,5]"#),
            "xlims must inject xaxis.range; got: {layout}"
        );
        // When ylims is None, no second "range" key should appear
        let range_count = layout.matches(r#""range""#).count();
        assert_eq!(
            range_count, 1,
            "only one range key expected (xaxis); got: {layout}"
        );
    }

    #[test]
    fn test_render_axis_layout_2d_ylims_injects_range() {
        let layout = render_axis_layout_2d(
            AspectRatio::None,
            "",
            false,
            None,
            Some((-1.0, 10.0)),
            &[],
            &[],
        );
        assert!(
            layout.contains(r#""range":[-1,10]"#),
            "ylims must inject yaxis.range; got: {layout}"
        );
    }

    #[test]
    fn test_render_axis_layout_2d_hlines_inject_shapes() {
        let layout = render_axis_layout_2d(AspectRatio::None, "", false, None, None, &[2.5], &[]);
        assert!(
            layout.contains(r#""shapes""#),
            "hlines must produce a shapes key; got: {layout}"
        );
        assert!(
            layout.contains(r#""yref":"y""#),
            "horizontal line must reference the y axis; got: {layout}"
        );
        assert!(
            layout.contains(r#""y0":2.5,"y1":2.5"#),
            "horizontal line y0 and y1 must equal the hline value; got: {layout}"
        );
    }

    #[test]
    fn test_render_axis_layout_2d_vlines_inject_shapes() {
        let layout = render_axis_layout_2d(AspectRatio::None, "", false, None, None, &[], &[1.5]);
        assert!(layout.contains(r#""shapes""#));
        assert!(
            layout.contains(r#""xref":"x""#),
            "vertical line must reference the x axis; got: {layout}"
        );
        assert!(layout.contains(r#""x0":1.5,"x1":1.5"#));
    }

    #[test]
    fn test_render_axis_layout_2d_no_shapes_when_empty() {
        let layout = render_axis_layout_2d(AspectRatio::None, "", false, None, None, &[], &[]);
        assert!(
            !layout.contains(r#""shapes""#),
            "no shapes key when hlines and vlines are empty; got: {layout}"
        );
    }

    #[test]
    fn test_render_plotly_json_with_xlims_and_hline_7850() {
        let json = render_plotly_json(
            &[series_2d(SeriesKind::Line)],
            AspectRatio::None,
            "",
            Some((0.0, 3.0)),
            None,
            &[1.0],
            &[],
        );
        assert!(
            json.contains(r#""range":[0,3]"#),
            "xlims not in json: {json}"
        );
        assert!(json.contains(r#""shapes""#), "shapes missing: {json}");
        assert!(
            json.contains(r#""y0":1,"y1":1"#),
            "hline value wrong: {json}"
        );
    }

    #[test]
    fn test_extract_lims_from_value_nothing() {
        assert_eq!(extract_lims_from_value(&Value::Nothing), None);
    }

    #[test]
    fn test_extract_lims_from_value_tuple() {
        use subset_julia_vm_bytecode::value::TupleValue;
        let t = Value::Tuple(TupleValue::new(vec![Value::F64(1.0), Value::F64(5.0)]));
        assert_eq!(extract_lims_from_value(&t), Some((1.0, 5.0)));
    }
}
