//! Unit tests for constant-step integer range for-loop specialization (Issue #5166).
//!
//! When the step of an integer `for i in a:b` / `a:s:b` loop is a compile-time
//! constant, the compiler hoists the per-iteration sign check out of the loop and
//! emits a single-direction exit test plus a constant increment. These tests assert
//! that the dynamic sign-check instructions (`PushI64(0)` + `GtI64` guarding a
//! `JumpIfZero`) disappear for constant steps, while remaining for dynamic steps.

use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};

fn compile_source_with_base(source: &str) -> CompiledProgram {
    let prelude_src = base::get_base();
    let mut parser = Parser::new().expect("create parser");
    let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let mut user_program = lowering.lower(parsed).expect("lower source");

    merge_programs(prelude_program, &mut user_program);
    compile_core_program(&user_program).expect("compile failed")
}

fn merge_programs(mut prelude: Program, user: &mut Program) {
    prelude.functions.append(&mut user.functions);
    user.functions = prelude.functions;

    prelude.structs.append(&mut user.structs);
    user.structs = prelude.structs;

    prelude.abstract_types.append(&mut user.abstract_types);
    user.abstract_types = prelude.abstract_types;
}

fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
    compiled
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function '{}' not found", name))
}

fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
    &compiled.code[f.code_start..f.code_end]
}

/// The dynamic sign-check path emits the `step > 0` test as `LoadI64(step)`,
/// `PushI64(0)`, `GtI64`, `JumpIfZero(...)`. After the peephole optimizer fuses
/// `GtI64 + JumpIfZero` into `JumpIfLeI64`, the residue is `PushI64(0)` immediately
/// followed by either `GtI64` (unfused) or `JumpIfLeI64` (fused). Either way the
/// `PushI64(0)` comparand against the step is the hallmark of the per-iteration sign
/// check; constant-step loops must not contain it.
fn has_sign_check(body: &[Instr]) -> bool {
    body.windows(2).any(|w| {
        matches!(
            (&w[0], &w[1]),
            (Instr::PushI64(0), Instr::GtI64) | (Instr::PushI64(0), Instr::JumpIfLeI64(_))
        )
    })
}

fn count_inc(body: &[Instr]) -> usize {
    body.iter()
        .filter(|i| {
            matches!(i, Instr::IncVarI64(_) | Instr::IncVarI64Slot(_))
                || matches!(i, Instr::AddConstI64Slot(_, delta) if *delta > 0)
                || matches!(i, Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _))
        })
        .count()
}

fn count_dec(body: &[Instr]) -> usize {
    body.iter()
        .filter(|i| {
            matches!(i, Instr::DecVarI64(_) | Instr::DecVarI64Slot(_))
                || matches!(i, Instr::AddConstI64Slot(_, delta) if *delta < 0)
        })
        .count()
}

fn count_directional_exit(body: &[Instr]) -> usize {
    body.iter()
        .filter(|i| {
            matches!(
                i,
                Instr::JumpIfGtI64(_) | Instr::JumpIfGtI64Slots(_, _, _) | Instr::JumpIfLtI64(_)
            )
        })
        .count()
}

#[test]
fn const_step_unit_increment_drops_sign_check() {
    // `for i in 1:n` — implicit step of 1.
    let compiled = compile_source_with_base(
        "function f(n)\n  s = 0\n  for i in 1:n\n    s += i\n  end\n  s\nend\n",
    );
    let f = get_function(&compiled, "f");
    let body = function_body(&compiled, f);
    assert!(
        !has_sign_check(body),
        "unit-step loop must not emit a dynamic step>0 sign check: {:?}",
        body
    );
    assert!(
        count_inc(body) >= 1,
        "unit-step loop must use a fused I64 increment for the loop variable: {:?}",
        body
    );
    assert!(
        count_directional_exit(body) >= 1,
        "unit-step loop must use a single-direction JumpIfGtI64 exit test: {:?}",
        body
    );
}

#[test]
fn const_step_negative_unit_uses_dec_and_lt_exit() {
    // `for i in n:-1:1` — literal negative unit step.
    let compiled = compile_source_with_base(
        "function g(n)\n  s = 0\n  for i in n:-1:1\n    s += i\n  end\n  s\nend\n",
    );
    let f = get_function(&compiled, "g");
    let body = function_body(&compiled, f);
    assert!(
        !has_sign_check(body),
        "negative-unit-step loop must not emit a dynamic step>0 sign check: {:?}",
        body
    );
    assert!(
        count_dec(body) >= 1,
        "step -1 loop must use DecVarI64 for the decrement: {:?}",
        body
    );
    let has_lt_exit = body.iter().any(|i| matches!(i, Instr::JumpIfLtI64(_)));
    assert!(
        has_lt_exit,
        "step<0 loop must exit via JumpIfLtI64: {:?}",
        body
    );
}

#[test]
fn const_step_nonunit_positive_drops_sign_check() {
    // `for i in 1:2:n` — constant non-unit step.
    let compiled = compile_source_with_base(
        "function h(n)\n  s = 0\n  for i in 1:2:n\n    s += i\n  end\n  s\nend\n",
    );
    let f = get_function(&compiled, "h");
    let body = function_body(&compiled, f);
    assert!(
        !has_sign_check(body),
        "constant non-unit-step loop must not emit a dynamic step>0 sign check: {:?}",
        body
    );
    assert!(
        count_directional_exit(body) >= 1,
        "constant non-unit-step loop must use a single-direction exit test: {:?}",
        body
    );
    assert!(
        body.iter()
            .any(|i| matches!(i, Instr::AddConstI64SlotAndJumpIfLe(_, 2, _, _))),
        "constant non-unit-step loop must use a fused backedge carrying delta=2: {:?}",
        body
    );
}

#[test]
fn dynamic_step_keeps_sign_check() {
    // `for i in 1:s:n` — step is a runtime variable, so the dynamic sign-check
    // path must remain intact.
    let compiled = compile_source_with_base(
        "function k(n, s)\n  acc = 0\n  for i in 1:s:n\n    acc += i\n  end\n  acc\nend\n",
    );
    let f = get_function(&compiled, "k");
    let body = function_body(&compiled, f);
    assert!(
        has_sign_check(body),
        "dynamic-step loop must keep the per-iteration step>0 sign check: {:?}",
        body
    );
}
