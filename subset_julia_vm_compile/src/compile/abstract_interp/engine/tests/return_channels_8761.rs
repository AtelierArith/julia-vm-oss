//! Return-channel fallthrough coverage for `StmtResult` (Issue #8761).
//!
//! These tests sit below fixtures so a future abstract-interpreter change that
//! confuses `Return` with `MaybeReturn` fails close to the statement boundary.

use super::super::*;
use super::*;
use std::collections::BTreeSet;

fn concrete(core: CorePrimitive) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(core)))
}

fn int64() -> LatticeType {
    concrete(CorePrimitive::Int64)
}

fn string() -> LatticeType {
    concrete(CorePrimitive::String)
}

fn union_int_string() -> LatticeType {
    LatticeType::Union(BTreeSet::from([
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
    ]))
}

fn param(name: &str, ty: JuliaType) -> TypedParam {
    TypedParam {
        name: name.to_string(),
        type_annotation: Some(ty),
        is_varargs: false,
        vararg_count: None,
        span: dummy_span(),
    }
}

fn block(stmts: Vec<Stmt>) -> Block {
    Block {
        stmts,
        span: dummy_span(),
    }
}

fn func(stmts: Vec<Stmt>, params: Vec<TypedParam>) -> Function {
    Function {
        name: "return_channel_8761".to_string(),
        params,
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: block(stmts),
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn infer(stmts: Vec<Stmt>, params: Vec<TypedParam>) -> LatticeType {
    InferenceEngine::new().infer_function(&func(stmts, params))
}

fn return_int(value: i64) -> Stmt {
    Stmt::Return {
        value: Some(Expr::Literal(Literal::Int(value), dummy_span())),
        span: dummy_span(),
    }
}

fn string_tail() -> Stmt {
    Stmt::Expr {
        expr: Expr::Literal(Literal::Str("tail".to_string()), dummy_span()),
        span: dummy_span(),
    }
}

fn cond_var() -> Expr {
    Expr::Var("c".to_string().into(), dummy_span())
}

#[test]
fn if_partial_return_is_maybe_return_and_joins_tail_8761() {
    let result = infer(
        vec![
            Stmt::If {
                condition: cond_var(),
                then_branch: block(vec![return_int(1)]),
                else_branch: None,
                span: dummy_span(),
            },
            string_tail(),
        ],
        vec![param("c", JuliaType::Bool)],
    );

    assert_eq!(result, union_int_string());
}

#[test]
fn block_partial_return_is_maybe_return_and_joins_tail_8761() {
    let result = infer(
        vec![
            Stmt::Block(block(vec![Stmt::If {
                condition: cond_var(),
                then_branch: block(vec![return_int(1)]),
                else_branch: None,
                span: dummy_span(),
            }])),
            string_tail(),
        ],
        vec![param("c", JuliaType::Bool)],
    );

    assert_eq!(result, union_int_string());
}

#[test]
fn begin_letblock_partial_return_is_maybe_return_and_joins_tail_8761() {
    let result = infer(
        vec![
            Stmt::Expr {
                expr: Expr::LetBlock {
                    bindings: vec![],
                    body: block(vec![Stmt::If {
                        condition: cond_var(),
                        then_branch: block(vec![return_int(1)]),
                        else_branch: None,
                        span: dummy_span(),
                    }]),
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
            string_tail(),
        ],
        vec![param("c", JuliaType::Bool)],
    );

    assert_eq!(result, union_int_string());
}

#[test]
fn try_partial_return_is_maybe_return_and_joins_tail_8761() {
    let result = infer(
        vec![
            Stmt::Try {
                try_block: block(vec![return_int(1)]),
                catch_var: None,
                catch_block: Some(block(vec![Stmt::Expr {
                    expr: Expr::Literal(Literal::Str("caught".to_string()), dummy_span()),
                    span: dummy_span(),
                }])),
                else_block: None,
                finally_block: None,
                span: dummy_span(),
            },
            string_tail(),
        ],
        vec![],
    );

    assert_eq!(result, union_int_string());
}

#[test]
fn loop_body_return_is_maybe_return_and_joins_tail_8761() {
    let result = infer(
        vec![
            Stmt::For {
                var: "i".to_string(),
                start: Expr::Literal(Literal::Int(1), dummy_span()),
                end: Expr::Var("n".to_string().into(), dummy_span()),
                step: None,
                body: block(vec![return_int(1)]),
                span: dummy_span(),
            },
            string_tail(),
        ],
        vec![param("n", JuliaType::Int64)],
    );

    assert_eq!(result, union_int_string());
}

#[test]
fn both_returning_if_remains_unconditional_return_8761() {
    let result = infer(
        vec![
            Stmt::If {
                condition: cond_var(),
                then_branch: block(vec![return_int(1)]),
                else_branch: Some(block(vec![return_int(2)])),
                span: dummy_span(),
            },
            string_tail(),
        ],
        vec![param("c", JuliaType::Bool)],
    );

    assert_eq!(result, int64());
    assert_ne!(result, string());
}
