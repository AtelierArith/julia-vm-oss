use subset_julia_vm::*;

#[test]
fn test_mandelbrot_grid_direct_index_api_regression() {
    let src = r#"
width = 5
height = 5
xmin = -2.0; xmax = 1.0
ymin = -1.2; ymax = 1.2

xs = range(xmin, xmax; length=width)
ys = range(ymax, ymin; length=height)

((xs' .+ im .* ys)[1, 1]).re
"#;

    let result = compile_and_run_value(src, 0).expect("Failed to run direct Mandelbrot index");
    match result {
        crate::vm::Value::F64(value) => assert!((value + 2.0).abs() < 1e-10),
        other => panic!("Expected F64, got {:?}", other),
    }
}

#[test]
fn test_mandelbrot_grid_assignment_index_api_regression() {
    let src = r#"
width = 5
height = 5
xmin = -2.0; xmax = 1.0
ymin = -1.2; ymax = 1.2

xs = range(xmin, xmax; length=width)
ys = range(ymax, ymin; length=height)

grid = xs' .+ im .* ys
grid[1, 1].re
"#;

    let result = compile_and_run_value(src, 0).expect("Failed to run assigned Mandelbrot index");
    match result {
        crate::vm::Value::F64(value) => assert!((value + 2.0).abs() < 1e-10),
        other => panic!("Expected F64, got {:?}", other),
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

    let result = compile_and_run_value(src, 0).expect("Failed to run Mandelbrot grid test");

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

    // Issue #3908: route the native-array destructure through the shared
    // `native_array_value_ref` helper instead of pattern-matching
    // the legacy native-array variant directly. The early-return panic
    // preserves the original "Expected Array, got ..." diagnostic when
    // `result` is not the native array carrier.
    let arr_owned = crate::vm::value::array_wrapper_value_to_array_value(&result, &[])
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("Expected Array, got {:?}", result));
    {
        let arr = &arr_owned;

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
                    if let Some((re, im)) = v.as_complex_parts() {
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
                        if let Some((re, im)) = v.as_complex_parts() {
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
                            println!("FAIL: [{}, {}] is not a complex number: {:?}", row, col, v);
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
                let (re, im) = v.as_complex_parts().unwrap_or_else(|| {
                    panic!("[{}, {}] is not a complex number: {:?}", row, col, v)
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
}
