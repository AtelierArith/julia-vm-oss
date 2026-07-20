//! Flow-sensitive early-return narrowing (Issue #8545).
//!
//! When one arm of a branch terminates (return/throw/break/continue), the
//! NEGATED condition applies to the fall-through state instead of joining
//! both arms, so guards like `isnothing(x) && return 0` narrow `x` for the
//! rest of the enclosing block.

use super::super::*;
use super::*;

fn int64() -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )))
}

fn union_int_nothing() -> JuliaType {
    JuliaType::Union(vec![JuliaType::Int64, JuliaType::Nothing])
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

fn func(name: &str, params: Vec<TypedParam>, stmts: Vec<Stmt>) -> Function {
    Function {
        name: name.to_string(),
        params,
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts,
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

fn x_egal_nothing() -> Expr {
    Expr::BinaryOp {
        op: BinaryOp::Egal,
        left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
        right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
        span: dummy_span(),
    }
}

fn x_isa(type_name: &str) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::Isa,
        args: vec![
            Expr::Var("x".to_string().into(), dummy_span()),
            Expr::Var(type_name.to_string().into(), dummy_span()),
        ],
        span: dummy_span(),
    }
}

fn return_expr(value: i64) -> Expr {
    Expr::ReturnExpr {
        value: Some(Box::new(Expr::Literal(Literal::Int(value), dummy_span()))),
        span: dummy_span(),
    }
}

fn guard(op: BinaryOp, condition: Expr, terminator: Expr) -> Stmt {
    Stmt::Expr {
        expr: Expr::BinaryOp {
            op,
            left: Box::new(condition),
            right: Box::new(terminator),
            span: dummy_span(),
        },
        span: dummy_span(),
    }
}

fn x_plus_one() -> Expr {
    Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
        right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
        span: dummy_span(),
    }
}

// ========== `cond && return v` narrows the fall-through ==========

#[test]
fn guard_and_return_narrows_fallthrough_8545() {
    // f(x::Union{Int64,Nothing}) = (x === nothing && return 0; x + 1)
    let mut engine = InferenceEngine::new();
    let f = func(
        "f",
        vec![param("x", union_int_nothing())],
        vec![
            guard(BinaryOp::And, x_egal_nothing(), return_expr(0)),
            Stmt::Expr {
                expr: x_plus_one(),
                span: dummy_span(),
            },
        ],
    );
    let result = engine.infer_function(&f);
    assert_eq!(
        result,
        int64(),
        "fall-through `x + 1` must see x narrowed to Int64 (negated guard), got {:?}",
        result
    );
}

#[test]
fn guard_or_return_narrows_fallthrough_8545() {
    // f(x::Union{Int64,Nothing}) = (x isa Int64 || return 0; x + 1)
    let mut engine = InferenceEngine::new();
    let f = func(
        "f",
        vec![param("x", union_int_nothing())],
        vec![
            guard(BinaryOp::Or, x_isa("Int64"), return_expr(0)),
            Stmt::Expr {
                expr: x_plus_one(),
                span: dummy_span(),
            },
        ],
    );
    let result = engine.infer_function(&f);
    assert_eq!(
        result,
        int64(),
        "fall-through of `cond || return` keeps the THEN split, got {:?}",
        result
    );
}

// ========== `if` with a terminating arm keeps the surviving env ==========

#[test]
fn if_then_returns_fallthrough_keeps_else_env_8545() {
    // function f(x::Union{Int64,Nothing})
    //     if x === nothing
    //         return 0
    //     end
    //     return x + 1     # x :: Int64 here
    // end
    let mut engine = InferenceEngine::new();
    let f = func(
        "f",
        vec![param("x", union_int_nothing())],
        vec![
            Stmt::If {
                condition: x_egal_nothing(),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(Literal::Int(0), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: None,
                span: dummy_span(),
            },
            Stmt::Return {
                value: Some(x_plus_one()),
                span: dummy_span(),
            },
        ],
    );
    let result = engine.infer_function(&f);
    assert_eq!(
        result,
        int64(),
        "post-if state must be the else split (x::Int64), got {:?}",
        result
    );
}

#[test]
fn if_else_returns_fallthrough_keeps_then_env_8545() {
    // function f(x::Union{Int64,Nothing})
    //     if x isa Int64
    //     else
    //         return 0
    //     end
    //     return x + 1     # x :: Int64 here
    // end
    let mut engine = InferenceEngine::new();
    let f = func(
        "f",
        vec![param("x", union_int_nothing())],
        vec![
            Stmt::If {
                condition: x_isa("Int64"),
                then_branch: Block {
                    stmts: vec![],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(Literal::Int(0), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            Stmt::Return {
                value: Some(x_plus_one()),
                span: dummy_span(),
            },
        ],
    );
    let result = engine.infer_function(&f);
    assert_eq!(
        result,
        int64(),
        "post-if state must be the then split (x::Int64), got {:?}",
        result
    );
}

// ========== narrowing survives >= 2 successive guard blocks ==========

#[test]
fn chained_guards_narrow_across_blocks_8545() {
    // function f(x::Union{Int64,Nothing,String})
    //     x === nothing && return -1
    //     x isa String && return -2
    //     return x + 1     # x :: Int64 here
    // end
    let mut engine = InferenceEngine::new();
    let f = func(
        "f",
        vec![param(
            "x",
            JuliaType::Union(vec![
                JuliaType::Int64,
                JuliaType::Nothing,
                JuliaType::String,
            ]),
        )],
        vec![
            guard(BinaryOp::And, x_egal_nothing(), return_expr(-1)),
            guard(BinaryOp::And, x_isa("String"), return_expr(-2)),
            Stmt::Return {
                value: Some(x_plus_one()),
                span: dummy_span(),
            },
        ],
    );
    let result = engine.infer_function(&f);
    assert_eq!(
        result,
        int64(),
        "each guard must remove one union member from the fall-through, got {:?}",
        result
    );
}

// ========== soundness: a partially-returning arm is NOT terminating ==========

#[test]
fn partial_return_then_arm_does_not_drop_surviving_env_8545() {
    // function f(x::Union{Int64,String}, c::Bool)
    //     if x isa Int64
    //         if c
    //             return 0
    //         end
    //         # falls through — then-arm does NOT terminate
    //     else
    //         return -1
    //     end
    //     return x + 1     # x :: Int64 (then-arm fell through)
    // end
    //
    // A naive `StmtResult::Return`-based termination check would treat the
    // then-arm as terminating (its nested `if` conditionally returns) and
    // wrongly keep the ELSE env (x::String) for the tail.
    let mut engine = InferenceEngine::new();
    let f = func(
        "f",
        vec![
            param(
                "x",
                JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]),
            ),
            param("c", JuliaType::Bool),
        ],
        vec![
            Stmt::If {
                condition: x_isa("Int64"),
                then_branch: Block {
                    stmts: vec![Stmt::If {
                        condition: Expr::Var("c".to_string().into(), dummy_span()),
                        then_branch: Block {
                            stmts: vec![Stmt::Return {
                                value: Some(Expr::Literal(Literal::Int(0), dummy_span())),
                                span: dummy_span(),
                            }],
                            span: dummy_span(),
                        },
                        else_branch: None,
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(Literal::Int(-1), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            },
            Stmt::Return {
                value: Some(x_plus_one()),
                span: dummy_span(),
            },
        ],
    );
    let result = engine.infer_function(&f);
    assert_eq!(
        result,
        int64(),
        "then-arm falls through, so the tail must see x::Int64 (else env would \
         leak x::String into `x + 1`), got {:?}",
        result
    );
}

// ========== structural termination checks ==========

#[test]
fn block_always_terminates_structural_cases_8545() {
    let ret = Stmt::Return {
        value: None,
        span: dummy_span(),
    };
    let expr_stmt = |expr: Expr| Stmt::Expr {
        expr,
        span: dummy_span(),
    };
    let block = |stmts: Vec<Stmt>| Block {
        stmts,
        span: dummy_span(),
    };

    // Unconditional return / break / continue terminate.
    assert!(block_always_terminates(&block(vec![ret.clone()])));
    assert!(block_always_terminates(&block(vec![Stmt::Break {
        span: dummy_span()
    }])));
    assert!(block_always_terminates(&block(vec![Stmt::Continue {
        span: dummy_span()
    }])));

    // A never-returning call terminates.
    let throw_call = Expr::Call {
        function: "throw".to_string().into(),
        args: vec![Expr::Literal(Literal::Int(1), dummy_span())],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: dummy_span(),
    };
    assert!(block_always_terminates(&block(vec![expr_stmt(throw_call)])));

    // A plain expression does not.
    assert!(!block_always_terminates(&block(vec![expr_stmt(
        Expr::Literal(Literal::Int(1), dummy_span())
    )])));
    assert!(!block_always_terminates(&block(vec![])));

    // `if` with only a then-arm never terminates unconditionally.
    let partial_if = Stmt::If {
        condition: Expr::Var("c".to_string().into(), dummy_span()),
        then_branch: block(vec![ret.clone()]),
        else_branch: None,
        span: dummy_span(),
    };
    assert!(!block_always_terminates(&block(vec![partial_if])));

    // `if` terminates only when BOTH arms terminate.
    let full_if = Stmt::If {
        condition: Expr::Var("c".to_string().into(), dummy_span()),
        then_branch: block(vec![ret.clone()]),
        else_branch: Some(block(vec![ret.clone()])),
        span: dummy_span(),
    };
    assert!(block_always_terminates(&block(vec![full_if])));

    // A guard statement (`cond && return`) is conditional — not terminating.
    let guard_stmt = guard(BinaryOp::And, x_egal_nothing(), return_expr(0));
    assert!(!block_always_terminates(&block(vec![guard_stmt])));
}
