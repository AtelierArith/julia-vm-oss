//! Tests for multiple dispatch functionality.

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::types::JuliaType;
use subset_julia_vm::vm::{Value, Vm};

/// Helper to parse, lower, compile, and run a program using the new pipeline.
fn run_core_program(src: &str, seed: u64) -> Result<Value, String> {
    let mut parser = Parser::new().map_err(|e| e.to_string())?;
    let parsed = parser.parse(src).map_err(|e| e.to_string())?;
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(parsed).map_err(|e| e.to_string())?;
    let compiled = compile_core_program(&program).map_err(|e| e.to_string())?;

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    vm.run().map_err(|e| e.to_string())
}

// ==================== Type System Tests ====================

#[test]
fn test_julia_type_subtype_int64() {
    // Int64 <: Integer <: Real <: Number <: Any
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Int64));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Integer));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Real));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Number));
    assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Any));
}

#[test]
fn test_julia_type_subtype_float64() {
    // Float64 <: AbstractFloat <: Real <: Number <: Any
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Float64));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::AbstractFloat));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Real));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Number));
    assert!(JuliaType::Float64.is_subtype_of(&JuliaType::Any));
}

#[test]
fn test_julia_type_not_subtype() {
    // Int64 is not a subtype of Float64
    assert!(!JuliaType::Int64.is_subtype_of(&JuliaType::Float64));
    assert!(!JuliaType::Float64.is_subtype_of(&JuliaType::Int64));

    // String is not a subtype of Number
    assert!(!JuliaType::String.is_subtype_of(&JuliaType::Number));
}

#[test]
fn test_julia_type_specificity() {
    // Concrete types are more specific than abstract types
    assert!(JuliaType::Int64.specificity() > JuliaType::Integer.specificity());
    assert!(JuliaType::Integer.specificity() > JuliaType::Real.specificity());
    assert!(JuliaType::Real.specificity() > JuliaType::Number.specificity());
    assert!(JuliaType::Number.specificity() > JuliaType::Any.specificity());
}

#[test]
fn test_julia_type_from_name() {
    assert_eq!(JuliaType::from_name("Int64"), Some(JuliaType::Int64));
    assert_eq!(JuliaType::from_name("Int"), Some(JuliaType::Int64));
    assert_eq!(JuliaType::from_name("Float64"), Some(JuliaType::Float64));
    assert_eq!(JuliaType::from_name("Number"), Some(JuliaType::Number));
    assert_eq!(JuliaType::from_name("Any"), Some(JuliaType::Any));
    assert_eq!(JuliaType::from_name("NotAType"), None);
}

// ==================== Basic Core Pipeline Tests ====================

#[test]
fn test_core_simple_return() {
    let src = "42";
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 42),
        other => panic!("Expected I64, got {:?}", other),
    }
}

#[test]
fn test_core_simple_arithmetic() {
    let src = "1 + 2 * 3";
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 7),
        other => panic!("Expected I64, got {:?}", other),
    }
}

#[test]
fn test_core_function_definition_and_call() {
    let src = r#"
function add(x, y)
    return x + y
end
add(3, 4)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 7),
        other => panic!("Expected I64, got {:?}", other),
    }
}

// ==================== Typed Parameter Tests ====================

#[test]
fn test_typed_parameter_int64() {
    let src = r#"
function double(x::Int64)
    return x * 2
end
double(21)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 42),
        other => panic!("Expected I64, got {:?}", other),
    }
}

#[test]
fn test_typed_parameter_float64() {
    let src = r#"
function half(x::Float64)
    return x / 2.0
end
half(10.0)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::F64(v) => assert!((v - 5.0).abs() < 1e-10),
        other => panic!("Expected F64, got {:?}", other),
    }
}

// ==================== Multiple Dispatch Tests ====================

#[test]
fn test_multiple_dispatch_same_name_different_types() {
    let src = r#"
function add(x::Int64, y::Int64)
    return x + y
end

function add(x::Float64, y::Float64)
    return x + y
end

add(3, 4)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 7),
        other => panic!("Expected I64 from Int64 method, got {:?}", other),
    }
}

#[test]
fn test_multiple_dispatch_with_abstract_type() {
    let src = r#"
function process(x::Number)
    return x + 100
end

function process(x::Int64)
    return x + 1
end

process(5)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        // Should dispatch to the more specific Int64 version
        Value::I64(v) => assert_eq!(v, 6),
        other => panic!("Expected I64 from specific method, got {:?}", other),
    }
}

#[test]
fn test_any_type_as_fallback() {
    let src = r#"
function identity(x)
    return x
end

identity(42)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        Value::I64(v) => assert_eq!(v, 42),
        other => panic!("Expected I64, got {:?}", other),
    }
}

// ==================== Runtime Type Tests ====================

#[test]
fn test_value_runtime_type() {
    assert_eq!(Value::I64(42).runtime_type(), JuliaType::Int64);
    let pi_approx = 314.0 / 100.0;
    assert_eq!(Value::F64(pi_approx).runtime_type(), JuliaType::Float64);
    assert_eq!(
        Value::Str("hello".to_string()).runtime_type(),
        JuliaType::String
    );
    // Complex is now a Pure Julia struct - type_id 0 is used as placeholder in tests
    assert_eq!(
        Value::new_complex(0, 1.0, 2.0).runtime_type(),
        JuliaType::Struct("Complex".to_string())
    );
}

// ==================== Operator Overloading Tests ====================

// ignore this test for now
#[test]
fn test_operator_function_definition_parses() {
    // Test that operator function definitions are accepted by the parser and lowering
    let src = r#"
function +(a::Int64, b::Int64)
    return 999
end

42
"#;
    let result = run_core_program(src, 0);
    assert!(
        result.is_ok(),
        "Operator function definition should parse: {:?}",
        result
    );
}

#[test]
fn test_primitive_operators_use_user_defined() {
    // Known limitation (Issue #1915, #2233): In Julia, user-defined +(::Int64, ::Int64)
    // shadows the builtin +. In SubsetJuliaVM, the is_builtin_numeric guard (Issue #2203)
    // intentionally routes primitive numeric operations through builtins for type
    // preservation (Float32/Float16 etc.), so user-defined operators do NOT shadow
    // builtins for primitive types. This is a deliberate architectural trade-off.
    let src = r#"
function +(a::Int64, b::Int64)
    return 999999
end

# In SubsetJuliaVM: builtin + is used for Int64 (is_builtin_numeric guard)
1 + 2
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        // Builtin + is used: 1 + 2 = 3 (not 999999)
        Value::I64(v) => assert_eq!(
            v, 3,
            "Builtin + should be used for primitive Int64, got {}",
            v
        ),
        other => panic!("Expected I64, got {:?}", other),
    }
}

#[test]
fn test_multiple_operators_defined() {
    // Known limitation (Issue #1915, #2233): User-defined operators for primitive types
    // (e.g., +(::Int64, ::Int64)) do NOT shadow builtins in the current architecture.
    // The is_builtin_numeric guard (Issue #2203) routes primitive-type operations
    // through builtins for type preservation, bypassing method dispatch.
    //
    // In Julia, user-defined +(::Int64, ::Int64) = 1 would shadow the builtin +,
    // so `(1+2) + (3-1) + (2*3)` = +(+(1,2), -(3,1), *(2,3)) = +(1, 2, 3) = 1.
    // In SubsetJuliaVM, builtins are used for all primitive ops:
    //   (1+2)=3, (3-1)=2, (2*3)=6
    // Then nary reduction: +(3, 2, 6) = 3 + 2 + 6 = 11
    let src = r#"
function +(a::Int64, b::Int64)
    return 1
end

function -(a::Int64, b::Int64)
    return 2
end

function *(a::Int64, b::Int64)
    return 3
end

# Known limitation: user-defined operators don't shadow builtins for primitives.
# Builtins are used: (1+2)=3, (3-1)=2, (2*3)=6
# Nary reduction: +(3, 2, 6) = 11
(1 + 2) + (3 - 1) + (2 * 3)
"#;
    let result = run_core_program(src, 0);
    assert!(result.is_ok(), "Failed: {:?}", result);
    match result.unwrap() {
        // Builtins used for all primitive ops; nary + produces sum: 3 + 2 + 6 = 11
        Value::I64(v) => assert_eq!(v, 11, "Expected 11 from builtin nary +, got {}", v),
        other => panic!("Expected I64, got {:?}", other),
    }
}

// ==================== Struct Operator Overloading Tests ====================

#[test]
fn test_struct_operator_overload_add() {
    // Test operator overloading for user-defined structs.
    // When user defines +(::Point, ::Point), it shadows the builtin +.
    // To use builtin + inside the method, use Base.:+(a, b) syntax.
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

function Base.:+(a::Point, b::Point)
    return Point(a.x + b.x, a.y + b.y)
end

p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)
p3 = p1 + p2
p3.x
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("Struct operator overload failed: {}", e),
    }
}

#[test]
fn test_base_method_definition_does_not_shadow_builtin() {
    // Test that using `function Base.:+(...)` syntax does NOT shadow builtin +
    // This is Julia's way to extend Base operators without breaking primitives.
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

function Base.:+(a::Point, b::Point)
    return Point(a.x + b.x, a.y + b.y)
end

# Primitive + should still work because we're extending Base.:+, not shadowing it
1 + 2
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::I64(v)) => assert_eq!(v, 3, "1 + 2 should be 3, got {}", v),
        Ok(other) => panic!("Expected I64(3), got {:?}", other),
        Err(e) => panic!("Base.:+ method definition failed: {}", e),
    }
}

#[test]
fn test_base_extension_field_access_addition() {
    // Test that field access + works inside Base.:+ methods
    // This is a simpler version of the iOS operator overload test
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

function Base.:+(a::Point, b::Point)
    # a.x + b.x should use builtin + for Float64
    return Point(a.x + b.x, a.y + b.y)
end

p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)
p3 = p1 + p2
p3.x
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 4.0).abs() < 0.001, "p3.x should be 4.0, got {}", v),
        Ok(other) => panic!("Expected F64(4.0), got {:?}", other),
        Err(e) => panic!("Base extension field access addition failed: {}", e),
    }
}

#[test]
fn test_base_extension_chained_ops() {
    // Test chained operations with Base extension
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

function Base.:+(a::Point, b::Point)
    return Point(a.x + b.x, a.y + b.y)
end

p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)

# This chain should work: (p1 + p2) + Point(10.0, 10.0)
p5 = p1 + p2 + Point(10.0, 10.0)
p5.x
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 14.0).abs() < 0.001, "p5.x should be 14.0, got {}", v),
        Ok(other) => panic!("Expected F64(14.0), got {:?}", other),
        Err(e) => panic!("Base extension field access addition failed: {}", e),
    }
}

#[test]
fn test_ios_operator_overload_sample() {
    // Test the exact iOS app sample code for operator overloading
    let src = r#"
# Define a 2D Point struct
struct Point
    x::Float64
    y::Float64
end

# Overload the + operator for Point
function Base.:+(a::Point, b::Point)
    return Point(a.x + b.x, a.y + b.y)
end

# Overload the - operator for Point
function Base.:-(a::Point, b::Point)
    return Point(a.x - b.x, a.y - b.y)
end

# Create two points
p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)

# Use the overloaded operators
p3 = p1 + p2
println("p1 = (", p1.x, ", ", p1.y, ")")
println("p2 = (", p2.x, ", ", p2.y, ")")
println("p1 + p2 = (", p3.x, ", ", p3.y, ")")

p4 = p2 - p1
println("p2 - p1 = (", p4.x, ", ", p4.y, ")")

# Chain operations
p5 = p1 + p2 + Point(10.0, 10.0)
println("p1 + p2 + (10,10) = (", p5.x, ", ", p5.y, ")")

p3.x + p3.y
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!(
            (v - 10.0).abs() < 0.001,
            "p3.x + p3.y should be 10.0, got {}",
            v
        ),
        Ok(other) => panic!("Expected F64(10.0), got {:?}", other),
        Err(e) => panic!("iOS operator overload sample failed: {}", e),
    }
}

// ==================== Type Conversion Tests (Issue #1698) ====================
// These tests ensure that type conversions for F32/F16 narrowing work correctly.
// Regression tests for Issue #1689: dispatch_tests fail with Cannot convert F64/I64 to F32

#[test]
fn test_float32_struct_i64_conversion() {
    // Test I64 -> F32 conversion in struct constructor
    // This triggered "Cannot convert I64 to F32" before the fix
    let src = r#"
mutable struct Point32
    x::Float32
    y::Float32
end
p = Point32(1, 2)  # I64 -> F32 conversion
Float64(p.x) + Float64(p.y)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 3.0).abs() < 1e-6, "Expected 3.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("Float32 struct I64 conversion failed: {}", e),
    }
}

#[test]
fn test_float32_struct_f64_conversion() {
    // Test F64 -> F32 narrowing conversion in struct constructor
    // This triggered "Cannot convert F64 to F32" before the fix
    let src = r#"
mutable struct Point32
    x::Float32
end
p = Point32(1.5)  # F64 -> F32 conversion
Float64(p.x)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 1.5).abs() < 1e-6, "Expected 1.5, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("Float32 struct F64 conversion failed: {}", e),
    }
}

#[test]
fn test_float32_arithmetic_mixed() {
    // Test mixed Float32/Float64 arithmetic
    // Ensures type promotion works correctly
    let src = r#"
x = Float32(2.0)
y = 3.0  # Float64
z = Float64(x) * y
z
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 6.0).abs() < 1e-6, "Expected 6.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("Float32 mixed arithmetic failed: {}", e),
    }
}

#[test]
fn test_complex_float32_construction() {
    // Test Complex{Float32} construction with various input types
    // This is a common use case that triggered the original bug
    let src = r#"
# Test that Complex{Float32} can be constructed
z = Complex{Float32}(1.0, 2.0)  # F64 -> F32 conversion
Float64(z.re) + Float64(z.im)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 3.0).abs() < 1e-6, "Expected 3.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("Complex Float32 construction failed: {}", e),
    }
}

// ==================== Type Conversion Matrix Tests (Issue #1695) ====================
// These tests ensure that all numeric type conversions to F32/F16 work correctly.
// Regression prevention for Issue #1689.

#[test]
fn test_type_conversion_i64_to_f32() {
    // Test explicit Int64 -> Float32 conversion
    let src = r#"
x::Int64 = 42
y = Float32(x)
Float64(y)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 42.0).abs() < 1e-6, "Expected 42.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("I64 to F32 conversion failed: {}", e),
    }
}

#[test]
fn test_type_conversion_f64_to_f32() {
    // Test explicit Float64 -> Float32 narrowing conversion
    let src = r#"
x::Float64 = 3.14159
y = Float32(x)
Float64(y)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => {
            let expected = 314_159.0 / 100_000.0;
            assert!((v - expected).abs() < 1e-4, "Expected ~{}, got {}", expected, v)
        }
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("F64 to F32 conversion failed: {}", e),
    }
}

#[test]
fn test_type_conversion_f32_to_f64() {
    // Test Float32 -> Float64 widening conversion
    let src = r#"
x = Float32(2.5)
y = Float64(x)
y
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 2.5).abs() < 1e-6, "Expected 2.5, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("F32 to F64 conversion failed: {}", e),
    }
}

#[test]
fn test_type_conversion_f32_struct_field_assignment() {
    // Test Float32 struct field with various input types (comprehensive)
    let src = r#"
mutable struct TestF32
    a::Float32
    b::Float32
    c::Float32
end

# Test different source types
t = TestF32(1, 2.0, Float32(3.0))  # I64, F64, F32
Float64(t.a) + Float64(t.b) + Float64(t.c)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 6.0).abs() < 1e-6, "Expected 6.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("F32 struct field assignment failed: {}", e),
    }
}

#[test]
fn test_type_conversion_f32_operator_return() {
    // Test that user-defined operators returning different types work with F32 context
    let src = r#"
mutable struct VecF32
    x::Float32
    y::Float32
end

function scale(v::VecF32, s::Float64)
    return VecF32(v.x * Float32(s), v.y * Float32(s))
end

v = VecF32(1.0, 2.0)
v2 = scale(v, 2.0)
Float64(v2.x) + Float64(v2.y)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 6.0).abs() < 1e-6, "Expected 6.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("F32 operator return conversion failed: {}", e),
    }
}

#[test]
fn test_type_conversion_array_to_f32_element() {
    // Test that array elements can be converted to Float32
    let src = r#"
arr = [1, 2, 3]  # Int64 array
x = Float32(arr[1])
Float64(x)
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 1.0).abs() < 1e-6, "Expected 1.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("Array element to F32 conversion failed: {}", e),
    }
}

#[test]
fn test_type_conversion_f32_in_function_args() {
    // Test Float32 type annotation in function parameters
    let src = r#"
function process_f32(x::Float32)
    return Float64(x) * 2.0
end

# Call with Int64 (should auto-convert)
result = process_f32(Float32(5))
result
"#;
    let result = run_core_program(src, 0);
    match result {
        Ok(Value::F64(v)) => assert!((v - 10.0).abs() < 1e-6, "Expected 10.0, got {}", v),
        Ok(other) => panic!("Expected F64, got {:?}", other),
        Err(e) => panic!("F32 function arg conversion failed: {}", e),
    }
}
