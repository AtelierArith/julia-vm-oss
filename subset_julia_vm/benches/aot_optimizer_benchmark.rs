//! AoT optimizer pass benchmarks (Issue #6945).
//!
//! Run with:
//!   cargo bench --features aot --bench aot_optimizer_benchmark

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::aot::ir::{
    AotBinOp, AotExpr, AotFunction, AotInlinePolicy, AotProgram, AotStmt,
};
use subset_julia_vm::aot::optimizer::{
    optimize_aot_program_with_constant_folding, optimize_aot_program_with_cse,
    optimize_aot_program_with_dce, optimize_aot_program_with_inlining,
    optimize_aot_program_with_loops, optimize_aot_program_with_strength_reduction,
    optimize_aot_program_with_tail_recursion,
};
use subset_julia_vm::aot::types::StaticType;

const STATEMENT_COUNT: usize = 256;

fn var(name: &str, ty: StaticType) -> AotExpr {
    AotExpr::Var {
        name: name.to_string(),
        ty,
    }
}

fn add_expr(left: AotExpr, right: AotExpr) -> AotExpr {
    AotExpr::BinOpStatic {
        op: AotBinOp::Add,
        left: Box::new(left),
        right: Box::new(right),
        result_ty: StaticType::I64,
    }
}

fn mul_expr(left: AotExpr, right: AotExpr) -> AotExpr {
    AotExpr::BinOpStatic {
        op: AotBinOp::Mul,
        left: Box::new(left),
        right: Box::new(right),
        result_ty: StaticType::I64,
    }
}

fn constant_folding_program() -> AotProgram {
    let mut program = AotProgram::new();
    for i in 0..STATEMENT_COUNT {
        program.main.push(AotStmt::Let {
            name: format!("cf_{i}"),
            ty: StaticType::I64,
            value: add_expr(
                AotExpr::LitI64(i as i64),
                mul_expr(AotExpr::LitI64(2), AotExpr::LitI64(3)),
            ),
            is_mutable: false,
        });
    }
    program
}

fn strength_reduction_program() -> AotProgram {
    let mut program = AotProgram::new();
    program.main.push(AotStmt::Let {
        name: "x".to_string(),
        ty: StaticType::I64,
        value: AotExpr::LitI64(7),
        is_mutable: false,
    });
    for i in 0..STATEMENT_COUNT {
        program.main.push(AotStmt::Let {
            name: format!("sr_{i}"),
            ty: StaticType::I64,
            value: mul_expr(var("x", StaticType::I64), AotExpr::LitI64(8)),
            is_mutable: false,
        });
    }
    program
}

fn cse_program() -> AotProgram {
    let mut program = AotProgram::new();
    program.main.push(AotStmt::Let {
        name: "a".to_string(),
        ty: StaticType::I64,
        value: AotExpr::LitI64(1),
        is_mutable: false,
    });
    program.main.push(AotStmt::Let {
        name: "b".to_string(),
        ty: StaticType::I64,
        value: AotExpr::LitI64(2),
        is_mutable: false,
    });
    for i in 0..STATEMENT_COUNT {
        program.main.push(AotStmt::Let {
            name: format!("cse_{i}"),
            ty: StaticType::I64,
            value: add_expr(var("a", StaticType::I64), var("b", StaticType::I64)),
            is_mutable: false,
        });
    }
    program
}

fn dce_program() -> AotProgram {
    let mut program = AotProgram::new();
    program.main.push(AotStmt::Let {
        name: "x".to_string(),
        ty: StaticType::I64,
        value: AotExpr::LitI64(0),
        is_mutable: true,
    });
    for i in 0..STATEMENT_COUNT {
        program.main.push(AotStmt::Assign {
            target: var("x", StaticType::I64),
            value: AotExpr::LitI64(i as i64),
        });
    }
    program
        .main
        .push(AotStmt::Return(Some(var("x", StaticType::I64))));
    program
}

fn loop_optimizer_program() -> AotProgram {
    let mut program = AotProgram::new();
    for i in 0..64 {
        program.main.push(AotStmt::ForRange {
            var: format!("i_{i}"),
            start: AotExpr::LitI64(1),
            stop: AotExpr::LitI64(4),
            step: None,
            body: vec![AotStmt::Let {
                name: format!("loop_inv_{i}"),
                ty: StaticType::I64,
                value: add_expr(AotExpr::LitI64(2), AotExpr::LitI64(3)),
                is_mutable: false,
            }],
        });
    }
    program
}

fn inlining_program() -> AotProgram {
    let mut program = AotProgram::new();
    let mut inc = AotFunction::new(
        "inc".to_string(),
        vec![("x".to_string(), StaticType::I64)],
        StaticType::I64,
    );
    inc.body = vec![AotStmt::Return(Some(add_expr(
        var("x", StaticType::I64),
        AotExpr::LitI64(1),
    )))];
    program.add_function(inc);

    for i in 0..STATEMENT_COUNT {
        program.main.push(AotStmt::Let {
            name: format!("inlined_{i}"),
            ty: StaticType::I64,
            value: AotExpr::CallStatic {
                function: "inc".to_string(),
                args: vec![AotExpr::LitI64(i as i64)],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Auto,
            },
            is_mutable: false,
        });
    }
    program
}

fn tail_recursion_program() -> AotProgram {
    let mut program = AotProgram::new();
    let mut fact = AotFunction::new(
        "fact".to_string(),
        vec![
            ("n".to_string(), StaticType::I64),
            ("acc".to_string(), StaticType::I64),
        ],
        StaticType::I64,
    );
    fact.body = vec![AotStmt::If {
        condition: AotExpr::BinOpStatic {
            op: AotBinOp::Le,
            left: Box::new(var("n", StaticType::I64)),
            right: Box::new(AotExpr::LitI64(1)),
            result_ty: StaticType::Bool,
        },
        then_branch: vec![AotStmt::Return(Some(var("acc", StaticType::I64)))],
        else_branch: Some(vec![AotStmt::Return(Some(AotExpr::CallStatic {
            function: "fact".to_string(),
            args: vec![
                AotExpr::BinOpStatic {
                    op: AotBinOp::Sub,
                    left: Box::new(var("n", StaticType::I64)),
                    right: Box::new(AotExpr::LitI64(1)),
                    result_ty: StaticType::I64,
                },
                mul_expr(var("acc", StaticType::I64), var("n", StaticType::I64)),
            ],
            return_ty: StaticType::I64,
            inline_policy: AotInlinePolicy::Auto,
        }))]),
    }];
    program.add_function(fact);
    program
}

fn bench_pass<F>(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    program: AotProgram,
    mut pass: F,
) where
    F: FnMut(&mut AotProgram) -> usize,
{
    group.bench_with_input(BenchmarkId::from_parameter(name), &program, |b, program| {
        b.iter(|| {
            let mut program = black_box(program.clone());
            black_box(pass(&mut program))
        });
    });
}

fn bench_aot_optimizer_passes(c: &mut Criterion) {
    let mut group = c.benchmark_group("aot_optimizer_passes");

    bench_pass(
        &mut group,
        "constant_folding",
        constant_folding_program(),
        optimize_aot_program_with_constant_folding,
    );
    bench_pass(
        &mut group,
        "strength_reduction",
        strength_reduction_program(),
        optimize_aot_program_with_strength_reduction,
    );
    bench_pass(&mut group, "cse", cse_program(), |p| {
        optimize_aot_program_with_cse(p)
    });
    bench_pass(&mut group, "dce", dce_program(), |p| {
        optimize_aot_program_with_dce(p)
    });
    bench_pass(&mut group, "loop_opt", loop_optimizer_program(), |p| {
        optimize_aot_program_with_loops(p)
    });
    bench_pass(&mut group, "inlining", inlining_program(), |p| {
        optimize_aot_program_with_inlining(p, 10)
    });
    bench_pass(
        &mut group,
        "tail_recursion",
        tail_recursion_program(),
        optimize_aot_program_with_tail_recursion,
    );

    group.finish();
}

criterion_group!(benches, bench_aot_optimizer_passes);
criterion_main!(benches);
