//! Shared runtime values for Julia constants used by compile-time lowering and
//! VM runtime default evaluation.

use crate::Value;
use half::f16;

/// Resolve a bare identifier that names a floating-point special constant
/// (the `Inf`/`NaN` family, plus `pi`/Unicode Euler) to its runtime `Value`.
///
/// These are Base global constants that the compiler emits as float literals in
/// expression position. The keyword-argument default evaluators must apply the
/// same mapping; otherwise a default like `f(; a = Inf)` falls through to the
/// `Value::I64(0)` fallback instead of resolving to infinity (Issue #8078).
/// `Inf`/`NaN` are not bound as runtime globals, so runtime default evaluation
/// also needs this shared mapping.
///
/// Returns `None` for any name that is not one of these constants. Callers
/// consult bound locals/globals first, so a parameter that shadows one of these
/// names still wins.
pub fn float_special_constant_value(name: &str) -> Option<Value> {
    if is_pi_name(name) {
        return Some(Value::F64(std::f64::consts::PI));
    }
    if is_euler_name(name) {
        return Some(Value::F64(std::f64::consts::E));
    }
    Some(match name {
        "Inf" | "Inf64" => Value::F64(f64::INFINITY),
        "NaN" | "NaN64" => Value::F64(f64::NAN),
        "Inf32" => Value::F32(f32::INFINITY),
        "NaN32" => Value::F32(f32::NAN),
        "Inf16" => Value::F16(f16::INFINITY),
        "NaN16" => Value::F16(f16::NAN),
        _ => return None,
    })
}

fn is_pi_name(name: &str) -> bool {
    matches!(name, "pi" | "\u{03C0}")
}

fn is_euler_name(name: &str) -> bool {
    matches!(name, "\u{212F}")
}

#[cfg(test)]
mod tests {
    use super::float_special_constant_value;
    use crate::Value;

    #[test]
    fn float_special_constant_value_resolves_inf_nan_family_issue_8078() {
        assert!(matches!(
            float_special_constant_value("Inf"),
            Some(Value::F64(v)) if v.is_infinite() && v > 0.0
        ));
        assert!(matches!(
            float_special_constant_value("Inf64"),
            Some(Value::F64(v)) if v.is_infinite() && v > 0.0
        ));
        assert!(matches!(
            float_special_constant_value("Inf32"),
            Some(Value::F32(v)) if v.is_infinite() && v > 0.0
        ));
        assert!(matches!(
            float_special_constant_value("Inf16"),
            Some(Value::F16(v)) if v.is_infinite()
        ));
        assert!(matches!(
            float_special_constant_value("NaN"),
            Some(Value::F64(v)) if v.is_nan()
        ));
        assert!(matches!(
            float_special_constant_value("NaN32"),
            Some(Value::F32(v)) if v.is_nan()
        ));
    }

    #[test]
    fn float_special_constant_value_resolves_base_math_exports_issue_8078() {
        assert!(matches!(
            float_special_constant_value("pi"),
            Some(Value::F64(_))
        ));
        assert!(matches!(
            float_special_constant_value("\u{03C0}"),
            Some(Value::F64(_))
        ));
        assert!(matches!(
            float_special_constant_value("\u{212F}"),
            Some(Value::F64(_))
        ));
        assert!(float_special_constant_value("e").is_none());
        assert!(float_special_constant_value("x").is_none());
        assert!(float_special_constant_value("Inf128").is_none());
    }
}
