//! Tests for function lowering.

use crate::ir::core::TypedParam;
use crate::lowering::Lowering;
use crate::parser::Parser;

/// Helper to parse a function and return the params
fn parse_function_params(source: &str) -> Vec<TypedParam> {
    let mut parser = Parser::new().expect("Failed to init parser");
    let parse_outcome = parser.parse(source).expect("Failed to parse");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parse_outcome).expect("Failed to lower");
    assert!(
        !program.functions.is_empty(),
        "No function definition found"
    );
    program.functions[0].params.clone()
}

#[test]
fn test_parse_untyped_varargs() {
    let params = parse_function_params("function f(args...) end");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "args");
    assert!(params[0].is_varargs, "Should be varargs");
}

#[test]
fn test_parse_typed_varargs_int64() {
    let params = parse_function_params("function f(xs::Int64...) end");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "xs");
    assert!(params[0].is_varargs, "Should be varargs");
    assert!(
        params[0].type_annotation.is_some(),
        "Should have type annotation"
    );
}

#[test]
fn test_parse_typed_varargs_float64() {
    let params = parse_function_params("function f(ys::Float64...) end");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "ys");
    assert!(params[0].is_varargs, "Should be varargs");
    assert!(
        params[0].type_annotation.is_some(),
        "Should have type annotation"
    );
}

#[test]
fn test_parse_mixed_params_with_typed_varargs() {
    let params = parse_function_params("function f(x::Int64, ys::Int64...) end");
    assert_eq!(params.len(), 2);

    // First param: x::Int64
    assert_eq!(params[0].name, "x");
    assert!(!params[0].is_varargs, "First param should not be varargs");
    assert!(
        params[0].type_annotation.is_some(),
        "First param should have type annotation"
    );

    // Second param: ys::Int64...
    assert_eq!(params[1].name, "ys");
    assert!(params[1].is_varargs, "Second param should be varargs");
    assert!(
        params[1].type_annotation.is_some(),
        "Second param should have type annotation"
    );
}

#[test]
fn test_parse_parametric_type_varargs() {
    // Test Vector{Int64}... varargs
    let params = parse_function_params("function f(vs::Vector{Int64}...) end");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "vs");
    assert!(params[0].is_varargs, "Should be varargs");
    assert!(
        params[0].type_annotation.is_some(),
        "Should have type annotation"
    );
}

#[test]
fn test_parse_union_type_varargs() {
    // Test Union{Int64,Float64}... varargs
    let params = parse_function_params("function f(xs::Union{Int64,Float64}...) end");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "xs");
    assert!(params[0].is_varargs, "Should be varargs");
    // Note: Union types may be parsed as Any, but should have type annotation
    assert!(
        params[0].type_annotation.is_some(),
        "Should have type annotation"
    );
}

#[test]
fn test_parse_complex_mixed_params() {
    // Test regular param + typed varargs
    let params = parse_function_params("function f(a, b::Int64, cs::Float64...) end");
    assert_eq!(params.len(), 3);

    // a - untyped regular param
    assert_eq!(params[0].name, "a");
    assert!(!params[0].is_varargs);
    assert!(params[0].type_annotation.is_none());

    // b::Int64 - typed regular param
    assert_eq!(params[1].name, "b");
    assert!(!params[1].is_varargs);
    assert!(params[1].type_annotation.is_some());

    // cs::Float64... - typed varargs
    assert_eq!(params[2].name, "cs");
    assert!(params[2].is_varargs);
    assert!(params[2].type_annotation.is_some());
}
