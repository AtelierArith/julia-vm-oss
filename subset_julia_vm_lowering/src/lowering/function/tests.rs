//! Tests for function lowering.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::ir::core::{Expr, Stmt, TypedParam};
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

#[test]
fn builtin_spelled_where_binder_is_dynamic_parametric_base_issue_10934() {
    let source = "f(::Type{Float64}) where Float64 = Float64{Int64}";
    let parser_result = Parser::new();
    assert!(parser_result.is_ok(), "failed to initialize parser");
    let Ok(mut parser) = parser_result else {
        return;
    };
    let parse_result = parser.parse(source);
    assert!(parse_result.is_ok(), "failed to parse Issue #10934 source");
    let Ok(parse_outcome) = parse_result else {
        return;
    };
    let mut lowering = Lowering::new(source);
    let lower_result = lowering.lower(parse_outcome);
    assert!(lower_result.is_ok(), "failed to lower Issue #10934 source");
    let Ok(program) = lower_result else {
        return;
    };
    let function = program.functions.first();
    assert!(function.is_some(), "missing lowered Issue #10934 function");
    let Some(function) = function else {
        return;
    };
    let body_stmt = function.body.stmts.first();
    assert!(
        matches!(
            body_stmt,
            Some(Stmt::Return {
                value: Some(Expr::DynamicTypeConstruct {
                    base_expr: Some(_),
                    ..
                }),
                ..
            })
        ),
        "builtin-spelled lexical binder must lower as a dynamic type base: {:?}",
        function.body
    );
    let Some(Stmt::Return {
        value:
            Some(Expr::DynamicTypeConstruct {
                base,
                base_expr: Some(base_expr),
                ..
            }),
        ..
    }) = body_stmt
    else {
        return;
    };
    assert_eq!(base, "Float64");
    assert!(
        matches!(base_expr.as_ref(), Expr::Var(name, _) if name == "Float64"),
        "dynamic base must read the lexical binder: {base_expr:?}"
    );
}

fn lowered_program(source: &str) -> Result<crate::ir::core::Program, String> {
    let mut parser = Parser::new().map_err(|err| format!("failed to initialize parser: {err}"))?;
    let parse_outcome = parser
        .parse(source)
        .map_err(|err| format!("failed to parse source: {err}"))?;
    let mut lowering = Lowering::new(source);
    lowering
        .lower(parse_outcome)
        .map_err(|err| format!("failed to lower source: {err}"))
}

#[test]
fn assigned_arrow_inherits_enclosing_where_binder_issue_11031() -> Result<(), String> {
    let program = lowered_program(
        r#"
        function outer(x::Float64) where Float64
            f = () -> Vector{Float64}
            f()
        end
        "#,
    )?;
    assert_eq!(
        program.functions.len(),
        1,
        "assigned arrow inside a closure-aware body must remain nested instead of being lifted flat: {program:#?}"
    );
    let ir = format!("{program:#?}");
    assert_eq!(
        ir.matches("DynamicTypeConstruct").count(),
        1,
        "assigned-arrow body must read the enclosing where binder dynamically: {ir}"
    );
    Ok(())
}

#[test]
fn nested_function_inherits_enclosing_where_binder_issue_11031() -> Result<(), String> {
    let program = lowered_program(
        r#"
        function outer(x::Float64) where Float64
            function inner()
                Vector{Float64}
            end
            inner()
        end
        "#,
    )?;
    let ir = format!("{program:#?}");
    assert_eq!(
        ir.matches("DynamicTypeConstruct").count(),
        1,
        "nested function body must read the enclosing where binder dynamically: {ir}"
    );
    Ok(())
}

#[test]
fn nested_short_function_inherits_enclosing_where_binder_issue_11031() -> Result<(), String> {
    let program = lowered_program(
        r#"
        function outer(x::Float64) where Float64
            inner() = Vector{Float64}
            inner()
        end
        "#,
    )?;
    let ir = format!("{program:#?}");
    assert_eq!(
        ir.matches("DynamicTypeConstruct").count(),
        1,
        "short-form nested function body must read the enclosing where binder dynamically: {ir}"
    );
    Ok(())
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
fn test_assignform_abstract_range_where_param_issue_10150() {
    let params = parse_function_params("range_t(r::AbstractRange{T}) where {T} = T");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::Struct("AbstractRange{T}".to_string()))
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
fn test_assignform_underscore_parametric_annotation_issue_9472() {
    let params = parse_function_params("describe(_::Container{Int64}) = \"int\"");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::Struct("Container{Int64}".to_string()))
    );
}

#[test]
fn test_assignform_namedtuple_names_only_annotation_issue_5063() {
    let params = parse_function_params("f(x::NamedTuple{(:a, :b)}) = x");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::Struct("NamedTuple{(:a, :b)}".to_string()))
    );

    let params = parse_function_params("g(::NamedTuple{(:a, :b)}) = 1");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::Struct("NamedTuple{(:a, :b)}".to_string()))
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
            Some("Real".to_string())
        ))))
    );
    assert_eq!(
        params[1].type_annotation,
        Some(JuliaType::Struct("AbstractVector{T}".to_string()))
    );
}

#[test]
fn test_parse_concrete_user_type_object_parameter_issue_10782() {
    let params =
        parse_function_params("Base.isbitstype(::Type{LayoutPredicateDispatchBox3911}) = 1");
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0].type_annotation,
        Some(JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "LayoutPredicateDispatchBox3911".to_string()
        ))))
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
            JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
            JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
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
            Some("Real".to_string())
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
            Some("Real".to_string())
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
            Some("Real".to_string())
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
            Some("Real".to_string())
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
            Some("Real".to_string())
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
            Some("Real".to_string())
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

// ── LambdaContext routing authority contract (Issues #10936 / #10965) ───────
//
// `function_lowering_capabilities` is the single routing authority for
// function-definition lowering. These tests are the MUTATION CONTRACT for the
// prevention Issues #10936 and #10965:
//
// - Reverting `requires_context` to a narrow `contains_macro_call` predicate
//   (the #10934 root cause) fails `where_binder_without_macro_requires_context`.
// - Broadening `requires_nested_lambda_lowering` to include
//   `parametric_type_applications` (the first #10948 attempt, which broke
//   closures/SROA/generators) fails
//   `parametric_type_alone_must_not_switch_nested_lowering`.
mod lambda_context_routing_authority {
    use crate::lowering::function_lowering_capabilities;
    use crate::parser::cst::{CstWalker, Node};
    use crate::parser::{ParseOutcome, Parser};

    fn with_first_node_capabilities<T>(
        source: &str,
        f: impl FnOnce(crate::lowering::FunctionLoweringCapabilities) -> T,
    ) -> Option<T> {
        let parser = Parser::new();
        assert!(parser.is_ok(), "parser init failed");
        let Ok(mut parser) = parser else {
            return None;
        };
        let outcome = parser.parse(source);
        assert!(outcome.is_ok(), "parse failed: {source}");
        let Ok(outcome) = outcome else {
            return None;
        };
        let ParseOutcome::Rust(parsed) = &outcome;
        let walker = CstWalker::new(parsed.source());
        let root = Node::new(parsed.root(), parsed.source());
        let node = walker.named_children(&root).next();
        assert!(node.is_some(), "no top-level node: {source}");
        let node = node?;
        Some(f(function_lowering_capabilities(&walker, node)))
    }

    #[test]
    fn where_binder_without_macro_requires_context() {
        // Issue #10936 negative mutation: a routing predicate that only looks
        // for macro calls loses the where-binder edge.
        let _ = with_first_node_capabilities(
            "f(x::Float64) where Float64<:Real = Vector{Float64}",
            |caps| {
                assert!(caps.where_binders, "where binder must be detected");
                assert!(!caps.macro_expansion, "no macro call in this source");
                assert!(
                    caps.requires_context(),
                    "where binder alone must retain the LambdaContext"
                );
                assert!(
                    caps.requires_nested_lambda_lowering(),
                    "where binder bodies must thread binder state into nested definitions"
                );
            },
        );
    }

    #[test]
    fn macro_call_requires_context_and_nested_lowering() {
        let _ = with_first_node_capabilities("function f(x)\n@assert true\nx\nend", |caps| {
            assert!(caps.macro_expansion);
            assert!(caps.requires_context());
            assert!(caps.requires_nested_lambda_lowering());
        });
    }

    #[test]
    fn parametric_type_alone_must_not_switch_nested_lowering() {
        // Issue #10965 negative mutation: entering nested-closure lowering for
        // every parametric type expression changes closure representation and
        // capture analysis for unrelated `Tuple{Int64}` / `Complex{Float64}`
        // bodies (the first #10948 attempt's full-suite regressions).
        let _ = with_first_node_capabilities("function f(n)\nT = Tuple{Int64}\nn\nend", |caps| {
            assert!(
                caps.parametric_type_applications,
                "parametric type application must be detected"
            );
            assert!(!caps.where_binders);
            assert!(!caps.macro_expansion);
            assert!(
                caps.requires_context(),
                "lexical binder lookup needs the context"
            );
            assert!(
                !caps.requires_nested_lambda_lowering(),
                "a static parametric type expression must NOT change closure lowering mode"
            );
        });
    }

    #[test]
    fn plain_function_requires_nothing() {
        let _ = with_first_node_capabilities("f(x) = x + 1", |caps| {
            assert!(!caps.macro_expansion);
            assert!(!caps.where_binders);
            assert!(!caps.parametric_type_applications);
            assert!(!caps.requires_context());
            assert!(!caps.requires_nested_lambda_lowering());
        });
    }
}

// ── Builtin-spelled where binder in SIGNATURE annotations (Issue #10942) ────
mod builtin_spelled_where_binder_signature_10942 {
    use super::{parse_function_params, parse_function_type_params};
    use crate::types::JuliaType;

    #[test]
    fn type_annotation_lowers_binder_as_typevar() {
        let params = parse_function_params("f(::Type{Float64}) where Float64 = Float64{Int64}");
        assert!(
            matches!(
                params[0].type_annotation.as_ref(),
                Some(JuliaType::TypeOf(inner))
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, None) if name == "Float64")
            ),
            "Type{{Float64}} under `where Float64` must lower to Type{{TypeVar}}: {:?}",
            params[0].type_annotation
        );
        let type_params =
            parse_function_type_params("f(::Type{Float64}) where Float64 = Float64{Int64}");
        assert!(type_params.iter().any(|tp| tp.name == "Float64"));
    }

    #[test]
    fn vector_annotation_lowers_binder_as_typevar() {
        let params = parse_function_params("f(x::Vector{Float64}) where Float64 = Float64");
        assert!(
            matches!(
                params[0].type_annotation.as_ref(),
                Some(JuliaType::VectorOf(inner))
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, None) if name == "Float64")
            ),
            "Vector{{Float64}} under `where Float64` must lower to Vector{{TypeVar}}: {:?}",
            params[0].type_annotation
        );
    }

    #[test]
    fn nested_type_annotation_lowers_binder_as_typevar() {
        let params = parse_function_params("f(x::Type{Vector{Float64}}) where Float64 = Float64");
        assert!(
            matches!(
                params[0].type_annotation.as_ref(),
                Some(JuliaType::TypeOf(v))
                    if matches!(
                        v.as_ref(),
                        JuliaType::VectorOf(inner)
                            if matches!(inner.as_ref(), JuliaType::TypeVar(name, None) if name == "Float64")
                    )
            ),
            "{:?}",
            params[0].type_annotation
        );
    }

    #[test]
    fn alias_spelling_is_not_shadowed() {
        // Name-based shadowing only: `Type{Int}` under `where Int64` keeps the
        // builtin alias target — the binder shadows the spelling `Int64`, not
        // the type the alias resolves to.
        let params = parse_function_params("f(x::Type{Int}) where {Int64} = 0");
        assert!(
            matches!(
                params[0].type_annotation.as_ref(),
                Some(JuliaType::TypeOf(inner)) if matches!(inner.as_ref(), JuliaType::Int64)
            ),
            "{:?}",
            params[0].type_annotation
        );
    }

    #[test]
    fn unshadowed_builtin_annotation_is_unchanged() {
        let params = parse_function_params("f(::Type{Float64}) = 1");
        assert!(
            matches!(
                params[0].type_annotation.as_ref(),
                Some(JuliaType::TypeOf(inner)) if matches!(inner.as_ref(), JuliaType::Float64)
            ),
            "{:?}",
            params[0].type_annotation
        );
    }
}
