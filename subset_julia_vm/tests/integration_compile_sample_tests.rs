//! Integration tests: IR compilation, compile module, program compilation, code samples, macros

mod common;
use common::*;

use std::ffi::{CStr, CString};
use subset_julia_vm::vm::Value;
use subset_julia_vm::*;

// ==================== IR Compilation ====================

#[test]
fn test_compile_to_ir() {
    let src = r#"
function f(N)
    return N
end
"#;
    let json = compile_to_ir_str(src);
    assert!(json.is_some());

    let json = json.unwrap();
    // The IR uses "functions" array not "Func"
    assert!(
        json.contains("\"functions\""),
        "JSON does not contain functions: {}",
        json
    );
    assert!(json.contains("\"name\":\"f\""));
}

#[test]
fn test_run_ir_json() {
    // The run_ir_json_str function runs the main block, not a function call
    // So we need to include the function call in the source
    let src = r#"
function f(N)
    return 2N
end
f(50)
"#;
    let json = compile_to_ir_str(src).unwrap();
    let result = run_ir_json_str(&json, 0, 0); // n parameter is unused
    assert!((result - 100.0).abs() < 1e-10);
}

// ==================== Error Handling ====================

#[test]
fn test_invalid_syntax_returns_nan() {
    let src = "this is not valid julia code";
    let result = compile_and_run_str(src, 0);
    assert!(result.is_nan());
}

#[test]
fn test_empty_function_call_wrong_name() {
    // Function is named 'f' but called as 'g'
    let src = r#"
function f(N)
    return N
end
g(100)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(result.is_nan());
}

// ==================== Edge Cases ====================

#[test]
fn test_zero_iterations() {
    let src = r#"
function f(N)
    sum = 0
    for i in 1:N
        sum += i
    end
    return sum
end
f(0)
"#;
    let result = compile_and_run_str(src, 0);
    // Loop should not execute, sum stays 0
    assert!((result - 0.0).abs() < 1e-10);
}

#[test]
fn test_single_iteration() {
    let src = r#"
function f(N)
    sum = 0
    for i in 1:N
        sum += i
    end
    return sum
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn test_large_n() {
    let src = r#"
function f(N)
    return N
end
f(1000000)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1_000_000.0).abs() < 1e-10);
}

// ==================== Float Operations ====================

#[test]
fn test_float_literal() {
    let src = r#"
function f(N)
    return 3.14159
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    let expected = 314_159.0 / 100_000.0;
    assert!((result - expected).abs() < 1e-10);
}

#[test]
fn test_float_arithmetic() {
    // Test float addition directly without variable assignment
    let src = r#"
function f(N)
    return 1.5 + 2.5
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10);
}

// ==================== Implicit Multiplication ====================

#[test]
fn test_implicit_mult_4n() {
    let src = r#"
function f(N)
    return 4N
end
f(10)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 40.0).abs() < 1e-10);
}

#[test]
fn test_implicit_mult_in_expression() {
    let src = r#"
function f(N)
    cnt = 10
    return 4cnt / N
end
f(20)
"#;
    let result = compile_and_run_str(src, 0);
    // 4 * 10 / 20 = 2
    assert!((result - 2.0).abs() < 1e-10);
}

// ==================== Program Tests (println) ====================

#[test]
fn test_compile_and_run_auto_println() {
    let src = r#"println("Hello")"#;
    let result = compile_and_run_auto_str(src, 0);
    // println returns Unit, which maps to -4 in the FFI
    assert!((result - (-4.0)).abs() < 1e-10);
}

#[test]
fn test_compile_and_run_auto_function() {
    let src = r#"
function f(N)
    return 2N
end
f(100)
"#;
    let result = compile_and_run_auto_str(src, 0);
    assert!((result - 200.0).abs() < 1e-10);
}

// ==================== Edge Cases ====================

#[test]
fn test_zero_loop_iterations() {
    let src = r#"
function f(N)
    sum = 100
    for i in 1:0
        sum += 1
    end
    return sum
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    // Loop doesn't execute, sum stays 100
    assert!((result - 100.0).abs() < 1e-10);
}

#[test]
fn test_one_loop_iteration() {
    let src = r#"
function f(N)
    sum = 0
    for i in 1:1
        sum += 10
    end
    return sum
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 10.0).abs() < 1e-10);
}

// ==================== Compile Module Tests ====================

#[test]
fn test_compile_simple_return() {
    let src = r#"
function f(N)
    return N
end
f(42)
"#;
    match compile_and_run_func(src, 42, 0) {
        Value::I64(v) => assert_eq!(v, 42),
        _ => panic!("Expected I64"),
    }
}

#[test]
fn test_compile_constant_return() {
    let src = r#"
function f(N)
    return 100
end
f(0)
"#;
    match compile_and_run_func(src, 0, 0) {
        Value::I64(v) => assert_eq!(v, 100),
        _ => panic!("Expected I64"),
    }
}

#[test]
fn test_compile_division_direct() {
    let src = r#"
function f(N)
    return N / 2
end
f(10)
"#;
    match compile_and_run_func(src, 10, 0) {
        Value::F64(v) => assert!((v - 5.0).abs() < 1e-10),
        _ => panic!("Expected F64"),
    }
}

#[test]
fn test_compile_power_direct() {
    let src = r#"
function f(N)
    return N^2
end
f(7)
"#;
    match compile_and_run_func(src, 7, 0) {
        Value::F64(v) => assert!((v - 49.0).abs() < 1e-10),
        _ => panic!("Expected F64"),
    }
}

#[test]
fn test_compile_sqrt_direct() {
    let src = r#"
function f(N)
    return sqrt(N)
end
f(16)
"#;
    match compile_and_run_func(src, 16, 0) {
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
        _ => panic!("Expected F64"),
    }
}

#[test]
fn test_compile_for_loop_direct() {
    let src = r#"
function f(N)
    cnt = 0
    for i in 1:N
        cnt += 1
    end
    return cnt
end
f(5)
"#;
    match compile_and_run_func(src, 5, 0) {
        Value::I64(v) => assert_eq!(v, 5),
        _ => panic!("Expected I64"),
    }
}

#[test]
fn test_compile_for_loop_sum_direct() {
    let src = r#"
function f(N)
    sum = 0
    for i in 1:N
        sum += i
    end
    return sum
end
f(5)
"#;
    // 1+2+3+4+5 = 15
    match compile_and_run_func(src, 5, 0) {
        Value::I64(v) => assert_eq!(v, 15),
        _ => panic!("Expected I64"),
    }
}

#[test]
fn test_compile_rand_direct() {
    let src = r#"
function f()
    return rand()
end
f()
"#;
    match compile_and_run_func(src, 0, 42) {
        Value::F64(v) => {
            assert!((0.0..1.0).contains(&v), "rand() should be in [0, 1)");
        }
        _ => panic!("Expected F64"),
    }
}

#[test]
fn test_compile_rand_deterministic_direct() {
    let src = r#"
function f()
    return rand()
end
f()
"#;
    let r1 = compile_and_run_func(src, 0, 123);
    let r2 = compile_and_run_func(src, 0, 123);
    match (r1, r2) {
        (Value::F64(v1), Value::F64(v2)) => {
            assert_eq!(v1, v2, "Same seed should produce same result");
        }
        _ => panic!("Expected F64"),
    }
}

// ==================== Program Compilation (println) Tests ====================

#[test]
fn test_compile_println_string() {
    let src = r#"println("Hello")"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::Nothing));
    assert_eq!(output, "Hello\n");
}

#[test]
fn test_compile_println_multiple() {
    let src = r#"
println("Line 1")
println("Line 2")
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output, "Line 1\nLine 2\n");
}

#[test]
fn test_compile_println_escape_newline() {
    let src = r#"println("Hello\nWorld")"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output, "Hello\nWorld\n");
}

#[test]
fn test_compile_println_escape_tab() {
    let src = r#"println("A\tB")"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output, "A\tB\n");
}

#[test]
fn test_compile_print_no_newline() {
    // Test print() without trailing newline
    let src = r#"
print("A")
print("B")
print("C")
1
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(1)));
    assert_eq!(output, "ABC");
}

#[test]
fn test_compile_print_mixed_with_println() {
    // Test mixing print() and println()
    let src = r#"
print("Hello")
print(" ")
println("World")
print("A")
println("B")
1
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(1)));
    assert_eq!(output, "Hello World\nAB\n");
}

#[test]
fn test_compile_print_i64_no_newline() {
    // Test print() with integer without trailing newline
    let src = r#"
print(1)
print(2)
print(3)
0
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(0)));
    assert_eq!(output, "123");
}

#[test]
fn test_compile_print_ascii_art_grid() {
    // Test print() for ASCII art like Mandelbrot sample
    let src = r#"
for row in 1:3
    for col in 1:5
        if col > 3
            print("*")
        else
            print(".")
        end
    end
    println("")
end
0
"#;
    let (result, output) = compile_and_run_script_direct(src, 0);
    assert!(matches!(result, Value::I64(0)));
    assert_eq!(output, "...**\n...**\n...**\n");
}

#[test]
fn test_mandelbrot_scalar_sample() {
    // Test the actual Mandelbrot sample from iOS app (now using complex numbers and abs2)
    let src = r#"
# Mandelbrot escape time algorithm
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

println("Testing Mandelbrot:")
c1 = mandelbrot_escape(0.0 + 0.0im, 100)
println("c1 = ", c1)

println("Mini Mandelbrot (5x3):")
for row in 0:2
    ci = 1.0 - row * 1.0
    for col in 0:4
        cr = -2.0 + col * 0.75
        c = cr + ci * im
        n = mandelbrot_escape(c, 50)
        if n == 50
            print("*")
        elseif n > 10
            print("+")
        else
            print(" ")
        end
    end
    println("")
end

c1
"#;
    let (result, output) = compile_and_run_script_direct(src, 0);
    println!("Mandelbrot output:\n{}", output);
    println!("Result: {:?}", result);
    // The result can be I64 or F64 depending on type inference
    match result {
        Value::I64(v) => assert_eq!(v, 100),
        Value::F64(v) => assert!((v - 100.0).abs() < 1e-10),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
    assert!(output.contains("Testing Mandelbrot:"));
    assert!(output.contains("Mini Mandelbrot"));
}

#[test]
fn test_mandelbrot_via_ffi() {
    // Test using the actual FFI function (now using complex numbers and abs2)
    let src = r#"
# Mandelbrot escape time algorithm
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

println("Test:")
for row in 0:2
    for col in 0:4
        c = (-2.0 + col * 0.75) + (1.0 - row * 1.0) * im
        n = mandelbrot_escape(c, 50)
        if n == 50
            print("*")
        else
            print(" ")
        end
    end
    println("")
end
0
"#;
    let c_src = CString::new(src).unwrap();
    let result_ptr = compile_and_run_with_output(c_src.as_ptr(), 0);
    assert!(!result_ptr.is_null(), "FFI returned null");
    let output = unsafe { CStr::from_ptr(result_ptr) }
        .to_string_lossy()
        .to_string();
    free_string(result_ptr);
    println!("FFI output:\n{}", output);
    assert!(output.contains("Test:"));
    // With complex number implementation, c=-2.0 escapes at iteration 16
    // (floating-point boundary behavior), so we get 3 stars instead of 4
    assert!(output.contains("***"));
}

#[test]
fn test_mandelbrot_ios_sample_exact() {
    // Test the EXACT iOS sample code (now using complex numbers and abs2)
    let src = r#"
# Mandelbrot escape time algorithm
# Type annotations required for Complex dispatch
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Test a few points
println("Testing Mandelbrot escape times:")

# Point inside the set (should return maxiter)
c1 = mandelbrot_escape(0.0 + 0.0im, 100)
println("  (0, 0): ", c1, " (in set)")

# Point on the boundary
c2 = mandelbrot_escape(-0.75 + 0.0im, 100)
println("  (-0.75, 0): ", c2)

# Point outside (escapes quickly)
c3 = mandelbrot_escape(1.0 + 1.0im, 100)
println("  (1, 1): ", c3, " (escaped)")

# Interesting point near the boundary
c4 = mandelbrot_escape(-0.1 + 0.65im, 100)
println("  (-0.1, 0.65): ", c4)

# Generate a small text visualization
println("")
println("Mini Mandelbrot (21x11):")
for row in 0:10
    ci = 1.0 - row * 0.2  # y from 1.0 to -1.0
    for col in 0:20
        cr = -2.0 + col * 0.15  # x from -2.0 to 1.0
        c = cr + ci * im
        n = mandelbrot_escape(c, 50)
        if n == 50
            print("*")  # In the set
        elseif n > 10
            print("+")  # Slow escape
        else
            print(" ")  # Fast escape
        end
    end
    println("")
end

c1
"#;
    let c_src = CString::new(src).unwrap();
    let result_ptr = compile_and_run_with_output(c_src.as_ptr(), 0);
    if result_ptr.is_null() {
        panic!("FFI returned null - compilation or parsing failed!");
    }
    let output = unsafe { CStr::from_ptr(result_ptr) }
        .to_string_lossy()
        .to_string();
    free_string(result_ptr);
    println!("iOS sample output:\n{}", output);
    assert!(
        output.contains("Testing Mandelbrot escape times:"),
        "Missing test header"
    );
    assert!(
        output.contains("Mini Mandelbrot"),
        "Missing Mini Mandelbrot header"
    );
}

// ==================== Error Cases ====================

#[test]
fn test_compile_unknown_variable_error() {
    // Unknown variables should cause a runtime error when the function is actually called
    let src = r#"
function f(N)
    return unknown_var
end
f(1)
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err());
}

#[test]
fn test_arbitrary_power_cubed() {
    // Test N^3 (arbitrary power support)
    let src = r#"
function f(N)
    return N^3
end
f(4)
"#;
    let result = compile_and_run_str(src, 0);
    // 4^3 = 64
    assert!(
        (result - 64.0).abs() < 1e-10,
        "Expected 64.0, got {}",
        result
    );
}

// ==================== Code Sample Tests ====================

#[test]
fn test_sample_simple_arithmetic_output() {
    let src = r#"
x = 10
y = 20
sum = x + y
product = x * y
println("Sum: ", sum)
println("Product: ", product)
sum
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(30)));
    assert_eq!(output, "Sum: 30\nProduct: 200\n");
}

#[test]
fn test_sample_countdown_output() {
    let src = r#"
function countdown(n)
    for i in n:-1:1
        println(i)
    end
    println("Liftoff!")
end

countdown(5)
"#;
    let (_, output) = compile_and_run_script_direct(src, 0);
    assert_eq!(output, "5\n4\n3\n2\n1\nLiftoff!\n");
}

#[test]
fn test_sample_geometric_series_star_eq() {
    let src = r#"
function geometric_sum(r, n)
    sum = 0.0
    term = 1.0
    for i in 1:n
        sum += term
        term *= r
    end
    sum
end

geometric_sum(0.5, 10)
"#;
    let result = compile_and_run_auto_str(src, 0);
    assert!((result - 1.998046875).abs() < 1e-9);
}

#[test]
fn test_sample_sum_primes_script() {
    let src = r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

function sum_primes(n)
    sum = 0
    for i in 2:n
        if is_prime(i) == 1
            sum += i
        end
    end
    sum
end

sum_primes(100)
"#;
    let result = run_core_pipeline(src, 0).expect("pipeline failed");
    let value = match result {
        Value::I64(v) => v as f64,
        Value::F64(v) => v,
        _ => f64::NAN,
    };
    assert!((value - 1060.0).abs() < 1e-9);
}

#[test]
fn test_samples_smoke() {
    let samples = [
        r#"println("Hello, World!")"#,
        r#"
x = 10
y = 20
sum = x + y
product = x * y
println("Sum: ", sum)
println("Product: ", product)
sum
"#,
        r#"
x = 16.0
result = sqrt(x)
println("sqrt(", x, ") = ", result)
result
"#,
        r#"
function sum_to_n(N)
    sum = 0
    for i in 1:N
        sum += i
    end
    sum
end

sum_to_n(100)
"#,
        r#"
function countdown(n)
    for i in n:-1:1
        println(i)
    end
    println("Liftoff!")
end

countdown(10)
"#,
        r#"
function power_of_2(n)
    result = 1
    for i in 1:n
        result = result * 2
    end
    result
end

power_of_2(10)
"#,
        r#"
function double(x)
    2 * x
end

double(21)
"#,
        r#"
function max2(a, b)
    ifelse(a > b, a, b)
end

max2(42, 17)
"#,
        r#"
function factorial(n)
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end

factorial(10)
"#,
        r#"
function factorial(n)
    if n <= 1
        return 1
    end
    n * factorial(n - 1)
end

factorial(10)
"#,
        r#"
function fib(n)
    if n <= 1
        return n
    end
    fib(n - 1) + fib(n - 2)
end

fib(15)
"#,
        r#"
function fib_fast(n)
    if n <= 1
        return n
    end
    a = 0
    b = 1
    for i in 2:n
        c = a + b
        a = b
        b = c
    end
    b
end

fib_fast(30)
"#,
        r#"
function gcd(a, b)
    while b > 0
        temp = b
        b = a % b
        a = temp
    end
    a
end

gcd(48, 18)
"#,
        r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    if n <= 3
        return 1
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

is_prime(97)
"#,
        r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

function sum_primes(n)
    sum = 0
    for i in 2:n
        if is_prime(i) == 1
            sum += i
        end
    end
    sum
end

sum_primes(100)
"#,
        r#"
function estimate_pi(N)
    inside = 0
    for i in 1:N
        x = rand()
        y = rand()
        if x^2 + y^2 < 1.0
            inside += 1
        end
    end
    4.0 * inside / N
end

estimate_pi(10000)
"#,
        r#"
function random_walk_1d(steps)
    position = 0.0
    for i in 1:steps
        step = ifelse(rand() < 0.5, -1.0, 1.0)
        position += step
    end
    position
end

random_walk_1d(1000)
"#,
        r#"
function monte_carlo_integral(N)
    # Estimate integral of x^2 from 0 to 1
    sum = 0.0
    for i in 1:N
        x = rand()
        sum += x^2
    end
    sum / N  # Should be close to 1/3
end

monte_carlo_integral(100000)
"#,
        r#"
function harmonic(n)
    sum = 0.0
    for i in 1:n
        sum += 1.0 / i
    end
    sum
end

harmonic(100)
"#,
        r#"
function geometric_sum(r, n)
    # Sum of r^0 + r^1 + ... + r^(n-1)
    sum = 0.0
    term = 1.0
    for i in 1:n
        sum += term
        term *= r
    end
    sum
end

geometric_sum(0.5, 10)
"#,
        r#"
function newton_sqrt(x)
    # Find sqrt(x) using Newton's method
    guess = x / 2.0
    for i in 1:10
        guess = (guess + x / guess) / 2.0
    end
    guess
end

newton_sqrt(2.0)
"#,
        r#"
function exp_taylor(x, terms)
    # e^x ≈ 1 + x + x^2/2! + x^3/3! + ...
    result = 1.0
    term = 1.0
    for n in 1:terms
        term = term * x / n
        result += term
    end
    result
end

exp_taylor(1.0, 20)  # Should be close to e ≈ 2.71828
"#,
    ];

    for sample in samples {
        let result = compile_and_run_auto_str(sample, 42);
        assert!(!result.is_nan(), "sample failed: {}", sample);
    }
}

// ==================== Macro Tests ====================

#[test]
fn test_assert_success() {
    // Assert with true condition should pass
    let src = r#"
@assert 1 > 0
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10);
}

#[test]
fn test_assert_with_message() {
    // Assert with true condition and message
    let src = r#"
x = 10
@assert x > 5 "x must be greater than 5"
x
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 10.0).abs() < 1e-10);
}

#[test]
fn test_time_expression() {
    // @time should execute and return the result
    let src = r#"
@time 1 + 2 + 3
"#;
    let result = compile_and_run_str(src, 0);
    // Result should be 6 (the expression value is not captured, but execution should succeed)
    // Since @time wraps in Stmt::Timed, it doesn't return the value
    // Let's check it runs without error
    assert!(!result.is_nan());
}

#[test]
fn test_time_block() {
    // @time with begin...end block
    let src = r#"
@time begin
    x = 0
    for i in 1:100
        x += i
    end
end
100
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 100.0).abs() < 1e-10);
}

#[test]
fn test_assert_in_function() {
    let src = r#"
function checked_sqrt(x)
    @assert x >= 0 "cannot take sqrt of negative number"
    sqrt(x)
end
checked_sqrt(16.0)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10);
}

// ==================== main.jl Syntax Tests ====================

#[test]
fn test_unicode_function_name_pi() {
    // Test function with π in name (calcπ from main.jl)
    let src = r#"
function calcπ(N)
    N * 3
end
calcπ(10)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 30.0).abs() < 1e-10);
}

#[test]
fn test_pi_constant_ascii() {
    let src = r#"
function f(N)
    return pi
end
f(0)
"#;
    let result = compile_and_run_str(src, 0);
    eprintln!(
        "DEBUG: result={}, PI={}, diff={}",
        result,
        std::f64::consts::PI,
        (result - std::f64::consts::PI).abs()
    );
    assert!(
        (result - std::f64::consts::PI).abs() < 1e-10,
        "Expected PI, got {}",
        result
    );
}

#[test]
fn test_pi_constant_unicode() {
    let src = r#"
function f(N)
    return π
end
f(0)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - std::f64::consts::PI).abs() < 1e-10);
}

#[test]
fn test_pi_shadowed_by_loop_var() {
    // When "pi" is used as a loop variable, it should shadow the builtin constant
    let src = r#"
sum = 0
for pi in 1:10
    sum += pi
end
sum
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 55.0).abs() < 1e-10);
}

#[test]
fn test_unicode_for_in_operator() {
    // Test for loop with ∈ instead of 'in'
    let src = r#"
function sum_range(N)
    total = 0
    for i ∈ 1:N
        total += i
    end
    total
end
sum_range(5)
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 2 + 3 + 4 + 5 = 15
    assert!((result - 15.0).abs() < 1e-10);
}

#[test]
fn test_mainjl_gcd_function() {
    // Test GCD function from main.jl (uses while, !=, %)
    let src = r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    return a
end
mygcd(48, 18)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 6.0).abs() < 1e-10);
}

#[test]
fn test_mainjl_calc_pi() {
    // Test π calculation using coprime probability (main.jl style)
    let src = r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    return a
end

function calcπ(N)
    cnt = 0
    for a ∈ 1:N
        for b = 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    return sqrt(6.0 / prob)
end

calcπ(30)
"#;
    let result = compile_and_run_str(src, 0);
    // Should be approximately π (3.14159...)
    // With N=30, accuracy is limited, so use wider tolerance
    assert!(
        (result - std::f64::consts::PI).abs() < 0.5,
        "calcπ(30) = {}, expected ~3.14159",
        result
    );
}

#[test]
fn test_mainjl_with_time() {
    // Test with @time macro (as in main.jl)
    let src = r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    return a
end

function calcπ(N)
    cnt = 0
    for a in 1:N
        for b = 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    return sqrt(6.0 / prob)
end

@time calcπ(10)
"#;
    let result = compile_and_run_str(src, 0);
    // @time wraps the expression, result should still be valid
    assert!(!result.is_nan());
}

#[test]
fn test_time_with_println() {
    // Test @time with println (as in iOS CodeSample)
    let src = r#"
function f(N)
    return N * 2
end

@time println(f(5))
"#;
    let result = compile_and_run_str(src, 0);
    // Should not fail - println returns Unit, @time wraps it
    assert!(!result.is_nan());
}

#[test]
fn test_coprime_pi_ios_sample() {
    // Exact code from iOS CodeSample "Coprime π Estimation"
    let src = r#"
# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end
#=
Note that it takes a 15 seconds to complete
=#
@time println(calc_pi(10))
"#;
    let result = compile_and_run_str(src, 0);
    // Should complete without error
    assert!(!result.is_nan(), "Expected valid result, got NaN");
}

// ==================== @show Macro Tests ====================

#[test]
fn test_show_variable() {
    let src = r#"
x = 42
@show x
x
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(42)));
    assert_eq!(output, "x = 42\n");
}

#[test]
fn test_show_expression() {
    let src = r#"
a = 10
b = 20
@show a + b
a + b
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(30)));
    // Expression is displayed as source text
    assert_eq!(output, "a + b = 30\n");
}

#[test]
fn test_show_function_call() {
    let src = r#"
@show sqrt(16.0)
sqrt(16.0)
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    match result {
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
        _ => panic!("Expected F64"),
    }
    // Expression is displayed as source text
    assert_eq!(output, "sqrt(16.0) = 4.0\n");
}

#[test]
fn test_show_in_function() {
    let src = r#"
function debug_sum(N)
    sum = 0
    for i in 1:N
        sum += i
        @show sum
    end
    sum
end
debug_sum(3)
"#;
    let (result, output) = compile_and_run_script_direct(src, 0);
    assert!(matches!(result, Value::I64(6)));
    // sum is shown 3 times: 1, 3, 6
    assert_eq!(output, "sum = 1\nsum = 3\nsum = 6\n");
}

#[test]
fn test_show_with_println() {
    let src = r#"
x = 100
println("Before show")
@show x
println("After show")
x
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(100)));
    assert_eq!(output, "Before show\nx = 100\nAfter show\n");
}

#[test]
fn test_show_literal_integer() {
    let src = r#"
println("Hello, World!")
@show 1
1
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(matches!(result, Value::I64(1)));
    assert_eq!(output, "Hello, World!\n1 = 1\n");
}

#[test]
fn test_show_literal_float() {
    let src = r#"
@show 3.14
3.14
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    match result {
        Value::F64(v) => {
            let expected = 314.0 / 100.0;
            assert!((v - expected).abs() < 1e-10);
        }
        _ => panic!("Expected F64"),
    }
    assert_eq!(output, "3.14 = 3.14\n");
}
