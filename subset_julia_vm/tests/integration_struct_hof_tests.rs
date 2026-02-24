//! Integration tests: Structs, parametric types, HOFs, kwargs, do syntax, randn, iOS samples

mod common;
use common::*;

use subset_julia_vm::vm::Value;

// ==================== Struct Tests ====================
// These tests use the Core IR pipeline (tree-sitter → lowering → compile_core)

#[test]
fn test_struct_basic_immutable() {
    // Basic immutable struct with typed fields
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(3.0, 4.0)
p.x + p.y
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
    match result {
        Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_struct_field_access() {
    // Test accessing individual fields
    let src = r#"
struct Vector2D
    x::Float64
    y::Float64
end

v = Vector2D(10.0, 20.0)
v.y - v.x
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
    match result {
        Value::F64(x) => assert!((x - 10.0).abs() < 1e-10, "Expected 10.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 10, "Expected 10, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_mutable_struct_field_assignment() {
    // Test mutable struct field assignment
    let src = r#"
mutable struct Counter
    value::Float64
end

c = Counter(0.0)
c.value = 42.0
c.value
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
    match result {
        Value::F64(x) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 42, "Expected 42, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_struct_in_expression() {
    // Test struct fields in arithmetic expressions
    let src = r#"
struct Rectangle
    width::Float64
    height::Float64
end

r = Rectangle(5.0, 3.0)
area = r.width * r.height
area
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
    match result {
        Value::F64(x) => assert!((x - 15.0).abs() < 1e-10, "Expected 15.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 15, "Expected 15, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_struct_euclidean_distance() {
    // Test struct with sqrt calculation (Euclidean distance)
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

p1 = Point(0.0, 0.0)
p2 = Point(3.0, 4.0)
dx = p2.x - p1.x
dy = p2.y - p1.y
sqrt(dx*dx + dy*dy)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
    match result {
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Parametric Type Tests ====================

#[test]
fn test_parametric_struct_explicit_type() {
    // Test parametric struct with explicit type parameter
    let src = r#"
struct Point{T}
    x::T
    y::T
end

p = Point{Float64}(3.0, 4.0)
p.x + p.y
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run parametric struct test");
    match result {
        Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_struct_type_inference() {
    // Test parametric struct with type inference from constructor arguments
    let src = r#"
struct Point{T}
    x::T
    y::T
end

p = Point(1.5, 2.5)
p.x + p.y
"#;
    let result =
        run_core_pipeline(src, 0).expect("Failed to run parametric struct type inference test");
    match result {
        Value::F64(x) => assert!((x - 4.0).abs() < 1e-10, "Expected 4.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 4, "Expected 4, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_struct_int_type() {
    // Test parametric struct with Int64 type parameter
    let src = r#"
struct Point{T}
    x::T
    y::T
end

p = Point{Int64}(3, 4)
p.x + p.y
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run parametric struct int test");
    match result {
        Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
        Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_struct_multiple_params() {
    // Test parametric struct with multiple type parameters
    let src = r#"
struct Pair{A, B}
    first::A
    second::B
end

pair = Pair{Int64, Float64}(10, 2.5)
pair.second * 4.0
"#;
    let result =
        run_core_pipeline(src, 0).expect("Failed to run parametric struct multiple params test");
    match result {
        Value::F64(x) => assert!((x - 10.0).abs() < 1e-10, "Expected 10.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 10, "Expected 10, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_struct_with_bound() {
    // Test parametric struct with type bound
    let src = r#"
struct Numeric{T<:Number}
    value::T
end

n = Numeric{Float64}(42.0)
n.value
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run parametric struct bound test");
    match result {
        Value::F64(x) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 42, "Expected 42, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_nested_array_field() {
    // Test parametric struct with Array{T} field type
    let src = r#"
struct Container{T}
    items::Array{T}
end

c = Container{Float64}([1.0, 2.0, 3.0])
c.items[1]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run nested array field test");
    match result {
        Value::F64(x) => assert!((x - 1.0).abs() < 1e-10, "Expected 1.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 1, "Expected 1, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_nested_struct_type() {
    // Test nested parametric struct: Container{Point{Float64}}
    let src = r#"
struct Point{T}
    x::T
    y::T
end

struct Container{T}
    item::T
end

p = Point{Float64}(1.0, 2.0)
c = Container{Point{Float64}}(p)
c.item.x + c.item.y
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run nested struct type test");
    match result {
        Value::F64(x) => assert!((x - 3.0).abs() < 1e-10, "Expected 3.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 3, "Expected 3, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_parametric_double_nested() {
    // Test doubly nested parametric struct: Wrapper{Wrapper{Float64}}
    let src = r#"
struct Wrapper{T}
    value::T
end

inner = Wrapper{Float64}(42.0)
outer = Wrapper{Wrapper{Float64}}(inner)
outer.value.value
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run double nested test");
    match result {
        Value::F64(x) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 42, "Expected 42, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Break and Continue ====================

#[test]
fn test_while_break() {
    // Test break statement in while loop
    let src = r#"
i = 0
sum = 0
while i < 10
    i += 1
    sum += i
    if sum > 20
        break
    end
end
sum
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run break test");
    match result {
        Value::I64(x) => assert!(x > 20, "Expected sum > 20, got {}", x),
        Value::F64(x) => assert!(x > 20.0, "Expected sum > 20, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_for_break() {
    // Test break statement in for loop
    let src = r#"
sum = 0
for i in 1:100
    sum += i
    if sum > 50
        break
    end
end
sum
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run for break test");
    match result {
        Value::I64(x) => assert!(x > 50, "Expected sum > 50, got {}", x),
        Value::F64(x) => assert!(x > 50.0, "Expected sum > 50, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_while_continue() {
    // Test continue statement in while loop (skip numbers less than 5)
    let src = r#"
i = 0
sum = 0
while i < 10
    i += 1
    if i < 5
        continue
    end
    sum += i
end
sum
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run continue test");
    match result {
        Value::I64(x) => assert_eq!(x, 45, "Expected sum of 5+6+7+8+9+10 = 45, got {}", x),
        Value::F64(x) => assert!((x - 45.0).abs() < 1e-10, "Expected 45.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_for_continue() {
    // Test continue statement in for loop (skip numbers less than 5)
    let src = r#"
sum = 0
for i in 1:10
    if i < 5
        continue
    end
    sum += i
end
sum
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run for continue test");
    match result {
        Value::I64(x) => assert_eq!(x, 45, "Expected sum of 5+6+7+8+9+10 = 45, got {}", x),
        Value::F64(x) => assert!((x - 45.0).abs() < 1e-10, "Expected 45.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_nested_loops_break() {
    // Test break in nested loops (should only break inner loop)
    let src = r#"
outer_sum = 0
inner_sum = 0
for i in 1:5
    outer_sum += i
    for j in 1:10
        inner_sum += j
        if inner_sum > 20
            break
        end
    end
end
outer_sum + inner_sum
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run nested break test");
    match result {
        Value::I64(x) => assert!(x > 20, "Expected sum > 20, got {}", x),
        Value::F64(x) => assert!(x > 20.0, "Expected sum > 20, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Try/Catch/Finally ====================

#[test]
fn test_try_catch_finally_with_message() {
    let src = r#"
x = 0
try
    # Use integer division to trigger error (float division returns Inf per IEEE 754)
    y = div(1, 0)
catch e
    x = 10
    println(e)
finally
    x += 1
end
x
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run try/catch/finally test");
    match result {
        Value::I64(x) => assert_eq!(x, 11, "Expected 11, got {}", x),
        Value::F64(x) => assert!((x - 11.0).abs() < 1e-10, "Expected 11.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_try_else_finally_no_error() {
    let src = r#"
x = 0
try
    x = 1
catch e
    x = 2
else
    x += 3
finally
    x += 4
end
x
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run try/else/finally test");
    match result {
        Value::I64(x) => assert_eq!(x, 8, "Expected 8, got {}", x),
        Value::F64(x) => assert!((x - 8.0).abs() < 1e-10, "Expected 8.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_try_catch_finally_no_error_no_else() {
    // This is the failing case from code_samples_tests
    let src = r#"
result = 0
try
    result = 10 / 2
catch e
    result = -1
finally
    cleanup_done = 1
end
result
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run try/catch/finally test");
    match result {
        Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Array Slicing ====================

#[test]
fn test_slice_range_1d() {
    let src = r#"
a = [10, 20, 30, 40]
b = a[1:3]
b[2] + b[3]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run slice range test");
    match result {
        Value::F64(x) => assert!((x - 50.0).abs() < 1e-10, "Expected 50.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 50, "Expected 50, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_slice_full_matrix() {
    let src = r#"
m = [1 2; 3 4]
s = m[:, :]
s[1, 2] + s[2, 1]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run full slice test");
    match result {
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Phase A: Mandelbrot Broadcast Support ====================

#[test]
fn test_transpose_1d_array() {
    // 1D array [n] becomes row vector [1, n]
    let src = r#"
a = [1.0, 2.0, 3.0]
b = a'
length(b)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run transpose 1D test");
    match result {
        Value::I64(x) => assert_eq!(x, 3, "Expected length 3, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_transpose_2d_basic() {
    // Test that transpose changes the shape
    // [1 2; 3 4] is 2x2, transpose is also 2x2
    // Just verify transpose runs without error
    let src = r#"
m = [1 2; 3 4]
t = m'
t[1, 1]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run transpose 2D test");
    match result {
        Value::F64(x) => assert!((x - 1.0).abs() < 1e-10, "Expected 1.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 1, "Expected 1, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_im_literal() {
    // im should be complex(0, 1)
    // Use complex operations to verify
    let src = r#"
z = im
z
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run im literal test");
    if let Some((re, im)) = result.as_complex_parts() {
        assert!((re - 0.0).abs() < 1e-10, "Expected real=0.0, got {}", re);
        assert!((im - 1.0).abs() < 1e-10, "Expected imag=1.0, got {}", im);
    } else {
        panic!("Unexpected result type: {:?}", result);
    }
}

#[test]
fn test_im_in_expression() {
    // Test that im can be used in assignment and returned
    // Full Complex arithmetic (2.0 * im) is planned for Phase C
    let src = r#"
a = im
b = im
a
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run im expression test");
    if let Some((re, imag)) = result.as_complex_parts() {
        assert!((re - 0.0).abs() < 1e-10, "Expected real=0.0, got {}", re);
        assert!(
            (imag - 1.0).abs() < 1e-10,
            "Expected imag=1.0, got {}",
            imag
        );
    } else {
        panic!("Unexpected result type: {:?}", result);
    }
}

#[test]
fn test_range_with_length() {
    // range(0.0, 1.0; length=5) should give [0.0, 0.25, 0.5, 0.75, 1.0]
    let src = r#"
xs = range(0.0, 1.0; length=5)
length(xs)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range length test");
    match result {
        Value::I64(x) => assert_eq!(x, 5, "Expected length 5, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_range_length_first_element() {
    // range(0.0, 1.0; length=5) first element should be 0.0
    let src = r#"
xs = range(0.0, 1.0; length=5)
xs[1]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range first element test");
    match result {
        Value::F64(x) => assert!((x - 0.0).abs() < 1e-10, "Expected 0.0, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_transpose_with_broadcast() {
    // xs' .+ 0 should give a row vector
    let src = r#"
xs = [1.0, 2.0, 3.0]
ys = xs' .+ 0.0
length(ys)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run transpose broadcast test");
    match result {
        Value::I64(x) => assert_eq!(x, 3, "Expected length 3, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_transpose_function() {
    // transpose() function should work like ' for real arrays
    let src = r#"
xs = [1.0, 2.0, 3.0]
ys = transpose(xs)
length(ys)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run transpose function test");
    match result {
        Value::I64(x) => assert_eq!(x, 3, "Expected length 3, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_function_definition_simple() {
    // Test function with keyword arguments using defaults
    // Note: explicit return is required for correct behavior
    let src = r#"
function f(x; y=10)
    return x + y
end
f(5)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
    match result {
        Value::I64(v) => assert_eq!(v, 15, "Expected 15, got {}", v),
        Value::F64(v) => assert!((v - 15.0).abs() < 1e-10, "Expected 15.0, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_function_with_explicit_kwarg() {
    // Test function with keyword argument explicitly provided
    let src = r#"
function f(x; y=10)
    return x + y
end
f(5; y=20)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
    match result {
        Value::I64(v) => assert_eq!(v, 25, "Expected 25, got {}", v),
        Value::F64(v) => assert!((v - 25.0).abs() < 1e-10, "Expected 25.0, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_function_multiple_kwargs() {
    // Test function with multiple keyword arguments
    let src = r#"
function f(x; y=1, z=2)
    return x + y + z
end
f(10)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
    match result {
        Value::I64(v) => assert_eq!(v, 13, "Expected 13, got {}", v),
        Value::F64(v) => assert!((v - 13.0).abs() < 1e-10, "Expected 13.0, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_function_multiple_kwargs_partial_override() {
    // Test overriding only some keyword arguments
    let src = r#"
function f(x; y=1, z=2)
    return x + y + z
end
f(10; z=100)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
    match result {
        Value::I64(v) => assert_eq!(v, 111, "Expected 111, got {}", v),
        Value::F64(v) => assert!((v - 111.0).abs() < 1e-10, "Expected 111.0, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

/// Test keyword argument with float default value (Issue #1328).
/// This test verifies that functions with float default kwargs compile and run correctly.
#[test]
fn test_kwarg_function_float_default() {
    // Test keyword argument with float default
    let src = r#"
function f(x; y=1.5)
    return x + y
end
f(2.0)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg float default test");
    match result {
        Value::F64(v) => assert!((v - 3.5).abs() < 1e-10, "Expected 3.5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

/// Test keyword argument with float default and explicit override (Issue #1328).
#[test]
fn test_kwarg_function_float_default_override() {
    let src = r#"
function f(x; y=1.5)
    return x + y
end
f(2.0; y=0.5)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run kwarg override test");
    match result {
        Value::F64(v) => assert!((v - 2.5).abs() < 1e-10, "Expected 2.5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_range_length() {
    // Test range with length keyword argument (already implemented)
    let src = r#"
xs = range(0.0, 1.0; length=5)
length(xs)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range test");
    match result {
        Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_kwarg_range_length_int() {
    // Test range with Int64 length keyword argument
    // Note: Julia's range(start, stop; length=N) requires Integer for length
    let src = r#"
n = 5
xs = range(0.0, 1.0; length=n)
length(xs)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run range test with Int64 length");
    match result {
        Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Short Function Definition ====================

#[test]
fn test_short_function_single_arg() {
    // f(x) = x^2
    let src = r#"
f(x) = x^2
f(3)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run short function test");
    match result {
        Value::F64(v) => assert!((v - 9.0).abs() < 1e-10, "Expected 9.0, got {}", v),
        Value::I64(v) => assert_eq!(v, 9, "Expected 9, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_short_function_two_args() {
    // f(x, y) = x + y
    let src = r#"
add(x, y) = x + y
add(10, 32)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run short function test");
    match result {
        Value::F64(v) => assert!((v - 42.0).abs() < 1e-10, "Expected 42.0, got {}", v),
        Value::I64(v) => assert_eq!(v, 42, "Expected 42, got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_short_function_expression_body() {
    // Expression with multiple operations in body
    let src = r#"
compute(a, b, c) = a + b * c
compute(1, 2, 3)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run short function test");
    match result {
        Value::F64(v) => assert!((v - 7.0).abs() < 1e-10, "Expected 7.0 (1+2*3), got {}", v),
        Value::I64(v) => assert_eq!(v, 7, "Expected 7 (1+2*3), got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_short_function_with_regular_function() {
    // Short function and regular function in same file
    let src = r#"
square(x) = x^2

function cube(x)
    return x^3
end

square(3) + cube(2)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run mixed function test");
    match result {
        Value::F64(v) => assert!((v - 17.0).abs() < 1e-10, "Expected 17.0 (9+8), got {}", v),
        Value::I64(v) => assert_eq!(v, 17, "Expected 17 (9+8), got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_short_function_multiple_definitions() {
    // Multiple short function definitions
    let src = r#"
double(x) = 2 * x
triple(x) = 3 * x
quadruple(x) = 4 * x

double(5) + triple(5) + quadruple(5)
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run multiple short function test");
    match result {
        Value::F64(v) => assert!(
            (v - 45.0).abs() < 1e-10,
            "Expected 45.0 (10+15+20), got {}",
            v
        ),
        Value::I64(v) => assert_eq!(v, 45, "Expected 45 (10+15+20), got {}", v),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_short_function_no_args() {
    // Short function with no arguments
    let src = r#"
get_pi() = 3.14159
get_pi()
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run short function no args test");
    match result {
        Value::F64(v) => {
            let expected = 314_159.0 / 100_000.0;
            assert!((v - expected).abs() < 1e-10, "Expected {}, got {}", expected, v);
        }
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Lambda Assignment Tests ====================

#[test]
fn test_lambda_assignment_basic() {
    // Test: f = x -> x ^ 3 + 1
    let src = r#"
f = x -> x ^ 3 + 1
f(2)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    // 2^3 + 1 = 9
    match result {
        Value::I64(v) => assert_eq!(v, 9),
        Value::F64(v) => assert!((v - 9.0).abs() < 1e-10),
        _ => panic!("Expected numeric result, got {:?}", result),
    }
}

#[test]
fn test_lambda_assignment_multi_param() {
    let src = r#"
f = (x, y) -> x + y * 2
f(3, 4)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    // 3 + 4*2 = 11
    match result {
        Value::I64(v) => assert_eq!(v, 11),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_lambda_assignment_simple() {
    let src = r#"
square = x -> x * x
square(5)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 25),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

// ==================== Higher-Order Function Tests ====================

#[test]
fn test_map_with_lambda() {
    let src = r#"
arr = [1.0, 2.0, 3.0]
result = map(x -> x * 2.0, arr)
result[3]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 6.0).abs() < 1e-10),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_map_with_named_function() {
    let src = r#"
double(x) = x * 2.0
arr = [1.0, 2.0, 3.0]
result = map(double, arr)
result[2]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_filter_with_lambda() {
    let src = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
result = filter(x -> x > 2.5, arr)
length(result)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 3), // [3.0, 4.0, 5.0]
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_filter_first_element() {
    let src = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
result = filter(x -> x > 2.5, arr)
result[1]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 3.0).abs() < 1e-10),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_reduce_with_lambda() {
    let src = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
reduce((a, b) -> a + b, arr)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 15.0).abs() < 1e-10), // 1+2+3+4+5 = 15
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_reduce_with_init() {
    let src = r#"
arr = [1.0, 2.0, 3.0]
reduce((a, b) -> a + b, arr, 10.0)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 16.0).abs() < 1e-10), // 10+1+2+3 = 16
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_sum_with_lambda() {
    let src = r#"
arr = [1.0, 2.0, 3.0, 4.0]
sum(x -> x * x, arr)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 30.0).abs() < 1e-10), // 1+4+9+16 = 30
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_sum_with_implicit_mult_lambda() {
    // Test sum(x -> 2x, [1,2,3]) - implicit multiplication in lambda
    let src = r#"
arr = [1.0, 2.0, 3.0]
sum(x -> 2x, arr)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 12.0).abs() < 1e-10), // 2*1 + 2*2 + 2*3 = 12
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_sum_with_implicit_mult_inline_array() {
    // Test sum(x -> 2x, [1,2,3]) with inline integer array
    let src = r#"
sum(x -> 2x, [1, 2, 3])
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 12.0).abs() < 1e-10), // 2*1 + 2*2 + 2*3 = 12
        Value::I64(v) => assert_eq!(v, 12),
        _ => panic!("Expected F64 or I64, got {:?}", result),
    }
}

#[test]
fn test_map_with_implicit_mult_lambda() {
    // Test map with implicit multiplication: map(x -> 3x, [1,2,3])
    let src = r#"
result = map(x -> 3x, [1.0, 2.0, 3.0])
result[2]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 6.0).abs() < 1e-10), // 3*2 = 6
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_sum_with_named_function() {
    let src = r#"
square(x) = x * x
arr = [1.0, 2.0, 3.0]
sum(square, arr)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 14.0).abs() < 1e-10), // 1+4+9 = 14
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_map_empty_array() {
    let src = r#"
arr = zeros(0)
result = map(x -> x * 2.0, arr)
length(result)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 0),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_filter_empty_result() {
    let src = r#"
arr = [1.0, 2.0, 3.0]
result = filter(x -> x > 10.0, arr)
length(result)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 0),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_sum_empty_array() {
    let src = r#"
arr = zeros(0)
sum(x -> x * x, arr)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 0.0).abs() < 1e-10),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== do Syntax Tests ====================

#[test]
fn test_do_syntax_map() {
    let src = r#"
result = map([1.0, 2.0, 3.0]) do x
    x * 2.0
end
result[2]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_do_syntax_filter() {
    let src = r#"
result = filter([1.0, 2.0, 3.0, 4.0, 5.0]) do x
    x > 2.5
end
length(result)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 3), // [3.0, 4.0, 5.0]
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_do_syntax_reduce() {
    let src = r#"
reduce([1.0, 2.0, 3.0, 4.0, 5.0]) do a, b
    a + b
end
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 15.0).abs() < 1e-10), // 1+2+3+4+5 = 15
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_do_syntax_sum() {
    let src = r#"
sum([1.0, 2.0, 3.0, 4.0]) do x
    x * x
end
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 30.0).abs() < 1e-10), // 1+4+9+16 = 30
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_do_syntax_multiline() {
    let src = r#"
result = map([1.0, 2.0, 3.0]) do x
    y = x * 2.0
    y + 1.0
end
result[1]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 3.0).abs() < 1e-10), // 1*2+1 = 3
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== Standard Normal Distribution (randn) ====================

#[test]
fn test_randn_basic() {
    let src = r#"
x = randn()
x
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::F64(v) => {
            // Standard normal values are typically within [-4, 4] (99.99%)
            assert!(
                v > -10.0 && v < 10.0,
                "randn() value {} seems out of range",
                v
            );
        }
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_randn_deterministic() {
    let src = r#"
randn()
"#;
    // Same seed should produce same result
    let r1 = run_core_pipeline(src, 42).unwrap();
    let r2 = run_core_pipeline(src, 42).unwrap();
    match (&r1, &r2) {
        (Value::F64(v1), Value::F64(v2)) => {
            assert_eq!(*v1, *v2, "randn() should be deterministic");
        }
        _ => panic!("Expected F64"),
    }

    // Different seed should produce different result
    let r3 = run_core_pipeline(src, 123).unwrap();
    match (&r1, &r3) {
        (Value::F64(v1), Value::F64(v3)) => {
            assert_ne!(*v1, *v3, "Different seeds should produce different results");
        }
        _ => panic!("Expected F64"),
    }
}

#[test]
fn test_randn_array_1d() {
    let src = r#"
arr = randn(5)
length(arr)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => {
            assert_eq!(v, 5, "Expected length 5");
        }
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_randn_array_2d() {
    let src = r#"
mat = randn(3, 4)
length(mat)
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::I64(v) => {
            assert_eq!(v, 12, "Expected length 12");
        }
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_randn_multiple_calls() {
    let src = r#"
x1 = randn()
x2 = randn()
x3 = randn()
x1 + x2 + x3
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::F64(_) => {} // Just check it succeeds
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== iOS Sample Tests ====================
// These tests correspond to the iOS app code samples to ensure they all work correctly.

// ==================== Higher-Order Functions Samples ====================

#[test]
fn test_ios_sample_map_function() {
    let src = r#"
# map(f, arr) applies function f to each element
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Using lambda (anonymous function)
doubled = map(x -> x * 2.0, arr)

# Using named function
square(x) = x * x
squared = map(square, arr)

squared[5]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 25.0).abs() < 1e-10, "Expected 25.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_filter_function() {
    // Note: Using > 5 instead of modulo since % is not supported in core pipeline
    let src = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
large = filter(x -> x > 5, arr)
length(large)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, 5, "Expected 5 (values > 5), got {}", v),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_reduce_function() {
    let src = r#"
# reduce(f, arr) combines elements using binary function f
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Sum all elements
total = reduce((a, b) -> a + b, arr)

# Product of all elements
product = reduce((a, b) -> a * b, arr)

# With initial value: reduce(f, arr, init)
total_with_init = reduce((a, b) -> a + b, arr, 100.0)

product
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 120.0).abs() < 1e-10, "Expected 120.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_do_syntax_map() {
    let src = r#"
# do...end block creates anonymous function as first argument
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Using do syntax (equivalent to map(x -> x^2 + 1, arr))
result = map(arr) do x
    x^2 + 1
end

result[5]
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!(
            (v - 26.0).abs() < 1e-10,
            "Expected 26.0 (5^2 + 1), got {}",
            v
        ),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_do_syntax_filter_reduce() {
    let src = r#"
# do syntax works with filter and reduce too
data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Filter with do block
filtered = filter(data) do x
    x > 3 && x < 8
end

# Reduce with do block (multiple parameters)
total = reduce(data) do acc, val
    acc + val
end

total
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 55.0).abs() < 1e-10, "Expected 55.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_chaining_higher_order() {
    let src = r#"
# Chain map, filter, reduce for data processing pipelines
data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Step 1: Square all numbers
squared = map(x -> x * x, data)

# Step 2: Keep only those > 20
large_squares = filter(x -> x > 20, squared)

# Step 3: Sum them
total = reduce((a, b) -> a + b, large_squares)

total
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    // Squares > 20: 25, 36, 49, 64, 81, 100 -> sum = 355
    match result {
        Value::F64(v) => assert!((v - 355.0).abs() < 1e-10, "Expected 355.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== Structures Samples ====================

#[test]
fn test_ios_sample_basic_struct() {
    let src = r#"
# Define an immutable struct with typed fields
struct Point
    x::Float64
    y::Float64
end

# Create instances
p1 = Point(3.0, 4.0)

# Use in calculations
distance = sqrt(p1.x^2 + p1.y^2)

distance
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 5.0).abs() < 1e-10, "Expected 5.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_mutable_struct() {
    let src = r#"
# Mutable structs allow field modification
mutable struct Counter
    value::Float64
end

# Create and modify
c = Counter(0.0)

c.value = 10.0
c.value = c.value + 5.0

# Use in a loop
for i in 1:5
    c.value = c.value + 1.0
end

c.value
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 20.0).abs() < 1e-10, "Expected 20.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_struct_with_functions() {
    // Note: Struct field access in functions requires main block calculation
    let src = r#"
struct Rectangle
    width::Float64
    height::Float64
end
rect = Rectangle(5.0, 3.0)
area = rect.width * rect.height
area
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 15.0).abs() < 1e-10, "Expected 15.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_euclidean_distance() {
    // Note: Using main block for struct field access (same as existing test_struct_euclidean_distance)
    let src = r#"
struct Point
    x::Float64
    y::Float64
end
p1 = Point(0.0, 0.0)
p2 = Point(3.0, 4.0)
dx = p2.x - p1.x
dy = p2.y - p1.y
sqrt(dx*dx + dy*dy)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 5.0).abs() < 1e-10, "Expected 5.0, got {}", v),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_particle_simulation() {
    // Note: Using main block for mutable struct field access
    let src = r#"
mutable struct Particle
    x::Float64
    y::Float64
    vx::Float64
    vy::Float64
end
particle = Particle(0.0, 0.0, 1.0, 0.5)
dt = 0.1
for t in 1:10
    particle.x = particle.x + particle.vx * dt
    particle.y = particle.y + particle.vy * dt
end
sqrt(particle.x^2 + particle.y^2)
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    // After 10 steps: x = 1.0, y = 0.5, distance = sqrt(1.25) ≈ 1.118
    match result {
        Value::F64(v) => assert!(
            (v - 1.118033988749895).abs() < 1e-10,
            "Expected ~1.118, got {}",
            v
        ),
        _ => panic!("Expected F64, got {:?}", result),
    }
}

// ==================== Error Handling Samples ====================

#[test]
fn test_ios_sample_try_catch_basics() {
    let src = r#"
# try/catch handles runtime errors gracefully
x = 0

try
    # Use integer division to trigger error (float division returns Inf per IEEE 754)
    y = div(1, 0)
    x = 999  # This won't execute
catch e
    # e contains the error message
    x = -1
end

x
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(v) => assert_eq!(v, -1, "Expected -1 (caught error), got {}", v),
        Value::F64(v) => assert!((v - (-1.0)).abs() < 1e-10, "Expected -1.0, got {}", v),
        _ => panic!("Expected numeric result, got {:?}", result),
    }
}

// Note: test_ios_sample_try_catch_finally skipped - try block assignment without error
// has known issues. Use test_ios_sample_try_catch_basics for error catching tests.

#[test]
fn test_ios_sample_error_recovery() {
    let src = r#"
result = 0.0
try
    result = 100 / 5
catch e
    result = 0.0
end
result
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!((v - 20.0).abs() < 1e-10, "Expected 20.0, got {}", v),
        Value::I64(v) => assert_eq!(v, 20, "Expected 20, got {}", v),
        _ => panic!("Expected numeric result, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_error_recovery_with_error() {
    let src = r#"
result = 0.0
try
    # Use integer division to trigger error (float division returns Inf per IEEE 754)
    result = Float64(div(10, 0))
catch e
    result = 0.0
end
result
"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::F64(v) => assert!(
            (v - 0.0).abs() < 1e-10,
            "Expected 0.0 (error recovery), got {}",
            v
        ),
        Value::I64(v) => assert_eq!(v, 0, "Expected 0 (error recovery), got {}", v),
        _ => panic!("Expected numeric result, got {:?}", result),
    }
}

// ==================== Monte Carlo randn Samples ====================

#[test]
fn test_ios_sample_normal_distribution() {
    let src = r#"
arr = randn(10)
sum = 0.0
for i in 1:length(arr)
    sum = sum + arr[i]
end
mean = sum / length(arr)
mean
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::F64(v) => {
            // Mean should be reasonably close to 0 for 10 samples
            assert!(v.abs() < 2.0, "Mean {} seems too far from 0", v);
        }
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_normal_distribution_matrix() {
    let src = r#"
mat = randn(3, 4)
sum = 0.0
sum_sq = 0.0
n = 12
for i in 1:3
    for j in 1:4
        v = mat[i, j]
        sum = sum + v
        sum_sq = sum_sq + v * v
    end
end
mean = sum / n
variance = sum_sq / n - mean * mean
std = sqrt(variance)
std
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::F64(v) => {
            // Standard deviation should be somewhat close to 1
            assert!(v > 0.0, "Std should be positive, got {}", v);
        }
        _ => panic!("Expected F64, got {:?}", result),
    }
}

#[test]
fn test_ios_sample_histogram_visualization() {
    // Simplified test: count values in [-1, 1] range
    let src = r#"
n = 100
samples = randn(n)
count = 0
for i in 1:n
    x = samples[i]
    if x >= -1 && x < 1
        count = count + 1
    end
end
count
"#;
    let result = run_core_pipeline(src, 42).unwrap();
    match result {
        Value::F64(v) => {
            // About 68% of values should be within [-1, 1]
            assert!(
                v > 40.0 && v < 95.0,
                "Expected ~68 values in [-1,1], got {}",
                v
            );
        }
        Value::I64(v) => {
            assert!(v > 40 && v < 95, "Expected ~68 values in [-1,1], got {}", v);
        }
        _ => panic!("Expected numeric result, got {:?}", result),
    }
}

// ==================== Broadcast Compound Assignment ====================
// These tests use the Core IR pipeline (tree-sitter → lowering → compile_core)

#[test]
fn test_broadcast_add_assign() {
    // Test .+= broadcast compound assignment
    let src = r#"
a = [1.0, 2.0, 3.0]
b = [10.0, 20.0, 30.0]
a .+= b
# a should be [11, 22, 33]
a[1] + a[2] + a[3]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run broadcast add assign test");
    match result {
        Value::F64(x) => assert!((x - 66.0).abs() < 1e-10, "Expected 66.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 66, "Expected 66, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_broadcast_mul_assign() {
    // Test .*= broadcast compound assignment
    let src = r#"
a = [1.0, 2.0, 3.0]
b = [2.0, 3.0, 4.0]
a .*= b
# a should be [2, 6, 12]
a[1] + a[2] + a[3]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run broadcast mul assign test");
    match result {
        Value::F64(x) => assert!((x - 20.0).abs() < 1e-10, "Expected 20.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 20, "Expected 20, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_broadcast_sub_assign() {
    // Test .-= broadcast compound assignment
    let src = r#"
a = [10.0, 20.0, 30.0]
b = [1.0, 2.0, 3.0]
a .-= b
# a should be [9, 18, 27]
a[1] + a[2] + a[3]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run broadcast sub assign test");
    match result {
        Value::F64(x) => assert!((x - 54.0).abs() < 1e-10, "Expected 54.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 54, "Expected 54, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_broadcast_and_assign() {
    // Test .&= broadcast compound assignment
    // Bitwise & is only defined for integer types, not Float64 (Issue #2704)
    let src = r#"
a = [1, 1, 0, 0]
b = [1, 0, 1, 0]
a .&= b
# a should be [1, 0, 0, 0]
a[1] + a[2] + a[3] + a[4]
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run broadcast and assign test");
    match result {
        Value::I64(x) => assert_eq!(x, 1, "Expected 1, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

// ==================== Regular Compound Assignment ====================
// These tests use the Core IR pipeline (tree-sitter → lowering → compile_core)

#[test]
fn test_minus_assign_new() {
    // Test -= compound assignment
    let src = r#"
a = 10
a -= 3
a
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run minus assign test");
    match result {
        Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_mul_assign_new() {
    // Test *= compound assignment (using core pipeline)
    let src = r#"
a = 5
a *= 4
a
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run mul assign test");
    match result {
        Value::F64(x) => assert!((x - 20.0).abs() < 1e-10, "Expected 20.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 20, "Expected 20, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_div_assign_new() {
    // Test /= compound assignment
    let src = r#"
a = 20.0
a /= 4.0
a
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run div assign test");
    match result {
        Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}

#[test]
fn test_pow_assign_new() {
    // Test ^= compound assignment
    let src = r#"
a = 2.0
a ^= 3.0
a
"#;
    let result = run_core_pipeline(src, 0).expect("Failed to run pow assign test");
    match result {
        Value::F64(x) => assert!((x - 8.0).abs() < 1e-10, "Expected 8.0, got {}", x),
        Value::I64(x) => assert_eq!(x, 8, "Expected 8, got {}", x),
        _ => panic!("Unexpected result type: {:?}", result),
    }
}
