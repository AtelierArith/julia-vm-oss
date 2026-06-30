//! Interprocedural return-type WORK budget tests (Issue #8185).
//!
//! `_bfgs` (un-annotated) blew `compile.build_method_tables` to ~5.3 s / 97 % of
//! `using Optim` load time (#8182): a closure defined in a loop, threaded through
//! the deep mutually-recursive HagerZhang line-search tree, re-specialized under
//! the loop fixpoint. `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH` bounds DEPTH but not
//! total WORK. These tests cover the two prevention layers:
//!
//! * the always-on per-package load-time smoke test (`using Optim` inference work
//!   stays bounded — the mechanism that catches a #8182-style regression while
//!   functional tests stay green), and
//! * the catastrophe backstop (`MAX_INTERPROCEDURAL_ANALYSIS_WORK`): when a root
//!   inference's work is exhausted, `infer_block_with_fixpoint` widens to `Top`.

use super::*;
use std::collections::HashMap;

/// The backstop fires: once a root inference has consumed the work budget, the
/// next `infer_block_with_fixpoint` widens to `Top` (safe over-approximation)
/// instead of recursing further, and the event is recorded.
#[test]
fn work_budget_backstop_widens_to_top_when_exhausted_8185() {
    work_budget_metrics::clear();
    let func = int_identity_function("trivial_8185");
    let mut engine = InferenceEngine::with_tables(HashMap::new(), HashMap::new());
    // Simulate being deep inside a pathological root whose work has already
    // reached the cap. `analysis_depth == 1` (non-root) so the per-root reset at
    // the top of `infer_block_with_fixpoint` does NOT clear the counter.
    engine.analysis_depth = 1;
    engine.analysis_work = MAX_INTERPROCEDURAL_ANALYSIS_WORK;
    let mut env = TypeEnv::new();
    let result = engine.infer_block_with_fixpoint(&func.body, &mut env);
    assert_eq!(
        result,
        LatticeType::Top,
        "an exhausted interprocedural work budget must widen to Top"
    );
    assert!(
        work_budget_metrics::budget_exceeded_count() >= 1,
        "the backstop trip must be recorded"
    );
}

/// A normal (trivial) function inference is nowhere near the budget, so it never
/// trips the backstop — guards against the cap being accidentally set too low.
#[test]
fn normal_inference_stays_far_under_work_budget_8185() {
    work_budget_metrics::clear();
    let func = int_identity_function("normal_8185");
    let mut engine = InferenceEngine::with_tables(HashMap::new(), HashMap::new());
    let _ = engine.infer_function(&func);
    assert_eq!(
        work_budget_metrics::budget_exceeded_count(),
        0,
        "trivial inference must not trip the work budget"
    );
    assert!(
        work_budget_metrics::peak_work() <= 16,
        "trivial inference peak work should be tiny, got {}",
        work_budget_metrics::peak_work()
    );
}

/// Load-time regression smoke test (#8185 task 1a): `using Optim` return-type
/// inference must stay bounded. With the `_bfgs(...)::MultivariateOptimizationResults`
/// annotation the per-root peak is ~700; without it the body inference explodes
/// to ~174k (#8182). The threshold sits far above the annotated baseline and far
/// below the blow-up, so it catches a regression (annotation removed, or a new
/// un-annotated closure-threaded solver added) while functional tests stay green.
#[test]
fn using_optim_load_inference_stays_bounded_8185() {
    work_budget_metrics::clear();
    let _ = crate::api::compile_and_run_str("using Optim\nprintln(\"ok\")\n", 0);
    let peak = work_budget_metrics::peak_work();
    assert!(
        peak < 50_000,
        "`using Optim` interprocedural inference peak work was {peak} (threshold 50_000): a \
         closure-threaded return-type blow-up likely regressed — e.g. the `_bfgs` return-type \
         annotation was removed, or a new un-annotated deep-recursion solver was added. See \
         #8182 / #8185."
    );
    assert_eq!(
        work_budget_metrics::budget_exceeded_count(),
        0,
        "`using Optim` must not trip the catastrophe backstop"
    );
}

/// `using Symbolics` used to spend ~10 s in `compile.build_method_tables` because
/// load-time return inference expanded recursive expression walkers
/// (`_simplify`/`_expand`/`det`/show/substitute helpers). The return annotations
/// added for #8213 keep that package-load work bounded while functional fixtures
/// continue to cover the symbolic behavior.
#[test]
fn using_symbolics_load_inference_stays_bounded_8213() {
    work_budget_metrics::clear();
    let _ = crate::api::compile_and_run_str("using Symbolics\nprintln(\"ok\")\n", 0);
    let peak = work_budget_metrics::peak_work();
    assert!(
        peak < 50_000,
        "`using Symbolics` interprocedural inference peak work was {peak} (threshold 50_000): \
         recursive Symbolics expression walkers likely lost their load-time return annotations. \
         See #8213."
    );
    assert_eq!(
        work_budget_metrics::budget_exceeded_count(),
        0,
        "`using Symbolics` must not trip the catastrophe backstop"
    );
}
