//! C-style `sprintf`/`printf` formatting.
//!
//! Split out of `formatting.rs` by category (Issue #6835).

use super::super::value::Value;
use super::*;

/// C-style sprintf formatting
pub(crate) fn format_sprintf(fmt: &str, args: &[Value]) -> String {
    let mut result = String::new();
    let mut chars = fmt.chars().peekable();
    let mut arg_idx = 0;

    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('%') => {
                    result.push('%');
                    chars.next();
                }
                Some(_) => {
                    // Skip flags, width, and precision
                    while chars
                        .peek()
                        .is_some_and(|&c| c == '-' || c == '+' || c == ' ' || c == '#' || c == '0')
                    {
                        chars.next();
                    }
                    // Skip width
                    while chars.peek().is_some_and(|&c| c.is_ascii_digit()) {
                        chars.next();
                    }
                    // Skip precision
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        while chars.peek().is_some_and(|&c| c.is_ascii_digit()) {
                            chars.next();
                        }
                    }
                    // Get type specifier
                    if let Some(&spec) = chars.peek() {
                        chars.next();
                        if arg_idx < args.len() {
                            let formatted = match spec {
                                's' => format_value(&args[arg_idx]),
                                'd' | 'i' => format_sprintf_int(&args[arg_idx]),
                                'f' | 'e' | 'E' | 'g' | 'G' => format_sprintf_float(&args[arg_idx]),
                                'x' => format_sprintf_hex(&args[arg_idx], false),
                                'X' => format_sprintf_hex(&args[arg_idx], true),
                                'o' => format_sprintf_octal(&args[arg_idx]),
                                'c' => format_sprintf_char(&args[arg_idx]),
                                _ => format_value(&args[arg_idx]),
                            };
                            result.push_str(&formatted);
                            arg_idx += 1;
                        }
                    }
                }
                None => result.push('%'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// C-style float→string for the pure-Julia Printf engine (Issue #6746).
/// `conv` is one of f F e E g G; `precision < 0` means the C default (6). Returns
/// only the converted number (sign of the value is included; flags/width padding
/// are applied by the pure-Julia caller). Inf/NaN render as Inf/-Inf/NaN.
pub(crate) fn format_printf_float(x: f64, conv: char, precision: i64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 {
            "-Inf".to_string()
        } else {
            "Inf".to_string()
        };
    }
    let p = if precision < 0 {
        6usize
    } else {
        precision as usize
    };
    match conv {
        'f' | 'F' => format!("{:.*}", p, x),
        'e' => format_printf_exp(x, p, false),
        'E' => format_printf_exp(x, p, true),
        'g' | 'G' => format_printf_g(x, p, conv == 'G'),
        _ => format!("{:.*}", p, x),
    }
}

/// C-style `%e`: mantissa + `e`/`E` + signed two-(or-more-)digit exponent.
fn format_printf_exp(x: f64, p: usize, upper: bool) -> String {
    let s = format!("{:.*e}", p, x); // e.g. "1.500000e0" / "1.5e-3"
    let (mant, exp) = match s.split_once('e') {
        Some((m, e)) => (m, e),
        None => return s,
    };
    let exp_n: i64 = exp.parse().unwrap_or(0);
    let e_ch = if upper { 'E' } else { 'e' };
    let sign = if exp_n < 0 { '-' } else { '+' };
    format!("{}{}{}{:02}", mant, e_ch, sign, exp_n.abs())
}

/// C-style `%g`: shortest of `%e`/`%f` with trailing zeros stripped.
fn format_printf_g(x: f64, precision: usize, upper: bool) -> String {
    let prec = if precision == 0 { 1 } else { precision };
    if x == 0.0 {
        return "0".to_string();
    }
    // True decimal exponent (avoids log10 rounding error at powers of ten).
    let exp = format!("{:e}", x.abs())
        .split('e')
        .nth(1)
        .and_then(|e| e.parse::<i64>().ok())
        .unwrap_or(0);
    let mut s = if exp >= -4 && exp < prec as i64 {
        let fprec = (prec as i64 - 1 - exp).max(0) as usize;
        format!("{:.*}", fprec, x)
    } else {
        format_printf_exp(x, prec - 1, upper)
    };
    // Strip trailing fractional zeros (C %g default; the # flag is handled by the
    // pure-Julia caller, which does not route through %g stripping).
    if let Some(e_pos) = s.find(['e', 'E']) {
        let (mant, rest) = s.split_at(e_pos);
        s = format!("{}{}", strip_frac_zeros(mant), rest);
    } else {
        s = strip_frac_zeros(&s);
    }
    s
}

fn strip_frac_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[inline]
fn format_sprintf_int(v: &Value) -> String {
    match v {
        Value::I64(x) => x.to_string(),
        Value::I32(x) => x.to_string(),
        Value::I16(x) => x.to_string(),
        Value::I8(x) => x.to_string(),
        Value::F64(x) => (*x as i64).to_string(),
        Value::F32(x) => (*x as i64).to_string(),
        Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        _ => format_value(v),
    }
}

#[inline]
fn format_sprintf_float(v: &Value) -> String {
    match v {
        Value::F64(x) => x.to_string(),
        Value::F32(x) => x.to_string(),
        Value::I64(x) => (*x as f64).to_string(),
        Value::I32(x) => (*x as f64).to_string(),
        _ => format_value(v),
    }
}

#[inline]
fn format_sprintf_hex(v: &Value, uppercase: bool) -> String {
    let n = match v {
        Value::I64(x) => *x,
        Value::I32(x) => *x as i64,
        Value::I16(x) => *x as i64,
        Value::I8(x) => *x as i64,
        _ => return format_value(v),
    };
    if uppercase {
        format!("{:X}", n)
    } else {
        format!("{:x}", n)
    }
}

#[inline]
fn format_sprintf_octal(v: &Value) -> String {
    let n = match v {
        Value::I64(x) => *x,
        Value::I32(x) => *x as i64,
        Value::I16(x) => *x as i64,
        Value::I8(x) => *x as i64,
        _ => return format_value(v),
    };
    format!("{:o}", n)
}

#[inline]
fn format_sprintf_char(v: &Value) -> String {
    match v {
        Value::Char(c) => c.to_string(),
        Value::I64(x) => char::from_u32(*x as u32).map_or("?".to_string(), |c| c.to_string()),
        Value::I32(x) => char::from_u32(*x as u32).map_or("?".to_string(), |c| c.to_string()),
        _ => format_value(v),
    }
}
