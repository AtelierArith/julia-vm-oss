//! A/B measurement gate for the #9125/#9154 `Complex{Float64}` Rust fast paths
//! (Issue #9198, slice S6).
//!
//! The #9125 binary fast path (`vm/exec/binary_both.rs::try_complex_f64_binary_op`)
//! and the #9154/#9155 pow fast path
//! (`vm/dynamic_ops::try_complex_f64_int_pow`) were the transitional stopgap for
//! the per-operation boxing cost of a `Complex{Float64}` `StructInstance`. With
//! the S2/S3 slot-pair SROA (typed loops never construct the boxed struct) and
//! the S4/S5 contiguous isbits array storage in, those fast paths are only
//! reachable on the **residual dynamic-dispatch** Complex route (non-SROA'd
//! locals, array reductions/`sum`, materialized `z^n`). This gate lets one build
//! measure that residual route with the fast paths **bypassed** (falling through
//! to the general Julia method resolver), compared against the fast-path arm,
//! so the retirement decision (acceptance criterion 4) rests on real A/B numbers
//! rather than intuition — the Performance Decision Protocol (CHECKLISTS.md).
//!
//! Default `false` = fast paths **active** (the shipping behaviour). Only the
//! `vm_complex_dynamic_9198` bench flips it. It is a measurement knob, not a
//! runtime feature; the S6 retirement removes the fast-path bodies once the
//! numbers support it.

use std::sync::atomic::{AtomicBool, Ordering};

/// When `true`, the #9125/#9154 Complex fast-path call sites are skipped and the
/// operands fall through to the general dispatch/SROA path. Process-wide, mirrors
/// the `set_register_vm_forced` (#8559) override pattern so the bench can toggle
/// it without environment plumbing.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Disable/enable the Complex fast paths for the A/B measurement (Issue #9198 S6).
pub fn set_complex_fastpath_disabled(disabled: bool) {
    DISABLED.store(disabled, Ordering::Relaxed);
}

/// True when the Complex fast paths should be bypassed (measurement arm B).
#[inline]
pub(crate) fn complex_fastpath_disabled() -> bool {
    DISABLED.load(Ordering::Relaxed)
}
