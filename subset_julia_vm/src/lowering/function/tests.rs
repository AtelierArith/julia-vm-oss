//! Tests for function lowering.

use crate::ir::core::TypedParam;
use crate::lowering::Lowering;
use crate::parser::Parser;
use crate::types::{JuliaType, TypeParam};

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

/// Helper to parse a function and return its where-clause type parameters.
fn parse_function_type_params(source: &str) -> Vec<TypeParam> {
    let mut parser = Parser::new().expect("Failed to init parser");
    let parse_outcome = parser.parse(source).expect("Failed to parse");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parse_outcome).expect("Failed to lower");
    assert!(
        !program.functions.is_empty(),
        "No function definition found"
    );
    program.functions[0].type_params.clone()
}

// Issue #5374: the long-form `function ... where {T<:Bound} ... end` lowering read
// the bound from `children[1]` of the pure parser's `BinaryExpression [T, <:, Bound]`
// — i.e. the bare operator `<:` — instead of the last child `Bound`. The dropped
// bound made base's `eltype(::Type{T}) where {T<:Number}` match any `Type{X}`.

#[test]
fn test_longform_braced_where_bound_is_type_name_not_operator() {
    let tps = parse_function_type_params(
        "function eltype(::Type{T}) where {T<:Number}\n    return T\nend",
    );
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(
        tps[0].get_upper_bound().map(String::as_str),
        Some("Number"),
        "long-form braced where bound must be the type name, not the `<:` operator: {:?}",
        tps[0]
    );
}

#[test]
fn test_longform_unbraced_where_bound_is_type_name_not_operator() {
    let tps = parse_function_type_params("function f(x::T) where T<:Real\n    x\nend");
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(
        tps[0].get_upper_bound().map(String::as_str),
        Some("Real"),
        "long-form unbraced where bound must be the type name, not the `<:` operator: {:?}",
        tps[0]
    );
}

#[test]
fn test_longform_unbounded_where_has_no_bound() {
    let tps = parse_function_type_params("function f(x::T) where {T}\n    x\nend");
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(
        tps[0].get_upper_bound(),
        None,
        "unbounded where must not invent a bound: {:?}",
        tps[0]
    );
}

// Issue #6537: assignment-form OPERATOR methods (`*(a, b) where {T<:Real} = ...`)
// lowered through `lower_operator_method`, whose ad-hoc where-clause loop dropped
// the braced bound (behaved as `where {T}`). The function-form path and the
// assignment-form non-operator path both kept the bound.

#[test]
fn test_operator_assignform_braced_where_keeps_upper_bound() {
    let tps =
        parse_function_type_params("*(a::Wrap{T}, b::Wrap{T}) where {T<:Real} = \"wrap-real\"");
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(
        tps[0].get_upper_bound().map(String::as_str),
        Some("Real"),
        "assignment-form operator braced where must keep the upper bound: {:?}",
        tps[0]
    );
    // `bound` is a live field consulted by julia_type/comparison.rs; it must stay
    // in sync with `upper_bound` (Issue #6518).
    assert_eq!(
        tps[0].bound.as_deref(),
        Some("Real"),
        "live `bound` field must mirror upper_bound: {:?}",
        tps[0]
    );
}

#[test]
fn test_operator_assignform_braced_where_multi_typevar_bounds() {
    let tps = parse_function_type_params(
        "==(a::Wrap{T}, b::Wrap{S}) where {T<:Real, S<:Number} = \"both\"",
    );
    assert_eq!(tps.len(), 2, "expected two type parameters, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(tps[0].get_upper_bound().map(String::as_str), Some("Real"));
    assert_eq!(tps[0].bound.as_deref(), Some("Real"));
    assert_eq!(tps[1].name, "S");
    assert_eq!(tps[1].get_upper_bound().map(String::as_str), Some("Number"));
    assert_eq!(tps[1].bound.as_deref(), Some("Number"));
}

#[test]
fn test_operator_assignform_braced_where_unbounded_stays_unbounded() {
    let tps = parse_function_type_params("+(a::Wrap{T}, b::Wrap{S}) where {T, S} = \"generic\"");
    assert_eq!(tps.len(), 2, "expected two type parameters, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(tps[0].get_upper_bound(), None);
    assert_eq!(tps[1].name, "S");
    assert_eq!(tps[1].get_upper_bound(), None);
}

#[test]
fn test_operator_assignform_where_converts_params_to_typevars() {
    // The non-operator path converts `Wrap{T}` params via convert_params_with_type_vars;
    // the operator path must match (param annotations should see typevar bounds).
    let params = parse_function_params("*(a::T, b::T) where {T<:Real} = a");
    assert_eq!(params.len(), 2);
    assert!(
        matches!(
            &params[0].type_annotation,
            Some(JuliaType::TypeVar(name, Some(bound))) if name == "T" && bound == "Real"
        ),
        "operator assignment-form param `a::T` must become a bounded TypeVar: {:?}",
        params[0].type_annotation
    );
}

#[test]
fn test_assignform_covariant_user_parametric_annotation_issue_8360() {
    let params = parse_function_params("f(x::H{<:Real}, z) = x");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::Struct("H{<:Real}".to_string()))
    );
}

#[test]
fn test_assignform_parametric_union_alias_annotation_issue_8360() {
    let params = parse_function_params("const U{T}=Union{H{T},S{T}}\ng(x::U{T}) where {T<:Real}=x");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::Union(vec![
            JuliaType::Struct("H{T}".to_string()),
            JuliaType::Struct("S{T}".to_string()),
        ]))
    );
}

#[test]
fn test_where_bound_expands_type_alias_issue_8406() {
    let tps = parse_function_type_params(
        "const RingElement = Union{RingElem,Integer,Rational,AbstractFloat}\n\
         f(x::Box{T}) where {T<:RingElement} = x",
    );
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(
        tps[0].get_upper_bound().map(String::as_str),
        Some("Union{RingElem, Integer, Rational, AbstractFloat}"),
        "where-clause bounds must expand aliases before method dispatch sees them: {:?}",
        tps[0]
    );
    assert_eq!(
        tps[0].bound.as_deref(),
        Some("Union{RingElem, Integer, Rational, AbstractFloat}"),
        "legacy `bound` mirror must match expanded upper_bound: {:?}",
        tps[0]
    );
}

#[test]
fn test_operator_assignform_unbraced_where_bound_parses_and_keeps_bound() {
    // The unbraced form previously failed to parse entirely (`expected Eq`):
    // the where-clause constraint was parsed with the general expression
    // parser, which swallowed `= body` as an Assignment (Issue #6537).
    let tps = parse_function_type_params("*(a::Wrap{T}, b::Wrap{T}) where T<:Real = \"wrap-real\"");
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(tps[0].get_upper_bound().map(String::as_str), Some("Real"));
    assert_eq!(tps[0].bound.as_deref(), Some("Real"));
}

#[test]
fn test_operator_assignform_unbraced_where_unbounded_parses() {
    let tps = parse_function_type_params("+(a::Wrap{T}, b::Wrap{T}) where T = \"any\"");
    assert_eq!(tps.len(), 1, "expected one type parameter, got {tps:?}");
    assert_eq!(tps[0].name, "T");
    assert_eq!(tps[0].get_upper_bound(), None);
}

#[test]
fn test_operator_assignform_chained_where_clauses() {
    // `where T where S` chains fold into one WhereClause with both typevars.
    let tps =
        parse_function_type_params("*(a::Wrap{T}, b::Wrap{S}) where S<:Number where T = \"x\"");
    assert_eq!(tps.len(), 2, "expected two type parameters, got {tps:?}");
    assert_eq!(tps[0].name, "S");
    assert_eq!(tps[0].get_upper_bound().map(String::as_str), Some("Number"));
    assert_eq!(tps[1].name, "T");
    assert_eq!(tps[1].get_upper_bound(), None);
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
fn test_parse_covariant_matrix_bound_short_form_issue_4020() {
    let params = parse_function_params("f(A::Matrix{<:Integer}) = 1");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "A");
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::MatrixOf(Box::new(JuliaType::TypeVar(
            "_".to_string(),
            Some("Integer".to_string())
        ))))
    );
}

#[test]
fn test_parse_abstract_vector_typevar_short_form_issue_6239() {
    let params = parse_function_params("f(::Type{T}, ::AbstractVector{T}) where {T<:Real} = 1");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractVector{T}".to_string()))
    );
}

#[test]
fn test_parse_tuple_anonymous_bounded_slots_issue_6251() {
    let params = parse_function_params("f(::Tuple{<:Real,<:Real}) = 1");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TupleOf(vec![
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ]))
    );
}

#[test]
fn test_parse_tuple_diagonal_short_form_bound_issue_6251() {
    let params = parse_function_params("f(::Tuple{T,T}) where {T<:Real} = 1");
    let type_params = parse_function_type_params("f(::Tuple{T,T}) where {T<:Real} = 1");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TupleOf(vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ]))
    );
    assert_eq!(type_params.len(), 1);
    assert_eq!(type_params[0].name, "T");
    assert_eq!(
        type_params[0].get_upper_bound().map(String::as_str),
        Some("Real")
    );
}

#[test]
fn test_parse_abstract_vector_typevar_long_form_issue_6239() {
    let params =
        parse_function_params("function f(::Type{T}, ::AbstractVector{T}) where {T<:Real}\n1\nend");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractVector{T}".to_string()))
    );
}

#[test]
fn test_parse_abstract_matrix_typevar_short_form_issue_6240() {
    let params = parse_function_params("f(::Type{T}, ::AbstractMatrix{T}) where {T<:Real} = 1");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractMatrix{T}".to_string()))
    );
}

#[test]
fn test_parse_abstract_array_rank2_typevar_short_form_issue_6243() {
    let params = parse_function_params("f(::Type{T}, ::AbstractArray{T,2}) where {T<:Real} = 1");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractArray{T, 2}".to_string()))
    );
}

#[test]
fn test_parse_abstract_array_rank1_typevar_short_form_issue_6245() {
    let params = parse_function_params("f(::Type{T}, ::AbstractArray{T,1}) where {T<:Real} = 1");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractArray{T, 1}".to_string()))
    );
}

#[test]
fn test_parse_abstract_array_rank_omitted_typevar_short_form_issue_6247() {
    let params = parse_function_params("f(::Type{T}, ::AbstractArray{T}) where {T<:Real} = 1");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractArray{T}".to_string()))
    );
}

#[test]
fn test_parse_abstract_array_rank_typevar_short_form_issue_6249() {
    let params = parse_function_params("f(::Type{T}, ::AbstractArray{T,N}) where {T<:Real,N} = 1");
    assert_eq!(params.len(), 2);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            None
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractArray{T, N}".to_string()))
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
