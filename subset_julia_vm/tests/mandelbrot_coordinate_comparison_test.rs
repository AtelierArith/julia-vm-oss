//! Regression coverage for scalar Mandelbrot arithmetic.

use subset_julia_vm::compile_and_run_value;
use subset_julia_vm::vm::Value;

fn run_and_get_f64(src: &str) -> f64 {
    match compile_and_run_value(src, 12345).expect("Execution failed") {
        Value::F64(v) => v,
        Value::I64(v) => v as f64,
        other => panic!("Expected numeric value, got {other:?}"),
    }
}

fn run_and_get_i64(src: &str) -> i64 {
    match compile_and_run_value(src, 12345).expect("Execution failed") {
        Value::I64(v) => v,
        Value::F64(v) => v as i64,
        other => panic!("Expected numeric value, got {other:?}"),
    }
}

fn rust_mandelbrot_escape(cr: f64, ci: f64, maxiter: i64) -> i64 {
    let mut zr = 0.0;
    let mut zi = 0.0;
    for k in 1..=maxiter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return k;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }
    maxiter
}

#[test]
fn test_coordinate_calculations_scalar() {
    let max_diff = run_and_get_f64(
        r#"
maxdiff = 0.0
for row in 0:10
    for col in 0:20
        ci = 1.0 - row * 0.2
        cr = -2.0 + col * 0.15
        expected_ci = 1.0 - row * 0.2
        expected_cr = -2.0 + col * 0.15
        ci_diff = ci - expected_ci
        cr_diff = cr - expected_cr
        if ci_diff < 0.0
            ci_diff = -ci_diff
        end
        if cr_diff < 0.0
            cr_diff = -cr_diff
        end
        if ci_diff > maxdiff
            maxdiff = ci_diff
        end
        if cr_diff > maxdiff
            maxdiff = cr_diff
        end
    end
end
maxdiff
"#,
    );

    assert!(
        max_diff < 1e-9,
        "coordinate difference too large: {max_diff:.2e}"
    );
}

#[test]
fn test_mandelbrot_escape_times() {
    let vm_checksum = run_and_get_i64(
        r#"
function mandelbrot_escape(cr, ci, maxiter)
    zr = 0.0
    zi = 0.0
    for k in 1:maxiter
        zr2 = zr * zr
        zi2 = zi * zi
        if zr2 + zi2 > 4.0
            return k
        end
        zi = 2.0 * zr * zi + ci
        zr = zr2 - zi2 + cr
    end
    return maxiter
end

mandelbrot_escape(0.0, 0.0, 100) +
mandelbrot_escape(-0.75, 0.0, 100) * 10 +
mandelbrot_escape(1.0, 1.0, 100) * 100 +
mandelbrot_escape(-0.1, 0.65, 100) * 1000
"#,
    );

    let expected = rust_mandelbrot_escape(0.0, 0.0, 100)
        + rust_mandelbrot_escape(-0.75, 0.0, 100) * 10
        + rust_mandelbrot_escape(1.0, 1.0, 100) * 100
        + rust_mandelbrot_escape(-0.1, 0.65, 100) * 1000;
    assert_eq!(vm_checksum, expected);
}

#[test]
fn test_intermediate_calculations() {
    let max_diff = run_and_get_f64(
        r#"
maxdiff = 0.0
row = 5
d = row * 0.2 - 1.0
if d < 0.0
    d = -d
end
if d > maxdiff
    maxdiff = d
end
col = 10
d = col * 0.15 - 1.5
if d < 0.0
    d = -d
end
if d > maxdiff
    maxdiff = d
end
row = 5
d = 1.0 - row * 0.2 - 0.0
if d < 0.0
    d = -d
end
if d > maxdiff
    maxdiff = d
end
col = 10
d = -2.0 + col * 0.15 - -0.5
if d < 0.0
    d = -d
end
if d > maxdiff
    maxdiff = d
end
zr = 0.5
zi = 0.3
d = 2.0 * zr * zi - 0.3
if d < 0.0
    d = -d
end
if d > maxdiff
    maxdiff = d
end
maxdiff
"#,
    );

    assert!(
        max_diff < 1e-9,
        "intermediate difference too large: {max_diff:.2e}"
    );
}

#[test]
fn test_specific_mandelbrot_coordinates() {
    let vm_checksum = run_and_get_i64(
        r#"
function mandelbrot_escape(cr, ci, maxiter)
    zr = 0.0
    zi = 0.0
    for k in 1:maxiter
        zr2 = zr * zr
        zi2 = zi * zi
        if zr2 + zi2 > 4.0
            return k
        end
        zi = 2.0 * zr * zi + ci
        zr = zr2 - zi2 + cr
    end
    return maxiter
end

checksum = 0
for row in 0:5
    ci = 1.0 - row * 0.2
    for col in 0:5
        cr = -2.0 + col * 0.15
        checksum = checksum + mandelbrot_escape(cr, ci, 50)
    end
end
checksum
"#,
    );

    let expected: i64 = (0..=5)
        .flat_map(|row| (0..=5).map(move |col| (row, col)))
        .map(|(row, col)| {
            let ci = 1.0 - row as f64 * 0.2;
            let cr = -2.0 + col as f64 * 0.15;
            rust_mandelbrot_escape(cr, ci, 50)
        })
        .sum();
    assert_eq!(vm_checksum, expected);
}

#[test]
fn test_mandelbrot_step_by_step() {
    let vm_escape = run_and_get_i64(
        r#"
function mandelbrot_escape(cr, ci, maxiter)
    zr = 0.0
    zi = 0.0
    for k in 1:maxiter
        zr2 = zr * zr
        zi2 = zi * zi
        if zr2 + zi2 > 4.0
            return k
        end
        zi = 2.0 * zr * zi + ci
        zr = zr2 - zi2 + cr
    end
    return maxiter
end

mandelbrot_escape(-2.0, 0.4, 50)
"#,
    );

    assert_eq!(vm_escape, rust_mandelbrot_escape(-2.0, 0.4, 50));
}
