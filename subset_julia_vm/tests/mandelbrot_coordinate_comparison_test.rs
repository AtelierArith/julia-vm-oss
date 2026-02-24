//! Test to compare coordinate calculations between Rust VM and Julia
//! This helps identify where floating-point precision differences occur

use subset_julia_vm::compile_and_run_value;
use subset_julia_vm::vm::Value;

fn run_and_get_f64(src: &str) -> f64 {
    match compile_and_run_value(src, 12345).expect("Execution failed") {
        Value::F64(v) => v,
        Value::I64(v) => v as f64,
        _ => panic!("Expected numeric value"),
    }
}

fn run_and_get_i64(src: &str) -> i64 {
    match compile_and_run_value(src, 12345).expect("Execution failed") {
        Value::I64(v) => v,
        Value::F64(v) => v as i64,
        _ => panic!("Expected numeric value"),
    }
}

#[test]
fn test_coordinate_calculations_scalar() {
    println!("\n=== Coordinate Calculation Test (Scalar Mandelbrot) ===");

    let mut max_ci_diff = 0.0;
    let mut max_cr_diff = 0.0;
    let mut max_ci_row_col = (0, 0);
    let mut max_cr_row_col = (0, 0);

    // Test coordinate calculations for each row and col
    for row in 0..=10 {
        for col in 0..=20 {
            // Test ci calculation
            let src_ci = format!(
                r#"
row = {}
ci = 1.0 - row * 0.2
ci
"#,
                row
            );

            let ci_val = run_and_get_f64(&src_ci);

            // Test cr calculation
            let src_cr = format!(
                r#"
col = {}
cr = -2.0 + col * 0.15
cr
"#,
                col
            );

            let cr_val = run_and_get_f64(&src_cr);

            // Calculate expected values (Julia's calculation)
            let expected_ci = 1.0 - row as f64 * 0.2;
            let expected_cr = -2.0 + col as f64 * 0.15;

            let ci_diff = (ci_val - expected_ci).abs();
            let cr_diff = (cr_val - expected_cr).abs();

            if ci_diff > max_ci_diff {
                max_ci_diff = ci_diff;
                max_ci_row_col = (row, col);
            }
            if cr_diff > max_cr_diff {
                max_cr_diff = cr_diff;
                max_cr_row_col = (row, col);
            }

            if ci_diff > 1e-10 || cr_diff > 1e-10 {
                println!("row={}, col={}:", row, col);
                println!("  Rust VM: ci={:.15}, cr={:.15}", ci_val, cr_val);
                println!("  Julia:   ci={:.15}, cr={:.15}", expected_ci, expected_cr);
                println!(
                    "  ⚠️  DIFFERENCE: ci_diff={:.2e}, cr_diff={:.2e}",
                    ci_diff, cr_diff
                );
            }

            // Allow small floating-point differences
            assert!(
                ci_diff < 1e-9,
                "ci difference too large for row={}, col={}: {:.2e}",
                row,
                col,
                ci_diff
            );
            assert!(
                cr_diff < 1e-9,
                "cr difference too large for row={}, col={}: {:.2e}",
                row,
                col,
                cr_diff
            );
        }
    }

    println!("\nMax differences:");
    println!(
        "  ci: {:.2e} at row={}, col={}",
        max_ci_diff, max_ci_row_col.0, max_ci_row_col.1
    );
    println!(
        "  cr: {:.2e} at row={}, col={}",
        max_cr_diff, max_cr_row_col.0, max_cr_row_col.1
    );
}

#[test]
fn test_mandelbrot_escape_times() {
    println!("\n=== Mandelbrot Escape Time Comparison ===");

    let test_points = vec![
        (0.0, 0.0, 100, "Point inside set"),
        (-0.75, 0.0, 100, "Point on boundary"),
        (1.0, 1.0, 100, "Point outside (escapes quickly)"),
        (-0.1, 0.65, 100, "Interesting point near boundary"),
    ];

    for (cr, ci, maxiter, description) in test_points {
        let src = format!(
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

mandelbrot_escape({}, {}, {})
"#,
            cr, ci, maxiter
        );

        let result_val = run_and_get_i64(&src);

        println!("{}: {}", description, result_val);

        // Expected values from Julia
        let expected = match (cr, ci) {
            (0.0, 0.0) => 100,
            (-0.75, 0.0) => 100,
            (1.0, 1.0) => 3,
            (-0.1, 0.65) => 76,
            _ => panic!("Unknown test point"),
        };

        assert_eq!(
            result_val, expected,
            "Escape time mismatch for ({}, {})",
            cr, ci
        );
    }
}

#[test]
fn test_intermediate_calculations() {
    println!("\n=== Intermediate Calculation Test ===");

    // Test specific calculations that appear in Mandelbrot
    let test_cases = vec![
        ("row * 0.2", "row = 5\nrow * 0.2", 1.0),
        ("col * 0.15", "col = 10\ncol * 0.15", 1.5),
        ("1.0 - row * 0.2", "row = 5\n1.0 - row * 0.2", 0.0),
        ("-2.0 + col * 0.15", "col = 10\n-2.0 + col * 0.15", -0.5),
        ("2.0 * zr * zi", "zr = 0.5\nzi = 0.3\n2.0 * zr * zi", 0.3),
    ];

    for (name, code, expected) in test_cases {
        let src = format!("{}\n", code);

        let val = run_and_get_f64(&src);
        println!(
            "{}: Rust VM = {:.15}, Expected = {:.15}",
            name, val, expected
        );
        let diff = (val - expected).abs();
        if diff > 1e-10 {
            println!("  ⚠️  DIFFERENCE: {:.2e}", diff);
        }
        assert!(
            diff < 1e-9,
            "Difference too large for {}: {:.2e}",
            name,
            diff
        );
    }
}

#[test]
fn test_specific_mandelbrot_coordinates() {
    println!("\n=== Specific Mandelbrot Coordinates Test ===");

    // Test the specific coordinates that appear in the visualization
    // Focus on row=3 which showed differences in the output
    for row in 0..=5 {
        let ci = 1.0 - row as f64 * 0.2;
        println!("\nRow {}: ci={:.15}", row, ci);

        for col in 0..=5 {
            let cr = -2.0 + col as f64 * 0.15;

            let src = format!(
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

mandelbrot_escape({}, {}, 50)
"#,
                cr, ci
            );

            let n = run_and_get_i64(&src);
            let ch = if n == 50 {
                '*'
            } else if n > 10 {
                '+'
            } else {
                ' '
            };

            println!("  col={}, cr={:.15}, n={}, ch='{}'", col, cr, n, ch);
        }
    }
}

#[test]
fn test_mandelbrot_step_by_step() {
    println!("\n=== Mandelbrot Step-by-Step Calculation Test ===");

    // Test a specific point that shows differences
    let cr = -2.0;
    let ci = 0.4; // row=3

    println!("Testing point: cr={}, ci={}", cr, ci);

    // Calculate first few iterations manually
    let mut zr = 0.0;
    let mut zi = 0.0;

    for k in 1..=10 {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        let mag2 = zr2 + zi2;

        println!(
            "Iteration {}: zr={:.15}, zi={:.15}, |z|^2={:.15}",
            k, zr, zi, mag2
        );

        if mag2 > 4.0 {
            println!("  Escaped at iteration {}", k);
            break;
        }

        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }

    // Compare with Rust VM
    let src = format!(
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

mandelbrot_escape({}, {}, 50)
"#,
        cr, ci
    );

    let n = run_and_get_i64(&src);
    println!("Rust VM result: n={}", n);
}
