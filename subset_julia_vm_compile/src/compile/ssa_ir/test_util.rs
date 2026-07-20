//! Shared Core IR builders for SSA unit tests (Issues #8550, #8551).
//!
//! Tests build Core IR snippets directly (TESTING_GUIDE.md IR-literal
//! conventions: `Literal::Int(i64)`, `zero_span()`, `call_expr` helper) and
//! convert them with [`build`], which asserts verifier cleanliness.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::compile::test_helpers::zero_span;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Stmt, TypedParam};

use super::model::{PhiNode, SsaBlock, SsaFunction, SsaOp, SsaStatement};
use super::{build_function, verify};

pub(super) fn block(stmts: Vec<Stmt>) -> Block {
    Block {
        stmts,
        span: zero_span(),
    }
}

pub(super) fn func_with(params: &[&str], stmts: Vec<Stmt>) -> Function {
    Function {
        name: "ssa_test_fn".to_string(),
        params: params
            .iter()
            .map(|p| TypedParam::untyped((*p).to_string(), zero_span()))
            .collect(),
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: block(stmts),
        is_base_extension: false,
        is_runtime_eval: false,
        span: zero_span(),
        new_struct_name: None,
    }
}

pub(super) fn assign(var: &str, value: Expr) -> Stmt {
    Stmt::Assign {
        var: var.to_string(),
        value,
        span: zero_span(),
    }
}

pub(super) fn ret(value: Expr) -> Stmt {
    Stmt::Return {
        value: Some(value),
        span: zero_span(),
    }
}

pub(super) fn if_stmt(
    condition: Expr,
    then_stmts: Vec<Stmt>,
    else_stmts: Option<Vec<Stmt>>,
) -> Stmt {
    Stmt::If {
        condition,
        then_branch: block(then_stmts),
        else_branch: else_stmts.map(block),
        span: zero_span(),
    }
}

pub(super) fn while_stmt(condition: Expr, body: Vec<Stmt>) -> Stmt {
    Stmt::While {
        condition,
        body: block(body),
        span: zero_span(),
    }
}

pub(super) fn binop(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span: zero_span(),
    }
}

pub(super) fn build(func: &Function) -> SsaFunction {
    let ssa = build_function(func).expect("conversion should succeed");
    assert_eq!(verify(&ssa), Ok(()), "constructed SSA must verify");
    ssa
}

pub(super) fn phi_count(block: &SsaBlock) -> usize {
    block.stmts.iter().filter(|s| s.op.is_phi()).count()
}

pub(super) fn first_phi(block: &SsaBlock) -> (&SsaStatement, &PhiNode) {
    let stmt = block
        .stmts
        .iter()
        .find(|s| s.op.is_phi())
        .expect("block should contain a phi");
    let SsaOp::Phi(phi) = &stmt.op else {
        unreachable!() // OK: panic! — guarded by the is_phi filter above
    };
    (stmt, phi)
}
