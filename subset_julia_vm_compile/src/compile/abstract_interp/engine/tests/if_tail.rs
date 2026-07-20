use super::super::*;
use super::*;

// ========== Issue #3513: implicit if-branch values are NOT explicit returns ==========

#[test]
fn test_issue_3513_if_branches_assign_then_string_tail() {
    // function f(c)
    //     if c
    //         x = 1
    //     else
    //         x = 2
    //     end
    //     "done"
    // end
    // Expected return: String("done"), NOT Int64.
    let mut engine = InferenceEngine::new();

    let if_stmt = Stmt::If {
        condition: Expr::Var("c".to_string().into(), dummy_span()),
        then_branch: Block {
            stmts: vec![Stmt::Assign {
                var: "x".to_string(),
                value: Expr::Literal(Literal::Int(1), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        else_branch: Some(Block {
            stmts: vec![Stmt::Assign {
                var: "x".to_string(),
                value: Expr::Literal(Literal::Int(2), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        }),
        span: dummy_span(),
    };

    let tail = Stmt::Expr {
        expr: Expr::Literal(Literal::Str("done".to_string()), dummy_span()),
        span: dummy_span(),
    };

    let func = Function {
        name: "f".to_string(),
        params: vec![TypedParam {
            name: "c".to_string(),
            type_annotation: Some(JuliaType::Bool),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![if_stmt, tail],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    let result_str = format!("{:?}", result);
    assert!(
        result_str.contains("done"),
        "Expected return String(\"done\"), got {:?}",
        result
    );
    assert!(
        !result_str.contains("Int64") && !result_str.contains("Const(Int64"),
        "Return type should not include Int64; if-branch assignment values \
         must not be propagated as function returns. Got {:?}",
        result
    );
}

// ========== Issue #3515: only one branch returns; tail returns different type ==========

#[test]
fn test_issue_3515_if_only_then_returns_tail_int() {
    // function f(c)
    //     if c
    //         return 1
    //     else
    //         x = "not returned"
    //     end
    //     2
    // end
    // Expected return: Int64 (NOT Union{Int64, String}).
    let mut engine = InferenceEngine::new();

    let if_stmt = Stmt::If {
        condition: Expr::Var("c".to_string().into(), dummy_span()),
        then_branch: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        else_branch: Some(Block {
            stmts: vec![Stmt::Assign {
                var: "x".to_string(),
                value: Expr::Literal(Literal::Str("not returned".to_string()), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        }),
        span: dummy_span(),
    };

    let tail = Stmt::Expr {
        expr: Expr::Literal(Literal::Int(2), dummy_span()),
        span: dummy_span(),
    };

    let func = Function {
        name: "f".to_string(),
        params: vec![TypedParam {
            name: "c".to_string(),
            type_annotation: Some(JuliaType::Bool),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![if_stmt, tail],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    let result_str = format!("{:?}", result);
    assert!(
        !result_str.contains("not returned") && !result_str.contains("String"),
        "Return type should not include String — the else branch only assigned \
         to a local. Got {:?}",
        result
    );
}

// ========== Issue #3514: try/catch implicit values are NOT explicit returns ==========

#[test]
fn test_issue_3514_try_block_then_string_tail() {
    // function f(flag)
    //     try
    //         x = 1
    //     catch
    //         x = 2
    //     end
    //     "after"
    // end
    let mut engine = InferenceEngine::new();

    let try_stmt = Stmt::Try {
        try_block: Block {
            stmts: vec![Stmt::Assign {
                var: "x".to_string(),
                value: Expr::Literal(Literal::Int(1), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        catch_var: None,
        catch_block: Some(Block {
            stmts: vec![Stmt::Assign {
                var: "x".to_string(),
                value: Expr::Literal(Literal::Int(2), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        }),
        else_block: None,
        finally_block: None,
        span: dummy_span(),
    };

    let tail = Stmt::Expr {
        expr: Expr::Literal(Literal::Str("after".to_string()), dummy_span()),
        span: dummy_span(),
    };

    let func = Function {
        name: "f".to_string(),
        params: vec![TypedParam {
            name: "flag".to_string(),
            type_annotation: Some(JuliaType::Bool),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![try_stmt, tail],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    let result_str = format!("{:?}", result);
    assert!(
        result_str.contains("after"),
        "Expected return value \"after\", got {:?}",
        result
    );
    assert!(
        !result_str.contains("Int64") && !result_str.contains("Const(Int64"),
        "Try/catch branch assignments must not be propagated as function returns. \
         Got {:?}",
        result
    );
}

// ========== Existing if-as-final-expression should still work ==========

#[test]
fn test_if_as_final_expression_returns_branch_join() {
    // function f(c)
    //     if c
    //         1
    //     else
    //         2
    //     end
    // end
    // Expected return: Int64 (the if's branch values, since if is the final stmt).
    let mut engine = InferenceEngine::new();

    let if_stmt = Stmt::If {
        condition: Expr::Var("c".to_string().into(), dummy_span()),
        then_branch: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Literal(Literal::Int(1), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        else_branch: Some(Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Literal(Literal::Int(2), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        }),
        span: dummy_span(),
    };

    let func = Function {
        name: "f".to_string(),
        params: vec![TypedParam {
            name: "c".to_string(),
            type_annotation: Some(JuliaType::Bool),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![if_stmt],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    let result_str = format!("{:?}", result);
    assert!(
        result_str.contains("Int64"),
        "Expected Int64 return when `if` is the final expression, got {:?}",
        result
    );
}
