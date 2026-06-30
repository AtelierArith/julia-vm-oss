//! Value formatting utilities for FFI output.
//!
//! These functions format VM values for display in REPL and FFI output.

use subset_julia_vm::ffi_support::{
    apply_complex_float_aliases, format_bigfloat_julia, is_native_array_value, vm_format_value,
};
use subset_julia_vm::vm::value::StructInstance;
use subset_julia_vm::vm::Value;

/// Format a struct instance for display.
/// Special cases for well-known types like Rational.
pub fn format_struct_instance(s: &StructInstance) -> String {
    // Special case: Rational - display as num//den like Julia
    if &*s.struct_name == "Rational" && s.values.len() == 2 {
        let num = format_value(&s.values[0]);
        let den = format_value(&s.values[1]);
        return format!("{}//{}", num, den);
    }

    // Special case: Irrational{:sym} singleton (π, ℯ, ...) → bare symbol (Issue #5656).
    if let Some(sym) = s
        .struct_name
        .strip_prefix("Irrational{:")
        .and_then(|rest| rest.strip_suffix('}'))
    {
        return sym.to_string();
    }

    // General case: StructName(field1, field2, ...)
    let fields: Vec<String> = s.values.iter().map(format_value).collect();
    format!("{}({})", s.struct_name, fields.join(", "))
}

/// Format a Complex struct by formatting its fields directly,
/// preserving type-correct display (e.g., `3.0 + 2.0im` for Float64).
///
/// FFI fallback for the C ABI display path, which has no VM to dispatch the
/// pure-Julia `Base.show(io, ::Complex)`. Kept consistent with that method —
/// including the `Complex{Bool}` / imaginary-unit cases — so the FFI output
/// matches upstream Julia (Issue #5155).
fn format_complex_struct_ffi(s: &StructInstance) -> String {
    if s.values.len() != 2 {
        return "Complex(?, ?)".to_string();
    }
    // Complex{Bool}: `im` for the imaginary unit, `Complex(re,im)` otherwise.
    if let (Value::Bool(re), Value::Bool(im)) = (&s.values[0], &s.values[1]) {
        if !*re && *im {
            return "im".to_string();
        }
        return format!("Complex({},{})", re, im);
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

fn numeric_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::F64(v) => Some(*v),
        Value::F32(v) => Some(*v as f64),
        Value::I64(v) => Some(*v as f64),
        Value::I32(v) => Some(*v as f64),
        Value::I16(v) => Some(*v as f64),
        Value::I8(v) => Some(*v as f64),
        Value::U64(v) => Some(*v as f64),
        Value::U32(v) => Some(*v as f64),
        Value::U16(v) => Some(*v as f64),
        Value::U8(v) => Some(*v as f64),
        _ => None,
    }
}

fn format_linrange_struct(s: &StructInstance) -> Option<String> {
    let short_name = s.struct_name.rsplit('.').next().unwrap_or(&s.struct_name);
    if !short_name.starts_with("LinRange") || s.values.len() < 4 {
        return None;
    }

    let start = numeric_to_f64(&s.values[0])?;
    let stop = numeric_to_f64(&s.values[1])?;
    let len = match s.values[2] {
        Value::I64(n) => n,
        Value::I32(n) => i64::from(n),
        _ => return None,
    };

    if len <= 0 {
        return Some(format!(
            "{}:{}:{}",
            format_range_float(start),
            format_range_float(f64::NAN),
            format_range_float(stop)
        ));
    }
    if len == 1 {
        return Some(format_range_float(start));
    }

    let step = (stop - start) / ((len - 1) as f64);
    Some(format!(
        "{}:{}:{}",
        format_range_float(start),
        format_range_float(step),
        format_range_float(stop)
    ))
}

/// Format a Value using the VM struct heap to resolve StructRefs.
pub fn format_value_with_struct_heap(value: &Value, struct_heap: &[StructInstance]) -> String {
    match value {
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .map(|s| format_struct_instance_with_struct_heap(s, struct_heap))
            .unwrap_or_else(|| format!("StructRef#{}", idx)),
        // The self-contained inline `Array{T,N}` wrapper (#6864) displays as the
        // compact `[…]` array form, not the generic struct form.
        Value::Struct(s) if s.array_wrapper_julia_type().is_some() => vm_format_value(value),
        Value::Struct(s) => format_struct_instance_with_struct_heap(s, struct_heap),
        Value::Tuple(t) => {
            let elements: Vec<String> = t
                .elements
                .iter()
                .map(|v| format_value_with_struct_heap(v, struct_heap))
                .collect();
            format!("({})", elements.join(", "))
        }
        Value::NamedTuple(nt) => {
            let pairs: Vec<String> = nt
                .names
                .iter()
                .zip(nt.values.iter())
                .map(|(n, v)| format!("{} = {}", n, format_value_with_struct_heap(v, struct_heap)))
                .collect();
            format!("({})", pairs.join(", "))
        }
        _ => format_value(value),
    }
}

fn format_struct_instance_with_struct_heap(
    s: &StructInstance,
    struct_heap: &[StructInstance],
) -> String {
    if let Some(range) = format_linrange_struct(s) {
        return range;
    }
    if s.is_complex() {
        return format_complex_struct_ffi(s);
    }
    if s.is_rational() && s.values.len() == 2 {
        let num = format_value_with_struct_heap(&s.values[0], struct_heap);
        let den = format_value_with_struct_heap(&s.values[1], struct_heap);
        return format!("{}//{}", num, den);
    }

    let fields: Vec<String> = s
        .values
        .iter()
        .map(|v| format_value_with_struct_heap(v, struct_heap))
        .collect();
    format!("{}({})", s.struct_name, fields.join(", "))
}

/// Format a Value for display in REPL output.
pub fn format_value(value: &Value) -> String {
    // Arrays delegate to `subset_julia_vm::vm::util::format_value` for shape-aware
    // Julia-style `[…]` display. Both the legacy native carrier and the
    // self-contained inline `Array{T,N}` wrapper (the host-return boundary's
    // output since #6864) route through it; the inline wrapper is heap-free, so
    // no `struct_heap` is needed here.
    if is_native_array_value(value)
        || matches!(value, Value::Struct(s) if s.array_wrapper_julia_type().is_some())
    {
        return vm_format_value(value);
    }
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
        Value::Range(r) => {
            if r.is_float {
                if r.is_unit_range() {
                    format!(
                        "{}:{}",
                        format_range_float(r.start),
                        format_range_float(r.stop)
                    )
                } else {
                    format!(
                        "{}:{}:{}",
                        format_range_float(r.start),
                        format_range_float(r.step),
                        format_range_float(r.stop)
                    )
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
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::Rng(_) => "Random.MersenneTwister(...)".to_string(),
        Value::Struct(s) => format_struct_instance(s),
        Value::StructRef(_) => "<struct ref>".to_string(), // Should be resolved by VM before formatting
        Value::SliceAll => ":".to_string(),
        Value::Ref(inner) => {
            // Base.RefValue{T}(value) (Issue #5130) - matches upstream display.
            let v = inner.borrow();
            format!(
                "Base.RefValue{{{}}}({})",
                v.runtime_type(),
                format_value(&v)
            )
        }
        Value::Generator(_) => "Generator(...)".to_string(),
        Value::Char(c) => format!("'{}'", c),
        // DataType displays as type name, with Complex{FloatNN} → ComplexFNN
        // alias to match upstream (Issue #5704).
        Value::DataType(jt) => apply_complex_float_aliases(&jt.to_string()),
        Value::RuntimeTypeVar(tv) => format!("TypeVar(:{})", tv.name),
        Value::Module(m) => m.name.clone(), // Module displays as module name
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
        Value::BigFloat(bf) => format_bigfloat_julia(bf),
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
        Value::MemoryRef(memref) => format!(
            "{}(index={})",
            memref.julia_type_name(),
            memref.memory_index()
        ),
        // The legacy native-array carrier is filtered out by the early-return
        // above (Issue #3908). This wildcard satisfies Rust's exhaustiveness
        // checking and provides a safe default for any future `Value` variant:
        // delegate to the VM's shape-aware formatter.
        _ => vm_format_value(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subset_julia_vm::vm::value::{native_array_value_from_array, ArrayData, ArrayValue};

    #[test]
    fn ffi_format_vector_uses_shared_compact_array_display() {
        let value = native_array_value_from_array(ArrayValue::from_i64(vec![1, 2, 3], vec![3]));

        assert_eq!(format_value(&value), "[1, 2, 3]");
    }

    #[test]
    fn ffi_format_empty_vector_uses_shared_element_type_display() {
        let value = native_array_value_from_array(ArrayValue::from_i64(vec![], vec![0]));

        assert_eq!(format_value(&value), "Int64[]");
    }

    #[test]
    fn ffi_format_matrix_uses_shared_compact_array_display() {
        let value =
            native_array_value_from_array(ArrayValue::from_i64(vec![1, 3, 2, 4], vec![2, 2]));

        assert_eq!(format_value(&value), "[1 2; 3 4]");
    }

    #[test]
    fn ffi_format_string_vector_uses_show_element_quoting() {
        let value = native_array_value_from_array(ArrayValue::new(
            ArrayData::String(vec!["a".to_string(), "b".to_string()]),
            vec![2],
        ));

        assert_eq!(format_value(&value), "[\"a\", \"b\"]");
    }
}
