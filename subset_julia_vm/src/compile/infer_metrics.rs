//! Inference and widening metrics for threshold tuning (Issue #5096).
//!
//! Metrics are compiled in only for the existing `profiling` feature. The
//! default build keeps inference hot paths as no-op calls, matching the VM
//! profiler's zero-overhead contract.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InferenceMetrics {
    /// `limit_type_size` invocations.
    pub limit_type_size_calls: u64,
    /// Union length budget exceeded and fell back to `widen_union`.
    pub union_length_widenings: u64,
    /// Structural depth exceeded `MAX_UNION_COMPLEXITY`.
    pub union_complexity_widenings: u64,
    /// Comparison-aware wrapper widening fired for recursive growth.
    pub comparison_wrapper_widenings: u64,
    /// Block fixpoint hit `MAX_INFERENCE_ITERATIONS`.
    pub inference_iteration_limit_hits: u64,
    /// Recursive return fixpoint hit `MAX_RECURSIVE_FIXPOINT_ITERATIONS`.
    pub recursive_fixpoint_limit_hits: u64,
    /// Worklist transfer hit its defensive step budget.
    pub worklist_step_limit_hits: u64,
}

#[cfg(feature = "profiling")]
thread_local! {
    static METRICS: std::cell::RefCell<InferenceMetrics> =
        std::cell::RefCell::new(InferenceMetrics::default());
}

#[cfg(feature = "profiling")]
fn bump(update: impl FnOnce(&mut InferenceMetrics)) {
    METRICS.with(|metrics| update(&mut metrics.borrow_mut()));
}

#[cfg(feature = "profiling")]
pub fn clear() {
    METRICS.with(|metrics| *metrics.borrow_mut() = InferenceMetrics::default());
}

#[cfg(not(feature = "profiling"))]
pub fn clear() {}

#[cfg(feature = "profiling")]
pub fn snapshot() -> InferenceMetrics {
    METRICS.with(|metrics| *metrics.borrow())
}

#[cfg(not(feature = "profiling"))]
pub fn snapshot() -> InferenceMetrics {
    InferenceMetrics::default()
}

#[cfg(feature = "profiling")]
pub(crate) fn record_limit_type_size_call() {
    bump(|m| m.limit_type_size_calls += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_limit_type_size_call() {}

#[cfg(feature = "profiling")]
pub(crate) fn record_union_length_widening() {
    bump(|m| m.union_length_widenings += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_union_length_widening() {}

#[cfg(feature = "profiling")]
pub(crate) fn record_union_complexity_widening() {
    bump(|m| m.union_complexity_widenings += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_union_complexity_widening() {}

#[cfg(feature = "profiling")]
pub(crate) fn record_comparison_wrapper_widening() {
    bump(|m| m.comparison_wrapper_widenings += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_comparison_wrapper_widening() {}

#[cfg(feature = "profiling")]
pub(crate) fn record_inference_iteration_limit_hit() {
    bump(|m| m.inference_iteration_limit_hits += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_inference_iteration_limit_hit() {}

#[cfg(feature = "profiling")]
pub(crate) fn record_recursive_fixpoint_limit_hit() {
    bump(|m| m.recursive_fixpoint_limit_hits += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_recursive_fixpoint_limit_hit() {}

#[cfg(feature = "profiling")]
pub(crate) fn record_worklist_step_limit_hit() {
    bump(|m| m.worklist_step_limit_hits += 1);
}

#[cfg(not(feature = "profiling"))]
pub(crate) fn record_worklist_step_limit_hit() {}

#[cfg(all(test, feature = "profiling"))]
mod tests {
    #[test]
    fn inference_metrics_record_when_profiling_enabled_issue_5096() {
        super::clear();
        super::record_limit_type_size_call();
        super::record_union_length_widening();
        super::record_union_complexity_widening();
        super::record_comparison_wrapper_widening();
        super::record_inference_iteration_limit_hit();
        super::record_recursive_fixpoint_limit_hit();
        super::record_worklist_step_limit_hit();

        let metrics = super::snapshot();
        assert_eq!(metrics.limit_type_size_calls, 1);
        assert_eq!(metrics.union_length_widenings, 1);
        assert_eq!(metrics.union_complexity_widenings, 1);
        assert_eq!(metrics.comparison_wrapper_widenings, 1);
        assert_eq!(metrics.inference_iteration_limit_hits, 1);
        assert_eq!(metrics.recursive_fixpoint_limit_hits, 1);
        assert_eq!(metrics.worklist_step_limit_hits, 1);
        super::clear();
    }
}

#[cfg(all(test, not(feature = "profiling")))]
mod tests_no_profiling {
    #[test]
    fn inference_metrics_are_noop_without_profiling_issue_5096() {
        super::clear();
        super::record_limit_type_size_call();
        super::record_union_length_widening();
        super::record_union_complexity_widening();
        super::record_comparison_wrapper_widening();
        super::record_inference_iteration_limit_hit();
        super::record_recursive_fixpoint_limit_hit();
        super::record_worklist_step_limit_hit();

        assert_eq!(super::snapshot(), super::InferenceMetrics::default());
    }
}
