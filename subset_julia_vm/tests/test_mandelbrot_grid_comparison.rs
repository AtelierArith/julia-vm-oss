use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{Value, Vm};
use subset_julia_vm::*;

/// Helper to run code through the Core IR pipeline
/// Returns both the result and the VM (needed to resolve StructRefs)
fn run_core_pipeline(src: &str, seed: u64) -> Result<(Value, Vm<StableRng>), String> {
    use subset_julia_vm::base;

    // Parse Base source
    let prelude_src = base::get_base();
    let mut parser = Parser::new().map_err(|e| e.to_string())?;
    let prelude_parsed = parser.parse(&prelude_src).map_err(|e| e.to_string())?;
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_parsed)
        .map_err(|e| e.to_string())?;

    // Parse user source
    let mut parser = Parser::new().map_err(|e| e.to_string())?;
    let parsed = parser.parse(src).map_err(|e| e.to_string())?;
    let mut lowering = Lowering::new(src);
    let mut program = lowering.lower(parsed).map_err(|e| e.to_string())?;

    // Merge prelude functions (prelude first, then user)
    let mut all_functions = prelude_program.functions;
    all_functions.extend(program.functions);
    program.functions = all_functions;

    // Merge prelude structs (prelude first, then user)
    let mut all_structs = prelude_program.structs;
    all_structs.extend(program.structs);
    program.structs = all_structs;

    // Merge prelude abstract types (prelude first, then user)
    let mut all_abstract_types = prelude_program.abstract_types;
    all_abstract_types.extend(program.abstract_types);
    program.abstract_types = all_abstract_types;

    let compiled = compile_core_program(&program).map_err(|e| e.to_string())?;

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().map_err(|e| e.to_string())?;
    Ok((result, vm))
}

/// Resolve a Value, converting StructRef to Struct using the VM's heap
fn resolve_value(v: &Value, heap: &[subset_julia_vm::vm::value::StructInstance]) -> Value {
    match v {
        Value::StructRef(idx) => {
            if let Some(s) = heap.get(*idx) {
                Value::Struct(s.clone())
            } else {
                v.clone()
            }
        }
        _ => v.clone(),
    }
}

/// Test that the Mandelbrot grid computation matches Julia's output exactly.
///
/// Julia code:
/// ```julia
/// width = 5
/// height = 5
/// xmin = -2.0; xmax = 1.0
/// ymin = -1.2; ymax = 1.2
///
/// xs = range(xmin, xmax; length=width)
/// ys = range(ymax, ymin; length=height)
///
/// xs' .+ im .* ys
/// ```
///
/// Expected output (5×5 Matrix{ComplexF64}):
/// ```
///  -2.0+1.2im  -1.25+1.2im  -0.5+1.2im  0.25+1.2im  1.0+1.2im
///  -2.0+0.6im  -1.25+0.6im  -0.5+0.6im  0.25+0.6im  1.0+0.6im
///  -2.0+0.0im  -1.25+0.0im  -0.5+0.0im  0.25+0.0im  1.0+0.0im
///  -2.0-0.6im  -1.25-0.6im  -0.5-0.6im  0.25-0.6im  1.0-0.6im
///  -2.0-1.2im  -1.25-1.2im  -0.5-1.2im  0.25-1.2im  1.0-1.2im
/// ```
#[test]
fn test_mandelbrot_grid_comparison() {
    let src = r#"
width = 5
height = 5
xmin = -2.0; xmax = 1.0
ymin = -1.2; ymax = 1.2

xs = range(xmin, xmax; length=width)
ys = range(ymax, ymin; length=height)

# Create 2D complex grid via broadcasting
xs' .+ im .* ys
"#;

    let (result, vm) = run_core_pipeline(src, 0).expect("Failed to run Mandelbrot grid test");
    let heap = vm.get_struct_heap();

    // Expected values from Julia (row-major order for readability, but Julia uses column-major)
    // Julia output:
    //  -2.0+1.2im  -1.25+1.2im  -0.5+1.2im  0.25+1.2im  1.0+1.2im
    //  -2.0+0.6im  -1.25+0.6im  -0.5+0.6im  0.25+0.6im  1.0+0.6im
    //  -2.0+0.0im  -1.25+0.0im  -0.5+0.0im  0.25+0.0im  1.0+0.0im
    //  -2.0-0.6im  -1.25-0.6im  -0.5-0.6im  0.25-0.6im  1.0-0.6im
    //  -2.0-1.2im  -1.25-1.2im  -0.5-1.2im  0.25-1.2im  1.0-1.2im
    //
    // Real parts (columns): -2.0, -1.25, -0.5, 0.25, 1.0
    // Imag parts (rows):    1.2,  0.6,   0.0, -0.6, -1.2
    let expected_re = [-2.0, -1.25, -0.5, 0.25, 1.0];
    let expected_im = [1.2, 0.6, 0.0, -0.6, -1.2];

    match result {
        crate::vm::Value::Array(arr) => {
            let arr = arr.borrow();

            // Verify shape is 5×5
            assert_eq!(
                arr.shape,
                vec![5, 5],
                "Expected 5×5 array, got {:?}",
                arr.shape
            );

            // Print the grid for visual inspection
            println!("\n=== sjulia Output ===");
            println!("Array shape: {:?}", arr.shape);
            println!("\n5×5 Matrix{{ComplexF64}}:");
            for row in 1..=5 {
                print!(" ");
                for col in 1..=5 {
                    if let Ok(v) = arr.get(&[row as i64, col as i64]) {
                        let resolved = resolve_value(&v, heap);
                        if let Some((re, im)) = resolved.as_complex_parts() {
                            if im >= 0.0 {
                                print!("{:5.2}+{:.1}im  ", re, im);
                            } else {
                                print!("{:5.2}{:.1}im  ", re, im);
                            }
                        }
                    }
                }
                println!();
            }

            // Verify all 25 values
            println!("\n=== Verification ===");
            let eps = 1e-10;
            let mut all_passed = true;

            for row in 1..=5_usize {
                for col in 1..=5_usize {
                    let expected_real = expected_re[col - 1];
                    let expected_imag = expected_im[row - 1];

                    match arr.get(&[row as i64, col as i64]) {
                        Ok(v) => {
                            let resolved = resolve_value(&v, heap);
                            if let Some((re, im)) = resolved.as_complex_parts() {
                                let re_ok = (re - expected_real).abs() < eps;
                                let im_ok = (im - expected_imag).abs() < eps;

                                if !re_ok || !im_ok {
                                    println!(
                                        "FAIL: [{}, {}] expected {}+{}im, got {}+{}im",
                                        row, col, expected_real, expected_imag, re, im
                                    );
                                    all_passed = false;
                                }
                            } else {
                                println!(
                                    "FAIL: [{}, {}] is not a complex number: {:?}",
                                    row, col, resolved
                                );
                                all_passed = false;
                            }
                        }
                        Err(e) => {
                            println!("FAIL: [{}, {}] access error: {:?}", row, col, e);
                            all_passed = false;
                        }
                    }
                }
            }

            if all_passed {
                println!("All 25 values match Julia's output!");
            }

            // Assert all values match
            for row in 1..=5_usize {
                for col in 1..=5_usize {
                    let expected_real = expected_re[col - 1];
                    let expected_imag = expected_im[row - 1];

                    let v = arr
                        .get(&[row as i64, col as i64])
                        .unwrap_or_else(|e| panic!("Failed to get [{}, {}]: {:?}", row, col, e));
                    let resolved = resolve_value(&v, heap);
                    let (re, im) = resolved.as_complex_parts().unwrap_or_else(|| {
                        panic!("[{}, {}] is not a complex number: {:?}", row, col, resolved)
                    });

                    assert!(
                        (re - expected_real).abs() < eps,
                        "[{}, {}].re: expected {}, got {}",
                        row,
                        col,
                        expected_real,
                        re
                    );
                    assert!(
                        (im - expected_imag).abs() < eps,
                        "[{}, {}].im: expected {}, got {}",
                        row,
                        col,
                        expected_imag,
                        im
                    );
                }
            }
        }
        _ => panic!("Expected Array, got {:?}", result),
    }
}
