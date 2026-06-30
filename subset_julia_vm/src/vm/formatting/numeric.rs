//! Float / BigFloat Julia-style display formatting.
//!
//! Split out of `formatting.rs` by category (Issue #6835).

/// Format a float value Julia-style: whole numbers get ".0" suffix
#[inline]
pub(crate) fn format_float_julia(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf" } else { "-Inf" }.to_string();
    }

    // Check if it's a whole number (no fractional part) AND within
    // the range where Julia uses fixed-point form (`< 1e6`). Larger
    // whole numbers fall through to the scientific arm below.
    // Issue #4802: lowered the upper bound from 1e15 to 1e6 to match
    // Julia's switchover threshold (`1.0e6` is scientific in upstream,
    // `100000.0` is fixed).
    if x.fract() == 0.0 && x.abs() < 1e6 {
        // Issue #4745: `(-0.0_f64) as i64` is `0`, which would lose
        // the sign of negative zero. IEEE 754 and upstream Julia
        // both preserve `-0.0`, so guard the whole-number path
        // before the i64 cast (mirrors `format_float16_julia`).
        if x.is_sign_negative() && x == 0.0 {
            return "-0.0".to_string();
        }
        // Whole number - format with .0 suffix
        return format!("{}.0", x as i64);
    }

    // Issue #4802: for very small / very large magnitudes, upstream
    // Julia uses scientific notation (`1.5e-10`, `1.0e308`) instead
    // of the multi-hundred-digit fixed-point form Rust's default
    // `Display` would produce. Thresholds match Julia's
    // "shortest-roundtrip" cutoff: `|x| < 1e-4` (small) or
    // `|x| >= 1e6` (large).
    let mag = x.abs();
    if mag != 0.0 && !(1e-4..1e6).contains(&mag) {
        return format_float_scientific_julia(x);
    }

    // Normal range: Rust's default Display works.
    x.to_string()
}

/// Format `x` in Julia's scientific form: mantissa always has a `.0`
/// suffix if integer (`1e10` → `1.0e10`), exponent is unsigned only
/// for negative exponents (`1.5e-10`, `1.5e10`). Rust's `{:e}`
/// almost matches but drops the trailing `.0` on whole mantissas.
/// (Issue #4802)
fn format_float_scientific_julia(x: f64) -> String {
    let raw = format!("{:e}", x);
    julia_scientific_postprocess(&raw)
}

/// Issue #4804: f32 scientific helper, using f32's `{:e}` directly so
/// the printed mantissa reflects f32's shortest round-trip rather
/// than the wider f64 form that an `as f64` cast would produce.
fn format_float32_scientific_julia(x: f32) -> String {
    let raw = format!("{:e}", x);
    julia_scientific_postprocess(&raw)
}

/// Append a `.0` to the mantissa when missing so `1e10` becomes
/// `1.0e10`, matching Julia's required decimal-point form.
fn julia_scientific_postprocess(raw: &str) -> String {
    let mut parts = raw.splitn(2, 'e');
    let mantissa = parts.next().unwrap_or("");
    let exponent = parts.next().unwrap_or("");
    if mantissa.contains('.') {
        format!("{}e{}", mantissa, exponent)
    } else {
        format!("{}.0e{}", mantissa, exponent)
    }
}

/// Format a 32-bit float value Julia-style: whole numbers get ".0" suffix.
/// Issue #4804: also routes very small / very large magnitudes through
/// scientific notation (same thresholds as the Float64 path #4802).
#[inline]
pub(crate) fn format_float32_julia(x: f32) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf" } else { "-Inf" }.to_string();
    }

    // Check if it's a whole number (no fractional part) AND within
    // the range where Julia uses fixed-point form.
    // Issue #4804: lowered the upper bound from 1e7 to 1e6 to match
    // Julia's switchover threshold (mirrors #4802 for Float64).
    if x.fract() == 0.0 && x.abs() < 1e6 {
        // Issue #4745: same -0.0 sign-preservation guard as the
        // Float64 path above.
        if x.is_sign_negative() && x == 0.0 {
            return "-0.0".to_string();
        }
        // Whole number - format with .0 suffix
        return format!("{}.0", x as i32);
    }

    // Issue #4804: scientific notation for very small / very large
    // magnitudes (mirrors the Float64 fix #4802). Format via the
    // f32-specific helper so the printed mantissa reflects f32's
    // shortest round-trip rather than the wider f64 form.
    let mag = x.abs();
    if mag != 0.0 && !(1e-4_f32..1e6_f32).contains(&mag) {
        return format_float32_scientific_julia(x);
    }

    // Normal range: Rust's default Display works.
    x.to_string()
}

/// Format a 16-bit float value Julia-style (Issue #3707).
///
/// Julia's `print`/`println` strips the `Float16(…)` type wrapper and shows
/// the shortest decimal that round-trips through Float16. For example,
/// `Float16(3.14)` stores ~3.140625 internally but Julia still prints `3.14`
/// because that decimal parses back to the same Float16 value.
///
/// We emulate this by trying increasing fixed-precision representations and
/// returning the first one that round-trips through `half::f16`.
#[inline]
pub(crate) fn format_float16_julia(x: half::f16) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > half::f16::ZERO { "Inf" } else { "-Inf" }.to_string();
    }
    let f = x.to_f32();
    // Whole numbers get the ".0" suffix; preserve the sign of -0.0 the way
    // Julia does (`println(Float16(-0.0))` prints `-0.0`).
    // Issue #4807: lowered the upper bound from 1e5 to 1e3 to match
    // Julia's F16 switchover threshold (narrower than F32/F64 because
    // F16's max magnitude is only ~6.5e4).
    if f.fract() == 0.0 && f.abs() < 1e3 {
        if f.is_sign_negative() && f == 0.0 {
            return "-0.0".to_string();
        }
        return format!("{}.0", f as i32);
    }

    // Issue #4807: scientific notation for very small / very large
    // F16 magnitudes (mirrors the F64/F32 fix family #4802/#4804 with
    // F16-specific large-side threshold). Find the shortest decimal
    // that round-trips through F16 and emit it in `1.5e3` form via
    // the shared scientific helper.
    let mag = f.abs();
    if mag != 0.0 && !(1e-4..1e3).contains(&mag) {
        // Compute the shortest mantissa via the round-trip loop on
        // the scientific-format candidate. Rust's `{:e}` gives a
        // single-digit mantissa for whole-mantissa cases (`1e3`); the
        // round-trip search varies precision to find the
        // shortest-faithful form.
        for precision in 0..=4 {
            let candidate = format!("{:.*e}", precision, f);
            if let Ok(parsed) = candidate.parse::<f32>() {
                if half::f16::from_f32(parsed) == x {
                    return julia_scientific_postprocess(&candidate);
                }
            }
        }
        // Fall through with the longest-precision form if no
        // shorter round-trip found (rare for F16 — 4 digits is more
        // than enough for f16's ~3.3 decimal digits of precision).
        return julia_scientific_postprocess(&format!("{:.4e}", f));
    }

    // Try 1..=8 digits past the decimal and pick the shortest that round-trips.
    // Float16 has ~3.31 decimal digits of precision, so the loop almost always
    // exits within 4 iterations.
    for precision in 1..=8 {
        let candidate = format!("{:.*}", precision, f);
        if let Ok(parsed) = candidate.parse::<f32>() {
            if half::f16::from_f32(parsed) == x {
                return candidate;
            }
        }
    }
    f.to_string()
}

/// Render a `BigFloat` the way upstream Julia 1.12's `Base.MPFR` prints it
/// (Issue #6789). `astro_float`'s own `Display` emits a raw normalized
/// scientific form (`5.e+0`, `1.0e+6` without padding, `2.5e-1` for `0.25`)
/// that diverges from Julia; this routes the value through
/// [`prettify_bigfloat_string`] after handling the non-finite / zero cases via
/// `astro_float`'s predicates.
pub(crate) fn format_bigfloat_julia(bf: &crate::vm::value::RustBigFloat) -> String {
    let inner: &astro_float::BigFloat = bf;
    if inner.is_nan() {
        return "NaN".to_string();
    }
    if inner.is_inf_pos() {
        return "Inf".to_string();
    }
    if inner.is_inf_neg() {
        return "-Inf".to_string();
    }
    if inner.is_zero() {
        // Julia prints negative zero as `-0.0`.
        return if inner.is_negative() { "-0.0" } else { "0.0" }.to_string();
    }
    prettify_bigfloat_string(&inner.to_string())
}

/// Reformat `astro_float`'s `Display` of a finite, non-zero `BigFloat` into
/// Julia 1.12's `Base.MPFR` spelling (Issue #6789).
///
/// Input is the normalized `[-]D[.F…]e[+-]N` form (single leading digit).
/// Mirrors upstream `_prettify_bigfloat`: positional decimal for exponents in
/// `[-4, 5]`, otherwise scientific. The only deviation from 1.14's helper is
/// the scientific exponent spelling — Julia 1.12 keeps the C `%e` `e±NN` form
/// (signed, ≥2-digit zero-padded: `e+06`, `e-08`, `e+100`), which is the parity
/// gold standard here.
fn prettify_bigfloat_string(raw: &str) -> String {
    let (mantissa_part, exp) = match raw.split_once('e') {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (raw, 0),
    };

    // Clean the mantissa: ensure a '.', strip trailing zeros, then restore a
    // trailing '0' so it always reads `D.D…` (upstream steps).
    let mut mantissa = mantissa_part.to_string();
    if !mantissa.contains('.') {
        mantissa.push('.');
    }
    let trimmed = mantissa.trim_end_matches('0');
    mantissa = if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_string()
    };

    if -5 < exp && exp < 6 {
        if exp == 0 {
            return mantissa;
        }
        let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa.as_str(), ""));
        if exp > 0 {
            let e = exp as usize;
            if e < frac_part.len() {
                format!("{}{}.{}", int_part, &frac_part[..e], &frac_part[e..])
            } else {
                format!(
                    "{}{}{}.0",
                    int_part,
                    frac_part,
                    "0".repeat(e - frac_part.len())
                )
            }
        } else {
            // exp < 0: leading "0.00…" then the single integer digit + frac.
            let neg = int_part.starts_with('-');
            let int_digits = int_part.trim_start_matches('-');
            let frac = if frac_part == "0" { "" } else { frac_part };
            format!(
                "{}0.{}{}{}",
                if neg { "-" } else { "" },
                "0".repeat((-exp - 1) as usize),
                int_digits,
                frac
            )
        }
    } else {
        // Scientific: signed, zero-padded (≥2-digit) exponent (Julia 1.12).
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", mantissa, sign, exp.abs())
    }
}
