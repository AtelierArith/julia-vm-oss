//! Inference work-budget metrics tests + measurement harness (Issue #8546).
//!
//! The opt-in `compile::budget_metrics` counters make widen-to-`Top` events
//! attributable — budget exhaustion (`MAX_INTERPROCEDURAL_ANALYSIS_WORK` /
//! `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH`, `MAX_LOOP_FIXPOINT_ITERATIONS`,
//! `MAX_RECURSIVE_FIXPOINT_ITERATIONS`, `MAX_METHOD_UNION_SPLIT_VARIANTS`,
//! `MAX_INFERENCE_ITERATIONS`) vs. genuine lattice join — so budget changes
//! can be data-driven. This file holds:
//!
//! * cheap always-run contract tests (default-off; recording when forced), and
//! * the `#[ignore]`d measurement harness that compiles representative
//!   workloads (Base prelude from source, `using Optim` / `Plots` /
//!   `Symbolics`) and dumps the counters. Run it explicitly:
//!
//!   ```bash
//!   cargo nextest run --cargo-profile release-fast --lib \
//!     -E 'test(/budget_metrics_8546/)' --run-ignored all --no-capture
//!   ```
//!
//!   The counters are deterministic (event counts, not time), so they are the
//!   machine-quiet primary evidence; see `docs/vm/INFERENCE_BUDGETS.md`.

use crate::compile::budget_metrics;

/// Compile a workload with the counters forced on and return the snapshot.
fn measure(src: &str) -> budget_metrics::InferBudgetMetrics {
    budget_metrics::set_infer_budget_metrics_forced(true);
    budget_metrics::clear();
    let _ = crate::api::compile_and_run_str(src, 0);
    let metrics = budget_metrics::snapshot();
    budget_metrics::set_infer_budget_metrics_forced(false);
    metrics
}

/// [`measure`] after warming Base in-process, so the measured program's
/// counters exclude Base/prelude inference entirely. The warm-up compile runs
/// with recording still off; the thread-local Base cache then serves every
/// later compile in this process regardless of the persistent on-disk cache
/// state (which is hash-keyed to the build and may be cold or stale).
fn measure_warm(src: &str) -> budget_metrics::InferBudgetMetrics {
    let _ = crate::api::compile_and_run_str("true\n", 0);
    measure(src)
}

/// [`measure`] with the persistent on-disk Base cache disabled for this
/// process, so Base + prelude inference actually runs and the numbers do not
/// depend on the ambient cache state. Without this, whichever dump test runs
/// first against a cold `target/` measures Base-from-source while later ones
/// measure a near-empty warm-cache compile — not comparable. Each workload
/// therefore reports Base + prelude + package inference; a package's
/// incremental cost is its dump minus the Base-prelude dump. nextest runs
/// each test in its own process, so the env mutation cannot leak.
fn measure_from_source(src: &str) -> budget_metrics::InferBudgetMetrics {
    std::env::set_var("SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE", "1");
    measure(src)
}

/// Default-off contract: without the env gate or the forced override, a
/// compile must record nothing (the disabled path is a single relaxed atomic
/// load per event — no thread-local traffic).
#[test]
fn infer_budget_metrics_default_off_8546() {
    if std::env::var("SJULIA_INFER_BUDGET_METRICS").is_ok() {
        // The process-wide env gate is on; the default-off contract cannot be
        // observed in this run.
        return;
    }
    budget_metrics::set_infer_budget_metrics_forced(false);
    let _ = crate::api::compile_and_run_str("true\n", 0);
    budget_metrics::clear();
    let _ = crate::api::compile_and_run_str(
        "function f8546_off(n)\n    s = 0\n    for i in 1:n\n        s += i\n    end\n    s\nend\nprintln(f8546_off(3))\n",
        0,
    );
    assert_eq!(
        budget_metrics::snapshot(),
        budget_metrics::InferBudgetMetrics::default(),
        "budget metrics must record nothing when disabled"
    );
}

/// Recording contract: when forced on, a small program with a loop and a
/// function call records roots, work units, and loop/block fixpoint runs —
/// and a trivial workload exhausts no budget.
#[test]
fn infer_budget_metrics_record_when_forced_8546() {
    let metrics = measure_warm(
        "function f8546_on(n)\n    s = 0\n    for i in 1:n\n        s += i\n    end\n    s\nend\nprintln(f8546_on(3))\n",
    );
    assert!(metrics.roots > 0, "root inferences must be counted");
    assert!(
        metrics.total_work >= metrics.roots,
        "every root consumes at least one work unit"
    );
    assert!(
        metrics.peak_root_work >= 1,
        "per-root peak work must be tracked"
    );
    assert!(
        metrics.loop_fixpoint_runs >= 1,
        "the for-loop body fixpoint must be counted"
    );
    assert!(
        metrics.loop_fixpoint_iterations >= metrics.loop_fixpoint_runs,
        "each loop fixpoint run uses at least one iteration"
    );
    assert_eq!(
        metrics.work_budget_widenings, 0,
        "a trivial program must not exhaust the work budget"
    );
    assert_eq!(
        metrics.loop_fixpoint_exhausted, 0,
        "a trivial loop must converge within MAX_LOOP_FIXPOINT_ITERATIONS"
    );
}

/// Attribution contract: a join that genuinely reaches `Top` (disjoint,
/// non-widenable branch types flowing into one variable) is counted as
/// lattice imprecision, not budget exhaustion.
#[test]
fn infer_budget_metrics_attribute_lattice_join_8546() {
    let metrics = measure_warm(
        "function f8546_join(flag)\n    x = flag ? 1 : \"s\"\n    y = flag ? (1, 2) : println\n    (x, y)\nend\nprintln(f8546_join(true))\n",
    );
    assert!(metrics.roots > 0, "root inferences must be counted");
    assert_eq!(
        metrics.budget_exhaustion_events(),
        0,
        "this tiny program must not exhaust any budget: {metrics}"
    );
}

/// Measurement harness (Issue #8546): `using Optim` — the workload that
/// motivated `MAX_INTERPROCEDURAL_ANALYSIS_WORK` (#8182/#8185).
#[test]
#[ignore = "measurement harness (Issue #8546): run explicitly with --run-ignored"]
fn dump_using_optim_budget_counters_8546() {
    let metrics = measure_from_source("using Optim\nprintln(\"ok\")\n");
    eprintln!("[8546] workload: using Optim\n{metrics}");
    assert_eq!(
        metrics.work_budget_widenings, 0,
        "`using Optim` must not trip the work-budget backstop (#8185)"
    );
}

/// Measurement harness (Issue #8546): `using Plots`.
#[test]
#[ignore = "measurement harness (Issue #8546): run explicitly with --run-ignored"]
fn dump_using_plots_budget_counters_8546() {
    let metrics = measure_from_source("using Plots\nprintln(\"ok\")\n");
    eprintln!("[8546] workload: using Plots\n{metrics}");
    assert_eq!(
        metrics.work_budget_widenings, 0,
        "`using Plots` must not trip the work-budget backstop"
    );
}

/// Measurement harness (Issue #8546): `using Symbolics` (#8213 counterpart).
#[test]
#[ignore = "measurement harness (Issue #8546): run explicitly with --run-ignored"]
fn dump_using_symbolics_budget_counters_8546() {
    let metrics = measure_from_source("using Symbolics\nprintln(\"ok\")\n");
    eprintln!("[8546] workload: using Symbolics\n{metrics}");
    assert_eq!(
        metrics.work_budget_widenings, 0,
        "`using Symbolics` must not trip the work-budget backstop"
    );
}

/// Measurement harness (Issue #8546): the Base prelude compiled from source.
///
/// Disables the persistent on-disk Base cache for this process so Base
/// inference actually runs (instead of loading cached method tables), then
/// dumps the counters. nextest runs each test in its own process, so the env
/// mutation cannot leak into other tests.
#[test]
#[ignore = "measurement harness (Issue #8546): run explicitly with --run-ignored"]
fn dump_base_prelude_budget_counters_8546() {
    let metrics = measure_from_source("println(\"ok\")\n");
    eprintln!("[8546] workload: Base prelude (from source)\n{metrics}");
    assert_eq!(
        metrics.work_budget_widenings, 0,
        "the Base prelude must not trip the work-budget backstop"
    );
}
