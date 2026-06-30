//! Issue #6561: make the *desugared* destructuring swap `a, b = b, a % b`
//! type-stable under the lazy specialization engine.
//!
//! The lowering pass rewrites `a, b = b, a % b` (RHS references the targets)
//! into a temporary tuple plus indexed reads:
//!
//! ```text
//! __tuple_tmp_N = (b, a % b)
//! a = __tuple_tmp_N[1]
//! b = __tuple_tmp_N[2]
//! ```
//!
//! Before this fix the `__tuple_tmp_N[k]` reads returned `Any`, so the
//! specializer widened `a`/`b` off the typed fast path (`StoreAny` + dynamic
//! reload). These tests assert the swapped bindings keep their I64/F64 tags.

use std::collections::HashSet;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::specialize::specialize_function;
use subset_julia_vm::vm::{CompiledProgram, Instr, Value, ValueType, Vm};

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_with_cache(&program).expect("compile source")
}

fn specializable_ir<'a>(
    compiled: &'a CompiledProgram,
    name: &str,
) -> &'a subset_julia_vm::ir::core::Function {
    &compiled
        .specializable_functions
        .iter()
        .find(|func| func.name == name)
        .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
        .ir
}

/// Count `StoreAny(var)` instructions targeting a specific variable name.
fn store_any_count(code: &[Instr], var: &str) -> usize {
    code.iter()
        .filter(|i| matches!(i, Instr::StoreAny(name) if name == var))
        .count()
}

fn store_i64_count(code: &[Instr], var: &str) -> usize {
    code.iter()
        .filter(|i| matches!(i, Instr::StoreI64(name) if name == var))
        .count()
}

fn store_f64_count(code: &[Instr], var: &str) -> usize {
    code.iter()
        .filter(|i| matches!(i, Instr::StoreF64(name) if name == var))
        .count()
}

const GCD_SWAP_SOURCE: &str = r#"
function gcd_swap_6561(a, b)
    while b != 0
        a, b = b, a % b
    end
    return a
end
"#;

/// The integer GCD swap keeps `a`/`b` on the typed `StoreI64` path; the
/// desugared `temp[k]` reads must not widen them to `Any`. (Issue #6561)
#[test]
fn int_swap_specializes_to_typed_store_6561() {
    let compiled = compile_source(GCD_SWAP_SOURCE);
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "gcd_swap_6561"),
        &[ValueType::I64, ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize integer swap loop");

    assert_eq!(
        store_any_count(&specialized.code, "a"),
        0,
        "swap target `a` must not fall back to StoreAny: {:?}",
        specialized.code
    );
    assert_eq!(
        store_any_count(&specialized.code, "b"),
        0,
        "swap target `b` must not fall back to StoreAny: {:?}",
        specialized.code
    );
    assert!(
        store_i64_count(&specialized.code, "a") > 0,
        "swap target `a` should use typed StoreI64: {:?}",
        specialized.code
    );
    assert!(
        store_i64_count(&specialized.code, "b") > 0,
        "swap target `b` should use typed StoreI64: {:?}",
        specialized.code
    );
    // Type stability propagates to the return: `return a` stays ReturnI64
    // instead of widening to the boxed ReturnAny.
    assert_eq!(specialized.return_type, ValueType::I64);
    assert!(
        specialized
            .code
            .iter()
            .any(|i| matches!(i, Instr::ReturnI64)),
        "type-stable swap should return via ReturnI64: {:?}",
        specialized.code
    );
}

const FLOAT_SWAP_SOURCE: &str = r#"
function float_swap_6561(x, y, n)
    for _ in 1:n
        x, y = y, x + y * 0.5
    end
    return x
end
"#;

/// The same type-stability holds for a Float64 swap. (Issue #6561)
#[test]
fn float_swap_specializes_to_typed_store_6561() {
    let compiled = compile_source(FLOAT_SWAP_SOURCE);
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "float_swap_6561"),
        &[ValueType::F64, ValueType::F64, ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize float swap loop");

    assert_eq!(
        store_any_count(&specialized.code, "x") + store_any_count(&specialized.code, "y"),
        0,
        "float swap targets must not fall back to StoreAny: {:?}",
        specialized.code
    );
    assert!(
        store_f64_count(&specialized.code, "x") > 0 && store_f64_count(&specialized.code, "y") > 0,
        "float swap targets should use typed StoreF64: {:?}",
        specialized.code
    );
    assert_eq!(specialized.return_type, ValueType::F64);
}

const SWAP_ACCUMULATE_SOURCE: &str = r#"
function swap_sum_6561(a, b, n)
    s = 0
    for _ in 1:n
        a, b = b, (a + b) % 1000003
        s += a
    end
    return s
end
"#;

/// The real payoff of #6561: when a swapped target is *used downstream*, its
/// type stability keeps the consuming op typed. Here `s += a` after the swap
/// stays on `AddI64`/`StoreI64` instead of (pre-fix) widening `a` to `Any`,
/// forcing `s += a` onto a dynamic `DynamicAdd`, and poisoning the accumulator
/// `s` to `Any`. The whole specialized loop must therefore be free of dynamic
/// arithmetic and return the typed accumulator. (Issue #6561)
#[test]
fn swap_target_used_downstream_stays_typed_6561() {
    let compiled = compile_source(SWAP_ACCUMULATE_SOURCE);
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "swap_sum_6561"),
        &[ValueType::I64, ValueType::I64, ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize swap-accumulate loop");

    // No dynamic arithmetic anywhere in the hot loop.
    let dynamic_ops = specialized
        .code
        .iter()
        .filter(|i| {
            matches!(
                i,
                Instr::DynamicAdd
                    | Instr::DynamicSub
                    | Instr::DynamicMul
                    | Instr::DynamicDiv
                    | Instr::DynamicMod
            )
        })
        .count();
    assert_eq!(
        dynamic_ops, 0,
        "downstream use of the typed swap target must not emit dynamic arithmetic: {:?}",
        specialized.code
    );
    // The accumulator `s` stays typed (no boxed StoreAny) and the function
    // returns the typed accumulator.
    assert_eq!(
        store_any_count(&specialized.code, "s"),
        0,
        "accumulator `s` must not widen to StoreAny: {:?}",
        specialized.code
    );
    assert!(
        store_i64_count(&specialized.code, "s") > 0,
        "accumulator `s` should use typed StoreI64: {:?}",
        specialized.code
    );
    assert_eq!(specialized.return_type, ValueType::I64);
}

/// A swap whose tuple mixes types keeps *each* target on its own typed path.
///
/// Originally (#6561, tuple-element tracking) this case sharpened only the
/// numeric target and left the non-numeric one on `Any`. After #6569 the swap
/// no longer goes through a tuple at all — it lowers to per-element temps — so
/// every target keeps its concrete type: the numeric `a` is `StoreI64` and the
/// string `s` is `StoreStr`, with no `StoreAny` widening.
#[test]
fn mixed_swap_keeps_each_target_typed_6561() {
    let compiled = compile_source(
        r#"
function mixed_swap_6561(a, s)
    a, s = a + 1, s
    return a
end
    "#,
    );
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "mixed_swap_6561"),
        &[ValueType::I64, ValueType::Str],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize mixed swap");

    assert!(
        store_i64_count(&specialized.code, "a") > 0,
        "numeric target `a` should be typed StoreI64: {:?}",
        specialized.code
    );
    assert!(
        specialized
            .code
            .iter()
            .any(|i| matches!(i, Instr::StoreStr(name) if name == "s")),
        "string target `s` should be typed StoreStr (per-element lowering, #6569): {:?}",
        specialized.code
    );
    assert_eq!(
        store_any_count(&specialized.code, "a") + store_any_count(&specialized.code, "s"),
        0,
        "neither swap target should widen to StoreAny: {:?}",
        specialized.code
    );
}

// ---- End-to-end runtime parity ----
//
// The structural tests above prove the *specialized output* is type-stable.
// These run the whole program through the VM (which triggers runtime
// specialization) to confirm the type-stable swap still produces results
// identical to upstream Julia.

fn run_program(source: &str) -> Value {
    let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));
    vm.run().expect("run program")
}

fn run_i64(source: &str) -> i64 {
    match run_program(source) {
        Value::I64(v) => v,
        other => panic!("expected Int64 result, got {other:?}"),
    }
}

fn run_f64(source: &str) -> f64 {
    match run_program(source) {
        Value::F64(v) => v,
        other => panic!("expected Float64 result, got {other:?}"),
    }
}

#[test]
fn gcd_swap_loop_matches_upstream_julia_6561() {
    // gcd_swap_6561(1071, 462) == 21, gcd_swap_6561(48, 36) == 12 (verified
    // against julia 1.12).
    assert_eq!(
        run_i64(&format!("{GCD_SWAP_SOURCE}\ngcd_swap_6561(1071, 462)\n")),
        21
    );
    assert_eq!(
        run_i64(&format!("{GCD_SWAP_SOURCE}\ngcd_swap_6561(48, 36)\n")),
        12
    );
}

#[test]
fn float_swap_loop_matches_upstream_julia_6561() {
    // float_swap_6561(1.0, 2.0, 10) == 15.9921875 (verified against julia 1.12).
    assert_eq!(
        run_f64(&format!(
            "{FLOAT_SWAP_SOURCE}\nfloat_swap_6561(1.0, 2.0, 10)\n"
        )),
        15.9921875
    );
}

#[test]
fn swap_accumulate_loop_matches_upstream_julia_6561() {
    // swap_sum_6561(1, 1, 2000) == 999369993 (verified against julia 1.12).
    assert_eq!(
        run_i64(&format!(
            "{SWAP_ACCUMULATE_SOURCE}\nswap_sum_6561(1, 1, 2000)\n"
        )),
        999369993
    );
}
