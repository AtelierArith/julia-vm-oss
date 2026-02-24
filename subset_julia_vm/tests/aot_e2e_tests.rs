//! AoT End-to-End Tests for Phase 1
//!
//! These tests verify that the AoT compiler correctly compiles Julia source code
//! to Rust code and that the type inference produces correct results.

#![cfg(feature = "aot")]

use subset_julia_vm::aot::analyze::program_to_aot_ir;
use subset_julia_vm::aot::codegen::aot_codegen::AotCodeGenerator;
use subset_julia_vm::aot::codegen::CodegenConfig;
use subset_julia_vm::aot::inference::TypeInferenceEngine;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;

/// Helper function to compile Julia source to Rust code
fn compile_to_rust(source: &str) -> Result<String, String> {
    // Parse source
    let mut parser = Parser::new().map_err(|e| format!("Parser error: {:?}", e))?;
    let outcome = parser
        .parse(source)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Lower to Core IR
    let mut lowering = Lowering::new(source);
    let program = lowering
        .lower(outcome)
        .map_err(|e| format!("Lowering error: {:?}", e))?;

    // Type inference
    let mut type_engine = TypeInferenceEngine::new();
    let typed_program = type_engine
        .analyze_program(&program)
        .map_err(|e| format!("Type inference error: {:?}", e))?;

    // Convert Core IR to AoT IR
    let aot_program = program_to_aot_ir(&program, &typed_program)
        .map_err(|e| format!("IR conversion error: {:?}", e))?;

    // Generate Rust code
    let config = CodegenConfig::default();
    let mut codegen = AotCodeGenerator::new(config);
    codegen
        .generate_program(&aot_program)
        .map_err(|e| format!("Codegen error: {:?}", e))
}

// ============================================================================
// Arithmetic Literal Tests
// ============================================================================

#[test]
fn test_aot_e2e_arithmetic_literal() {
    let source = "1 + 2 * 3";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify the generated code contains the expected structure
    assert!(
        rust_code.contains("pub fn main()"),
        "Generated code should contain main function"
    );
}

#[test]
fn test_aot_e2e_arithmetic_with_parentheses() {
    let source = "(1 + 2) * 3";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_float_arithmetic() {
    let source = "3.14 * 2.0";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify float literals are present
    assert!(
        rust_code.contains("f64") || rust_code.contains("3.14"),
        "Generated code should contain float operations"
    );
}

// ============================================================================
// Typed Function Tests
// ============================================================================

#[test]
fn test_aot_e2e_typed_function() {
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end
add(10, 5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify function definition is present
    assert!(
        rust_code.contains("fn add"),
        "Generated code should contain add function"
    );
    assert!(
        rust_code.contains("i64"),
        "Generated code should contain i64 types"
    );
}

#[test]
fn test_aot_e2e_function_with_return() {
    let source = r#"
function double(x::Int64)::Int64
    return x * 2
end
double(21)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn double"),
        "Generated code should contain double function"
    );
    assert!(
        rust_code.contains("return"),
        "Generated code should contain return statement"
    );
}

#[test]
fn test_aot_e2e_untyped_function() {
    let source = r#"
function add(x, y)
    x + y
end
add(10, 5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Variable Assignment Tests
// ============================================================================

#[test]
fn test_aot_e2e_variable_assignment() {
    let source = r#"
x = 10
y = x + 5
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify main function is present (variables are in main block)
    assert!(
        rust_code.contains("pub fn main()"),
        "Generated code should contain main function"
    );
}

#[test]
fn test_aot_e2e_multiple_assignments() {
    let source = r#"
a = 1
b = 2
c = a + b
d = c * 2
d
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reassignment() {
    let source = r#"
x = 10
x = x + 1
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// If Statement Tests
// ============================================================================

#[test]
fn test_aot_e2e_if_statement() {
    let source = r#"
x = 5
if x > 0
    1
else
    -1
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("if"),
        "Generated code should contain if statement"
    );
}

#[test]
fn test_aot_e2e_if_elseif_else() {
    let source = r#"
x = 0
if x > 0
    1
elseif x < 0
    -1
else
    0
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_if() {
    let source = r#"
x = 10
y = 20
if x > 0
    if y > 0
        1
    else
        2
    end
else
    3
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// For Loop Tests
// ============================================================================

#[test]
fn test_aot_e2e_for_loop() {
    let source = r#"
sum = 0
for i in 1:10
    sum = sum + i
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("for"),
        "Generated code should contain for loop"
    );
}

#[test]
fn test_aot_e2e_for_loop_with_step() {
    let source = r#"
sum = 0
for i in 1:2:10
    sum = sum + i
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_for_loop() {
    let source = r#"
sum = 0
for i in 1:3
    for j in 1:3
        sum = sum + i * j
    end
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// While Loop Tests
// ============================================================================

#[test]
fn test_aot_e2e_while_loop() {
    let source = r#"
x = 0
while x < 10
    x = x + 1
end
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("while"),
        "Generated code should contain while loop"
    );
}

// ============================================================================
// Combined Tests
// ============================================================================

#[test]
fn test_aot_e2e_function_with_loop() {
    let source = r#"
function factorial(n::Int64)::Int64
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end
factorial(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn factorial"),
        "Generated code should contain factorial function"
    );
}

#[test]
fn test_aot_e2e_function_with_conditional() {
    let source = r#"
function abs_value(x::Int64)::Int64
    if x < 0
        return -x
    else
        return x
    end
end
abs_value(-5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_recursive_function() {
    let source = r#"
function fib(n::Int64)::Int64
    if n <= 1
        return n
    else
        return fib(n - 1) + fib(n - 2)
    end
end
fib(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Type Inference Tests
// ============================================================================

#[test]
fn test_aot_e2e_type_inference_int() {
    let source = "x = 42";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("i64") || rust_code.contains("42"),
        "Generated code should infer integer type"
    );
}

#[test]
fn test_aot_e2e_type_inference_float() {
    let source = "x = 3.14";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("f64") || rust_code.contains("3.14"),
        "Generated code should infer float type"
    );
}

#[test]
fn test_aot_e2e_type_inference_bool() {
    let source = "x = true";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("bool") || rust_code.contains("true"),
        "Generated code should infer boolean type"
    );
}

#[test]
fn test_aot_e2e_type_inference_string() {
    let source = r#"x = "hello""#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_aot_e2e_empty_function() {
    let source = r#"
function empty()
    nothing
end
empty()
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_single_expression() {
    let source = "42";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_boolean_operators() {
    let source = "true && false || true";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_comparison_operators() {
    let source = "1 < 2 && 3 >= 2 && 4 != 5";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_unary_operators() {
    let source = r#"
x = 5
y = -x
z = !true
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Code Generation Quality Tests
// ============================================================================

#[test]
fn test_aot_e2e_code_has_header() {
    let source = "1 + 2";
    let result = compile_to_rust(source).unwrap();

    assert!(
        result.contains("Auto-generated"),
        "Generated code should have header comment"
    );
}

#[test]
fn test_aot_e2e_code_has_allow_attributes() {
    let source = "1 + 2";
    let result = compile_to_rust(source).unwrap();

    assert!(
        result.contains("#![allow(unused_variables)]"),
        "Generated code should have #![allow] attributes"
    );
}

#[test]
fn test_aot_e2e_multiple_functions() {
    let source = r#"
function add(x, y)
    x + y
end

function sub(x, y)
    x - y
end

function mul(x, y)
    x * y
end

add(1, 2) + sub(5, 3) + mul(2, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(rust_code.contains("fn add"));
    assert!(rust_code.contains("fn sub"));
    assert!(rust_code.contains("fn mul"));
}

// ============================================================================
// Static Function Call Tests
// ============================================================================

#[test]
fn test_aot_e2e_static_function_call_basic() {
    let source = r#"
function square(x::Int64)::Int64
    x * x
end
square(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn square"),
        "Generated code should contain square function"
    );
    assert!(
        rust_code.contains("square("),
        "Generated code should contain function call"
    );
}

#[test]
fn test_aot_e2e_static_function_call_multiple_args() {
    let source = r#"
function add3(a::Int64, b::Int64, c::Int64)::Int64
    a + b + c
end
add3(1, 2, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn add3"),
        "Generated code should contain add3 function"
    );
    assert!(
        rust_code.contains("add3("),
        "Generated code should contain function call with multiple args"
    );
}

#[test]
fn test_aot_e2e_static_function_call_nested() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

function quadruple(x::Int64)::Int64
    double(double(x))
end

quadruple(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn double"),
        "Generated code should contain double function"
    );
    assert!(
        rust_code.contains("fn quadruple"),
        "Generated code should contain quadruple function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_in_expression() {
    let source = r#"
function inc(x::Int64)::Int64
    x + 1
end

function dec(x::Int64)::Int64
    x - 1
end

inc(5) + dec(10) * 2
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn inc"),
        "Generated code should contain inc function"
    );
    assert!(
        rust_code.contains("fn dec"),
        "Generated code should contain dec function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_recursive_fib() {
    let source = r#"
function fib(n::Int64)::Int64
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

fib(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn fib"),
        "Generated code should contain fib function"
    );
    // Check for recursive calls
    assert!(
        rust_code.contains("fib(n - 1)")
            || rust_code.contains("fib(n -")
            || rust_code.matches("fib(").count() >= 3,
        "Generated code should contain recursive calls"
    );
}

#[test]
fn test_aot_e2e_static_function_call_mutual_recursion() {
    let source = r#"
function is_even(n::Int64)::Bool
    if n == 0
        return true
    end
    return is_odd(n - 1)
end

function is_odd(n::Int64)::Bool
    if n == 0
        return false
    end
    return is_even(n - 1)
end

is_even(4)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn is_even"),
        "Generated code should contain is_even function"
    );
    assert!(
        rust_code.contains("fn is_odd"),
        "Generated code should contain is_odd function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_float_typed() {
    let source = r#"
function average(a::Float64, b::Float64)::Float64
    (a + b) / 2.0
end

average(3.0, 5.0)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn average"),
        "Generated code should contain average function"
    );
    assert!(
        rust_code.contains("f64"),
        "Generated code should contain f64 type"
    );
}

#[test]
fn test_aot_e2e_static_function_call_mixed_types() {
    let source = r#"
function scale(x::Int64, factor::Float64)::Float64
    x * factor
end

scale(10, 2.5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn scale"),
        "Generated code should contain scale function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_in_loop() {
    let source = r#"
function square(x::Int64)::Int64
    x * x
end

sum = 0
for i in 1:5
    sum = sum + square(i)
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn square"),
        "Generated code should contain square function"
    );
    assert!(
        rust_code.contains("for"),
        "Generated code should contain for loop"
    );
}

#[test]
fn test_aot_e2e_static_function_call_in_conditional() {
    let source = r#"
function abs(x::Int64)::Int64
    if x < 0
        return -x
    end
    return x
end

function sign(x::Int64)::Int64
    if abs(x) == 0
        return 0
    elseif x > 0
        return 1
    else
        return -1
    end
end

sign(-5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn abs") || rust_code.contains("fn abs_"),
        "Generated code should contain abs function"
    );
    assert!(
        rust_code.contains("fn sign"),
        "Generated code should contain sign function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_chain() {
    let source = r#"
function step1(x::Int64)::Int64
    x + 1
end

function step2(x::Int64)::Int64
    x * 2
end

function step3(x::Int64)::Int64
    x - 3
end

step3(step2(step1(10)))
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn step1"),
        "Generated code should contain step1 function"
    );
    assert!(
        rust_code.contains("fn step2"),
        "Generated code should contain step2 function"
    );
    assert!(
        rust_code.contains("fn step3"),
        "Generated code should contain step3 function"
    );
}

#[test]
fn test_aot_e2e_static_function_with_local_vars() {
    let source = r#"
function compute(a::Int64, b::Int64)::Int64
    temp = a * 2
    result = temp + b
    return result
end

compute(5, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn compute"),
        "Generated code should contain compute function"
    );
    assert!(
        rust_code.contains("let"),
        "Generated code should contain local variable declarations"
    );
}

#[test]
fn test_aot_e2e_static_function_bool_return() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

function is_negative(x::Int64)::Bool
    x < 0
end

is_positive(5) && is_negative(-3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn is_positive"),
        "Generated code should contain is_positive function"
    );
    assert!(
        rust_code.contains("fn is_negative"),
        "Generated code should contain is_negative function"
    );
    assert!(
        rust_code.contains("bool"),
        "Generated code should contain bool type"
    );
}

#[test]
fn test_aot_e2e_static_function_early_return() {
    let source = r#"
function find_first_positive(a::Int64, b::Int64, c::Int64)::Int64
    if a > 0
        return a
    end
    if b > 0
        return b
    end
    if c > 0
        return c
    end
    return 0
end

find_first_positive(-1, 5, 10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn find_first_positive"),
        "Generated code should contain find_first_positive function"
    );
    assert!(
        rust_code.matches("return").count() >= 4,
        "Generated code should contain multiple return statements"
    );
}

// ============================================================================
// Multiple Dispatch Tests
// ============================================================================

#[test]
fn test_aot_e2e_multiple_dispatch_two_methods() {
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end

function add(x::Float64, y::Float64)::Float64
    x + y
end

add(1, 2)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have mangled function names
    assert!(
        rust_code.contains("add_i64_i64") || rust_code.contains("fn add"),
        "Generated code should contain mangled function names for dispatch"
    );
}

#[test]
fn test_aot_e2e_multiple_dispatch_three_methods() {
    let source = r#"
function compute(x::Int64)::Int64
    x * 2
end

function compute(x::Float64)::Float64
    x * 2.0
end

function compute(x::Bool)::Int64
    if x
        1
    else
        0
    end
end

compute(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Check that multiple dispatch comment is generated
    assert!(
        rust_code.contains("compute") && rust_code.contains("fn"),
        "Generated code should contain compute function definitions"
    );
}

#[test]
fn test_aot_e2e_multiple_dispatch_mixed_arg_count() {
    let source = r#"
function process(x::Int64)::Int64
    x
end

function process(x::Int64, y::Int64)::Int64
    x + y
end

process(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_multiple_dispatch_with_call() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

function double(x::Float64)::Float64
    x * 2.0
end

a = double(5)
b = double(3.14)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify both calls are present
    assert!(
        rust_code.contains("double"),
        "Generated code should contain double function calls"
    );
}

#[test]
fn test_aot_e2e_multiple_dispatch_nested_calls() {
    let source = r#"
function transform(x::Int64)::Int64
    x + 1
end

function transform(x::Float64)::Float64
    x + 1.0
end

function apply_twice(x::Int64)::Int64
    transform(transform(x))
end

apply_twice(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_single_function_no_dispatch() {
    let source = r#"
function single(x::Int64, y::Int64)::Int64
    x + y
end

single(1, 2)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Single function should NOT have mangled name
    assert!(
        rust_code.contains("fn single("),
        "Single method function should use original name"
    );
}

#[test]
fn test_aot_e2e_dispatch_in_expression() {
    let source = r#"
function negate(x::Int64)::Int64
    -x
end

function negate(x::Float64)::Float64
    -x
end

result = negate(5) + negate(3)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_dispatch_with_different_return_types() {
    let source = r#"
function convert(x::Int64)::Float64
    x * 1.0
end

function convert(x::Float64)::Int64
    x
end

convert(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Higher-Order Function Tests
// ============================================================================

#[test]
fn test_aot_e2e_map_with_function() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

arr = [1, 2, 3, 4, 5]
result = map(double, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("map") || rust_code.contains("iter"),
        "Generated code should contain map or iter operation"
    );
}

#[test]
fn test_aot_e2e_filter_with_function() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

arr = [-2, -1, 0, 1, 2]
result = filter(is_positive, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("filter") || rust_code.contains("iter"),
        "Generated code should contain filter operation"
    );
}

#[test]
fn test_aot_e2e_reduce_with_function() {
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end

arr = [1, 2, 3, 4, 5]
result = reduce(add, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("reduce") || rust_code.contains("fold"),
        "Generated code should contain reduce or fold operation"
    );
}

#[test]
fn test_aot_e2e_foreach_with_function() {
    let source = r#"
function print_item(x::Int64)::Nothing
    println(x)
    nothing
end

arr = [1, 2, 3]
foreach(print_item, arr)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_any_with_function() {
    let source = r#"
function is_even(x::Int64)::Bool
    x % 2 == 0
end

arr = [1, 3, 5, 7, 8]
result = any(is_even, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("any") || rust_code.contains("iter"),
        "Generated code should contain any operation"
    );
}

#[test]
fn test_aot_e2e_all_with_function() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

arr = [1, 2, 3, 4, 5]
result = all(is_positive, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("all") || rust_code.contains("iter"),
        "Generated code should contain all operation"
    );
}

#[test]
fn test_aot_e2e_map_chain() {
    let source = r#"
function inc(x::Int64)::Int64
    x + 1
end

function double(x::Int64)::Int64
    x * 2
end

arr = [1, 2, 3]
result = map(double, map(inc, arr))
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_filter_then_map() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

function square(x::Int64)::Int64
    x * x
end

arr = [-2, -1, 0, 1, 2, 3]
filtered = filter(is_positive, arr)
result = map(square, filtered)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reduce_sum() {
    let source = r#"
function add(a::Int64, b::Int64)::Int64
    a + b
end

arr = [1, 2, 3, 4, 5]
total = reduce(add, arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reduce_product() {
    let source = r#"
function mul(a::Int64, b::Int64)::Int64
    a * b
end

arr = [1, 2, 3, 4, 5]
product = reduce(mul, arr)
product
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_hof_with_float_array() {
    let source = r#"
function halve(x::Float64)::Float64
    x / 2.0
end

arr = [2.0, 4.0, 6.0, 8.0]
result = map(halve, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_sum_builtin() {
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
total = sum(arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("sum"),
        "Generated code should contain sum operation"
    );
}

// ============================================================================
// Closure Tests (Phase 3)
// ============================================================================

#[test]
fn test_aot_e2e_simple_lambda() {
    // Simple single-parameter lambda: x -> x + 1
    // Note: In the current implementation, lambdas assigned to variables
    // are lowered to named functions, not Rust closures.
    let source = r#"
f = x -> x + 1
result = f(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify the lambda was converted to a function named 'f'
    assert!(
        rust_code.contains("fn f("),
        "Generated code should contain function f: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_two_param_lambda() {
    // Two-parameter lambda: (x, y) -> x + y
    let source = r#"
add = (x, y) -> x + y
result = add(3, 4)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_with_multiplication() {
    // Lambda with multiplication: x -> x * 2
    let source = r#"
double = x -> x * 2
result = double(10)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_with_power() {
    // Lambda with power operation: x -> x ^ 2
    let source = r#"
square = x -> x ^ 2
result = square(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_returning_bool() {
    // Lambda returning boolean: x -> x > 0
    let source = r#"
is_positive = x -> x > 0
result = is_positive(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_map_with_lambda() {
    // map with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5]
squared = map(x -> x ^ 2, arr)
squared
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("map") && rust_code.contains("|"),
        "Generated code should contain map with closure"
    );
}

#[test]
fn test_aot_e2e_filter_with_lambda() {
    // filter with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
evens = filter(x -> x % 2 == 0, arr)
evens
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("filter"),
        "Generated code should contain filter"
    );
}

#[test]
fn test_aot_e2e_any_with_lambda() {
    // any with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5]
has_three = any(x -> x == 3, arr)
has_three
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_all_with_lambda() {
    // all with inline lambda
    let source = r#"
arr = [2, 4, 6, 8]
all_even = all(x -> x % 2 == 0, arr)
all_even
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reduce_with_lambda() {
    // reduce with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5]
total = reduce((a, b) -> a + b, arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_complex_body() {
    // Lambda with more complex expression body
    let source = r#"
normalize = x -> (x - 5) / 10
result = normalize(15)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_lambda_calls() {
    // Using lambda result with another lambda
    let source = r#"
double = x -> x * 2
triple = x -> x * 3
result = triple(double(5))
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_with_float() {
    // Lambda with float operations
    let source = r#"
half = x -> x / 2.0
result = half(10.0)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_map_float_lambda() {
    // map with float lambda
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0]
doubled = map(x -> x * 2.0, arr)
doubled
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_chained_hof_with_lambda() {
    // Chained HOF operations with lambdas
    let source = r#"
arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
evens = filter(x -> x % 2 == 0, arr)
doubled = map(x -> x * 2, evens)
doubled
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Typed Array Tests (Phase 4)
// ============================================================================

#[test]
fn test_aot_e2e_array_literal_int() {
    // Integer array literal
    let source = r#"
arr = [1, 2, 3, 4, 5]
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec!["),
        "Generated code should contain vec! macro: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_literal_float() {
    // Float array literal
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("f64"),
        "Generated code should contain f64 type"
    );
}

#[test]
fn test_aot_e2e_array_index_get() {
    // Array indexing (get element)
    let source = r#"
arr = [10, 20, 30, 40, 50]
x = arr[3]
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have 1-based to 0-based index conversion
    assert!(
        rust_code.contains("- 1]") || rust_code.contains("-1]"),
        "Generated code should contain index conversion: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_index_set() {
    // Array indexing (set element)
    let source = r#"
arr = [10, 20, 30, 40, 50]
arr[3] = 100
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_length() {
    // Array length
    let source = r#"
arr = [1, 2, 3, 4, 5]
n = length(arr)
n
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".len()"),
        "Generated code should contain .len() method"
    );
}

#[test]
fn test_aot_e2e_array_push() {
    // push! operation
    let source = r#"
arr = [1, 2, 3]
push!(arr, 4)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".push("),
        "Generated code should contain .push() method"
    );
}

#[test]
fn test_aot_e2e_array_pop() {
    // pop! operation
    let source = r#"
arr = [1, 2, 3]
x = pop!(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".pop()"),
        "Generated code should contain .pop() method"
    );
}

#[test]
fn test_aot_e2e_array_first() {
    // first() operation
    let source = r#"
arr = [10, 20, 30]
x = first(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("[0]"),
        "Generated code should access index 0 for first: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_last() {
    // last() operation
    let source = r#"
arr = [10, 20, 30]
x = last(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".len() - 1]"),
        "Generated code should access last index: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_isempty() {
    // isempty() operation
    let source = r#"
arr = [1, 2, 3]
empty = isempty(arr)
empty
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".is_empty()"),
        "Generated code should contain .is_empty() method"
    );
}

#[test]
fn test_aot_e2e_array_insert() {
    // insert! operation
    let source = r#"
arr = [1, 2, 4, 5]
insert!(arr, 3, 3)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".insert("),
        "Generated code should contain .insert() method"
    );
}

#[test]
fn test_aot_e2e_array_deleteat() {
    // deleteat! operation
    let source = r#"
arr = [1, 2, 3, 4, 5]
deleteat!(arr, 3)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".remove("),
        "Generated code should contain .remove() method"
    );
}

#[test]
fn test_aot_e2e_array_append() {
    // append! operation
    let source = r#"
arr1 = [1, 2, 3]
arr2 = [4, 5, 6]
append!(arr1, arr2)
arr1
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".extend("),
        "Generated code should contain .extend() method"
    );
}

#[test]
fn test_aot_e2e_array_zeros() {
    // zeros() operation
    let source = r#"
arr = zeros(5)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec![0.0_f64;"),
        "Generated code should contain vec![0.0_f64;...]: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_ones() {
    // ones() operation
    let source = r#"
arr = ones(5)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec![1.0_f64;"),
        "Generated code should contain vec![1.0_f64;...]: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_fill() {
    // fill() operation — fill is now Pure Julia (Issue #2640),
    // so the generated code emits a function call, not vec!
    let source = r#"
arr = fill(42, 5)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fill("),
        "Generated code should contain fill function call: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_sum() {
    // sum() operation
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
total = sum(arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".sum::<"),
        "Generated code should contain .sum::<>(): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_multiple_operations() {
    // Multiple array operations in sequence
    let source = r#"
arr = [1, 2, 3]
push!(arr, 4)
push!(arr, 5)
n = length(arr)
n
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_in_function() {
    // Array operations inside a function
    let source = r#"
function sum_array(arr::Array{Int64,1})::Int64
    total = 0
    for x in arr
        total = total + x
    end
    total
end

arr = [1, 2, 3, 4, 5]
result = sum_array(arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_empty_check() {
    // Check if array is empty in conditional
    let source = r#"
arr = [1, 2, 3]
if isempty(arr)
    0
else
    first(arr)
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_pushfirst() {
    // pushfirst! operation
    let source = r#"
arr = [2, 3, 4]
pushfirst!(arr, 1)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".insert(0,"),
        "Generated code should contain .insert(0,...) for pushfirst"
    );
}

#[test]
fn test_aot_e2e_array_popfirst() {
    // popfirst! operation
    let source = r#"
arr = [1, 2, 3, 4]
x = popfirst!(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".remove(0)"),
        "Generated code should contain .remove(0) for popfirst"
    );
}

// ============================================================================
// Multidimensional Array Tests (Phase 5)
// ============================================================================

#[test]
fn test_aot_e2e_matrix_zeros_2d() {
    // zeros(m, n) creates a 2D matrix
    let source = r#"
mat = zeros(3, 4)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should generate nested Vec for 2D array
    assert!(
        rust_code.contains("map(|_|") || rust_code.contains("collect::<Vec<_>>()"),
        "Generated code should create 2D Vec structure: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_ones_2d() {
    // ones(m, n) creates a 2D matrix of ones
    let source = r#"
mat = ones(2, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("1.0")
            && (rust_code.contains("map(|_|") || rust_code.contains("collect")),
        "Generated code should create 2D ones matrix: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_fill_2d() {
    // fill(value, m, n) creates a 2D matrix filled with value
    // fill is now Pure Julia (Issue #2640), so it generates a function call
    let source = r#"
mat = fill(5, 2, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fill(") && rust_code.contains("5"),
        "Generated code should contain fill function call: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_shape_preserved() {
    // Array shape should be preserved in IR
    let source = r#"
arr = [1, 2, 3, 4, 5]
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec!["),
        "1D array should generate vec!: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_size_1d() {
    // size() for 1D array
    let source = r#"
arr = [1, 2, 3, 4, 5]
s = size(arr)
s
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".len()"),
        "size() should use .len() for 1D arrays: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_zeros_comparison() {
    // Compare 1D zeros vs 2D zeros generation
    let source = r#"
arr1d = zeros(5)
arr1d
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "1D zeros failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // 1D should be simple vec!
    assert!(
        rust_code.contains("vec![0.0_f64;"),
        "1D zeros should be vec![0.0_f64;...]: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_function_with_2d() {
    // Function that takes a 2D matrix parameter
    let source = r#"
function process_matrix()
    mat = zeros(2, 2)
    mat
end
process_matrix()
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_vec_generation() {
    // Verify nested Vec generation for 2D
    let source = r#"
mat = zeros(3, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should create nested Vec<Vec<_>> structure
    assert!(
        rust_code.contains("Vec<_>") || rust_code.contains("collect"),
        "2D arrays should generate Vec of Vecs: {}",
        rust_code
    );
}

// ============================================================================
// Struct Tests (Phase 4)
// ============================================================================

#[test]
fn test_aot_e2e_struct_definition_immutable() {
    // Immutable struct definition and instantiation
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should define the struct
    assert!(
        rust_code.contains("pub struct Point"),
        "Generated code should define struct Point: {}",
        rust_code
    );
    // Should have fields
    assert!(
        rust_code.contains("x: f64") || rust_code.contains("pub x:"),
        "Generated code should have field x: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_instantiation() {
    // Struct constructor call
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(3.0, 4.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should call constructor
    assert!(
        rust_code.contains("Point::new(") || rust_code.contains("Point {"),
        "Generated code should instantiate Point: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_field_access() {
    // Field access (read)
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
x_val = p.x
x_val
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should access field
    assert!(
        rust_code.contains("p.x"),
        "Generated code should access p.x: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_mutable_struct() {
    // Mutable struct with field modification
    let source = r#"
mutable struct Counter
    count::Int64
end

c = Counter(0)
c
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should define the struct
    assert!(
        rust_code.contains("struct Counter"),
        "Generated code should define Counter struct: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_mutable_struct_field_write() {
    // Mutable struct field assignment
    let source = r#"
mutable struct Counter
    count::Int64
end

c = Counter(0)
c.count = 10
c
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should modify field
    assert!(
        rust_code.contains("c.count = ") || rust_code.contains("c.count="),
        "Generated code should assign to c.count: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_in_function() {
    // Struct used in function
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

function distance(p::Point)::Float64
    sqrt(p.x * p.x + p.y * p.y)
end

p = Point(3.0, 4.0)
d = distance(p)
d
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have function with Point parameter
    assert!(
        rust_code.contains("fn distance"),
        "Generated code should define distance function: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_constructor() {
    // Struct with new() constructor generated
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(5.0, 12.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have impl block with new
    assert!(
        rust_code.contains("impl Point") && rust_code.contains("fn new"),
        "Generated code should have impl Point with new(): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_with_int_fields() {
    // Struct with integer fields
    let source = r#"
struct Rectangle
    width::Int64
    height::Int64
end

r = Rectangle(10, 20)
r
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("i64"),
        "Generated code should have i64 type for Int64 fields: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_derive_traits() {
    // Struct should derive Debug and Clone
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("#[derive(Debug, Clone)]"),
        "Generated code should derive Debug and Clone: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_multiple_instances() {
    // Multiple struct instances
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)
p1
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_struct_field_in_expression() {
    // Struct fields used in expressions
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(3.0, 4.0)
sum = p.x + p.y
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("p.x") && rust_code.contains("p.y"),
        "Generated code should access both fields: {}",
        rust_code
    );
}

// ============================================================================
// Tuple Tests (Phase 4)
// ============================================================================

#[test]
fn test_aot_e2e_tuple_literal_basic() {
    // Basic tuple literal
    let source = r#"
t = (1, 2, 3)
t
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("(") && rust_code.contains(")"),
        "Generated code should contain tuple literal: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_single_element() {
    // Single element tuple (trailing comma)
    let source = r#"
t = (42,)
t
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Single element tuple should have trailing comma in Rust too
    assert!(
        rust_code.contains(",)"),
        "Generated code should have trailing comma for single-element tuple: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_heterogeneous() {
    // Tuple with different types
    let source = r#"
t = (1, 3.14)
t
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_tuple_index_access() {
    // Tuple indexing should use .0, .1 syntax
    let source = r#"
t = (10, 20, 30)
x = t[1]
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Julia t[1] should become Rust t.0
    assert!(
        rust_code.contains(".0"),
        "Generated code should use tuple field access (.0): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_index_second() {
    // Tuple indexing second element
    let source = r#"
t = (10, 20, 30)
y = t[2]
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Julia t[2] should become Rust t.1
    assert!(
        rust_code.contains(".1"),
        "Generated code should use tuple field access (.1): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_index_third() {
    // Tuple indexing third element
    let source = r#"
t = (10, 20, 30)
z = t[3]
z
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Julia t[3] should become Rust t.2
    assert!(
        rust_code.contains(".2"),
        "Generated code should use tuple field access (.2): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_in_expression() {
    // Tuple elements used in expression
    let source = r#"
t = (3, 4)
sum = t[1] + t[2]
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".0") && rust_code.contains(".1"),
        "Generated code should access both tuple elements: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_multiple() {
    // Multiple tuples
    let source = r#"
t1 = (1, 2)
t2 = (3, 4)
t1
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_tuple_in_function_return() {
    // Function returning a tuple
    let source = r#"
function make_pair(a::Int64, b::Int64)
    (a, b)
end

p = make_pair(1, 2)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_tuple_with_float() {
    // Tuple with float values
    let source = r#"
t = (1.0, 2.0, 3.0)
x = t[1]
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Type Specialization Tests (Issue #1018)
// ============================================================================

#[test]
fn test_aot_e2e_type_specialization_int64() {
    // Fully typed function should generate specialized version
    let source = r#"
function add_nums(x::Int64, y::Int64)::Int64
    x + y
end

result = add_nums(1, 2)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    // Should generate a function with mangled name containing type info
    assert!(
        code.contains("add_nums_i64_i64") || code.contains("fn add_nums"),
        "Expected typed function in generated code"
    );
}

#[test]
fn test_aot_e2e_type_specialization_float64() {
    // Float64 typed function
    let source = r#"
function multiply(x::Float64, y::Float64)::Float64
    x * y
end

result = multiply(2.0, 3.0)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    assert!(
        code.contains("multiply") && (code.contains("f64") || code.contains("Float64")),
        "Expected float64 typed function"
    );
}

#[test]
fn test_aot_e2e_type_specialization_bool() {
    // Bool typed function
    let source = r#"
function negate(x::Bool)::Bool
    !x
end

result = negate(true)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_mixed_types() {
    // Function with mixed parameter types
    let source = r#"
function scale(x::Int64, factor::Float64)::Float64
    x * factor
end

result = scale(5, 2.0)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_multiple_functions() {
    // Multiple typed functions with same name different signatures
    let source = r#"
function compute(x::Int64)::Int64
    x * 2
end

function compute(x::Float64)::Float64
    x * 2.0
end

a = compute(5)
b = compute(3.0)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    // Should have two specialized versions
    assert!(
        code.contains("compute"),
        "Expected compute function in generated code"
    );
}

#[test]
fn test_aot_e2e_type_specialization_nested_calls() {
    // Typed functions calling other typed functions - using two separate calls
    let source = r#"
function double_it(x::Int64)::Int64
    x * 2
end

a = double_it(5)
b = double_it(a)
b
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_no_value_type() {
    // Ensure no Value type appears in generated code for typed functions
    let source = r#"
function typed_add(a::Int64, b::Int64)::Int64
    a + b
end

x = typed_add(10, 20)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    // For fully typed functions, we should not see Value::Int64 wrapping
    assert!(
        !code.contains("Value::Int64(a)") || code.contains("i64"),
        "Fully typed function should use native types"
    );
}

#[test]
fn test_aot_e2e_type_specialization_return_type_inference() {
    // Function with explicit return type
    let source = r#"
function get_double(x::Int64)::Int64
    x * 2
end

y = get_double(7)
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    assert!(
        code.contains("-> i64") || code.contains("Int64") || code.contains("i64"),
        "Return type should be specialized"
    );
}

#[test]
fn test_aot_e2e_type_specialization_recursive() {
    // Recursive typed function
    let source = r#"
function factorial(n::Int64)::Int64
    if n <= 1
        1
    else
        n * factorial(n - 1)
    end
end

result = factorial(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_with_locals() {
    // Typed function with local variables
    let source = r#"
function compute_sum(a::Int64, b::Int64, c::Int64)::Int64
    temp1 = a + b
    temp2 = temp1 + c
    temp2
end

result = compute_sum(1, 2, 3)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Full Pipeline Verification Tests (Issue #2595)
//
// These tests verify the complete AoT pipeline end-to-end:
// Julia source → parse → lower → type inference → AoT IR → codegen → Rust code
// Each test checks both successful compilation AND correctness of generated code.
// ============================================================================

// ----------------------------------------------------------------------------
// Arithmetic: integers and floats
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_integer_arithmetic_all_ops() {
    // Verify all basic integer arithmetic operations generate correct Rust
    let source = r#"
a = 10
b = 3
c = a + b
d = a - b
e = a * b
f = a ÷ b
g = a % b
c + d + e + f + g
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub fn main()"),
        "Should contain main function"
    );
}

#[test]
fn test_e2e_pipeline_float_arithmetic_precision() {
    // Verify float arithmetic preserves f64 types
    let source = r#"
x = 1.5
y = 2.7
z = x + y
w = x * y - z / 2.0
w
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("f64") || rust_code.contains("1.5"),
        "Should contain float type or literal: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_mixed_int_float_arithmetic() {
    // Mixed integer and float operations
    let source = r#"
function convert_and_add(x::Int64, y::Float64)::Float64
    x + y
end

result = convert_and_add(3, 4.5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn convert_and_add"),
        "Should contain function definition"
    );
}

#[test]
fn test_e2e_pipeline_negative_numbers() {
    // Negative number handling
    let source = r#"
x = -5
y = -3.14
z = x + 10
w = y * (-2.0)
z
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ----------------------------------------------------------------------------
// Variable assignment and scoping
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_variable_chain() {
    // Chain of variable assignments and usage
    // Top-level variables are emitted as `static` in the AoT pipeline
    let source = r#"
a = 1
b = a + 1
c = b + a
d = c * b
e = d - c + b - a
e
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("static") || rust_code.contains("let"),
        "Should contain variable bindings: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_variable_shadowing() {
    // Variable reassignment (shadowing in Julia)
    let source = r#"
x = 10
x = x + 1
x = x * 2
x = x - 5
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_e2e_pipeline_variables_in_function() {
    // Local variables inside functions
    let source = r#"
function compute(a::Int64, b::Int64)::Int64
    sum = a + b
    diff = a - b
    product = sum * diff
    product
end

compute(10, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn compute"),
        "Should define compute function"
    );
    assert!(
        rust_code.contains("let"),
        "Should use let for local variables: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Functions with multiple dispatch
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_dispatch_int_vs_float() {
    // Multiple dispatch with Int64 and Float64
    let source = r#"
function process(x::Int64)::Int64
    x * 2
end

function process(x::Float64)::Float64
    x * 2.0
end

a = process(5)
b = process(3.14)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have dispatch mechanism (either mangled names or enum-based)
    assert!(
        rust_code.contains("process"),
        "Should contain process function(s)"
    );
}

#[test]
fn test_e2e_pipeline_dispatch_arity() {
    // Dispatch on number of arguments
    let source = r#"
function area(r::Float64)::Float64
    3.14159 * r * r
end

function area(w::Float64, h::Float64)::Float64
    w * h
end

circle_area = area(5.0)
rect_area = area(3.0, 4.0)
circle_area
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_e2e_pipeline_dispatch_struct_parameter() {
    // Dispatch on struct types
    let source = r#"
struct Circle
    radius::Float64
end

struct Rectangle
    width::Float64
    height::Float64
end

function area(c::Circle)::Float64
    3.14159 * c.radius * c.radius
end

function area(r::Rectangle)::Float64
    r.width * r.height
end

c = Circle(5.0)
r = Rectangle(3.0, 4.0)
area(c)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("struct Circle"),
        "Should define Circle struct: {}",
        rust_code
    );
    assert!(
        rust_code.contains("struct Rectangle"),
        "Should define Rectangle struct: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Control flow: if/else, while, for
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_if_else_chain() {
    // Complex if/elseif/else chain
    let source = r#"
function classify(x::Int64)::Int64
    if x > 100
        return 3
    elseif x > 10
        return 2
    elseif x > 0
        return 1
    else
        return 0
    end
end

classify(50)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("if") && rust_code.contains("else"),
        "Should contain if/else: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_while_with_counter() {
    // While loop with counter
    let source = r#"
function sum_to(n::Int64)::Int64
    total = 0
    i = 1
    while i <= n
        total = total + i
        i = i + 1
    end
    total
end

sum_to(100)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("while"),
        "Should contain while loop: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_for_accumulation() {
    // For loop with accumulation
    let source = r#"
function sum_squares(n::Int64)::Int64
    total = 0
    for i in 1:n
        total = total + i * i
    end
    total
end

sum_squares(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("for"),
        "Should contain for loop: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_nested_loops_with_condition() {
    // Nested loops with conditional
    let source = r#"
function count_pairs(n::Int64)::Int64
    count = 0
    for i in 1:n
        for j in 1:n
            if i + j > n
                count = count + 1
            end
        end
    end
    count
end

count_pairs(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn count_pairs"),
        "Should define count_pairs function"
    );
}

#[test]
fn test_e2e_pipeline_loop_with_early_return() {
    // Loop with early return
    let source = r#"
function find_first_gt(n::Int64)::Int64
    for i in 1:100
        if i * i > n
            return i
        end
    end
    return -1
end

find_first_gt(50)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("return"),
        "Should contain return statement: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Struct definitions and usage
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_struct_with_methods() {
    // Struct with associated functions
    let source = r#"
struct Vec2D
    x::Float64
    y::Float64
end

function magnitude(v::Vec2D)::Float64
    sqrt(v.x * v.x + v.y * v.y)
end

function dot(a::Vec2D, b::Vec2D)::Float64
    a.x * b.x + a.y * b.y
end

v1 = Vec2D(3.0, 4.0)
v2 = Vec2D(1.0, 0.0)
m = magnitude(v1)
d = dot(v1, v2)
m
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub struct Vec2D"),
        "Should define Vec2D struct: {}",
        rust_code
    );
    assert!(
        rust_code.contains("fn magnitude"),
        "Should define magnitude function"
    );
    assert!(rust_code.contains("fn dot"), "Should define dot function");
}

#[test]
fn test_e2e_pipeline_mutable_struct_update() {
    // Mutable struct with field updates
    let source = r#"
mutable struct Accumulator
    total::Int64
    count::Int64
end

function add_value(acc::Accumulator, val::Int64)
    acc.total = acc.total + val
    acc.count = acc.count + 1
end

a = Accumulator(0, 0)
add_value(a, 10)
add_value(a, 20)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("struct Accumulator"),
        "Should define Accumulator struct: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_struct_mixed_field_types() {
    // Struct with different field types
    let source = r#"
struct Particle
    x::Float64
    y::Float64
    mass::Float64
end

function kinetic_energy(p::Particle, vx::Float64, vy::Float64)::Float64
    0.5 * p.mass * (vx * vx + vy * vy)
end

p = Particle(0.0, 0.0, 2.5)
ke = kinetic_energy(p, 3.0, 4.0)
ke
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("struct Particle"),
        "Should define Particle struct"
    );
    assert!(
        rust_code.contains("f64"),
        "Should use f64 type for Float64 fields"
    );
}

// ----------------------------------------------------------------------------
// Array operations
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_array_iteration() {
    // Iterating over array with for loop
    let source = r#"
function sum_array(arr::Array{Int64,1})::Int64
    total = 0
    for x in arr
        total = total + x
    end
    total
end

arr = [10, 20, 30, 40, 50]
sum_array(arr)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn sum_array"),
        "Should define sum_array function"
    );
}

#[test]
fn test_e2e_pipeline_array_index_computation() {
    // Array indexing with computed indices
    let source = r#"
arr = [1, 2, 3, 4, 5]
i = 3
x = arr[i]
y = arr[1] + arr[5]
x + y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_e2e_pipeline_array_build_with_loop() {
    // Building array in a loop
    let source = r#"
arr = zeros(5)
for i in 1:5
    arr[i] = i * i * 1.0
end
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec![0.0_f64;"),
        "Should generate zeros as vec!: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Combined complex programs
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_fibonacci_iterative() {
    // Iterative Fibonacci — tests variables, loops, and conditionals together
    let source = r#"
function fib_iter(n::Int64)::Int64
    if n <= 1
        return n
    end
    a = 0
    b = 1
    for i in 2:n
        temp = a + b
        a = b
        b = temp
    end
    b
end

fib_iter(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(rust_code.contains("fn fib_iter"), "Should define fib_iter");
    assert!(rust_code.contains("for"), "Should contain for loop");
    assert!(rust_code.contains("if"), "Should contain if statement");
}

#[test]
fn test_e2e_pipeline_gcd_euclid() {
    // Euclidean GCD algorithm — tests while loops with modulo
    let source = r#"
function gcd(a::Int64, b::Int64)::Int64
    while b != 0
        temp = b
        b = a % b
        a = temp
    end
    a
end

gcd(48, 18)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(rust_code.contains("fn gcd"), "Should define gcd function");
    assert!(rust_code.contains("while"), "Should contain while loop");
}

#[test]
fn test_e2e_pipeline_bubble_sort() {
    // Bubble sort — tests nested loops, array indexing, and swapping
    let source = r#"
function bubble_sort(arr::Array{Int64,1})
    n = length(arr)
    for i in 1:n
        for j in 1:(n - i)
            if arr[j] > arr[j + 1]
                temp = arr[j]
                arr[j] = arr[j + 1]
                arr[j + 1] = temp
            end
        end
    end
    arr
end

data = [5, 3, 8, 1, 2]
bubble_sort(data)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn bubble_sort"),
        "Should define bubble_sort function"
    );
}

#[test]
fn test_e2e_pipeline_matrix_zeros_2d() {
    // 2D matrix creation
    let source = r#"
mat = zeros(3, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("collect") || rust_code.contains("map(|_|"),
        "Should generate 2D Vec structure: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_struct_with_function() {
    // Struct and function used together
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

function make_point(i::Int64)::Point
    Point(i * 1.0, i * 2.0)
end

p = make_point(5)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub struct Point"),
        "Should define Point struct"
    );
    assert!(
        rust_code.contains("fn make_point"),
        "Should define make_point function"
    );
}

#[test]
fn test_e2e_pipeline_power_function() {
    // Power computation using loop
    let source = r#"
function power(base::Int64, exp::Int64)::Int64
    result = 1
    for i in 1:exp
        result = result * base
    end
    result
end

power(2, 10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn power"),
        "Should define power function"
    );
}

#[test]
fn test_e2e_pipeline_string_literal() {
    // String literal handling
    let source = r#"
msg = "hello world"
msg
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("hello world"),
        "Should contain string literal: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_boolean_logic() {
    // Boolean logic operations
    let source = r#"
function check(a::Bool, b::Bool)::Bool
    (a && b) || (!a && !b)
end

check(true, true)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn check"),
        "Should define check function"
    );
    assert!(
        rust_code.contains("bool"),
        "Should use bool type: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_collatz_sequence() {
    // Collatz conjecture step count — tests while + if/else + modulo
    let source = r#"
function collatz_steps(n::Int64)::Int64
    steps = 0
    while n != 1
        if n % 2 == 0
            n = n ÷ 2
        else
            n = 3 * n + 1
        end
        steps = steps + 1
    end
    steps
end

collatz_steps(27)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn collatz_steps"),
        "Should define collatz_steps function"
    );
    assert!(
        rust_code.contains("while") && rust_code.contains("if"),
        "Should contain while loop and if statement"
    );
}

// ----------------------------------------------------------------------------
// Generated code quality checks
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_no_panic_in_generated_code() {
    // Verify generated code doesn't contain panic or unwrap calls
    let source = r#"
function safe_divide(a::Int64, b::Int64)::Int64
    if b == 0
        return 0
    end
    a ÷ b
end

safe_divide(10, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        !rust_code.contains("panic!"),
        "Generated code should not contain panic!: {}",
        rust_code
    );
    assert!(
        !rust_code.contains(".unwrap()"),
        "Generated code should not contain .unwrap(): {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_valid_rust_structure() {
    // Verify the overall structure of generated Rust code
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end

result = add(1, 2)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have header
    assert!(
        rust_code.contains("Auto-generated"),
        "Should have auto-generated header"
    );
    // Should have allow attributes
    assert!(
        rust_code.contains("#![allow("),
        "Should have #![allow] attributes"
    );
    // Should have main function
    assert!(
        rust_code.contains("pub fn main()"),
        "Should have pub fn main()"
    );
}
