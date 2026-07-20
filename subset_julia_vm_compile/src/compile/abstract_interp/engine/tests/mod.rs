// Test-only subtree (already gated by `#[cfg(test)] mod tests;` in the
// parent `engine/mod.rs`); this inner allow cascades to every module declared
// below, overriding the ancestor `compile/abstract_interp/mod.rs`
// `#![deny(...)]` cascade (Issue #10908 Phase 3 of #10869).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::compile::method_table::MethodSig;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
use crate::runtime_types::ValueType;
use crate::span::Span;
use std::collections::HashMap;

mod annotations;
mod backedges;
// The #8546 measurement harness prints counter dumps to stderr by design,
// exempting it from the crate-wide `#![deny(clippy::print_stderr)]` (lib.rs,
// Issue #2888), like `compile/profile.rs`.
#[allow(clippy::print_stderr)]
mod budget_metrics_8546;
mod builtin_op_inference_3525;
mod cache_global;
mod cache_invalidation;
mod cache_key;
mod cfg_authoritative_basic;
mod cfg_authoritative_payloads;
mod cfg_worklist;
mod early_return_narrowing_8545;
mod effects;
mod exception_union_4700;
mod fields;
mod foreach;
mod if_tail;
mod interprocedural;
mod literals;
mod nested_partial_struct_4269;
mod precise_invalidation;
mod ranges;
mod recursion;
mod return_cache;
mod return_channels_8761;
mod type_value_9914;
mod work_budget_8185;

fn dummy_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

/// Builds a trivial `name(x::Int64) -> I64` method signature for tests.
fn int_identity_method_sig() -> MethodSig {
    MethodSig::for_tests(
        0,
        0,
        vec![("x".to_string(), JuliaType::Int64)],
        ValueType::I64,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    )
}

fn any_identity_method_sig() -> MethodSig {
    MethodSig::for_tests(
        0,
        0,
        vec![("x".to_string(), JuliaType::Any)],
        ValueType::Any,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    )
}

fn float_identity_method_sig() -> MethodSig {
    MethodSig::for_tests(
        0,
        0,
        vec![("x".to_string(), JuliaType::Float64)],
        ValueType::F64,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    )
}

fn zero_arg_i64_method_sig() -> MethodSig {
    MethodSig::for_tests(
        0,
        0,
        vec![],
        ValueType::I64,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    )
}

#[test]
fn issue_10133_method_body_retention_is_narrowly_type_object_gated() {
    let erased_type_branch = MethodSig::for_tests(
        0,
        7,
        vec![("t".to_string(), JuliaType::DataType)],
        ValueType::Any,
        Some(JuliaType::Union(vec![
            JuliaType::DataType,
            JuliaType::DataType,
        ])),
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    );
    assert!(InferenceEngine::method_sig_needs_body(&erased_type_branch));

    assert!(!InferenceEngine::method_sig_needs_body(
        &any_identity_method_sig()
    ));

    let mixed_union = MethodSig::for_tests(
        0,
        8,
        vec![("x".to_string(), JuliaType::Any)],
        ValueType::Any,
        Some(JuliaType::Union(vec![
            JuliaType::DataType,
            JuliaType::String,
        ])),
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    );
    assert!(!InferenceEngine::method_sig_needs_body(&mixed_union));
}

/// Builds `name(x::Int64) = x` (returns its `Int64` argument unchanged).
fn int_identity_function(name: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Int64),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

/// Builds `name(x::Any) = x` so the supplied call-site arg type determines the
/// inferred return type while all entries share one declared function shape.
fn any_identity_function(name: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Any),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn constant_string_function(name: &str, param_ty: JuliaType, value: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(param_ty),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Str(value.to_string()), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn typed_forwarder_function_5603(name: &str, param_ty: JuliaType, callee: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(param_ty),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: callee.to_string().into(),
                    args: vec![Expr::Var("x".to_string().into(), dummy_span())],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn partial_box_struct_table_5603() -> HashMap<String, StructTypeInfo> {
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Top);
    struct_table.insert(
        "PartialBox5603".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );
    struct_table
}

fn partial_box_constructor_function_5603(name: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Int64),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "PartialBox5603".to_string().into(),
                    args: vec![Expr::Var("x".to_string().into(), dummy_span())],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn partial_box_forwarder_function_5603(name: &str, callee: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Int64),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: callee.to_string().into(),
                    args: vec![Expr::Var("x".to_string().into(), dummy_span())],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn partial_box_global_function_5603(name: &str, global_name: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "PartialBox5603".to_string().into(),
                    args: vec![Expr::Var(global_name.to_string().into(), dummy_span())],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

/// Builds `name() = global_name` (a zero-arg reader that returns the value of
/// the top-level binding `global_name` directly). Used to exercise the
/// global-binding dependency tracking added for Issue #4285.
fn global_reader_function(name: &str, global_name: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var(global_name.to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn typed_global_reader_function(name: &str, param_ty: JuliaType, global_name: &str) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(param_ty),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var(global_name.to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

// ========== Issue #3505: bounded fixpoint over recursive cycles ==========

/// Helper for the mutual-recursion tests below: build a function whose body is
/// `if n == 0 return <base>; end; return <other>(n - 1)`. This is the canonical
/// shape of `is_even` / `is_odd` and our type-mixing variant `f_mix` / `g_mix`.
fn cycle_branch_function(name: &str, other: &str, base: Literal) -> Function {
    Function {
        name: name.to_string(),
        params: vec![TypedParam::new(
            "n".to_string(),
            Some(JuliaType::Int64),
            dummy_span(),
        )],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::If {
                    condition: Expr::BinaryOp {
                        op: BinaryOp::Eq,
                        left: Box::new(Expr::Var("n".to_string().into(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Int(0), dummy_span())),
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Literal(base, dummy_span())),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::Call {
                        function: other.to_string().into(),
                        args: vec![Expr::BinaryOp {
                            op: BinaryOp::Sub,
                            left: Box::new(Expr::Var("n".to_string().into(), dummy_span())),
                            right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        }],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}
