//! Test to debug coordinate calculation differences between Julia and Rust VM
//! This test checks specific coordinates and their calculated values

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{Value, Vm};

fn run_coordinate_calculation(row: i64, col: i64) -> (f64, f64) {
    // Calculate ci
    let src_ci = format!(
        r#"
row = {}
ci = 1.0 - row * 0.2
ci
"#,
        row
    );

    let mut parser = Parser::new().expect("Parser initialization failed");
    let parsed_ci = parser.parse(&src_ci).expect("Parse failed");
    let mut lowering = Lowering::new(&src_ci);
    let program_ci = lowering.lower(parsed_ci).expect("Lowering failed");
    let compiled_ci = compile_core_program(&program_ci).expect("Compilation failed");

    let rng_ci = StableRng::new(0);
    let mut vm_ci = Vm::new_program(compiled_ci, rng_ci);
    let ci = match vm_ci.run().expect("VM execution failed") {
        Value::F64(v) => v,
        _ => panic!("Expected F64 for ci"),
    };

    // Calculate cr
    let src_cr = format!(
        r#"
col = {}
cr = -2.0 + col * 0.15
cr
"#,
        col
    );

    let parsed_cr = parser.parse(&src_cr).expect("Parse failed");
    let mut lowering = Lowering::new(&src_cr);
    let program_cr = lowering.lower(parsed_cr).expect("Lowering failed");
    let compiled_cr = compile_core_program(&program_cr).expect("Compilation failed");

    let rng_cr = StableRng::new(0);
    let mut vm_cr = Vm::new_program(compiled_cr, rng_cr);
    let cr = match vm_cr.run().expect("VM execution failed") {
        Value::F64(v) => v,
        _ => panic!("Expected F64 for cr"),
    };

    (ci, cr)
}

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
fn test_coordinate_calculations() {
    println!("\n=== Coordinate Calculation Test ===");

    // Test specific coordinates that differ between Julia and Rust VM
    for row in 0..=5 {
        for col in 0..=5 {
            let (ci, cr) = run_coordinate_calculation(row, col);
            let expected_ci = 1.0 - row as f64 * 0.2;
            let expected_cr = -2.0 + col as f64 * 0.15;

            println!("row={}, col={}: ci={:.15}, cr={:.15}", row, col, ci, cr);
            println!("  Expected: ci={:.15}, cr={:.15}", expected_ci, expected_cr);

            let ci_diff = (ci - expected_ci).abs();
            let cr_diff = (cr - expected_cr).abs();

            if ci_diff > 1e-10 || cr_diff > 1e-10 {
                println!(
                    "  WARNING: Difference detected! ci_diff={:.2e}, cr_diff={:.2e}",
                    ci_diff, cr_diff
                );
            }
        }
    }
}

#[test]
fn test_mandelbrot_escape_for_coordinates() {
    println!("\n=== Mandelbrot Escape Test for Specific Coordinates ===");

    // Test row=3, col=0 and col=1 which differ between Julia and Rust VM
    for row in 0..=5 {
        let ci = 1.0 - row as f64 * 0.2;
        println!("\nRow {}: ci={:.15}", row, ci);

        for col in 0..=5 {
            let cr = -2.0 + col as f64 * 0.15;
            let n = run_mandelbrot_escape(cr, ci, 50);
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
fn test_row3_specific_coordinates() {
    println!("\n=== Row 3 Specific Coordinates Test ===");

    let row = 3;
    let ci = 1.0 - row as f64 * 0.2;
    println!("Row {}: ci={:.15}", row, ci);

    // Test col=0 and col=1 which differ between Julia and Rust VM
    for col in [0, 1, 4, 5, 6].iter() {
        let cr = -2.0 + *col as f64 * 0.15;
        let n = run_mandelbrot_escape(cr, ci, 50);
        let ch = if n == 50 {
            '*'
        } else if n > 10 {
            '+'
        } else {
            ' '
        };

        println!("col={}, cr={:.15}, n={}, ch='{}'", col, cr, n, ch);

        // Compare with Julia's expected values
        // Julia: col 0-4: spaces, col 5: +, col 6-7: spaces
        // Rust VM/iOS: col 0: +, col 1: +, ...
        match col {
            0 | 1 => {
                // Julia expects spaces, but Rust VM/iOS shows +
                println!(
                    "  Note: Julia expects space, but Rust VM/iOS shows '{}'",
                    ch
                );
            }
            5 => {
                // Both should show +
                assert_eq!(ch, '+', "col 5 should be '+'");
            }
            _ => {}
        }
    }
}
