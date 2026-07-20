//! A/B benchmark for the Dynamic-arithmetic / Dynamic-conversion instruction
//! family (Issue #9098).
//!
//! Covers perf-pending items:
//!   DynamicAdd DynamicSub DynamicMul DynamicDiv DynamicMod DynamicIntDiv
//!   DynamicNeg DynamicPow
//!   DynamicToBool DynamicToF16 DynamicToF32 DynamicToF64
//!   DynamicToI8 DynamicToI16 DynamicToI32 DynamicToI64
//!   DynamicToU8 DynamicToU16 DynamicToU32 DynamicToU64
//!
//! A-side (fast path): code whose operand types are unknown at compile time so
//!   the compiler emits Dynamic* instructions (Rust-level runtime dispatch with
//!   an inline same-type check before the Julia method table).
//! B-side (slower path): same operations invoked via a wrapper that defeats
//!   the Dynamic* inline optimisation, falling back to the Julia method table.
//!   Achieved by wrapping arguments in a `Ref{Any}`-like barrier.
//!
//! Note: for DynamicAdd/etc. the "B-side" in this no-JIT VM is the full
//! Julia dispatch path (method table lookup + call frame setup).  The A-side
//! (Dynamic*) saves that overhead for same-type primitive pairs.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_perf_pending_dynamic_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

// ---------------------------------------------------------------------------
// Dynamic arithmetic: A-side uses untyped params (→ Dynamic* instrs)
// B-side wraps in a @noinline fence to force full Julia dispatch.
// ---------------------------------------------------------------------------

/// A-side: untyped function → compiler emits DynamicAdd / DynamicMul / etc.
const DYN_ARITH_A: &str = r#"
function dyn_arith_a_9098(n)
    s = 0
    for i in 1:n
        s = s + i
        s = s - 1
        s = s * 2
        s = div(s, 2)
        s = s % 1000003
        s = -s
        s = -s
        s = s ^ 1
    end
    return s
end
dyn_arith_a_9098(20000)
"#;

/// B-side: same arithmetic but each op goes through a named dispatch barrier
/// that prevents the compiler from recognising the type → Julia method table.
const DYN_ARITH_B: &str = r#"
@noinline dispatch_add_9098(a, b) = a + b
@noinline dispatch_sub_9098(a, b) = a - b
@noinline dispatch_mul_9098(a, b) = a * b
@noinline dispatch_div_9098(a, b) = div(a, b)
@noinline dispatch_mod_9098(a, b) = a % b
@noinline dispatch_neg_9098(a) = -a
@noinline dispatch_pow_9098(a, b) = a ^ b

function dyn_arith_b_9098(n)
    s = 0
    for i in 1:n
        s = dispatch_add_9098(s, i)
        s = dispatch_sub_9098(s, 1)
        s = dispatch_mul_9098(s, 2)
        s = dispatch_div_9098(s, 2)
        s = dispatch_mod_9098(s, 1000003)
        s = dispatch_neg_9098(s)
        s = dispatch_neg_9098(s)
        s = dispatch_pow_9098(s, 1)
    end
    return s
end
dyn_arith_b_9098(20000)
"#;

// ---------------------------------------------------------------------------
// Dynamic conversions: A-side uses Dynamic* conversion instrs
// ---------------------------------------------------------------------------

const DYN_CONV_A: &str = r#"
function dyn_conv_a_9098(n)
    total = 0
    for i in 1:n
        b  = Bool(i % 2)
        f32v = Float32(i)
        f64v = Float64(i)
        i8v  = Int8(i % 100)
        i16v = Int16(i % 1000)
        i32v = Int32(i)
        i64v = Int64(i)
        u8v  = UInt8(i % 200)
        u16v = UInt16(i % 5000)
        u32v = UInt32(i)
        u64v = UInt64(i)
        total = total + Int64(b) + Int64(i8v) + Int64(i16v) + i32v + i64v + Int64(u8v) + Int64(u16v) + Int64(u32v) + Int64(u64v)
    end
    return total
end
dyn_conv_a_9098(5000)
"#;

const DYN_CONV_B: &str = r#"
@noinline conv_bool_9098(x) = Bool(x % 2)
@noinline conv_f32_9098(x) = Float32(x)
@noinline conv_f64_9098(x) = Float64(x)
@noinline conv_i8_9098(x) = Int8(x % 100)
@noinline conv_i16_9098(x) = Int16(x % 1000)
@noinline conv_i32_9098(x) = Int32(x)
@noinline conv_i64_9098(x) = Int64(x)
@noinline conv_u8_9098(x) = UInt8(x % 200)
@noinline conv_u16_9098(x) = UInt16(x % 5000)
@noinline conv_u32_9098(x) = UInt32(x)
@noinline conv_u64_9098(x) = UInt64(x)

function dyn_conv_b_9098(n)
    total = 0
    for i in 1:n
        b    = conv_bool_9098(i)
        f32v = conv_f32_9098(i)
        f64v = conv_f64_9098(i)
        i8v  = conv_i8_9098(i)
        i16v = conv_i16_9098(i)
        i32v = conv_i32_9098(i)
        i64v = conv_i64_9098(i)
        u8v  = conv_u8_9098(i)
        u16v = conv_u16_9098(i)
        u32v = conv_u32_9098(i)
        u64v = conv_u64_9098(i)
        total = total + Int64(b) + Int64(i8v) + Int64(i16v) + i32v + i64v + Int64(u8v) + Int64(u16v) + Int64(u32v) + Int64(u64v)
    end
    return total
end
dyn_conv_b_9098(5000)
"#;

fn compile(source: &str) -> CompiledProgram {
    let program = parse_and_lower(source).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    black_box(vm);
}

struct Case {
    name: &'static str,
    a_src: &'static str,
    b_src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "dyn_arith",
        a_src: DYN_ARITH_A,
        b_src: DYN_ARITH_B,
    },
    Case {
        name: "dyn_conv",
        a_src: DYN_CONV_A,
        b_src: DYN_CONV_B,
    },
];

fn bench_dynamic(c: &mut Criterion) {
    for case in CASES {
        let a_compiled = compile(case.a_src);
        let b_compiled = compile(case.b_src);

        run(&a_compiled);
        run(&b_compiled);

        let mut group = c.benchmark_group(format!("perf_pending_dynamic/{}", case.name));
        group.warm_up_time(Duration::from_millis(500));
        group.measurement_time(Duration::from_millis(2000));

        group.bench_with_input(
            BenchmarkId::new("dynamic_instr", case.name),
            &a_compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let r = vm.run().unwrap();
                        black_box(r);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("julia_dispatch", case.name),
            &b_compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let r = vm.run().unwrap();
                        black_box(r);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.finish();
    }
}

criterion_group!(benches, bench_dynamic);
criterion_main!(benches);
