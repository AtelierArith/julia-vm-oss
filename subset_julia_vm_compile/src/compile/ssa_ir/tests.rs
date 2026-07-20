//! Unit tests for the durable SSA model and Core IR → SSA conversion
//! (Issue #8550).
//!
//! Tests build Core IR snippets via the shared [`super::test_util`] builders
//! and assert on block structure, Phi placement, and verifier cleanliness.
//! Optimization pass tests (Issue #8551) live in `opt.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::test_util::{
    assign, binop, block, build, first_phi, func_with, if_stmt, phi_count, ret, while_stmt,
};
use super::*;
use crate::compile::test_helpers::{call_expr, int_lit, var_expr, zero_span};
use crate::ir::core::{BinaryOp, Expr, Literal, Stmt};

// ---------------------------------------------------------------------------
// Straight-line code
// ---------------------------------------------------------------------------

#[test]
fn ssa_straight_line_constant_binding_emits_no_statements() {
    // x = 1; y = x; return y — constants flow through bindings as operands,
    // so no SSA statements are needed at all.
    let func = func_with(
        &[],
        vec![
            assign("x", int_lit(1)),
            assign("y", var_expr("x")),
            ret(var_expr("y")),
        ],
    );
    let ssa = build(&func);
    assert_eq!(ssa.blocks.len(), 1);
    let entry = &ssa.blocks[ssa.entry.0 as usize];
    assert_eq!(entry.stmts.len(), 0);
    assert_eq!(
        entry.terminator,
        Terminator::Return {
            value: Some(SsaValue::Const(Literal::Int(1)))
        }
    );
}

#[test]
fn ssa_straight_line_call_chain_defs_in_order() {
    // x = f(1); y = g(x); return y
    let func = func_with(
        &[],
        vec![
            assign("x", call_expr("f", vec![int_lit(1)])),
            assign("y", call_expr("g", vec![var_expr("x")])),
            ret(var_expr("y")),
        ],
    );
    let ssa = build(&func);
    assert_eq!(ssa.blocks.len(), 1);
    let entry = &ssa.blocks[0];
    assert_eq!(entry.stmts.len(), 2);
    let first_id = entry.stmts[0].id;
    assert!(matches!(
        &entry.stmts[1].op,
        SsaOp::Call { function, args, .. }
            if function == "g" && args == &[SsaValue::Def(first_id)]
    ));
    assert_eq!(
        entry.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(entry.stmts[1].id))
        }
    );
}

#[test]
fn ssa_variable_reassignment_uses_latest_def() {
    // x = f(); x = g(); return x — both calls kept (side effects), return
    // uses the second definition.
    let func = func_with(
        &[],
        vec![
            assign("x", call_expr("f", vec![])),
            assign("x", call_expr("g", vec![])),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let entry = &ssa.blocks[0];
    assert_eq!(entry.stmts.len(), 2);
    assert_eq!(
        entry.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(entry.stmts[1].id))
        }
    );
}

#[test]
fn ssa_add_assign_desugars_to_binary_add() {
    // x = f(); x += 2; return x
    let func = func_with(
        &[],
        vec![
            assign("x", call_expr("f", vec![])),
            Stmt::AddAssign {
                var: "x".to_string(),
                value: int_lit(2),
                span: zero_span(),
            },
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let entry = &ssa.blocks[0];
    assert_eq!(entry.stmts.len(), 2);
    let call_id = entry.stmts[0].id;
    assert!(matches!(
        &entry.stmts[1].op,
        SsaOp::Binary { op: BinaryOp::Add, left, right }
            if left == &SsaValue::Def(call_id) && right == &SsaValue::Const(Literal::Int(2))
    ));
}

#[test]
fn ssa_params_bind_as_arguments() {
    // return a + b
    let func = func_with(
        &["a", "b"],
        vec![ret(binop(BinaryOp::Add, var_expr("a"), var_expr("b")))],
    );
    let ssa = build(&func);
    assert_eq!(ssa.params.len(), 2);
    let entry = &ssa.blocks[0];
    assert!(matches!(
        &entry.stmts[0].op,
        SsaOp::Binary { op: BinaryOp::Add, left, right }
            if left == &SsaValue::Argument(0) && right == &SsaValue::Argument(1)
    ));
}

#[test]
fn ssa_unbound_var_reads_global() {
    // return x — with no local binding, `x` resolves to a global load.
    let func = func_with(&[], vec![ret(var_expr("x"))]);
    let ssa = build(&func);
    let entry = &ssa.blocks[0];
    assert_eq!(entry.stmts.len(), 1);
    assert!(matches!(
        &entry.stmts[0].op,
        SsaOp::LoadGlobal { name } if name == "x"
    ));
    assert_eq!(
        entry.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(entry.stmts[0].id))
        }
    );
}

#[test]
fn ssa_global_declared_write_routes_to_store_global() {
    // global x; x = f(); return x — the write becomes a StoreGlobal and the
    // read after it goes back through a LoadGlobal (globals are not SSA
    // numbered in this slice).
    let func = func_with(
        &[],
        vec![
            Stmt::Global {
                names: vec!["x".to_string()],
                span: zero_span(),
            },
            assign("x", call_expr("f", vec![])),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let entry = &ssa.blocks[0];
    assert_eq!(entry.stmts.len(), 3);
    let call_id = entry.stmts[0].id;
    assert!(matches!(
        &entry.stmts[1].op,
        SsaOp::StoreGlobal { name, value }
            if name == "x" && value == &SsaValue::Def(call_id)
    ));
    assert!(matches!(
        &entry.stmts[2].op,
        SsaOp::LoadGlobal { name } if name == "x"
    ));
}

// ---------------------------------------------------------------------------
// if / else
// ---------------------------------------------------------------------------

#[test]
fn ssa_if_else_join_gets_exactly_one_phi() {
    // x assigned in both arms yields exactly one Phi at the join block.
    let func = func_with(
        &["c"],
        vec![
            assign("x", int_lit(0)),
            if_stmt(
                var_expr("c"),
                vec![assign("x", call_expr("f", vec![int_lit(1)]))],
                Some(vec![assign("x", call_expr("g", vec![int_lit(2)]))]),
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    assert_eq!(ssa.blocks.len(), 4); // entry, then, else, join

    let entry = &ssa.blocks[0];
    assert!(matches!(
        entry.terminator,
        Terminator::Branch {
            condition: SsaValue::Argument(0),
            then_target: BlockId(1),
            else_target: BlockId(2),
        }
    ));

    let join = &ssa.blocks[3];
    assert_eq!(join.preds, vec![BlockId(1), BlockId(2)]);
    assert_eq!(phi_count(join), 1);
    let (phi_stmt, phi) = first_phi(join);
    assert_eq!(phi.edges, vec![BlockId(1), BlockId(2)]);
    let then_def = ssa.blocks[1].stmts[0].id;
    let else_def = ssa.blocks[2].stmts[0].id;
    assert_eq!(
        phi.values,
        vec![Some(SsaValue::Def(then_def)), Some(SsaValue::Def(else_def))]
    );
    assert_eq!(
        join.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(phi_stmt.id))
        }
    );
}

#[test]
fn shared_plan_exposes_phi_edge_copies_for_backends_9089() {
    use super::plan::{plan_function, NumericConvertGate, SharedRootPlan, SharedTermPlan};

    let func = func_with(
        &["c"],
        vec![
            assign("x", int_lit(0)),
            if_stmt(
                var_expr("c"),
                vec![assign("x", call_expr("f", vec![int_lit(1)]))],
                Some(vec![assign("x", call_expr("g", vec![int_lit(2)]))]),
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let plan = plan_function(&ssa, func.span, NumericConvertGate::default()).expect("shared plan");

    assert!(plan
        .blocks()
        .iter()
        .any(|block| matches!(block.terminator(), SharedTermPlan::Branch { .. })));
    assert!(plan.blocks().iter().any(|block| {
        block.roots().iter().any(
            |root| matches!(root, SharedRootPlan::Assign { name, .. } if name.starts_with("#ssa")),
        )
    }));
}

#[test]
fn shared_plan_numeric_convert_rewrite_requires_open_gate_9803() {
    use super::plan::{plan_function, NumericConvertGate, SharedTermPlan};
    use crate::ir::core::NumericConvertTarget;

    // return Float64(x) — the rewrite decision must come from the gate, which
    // lower.rs computes from the SAME method-table evidence the stack
    // compiler's `compile_generic_dispatch_call` uses. A program defining a
    // user method named `Float64` (e.g. `Float64(::MyIrrational{:tau})`,
    // dispatch fixture `dispatch/symbol_type_param_dispatch.jl`, Issue #633)
    // closes the gate: the call must stay a plain `Expr::Call` whose stack
    // lowering performs full user-method dispatch. Only a proven-builtin
    // resolution (open gate) may produce the structural `Expr::Convert`.
    let func = func_with(&["x"], vec![ret(call_expr("Float64", vec![var_expr("x")]))]);
    let ssa = build(&func);

    let term_expr = |gate: NumericConvertGate| {
        let plan = plan_function(&ssa, func.span, gate).expect("shared plan");
        match plan.blocks()[0].terminator() {
            SharedTermPlan::Return { expr: Some(expr) } => expr.clone(),
            other => panic!("expected return terminator, got {other:?}"),
        }
    };

    // Gate closed (default: nothing proven) — the call is preserved verbatim.
    let closed = term_expr(NumericConvertGate::default());
    assert!(
        matches!(&closed, Expr::Call { function, .. } if function == "Float64"),
        "closed gate must keep the user-dispatchable call, got {closed:?}"
    );

    // Gate open for Float64 — the structural conversion node is produced.
    let open = term_expr(NumericConvertGate {
        int64: false,
        float64: true,
    });
    assert!(
        matches!(
            &open,
            Expr::Convert {
                target: NumericConvertTarget::Float64,
                ..
            }
        ),
        "open gate must rewrite to Expr::Convert, got {open:?}"
    );

    // The Int64 half of the gate must not affect Float64 calls (and vice
    // versa): opening only `int64` keeps this Float64 call un-rewritten.
    let wrong_half = term_expr(NumericConvertGate {
        int64: true,
        float64: false,
    });
    assert!(
        matches!(&wrong_half, Expr::Call { function, .. } if function == "Float64"),
        "per-target gate must not leak across targets, got {wrong_half:?}"
    );
}

#[test]
fn ssa_if_without_else_merges_old_binding() {
    // x = f(); if c; x = g(); end; return x — phi joins the then-arm def with
    // the pre-branch def flowing along the fallthrough edge.
    let func = func_with(
        &["c"],
        vec![
            assign("x", call_expr("f", vec![])),
            if_stmt(
                var_expr("c"),
                vec![assign("x", call_expr("g", vec![]))],
                None,
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    assert_eq!(ssa.blocks.len(), 3); // entry, then, join
    let join = &ssa.blocks[2];
    assert_eq!(join.preds, vec![BlockId(0), BlockId(1)]);
    assert_eq!(phi_count(join), 1);
    let (_, phi) = first_phi(join);
    let old_def = ssa.blocks[0].stmts[0].id;
    let then_def = ssa.blocks[1].stmts[0].id;
    assert_eq!(phi.edges, vec![BlockId(0), BlockId(1)]);
    assert_eq!(
        phi.values,
        vec![Some(SsaValue::Def(old_def)), Some(SsaValue::Def(then_def))]
    );
}

#[test]
fn ssa_if_same_constant_both_arms_needs_no_phi() {
    // The bridge's literal Phi fold falls out of SSA construction naturally:
    // identical incoming values merge without a Phi node.
    let func = func_with(
        &["c"],
        vec![
            if_stmt(
                var_expr("c"),
                vec![assign("x", int_lit(41))],
                Some(vec![assign("x", int_lit(41))]),
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let join = &ssa.blocks[3];
    assert_eq!(phi_count(join), 0);
    assert_eq!(
        join.terminator,
        Terminator::Return {
            value: Some(SsaValue::Const(Literal::Int(41)))
        }
    );
}

#[test]
fn ssa_if_var_assigned_only_in_then_gets_undef_edge() {
    // `x` is unbound before the branch and assigned only in the then arm, so
    // the join phi carries a None (maybe-undef) entry on the fallthrough edge,
    // mirroring upstream #undef PhiNode entries.
    let func = func_with(
        &["c"],
        vec![
            if_stmt(
                var_expr("c"),
                vec![assign("x", call_expr("f", vec![]))],
                None,
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let join = &ssa.blocks[2];
    assert_eq!(phi_count(join), 1);
    let (_, phi) = first_phi(join);
    let then_def = ssa.blocks[1].stmts[0].id;
    assert_eq!(phi.edges, vec![BlockId(0), BlockId(1)]);
    assert_eq!(phi.values, vec![None, Some(SsaValue::Def(then_def))]);
}

#[test]
fn ssa_nested_if_places_phi_at_each_join() {
    // if a; if b; x = f() else x = g() end else x = h() end; return x
    let func = func_with(
        &["a", "b"],
        vec![
            if_stmt(
                var_expr("a"),
                vec![if_stmt(
                    var_expr("b"),
                    vec![assign("x", call_expr("f", vec![]))],
                    Some(vec![assign("x", call_expr("g", vec![]))]),
                )],
                Some(vec![assign("x", call_expr("h", vec![]))]),
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    // entry(0), outer-then(1), inner-then(2), inner-else(3), inner-join(4),
    // outer-else(5), outer-join(6)
    assert_eq!(ssa.blocks.len(), 7);
    let inner_join = &ssa.blocks[4];
    assert_eq!(phi_count(inner_join), 1);
    let (inner_phi_stmt, _) = first_phi(inner_join);

    let outer_join = &ssa.blocks[6];
    assert_eq!(phi_count(outer_join), 1);
    let (_, outer_phi) = first_phi(outer_join);
    let else_def = ssa.blocks[5].stmts[0].id;
    assert_eq!(outer_phi.edges, vec![BlockId(4), BlockId(5)]);
    assert_eq!(
        outer_phi.values,
        vec![
            Some(SsaValue::Def(inner_phi_stmt.id)),
            Some(SsaValue::Def(else_def))
        ]
    );
}

#[test]
fn ssa_return_in_both_arms_leaves_unreachable_join() {
    // if c; return f() else return g() end; h() — the join block is
    // unreachable but still well-formed; the verifier skips dominance checks
    // for unreachable blocks.
    let func = func_with(
        &["c"],
        vec![
            if_stmt(
                var_expr("c"),
                vec![ret(call_expr("f", vec![]))],
                Some(vec![ret(call_expr("g", vec![]))]),
            ),
            Stmt::Expr {
                expr: call_expr("h", vec![]),
                span: zero_span(),
            },
        ],
    );
    let ssa = build(&func);
    let join = &ssa.blocks[3];
    assert!(join.preds.is_empty());
    assert!(matches!(
        ssa.blocks[1].terminator,
        Terminator::Return { value: Some(_) }
    ));
    assert!(matches!(
        ssa.blocks[2].terminator,
        Terminator::Return { value: Some(_) }
    ));
}

// ---------------------------------------------------------------------------
// while
// ---------------------------------------------------------------------------

#[test]
fn ssa_while_loop_header_phi_for_reassigned_var() {
    // i = 0; while i < n; i = i + 1; end; return i
    let func = func_with(
        &["n"],
        vec![
            assign("i", int_lit(0)),
            while_stmt(
                binop(BinaryOp::Lt, var_expr("i"), var_expr("n")),
                vec![assign("i", binop(BinaryOp::Add, var_expr("i"), int_lit(1)))],
            ),
            ret(var_expr("i")),
        ],
    );
    let ssa = build(&func);
    // entry(0), header(1), body(2), exit(3)
    assert_eq!(ssa.blocks.len(), 4);

    let entry = &ssa.blocks[0];
    assert_eq!(entry.terminator, Terminator::Jump { target: BlockId(1) });

    let header = &ssa.blocks[1];
    assert_eq!(phi_count(header), 1);
    let (phi_stmt, phi) = first_phi(header);
    let body = &ssa.blocks[2];
    let add_id = body.stmts[0].id;
    assert_eq!(phi.edges, vec![BlockId(0), BlockId(2)]);
    assert_eq!(
        phi.values,
        vec![
            Some(SsaValue::Const(Literal::Int(0))),
            Some(SsaValue::Def(add_id))
        ]
    );
    // The condition compares the phi against the argument.
    assert!(matches!(
        &header.stmts[1].op,
        SsaOp::Binary { op: BinaryOp::Lt, left, right }
            if left == &SsaValue::Def(phi_stmt.id) && right == &SsaValue::Argument(0)
    ));
    assert!(matches!(
        header.terminator,
        Terminator::Branch {
            then_target: BlockId(2),
            else_target: BlockId(3),
            ..
        }
    ));
    // The body increment reads the phi, and the loop exit returns it.
    assert!(matches!(
        &body.stmts[0].op,
        SsaOp::Binary { op: BinaryOp::Add, left, .. }
            if left == &SsaValue::Def(phi_stmt.id)
    ));
    assert_eq!(body.terminator, Terminator::Jump { target: BlockId(1) });
    let exit = &ssa.blocks[3];
    assert_eq!(phi_count(exit), 0);
    assert_eq!(
        exit.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(phi_stmt.id))
        }
    );
}

#[test]
fn ssa_while_header_phi_only_for_assigned_vars() {
    // A variable that is only read inside the loop keeps its dominating def
    // and gets no header phi; only assigned variables are phi'd (the loop
    // pre-scan is not pruned, so a body-local temp still gets a — dead — phi).
    let func = func_with(
        &["c", "x"],
        vec![
            while_stmt(
                var_expr("c"),
                vec![assign("y", call_expr("f", vec![var_expr("x")]))],
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    let header = &ssa.blocks[1];
    assert_eq!(phi_count(header), 1); // only `y`
    let (_, phi) = first_phi(header);
    assert_eq!(phi.values[0], None); // `y` is undef on the preheader edge
}

#[test]
fn ssa_while_break_adds_exit_phi() {
    // i = f(); while c(i); i = g(i); if d(i); break; end; end; return i
    let func = func_with(
        &[],
        vec![
            assign("i", call_expr("f", vec![])),
            while_stmt(
                call_expr("c", vec![var_expr("i")]),
                vec![
                    assign("i", call_expr("g", vec![var_expr("i")])),
                    if_stmt(
                        call_expr("d", vec![var_expr("i")]),
                        vec![Stmt::Break { span: zero_span() }],
                        None,
                    ),
                ],
            ),
            ret(var_expr("i")),
        ],
    );
    let ssa = build(&func);
    let header = &ssa.blocks[1];
    assert_eq!(phi_count(header), 1);
    let (header_phi_stmt, _) = first_phi(header);

    let exit = ssa
        .blocks
        .iter()
        .find(|b| matches!(b.terminator, Terminator::Return { .. }) && !b.preds.is_empty())
        .expect("reachable exit block");
    assert_eq!(phi_count(exit), 1);
    let (exit_phi_stmt, exit_phi) = first_phi(exit);
    assert_eq!(exit_phi.edges.len(), 2);
    // One incoming value is the header phi (normal exit), the other is the
    // body redefinition (break exit).
    assert!(exit_phi
        .values
        .contains(&Some(SsaValue::Def(header_phi_stmt.id))));
    assert_eq!(
        exit.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(exit_phi_stmt.id))
        }
    );
}

#[test]
fn ssa_break_outside_loop_is_rejected() {
    let func = func_with(&[], vec![Stmt::Break { span: zero_span() }]);
    assert!(matches!(
        build_function(&func),
        Err(SsaBuildError::LoopControlOutsideLoop { .. })
    ));
}

// ---------------------------------------------------------------------------
// Ternary (structured expression-position control flow)
// ---------------------------------------------------------------------------

#[test]
fn ssa_ternary_produces_phi() {
    // x = c ? f() : g(); return x
    let func = func_with(
        &["c"],
        vec![
            assign(
                "x",
                Expr::Ternary {
                    condition: Box::new(var_expr("c")),
                    then_expr: Box::new(call_expr("f", vec![])),
                    else_expr: Box::new(call_expr("g", vec![])),
                    span: zero_span(),
                },
            ),
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    assert_eq!(ssa.blocks.len(), 4);
    let join = &ssa.blocks[3];
    assert_eq!(phi_count(join), 1);
    let (phi_stmt, _) = first_phi(join);
    assert_eq!(
        join.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(phi_stmt.id))
        }
    );
}

// ---------------------------------------------------------------------------
// Opaque statements and the try/catch barrier
// ---------------------------------------------------------------------------

#[test]
fn ssa_try_catch_is_opaque_barrier() {
    // x = f(); try; x = g(x); catch; end; return x — the try/catch region is
    // one opaque statement that reads the live value of `x` and rebinds it
    // via a barrier reload afterwards.
    let func = func_with(
        &[],
        vec![
            assign("x", call_expr("f", vec![])),
            Stmt::Try {
                try_block: block(vec![assign("x", call_expr("g", vec![var_expr("x")]))]),
                catch_var: None,
                catch_block: Some(block(vec![])),
                else_block: None,
                finally_block: None,
                span: zero_span(),
            },
            ret(var_expr("x")),
        ],
    );
    let ssa = build(&func);
    assert_eq!(ssa.blocks.len(), 1);
    let entry = &ssa.blocks[0];
    assert_eq!(entry.stmts.len(), 3);
    let f_id = entry.stmts[0].id;
    let barrier_id = entry.stmts[1].id;
    assert!(matches!(
        &entry.stmts[1].op,
        SsaOp::OpaqueStmt { reads, .. }
            if reads == &[("x".to_string(), SsaValue::Def(f_id))]
    ));
    assert!(matches!(
        &entry.stmts[2].op,
        SsaOp::BarrierReload { var, barrier }
            if var == "x" && barrier == &SsaValue::Def(barrier_id)
    ));
    assert_eq!(
        entry.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(entry.stmts[2].id))
        }
    );
}

#[test]
fn ssa_opaque_expr_records_reads_and_embedded_writes() {
    // y = f(); z = [y, (w = g())]; return w — the array literal is opaque:
    // it reads `y` and its embedded assignment rebinds `w` via barrier reload.
    let func = func_with(
        &[],
        vec![
            assign("y", call_expr("f", vec![])),
            assign(
                "z",
                Expr::ArrayLiteral {
                    elements: vec![
                        var_expr("y"),
                        Expr::AssignExpr {
                            var: "w".to_string().into(),
                            value: Box::new(call_expr("g", vec![])),
                            span: zero_span(),
                        },
                    ],
                    shape: vec![2],
                    span: zero_span(),
                },
            ),
            ret(var_expr("w")),
        ],
    );
    let ssa = build(&func);
    let entry = &ssa.blocks[0];
    let f_id = entry.stmts[0].id;
    let opaque = entry
        .stmts
        .iter()
        .find(|s| matches!(s.op, SsaOp::Opaque { .. }))
        .expect("array literal should lower to an opaque op");
    assert!(matches!(
        &opaque.op,
        SsaOp::Opaque { reads, .. }
            if reads == &[("y".to_string(), SsaValue::Def(f_id))]
    ));
    let reload = entry
        .stmts
        .iter()
        .find(|s| matches!(&s.op, SsaOp::BarrierReload { var, .. } if var == "w"))
        .expect("embedded write should produce a barrier reload");
    assert_eq!(
        entry.terminator,
        Terminator::Return {
            value: Some(SsaValue::Def(reload.id))
        }
    );
}

#[test]
fn ssa_goto_is_rejected() {
    let func = func_with(
        &[],
        vec![Stmt::Goto {
            name: "somewhere".to_string(),
            span: zero_span(),
        }],
    );
    assert!(matches!(
        build_function(&func),
        Err(SsaBuildError::UnstructuredControlFlow { .. })
    ));
}

// ---------------------------------------------------------------------------
// Verifier rejections (hand-built invalid SSA)
// ---------------------------------------------------------------------------

fn load_global(id: u32, name: &str) -> SsaStatement {
    SsaStatement {
        id: SsaValueId(id),
        op: SsaOp::LoadGlobal {
            name: name.to_string(),
        },
        span: zero_span(),
    }
}

fn unary_use(id: u32, operand: SsaValue) -> SsaStatement {
    SsaStatement {
        id: SsaValueId(id),
        op: SsaOp::Unary {
            op: crate::ir::core::UnaryOp::Neg,
            operand,
        },
        span: zero_span(),
    }
}

#[test]
fn ssa_verifier_rejects_use_of_unknown_def() {
    let func = SsaFunction {
        name: "bad".to_string(),
        params: vec![],
        entry: BlockId(0),
        blocks: vec![SsaBlock {
            id: BlockId(0),
            stmts: vec![unary_use(0, SsaValue::Def(SsaValueId(99)))],
            terminator: Terminator::Return { value: None },
            preds: vec![],
            succs: vec![],
        }],
    };
    let err = verify(&func).expect_err("unknown def must be rejected");
    assert!(err.contains("unknown def"), "unexpected message: {err}");
}

#[test]
fn ssa_verifier_rejects_use_not_dominated_by_def() {
    // entry branches to b1 (defines v0) and b2 (uses v0): b1 does not
    // dominate b2.
    let func = SsaFunction {
        name: "bad".to_string(),
        params: vec![],
        entry: BlockId(0),
        blocks: vec![
            SsaBlock {
                id: BlockId(0),
                stmts: vec![],
                terminator: Terminator::Branch {
                    condition: SsaValue::Const(Literal::Bool(true)),
                    then_target: BlockId(1),
                    else_target: BlockId(2),
                },
                preds: vec![],
                succs: vec![BlockId(1), BlockId(2)],
            },
            SsaBlock {
                id: BlockId(1),
                stmts: vec![load_global(0, "g")],
                terminator: Terminator::Return { value: None },
                preds: vec![BlockId(0)],
                succs: vec![],
            },
            SsaBlock {
                id: BlockId(2),
                stmts: vec![unary_use(1, SsaValue::Def(SsaValueId(0)))],
                terminator: Terminator::Return { value: None },
                preds: vec![BlockId(0)],
                succs: vec![],
            },
        ],
    };
    let err = verify(&func).expect_err("non-dominating def must be rejected");
    assert!(err.contains("dominate"), "unexpected message: {err}");
}

#[test]
fn ssa_verifier_rejects_phi_arity_mismatch() {
    // Join block has two predecessors but the phi lists only one edge.
    let func = SsaFunction {
        name: "bad".to_string(),
        params: vec![],
        entry: BlockId(0),
        blocks: vec![
            SsaBlock {
                id: BlockId(0),
                stmts: vec![],
                terminator: Terminator::Branch {
                    condition: SsaValue::Const(Literal::Bool(true)),
                    then_target: BlockId(1),
                    else_target: BlockId(2),
                },
                preds: vec![],
                succs: vec![BlockId(1), BlockId(2)],
            },
            SsaBlock {
                id: BlockId(1),
                stmts: vec![],
                terminator: Terminator::Jump { target: BlockId(3) },
                preds: vec![BlockId(0)],
                succs: vec![BlockId(3)],
            },
            SsaBlock {
                id: BlockId(2),
                stmts: vec![],
                terminator: Terminator::Jump { target: BlockId(3) },
                preds: vec![BlockId(0)],
                succs: vec![BlockId(3)],
            },
            SsaBlock {
                id: BlockId(3),
                stmts: vec![SsaStatement {
                    id: SsaValueId(0),
                    op: SsaOp::Phi(PhiNode {
                        edges: vec![BlockId(1)],
                        values: vec![Some(SsaValue::Const(Literal::Int(1)))],
                    }),
                    span: zero_span(),
                }],
                terminator: Terminator::Return { value: None },
                preds: vec![BlockId(1), BlockId(2)],
                succs: vec![],
            },
        ],
    };
    let err = verify(&func).expect_err("phi arity mismatch must be rejected");
    assert!(err.contains("phi"), "unexpected message: {err}");
}

#[test]
fn ssa_verifier_rejects_terminator_edge_mismatch() {
    let func = SsaFunction {
        name: "bad".to_string(),
        params: vec![],
        entry: BlockId(0),
        blocks: vec![
            SsaBlock {
                id: BlockId(0),
                stmts: vec![],
                terminator: Terminator::Jump { target: BlockId(1) },
                preds: vec![],
                succs: vec![], // inconsistent: terminator says [1]
            },
            SsaBlock {
                id: BlockId(1),
                stmts: vec![],
                terminator: Terminator::Return { value: None },
                preds: vec![BlockId(0)],
                succs: vec![],
            },
        ],
    };
    let err = verify(&func).expect_err("edge mismatch must be rejected");
    assert!(err.contains("successor"), "unexpected message: {err}");
}

#[test]
fn ssa_verifier_rejects_phi_after_non_phi() {
    let func = SsaFunction {
        name: "bad".to_string(),
        params: vec![],
        entry: BlockId(0),
        blocks: vec![SsaBlock {
            id: BlockId(0),
            stmts: vec![
                load_global(0, "g"),
                SsaStatement {
                    id: SsaValueId(1),
                    op: SsaOp::Phi(PhiNode {
                        edges: vec![],
                        values: vec![],
                    }),
                    span: zero_span(),
                },
            ],
            terminator: Terminator::Return { value: None },
            preds: vec![],
            succs: vec![],
        }],
    };
    let err = verify(&func).expect_err("phi after non-phi must be rejected");
    assert!(err.contains("phi"), "unexpected message: {err}");
}
