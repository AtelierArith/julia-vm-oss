#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::ir::core::MetaAnnotation;

fn sp() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

fn var(name: &str) -> Expr {
    Expr::Var(name.to_string().into(), sp())
}

fn int(value: i64) -> Expr {
    Expr::Literal(Literal::Int(value), sp())
}

fn nothing() -> Expr {
    Expr::Literal(Literal::Nothing, sp())
}

fn add(left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
        span: sp(),
    }
}

fn mul(left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        op: BinaryOp::Mul,
        left: Box::new(left),
        right: Box::new(right),
        span: sp(),
    }
}

fn egal(left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        op: BinaryOp::Egal,
        left: Box::new(left),
        right: Box::new(right),
        span: sp(),
    }
}

fn call(function: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        function: function.to_string().into(),
        args,
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: sp(),
    }
}

fn length_call(name: &str) -> Expr {
    call("length", vec![var(name)])
}

fn block(stmts: Vec<Stmt>) -> Block {
    Block { stmts, span: sp() }
}

fn optimizer() -> IrOptimizer {
    IrOptimizer::new(HashMap::new())
}

fn function(name: &str, body: Block) -> Function {
    Function {
        name: name.to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span: sp(),
        new_struct_name: None,
    }
}

fn empty_program(functions: Vec<Function>) -> Program {
    Program {
        abstract_types: vec![],
        primitive_types: vec![],
        type_aliases: vec![],
        structs: vec![],
        functions: functions.into_iter().map(std::sync::Arc::new).collect(),
        base_function_count: 0,
        modules: vec![],
        usings: vec![],
        macros: vec![],
        enums: vec![],
        main: block(vec![]),
    }
}

fn expr_contains_var_prefix(expr: &Expr, prefix: &str) -> bool {
    match expr {
        Expr::Var(name, _) => name.starts_with(prefix),
        Expr::UnaryOp { operand, .. }
        | Expr::FieldAccess {
            object: operand, ..
        } => expr_contains_var_prefix(operand, prefix),
        Expr::BinaryOp { left, right, .. } => {
            expr_contains_var_prefix(left, prefix) || expr_contains_var_prefix(right, prefix)
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            args.iter().any(|arg| expr_contains_var_prefix(arg, prefix))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_contains_var_prefix(value, prefix))
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            args.iter().any(|arg| expr_contains_var_prefix(arg, prefix))
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => elements
            .iter()
            .any(|element| expr_contains_var_prefix(element, prefix)),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_contains_var_prefix(value, prefix)),
        Expr::Pair { key, value, .. } => {
            expr_contains_var_prefix(key, prefix) || expr_contains_var_prefix(value, prefix)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
            expr_contains_var_prefix(key, prefix) || expr_contains_var_prefix(value, prefix)
        }),
        Expr::Index { array, indices, .. } => {
            expr_contains_var_prefix(array, prefix)
                || indices
                    .iter()
                    .any(|index| expr_contains_var_prefix(index, prefix))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_contains_var_prefix(start, prefix)
                || step
                    .as_ref()
                    .is_some_and(|step| expr_contains_var_prefix(step, prefix))
                || expr_contains_var_prefix(stop, prefix)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var_prefix(condition, prefix)
                || expr_contains_var_prefix(then_expr, prefix)
                || expr_contains_var_prefix(else_expr, prefix)
        }
        _ => false,
    }
}

#[test]
fn straight_line_cse_reuses_prior_local_issue_5185() {
    let input = block(vec![
        Stmt::Assign {
            var: "a".to_string(),
            value: add(var("x"), int(1)),
            span: sp(),
        },
        Stmt::Assign {
            var: "b".to_string(),
            value: add(var("x"), int(1)),
            span: sp(),
        },
    ]);

    let output = optimizer().optimize_block(&input);

    assert!(matches!(
        &output.stmts[1],
        Stmt::Assign {
            var,
            value: Expr::Var(source, _),
            ..
        } if var == "b" && source == "a"
    ));
}

#[test]
fn straight_line_cse_invalidates_mutated_inputs_issue_5185() {
    let input = block(vec![
        Stmt::Assign {
            var: "a".to_string(),
            value: add(var("x"), int(1)),
            span: sp(),
        },
        Stmt::Assign {
            var: "x".to_string(),
            value: int(10),
            span: sp(),
        },
        Stmt::Assign {
            var: "b".to_string(),
            value: add(var("x"), int(1)),
            span: sp(),
        },
    ]);

    let output = optimizer().optimize_block(&input);

    assert!(matches!(
        &output.stmts[2],
        Stmt::Assign {
            value: Expr::BinaryOp { .. },
            ..
        }
    ));
}

#[test]
fn straight_line_cse_reuses_pure_call_issue_5185() {
    let input = block(vec![
        Stmt::Assign {
            var: "a".to_string(),
            value: length_call("xs"),
            span: sp(),
        },
        Stmt::Assign {
            var: "b".to_string(),
            value: length_call("xs"),
            span: sp(),
        },
    ]);

    let output = optimizer().optimize_block(&input);

    assert!(matches!(
        &output.stmts[1],
        Stmt::Assign {
            var,
            value: Expr::Var(source, _),
            ..
        } if var == "b" && source == "a"
    ));
}

/// Regression for Issue #9270: an RNG constructor (`MersenneTwister(seed)`,
/// `Xoshiro(seed)`, `StableRNG(seed)`) allocates a fresh, independently-
/// mutable engine, so two textually-identical constructor calls must NOT be
/// CSE'd. Before the fix `infer_builtin_op_effects` fell into the pure
/// default and value-numbered the second `MersenneTwister(123)` into a reuse
/// of the first, aliasing one engine (same class as zeros/ones, Issue #7176).
#[test]
fn rng_constructors_are_not_pure_issue_9270() {
    use crate::ir::core::BuiltinOp;
    use effect_inference::infer_expr_effects;

    for op in [
        BuiltinOp::MersenneTwisterRNG,
        BuiltinOp::XoshiroRNG,
        BuiltinOp::StableRNG,
    ] {
        let ctor = Expr::Builtin {
            name: op,
            args: vec![int(123)],
            span: sp(),
        };
        assert!(
            !infer_expr_effects(&ctor).is_pure(),
            "{op:?} must not be classified pure (would enable CSE aliasing)"
        );

        let input = block(vec![
            Stmt::Assign {
                var: "m1".to_string(),
                value: ctor.clone(),
                span: sp(),
            },
            Stmt::Assign {
                var: "m2".to_string(),
                value: ctor.clone(),
                span: sp(),
            },
        ]);
        let output = optimizer().optimize_block(&input);
        // The second construction must survive as its own Builtin, never a
        // `Expr::Var("m1", _)` reuse of the first engine.
        assert!(
            matches!(
                &output.stmts[1],
                Stmt::Assign {
                    var,
                    value: Expr::Builtin { name, .. },
                    ..
                } if var == "m2" && *name == op
            ),
            "{op:?}: second RNG construction was CSE'd into a reuse: {:?}",
            output.stmts[1]
        );
    }
}

/// Regression for Issue #9323 (sibling audit of #9270): `Ref(x)` lowers
/// directly to `Expr::Builtin { BuiltinOp::Ref }` and allocates a fresh
/// mutable single-element cell (`Rc<RefCell<Value>>`, Issue #5130). Two
/// textually-identical `Ref(0)` constructors must NOT be CSE'd, otherwise
/// `r1 = Ref(0); r2 = Ref(0); r2[] = 20` corrupts `r1[]` (observed: sjulia
/// printed `20` for `r1[]` before the fix). Same class as the RNG
/// constructors — before the audit `BuiltinOp::Ref` fell into
/// `infer_builtin_op_effects`'s `_ => pure_arithmetic()` default.
#[test]
fn ref_constructor_is_not_cse_aliased_issue_9323() {
    use crate::ir::core::BuiltinOp;
    use effect_inference::infer_expr_effects;

    let ctor = Expr::Builtin {
        name: BuiltinOp::Ref,
        args: vec![int(0)],
        span: sp(),
    };
    assert!(
        !infer_expr_effects(&ctor).is_pure(),
        "Ref(0) must not be classified pure (would enable CSE aliasing)"
    );

    let input = block(vec![
        Stmt::Assign {
            var: "r1".to_string(),
            value: ctor.clone(),
            span: sp(),
        },
        Stmt::Assign {
            var: "r2".to_string(),
            value: ctor.clone(),
            span: sp(),
        },
    ]);
    let output = optimizer().optimize_block(&input);
    // The second construction must survive as its own Builtin, never a
    // `Expr::Var("r1", _)` reuse of the first cell.
    assert!(
        matches!(
            &output.stmts[1],
            Stmt::Assign {
                var,
                value: Expr::Builtin { name, .. },
                ..
            } if var == "r2" && *name == BuiltinOp::Ref
        ),
        "second Ref construction was CSE'd into a reuse: {:?}",
        output.stmts[1]
    );
}

#[test]
fn assume_effects_total_call_is_cse_candidate_issue_8441() {
    let assumed = function(
        "assumed_total_8441",
        block(vec![
            Stmt::Meta {
                annotation: MetaAnnotation {
                    name: "assume_effects".to_string(),
                    args: vec![":total".to_string()],
                },
                span: sp(),
            },
            Stmt::Return {
                value: Some(int(1)),
                span: sp(),
            },
        ]),
    );
    let caller = function(
        "caller_8441",
        block(vec![
            Stmt::Assign {
                var: "a".to_string(),
                value: call("assumed_total_8441", vec![var("x")]),
                span: sp(),
            },
            Stmt::Assign {
                var: "b".to_string(),
                value: call("assumed_total_8441", vec![var("x")]),
                span: sp(),
            },
        ]),
    );
    let program = empty_program(vec![assumed, caller]);

    let output = optimize_pure_expressions_user_only(&program, 0);
    let caller = output
        .user_functions
        .iter()
        .find(|func| func.name == "caller_8441")
        .expect("optimized caller");

    assert!(matches!(
        &caller.body.stmts[1],
        Stmt::Assign {
            var,
            value: Expr::Var(source, _),
            ..
        } if var == "b" && source == "a"
    ));
}

#[test]
fn loop_invariant_expression_is_hoisted_issue_5185() {
    let input = block(vec![Stmt::For {
        var: "i".to_string(),
        start: int(1),
        end: int(3),
        step: None,
        body: block(vec![Stmt::Assign {
            var: "y".to_string(),
            value: add(var("limit"), int(1)),
            span: sp(),
        }]),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    assert_eq!(output.stmts.len(), 2);
    assert!(matches!(
        &output.stmts[0],
        Stmt::Assign {
            var,
            value: Expr::BinaryOp { .. },
            ..
        } if var.starts_with(HOIST_TEMP_PREFIX)
    ));
    assert!(matches!(
        &output.stmts[1],
        Stmt::For {
            body: Block { stmts, .. },
            ..
        } if matches!(
            &stmts[0],
            Stmt::Assign {
                value: Expr::Var(name, _),
                ..
            } if name.starts_with(HOIST_TEMP_PREFIX)
        )
    ));
}

#[test]
fn loop_invariant_arithmetic_is_not_hoisted_issue_5618() {
    let input = block(vec![Stmt::While {
        condition: egal(var("x"), nothing()),
        body: block(vec![Stmt::Return {
            value: Some(add(var("x"), int(1))),
            span: sp(),
        }]),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    assert_eq!(output.stmts.len(), 1);
    assert!(matches!(&output.stmts[0], Stmt::While { .. }));
}

#[test]
fn ir_opt_does_not_fold_identical_branch_assignments_issue_8440() {
    // Identical assignments in both branches are no longer folded by ir_opt
    // after the bridge retirement (Issue #8832). The fold_constants trivial-Phi
    // rule in the SSA pipeline (`ssa_ir::opt`) now handles this pattern for
    // all user functions that go through the SSA path. The Core IR (legacy)
    // path leaves the if-else intact; the identical stores are cheaper to
    // handle post-slotization than pre-slotize hoisting.
    let input = block(vec![Stmt::If {
        condition: var("flag"),
        then_branch: block(vec![Stmt::Assign {
            var: "x".to_string(),
            value: int(41),
            span: sp(),
        }]),
        else_branch: Some(block(vec![Stmt::Assign {
            var: "x".to_string(),
            value: int(41),
            span: sp(),
        }])),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    // ir_opt passes through the if-else unchanged; SSA opt handles the fold.
    assert_eq!(output.stmts.len(), 1);
    assert!(matches!(&output.stmts[0], Stmt::If { .. }));
}

#[test]
fn loop_invariant_pure_call_is_hoisted_issue_5185() {
    let input = block(vec![Stmt::For {
        var: "i".to_string(),
        start: int(1),
        end: int(3),
        step: None,
        body: block(vec![Stmt::Assign {
            var: "n".to_string(),
            value: length_call("xs"),
            span: sp(),
        }]),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    assert_eq!(output.stmts.len(), 2);
    assert!(matches!(
        &output.stmts[0],
        Stmt::Assign {
            var,
            value: Expr::Call { function, .. },
            ..
        } if var.starts_with(HOIST_TEMP_PREFIX) && function == "length"
    ));
}

#[test]
fn outer_licm_skips_nested_hoist_temp_dependencies_issue_5592() {
    let invariant = add(var("limit"), int(1));
    let input = block(vec![Stmt::For {
        var: "i".to_string(),
        start: int(1),
        end: int(3),
        step: None,
        body: block(vec![Stmt::For {
            var: "j".to_string(),
            start: int(1),
            end: int(3),
            step: None,
            body: block(vec![Stmt::Assign {
                var: "y".to_string(),
                value: mul(invariant.clone(), invariant),
                span: sp(),
            }]),
            span: sp(),
        }]),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    for stmt in output
        .stmts
        .iter()
        .take_while(|stmt| !matches!(stmt, Stmt::For { var, .. } if var == "i"))
    {
        let Stmt::Assign { value, .. } = stmt else {
            continue;
        };
        assert!(
            !expr_contains_var_prefix(value, HOIST_TEMP_PREFIX),
            "outer LICM hoisted a value that depends on a nested hoist temp: {value:?}"
        );
    }
}

#[test]
fn loop_invariant_call_skips_mutated_argument_issue_5185() {
    let input = block(vec![Stmt::For {
        var: "i".to_string(),
        start: int(1),
        end: int(3),
        step: None,
        body: block(vec![
            Stmt::Expr {
                expr: call("push!", vec![var("xs"), var("i")]),
                span: sp(),
            },
            Stmt::Assign {
                var: "n".to_string(),
                value: length_call("xs"),
                span: sp(),
            },
        ]),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    assert_eq!(output.stmts.len(), 1);
    assert!(matches!(&output.stmts[0], Stmt::For { .. }));
}

#[test]
fn loop_invariant_hoist_skips_loop_var_and_mutated_inputs_issue_5185() {
    let input = block(vec![Stmt::For {
        var: "i".to_string(),
        start: int(1),
        end: int(3),
        step: None,
        body: block(vec![
            Stmt::Assign {
                var: "a".to_string(),
                value: add(var("i"), int(1)),
                span: sp(),
            },
            Stmt::Assign {
                var: "limit".to_string(),
                value: add(var("limit"), int(1)),
                span: sp(),
            },
            Stmt::Assign {
                var: "b".to_string(),
                value: add(var("limit"), int(2)),
                span: sp(),
            },
        ]),
        span: sp(),
    }]);

    let output = optimizer().optimize_block(&input);

    assert_eq!(output.stmts.len(), 1);
    assert!(matches!(&output.stmts[0], Stmt::For { .. }));
}
