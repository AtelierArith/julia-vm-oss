//! Issue #6569: lower a self-referential destructuring swap whose RHS is a
//! tuple literal (`a, b = b, a % b`) WITHOUT allocating a tuple.
//!
//! The swap is desugared into per-element temporaries
//! (`__t0 = b; __t1 = a % b; a = __t0; b = __t1`) instead of a temporary tuple
//! plus indexed reads (`__tmp = (b, a % b); a = __tmp[1]; b = __tmp[2]`). This
//! removes the per-iteration `NewTuple` heap allocation and the `IndexLoad`
//! reads, matching CPython's allocation-free swap and Julia's native handling.

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

const GCD_SWAP_SOURCE: &str = r#"
function gcd_swap_6569(a, b)
    while b != 0
        a, b = b, a % b
    end
    return a
end
"#;

/// The integer swap specializes with NO tuple allocation and NO tuple index:
/// the desugared swap is pure per-element temps. (Issue #6569)
#[test]
fn int_swap_specializes_without_tuple_alloc_6569() {
    let compiled = compile_source(GCD_SWAP_SOURCE);
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "gcd_swap_6569"),
        &[ValueType::I64, ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize integer swap loop");

    assert!(
        !specialized
            .code
            .iter()
            .any(|i| matches!(i, Instr::NewTuple(_))),
        "tuple-literal swap must not allocate a tuple: {:?}",
        specialized.code
    );
    assert!(
        !specialized
            .code
            .iter()
            .any(|i| matches!(i, Instr::IndexLoad(_))),
        "tuple-literal swap must not index a tuple: {:?}",
        specialized.code
    );
    // Still type-stable: a/b keep their typed stores and the function returns
    // the typed value.
    assert!(
        specialized
            .code
            .iter()
            .any(|i| matches!(i, Instr::StoreI64(name) if name == "a"))
            && specialized
                .code
                .iter()
                .any(|i| matches!(i, Instr::StoreI64(name) if name == "b")),
        "swap targets should still use typed StoreI64: {:?}",
        specialized.code
    );
    assert_eq!(specialized.return_type, ValueType::I64);
}

/// A three-way rotation `a, b, c = b, c, a` also lowers allocation-free and
/// preserves the simultaneous-assignment semantics. (Issue #6569)
#[test]
fn three_cycle_rotation_specializes_without_tuple_alloc_6569() {
    let compiled = compile_source(
        r#"
function rotate3_6569(a, b, c, n)
    for _ in 1:n
        a, b, c = b, c, a
    end
    return a * 100 + b * 10 + c
end
    "#,
    );
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "rotate3_6569"),
        &[
            ValueType::I64,
            ValueType::I64,
            ValueType::I64,
            ValueType::I64,
        ],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize 3-cycle rotation");

    assert!(
        !specialized
            .code
            .iter()
            .any(|i| matches!(i, Instr::NewTuple(_) | Instr::IndexLoad(_))),
        "3-cycle rotation must not allocate or index a tuple: {:?}",
        specialized.code
    );
}

/// Lower a function and return the debug representation of its body, used to
/// inspect the desugared destructuring shape.
fn lower_function_body_debug(source: &str, name: &str) -> String {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    let func = program
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function '{name}' not found"));
    format!("{:#?}", func.body)
}

/// A tuple-literal swap lowers to per-element temps with NO `Expr::Index` (no
/// tuple indexing), while a non-tuple-literal RHS (`a, b = t`) cannot be split
/// element-wise and keeps the temp-tuple + `Expr::Index` lowering. (Issue #6569)
#[test]
fn tuple_literal_swap_lowers_without_index_but_var_rhs_keeps_it_6569() {
    let swap_body = lower_function_body_debug(GCD_SWAP_SOURCE, "gcd_swap_6569");
    assert!(
        !swap_body.contains("Index"),
        "tuple-literal swap must lower without Expr::Index:\n{swap_body}"
    );

    let var_rhs = lower_function_body_debug(
        r#"
function take_pair_6569(t)
    a, b = t
    return a + b
end
"#,
        "take_pair_6569",
    );
    assert!(
        var_rhs.contains("Index"),
        "non-literal RHS destructuring should still use Expr::Index:\n{var_rhs}"
    );
}

// ---- End-to-end runtime parity ----

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

#[test]
fn swap_results_match_upstream_julia_6569() {
    // gcd_swap_6569(1071, 462) == 21 (verified against julia 1.12).
    assert_eq!(
        run_i64(&format!("{GCD_SWAP_SOURCE}\ngcd_swap_6569(1071, 462)\n")),
        21
    );
    // 3-cycle rotation: starting (1,2,3), after 1 rotation (b,c,a) = (2,3,1)
    // -> 2*100 + 3*10 + 1 = 231; after 3 rotations back to (1,2,3) -> 123.
    let rot = r#"
function rotate3_6569(a, b, c, n)
    for _ in 1:n
        a, b, c = b, c, a
    end
    return a * 100 + b * 10 + c
end
"#;
    assert_eq!(run_i64(&format!("{rot}\nrotate3_6569(1, 2, 3, 1)\n")), 231);
    assert_eq!(run_i64(&format!("{rot}\nrotate3_6569(1, 2, 3, 3)\n")), 123);
}
