//! Tests migrated from tree-sitter-julia/test/corpus/definitions.txt

use subset_julia_vm_parser::{parse, NodeKind};

fn assert_parses(source: &str) {
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse: {}\nError: {:?}",
        source,
        result.err()
    );
}

fn assert_root_child_kind(source: &str, expected_kind: NodeKind) {
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));
    assert_eq!(cst.kind, NodeKind::SourceFile);
    assert!(!cst.children.is_empty(), "Expected at least one child");
    assert_eq!(
        cst.children[0].kind, expected_kind,
        "Expected {:?}, got {:?} for source: {}",
        expected_kind, cst.children[0].kind, source
    );
}

// =============================================================================
// Module Definition
// =============================================================================

#[test]
fn test_module_empty() {
    assert_root_child_kind("module Foo\nend", NodeKind::ModuleDefinition);
}

#[test]
fn test_module_with_body() {
    assert_root_child_kind("module Foo\n  x = 1\nend", NodeKind::ModuleDefinition);
}

#[test]
fn test_module_with_export() {
    assert_root_child_kind(
        "module Foo\n  export bar\n  bar() = 1\nend",
        NodeKind::ModuleDefinition,
    );
}

#[test]
fn test_baremodule() {
    assert_root_child_kind("baremodule Foo\nend", NodeKind::BaremoduleDefinition);
}

// =============================================================================
// Abstract Type Definition
// =============================================================================

#[test]
fn test_abstract_simple() {
    assert_root_child_kind("abstract type Foo end", NodeKind::AbstractDefinition);
}

#[test]
fn test_abstract_with_supertype() {
    assert_root_child_kind("abstract type Foo <: Bar end", NodeKind::AbstractDefinition);
}

// Parametric abstract types
#[test]
fn test_abstract_parametric() {
    assert_parses("abstract type Foo{T} end");
}

#[test]
fn test_abstract_parametric_supertype() {
    assert_parses("abstract type Foo{T} <: Bar{T} end");
}

#[test]
fn test_abstract_parametric_bounded() {
    assert_parses("abstract type Foo{T <: Number} end");
}

// =============================================================================
// Primitive Type Definition
// =============================================================================

#[test]
fn test_primitive_simple() {
    assert_root_child_kind(
        "primitive type Int128 128 end",
        NodeKind::PrimitiveDefinition,
    );
}

#[test]
fn test_primitive_with_supertype() {
    assert_root_child_kind(
        "primitive type MyInt <: Integer 32 end",
        NodeKind::PrimitiveDefinition,
    );
}

// =============================================================================
// Struct Definition
// =============================================================================

#[test]
fn test_struct_empty() {
    assert_root_child_kind("struct Foo\nend", NodeKind::StructDefinition);
}

#[test]
fn test_struct_with_fields() {
    assert_root_child_kind("struct Point\n  x\n  y\nend", NodeKind::StructDefinition);
}

#[test]
fn test_struct_typed_fields() {
    assert_root_child_kind(
        "struct Point\n  x::Float64\n  y::Float64\nend",
        NodeKind::StructDefinition,
    );
}

#[test]
fn test_struct_with_supertype() {
    assert_root_child_kind("struct Foo <: Bar\nend", NodeKind::StructDefinition);
}

#[test]
fn test_struct_parametric() {
    assert_root_child_kind(
        "struct Point{T}\n  x::T\n  y::T\nend",
        NodeKind::StructDefinition,
    );
}

#[test]
fn test_struct_parametric_bounded() {
    assert_root_child_kind(
        "struct Point{T <: Number}\n  x::T\n  y::T\nend",
        NodeKind::StructDefinition,
    );
}

#[test]
fn test_struct_const_field() {
    // Julia 1.8+ const field syntax
    assert_root_child_kind(
        "struct Foo\n  const x::Int\nend",
        NodeKind::StructDefinition,
    );
}

#[test]
fn test_struct_with_constructor() {
    assert_root_child_kind(
        "struct Point\n  x\n  y\n  Point(x) = new(x, x)\nend",
        NodeKind::StructDefinition,
    );
}

// =============================================================================
// Mutable Struct Definition
// =============================================================================

#[test]
fn test_mutable_struct_empty() {
    assert_root_child_kind("mutable struct Foo\nend", NodeKind::MutableStructDefinition);
}

#[test]
fn test_mutable_struct_with_fields() {
    assert_root_child_kind(
        "mutable struct Point\n  x::Float64\n  y::Float64\nend",
        NodeKind::MutableStructDefinition,
    );
}

#[test]
fn test_mutable_struct_parametric() {
    assert_root_child_kind(
        "mutable struct Box{T}\n  value::T\nend",
        NodeKind::MutableStructDefinition,
    );
}

// =============================================================================
// Function Definition
// =============================================================================

#[test]
fn test_function_empty() {
    assert_root_child_kind("function foo()\nend", NodeKind::FunctionDefinition);
}

#[test]
fn test_function_with_body() {
    assert_root_child_kind(
        "function foo()\n  return 1\nend",
        NodeKind::FunctionDefinition,
    );
}

#[test]
fn test_function_with_args() {
    assert_root_child_kind(
        "function foo(x, y)\n  x + y\nend",
        NodeKind::FunctionDefinition,
    );
}

#[test]
fn test_function_typed_args() {
    assert_root_child_kind(
        "function foo(x::Int, y::Int)\n  x + y\nend",
        NodeKind::FunctionDefinition,
    );
}

#[test]
fn test_function_default_args() {
    assert_root_child_kind(
        "function foo(x, y=1)\n  x + y\nend",
        NodeKind::FunctionDefinition,
    );
}

// Keyword args
#[test]
fn test_function_keyword_args() {
    assert_parses("function foo(x; y=1)\n  x + y\nend");
    assert_parses("function foo(; x=1, y=2)\n  x + y\nend"); // keyword-only
    assert_parses("function foo(a, b; x=1, y=2)\n  a + b + x + y\nend");
}

// Varargs
#[test]
fn test_function_varargs() {
    assert_parses("function foo(x, args...)\n  sum(args)\nend");
    assert_parses("function foo(args...)\n  sum(args)\nend");
    assert_parses("function foo(x::Int, args::T...)\n  sum(args)\nend");
}

// Return type annotation
#[test]
fn test_function_return_type() {
    assert_parses("function foo(x)::Int\n  x\nend");
}

// Where clause functions
#[test]
fn test_function_where() {
    assert_parses("function foo(x::T) where T\n  x\nend");
}

#[test]
fn test_function_where_bounded() {
    assert_parses("function foo(x::T) where T <: Number\n  x\nend");
}

#[test]
fn test_function_multiple_where() {
    assert_parses("function foo(x::T, y::S) where T where S\n  x + y\nend");
}

// Parametric function (old syntax)
#[test]
fn test_function_parametric() {
    assert_parses("function foo{T}(x::T)\n  x\nend");
}

// =============================================================================
// Short Function Definition (parsed as Assignment with = operator)
// =============================================================================

#[test]
fn test_short_function_simple() {
    // Short form functions are parsed as Assignment: call_expr = expr
    // The distinction between variable assignment and short function is semantic
    assert_root_child_kind("foo() = 1", NodeKind::Assignment);
}

#[test]
fn test_short_function_with_args() {
    assert_root_child_kind("foo(x, y) = x + y", NodeKind::Assignment);
}

#[test]
fn test_short_function_typed() {
    assert_root_child_kind("foo(x::Int) = x * 2", NodeKind::Assignment);
}

// Short function with return type annotation
#[test]
fn test_short_function_return_type() {
    assert_root_child_kind("foo(x)::Int = x", NodeKind::Assignment);
}

// Short function with where clause
#[test]
fn test_short_function_where() {
    assert_parses("foo(x::T) where T = x");
}

// =============================================================================
// Operator Definition
// =============================================================================

// Operator definition - simple form
#[test]
fn test_operator_definition() {
    assert_parses("(+)(a, b) = a + b");
}

#[test]
fn test_operator_definition_typed() {
    assert_parses("(+)(a::MyType, b::MyType) = MyType(a.value + b.value)");
}

// Operator definition with module prefix
#[test]
fn test_operator_short_form() {
    assert_parses("Base.:(==)(a::MyType, b::MyType) = a.value == b.value");
}

// =============================================================================
// Macro Definition
// =============================================================================

#[test]
fn test_macro_empty() {
    assert_root_child_kind("macro foo()\nend", NodeKind::MacroDefinition);
}

#[test]
fn test_macro_with_body() {
    assert_root_child_kind("macro foo()\n  :(1 + 1)\nend", NodeKind::MacroDefinition);
}

// Macro with interpolation in body
#[test]
fn test_macro_with_args() {
    assert_parses("macro foo(x)\n  :($x + 1)\nend");
}

// =============================================================================
// Tuple Parameter Syntax
// =============================================================================

// Tuple destructuring in function params
#[test]
fn test_function_tuple_param() {
    assert_parses("function foo((x, y))\n  x + y\nend");
}

#[test]
fn test_short_function_tuple_param() {
    // This parses as a call with tuple arg
    assert_parses("foo((x, y)) = x + y");
}

// =============================================================================
// Anonymous Function Definition
// =============================================================================

// Anonymous function syntax
#[test]
fn test_anonymous_function() {
    assert_parses("function (x)\n  x^2\nend");
}

#[test]
fn test_anonymous_function_typed() {
    assert_parses("function (x::Int)::Int\n  x^2\nend");
}
