//! Value formatting utilities for FFI output.
//!
//! These functions format VM values for display in REPL and FFI output.

use crate::vm::value::StructInstance;
use crate::vm::Value;

/// Format a struct instance for display.
/// Special cases for well-known types like Rational.
pub fn format_struct_instance(s: &StructInstance) -> String {
    // Special case: Rational - display as num//den like Julia
    if s.struct_name == "Rational" && s.values.len() == 2 {
        let num = format_value(&s.values[0]);
        let den = format_value(&s.values[1]);
        return format!("{}//{}", num, den);
    }

    // General case: StructName(field1, field2, ...)
    let fields: Vec<String> = s.values.iter().map(format_value).collect();
    format!("{}({})", s.struct_name, fields.join(", "))
}

/// Format a Complex struct by formatting its fields directly,
/// preserving type-correct display (e.g., `3.0 + 2.0im` for Float64).
fn format_complex_struct_ffi(s: &StructInstance) -> String {
    if s.values.len() != 2 {
        return "Complex(?, ?)".to_string();
    }
    let re_str = format_value(&s.values[0]);
    let im_val = &s.values[1];
    let is_negative = match im_val {
        Value::F64(x) => *x < 0.0,
        Value::I64(x) => *x < 0,
        Value::F32(x) => *x < 0.0,
        _ => false,
    };
    if is_negative {
        let neg_im = match im_val {
            Value::F64(x) => format_value(&Value::F64(-x)),
            Value::I64(x) => format_value(&Value::I64(-x)),
            Value::F32(x) => format_value(&Value::F32(-x)),
            other => format_value(other),
        };
        format!("{} - {}im", re_str, neg_im)
    } else {
        let im_str = format_value(im_val);
        format!("{} + {}im", re_str, im_str)
    }
}

/// Format a float value for range display.
fn format_range_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

/// Format a Value for display in REPL output.
pub fn format_value(value: &Value) -> String {
    match value {
        // Signed integers
        Value::I8(v) => v.to_string(),
        Value::I16(v) => v.to_string(),
        Value::I32(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::I128(v) => v.to_string(),
        // Unsigned integers
        Value::U8(v) => v.to_string(),
        Value::U16(v) => v.to_string(),
        Value::U32(v) => v.to_string(),
        Value::U64(v) => v.to_string(),
        Value::U128(v) => v.to_string(),
        // Boolean
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        // Floating point
        Value::F16(v) => format!("Float16({})", v.to_f32()),
        Value::F32(v) => v.to_string(),
        Value::F64(v) => {
            // Format floats like Julia: show .0 for integers
            if v.fract() == 0.0 && v.abs() < 1e15 {
                format!("{:.1}", v)
            } else {
                format!("{}", v)
            }
        }
        Value::Struct(s) if s.is_complex() => format_complex_struct_ffi(s),
        Value::Str(s) => format!("\"{}\"", s),
        Value::Array(arr) => {
            let arr_borrow = arr.borrow();
            if arr_borrow.shape.len() == 1 {
                // 1D array
                let values = arr_borrow.to_value_vec();
                let elements: Vec<String> = values.iter().take(10).map(format_value).collect();
                if values.len() > 10 {
                    format!("[{}, ...]", elements.join(", "))
                } else {
                    format!("[{}]", elements.join(", "))
                }
            } else if arr_borrow.shape.len() == 2 {
                // 2D matrix: display with elements
                let rows = arr_borrow.shape[0];
                let cols = arr_borrow.shape[1];
                let values = arr_borrow.to_value_vec();
                let mut lines = Vec::new();

                // Format matrix dimensions with correct element type
                let element_type = arr_borrow.element_type();
                let type_name = element_type.julia_type_name();
                lines.push(format!("{}×{} Matrix{{{}}}", rows, cols, type_name));

                // Format matrix rows (column-major indexing)
                for r in 0..rows {
                    let row: Vec<String> = (0..cols)
                        .map(|c| format_value(&values[r + c * rows]))
                        .collect();
                    lines.push(format!(" {}", row.join("  ")));
                }
                lines.join("\n")
            } else {
                format!("{:?}-element Array", arr_borrow.shape)
            }
        }
        Value::Range(r) => {
            if r.is_float {
                if r.is_unit_range() {
                    format!("{}:{}", format_range_float(r.start), format_range_float(r.stop))
                } else {
                    format!("{}:{}:{}", format_range_float(r.start), format_range_float(r.step), format_range_float(r.stop))
                }
            } else if r.is_unit_range() {
                format!("{:.0}:{:.0}", r.start, r.stop)
            } else {
                format!("{:.0}:{:.0}:{:.0}", r.start, r.step, r.stop)
            }
        }
        Value::Tuple(t) => {
            let elements: Vec<String> = t.elements.iter().map(format_value).collect();
            format!("({})", elements.join(", "))
        }
        Value::NamedTuple(nt) => {
            let pairs: Vec<String> = nt
                .names
                .iter()
                .zip(nt.values.iter())
                .map(|(n, v)| format!("{} = {}", n, format_value(v)))
                .collect();
            format!("({})", pairs.join(", "))
        }
        Value::Dict(d) => format!("Dict with {} entries", d.len()),
        Value::Set(s) => format!("Set with {} elements", s.elements.len()),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::Rng(_) => "Random.MersenneTwister(...)".to_string(),
        Value::Struct(s) => format_struct_instance(s),
        Value::StructRef(_) => "<struct ref>".to_string(), // Should be resolved by VM before formatting
        Value::SliceAll => ":".to_string(),
        Value::Ref(inner) => format!("Ref({})", format_value(inner)),
        Value::Generator(_) => "Generator(...)".to_string(),
        Value::Char(c) => format!("'{}'", c),
        Value::DataType(jt) => jt.to_string(), // DataType displays as type name
        Value::Module(m) => m.name.clone(),    // Module displays as module name
        Value::Function(f) => format!("function {}", f.name),
        Value::Closure(c) => {
            if c.captures.is_empty() {
                format!("closure {}", c.name)
            } else {
                let caps: Vec<String> = c.captures.iter().map(|(n, _)| n.clone()).collect();
                format!("closure {} [captures: {}]", c.name, caps.join(", "))
            }
        }
        Value::ComposedFunction(cf) => {
            let outer_str = format_value(&cf.outer);
            let inner_str = format_value(&cf.inner);
            format!("{} ∘ {}", outer_str, inner_str)
        }
        Value::BigInt(n) => n.to_string(),
        Value::BigFloat(bf) => bf.to_string(),
        Value::Undef => "#undef".to_string(),
        Value::IO(io_ref) => {
            if io_ref.borrow().is_stdout() {
                "stdout".to_string()
            } else {
                "IOBuffer(...)".to_string()
            }
        }
        // Macro system types
        Value::Symbol(s) => format!(":{}", s.as_str()),
        Value::Expr(e) => e.to_string(),
        Value::QuoteNode(v) => format!("QuoteNode({})", format_value(v)),
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        // Base.Pairs type (for kwargs...)
        Value::Pairs(p) => {
            let pairs: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(n, v)| format!(":{} => {}", n, format_value(v)))
                .collect();
            format!("pairs({})", pairs.join(", "))
        }
        // Regex types
        Value::Regex(r) => {
            if r.flags.is_empty() {
                format!("r\"{}\"", r.pattern)
            } else {
                format!("r\"{}\"{}", r.pattern, r.flags)
            }
        }
        Value::RegexMatch(m) => {
            let captures_str = if m.captures.is_empty() {
                String::new()
            } else {
                let caps: Vec<String> = m
                    .captures
                    .iter()
                    .map(|c| match c {
                        Some(s) => format!("\"{}\"", s),
                        None => "nothing".to_string(),
                    })
                    .collect();
                format!(", captures=({})", caps.join(", "))
            };
            format!(
                "RegexMatch(\"{}\", offset={}{})",
                m.match_str, m.offset, captures_str
            )
        }
        // Enum type
        Value::Enum { type_name, value } => format!("{}({})", type_name, value),
        // Memory{T} flat typed buffer
        Value::Memory(mem) => {
            let mem = mem.borrow();
            let n = mem.len();
            let type_name = mem.element_type().julia_type_name();
            if n == 0 {
                format!("0-element Memory{{{}}}", type_name)
            } else {
                let mut parts = Vec::new();
                for i in 1..=n.min(10) {
                    if let Ok(v) = mem.get(i) {
                        parts.push(format_value(&v));
                    }
                }
                if n > 10 {
                    format!("[{}, ...]", parts.join(", "))
                } else {
                    format!("[{}]", parts.join(", "))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{new_array_ref, ArrayData, ArrayValue};

    #[test]
    fn test_matrix_display_int64_type() {
        // Create a 2x2 Int64 matrix: [1 2; 3 4]
        let data = ArrayData::I64(vec![1, 3, 2, 4]); // column-major
        let arr = ArrayValue::new(data, vec![2, 2]);
        let value = Value::Array(new_array_ref(arr));

        let formatted = format_value(&value);
        assert!(
            formatted.contains("Matrix{Int64}"),
            "Expected 'Matrix{{Int64}}', got: {}",
            formatted
        );
    }

    #[test]
    fn test_matrix_display_float64_type() {
        // Create a 2x2 Float64 matrix: [1.0 2.0; 3.0 4.0]
        let data = ArrayData::F64(vec![1.0, 3.0, 2.0, 4.0]); // column-major
        let arr = ArrayValue::new(data, vec![2, 2]);
        let value = Value::Array(new_array_ref(arr));

        let formatted = format_value(&value);
        assert!(
            formatted.contains("Matrix{Float64}"),
            "Expected 'Matrix{{Float64}}', got: {}",
            formatted
        );
    }

    #[test]
    fn test_matrix_display_bool_type() {
        // Create a 2x2 Bool matrix
        let data = ArrayData::Bool(vec![true, false, false, true]); // column-major
        let arr = ArrayValue::new(data, vec![2, 2]);
        let value = Value::Array(new_array_ref(arr));

        let formatted = format_value(&value);
        assert!(
            formatted.contains("Matrix{Bool}"),
            "Expected 'Matrix{{Bool}}', got: {}",
            formatted
        );
    }
}
