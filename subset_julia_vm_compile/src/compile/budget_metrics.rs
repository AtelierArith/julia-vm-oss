//! Opt-in inference work-budget metrics (Issue #8546).
//!
//! The inference engine bounds its work with several budgets
//! (`MAX_LOOP_FIXPOINT_ITERATIONS`, `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH`,
//! `MAX_INTERPROCEDURAL_ANALYSIS_WORK`, `MAX_RECURSIVE_FIXPOINT_ITERATIONS`,
//! `MAX_METHOD_UNION_SPLIT_VARIANTS` in `abstract_interp/engine/mod.rs`).
//! When a budget is exhausted the engine widens (typically to `Top`), which is
//! indistinguishable — from the result alone — from a *genuine lattice join*
//! that reached `Any`. After the abstract-domain enrichment slices
//! (PartialStruct #8544, InterConditional #8545) the budgets need re-evaluation
//! with data, so this module makes every widening event *attributable*:
//! budget exhaustion (split per budget) vs. genuine lattice join to `Top`.
//!
//! # Relationship to existing metrics
//!
//! - `work_budget_metrics` (engine/mod.rs, Issue #8185) is *always on* and
//!   intentionally minimal (peak work + backstop trips) so the default `--lib`
//!   regression tests can read it. It stays untouched.
//! - `infer_metrics` (Issue #5096) is compiled only under the `profiling`
//!   feature, so it cannot serve a default-build measurement harness.
//! - This module is compiled in every build but **off by default**: recording
//!   is gated on `SJULIA_INFER_BUDGET_METRICS=1` (or the process-wide
//!   [`set_infer_budget_metrics_forced`] override, mirroring
//!   `vm/stack_metrics.rs` from Issue #8559). When disabled, every record call
//!   is a single relaxed atomic load and an early return — no thread-local
//!   traffic — so wall-time benchmarks are unaffected.
//!
//! # Determinism
//!
//! All counters are deterministic for a deterministic compilation: they count
//! engine events, not time, so they are load-independent and comparable across
//! hosts and build profiles (the machine-quiet primary evidence for budget
//! decisions; see `docs/vm/INFERENCE_BUDGETS.md`).
//!
//! Counters are thread-local: compilation is synchronous on the calling
//! thread, so a harness reads back exactly what its compile wrote.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Process-wide override so tests (and hosts without an environment) can
/// enable collection without `std::env::set_var`. OR-ed with the environment
/// gate on every record call.
static FORCED: AtomicBool = AtomicBool::new(false);

/// Environment gate, read once per process.
static ENV_ENABLED: OnceLock<bool> = OnceLock::new();

/// Force inference budget metrics collection on/off for the whole process,
/// regardless of `SJULIA_INFER_BUDGET_METRICS` (Issue #8546).
pub fn set_infer_budget_metrics_forced(enabled: bool) {
    FORCED.store(enabled, Ordering::Relaxed);
}

#[inline]
fn env_enabled() -> bool {
    *ENV_ENABLED.get_or_init(|| {
        std::env::var("SJULIA_INFER_BUDGET_METRICS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Whether recording is enabled. One relaxed atomic load on the common
/// (disabled) path; the `OnceLock` is only consulted when not forced.
#[inline]
pub(crate) fn enabled() -> bool {
    FORCED.load(Ordering::Relaxed) || env_enabled()
}

/// Snapshot of the inference budget counters (Issue #8546).
///
/// Widen-to-`Top` (and widen-to-imprecise) events are split by trigger so a
/// budget re-evaluation can tell budget exhaustion apart from genuine lattice
/// imprecision:
///
/// - budget exhaustion: `work_budget_widenings`, `depth_limit_cutoffs`,
///   `loop_fixpoint_exhausted`, `recursive_fixpoint_exhausted`,
///   `block_fixpoint_limit_hits`, `union_split_bailouts`
/// - genuine lattice join: `lattice_join_top_widenings`
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InferBudgetMetrics {
    /// Root inferences (`infer_block_with_fixpoint` at `analysis_depth == 0`,
    /// where the #8185 per-root work counter resets).
    pub roots: u64,
    /// Total `infer_block_with_fixpoint` invocations (interprocedural work
    /// units — the same unit `MAX_INTERPROCEDURAL_ANALYSIS_WORK` caps).
    pub total_work: u64,
    /// Highest per-root work observed (max of the #8185 `analysis_work`
    /// counter across roots).
    pub peak_root_work: u64,
    /// `MAX_INTERPROCEDURAL_ANALYSIS_WORK` exhausted → widened to `Top`.
    pub work_budget_widenings: u64,
    /// `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH` reached → callee analysis was cut
    /// off (widened to `Top` on the main return-type path; declined — `None`,
    /// falling back to a wider path — on the partial-struct / named-callable /
    /// element-call paths).
    pub depth_limit_cutoffs: u64,
    /// Loop-body fixpoint loops run (`for`/`foreach`/`while` statements).
    pub loop_fixpoint_runs: u64,
    /// Total loop-body fixpoint iterations across all runs.
    pub loop_fixpoint_iterations: u64,
    /// Highest iteration count a single loop-body fixpoint used.
    pub loop_fixpoint_max_iterations: u64,
    /// Loop-body fixpoints that hit `MAX_LOOP_FIXPOINT_ITERATIONS` while the
    /// environment was still changing (non-converged → imprecise post-loop env).
    pub loop_fixpoint_exhausted: u64,
    /// Recursive return-type outer fixpoints run (Issue #3527 loop).
    pub recursive_fixpoint_runs: u64,
    /// Total recursive outer-fixpoint iterations across all runs.
    pub recursive_fixpoint_iterations: u64,
    /// Highest iteration count a single recursive outer fixpoint used.
    pub recursive_fixpoint_max_iterations: u64,
    /// Recursive outer fixpoints that hit `MAX_RECURSIVE_FIXPOINT_ITERATIONS`
    /// without converging (result marked limited-accuracy).
    pub recursive_fixpoint_exhausted: u64,
    /// Block CFG fixpoints (the `MAX_INFERENCE_ITERATIONS` loop) run to
    /// convergence or exhaustion.
    pub block_fixpoint_runs: u64,
    /// Total block CFG fixpoint iterations across all runs.
    pub block_fixpoint_iterations: u64,
    /// Block CFG fixpoints that hit `MAX_INFERENCE_ITERATIONS` without
    /// stabilizing (returned the current best guess).
    pub block_fixpoint_limit_hits: u64,
    /// Method-call union splits declined because the variant product exceeded
    /// `MAX_METHOD_UNION_SPLIT_VARIANTS` (call inferred on the joined type).
    pub union_split_bailouts: u64,
    /// Genuine lattice joins that produced `Top` from two non-`Top` inputs
    /// (`join` / `join_limited` entry points): imprecision inherent to the
    /// abstract domain, NOT budget exhaustion.
    pub lattice_join_top_widenings: u64,
}

impl InferBudgetMetrics {
    /// Sum of all budget-exhaustion-triggered widening/cutoff events (the
    /// counters a budget re-evaluation needs at zero before tightening).
    pub fn budget_exhaustion_events(&self) -> u64 {
        self.work_budget_widenings
            + self.depth_limit_cutoffs
            + self.loop_fixpoint_exhausted
            + self.recursive_fixpoint_exhausted
            + self.block_fixpoint_limit_hits
            + self.union_split_bailouts
    }
}

impl std::fmt::Display for InferBudgetMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "inference budget metrics (Issue #8546):")?;
        writeln!(f, "  roots:                        {}", self.roots)?;
        writeln!(f, "  total_work:                   {}", self.total_work)?;
        writeln!(f, "  peak_root_work:               {}", self.peak_root_work)?;
        writeln!(f, "  -- budget-exhaustion widenings/cutoffs --")?;
        writeln!(
            f,
            "  work_budget_widenings:        {}",
            self.work_budget_widenings
        )?;
        writeln!(
            f,
            "  depth_limit_cutoffs:          {}",
            self.depth_limit_cutoffs
        )?;
        writeln!(
            f,
            "  loop_fixpoint_exhausted:      {}",
            self.loop_fixpoint_exhausted
        )?;
        writeln!(
            f,
            "  recursive_fixpoint_exhausted: {}",
            self.recursive_fixpoint_exhausted
        )?;
        writeln!(
            f,
            "  block_fixpoint_limit_hits:    {}",
            self.block_fixpoint_limit_hits
        )?;
        writeln!(
            f,
            "  union_split_bailouts:         {}",
            self.union_split_bailouts
        )?;
        writeln!(f, "  -- genuine lattice imprecision --")?;
        writeln!(
            f,
            "  lattice_join_top_widenings:   {}",
            self.lattice_join_top_widenings
        )?;
        writeln!(f, "  -- fixpoint iteration usage --")?;
        writeln!(
            f,
            "  loop_fixpoint: runs {} / iterations {} / max {}",
            self.loop_fixpoint_runs,
            self.loop_fixpoint_iterations,
            self.loop_fixpoint_max_iterations
        )?;
        writeln!(
            f,
            "  recursive_fixpoint: runs {} / iterations {} / max {}",
            self.recursive_fixpoint_runs,
            self.recursive_fixpoint_iterations,
            self.recursive_fixpoint_max_iterations
        )?;
        write!(
            f,
            "  block_fixpoint: runs {} / iterations {}",
            self.block_fixpoint_runs, self.block_fixpoint_iterations
        )
    }
}

thread_local! {
    static METRICS: Cell<InferBudgetMetrics> = Cell::new(InferBudgetMetrics::default());
}

#[inline]
fn bump(update: impl FnOnce(&mut InferBudgetMetrics)) {
    METRICS.with(|m| {
        let mut v = m.get();
        update(&mut v);
        m.set(v);
    });
}

/// Reset the counters on this thread; call before a measured compilation.
pub fn clear() {
    METRICS.with(|m| m.set(InferBudgetMetrics::default()));
}

/// Snapshot of the counters recorded on this thread so far.
pub fn snapshot() -> InferBudgetMetrics {
    METRICS.with(Cell::get)
}

/// Print the counters to stderr when collection is enabled (used by the
/// `sjulia` CLI so `SJULIA_INFER_BUDGET_METRICS=1 sjulia file.jl` doubles as
/// a measurement harness).
///
/// This function's whole purpose is opt-in diagnostics output, mirroring
/// `compile/profile.rs`, so the crate-wide `#![deny(clippy::print_stderr)]`
/// (lib.rs, Issue #2888) does not apply here.
#[allow(clippy::print_stderr)]
pub fn report_to_stderr_if_enabled() {
    if enabled() {
        eprintln!("{}", snapshot());
    }
}

/// A root inference started (`analysis_depth == 0` work-counter reset).
#[inline]
pub(crate) fn record_root() {
    if !enabled() {
        return;
    }
    bump(|m| m.roots += 1);
}

/// One interprocedural work unit consumed; `per_root_work` is the #8185
/// per-root counter value after the bump.
#[inline]
pub(crate) fn record_work(per_root_work: usize) {
    if !enabled() {
        return;
    }
    bump(|m| {
        m.total_work += 1;
        m.peak_root_work = m.peak_root_work.max(per_root_work as u64);
    });
}

/// `MAX_INTERPROCEDURAL_ANALYSIS_WORK` exhausted → widened to `Top`.
#[inline]
pub(crate) fn record_work_budget_widening() {
    if !enabled() {
        return;
    }
    bump(|m| m.work_budget_widenings += 1);
}

/// `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH` cut a callee analysis off.
#[inline]
pub(crate) fn record_depth_limit_cutoff() {
    if !enabled() {
        return;
    }
    bump(|m| m.depth_limit_cutoffs += 1);
}

/// A loop-body fixpoint finished; `exhausted` when it stopped at
/// `MAX_LOOP_FIXPOINT_ITERATIONS` while the environment was still changing.
#[inline]
pub(crate) fn record_loop_fixpoint(iterations: u64, exhausted: bool) {
    if !enabled() {
        return;
    }
    bump(|m| {
        m.loop_fixpoint_runs += 1;
        m.loop_fixpoint_iterations += iterations;
        m.loop_fixpoint_max_iterations = m.loop_fixpoint_max_iterations.max(iterations);
        if exhausted {
            m.loop_fixpoint_exhausted += 1;
        }
    });
}

/// A recursive return-type outer fixpoint finished; `exhausted` when it hit
/// `MAX_RECURSIVE_FIXPOINT_ITERATIONS` without converging.
#[inline]
pub(crate) fn record_recursive_fixpoint(iterations: u64, exhausted: bool) {
    if !enabled() {
        return;
    }
    bump(|m| {
        m.recursive_fixpoint_runs += 1;
        m.recursive_fixpoint_iterations += iterations;
        m.recursive_fixpoint_max_iterations = m.recursive_fixpoint_max_iterations.max(iterations);
        if exhausted {
            m.recursive_fixpoint_exhausted += 1;
        }
    });
}

/// A block CFG fixpoint (`MAX_INFERENCE_ITERATIONS` loop) finished;
/// `exhausted` when it returned a best guess at the iteration cap.
#[inline]
pub(crate) fn record_block_fixpoint(iterations: u64, exhausted: bool) {
    if !enabled() {
        return;
    }
    bump(|m| {
        m.block_fixpoint_runs += 1;
        m.block_fixpoint_iterations += iterations;
        if exhausted {
            m.block_fixpoint_limit_hits += 1;
        }
    });
}

/// A method-call union split was declined (`MAX_METHOD_UNION_SPLIT_VARIANTS`).
#[inline]
pub(crate) fn record_union_split_bailout() {
    if !enabled() {
        return;
    }
    bump(|m| m.union_split_bailouts += 1);
}

/// A lattice `join`/`join_limited` produced `Top` from two non-`Top` inputs —
/// genuine lattice imprecision, not budget exhaustion.
#[inline]
pub(crate) fn record_join_top_widening() {
    if !enabled() {
        return;
    }
    bump(|m| m.lattice_join_top_widenings += 1);
}
