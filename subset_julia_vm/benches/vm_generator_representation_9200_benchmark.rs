//! Generator representation A/B benchmark for the Issue #9200 S6 retire-or-keep
//! decision on the native `MakeGenerator`/`GeneratorCallable` fast paths.
//!
//! By S5 (PR #9465) every native `Value::Generator` consumer was found
//! load-bearing. The single genuine *perf* fast path among them is the
//! `collect(::Base.Generator)` route (`collect_generator`), which materializes
//! the base iterator once and runs the map/filter step through the HOF
//! broadcast machinery in one Rust loop. The upstream iterate-only ideal
//! (`julia/base/generator.jl`) would instead drive `iterate(g::Generator)` one
//! interpreter re-entry per element.
//!
//! Each `collect_*` case runs two interleaved arms via
//! [`set_generator_fastpath_disabled`]: `/fastpath` (shipping) and `/iterate`
//! (bypass → `collect_iterator_via_iterate_protocol`). Numeric parity + an
//! eltype/shape probe are printed (not hard-asserted) so a divergent or erroring
//! `/iterate` route is captured as decision evidence rather than aborting the
//! run. Up front the bench asserts the two arms are genuinely different paths
//! (Protocol step 2) via the empty-generator collect eltype. This is the
//! acceptance-criterion evidence, per the Performance Decision Protocol
//! (CHECKLISTS.md).
//!
//! `sum_generator` is gate-sensitive too: `sum(g::Generator)` is defined
//! `sum(collect(g))` (`base/array.jl`), so it flows through the same `collect`
//! fast path — the `/iterate` arm pays the same synchronous-collector cost.
//!
//! The `comprehension_*` cases compare the eager bracket-comprehension
//! representation (a dedicated array-building loop — the "eager FilterMap fast
//! path") against the generator `collect` representations, since retiring the
//! eager loop would re-lower `[f(x) for x in xs]` to `collect(Generator(f,xs))`.
//!
//! Run: `cargo bench -p subset_julia_vm --bench vm_generator_representation_9200_benchmark`

use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{set_generator_fastpath_disabled, Vm};
use subset_julia_vm_bytecode::CompiledProgram;

const N: usize = 20_000;

fn compile(src: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(src).unwrap();
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run_output(compiled: &CompiledProgram) -> String {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    vm.get_output().to_string()
}

/// Run and return `Ok(output)` or `Err(message)` — used for the parity/probe
/// arms so an erroring `/iterate` route (e.g. a nested lazy generator the pure
/// `iterate` collect cannot drive) is captured as decision evidence instead of
/// panicking and aborting the whole bench.
fn try_run_output(compiled: &CompiledProgram) -> Result<String, String> {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    match vm.run() {
        Ok(_) => Ok(vm.get_output().to_string()),
        Err(e) => Err(format!("{e:?}")),
    }
}

fn bench_run(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    compiled: &CompiledProgram,
    disabled: bool,
) {
    // A filtered generator's collapsed `FilteredFunctionIndex` callable is not
    // drivable by the synchronous pure-`iterate` collector, so the `/iterate`
    // arm errors for filtered shapes. Record that and skip timing rather than
    // panic (keeps the bench exit-clean; the error IS the KEEP-forcing evidence).
    set_generator_fastpath_disabled(disabled);
    let runnable = {
        let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
        vm.run().is_ok()
    };
    set_generator_fastpath_disabled(false);
    if !runnable {
        println!("[9200-S6] {name}: not runnable under this arm (pure-iterate cannot drive it) — skipped");
        return;
    }
    group.bench_function(name, |b| {
        set_generator_fastpath_disabled(disabled);
        b.iter_batched(
            || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
            |mut vm| {
                vm.run().unwrap();
                black_box(vm.get_output().len());
                vm
            },
            BatchSize::PerIteration,
        );
        set_generator_fastpath_disabled(false);
    });
}

/// A/B a gate-sensitive `collect`/generator shape. `big` is the timed program
/// (small output = `sum` of the collected array); `probe` is a small-N program
/// whose full output (values + `typeof`) is compared across arms to surface any
/// eltype/shape divergence.
fn ab_collect(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    big: &str,
    probe: &str,
) {
    let compiled = compile(big);

    // Numeric parity (map/filter semantics) + eltype/shape probe. Both are
    // printed rather than hard-asserted so a divergent or erroring `/iterate`
    // route is captured as S6 decision evidence instead of aborting the bench.
    set_generator_fastpath_disabled(false);
    let out_fast = try_run_output(&compiled);
    set_generator_fastpath_disabled(true);
    let out_iter = try_run_output(&compiled);
    set_generator_fastpath_disabled(false);
    let numeric_ok = out_fast == out_iter;

    let probe_compiled = compile(probe);
    set_generator_fastpath_disabled(false);
    let probe_fast = try_run_output(&probe_compiled);
    set_generator_fastpath_disabled(true);
    let probe_iter = try_run_output(&probe_compiled);
    set_generator_fastpath_disabled(false);
    let probe_ok = probe_fast == probe_iter;

    println!(
        "[9200-S6 probe] {name}: numeric_parity={numeric_ok} byte_identical={probe_ok}\n              \
         fast   = {probe_fast:?}\n              iterate= {probe_iter:?}"
    );

    // Time both arms only when the iterate arm is a valid (byte-identical)
    // drop-in — otherwise the case is correctness-load-bearing and there is no
    // honest apples-to-apples timing to report.
    if numeric_ok && probe_ok {
        bench_run(group, &format!("{name}/fastpath"), &compiled, false);
        bench_run(group, &format!("{name}/iterate"), &compiled, true);
    } else {
        println!(
            "[9200-S6 probe] {name}: iterate arm not byte-identical → \
             timed fastpath only (correctness-load-bearing, KEEP-forcing)"
        );
        bench_run(group, &format!("{name}/fastpath"), &compiled, false);
    }
}

/// Run a source under both arms and print the outputs — no timing. Used for
/// gate-measurable divergences whose timing is not the decision driver.
fn probe_only(name: &str, src: &str) {
    let compiled = compile(src);
    set_generator_fastpath_disabled(false);
    let fast = try_run_output(&compiled);
    set_generator_fastpath_disabled(true);
    let iter = try_run_output(&compiled);
    set_generator_fastpath_disabled(false);
    println!(
        "[9200-S6 probe] {name}: byte_identical={}\n              fast   = {fast:?}\n              iterate= {iter:?}",
        fast == iter
    );
}

fn bench_generator_representation(c: &mut Criterion) {
    // Protocol step 2 — prove the two arms are genuinely different execution
    // paths (not the same code measured twice). The empty-generator collect
    // eltype recovery differs between the fast path and the pure-iterate route,
    // so the arms MUST diverge; if they don't, the gate is a no-op and every
    // number below is meaningless.
    let arms_probe = compile("s = collect(x*x for x in 1:0); println(typeof(s))");
    set_generator_fastpath_disabled(false);
    let arm_a = run_output(&arms_probe);
    set_generator_fastpath_disabled(true);
    let arm_b = run_output(&arms_probe);
    set_generator_fastpath_disabled(false);
    assert_ne!(
        arm_a, arm_b,
        "generator fast-path gate is a no-op (both arms = {arm_a:?}) — the A/B is invalid"
    );

    let mut group = c.benchmark_group("vm_generator_representation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(6));
    group.warm_up_time(Duration::from_secs(1));

    ab_collect(
        &mut group,
        "collect_simple",
        &format!("s = collect(x*x for x in 1:{N}); println(sum(s))"),
        "s = collect(x*x for x in 1:10); println(s); println(typeof(s))",
    );
    ab_collect(
        &mut group,
        "collect_filtered",
        &format!("s = collect(x*x for x in 1:{N} if x % 2 == 0); println(sum(s))"),
        "s = collect(x*x for x in 1:10 if x % 2 == 0); println(s); println(typeof(s))",
    );
    ab_collect(
        &mut group,
        "collect_nested",
        &format!("s = collect(y + 1 for y in (x*x for x in 1:{N})); println(sum(s))"),
        "s = collect(y + 1 for y in (x*x for x in 1:10)); println(s); println(typeof(s))",
    );
    // Empty generator: the fast path recovers the inferred `Vector{Int64}`
    // eltype; the pure-iterate route typejoins an empty value list. Probe-only
    // (no timing) — this is a gate-measurable byte-identical DIVERGENCE that
    // makes eltype recovery correctness-load-bearing, and it doubles as the
    // proof that the gate genuinely flips the execution path (Protocol step 2).
    probe_only(
        "empty_generator",
        "s = collect(x*x for x in 1:0); println(typeof(s)); println(length(s))",
    );

    // Reference: sum over a generator already runs on the pure iterate protocol
    // (both arms identical) — anchors the per-element re-entry cost.
    let sum_src = format!("println(sum(x*x for x in 1:{N}))");
    let sum_compiled = compile(&sum_src);
    bench_run(&mut group, "sum_generator/fastpath", &sum_compiled, false);
    bench_run(&mut group, "sum_generator/iterate", &sum_compiled, true);

    // Eager comprehension representation (dedicated array-building loop) vs the
    // generator collect representations for the same result.
    let comp_simple = compile(&format!("s = [x*x for x in 1:{N}]; println(sum(s))"));
    let comp_simple_gen = compile(&format!("s = collect(x*x for x in 1:{N}); println(sum(s))"));
    assert_eq!(run_output(&comp_simple), {
        set_generator_fastpath_disabled(false);
        run_output(&comp_simple_gen)
    });
    bench_run(
        &mut group,
        "comprehension_simple/eager_loop",
        &comp_simple,
        false,
    );
    bench_run(
        &mut group,
        "comprehension_simple/generator_fastpath",
        &comp_simple_gen,
        false,
    );
    bench_run(
        &mut group,
        "comprehension_simple/generator_iterate",
        &comp_simple_gen,
        true,
    );

    let comp_filtered = compile(&format!(
        "s = [x*x for x in 1:{N} if x % 2 == 0]; println(sum(s))"
    ));
    let comp_filtered_gen = compile(&format!(
        "s = collect(x*x for x in 1:{N} if x % 2 == 0); println(sum(s))"
    ));
    bench_run(
        &mut group,
        "comprehension_filtered/eager_loop",
        &comp_filtered,
        false,
    );
    bench_run(
        &mut group,
        "comprehension_filtered/generator_fastpath",
        &comp_filtered_gen,
        false,
    );
    bench_run(
        &mut group,
        "comprehension_filtered/generator_iterate",
        &comp_filtered_gen,
        true,
    );

    group.finish();
    set_generator_fastpath_disabled(false);
}

criterion_group!(benches, bench_generator_representation);
criterion_main!(benches);
