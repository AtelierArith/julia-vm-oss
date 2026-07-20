//! A/B measurement gate for the native generator-consumer fast paths
//! (Issue #9200, slice S6).
//!
//! The S5 survey (PR #9465) found every native `Value::Generator` consumer
//! load-bearing and deferred the retirement decision to this slice. The single
//! genuine *perf* fast path among them is the `collect(::Base.Generator)` route
//! (`call_dynamic.rs` interception → `collect_generator` in
//! `type_ops/iteration.rs`), which materializes the base iterator once and
//! drives the map/filter step through the HOF broadcast machinery
//! (`start_hof_call_values_with_array_result` /
//! `start_hof_filter_map_values_with_array_result`) in a single Rust loop.
//!
//! The upstream iterate-only ideal (`julia/base/generator.jl`) would instead
//! collect a `Base.Generator` purely through `iterate(g::Generator)` — one
//! interpreter re-entry per element via `collect_iterator_via_iterate_protocol`.
//! This gate lets one build measure that pure-iterate arm, compared against
//! the shipping fast-path arm, with output parity asserted, so the S6
//! retire-or-keep decision rests on real A/B numbers rather than intuition —
//! the Performance Decision Protocol (CHECKLISTS.md).
//!
//! Default `false` = fast path **active** (the shipping behaviour). Only the
//! `vm_generator_representation_9200` bench flips it. It is a measurement knob,
//! not a runtime feature; mirrors the `complex_fastpath_gate` pattern
//! (Issue #9198 S6).

use std::sync::atomic::{AtomicBool, Ordering};

/// When `true`, the `collect(::Generator)` / `collect_similar(_, ::Generator)`
/// fast-path interceptions are skipped and the generator is collected purely
/// through its `iterate` protocol (`collect_iterator_via_iterate_protocol`).
/// Process-wide, mirrors the `set_complex_fastpath_disabled` (#9198 S6) and
/// `set_register_vm_forced` (#8559) override pattern so the bench can toggle it
/// without environment plumbing.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Disable/enable the generator collect fast path for the A/B measurement
/// (Issue #9200 S6).
pub fn set_generator_fastpath_disabled(disabled: bool) {
    DISABLED.store(disabled, Ordering::Relaxed);
}

/// True when the generator collect fast path should be bypassed (measurement
/// arm B — the upstream iterate-only route).
#[inline]
pub(crate) fn generator_fastpath_disabled() -> bool {
    DISABLED.load(Ordering::Relaxed)
}
