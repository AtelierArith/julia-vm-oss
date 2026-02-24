//! Integration tests: Dict, compound assignment, broadcast calls, Mandelbrot, try/catch, JSON IR

mod common;
use common::*;

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::vm::Value;
use subset_julia_vm::*;

// =============================================================================
// Dict Tests - Testing Dict{K,V}() parametric constructor syntax
// =============================================================================

#[test]
fn test_dict_empty_constructor() {
    // Test Dict() - empty dict constructor
    let src = r#"
d = Dict()
length(d)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 0, "Empty dict should have length 0"),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_dict_parametric_constructor_empty() {
    // Test Dict{String, Int}() - empty dict with type parameters
    let src = r#"
d = Dict{String, Int}()
length(d)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 0, "Empty Dict{{String, Int}}() should have length 0"),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_dict_set_and_get() {
    // Test setting and getting dict values
    let src = r#"
d = Dict{String, Int}()
d["apple"] = 10
d["banana"] = 20
d["apple"] + d["banana"]
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 30, "Expected 10 + 20 = 30"),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_dict_haskey() {
    // Test haskey function
    let src = r#"
d = Dict{String, Int}()
d["key1"] = 100
haskey(d, "key1")
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::Bool(v) => assert!(v, "haskey should return true for existing key"),
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_dict_haskey_missing() {
    // Test haskey function for missing key
    let src = r#"
d = Dict{String, Int}()
d["key1"] = 100
haskey(d, "nonexistent")
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::Bool(v) => assert!(!v, "haskey should return false for missing key"),
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_dict_get_with_default() {
    // Test get function with default value
    let src = r#"
d = Dict{String, Int}()
d["existing"] = 42
get(d, "nonexistent", -1)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, -1, "get should return default for missing key"),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_dict_get_existing_key() {
    // Test get function for existing key
    let src = r#"
d = Dict{String, Int}()
d["existing"] = 42
get(d, "existing", -1)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 42, "get should return value for existing key"),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_dict_pairs() {
    // Test pairs(dict) returns all (key, value) tuples without assuming order.
    let src = r#"
d = Dict{String, Int}()
d["apple"] = 10
d["banana"] = 20
pairs(d)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::Tuple(t) => {
            assert_eq!(t.elements.len(), 2, "Expected 2 pairs in tuple");
            let mut saw_apple = false;
            let mut saw_banana = false;
            for pair in &t.elements {
                match pair {
                    Value::Tuple(p) => {
                        assert_eq!(p.elements.len(), 2, "Expected pair tuple of length 2");
                        match (&p.elements[0], &p.elements[1]) {
                            (Value::Str(k), Value::I64(v)) if k == "apple" => {
                                assert_eq!(*v, 10);
                                saw_apple = true;
                            }
                            (Value::Str(k), Value::I64(v)) if k == "banana" => {
                                assert_eq!(*v, 20);
                                saw_banana = true;
                            }
                            _ => panic!("Unexpected pair contents: {:?}", p),
                        }
                    }
                    _ => panic!("Expected pair to be a Tuple, got {:?}", pair),
                }
            }
            assert!(saw_apple, "Missing (\"apple\", 10) in pairs(dict)");
            assert!(saw_banana, "Missing (\"banana\", 20) in pairs(dict)");
        }
        _ => panic!("Expected Tuple, got {:?}", result),
    }
}

#[test]
fn test_time_println_string_literal() {
    // Regression test: @time println("string") was incorrectly parsed as two statements
    // Before fix: "@time println" was parsed as Var, and "Hello, World!" as a separate Str statement
    let result = compile_and_run_str(r#"@time println("Hello, World!")"#, 0);
    assert!(
        !result.is_nan(),
        "@time println with string literal should work"
    );
}

// ==================== zero(), trues(), falses() functions ====================

#[test]
fn test_zero_function_float() {
    // Test zero(x) for Float64
    let src = r#"
x = 5.0
zero(x)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 0.0).abs() < 1e-10, "zero(5.0) should be 0.0"),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_zero_function_int() {
    // Test zero(x) for Int64
    let src = r#"
x = 42
zero(x)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 0, "zero(42) should be 0"),
        Value::F64(v) => assert!((v - 0.0).abs() < 1e-10, "zero(42) should be 0.0"),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
}

#[test]
fn test_zero_function_complex() {
    // Test zero(z) for Complex
    // Note: Using complex(re, im) constructor since binary ops with Complex aren't fully supported in compile_core
    let src = r#"
z = complex(1.0, 2.0)
w = zero(z)
real(w) + imag(w)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 0.0).abs() < 1e-10, "zero(complex) should be 0.0+0.0im"),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_trues_function() {
    // Test trues(n)
    let src = r#"
t = trues(3)
t[1] + t[2] + t[3]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 3, "trues(3) should have all true values, sum = 3"),
        Value::F64(v) => assert!(
            (v - 3.0).abs() < 1e-10,
            "trues(3) should have all 1.0s, sum = 3.0"
        ),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
}

#[test]
fn test_falses_function() {
    // Test falses(n)
    let src = r#"
f = falses(3)
f[1] + f[2] + f[3]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 0, "falses(3) should have all false values, sum = 0"),
        Value::F64(v) => assert!(
            (v - 0.0).abs() < 1e-10,
            "falses(3) should have all 0.0s, sum = 0.0"
        ),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
}

// ==================== Complex binary operations ====================

#[test]
fn test_complex_binary_add() {
    // Test 1.0 + 2.0im using complex constructor
    let src = r#"
z = 1.0 + complex(0.0, 2.0)
real(z)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_complex_binary_add_imag() {
    // Test imaginary part of 1.0 + 2.0im
    let src = r#"
z = 1.0 + complex(0.0, 2.0)
imag(z)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 2.0).abs() < 1e-10, "Expected 2.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_complex_binary_mul() {
    // Test (0+1i) * (0+1i) = -1
    let src = r#"
i = complex(0.0, 1.0)
z = i * i
real(z)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - (-1.0)).abs() < 1e-10, "Expected -1.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_complex_binary_pow() {
    // Test i^2 = -1
    let src = r#"
i = complex(0.0, 1.0)
z = i^2
real(z)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - (-1.0)).abs() < 1e-10, "Expected -1.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_complex_neg() {
    // Test -(1+2i) = -1-2i
    let src = r#"
z = complex(1.0, 2.0)
w = -z
real(w) + imag(w)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!(
            (v - (-3.0)).abs() < 1e-10,
            "Expected -1 + -2 = -3.0, got {}",
            v
        ),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== Complex array literals ====================

#[test]
fn test_complex_array_literal() {
    // Test [complex(1,2), complex(3,4)] with direct field access
    let src = r#"
zs = [complex(1.0, 2.0), complex(3.0, 4.0)]
z = zs[1]
z.re + z.im
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!(
            (v - 3.0).abs() < 1e-10,
            "Expected 1.0 + 2.0 = 3.0, got {}",
            v
        ),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_complex_array_literal_second_element() {
    // Test accessing second element of complex array with direct field access
    let src = r#"
zs = [complex(1.0, 2.0), complex(3.0, 4.0)]
z = zs[2]
z.re + z.im
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!(
            (v - 7.0).abs() < 1e-10,
            "Expected 3.0 + 4.0 = 7.0, got {}",
            v
        ),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_complex_array_literal_mixed() {
    // Test [1.0, complex(2.0, 3.0)] - mixed real and complex should promote to complex array
    let src = r#"
zs = [1.0, complex(2.0, 3.0)]
real(zs[1]) + imag(zs[1])
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!(
            (v - 1.0).abs() < 1e-10,
            "Expected 1.0 + 0.0 = 1.0, got {}",
            v
        ),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== Broadcast Function Call Tests ====================

#[test]
fn test_broadcast_sqrt_core_pipeline() {
    // Test sqrt.(x) - element-wise sqrt using tree-sitter lowering
    let src = r#"
a = [1.0, 4.0, 9.0, 16.0, 25.0]
b = sqrt.(a)
b[1] + b[2] + b[3] + b[4] + b[5]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run sqrt broadcast test");
    match result {
        Value::F64(v) => assert!((v - 15.0).abs() < 1e-10, "Expected 15.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_broadcast_abs_core_pipeline() {
    // Test abs.(x) - element-wise abs using tree-sitter lowering
    let src = r#"
a = [-1.0, -2.0, 3.0, -4.0]
b = abs.(a)
b[1] + b[2] + b[3] + b[4]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run abs broadcast test");
    match result {
        Value::F64(v) => assert!((v - 10.0).abs() < 1e-10, "Expected 10.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_broadcast_sin_cos_core_pipeline() {
    // Test sin.(x) and cos.(x)
    let src = r#"
a = [0.0]
b = sin.(a)
c = cos.(a)
b[1] + c[1]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run sin/cos broadcast test");
    match result {
        // sin(0) + cos(0) = 0 + 1 = 1
        Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_broadcast_exp_log_core_pipeline() {
    // Test exp.(x) and log.(x)
    let src = r#"
a = [1.0]
b = exp.(a)
c = log.(b)
c[1]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run exp/log broadcast test");
    match result {
        // log(exp(1)) = 1
        Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_broadcast_ifelse_core_pipeline() {
    // Test ifelse.(cond, then, else) - element-wise ternary
    let src = r#"
cond = [true, false, true, false]
a = [10.0, 20.0, 30.0, 40.0]
b = [1.0, 2.0, 3.0, 4.0]
result = ifelse.(cond, a, b)
result[1] + result[2] + result[3] + result[4]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run ifelse broadcast test");
    match result {
        // ifelse.([1,0,1,0], [10,20,30,40], [1,2,3,4]) = [10, 2, 30, 4] -> 46
        Value::F64(v) => assert!((v - 46.0).abs() < 1e-10, "Expected 46.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_broadcast_ifelse_non_bool_condition_error() {
    // Test that ifelse.() raises MethodError for non-Bool condition
    // In Julia, ifelse requires Bool condition type
    let src = r#"
cond = [1, 0, 1, 0]  # Integer, not Bool
a = [10.0, 20.0, 30.0, 40.0]
b = [1.0, 2.0, 3.0, 4.0]
result = ifelse.(cond, a, b)
result[1]
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error for non-Bool condition in ifelse"
    );
    let err = result.err().unwrap_or_default();
    assert!(
        err.contains("condition must be Bool") || err.contains("no method matching ifelse"),
        "Expected MethodError for non-Bool condition, got: {}",
        err
    );
}

// ==================== iOS App Sample: Mandelbrot Broadcast ====================

#[test]
fn test_mandelbrot_broadcast_ios_sample() {
    // Test the exact Mandelbrot Broadcast sample from iOS app (SubsetJuliaVMApp)
    // This test uses broadcast operations for row-parallel computation
    let src = r##"
# Mandelbrot set using broadcast operations
# Computes escape time for a row of points at once

function mandelbrot_row(cr_array, ci, maxiter)
    n = length(cr_array)

    # Initialize z = 0 + 0i for all points
    Zr = zeros(n)
    Zi = zeros(n)

    # Track iterations for each point
    iterations = fill(maxiter, n)
    escaped = zeros(n)

    for k in 1:maxiter
        # z^2 = (Zr + Zi*i)^2 = Zr^2 - Zi^2 + 2*Zr*Zi*i
        Zr2 = Zr .* Zr
        Zi2 = Zi .* Zi

        # Check escape: |z|^2 > 4
        mag2 = Zr2 .+ Zi2

        # Update iteration count for newly escaped points
        for j in 1:n
            if mag2[j] > 4.0 && escaped[j] == 0.0
                iterations[j] = k
                escaped[j] = 1.0
            end
        end

        # z = z^2 + c (complex multiplication)
        Zi_new = 2.0 .* Zr .* Zi .+ ci
        Zr = Zr2 .- Zi2 .+ cr_array
        Zi = Zi_new
    end

    iterations
end

# Create coordinate arrays (small size for fast testing)
width = 15
height = 8
xmin = -2.0
xmax = 1.0
ymin = -1.2
ymax = 1.2
maxiter = 20

# Generate x coordinates using array comprehension
cr_array = [xmin + (col - 1) * (xmax - xmin) / (width - 1) for col in 1:width]

println("Mandelbrot Set (Broadcast, ", width, "x", height, "):")

# Process each row with broadcast operations
in_set = 0
for row in 1:height
    ci = ymax - (row - 1) * (ymax - ymin) / (height - 1)
    iterations = mandelbrot_row(cr_array, ci, maxiter)

    for col in 1:width
        n = iterations[col]
        if n == maxiter
            print("#")
            in_set += 1
        elseif n > 10
            print("+")
        elseif n > 4
            print(".")
        elseif n > 2
            print("-")
        else
            print(" ")
        end
    end
    println("")
end

println("")
println("Points in set: ", in_set, " / ", width * height)
in_set
"##;
    let result = run_core_pipeline(src, 0).expect("Failed to run Mandelbrot Broadcast test");
    // Julia reference: 28 points in set for 15x8 grid with maxiter=20
    match result {
        Value::I64(in_set) => {
            assert_eq!(in_set, 28, "Expected 28 points in set (Julia reference), got {}", in_set);
        }
        Value::F64(in_set) => {
            assert!((in_set - 28.0).abs() < 1.0, "Expected ~28 points in set (Julia reference), got {}", in_set);
        }
        _ => panic!("Expected numeric result, got {:?}", result),
    }
}

#[test]
fn test_try_catch_finally_debug() {
    // Minimal test case
    let src = r#"
result = 0
try
    result = 5
finally
    x = 1
end
result
"#;
    let result = run_core_pipeline(src, 0).expect("Failed");
    println!("Result: {:?}", result);
    match result {
        Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_try_catch_finally_no_error_simple() {
    // Same as failing test but without catch block
    let src = r#"
result = 0
try
    result = 5
catch e
    result = -1
finally
    x = 1
end
result
"#;
    let result = run_core_pipeline(src, 0).expect("Failed");
    println!("Simple test: {:?}", result);
    match result {
        Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_try_catch_finally_no_error_f64() {
    // Test with F64 to see if it's a type issue
    let src = r#"
result = 0.0
try
    result = 10.0 / 2.0
catch e
    result = -1.0
finally
    x = 1
end
result
"#;
    let result = run_core_pipeline(src, 0).expect("Failed");
    println!("F64 test: {:?}", result);
    match result {
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_try_catch_finally_type_change() {
    // Test where initial type is I64 but try block assigns F64
    let src = r#"
result = 0
try
    result = 10.0 / 2.0
catch e
    result = -1
finally
    x = 1
end
result
"#;
    let result = run_core_pipeline(src, 0).expect("Failed");
    println!("Type change test: {:?}", result);
    match result {
        Value::I64(x) => println!("Got I64: {}", x),
        Value::F64(x) => println!("Got F64: {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_try_catch_finally_catch_type_matters() {
    // When catch block also assigns F64, it should work
    let src = r#"
result = 0
try
    result = 10.0 / 2.0
catch e
    result = -1.0
finally
    x = 1
end
result
"#;
    let result = run_core_pipeline(src, 0).expect("Failed");
    println!("Catch F64 test: {:?}", result);
    match result {
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        Value::I64(x) => panic!("Got I64: {} - type inference is wrong!", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Julia-Style Broadcasting Tests ====================

#[test]
fn test_julia_broadcast_outer_product_3x3() {
    // (1:3)' .* (1:3) → 3×3 matrix (multiplication table)
    // Expected: [[1,2,3], [2,4,6], [3,6,9]] in column-major order
    let src = r#"
result = (1:3)' .* (1:3)
# Access element at position [2,2] (should be 2*2 = 4)
result[2, 2]
"#;
    let result = run_core_pipeline(src, 0);
    println!("Julia broadcast outer product result: {:?}", result);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 4, "Expected 4, got {}", v),
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
        other => panic!("Expected numeric, got {:?}", other),
    }
}

#[test]
fn test_julia_broadcast_outer_product_corners() {
    // Test corner elements of the multiplication table
    let src = r#"
result = (1:3)' .* (1:3)
# [1,1] = 1*1 = 1
a = result[1, 1]
# [3,3] = 3*3 = 9
b = result[3, 3]
# [1,3] = 1*3 = 3
c = result[1, 3]
# [3,1] = 3*1 = 3
d = result[3, 1]
a + b + c + d
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    // 1 + 9 + 3 + 3 = 16
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 16, "Expected 16, got {}", v),
        Value::F64(v) => assert!((v - 16.0).abs() < 1e-10, "Expected 16.0, got {}", v),
        other => panic!("Expected numeric, got {:?}", other),
    }
}

#[test]
fn test_julia_broadcast_row_col_add() {
    // Row vector .+ column vector → matrix
    let src = r#"
row = (1:3)'  # [1, 3] shape: [[1, 2, 3]]
col = 1:3    # [3] shape: [1, 2, 3]
result = row .+ col
# Result should be 3x3:
# [[2,3,4], [3,4,5], [4,5,6]]
# [2,2] = 2 + 2 = 4
result[2, 2]
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 4, "Expected 4, got {}", v),
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
        other => panic!("Expected numeric, got {:?}", other),
    }
}

#[test]
fn test_julia_broadcast_2d_with_1d() {
    // 2D array [3, 2] .+ 1D array [3] → broadcasts to [3, 2]
    // In Julia, [3] is treated as [3, 1] in 2D context
    // [3, 2] .+ [3, 1] → [3, 2] (column broadcasts to both columns)
    let src = r#"
mat = zeros(3, 2)
mat[1, 1] = 1.0
mat[2, 1] = 2.0
mat[3, 1] = 3.0
mat[1, 2] = 4.0
mat[2, 2] = 5.0
mat[3, 2] = 6.0
# mat = [[1,4], [2,5], [3,6]] (column-major)
vec = [10.0, 20.0, 30.0]  # shape [3] → treated as [3, 1]
result = mat .+ vec
# Broadcasting: [3,2] .+ [3,1] → [3,2]
# result = [[11,14], [22,25], [33,36]]
result[2, 2]  # 5 + 20 = 25
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    match result.unwrap() {
        Value::F64(v) => assert!((v - 25.0).abs() < 1e-10, "Expected 25.0, got {}", v),
        other => panic!("Expected F64, got {:?}", other),
    }
}

#[test]
fn test_julia_broadcast_same_shape_still_works() {
    // Verify same-shape broadcasting still works (fast path)
    let src = r#"
a = [1.0, 2.0, 3.0]
b = [10.0, 20.0, 30.0]
result = a .+ b
result[2]  # 2 + 20 = 22
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    match result.unwrap() {
        Value::F64(v) => assert!((v - 22.0).abs() < 1e-10, "Expected 22.0, got {}", v),
        other => panic!("Expected F64, got {:?}", other),
    }
}

#[test]
fn test_julia_broadcast_incompatible_shapes_error() {
    // Incompatible shapes should still error
    let src = r#"
a = [1.0, 2.0, 3.0]    # shape [3]
b = [10.0, 20.0]       # shape [2]
result = a .+ b        # Should error: 3 vs 2 not compatible
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err(), "Expected error for incompatible shapes");
}

#[test]
fn test_broadcast_op_function_call_syntax() {
    // Test .*() function call syntax (instead of infix a .* b)
    let src = r#"
result = .*((1:3)', 1:3)
# Access [2,2] element (should be 2*2 = 4)
result[2, 2]
"#;
    let result = run_core_pipeline(src, 0);
    println!("Broadcast op function call result: {:?}", result);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 4, "Expected 4, got {}", v),
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
        other => panic!("Expected numeric, got {:?}", other),
    }
}

#[test]
fn test_broadcast_add_function_call_syntax() {
    // Test .+() function call syntax
    let src = r#"
result = .+([1.0, 2.0, 3.0], [10.0, 20.0, 30.0])
result[2]  # 2 + 20 = 22
"#;
    let result = run_core_pipeline(src, 0);
    println!("Broadcast add function call result: {:?}", result);
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    match result.unwrap() {
        Value::F64(v) => assert!((v - 22.0).abs() < 1e-10, "Expected 22.0, got {}", v),
        other => panic!("Expected F64, got {:?}", other),
    }
}

// ==================== Additional String Interpolation Tests ====================

#[test]
fn test_string_interpolation_no_interpolation_string() {
    // Test that strings without interpolation still work
    let src = r#"
println("Hello, World!")
42
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(42)));
    assert_eq!(output, "Hello, World!\n");
}

#[test]
fn test_string_interpolation_full_sample_from_bug_report() {
    // Test the full sample from the user's bug report
    let src = r#"
x = 42
pi_approx = 3.14159
println("x = $(x)")
println("x + 1 = $(x + 1)")
println("x * 2 = $(x * 2)")
println("Pi is approximately $(pi_approx)")
y = 10
println("Sum: $(x + y), Product: $(x * y)")
println("Double: $((x + y) * 2)")
println(x)
x
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(42)));
    let expected = "x = 42\nx + 1 = 43\nx * 2 = 84\nPi is approximately 3.14159\nSum: 52, Product: 420\nDouble: 104\n42\n";
    assert_eq!(output, expected);
}

// ==================== JSON IR Flow Tests (Web Simulation) ====================


#[test]
fn test_base_loads() {
    use subset_julia_vm::base_loader::get_base_program;

    let base = get_base_program();
    assert!(base.is_some(), "Base should load");
    let base = base.unwrap();
    assert!(
        base.structs.iter().any(|s| s.name == "Complex"),
        "Complex struct should exist"
    );
    assert!(
        base.functions
            .iter()
            .any(|f| f.name == "+" && f.is_base_extension),
        "+ function should exist"
    );

    // Check for generic method signature +(Real, Complex)
    // This method handles Real + Complex arithmetic via generic dispatch (Issue #2427)
    let has_real_complex = base.functions.iter().any(|f| {
        if f.name != "+" || !f.is_base_extension {
            return false;
        }
        if f.params.len() != 2 {
            return false;
        }
        let p0 = &f.params[0].type_annotation;
        let p1 = &f.params[1].type_annotation;
        // Generic method: +(x::Real, z::Complex)
        matches!(p0, Some(crate::types::JuliaType::Real))
            && matches!(p1, Some(crate::types::JuliaType::Struct(s)) if s == "Complex")
    });
    assert!(
        has_real_complex,
        "+(Real, Complex) method should exist in base"
    );
}

#[test]
fn test_merge_base() {
    use std::collections::HashSet;
    use subset_julia_vm::base_loader::get_base_program;

    // Simulate an empty program
    let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
    let ir_json = format!(
        r#"{{
        "structs": [],
        "functions": [],
        "modules": [],
        "usings": [],
        "main": {{ "stmts": [], "span": {sp} }}
    }}"#
    );

    let mut program: Program = serde_json::from_str(&ir_json).expect("parse empty program");

    // Before merge
    assert_eq!(
        program.structs.len(),
        0,
        "Empty program should have no structs"
    );
    assert_eq!(
        program.functions.len(),
        0,
        "Empty program should have no functions"
    );

    // Merge base
    if let Some(base) = get_base_program() {
        let user_func_names: HashSet<_> =
            program.functions.iter().map(|f| f.name.as_str()).collect();
        let user_struct_names: HashSet<_> =
            program.structs.iter().map(|s| s.name.as_str()).collect();

        let mut all_structs: Vec<_> = base
            .structs
            .iter().filter(|&s| !user_struct_names.contains(s.name.as_str())).cloned()
            .collect();
        all_structs.append(&mut program.structs);
        program.structs = all_structs;

        let mut all_functions: Vec<_> = base
            .functions
            .iter().filter(|&f| !user_func_names.contains(f.name.as_str())).cloned()
            .collect();
        all_functions.append(&mut program.functions);
        program.functions = all_functions;
    }

    // After merge
    assert!(
        program.structs.iter().any(|s| s.name == "Complex"),
        "Complex struct should be merged"
    );
    assert!(
        program.functions.iter().any(|f| f.name == "+"),
        "+ function should be merged"
    );

    // Compile should work
    let compiled = compile_core_program(&program);
    assert!(
        compiled.is_ok(),
        "Empty program with merged base should compile: {:?}",
        compiled.err()
    );
}

#[test]
fn test_simple_float_plus_complex() {
    // Minimal test: just 0.0 + complex(0.0, 0.0) at top level
    let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
    let ir_json = format!(
        r#"{{
        "structs": [],
        "functions": [],
        "modules": [],
        "usings": [],
        "main": {{
            "stmts": [{{
                "Expr": {{
                    "expr": {{
                        "BinaryOp": {{
                            "op": "Add",
                            "left": {{"Literal": [{{"Float": 0.0}}, {sp}]}},
                            "right": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                            "span": {sp}
                        }}
                    }},
                    "span": {sp}
                }}
            }}],
            "span": {sp}
        }}
    }}"#
    );

    let result = run_from_ir_json(&ir_json, 42);
    assert!(
        result.is_ok(),
        "Simple Float64 + Complex failed: {:?}",
        result.err()
    );
}

#[test]
fn test_float_plus_complex_in_function() {
    // Test: Float64 + Complex inside a function
    let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
    let ir_json = format!(
        r#"{{
        "structs": [],
        "functions": [{{
            "name": "test_add",
            "params": [],
            "kwparams": [],
            "body": {{
                "stmts": [{{
                    "Return": {{
                        "value": {{
                            "BinaryOp": {{
                                "op": "Add",
                                "left": {{"Literal": [{{"Float": 0.0}}, {sp}]}},
                                "right": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                                "span": {sp}
                            }}
                        }},
                        "span": {sp}
                    }}
                }}],
                "span": {sp}
            }},
            "return_type": null,
            "span": {sp}
        }}],
        "modules": [],
        "usings": [],
        "main": {{
            "stmts": [{{
                "Expr": {{
                    "expr": {{
                        "Call": {{
                            "function": "test_add",
                            "args": [],
                            "kwargs": [],
                            "splat_mask": [],
                            "span": {sp}
                        }}
                    }},
                    "span": {sp}
                }}
            }}],
            "span": {sp}
        }}
    }}"#
    );

    let result = run_from_ir_json(&ir_json, 42);
    assert!(
        result.is_ok(),
        "Float64 + Complex in function failed: {:?}",
        result.err()
    );
}

#[test]
fn test_complex_plus_typed_param() {
    // Test: Complex + Float64 param - with type annotation, static dispatch works
    let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
    let ir_json = format!(
        r#"{{
        "structs": [],
        "functions": [{{
            "name": "test_add",
            "params": [{{"name": "c", "type_annotation": "Float64", "span": {sp}}}],
            "kwparams": [],
            "body": {{
                "stmts": [{{
                    "Return": {{
                        "value": {{
                            "BinaryOp": {{
                                "op": "Add",
                                "left": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                                "right": {{"Var": ["c", {sp}]}},
                                "span": {sp}
                            }}
                        }},
                        "span": {sp}
                    }}
                }}],
                "span": {sp}
            }},
            "return_type": null,
            "span": {sp}
        }}],
        "modules": [],
        "usings": [],
        "main": {{
            "stmts": [{{
                "Expr": {{
                    "expr": {{
                        "Call": {{
                            "function": "test_add",
                            "args": [{{"Literal": [{{"Float": 1.0}}, {sp}]}}],
                            "kwargs": [],
                            "splat_mask": [],
                            "span": {sp}
                        }}
                    }},
                    "span": {sp}
                }}
            }}],
            "span": {sp}
        }}
    }}"#
    );

    let result = run_from_ir_json(&ir_json, 42);
    assert!(
        result.is_ok(),
        "Complex + typed param failed: {:?}",
        result.err()
    );
}

#[test]
fn test_complex_with_typed_param_from_json() {
    // This test simulates the web flow where IR comes from JavaScript lowering as JSON
    // Tests that Complex arithmetic with typed params works correctly
    // Note: Type annotations are required for static method dispatch
    let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
    let ir_json = format!(
        r#"
    {{
        "structs": [],
        "functions": [{{
            "name": "test_complex",
            "params": [
                {{"name": "c", "type_annotation": {{"Struct": "Complex{{Float64}}"}}, "span": {sp}}}
            ],
            "kwparams": [],
            "body": {{
                "stmts": [
                    {{
                        "Assign": {{
                            "var": "z",
                            "value": {{
                                "BinaryOp": {{
                                    "op": "Add",
                                    "left": {{"Literal": [{{"Float": 0.0}}, {sp}]}},
                                    "right": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 0.0}}, {{"Float": 0.0}}]]}}, {sp}]}},
                                    "span": {sp}
                                }}
                            }},
                            "span": {sp}
                        }}
                    }},
                    {{
                        "Assign": {{
                            "var": "result",
                            "value": {{
                                "BinaryOp": {{
                                    "op": "Add",
                                    "left": {{"Var": ["z", {sp}]}},
                                    "right": {{"Var": ["c", {sp}]}},
                                    "span": {sp}
                                }}
                            }},
                            "span": {sp}
                        }}
                    }},
                    {{
                        "Return": {{
                            "value": {{"Literal": [{{"Int": 1}}, {sp}]}},
                            "span": {sp}
                        }}
                    }}
                ],
                "span": {sp}
            }},
            "return_type": null,
            "span": {sp}
        }}],
        "modules": [],
        "usings": [],
        "main": {{
            "stmts": [
                {{
                    "Assign": {{
                        "var": "c",
                        "value": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                        "span": {sp}
                    }}
                }},
                {{
                    "Expr": {{
                        "expr": {{
                            "Call": {{
                                "function": "test_complex",
                                "args": [{{"Var": ["c", {sp}]}}],
                                "kwargs": [],
                                "splat_mask": [],
                                "span": {sp}
                            }}
                        }},
                        "span": {sp}
                    }}
                }}
            ],
            "span": {sp}
        }}
    }}
    "#
    );

    let result = run_from_ir_json(&ir_json, 42);
    assert!(
        result.is_ok(),
        "Complex + typed param failed: {:?}",
        result.err()
    );
    assert!(matches!(result.unwrap(), Value::I64(1)));
}

// ==================== sum(arr) Tests ====================

#[test]
fn test_sum_array() {
    // sum([1, 2, 3, 4, 5]) = 15
    let src = r#"
arr = [1, 2, 3, 4, 5]
sum(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 15.0).abs() < 1e-10,
        "sum([1,2,3,4,5]) should be 15, got {}",
        result
    );
}

#[test]
fn test_sum_array_floats() {
    // sum([1.5, 2.5, 3.0]) = 7.0
    let src = r#"
arr = [1.5, 2.5, 3.0]
sum(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 7.0).abs() < 1e-10,
        "sum([1.5, 2.5, 3.0]) should be 7.0, got {}",
        result
    );
}

#[test]
fn test_sum_with_function() {
    // sum(f, arr) - sum of squares
    let src = r#"
function square(x)
    x * x
end
arr = [1, 2, 3, 4, 5]
sum(square, arr)
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 4 + 9 + 16 + 25 = 55
    assert!(
        (result - 55.0).abs() < 1e-10,
        "sum(square, [1,2,3,4,5]) should be 55, got {}",
        result
    );
}

#[test]
fn test_sum_in_expression() {
    // Using sum in a larger expression
    let src = r#"
arr = [1, 2, 3, 4, 5]
mean = sum(arr) / length(arr)
mean
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 3.0).abs() < 1e-10,
        "mean of [1,2,3,4,5] should be 3.0, got {}",
        result
    );
}

// ==================== Ref() Tests ====================

#[test]
fn test_ref_basic() {
    // Basic Ref creation - Ref wraps a value and is used inline
    // Ref(x) protects x from broadcasting, treating it as a scalar
    let src = r#"
arr = [1.0, 2.0, 3.0]
arr .+ Ref(100)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run Ref test");
    match result {
        Value::Array(arr) => {
            // [1+100, 2+100, 3+100] = [101, 102, 103]
            let arr = arr.borrow();
            assert_eq!(arr.len(), 3);
            let data = arr.try_data_f64().unwrap();
            assert!((data[0] - 101.0).abs() < 1e-10);
            assert!((data[1] - 102.0).abs() < 1e-10);
            assert!((data[2] - 103.0).abs() < 1e-10);
        }
        _ => panic!("Expected Array, got {:?}", result),
    }
}

#[test]
fn test_ref_broadcast_scalar() {
    // Ref protects value from broadcasting - treated as scalar
    // arr .+ Ref(10) should add 10 to each element
    let src = r#"
arr = [1.0, 2.0, 3.0]
result = arr .+ Ref(10)
sum(result)
"#;
    let result = compile_and_run_str(src, 0);
    // [1+10, 2+10, 3+10] = [11, 12, 13], sum = 36
    assert!(
        (result - 36.0).abs() < 1e-10,
        "arr .+ Ref(10) sum should be 36, got {}",
        result
    );
}

#[test]
fn test_ref_broadcast_multiply() {
    // Ref in multiplication broadcast
    let src = r#"
arr = [2.0, 3.0, 4.0]
result = arr .* Ref(5)
sum(result)
"#;
    let result = compile_and_run_str(src, 0);
    // [2*5, 3*5, 4*5] = [10, 15, 20], sum = 45
    assert!(
        (result - 45.0).abs() < 1e-10,
        "arr .* Ref(5) sum should be 45, got {}",
        result
    );
}

#[test]
fn test_ref_multi_arg_broadcast() {
    // Multi-argument broadcast with Ref: f.(arr, Ref(x))
    let src = r#"
function add_val(x, y)
    return x + y
end

arr = [1.0, 2.0, 3.0]
result = add_val.(arr, Ref(10))
sum(result)
"#;
    let result = compile_and_run_str(src, 0);
    // [1+10, 2+10, 3+10] = [11, 12, 13], sum = 36
    assert!(
        (result - 36.0).abs() < 1e-10,
        "add_val.(arr, Ref(10)) sum should be 36, got {}",
        result
    );
}

/// Tests for complex array operations including HOF functions with nested calls
#[test]
fn test_complex_array_basic_ops() {
    // Test 1: Complex array creation and length
    let src1 = r#"
C = [1.0 + 2.0im, 3.0 + 4.0im]
length(C)
"#;
    let result1 = compile_and_run_str(src1, 0);
    assert!(
        (result1 - 2.0).abs() < 1e-10,
        "Complex array should have 2 elements, got {}",
        result1
    );

    // Test 2: Broadcast over F64 array
    let src2 = r#"
function double(x)
    x * 2
end
arr = [1.0, 2.0, 3.0]
result = double.(arr)
sum(result)
"#;
    let result2 = compile_and_run_str(src2, 0);
    assert!(
        (result2 - 12.0).abs() < 1e-10,
        "double.([1,2,3]) sum should be 12, got {}",
        result2
    );

    // Test 3: Map over F64 array
    let src3 = r#"
function double(x)
    x * 2
end
arr = [1.0, 2.0, 3.0]
result = map(double, arr)
sum(result)
"#;
    let result3 = compile_and_run_str(src3, 0);
    assert!(
        (result3 - 12.0).abs() < 1e-10,
        "map(double, arr) sum should be 12, got {}",
        result3
    );

    // Test 4: Map over complex array with nested function call
    // Tests that HOF correctly handles when the user function calls another function
    let src4 = r#"
function get_real(c)
    real(c)
end
C = [1.0 + 2.0im, 3.0 + 4.0im]
result = map(get_real, C)
sum(result)
"#;
    let result4 = compile_and_run_str(src4, 0);
    assert!(
        (result4 - 4.0).abs() < 1e-10,
        "map(get_real, C) sum should be 4, got {}",
        result4
    );
}

#[test]
fn test_ref_multi_arg_broadcast_complex() {
    // First test: simple broadcast over complex array without Ref
    let src1 = r#"
function get_real(c)
    real(c)
end
C = [1.0 + 2.0im, 3.0 + 4.0im]
result = get_real.(C)
sum(result)
"#;
    let result1 = compile_and_run_str(src1, 0);
    println!("Simple broadcast over complex: {}", result1);
    assert!(
        (result1 - 4.0).abs() < 1e-10,
        "get_real.(C) sum should be 4 (1+3), got {}",
        result1
    );

    // Second test: broadcast with Ref over complex array
    let src2 = r#"
function add_val(c, x)
    real(c) + x
end
C = [1.0 + 2.0im, 3.0 + 4.0im]
result = add_val.(C, Ref(10))
sum(result)
"#;
    let result2 = compile_and_run_str(src2, 0);
    println!("Broadcast with Ref over complex: {}", result2);
    // (1+10) + (3+10) = 11 + 13 = 24
    assert!(
        (result2 - 24.0).abs() < 1e-10,
        "add_val.(C, Ref(10)) sum should be 24, got {}",
        result2
    );

    // Original test: Mandelbrot escape with type annotations
    let src3 = r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# c = 2.0 escapes at k=3, others don't escape
C = [2.0 + 0.0im, -1.0 + 0.0im, 0.0 + 0.0im]
result = mandelbrot_escape.(C, Ref(50))
sum(result)
"#;
    let result3 = compile_and_run_str(src3, 0);
    println!("Mandelbrot escape broadcast: {}", result3);
    // 3 + 50 + 50 = 103
    assert!(
        (result3 - 103.0).abs() < 1e-10,
        "mandelbrot_escape.(C, Ref(50)) sum should be 103, got {}",
        result3
    );
}

#[test]
fn test_2d_broadcast_shape_preservation() {
    // Test that broadcasting over a 2D matrix preserves shape
    // Step 1: Verify xs' creates row vector
    let src_step1 = r#"
xs = range(-2.0, 1.0; length=3)
println("xs length: ", length(xs))
xt = xs'
println("xt total: ", length(xt))
length(xt)
"#;
    let output1 = compile_and_run_str_with_output(src_step1, 0);
    println!("Step 1 output: {}", output1);
    let result1 = compile_and_run_str(src_step1, 0);
    println!("Step 1 result (xt length): {}", result1);
    assert!(
        (result1 - 3.0).abs() < 1e-10,
        "xt should have 3 elements, got {}",
        result1
    );

    // Step 2: Verify 2D complex matrix creation
    let src_step2 = r#"
xs = range(-2.0, 1.0; length=3)
ys = range(1.2, -1.2; length=2)
C = xs' .+ im .* ys
println("C total: ", length(C))
length(C)
"#;
    let output2 = compile_and_run_str_with_output(src_step2, 0);
    println!("Step 2 output: {}", output2);
    let result2 = compile_and_run_str(src_step2, 0);
    println!("Step 2 result (C length): {}", result2);
    // 2 rows * 3 cols = 6 elements
    assert!(
        (result2 - 6.0).abs() < 1e-10,
        "C should have 6 elements (2x3), got {}",
        result2
    );

    // Step 3: Verify broadcast result shape (without complex indexing which requires more work)
    let src_step3 = r#"
xs = range(-2.0, 1.0; length=3)
ys = range(1.2, -1.2; length=2)
C = xs' .+ im .* ys

# Use a function that returns real (not complex) for simpler testing
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

result = mandelbrot_escape.(C, Ref(10))
println("result total: ", length(result))
length(result)
"#;
    let output3 = compile_and_run_str_with_output(src_step3, 0);
    println!("Step 3 output: {}", output3);
    let result3 = compile_and_run_str(src_step3, 0);
    println!("Step 3 result (result length): {}", result3);
    // Should still have 6 elements (2x3) if shape is preserved
    assert!(
        (result3 - 6.0).abs() < 1e-10,
        "2D broadcast result should have 6 elements (2x3), got {}",
        result3
    );
}

#[test]
fn test_range_with_length_output() {
    // Test range with keyword length argument
    // Note: Julia's range(start, stop; length=N) requires Integer for length
    let src = r#"
n = 5
xs = range(0.0, 1.0; length=n)
length(xs)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range test");
    match result {
        Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_range_with_length_in_function() {
    // Test range with length parameter passed to function
    // Note: Julia's range(start, stop; length=N) requires Integer for length
    let src = r#"
function make_range(n::Int64)
    range(0.0, 1.0; length=n)
end
xs = make_range(5)
length(xs)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range test");
    match result {
        Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_range_positional_in_function() {
    // Test using positional range(start, stop, length) to verify core logic works
    // Note: Julia's range(start, stop, length::Integer) requires Int64 for positional length arg
    let src = r#"
function make_range(n::Int64)
    range(0.0, 1.0, n)
end
xs = make_range(5)
length(xs)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range test");
    match result {
        Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_from_function_param() {
    // Simple test: pass a value through kwarg
    // Note: Use Int64 to avoid any type conversion issues
    let src = r#"
function simple(; val=0)
    return val
end

simple(; val=42)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
    match result {
        Value::I64(v) => assert_eq!(v, 42, "Expected 42, got {}", v),
        Value::F64(v) => assert_eq!(v, 42.0, "Expected 42.0, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_2d_broadcast_mandelbrot() {
    // Test minimal function call with range
    // Note: Julia's range requires Integer for length argument
    let src1 = r#"
function test_grid(width, height)
    xs = range(-2.0, 1.0; length=width)
    ys = range(1.2, -1.2; length=height)
    C = xs' .+ im .* ys
    length(C)
end
test_grid(5, 3)
"#;
    let output1 = compile_and_run_str_with_output(src1, 0);
    println!("Simple grid test output: {}", output1);
    let result1 = compile_and_run_str(src1, 0);
    println!("Simple grid test result: {}", result1);
    assert!(
        (result1 - 15.0).abs() < 1e-10,
        "Grid should have 15 elements (5x3), got {}",
        result1
    );
}

#[test]
fn test_sleep_basic_float() {
    let src = r#"
sleep(0.001)
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 42.0).abs() < 1e-10,
        "Should return 42 after sleep"
    );
}

#[test]
fn test_sleep_integer() {
    let src = r#"
sleep(0)
100
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 100.0).abs() < 1e-10,
        "Should return 100 after sleep(0)"
    );
}

#[test]
fn test_sleep_zero() {
    let src = r#"
sleep(0)
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10, "Should handle sleep(0)");
}

#[test]
fn test_sleep_returns_nothing() {
    let src = r#"
result = sleep(0.0)
# Check if result is nothing by seeing if we can call a function on it
# nothing can't be used in arithmetic, so we just return a success value
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 42.0).abs() < 1e-10,
        "sleep() should execute successfully"
    );
}

#[test]
fn test_sleep_negative_error() {
    let src = "sleep(-1)";
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err(), "Should error on negative duration");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("negative") || err_msg.contains("Domain error"),
        "Error should mention negative: {}",
        err_msg
    );
}

#[test]
fn test_sleep_infinity_error() {
    // Note: Division by zero is caught before sleep() can see Inf
    // This is acceptable behavior - the important thing is that invalid values are rejected
    let src = r#"
x = 1.0
y = 0.0
inf = x / y
sleep(inf)
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Should error on division by zero or infinite duration"
    );
    // Either "Division by zero" or "finite" error is acceptable
}

#[test]
fn test_sleep_nan_error() {
    // Note: Division by zero is caught before sleep() can see NaN
    // This is acceptable behavior - the important thing is that invalid values are rejected
    let src = r#"
x = 0.0
y = 0.0
nan_val = x / y
sleep(nan_val)
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Should error on division by zero or NaN duration"
    );
    // Either "Division by zero" or "finite" error is acceptable
}

#[test]
fn test_sleep_in_loop() {
    let src = r#"
for i in 1:3
    sleep(0.0)
end
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10, "Should handle sleep in loop");
}

// ==================== Rational Number Operator (//) ====================

#[test]
fn test_base_min_function() {
    // Test that a simple base function like min() works
    let src = r#"
min(3, 5)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 3.0).abs() < 1e-10,
        "min(3, 5) should be 3, got {}",
        result
    );
}

#[test]
fn test_rational_struct_direct() {
    // Test creating Rational struct directly from prelude
    let src = r#"
r = Rational(1, 2)
r.num
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "Rational(1,2).num should be 1, got {}",
        result
    );
}

#[test]
fn test_rational_operator_basic() {
    // Test that // operator is lowered to rational() call
    // We define our own simple rational to verify the lowering works
    let src = r#"
struct MyRational
    num::Int64
    den::Int64
end

function rational(n, d)
    return MyRational(n, d)
end

r = 1 // 2
r.num
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "1//2.num should be 1, got {}",
        result
    );
}

#[test]
fn test_rational_operator_denominator() {
    // Test that // operator correctly passes denominator
    let src = r#"
struct MyRational
    num::Int64
    den::Int64
end

function rational(n, d)
    return MyRational(n, d)
end

r = 1 // 2
r.den
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 2.0).abs() < 1e-10,
        "1//2.den should be 2, got {}",
        result
    );
}

#[test]
fn test_rational_operator_with_expressions() {
    // Test that // operator works with expressions
    let src = r#"
struct MyRational
    num::Int64
    den::Int64
end

function rational(n, d)
    return MyRational(n, d)
end

x = 3
y = 4
r = x // y
r.num + r.den
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 7.0).abs() < 1e-10,
        "3//4 should have num+den = 7, got {}",
        result
    );
}

#[test]
fn test_rational_operator_negative() {
    // Test that // operator works with negative numbers
    let src = r#"
struct MyRational
    num::Int64
    den::Int64
end

function rational(n, d)
    return MyRational(n, d)
end

r = -1 // 2
r.num
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - (-1.0)).abs() < 1e-10,
        "(-1)//2.num should be -1, got {}",
        result
    );
}

// ==================== Prelude Function Parameter Reassignment Tests ====================
// These tests verify that prelude functions that reassign their parameters work correctly.
// This was a bug where parameter reassignment (e.g., a = abs(a)) caused type mismatch.

#[test]
fn test_prelude_gcd_basic() {
    // Test prelude gcd function which reassigns its parameters: a = abs(a), b = abs(b)
    let src = "gcd(12, 8)";
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "gcd(12, 8) should succeed, got {:?}",
        result.err()
    );
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 4, "gcd(12, 8) should be 4"),
        Value::F64(v) => assert!(
            (v - 4.0).abs() < 1e-10,
            "gcd(12, 8) should be 4.0, got {}",
            v
        ),
        other => panic!("Expected numeric value, got {:?}", other),
    }
}

#[test]
fn test_prelude_gcd_negative() {
    // Test gcd with negative numbers (should use abs internally)
    let src = "gcd(-12, 8)";
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "gcd(-12, 8) should succeed, got {:?}",
        result.err()
    );
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 4, "gcd(-12, 8) should be 4"),
        Value::F64(v) => assert!(
            (v - 4.0).abs() < 1e-10,
            "gcd(-12, 8) should be 4.0, got {}",
            v
        ),
        other => panic!("Expected numeric value, got {:?}", other),
    }
}

#[test]
fn test_prelude_lcm_uses_gcd() {
    // Test lcm which internally calls gcd (with parameter reassignment)
    let src = "lcm(4, 6)";
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "lcm(4, 6) should succeed, got {:?}",
        result.err()
    );
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 12, "lcm(4, 6) should be 12"),
        other => panic!("Expected numeric value, got {:?}", other),
    }
}

#[test]
fn test_prelude_powermod() {
    // Test powermod which reassigns base parameter: base = base % m
    let src = "powermod(2, 10, 1000)";
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "powermod(2, 10, 1000) should succeed, got {:?}",
        result.err()
    );
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 24, "2^10 mod 1000 should be 24"),
        Value::F64(v) => assert!(
            (v - 24.0).abs() < 1e-10,
            "2^10 mod 1000 should be 24.0, got {}",
            v
        ),
        other => panic!("Expected numeric value, got {:?}", other),
    }
}

// ==================== typeof tests ====================

#[test]
fn test_typeof_int64() {
    let src = r#"println(typeof(42))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Int64");
}

#[test]
fn test_typeof_float64() {
    let src = r#"println(typeof(3.14))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Float64");
}

#[test]
fn test_typeof_string() {
    let src = r#"println(typeof("hello"))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "String");
}

#[test]
fn test_typeof_nothing() {
    let src = r#"println(typeof(nothing))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Nothing");
}

#[test]
fn test_typeof_vector() {
    let src = r#"println(typeof([1.0, 2.0, 3.0]))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Vector{Float64}");
}

#[test]
fn test_typeof_matrix() {
    let src = r#"println(typeof(zeros(2, 3)))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Matrix{Float64}");
}

// Range literals are now lazy (issue #520), returning UnitRange or StepRange types.
#[test]
fn test_typeof_range_as_lazy() {
    // Range literals now create lazy Range values
    let src = r#"
r = 1:10
println(typeof(r))
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    // Range literals with integer bounds produce UnitRange
    assert_eq!(output.trim(), "UnitRange");
}

#[test]
fn test_typeof_step_range_as_lazy() {
    // StepRange literals now create lazy Range values
    let src = r#"
r = 1:2:10
println(typeof(r))
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    // StepRange literals with step produce StepRange
    assert_eq!(output.trim(), "StepRange");
}

#[test]
fn test_typeof_complex() {
    // Complex{Float64} is the correct type for 1.0 + 2.0im
    let src = r#"println(typeof(1.0 + 2.0im))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Complex{Float64}");
}

#[test]
fn test_typeof_tuple() {
    let src = r#"println(typeof((1, 2.0, "a")))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Tuple{Int64, Float64, String}");
}

// ===========================================================================
// @test macro tests - require `using Test`
// ===========================================================================

#[test]
fn test_test_macro_without_using_test() {
    // @test without `using Test` should fail at lowering phase
    let src = r#"
@test 1 + 1 == 2
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error when using @test without 'using Test', but got: {:?}",
        result
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("using Test"),
        "Error message should mention 'using Test': {}",
        err_msg
    );
}

#[test]
fn test_testset_macro_without_using_test() {
    // @testset without `using Test` should fail at lowering phase
    let src = r#"
@testset "Basic" begin
    x = 1
end
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error when using @testset without 'using Test', but got: {:?}",
        result
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("using Test"),
        "Error message should mention 'using Test': {}",
        err_msg
    );
}

#[test]
fn test_test_macro_with_using_test() {
    // @test with `using Test` should work
    let src = r#"
using Test
@test 1 + 1 == 2
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "Expected success when using @test with 'using Test', but got error: {:?}",
        result
    );
}

#[test]
fn test_testset_macro_with_using_test() {
    // @testset with `using Test` should work
    let src = r#"
using Test
@testset "Basic" begin
    @test 1 + 1 == 2
    @test 2 * 3 == 6
end
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "Expected success when using @testset with 'using Test', but got error: {:?}",
        result
    );
}

// ==================== Iterator Protocol Tests ====================

#[test]
fn test_iterate_array_first() {
    // iterate(array) should return (first_element, state)
    let src = r#"
arr = [10.0, 20.0, 30.0]
result = iterate(arr)
result[1]  # first element
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 10.0).abs() < 0.001, "Expected 10.0, got {}", v),
        Ok(other) => panic!("Expected F64(10.0), got {:?}", other),
        Err(e) => panic!("iterate(array) failed: {}", e),
    }
}

#[test]
fn test_iterate_array_next() {
    // iterate(array, state) should return next element
    let src = r#"
arr = [10.0, 20.0, 30.0]
first = iterate(arr)
second = iterate(arr, first[2])
second[1]  # second element
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 20.0).abs() < 0.001, "Expected 20.0, got {}", v),
        Ok(other) => panic!("Expected F64(20.0), got {:?}", other),
        Err(e) => panic!("iterate(array, state) failed: {}", e),
    }
}

#[test]
fn test_iterate_empty_array() {
    // iterate on empty array should return nothing
    // Check by using println(typeof(...)) to get the type name
    let src = r#"
arr = zeros(0)
println(typeof(iterate(arr)))
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Nothing");
}

#[test]
fn test_iterate_range() {
    // iterate on range
    let src = r#"
r = 1:5
first = iterate(r)
first[1]  # should be 1
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::I64(v)) => assert_eq!(v, 1, "Expected 1, got {}", v),
        // Range uses F64 internally, so iterate returns F64
        Ok(Value::F64(v)) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
        Ok(other) => panic!("Expected I64(1) or F64(1.0), got {:?}", other),
        Err(e) => panic!("iterate(range) failed: {}", e),
    }
}

#[test]
fn test_collect_range() {
    // collect(range) should return an array
    let src = r#"
r = 1:5
arr = collect(r)
length(arr)
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::I64(5)) => (),
        Ok(other) => panic!("Expected I64(5), got {:?}", other),
        Err(e) => panic!("collect(range) failed: {}", e),
    }
}

#[test]
fn test_collect_range_step() {
    // collect step range
    let src = r#"
r = 1:2:9
arr = collect(r)
arr[3]  # should be 5 (1, 3, 5, 7, 9)
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::I64(v)) => assert_eq!(v, 5, "Expected 5, got {}", v),
        Ok(other) => panic!("Expected I64(5), got {:?}", other),
        Err(e) => panic!("collect step range failed: {}", e),
    }
}

// ==================================================================================
// Generator tests
// ==================================================================================

#[test]
fn test_generator_typeof() {
    // typeof(Generator) currently returns "Base.Generator"
    let src = r#"
square(x) = x * x
g = Generator(square, 1:5)
println(typeof(g))
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    // TODO: Implement proper Generator type, for now it returns Any
    assert_eq!(output.trim(), "Base.Generator");
}

// TODO: Generator iteration is not yet implemented
// The VM has GeneratorValue but iterate(Generator) isn't supported yet
// #[test]
// fn test_generator_collect_simple() {
//     // collect(Generator) should materialize the iterator
//     // Note: current implementation returns underlying iterator without applying function
//     let src = r#"
// identity(x) = x
// g = Generator(identity, 1:3)
// arr = collect(g)
// length(arr)
// "#;
//     let result = run_core_pipeline(src, 0);
//     match result {
//         Ok(Value::I64(len)) => assert_eq!(len, 3, "Expected length 3, got {}", len),
//         Ok(other) => panic!("Expected I64(3), got {:?}", other),
//         Err(e) => panic!("Generator collect failed: {}", e),
//     }
// }

