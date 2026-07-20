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
fn test_keyword_operator_function_name_issue_8756() {
    assert_root_child_kind(
        "function in(p::Pair, a::AbstractDict, valcmp=(==))\n  true\nend",
        NodeKind::FunctionDefinition,
    );
}

#[test]
fn test_additional_definition_gaps_issue_8759() {
    assert_root_child_kind(
        "abstract type AbstractPlatform; end",
        NodeKind::AbstractDefinition,
    );
    assert_root_child_kind(
        "function @main(args::Vector{String})::Cint\n  return 0\nend",
        NodeKind::FunctionDefinition,
    );
    assert_root_child_kind(
        "function warntype_type_printer(io::IO; @nospecialize(type), used::Bool)\nend",
        NodeKind::FunctionDefinition,
    );
    assert_root_child_kind(
        "function _setup_module!(mod::Module, Core.@nospecialize syntax_ver)\nend",
        NodeKind::FunctionDefinition,
    );
    assert_root_child_kind(
        "function test_typed_ir_printing(Base.@nospecialize(f), Base.@nospecialize(types), must_used_vars)\nend",
        NodeKind::FunctionDefinition,
    );
}

#[test]
fn test_function_head_type_expression_parameters_issue_8759() {
    assert_root_child_kind(
        "function Stateful{<:Any, Any}(itr::T) where {T}\nend",
        NodeKind::FunctionDefinition,
    );
    assert_root_child_kind(
        "function IdOffsetRange{T,IdOffsetRange{T,I}}(r::IdOffsetRange{T,I}, offset::T) where {T<:Integer,I<:AbstractUnitRange{T}}\nend",
        NodeKind::FunctionDefinition,
    );
}

#[test]
fn test_multiline_function_parameters_issue_8753() {
    assert_root_child_kind(
        "function f(x,\n    y::Int,\n    z = 1)\n  x + y + z\nend",
        NodeKind::FunctionDefinition,
    );
    assert_root_child_kind(
        "function f(\n    x::Int,\n    y::Int\n)\n  x + y\nend",
        NodeKind::FunctionDefinition,
    );
    assert_root_child_kind(
        "function f(x =\n    1)\n  x\nend",
        NodeKind::FunctionDefinition,
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

#[test]
fn test_primitive_parametric_and_interpolated_issue_8759() {
    assert_root_child_kind(
        "primitive type Date4581{T} 64 end",
        NodeKind::PrimitiveDefinition,
    );
    assert_root_child_kind(
        "primitive type C28593{S<:Real, V<:AbstractVector{S}} 32 end",
        NodeKind::PrimitiveDefinition,
    );
    assert_root_child_kind(
        "primitive type $(esc(:T)) <: Enum{$(esc(:B))} $(8) end",
        NodeKind::PrimitiveDefinition,
    );
    assert_root_child_kind(
        "@eval primitive type $(:T) <: Signed $8 end",
        NodeKind::MacrocallExpression,
    );
}

#[test]
fn test_primitive_parenthesized_bits_expression_issue_9050() {
    assert_root_child_kind(
        "primitive type ByteString58434 (18 * 8) end",
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

#[test]
fn test_function_anonymous_typed_default_arg_where_issue_8514() {
    assert_root_child_kind(
        "function foo(v::Val{N}, ::Type{T}=Float64) where {N,T<:Real}\n  T\nend",
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

#[test]
fn test_var_string_identifier_function_parameter_issue_8754() {
    assert_parses("function f(var\"my weird name\")\n  var\"my weird name\"\nend");
    assert_parses("f(var\"my weird name\") = var\"my weird name\" + 1");
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
fn test_where_soft_keyword_identifier_issue_8755() {
    assert_parses(
        "function identify_package(where::Module, name::String)\n  where.name === name\nend",
    );
    assert_parses("f(where) = where.name");
    assert_parses("where = 1");
    assert_parses("function where(x)\n  x\nend");
}

#[test]
fn test_function_where_bounded() {
    assert_parses("function foo(x::T) where T <: Number\n  x\nend");
}

#[test]
fn test_function_multiple_where() {
    assert_parses("function foo(x::T, y::S) where T where S\n  x + y\nend");
}

#[test]
fn test_function_where_supertype_bound() {
    // Lower bound only: T >: Integer (Issue #5051).
    assert_parses("foo(x::T) where {T>:Integer} = x");
}

#[test]
fn test_function_where_double_bound() {
    // Double bound: Integer <: T <: Real (Issue #5051). The constraint is
    // emitted as a SubtypeConstraint with three children [name, upper, lower].
    let source = "foo(x::T) where {Integer<:T<:Real} = x";
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));

    fn find_subtype_constraint(
        node: &subset_julia_vm_parser::cst::CstNode,
    ) -> Option<&subset_julia_vm_parser::cst::CstNode> {
        if node.kind == NodeKind::SubtypeConstraint {
            return Some(node);
        }
        node.children.iter().find_map(find_subtype_constraint)
    }

    let constraint = find_subtype_constraint(&cst)
        .expect("expected a SubtypeConstraint node for the double bound");
    assert_eq!(
        constraint.children.len(),
        3,
        "double bound should have three children [name, upper, lower]"
    );
    let text_of = |n: &subset_julia_vm_parser::cst::CstNode| n.text_from_source(source).to_string();
    assert_eq!(text_of(&constraint.children[0]), "T", "name child");
    assert_eq!(
        text_of(&constraint.children[1]),
        "Real",
        "upper bound child"
    );
    assert_eq!(
        text_of(&constraint.children[2]),
        "Integer",
        "lower bound child"
    );
}

#[test]
fn test_type_position_where_parameter_issue_8759() {
    assert_parses("eltype(::Type{TakeWhile{I,P}} where P) where {I} = eltype(I)");
    assert_parses(
        "function check_readable(a::ReinterpretArray{T, N, S} where N) where {T,S}\n  a\nend",
    );
    assert_parses("function runviews(SB::AbstractArray{T, 3} where T, indexN)\n  SB\nend");
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

#[test]
fn test_short_function_anonymous_typed_default_arg_where_issue_8514() {
    assert_parses("foo(v::Val{N}, ::Type{T}=Float64) where {N,T<:Real} = T");
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
    assert_parses("Base.:(:)(a, b) = a:b");
}

// =============================================================================
// Macro Definition
// =============================================================================

#[test]
fn test_macro_empty() {
    assert_root_child_kind("macro foo()\nend", NodeKind::MacroDefinition);
    assert_root_child_kind("macro var\"#\" end", NodeKind::MacroDefinition);
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
fn test_function_tuple_param_default_issue_8759() {
    assert_parses(
        "function iterate(itr::RegexMatchIterator, (offset,prevempty)=(1,false))\n  offset\nend",
    );
    assert_parses(
        "function iterate(I::ANSIIterator, (i, m_st)=(1, iterate(I.captures)))\n  i\nend",
    );
    assert_parses("foo((x, y)=(1, 2)) = x + y");
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

// =============================================================================
// Callable struct / functor definitions (Issue #5126)
// =============================================================================

/// Helper: locate the FunctionDefinition node's children for structural checks.
fn function_def_children(source: &str) -> Vec<NodeKind> {
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));
    assert_eq!(cst.kind, NodeKind::SourceFile);
    let func = &cst.children[0];
    assert_eq!(
        func.kind,
        NodeKind::FunctionDefinition,
        "Expected FunctionDefinition for: {}",
        source
    );
    func.children.iter().map(|c| c.kind).collect()
}

// Anonymous callable struct: full form `function (::Type)(args) ... end`
#[test]
fn test_callable_struct_anonymous_full_form() {
    assert_parses("function (::Doubler)(x)\n  x * 2\nend");
}

// Bound callable struct: full form `function (self::Type)(args) ... end`.
// The parenthesized head `(p::Poly)` must be parsed as the function name, and
// the following `(x)` as the parameter list — NOT the head as the parameter
// list and `(x)` as a body expression (Issue #5126).
#[test]
fn test_callable_struct_bound_full_form_parses() {
    assert_parses("function (p::Poly)(x)\n  p.coeff * x\nend");
}

#[test]
fn test_callable_struct_bound_full_form_structure() {
    let kinds = function_def_children("function (p::Poly)(x)\n  p.coeff * x\nend");
    // name (parenthesized head), parameter list, body block
    assert_eq!(kinds[0], NodeKind::ParenthesizedExpression);
    assert_eq!(kinds[1], NodeKind::ParameterList);
    assert_eq!(kinds[kinds.len() - 1], NodeKind::Block);
}

// Parametric bound callable struct with where clause.
#[test]
fn test_callable_struct_parametric_where_full_form() {
    assert_parses("function (s::Scaler{T})(x) where T\n  s.factor * x\nend");
}

#[test]
fn test_callable_struct_parametric_where_structure() {
    let kinds = function_def_children("function (s::Scaler{T})(x) where T\n  s.factor * x\nend");
    assert_eq!(kinds[0], NodeKind::ParenthesizedExpression);
    assert_eq!(kinds[1], NodeKind::ParameterList);
    assert!(
        kinds.contains(&NodeKind::WhereClause),
        "Expected a WhereClause child, got {:?}",
        kinds
    );
}

// Regression: a genuine anonymous function must still parse with `(x)` as its
// parameter list, not be misread as a callable-object head.
#[test]
fn test_anonymous_function_not_callable() {
    let kinds = function_def_children("function (x)\n  x + 1\nend");
    assert_eq!(kinds[0], NodeKind::ParameterList);
}

#[test]
fn test_macro_definition_interpolated_var_string_name_issue_8961() {
    assert_parses("@eval macro $(:var\"try\")(expr)\n  esc(expr)\nend");
}

#[test]
fn test_macro_definition_qualified_name_issue_9046() {
    assert_parses("macro MyMacroModule.mymacro()\nend");
}

#[test]
fn test_slurp_parameter_default_issue_9046() {
    assert_parses("function g1(a=(1,2)..., b...=3)\nend");
    assert_parses("function g3(a=(1,2)..., b=3, c...=4)\nend");
}
