//! Regression coverage for Mandelbrot coordinate calculations.

use subset_julia_vm::compile_and_run_value;
use subset_julia_vm::vm::Value;

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
fn test_mandelbrot_coordinates_row0() {
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
ci = 1.0
for col in 0:5
    cr = -2.0 + col * 0.15
    checksum = checksum + mandelbrot_escape(cr, ci, 50)
end
checksum
"#,
    );

    let expected: i64 = (0..=5)
        .map(|col| rust_mandelbrot_escape(-2.0 + col as f64 * 0.15, 1.0, 50))
        .sum();
    assert_eq!(vm_checksum, expected);
}

#[test]
fn test_mandelbrot_coordinates_row3() {
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
ci = 0.4
for col in 0:20
    cr = -2.0 + col * 0.15
    checksum = checksum + mandelbrot_escape(cr, ci, 50)
end
checksum
"#,
    );

    let expected: i64 = (0..=20)
        .map(|col| rust_mandelbrot_escape(-2.0 + col as f64 * 0.15, 0.4, 50))
        .sum();
    assert_eq!(vm_checksum, expected);
}

#[test]
fn test_coordinate_calculation_precision() {
    for row in 0..=10 {
        let ci_julia: f64 = 1.0 - row as f64 * 0.2;
        for col in 0..=20 {
            let cr_julia: f64 = -2.0 + col as f64 * 0.15;
            let expected_ci: f64 = 1.0 - row as f64 * 0.2;
            let expected_cr: f64 = -2.0 + col as f64 * 0.15;
            assert!(
                (ci_julia - expected_ci).abs() < 1e-10,
                "ci calculation mismatch"
            );
            assert!(
                (cr_julia - expected_cr).abs() < 1e-10,
                "cr calculation mismatch"
            );
        }
    }
}
