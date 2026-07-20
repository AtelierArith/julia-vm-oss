//! A/B benchmark for the typed-numeric-fast-path instruction family (Issue #9098).
//!
//! Covers perf-pending items in the arithmetic, comparison, math-function,
//! boolean-logic, select, and numeric-conversion sub-families:
//!
//!   AddF64 SubF64 MulF64 DivF64 NegF64 Abs2F64 AbsF64 PowF64 SqrtF64 CeilF64 FloorF64
//!   AddI64 SubI64 MulI64 ModI64 NegI64
//!   EqF64 NeF64 LtF64 LeF64 GtF64 GeF64
//!   EqI64 NeI64 LtI64 LeI64 GtI64 GeI64
//!   NotBool SelectF64 SelectI64
//!   BoolToI64 I64ToBool ToF64 ToI64
//!
//! A-side (fast path): explicit `::Float64` / `::Int64` annotations force the
//!   compiler to emit typed instructions (AddF64, MulF64, etc.).
//! B-side (dispatch path): annotations removed so the compiler emits DynamicAdd
//!   / DynamicMul / etc., which do a runtime type check before the arithmetic.
//!
//! Threshold for perf-measured: B/A ≥ 1.5x (≥50% slower without fast path).
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_perf_pending_numeric_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

// ---------------------------------------------------------------------------
// A-side: typed Float64 arithmetic hot loop (AddF64 SubF64 MulF64 DivF64 NegF64)
// ---------------------------------------------------------------------------
const F64_ARITH_TYPED: &str = r#"
function f64_arith_typed_9098(n::Int64)::Float64
    s = 0.0
    x = 1.0
    for i in 1:n
        s = s + x
        s = s - 0.5
        s = s * 1.001
        s = s / 1.001
        x = -x
        x = -x
    end
    return s
end
f64_arith_typed_9098(50000)
"#;

// B-side: same logic but no type annotations → DynamicAdd / DynamicSub / …
const F64_ARITH_DYNAMIC: &str = r#"
function f64_arith_dynamic_9098(n)
    s = 0.0
    x = 1.0
    for i in 1:n
        s = s + x
        s = s - 0.5
        s = s * 1.001
        s = s / 1.001
        x = -x
        x = -x
    end
    return s
end
f64_arith_dynamic_9098(50000)
"#;

// ---------------------------------------------------------------------------
// Int64 arithmetic (AddI64 SubI64 MulI64 ModI64 NegI64)
// ---------------------------------------------------------------------------
const I64_ARITH_TYPED: &str = r#"
function i64_arith_typed_9098(n::Int64)::Int64
    s = Int64(0)
    for i in 1:n
        s = s + i
        s = s - 1
        s = s * 1
        s = s % 1000003
        s = -s
        s = -s
    end
    return s
end
i64_arith_typed_9098(50000)
"#;

const I64_ARITH_DYNAMIC: &str = r#"
function i64_arith_dynamic_9098(n)
    s = 0
    for i in 1:n
        s = s + i
        s = s - 1
        s = s * 1
        s = s % 1000003
        s = -s
        s = -s
    end
    return s
end
i64_arith_dynamic_9098(50000)
"#;

// ---------------------------------------------------------------------------
// Math functions (Abs2F64 AbsF64 SqrtF64 CeilF64 FloorF64 PowF64)
// ---------------------------------------------------------------------------
const MATH_TYPED: &str = r#"
function math_typed_9098(n::Int64)::Float64
    s = 1.0
    for i in 1:n
        s = abs(s)
        s = sqrt(abs(s) + 1.0)
        s = ceil(s * 0.999)
        s = floor(s + 0.5)
        s = abs2(s - 1.0) + 0.001
        s = s ^ 1.001
    end
    return s
end
math_typed_9098(10000)
"#;

const MATH_DYNAMIC: &str = r#"
function math_dynamic_9098(n)
    s = 1.0
    for i in 1:n
        s = abs(s)
        s = sqrt(abs(s) + 1.0)
        s = ceil(s * 0.999)
        s = floor(s + 0.5)
        s = abs2(s - 1.0) + 0.001
        s = s ^ 1.001
    end
    return s
end
math_dynamic_9098(10000)
"#;

// ---------------------------------------------------------------------------
// Comparisons F64 (EqF64 NeF64 LtF64 LeF64 GtF64 GeF64)
// ---------------------------------------------------------------------------
const CMP_F64_TYPED: &str = r#"
function cmp_f64_typed_9098(n::Int64)::Int64
    count = Int64(0)
    x = 0.0
    for i in 1:n
        x = x + 1.0
        y = x - 0.5
        if x == y + 0.5
            count = count + 1
        end
        if x != y
            count = count + 1
        end
        if x > y
            count = count + 1
        end
        if x >= y
            count = count + 1
        end
        if y < x
            count = count + 1
        end
        if y <= x
            count = count + 1
        end
    end
    return count
end
cmp_f64_typed_9098(20000)
"#;

const CMP_F64_DYNAMIC: &str = r#"
function cmp_f64_dynamic_9098(n)
    count = 0
    x = 0.0
    for i in 1:n
        x = x + 1.0
        y = x - 0.5
        if x == y + 0.5
            count = count + 1
        end
        if x != y
            count = count + 1
        end
        if x > y
            count = count + 1
        end
        if x >= y
            count = count + 1
        end
        if y < x
            count = count + 1
        end
        if y <= x
            count = count + 1
        end
    end
    return count
end
cmp_f64_dynamic_9098(20000)
"#;

// ---------------------------------------------------------------------------
// Comparisons I64 (EqI64 NeI64 LtI64 LeI64 GtI64 GeI64)
// ---------------------------------------------------------------------------
const CMP_I64_TYPED: &str = r#"
function cmp_i64_typed_9098(n::Int64)::Int64
    count = Int64(0)
    for i in 1:n
        j = i + 1
        if i == i
            count = count + 1
        end
        if i != j
            count = count + 1
        end
        if i < j
            count = count + 1
        end
        if i <= j
            count = count + 1
        end
        if j > i
            count = count + 1
        end
        if j >= i
            count = count + 1
        end
    end
    return count
end
cmp_i64_typed_9098(20000)
"#;

const CMP_I64_DYNAMIC: &str = r#"
function cmp_i64_dynamic_9098(n)
    count = 0
    for i in 1:n
        j = i + 1
        if i == i
            count = count + 1
        end
        if i != j
            count = count + 1
        end
        if i < j
            count = count + 1
        end
        if i <= j
            count = count + 1
        end
        if j > i
            count = count + 1
        end
        if j >= i
            count = count + 1
        end
    end
    return count
end
cmp_i64_dynamic_9098(20000)
"#;

// ---------------------------------------------------------------------------
// Bool logic + select + conversions (NotBool SelectF64 SelectI64 BoolToI64 I64ToBool ToF64 ToI64)
// ---------------------------------------------------------------------------
const BOOL_SELECT_CONV_TYPED: &str = r#"
function bool_select_conv_typed_9098(n::Int64)::Int64
    total = Int64(0)
    for i in 1:n
        b = (i % 2) == 0
        nb = !b
        sel_f = ifelse(b, 1.0, 2.0)
        sel_i = ifelse(nb, Int64(10), Int64(20))
        iv = Int64(b)
        bv = iv != 0
        fv = Float64(i)
        iv2 = Int64(fv)
        total = total + sel_i + iv + Int64(bv) + iv2
    end
    return total
end
bool_select_conv_typed_9098(20000)
"#;

const BOOL_SELECT_CONV_DYNAMIC: &str = r#"
function bool_select_conv_dynamic_9098(n)
    total = 0
    for i in 1:n
        b = (i % 2) == 0
        nb = !b
        sel_f = ifelse(b, 1.0, 2.0)
        sel_i = ifelse(nb, 10, 20)
        iv = Int64(b)
        bv = iv != 0
        fv = Float64(i)
        iv2 = Int64(fv)
        total = total + sel_i + iv + Int64(bv) + iv2
    end
    return total
end
bool_select_conv_dynamic_9098(20000)
"#;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compile(source: &str) -> CompiledProgram {
    let program = parse_and_lower(source).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run_and_check(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    black_box(vm);
}

struct Case {
    name: &'static str,
    typed_src: &'static str,
    dynamic_src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "f64_arith",
        typed_src: F64_ARITH_TYPED,
        dynamic_src: F64_ARITH_DYNAMIC,
    },
    Case {
        name: "i64_arith",
        typed_src: I64_ARITH_TYPED,
        dynamic_src: I64_ARITH_DYNAMIC,
    },
    Case {
        name: "math_fns",
        typed_src: MATH_TYPED,
        dynamic_src: MATH_DYNAMIC,
    },
    Case {
        name: "cmp_f64",
        typed_src: CMP_F64_TYPED,
        dynamic_src: CMP_F64_DYNAMIC,
    },
    Case {
        name: "cmp_i64",
        typed_src: CMP_I64_TYPED,
        dynamic_src: CMP_I64_DYNAMIC,
    },
    Case {
        name: "bool_select_conv",
        typed_src: BOOL_SELECT_CONV_TYPED,
        dynamic_src: BOOL_SELECT_CONV_DYNAMIC,
    },
];

fn bench_numeric(c: &mut Criterion) {
    for case in CASES {
        let typed_compiled = compile(case.typed_src);
        let dynamic_compiled = compile(case.dynamic_src);

        // Warm-up validation.
        run_and_check(&typed_compiled);
        run_and_check(&dynamic_compiled);

        let mut group = c.benchmark_group(format!("perf_pending_numeric/{}", case.name));
        // Reduced time so CI and iterative runs are fast; increase for publication.
        group.warm_up_time(Duration::from_millis(500));
        group.measurement_time(Duration::from_millis(2000));

        group.bench_with_input(
            BenchmarkId::new("typed_fastpath", case.name),
            &typed_compiled,
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
            BenchmarkId::new("dynamic_dispatch", case.name),
            &dynamic_compiled,
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

criterion_group!(benches, bench_numeric);
criterion_main!(benches);
