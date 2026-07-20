//! Float / BigFloat Julia-style display formatting.
//!
//! Split out of `formatting.rs` by category (Issue #6835).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

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
pub fn format_bigfloat_julia(bf: &crate::vm::value::RustBigFloat) -> String {
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
    prettify_bigfloat_string(&bigfloat_mpfr_digits(inner))
}

/// Produce the `astro_float` decimal Display string of a finite, non-zero
/// `BigFloat`, but with the *same number of significant decimal digits* that
/// upstream Julia's `Base.MPFR` (`mpfr_asprintf` with `%Re`) would print
/// (Issue #8885).
///
/// astro-float's `Display` emits `ceil(prec · log10 2)` significant digits,
/// whereas MPFR's `%Re` emits one "guard" digit more —
/// `m = 1 + ceil(prec · log10 2)`. Both are correctly-rounded renderings of the
/// *same* binary value, so astro-float's shorter output is MPFR's value rounded
/// to one fewer digit; the missing guard digit is exactly what makes the last
/// decimal place drift (e.g. `true / BigFloat("2.5")` printed `…0001` instead of
/// MPFR's `…0009`). We reproduce MPFR's digit count by re-rendering the value at
/// a higher working precision (which only appends more true digits — padding the
/// mantissa with zero bits does not change the value) and rounding the decimal
/// string back to exactly `m` significant digits with round-half-to-even.
fn bigfloat_mpfr_digits(inner: &astro_float::BigFloat) -> String {
    // Precision (in bits) that MPFR bases its digit count on: the mantissa
    // length of this specific value. `mantissa_max_bit_len` returns the full
    // mantissa length for normal *and* subnormal numbers.
    let prec = match inner.mantissa_max_bit_len() {
        Some(p) if p > 0 => p,
        _ => return inner.to_string(),
    };
    // MPFR's `%Re` significant-digit count for a precision-`prec` value.
    let m = 1 + (prec as f64 * std::f64::consts::LOG10_2).ceil() as usize;

    // Subnormal astro-float values print with a leading `0` and do not carry a
    // clean single-leading-digit normalized form; leave those on the direct
    // path rather than risk mis-rounding their significant digits.
    if inner.is_subnormal() {
        return inner.to_string();
    }

    // Re-render at extra precision so astro-float emits comfortably more than
    // `m` correctly-rounded digits (padding with zero bits leaves the value
    // unchanged), then round the decimal string down to `m` digits.
    let mut hi = inner.clone();
    if hi
        .set_precision(prec + 160, astro_float::RoundingMode::ToEven)
        .is_err()
    {
        return inner.to_string();
    }
    round_decimal_string_to_sig(&hi.to_string(), m)
}

/// Round a normalized decimal string `s` in astro-float `Display` form
/// (`[-]D.F…e±N`, a single leading integer digit and an `e`-exponent) to at most
/// `m` significant digits using round-half-to-even (banker's rounding, matching
/// MPFR's default). Returns a string in the same `[-]D.F…e±N` form that
/// [`prettify_bigfloat_string`] consumes. If `s` already has `≤ m` significant
/// digits it is returned unchanged (Issue #8885).
fn round_decimal_string_to_sig(s: &str, m: usize) -> String {
    debug_assert!(m >= 1);
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s),
    };
    let (mant, exp) = match rest.split_once('e') {
        Some((mt, ex)) => (mt, ex.parse::<i64>().unwrap_or(0)),
        None => (rest, 0i64),
    };
    // Significant digits with the decimal point removed. astro-float's
    // normalized non-subnormal form has a single nonzero leading digit, so the
    // implied decimal point sits after `digits[0]` and `exp` already accounts
    // for it (value = digits[0].digits[1..] × 10^exp).
    let mut digits: Vec<u8> = mant
        .bytes()
        .filter(|&b| b != b'.')
        .map(|b| b.wrapping_sub(b'0'))
        .collect();

    if digits.len() <= m {
        return s.to_string();
    }

    // Round-half-to-even at position `m` (0-based first dropped digit).
    let round_digit = digits[m];
    let tail_nonzero = digits[m + 1..].iter().any(|&d| d != 0);
    let round_up =
        round_digit > 5 || (round_digit == 5 && (tail_nonzero || (digits[m - 1] & 1) == 1));
    digits.truncate(m);

    let mut exp = exp;
    if round_up {
        // Propagate the carry leftward; if it falls off the leading digit the
        // mantissa was all 9s (→ 1.000…) so we renormalize and bump the exponent.
        let mut i = m;
        let mut carry_out = true;
        while i > 0 {
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                carry_out = false;
                break;
            }
        }
        if carry_out {
            digits.insert(0, 1);
            digits.truncate(m);
            exp += 1;
        }
    }

    let mut out = String::with_capacity(m + 8 + sign.len());
    out.push_str(sign);
    out.push((b'0' + digits[0]) as char);
    out.push('.');
    if m == 1 {
        out.push('0');
    } else {
        for &d in &digits[1..] {
            out.push((b'0' + d) as char);
        }
    }
    // Reconstruct astro-float's exponent spelling (`e+N` / `e-N`) so the result
    // is indistinguishable from an un-rounded Display string to the downstream
    // `prettify_bigfloat_string`.
    out.push('e');
    if exp < 0 {
        out.push('-');
        out.push_str(&exp.unsigned_abs().to_string());
    } else {
        out.push('+');
        out.push_str(&exp.to_string());
    }
    out
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod bigfloat_repr_tests {
    use super::round_decimal_string_to_sig;

    #[test]
    fn returns_unchanged_when_already_short_enough() {
        // 3 significant digits, m = 5 → nothing to round.
        assert_eq!(round_decimal_string_to_sig("1.23e+0", 5), "1.23e+0");
        assert_eq!(round_decimal_string_to_sig("4.00e-1", 5), "4.00e-1");
    }

    #[test]
    fn round_down_drops_trailing_below_half() {
        // 1.234 → 3 sig digits, next digit 4 < 5 → 1.23.
        assert_eq!(round_decimal_string_to_sig("1.234e+0", 3), "1.23e+0");
    }

    #[test]
    fn round_up_above_half() {
        // 1.236 → 3 sig digits, next digit 6 > 5 → 1.24.
        assert_eq!(round_decimal_string_to_sig("1.236e+0", 3), "1.24e+0");
    }

    #[test]
    fn round_half_to_even_ties() {
        // Exact tie 1.25 → keep even last digit (2) → 1.2.
        assert_eq!(round_decimal_string_to_sig("1.250e+0", 2), "1.2e+0");
        // Exact tie 1.35 → round to even (4) → 1.4.
        assert_eq!(round_decimal_string_to_sig("1.350e+0", 2), "1.4e+0");
        // Tie with a nonzero tail is NOT a tie → always rounds up.
        assert_eq!(round_decimal_string_to_sig("1.2500001e+0", 2), "1.3e+0");
    }

    #[test]
    fn carry_propagates_through_nines() {
        // 1.99 rounds up at 2 sig digits → 2.0 (carry into the leading digit,
        // no exponent change).
        assert_eq!(round_decimal_string_to_sig("1.996e+0", 2), "2.0e+0");
    }

    #[test]
    fn carry_out_of_leading_digit_bumps_exponent() {
        // 9.99 → 2 sig digits rounds to 10 → renormalize to 1.0 × 10^(exp+1).
        assert_eq!(round_decimal_string_to_sig("9.99e+0", 2), "1.0e+1");
        // Same with a negative exponent.
        assert_eq!(round_decimal_string_to_sig("9.999e-3", 3), "1.00e-2");
    }

    #[test]
    fn preserves_sign() {
        assert_eq!(round_decimal_string_to_sig("-1.236e+0", 3), "-1.24e+0");
        assert_eq!(round_decimal_string_to_sig("-9.99e+0", 2), "-1.0e+1");
    }

    #[test]
    fn single_digit_target() {
        // m == 1 keeps a single significant digit with a ".0" fractional part.
        assert_eq!(round_decimal_string_to_sig("4.6e+0", 1), "5.0e+0");
        assert_eq!(round_decimal_string_to_sig("4.4e+0", 1), "4.0e+0");
    }

    #[test]
    fn keeps_exactly_m_significant_digits_with_guard_digit() {
        // Mirrors the Issue #8885 shape: a value `0.400…0009…` whose exact
        // expansion has a `9` guard digit at position 79 followed by more
        // digits. Rounding the longer (higher-precision) rendering to exactly
        // 79 significant digits must keep that trailing `…0009`, reproducing
        // MPFR's output rather than astro-float's one-digit-shorter `…001`.
        let long = format!("4.{}9123e-1", "0".repeat(77)); // 4 + 77 zeros + 9…
        let out = round_decimal_string_to_sig(&long, 79);
        assert!(out.ends_with("0009e-1"), "expected …0009e-1, got {out}");
        // 79 significant digits total: leading '4' + 78 fractional digits.
        let frac = out
            .strip_prefix("4.")
            .and_then(|s| s.split_once('e'))
            .map(|(m, _)| m.len())
            .unwrap();
        assert_eq!(frac, 78);
    }

    #[test]
    fn round_up_can_produce_a_trailing_nine() {
        // 1.089 → 3 sig digits: the dropped 9 rounds 8 up to 9 → 1.09.
        assert_eq!(round_decimal_string_to_sig("1.089e+0", 3), "1.09e+0");
    }
}
