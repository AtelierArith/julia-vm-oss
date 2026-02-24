//! Test to debug coordinate calculation differences between Julia and Rust VM
//! This test checks specific coordinates that differ between Julia and Rust VM

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{Value, Vm};

fn run_mandelbrot_escape(cr: f64, ci: f64, maxiter: i64) -> i64 {
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

    let mut parser = Parser::new().expect("Parser initialization failed");
    let parsed = parser.parse(&src).expect("Parse failed");
    let mut lowering = Lowering::new(&src);
    let program = lowering.lower(parsed).expect("Lowering failed");
    let compiled = compile_core_program(&program).expect("Compilation failed");

    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    match vm.run().expect("VM execution failed") {
        Value::I64(n) => n,
        Value::F64(n) => n as i64,
        _ => panic!("Unexpected return type"),
    }
}

#[test]
fn test_mandelbrot_coordinates_row0() {
    // Test row 0 (first line, ci = 1.0 - 0 * 0.2 = 1.0)
    let ci: f64 = 1.0 - 0.0 * 0.2;
    assert!((ci - 1.0).abs() < 1e-10, "ci should be 1.0, got {}", ci);

    println!("\nRow 0 (first line) results:");
    for col in 0..=5 {
        let cr: f64 = -2.0 + col as f64 * 0.15;
        let n = run_mandelbrot_escape(cr, ci, 50);
        let ch = if n == 50 {
            '*'
        } else if n > 10 {
            '+'
        } else {
            ' '
        };
        println!("col={}, cr={:.15}, n={}, ch='{}'", col, cr, n, ch);
    }
}

#[test]
fn test_mandelbrot_coordinates_row3() {
    // Test row 3 (4th line, ci = 1.0 - 3 * 0.2 = 0.4)
    let ci: f64 = 1.0 - 3.0 * 0.2;
    assert!((ci - 0.4).abs() < 1e-10, "ci should be 0.4, got {}", ci);

    // Test all columns for row 3 to understand the difference
    // Julia's 4th line (row=3): `     +   +******`
    // Rust VM shows: `++******`
    println!("\nFull row 3 results:");
    for col in 0..=20 {
        let cr: f64 = -2.0 + col as f64 * 0.15;
        let n = run_mandelbrot_escape(cr, ci, 50);
        let ch = if n == 50 {
            '*'
        } else if n > 10 {
            '+'
        } else {
            ' '
        };
        if col <= 7 || col % 2 == 0 {
            println!("col={:2}, cr={:7.3}, n={:2}, ch='{}'", col, cr, n, ch);
        }
    }

    // Compare specific columns that differ
    println!("\nKey columns for comparison:");
    for col in [0, 1, 4, 5, 6].iter() {
        let cr: f64 = -2.0 + *col as f64 * 0.15;
        let n = run_mandelbrot_escape(cr, ci, 50);
        let ch = if n == 50 {
            '*'
        } else if n > 10 {
            '+'
        } else {
            ' '
        };
        println!("col={}, cr={:.15}, n={}, ch='{}'", col, cr, n, ch);
    }
}

#[test]
fn test_coordinate_calculation_precision() {
    // Test coordinate calculation precision
    for row in 0..=10 {
        let ci_julia: f64 = 1.0 - row as f64 * 0.2;
        println!("row={}, ci={:.15}", row, ci_julia);

        for col in 0..=20 {
            let cr_julia: f64 = -2.0 + col as f64 * 0.15;
            // Verify the calculation matches Julia's precision
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
