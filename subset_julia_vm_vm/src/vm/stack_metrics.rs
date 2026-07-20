//! Opt-in stack VM execution metrics for the Issue #8559 measurement matrix.
//!
//! The register VM prototype (Issue #8558) reports its dynamic dispatch count
//! per run; this module adds the stack-VM counterpart so the two engines can
//! be compared on equal terms: interpreter dispatches, executable-block
//! bypass runs, and the dynamic frame-memory signals (operand-stack
//! high-water mark, call-frame high-water mark).
//!
//! # Zero default overhead
//!
//! Collection is off by default. `Vm` construction reads
//! `SJULIA_STACK_VM_METRICS=1` (or the [`set_stack_vm_metrics_forced`]
//! process override, for targets without an environment such as
//! wasm32-unknown-unknown) into an `Option<Box<StackVmMetrics>>`; the
//! dispatch loop pays a single null check per instruction when disabled,
//! mirroring the `SJULIA_REGISTER_VM` gate's one-`Option`-check contract.
//! Wall-time benchmarks must run with the gate off (unset) so the recording
//! branch stays never-taken.

use std::sync::atomic::{AtomicBool, Ordering};

/// Process-wide override for hosts where environment variables are not
/// practical (wasm32 has no real environment; iOS harnesses may prefer an
/// API call over `simctl` env plumbing). OR-ed with the environment gate at
/// `Vm` construction time.
static FORCED: AtomicBool = AtomicBool::new(false);

/// Force stack VM metrics collection on/off for subsequently constructed
/// `Vm`s, regardless of `SJULIA_STACK_VM_METRICS` (Issue #8559).
pub fn set_stack_vm_metrics_forced(enabled: bool) {
    FORCED.store(enabled, Ordering::Relaxed);
}

/// `size_of` of the stack VM's per-call `Frame` struct, for the Issue #8559
/// per-frame memory comparison (`Frame` itself is crate-private).
pub fn frame_struct_size_bytes() -> usize {
    std::mem::size_of::<super::frame::Frame>()
}

/// Dynamic stack VM execution counters for one `Vm` (Issue #8559).
///
/// All counters are deterministic for a deterministic program: they count
/// instruction dispatches and observed data-structure sizes, not time, so
/// they are load-independent and comparable across hosts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StackVmMetrics {
    /// Interpreter loop dispatches (`dispatch_instr` invocations) across
    /// `run` and every nested `run_until_frame_return` loop.
    pub dispatches: u64,
    /// Executable-block fast-path executions (`try_execute_executable_block`
    /// hits). Each run replaces many per-instruction dispatches with one
    /// native block, so it is counted separately, not folded into
    /// `dispatches`.
    pub executable_block_runs: u64,
    /// Highest operand-stack depth observed between instruction dispatches.
    /// The operand stack is shared by all live frames; compare with the
    /// register VM's per-call `frame_registers` allocation.
    pub operand_stack_high_water: usize,
    /// Highest call-frame count observed between instruction dispatches.
    pub frames_high_water: usize,
    /// Call-site inline-cache hits at dynamic dispatch sites (Issue #8561):
    /// executions of a `CallDynamic`/`CallTypedDispatch`-family (or
    /// `IterateDynamic`/`CallDynamicBinary`) instruction that reused a
    /// per-call-site cached method target instead of running the resolver.
    pub dispatch_inline_cache_hits: u64,
    /// Call-site inline-cache misses (Issue #8561): cache-eligible dynamic
    /// dispatch executions that fell through to the full resolver (and then
    /// filled the call site's cache slot). Executions whose argument types
    /// are excluded from inline caching (structs, `Type{T}`, containers, …)
    /// are counted in neither field.
    pub dispatch_inline_cache_misses: u64,
}

impl StackVmMetrics {
    /// Read the metrics gate at `Vm` construction: `None` (default, zero
    /// collection) unless `SJULIA_STACK_VM_METRICS` is `1`/`true` or the
    /// process override is set.
    pub(crate) fn from_env() -> Option<Box<Self>> {
        let enabled = FORCED.load(Ordering::Relaxed)
            || std::env::var("SJULIA_STACK_VM_METRICS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        enabled.then(|| Box::new(Self::default()))
    }

    /// Whether any activity was recorded (used by tests to assert the
    /// default-off contract).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Record one interpreter dispatch and sample the dynamic high-water
    /// marks. Called between instructions, so intra-instruction transients
    /// (e.g. the two popped operands of a binary op) are not observed; the
    /// between-instruction depth is the value comparable to the register
    /// VM's statically computed register count.
    #[inline]
    pub(crate) fn record_dispatch(&mut self, operand_stack_len: usize, frames_len: usize) {
        self.dispatches += 1;
        if operand_stack_len > self.operand_stack_high_water {
            self.operand_stack_high_water = operand_stack_len;
        }
        if frames_len > self.frames_high_water {
            self.frames_high_water = frames_len;
        }
    }
}

impl<R: crate::rng::RngLike> super::Vm<R> {
    /// Snapshot of the stack VM execution counters recorded so far, or `None`
    /// when metrics collection is disabled for this `Vm` (Issue #8559).
    pub fn stack_vm_metrics(&self) -> Option<StackVmMetrics> {
        self.stack_metrics.as_deref().copied()
    }
}
