//! Handler-function-pointer instruction dispatch experiment (Issue #8562).
//!
//! Follow-up to the #8446 Phase 1 plan ("dispatch-table / function-pointer
//! interpreter"): instead of dispatching every `Instr` through the exhaustive
//! `match` in `exec/mod.rs` (`dispatch_instr_match!`), this module routes each
//! instruction through a dense table of uniform function pointers
//! `fn(&mut Vm<R>, &Instr) -> Result<DispatchAction, VmError>` indexed by a
//! generated discriminant-to-row mapping. Rust has no computed goto and
//! `std::mem::discriminant` cannot index an array, so the mapping is a
//! macro-generated `match` over the hot instruction subset that returns the
//! row index; LLVM lowers that constant-returning match to a jump-table /
//! lookup on the enum discriminant, after which dispatch is one indirect call.
//!
//! # Gate (experiment only — never on by default)
//!
//! Compiled only under the `vm-handler-table` cargo feature; default builds
//! contain none of this code and the `match` loop is byte-for-byte unchanged.
//! Within a feature build, the table is armed per `Vm` at construction by
//! `SJULIA_HANDLER_TABLE=1` or the [`set_handler_table_forced`] process
//! override (for wasm32, which has no environment, and the iOS Simulator
//! harness), mirroring the `SJULIA_REGISTER_VM` / `SJULIA_STACK_VM_METRICS`
//! gate contract from Issues #8558/#8559.
//!
//! # Semantics cannot diverge
//!
//! Hot table rows call the *same* `execute_*` group handlers the match arms
//! call (including the `handle_pending_call_depth_overflow` postlude on the
//! call/return rows), and every instruction outside the hot subset lands on
//! the shared fallback row, which re-enters the full
//! `dispatch_instr_match_path` match. A mis-routed hot row would hit the
//! group handler's internal `_ => Err(unhandled)` arm instead of silently
//! doing the wrong thing.
//!
//! # What is deliberately paid on the gated path
//!
//! - the indirect call itself (the point of the experiment: fn-pointer calls
//!   defeat the inlining `match` arms get);
//! - one bounds check on the table index (`HANDLER_TABLE[index]`; a real
//!   byte-coded interpreter with a 256-row table would not have one);
//! - two branchless counter adds (`table_hits` / `fallback_dispatches`),
//!   the same always-counted contract as the register VM's `dispatch_total`
//!   (#8559), so the deterministic coverage counters need no separate
//!   instrumented binary.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::sync::atomic::{AtomicBool, Ordering};

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::instr::Instr;
use crate::vm::Vm;

use super::DispatchAction;

const HANDLER_TABLE_ENV: &str = "SJULIA_HANDLER_TABLE";

/// Process-wide gate override for hosts where environment variables are not
/// practical (wasm32 has no real environment; the iOS Simulator harness
/// prefers an API call over `simctl` env plumbing). OR-ed with
/// `SJULIA_HANDLER_TABLE` at `Vm` construction time (Issue #8562).
static FORCED: AtomicBool = AtomicBool::new(false);

/// Force handler-table dispatch on/off for subsequently constructed `Vm`s,
/// regardless of the `SJULIA_HANDLER_TABLE` environment variable.
pub fn set_handler_table_forced(enabled: bool) {
    FORCED.store(enabled, Ordering::Relaxed);
}

/// Per-`Vm` state for the handler-table gate: presence enables the table
/// path; the counters are the experiment's deterministic coverage metrics.
pub(crate) struct HandlerTableState {
    /// Dispatches served by a hot table row.
    pub(crate) table_hits: u64,
    /// Dispatches that landed on the fallback row (full `match` re-entry).
    pub(crate) fallback_dispatches: u64,
}

impl HandlerTableState {
    /// Read the gate at `Vm` construction: `None` (table path never taken;
    /// one `is_some` check per dispatch) unless `SJULIA_HANDLER_TABLE` is
    /// `1`/`true` or [`set_handler_table_forced`] is active.
    pub(crate) fn from_env() -> Option<Box<Self>> {
        let enabled = FORCED.load(Ordering::Relaxed)
            || std::env::var(HANDLER_TABLE_ENV)
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        enabled.then(|| {
            Box::new(Self {
                table_hits: 0,
                fallback_dispatches: 0,
            })
        })
    }
}

/// Uniform handler signature: same inputs/outputs as one `dispatch_instr`
/// match arm, so the table can replace the match one-for-one.
type Handler<R> = fn(&mut Vm<R>, &Instr) -> Result<DispatchAction, VmError>;

// === Group handlers ===
//
// Each hot row delegates to the exact `execute_*` group method its `match`
// arm calls in `dispatch_instr_match!`; the call/return rows reproduce the
// arm's `handle_pending_call_depth_overflow` postlude. Sharing the inner
// functions is what keeps the two dispatch paths semantically identical.

fn h_stack<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.execute_stack(instr)
}

fn h_locals<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.execute_locals(instr)
}

fn h_arithmetic<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.execute_arithmetic(instr)
}

fn h_comparison<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.execute_comparison(instr)
}

fn h_jump<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.execute_jump(instr)
}

fn h_conversion<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.execute_conversion(instr)
}

fn h_call<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    let action = vm.execute_call(instr)?;
    vm.handle_pending_call_depth_overflow()?;
    Ok(action)
}

fn h_call_dynamic<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    let action = vm.execute_call_dynamic(instr)?;
    vm.handle_pending_call_depth_overflow()?;
    Ok(action)
}

fn h_return<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    let action = vm.execute_return(instr)?;
    vm.handle_pending_call_depth_overflow()?;
    Ok(action)
}

/// Shared fallback row: every instruction outside the hot subset re-enters
/// the full exhaustive `match`, so coverage gaps change performance only,
/// never semantics.
fn h_fallback<R: RngLike>(vm: &mut Vm<R>, instr: &Instr) -> Result<DispatchAction, VmError> {
    vm.dispatch_instr_match_path(instr)
}

/// Generates, from a single hot-instruction list, (a) the dense row-index
/// enum, (b) the discriminant-to-row `match`, and (c) the handler table in
/// the same order — the three cannot drift apart because they share the list.
macro_rules! handler_table {
    ($(($name:ident, $pat:pat, $handler:ident)),+ $(,)?) => {
        /// Dense table rows for the hot subset; `Fallback` is the last row.
        #[derive(Clone, Copy)]
        enum HotRow {
            $($name,)+
            Fallback,
        }

        /// Table length: hot rows plus the shared fallback row.
        const HANDLER_TABLE_LEN: usize = HotRow::Fallback as usize + 1;
        const FALLBACK_INDEX: usize = HotRow::Fallback as usize;

        /// Map an instruction to its table row (`FALLBACK_INDEX` for every
        /// variant outside the hot subset).
        #[inline(always)]
        fn table_row(instr: &Instr) -> usize {
            match instr {
                $($pat => HotRow::$name as usize,)+
                _ => FALLBACK_INDEX,
            }
        }

        /// Build the per-monomorphization handler table; row order matches
        /// `HotRow` because both expand from the same list.
        const fn build_table<R: RngLike>() -> [Handler<R>; HANDLER_TABLE_LEN] {
            [$($handler::<R>,)+ h_fallback::<R>]
        }
    };
}

// Hot subset: the numeric/loop/call instructions the Issue #8559 benchmark
// set (fib / calc_pi / lorenz) exercises — constants, the slot load/store
// families (incl. the fused load-arith forms), I64/F64 arithmetic and
// comparisons, the fused jump family, and the direct-call/return groups.
handler_table! {
    // constants / operand stack (execute_stack group)
    (PushI64, Instr::PushI64(..), h_stack),
    (PushF64, Instr::PushF64(..), h_stack),
    (PushBool, Instr::PushBool(..), h_stack),
    (PushNothing, Instr::PushNothing, h_stack),
    (Pop, Instr::Pop, h_stack),
    // locals: slot load/store families + fused load-arith forms
    (LoadSlot, Instr::LoadSlot(..), h_locals),
    (StoreSlot, Instr::StoreSlot(..), h_locals),
    (LoadSlotI64, Instr::LoadSlotI64(..), h_locals),
    (LoadSlotI64ToF64, Instr::LoadSlotI64ToF64(..), h_locals),
    (StoreSlotI64, Instr::StoreSlotI64(..), h_locals),
    (LoadSlotF64, Instr::LoadSlotF64(..), h_locals),
    (StoreSlotF64, Instr::StoreSlotF64(..), h_locals),
    (LoadSlotBool, Instr::LoadSlotBool(..), h_locals),
    (StoreSlotBool, Instr::StoreSlotBool(..), h_locals),
    (LoadSquareF64Slot, Instr::LoadSquareF64Slot(..), h_locals),
    (LoadAddF64Slot, Instr::LoadAddF64Slot(..), h_locals),
    (AddF64Slots, Instr::AddF64Slots(..), h_locals),
    (AddF64I64Slots, Instr::AddF64I64Slots(..), h_locals),
    (LoadSubF64Slot, Instr::LoadSubF64Slot(..), h_locals),
    (LoadMulF64Slot, Instr::LoadMulF64Slot(..), h_locals),
    (LoadDivF64Slot, Instr::LoadDivF64Slot(..), h_locals),
    (LoadAddI64, Instr::LoadAddI64(..), h_locals),
    (LoadAddI64Slot, Instr::LoadAddI64Slot(..), h_locals),
    (LoadAddConstI64Slot, Instr::LoadAddConstI64Slot(..), h_locals),
    (LoadSubI64, Instr::LoadSubI64(..), h_locals),
    (LoadSubI64Slot, Instr::LoadSubI64Slot(..), h_locals),
    (LoadMulI64, Instr::LoadMulI64(..), h_locals),
    (LoadMulI64Slot, Instr::LoadMulI64Slot(..), h_locals),
    (LoadModI64, Instr::LoadModI64(..), h_locals),
    (LoadModI64Slot, Instr::LoadModI64Slot(..), h_locals),
    (IncVarI64, Instr::IncVarI64(..), h_locals),
    (IncVarI64Slot, Instr::IncVarI64Slot(..), h_locals),
    (AddConstI64Slot, Instr::AddConstI64Slot(..), h_locals),
    (DecVarI64, Instr::DecVarI64(..), h_locals),
    (DecVarI64Slot, Instr::DecVarI64Slot(..), h_locals),
    (LoadI64, Instr::LoadI64(..), h_locals),
    (StoreI64, Instr::StoreI64(..), h_locals),
    (LoadF64, Instr::LoadF64(..), h_locals),
    (StoreF64, Instr::StoreF64(..), h_locals),
    (LoadAny, Instr::LoadAny(..), h_locals),
    (StoreAny, Instr::StoreAny(..), h_locals),
    (LoadGlobalAny, Instr::LoadGlobalAny(..), h_locals),
    (StoreGlobalAny, Instr::StoreGlobalAny(..), h_locals),
    // I64/F64 arithmetic + stack shuffles (execute_arithmetic group)
    (AddI64, Instr::AddI64, h_arithmetic),
    (SubI64, Instr::SubI64, h_arithmetic),
    (MulI64, Instr::MulI64, h_arithmetic),
    (ModI64, Instr::ModI64, h_arithmetic),
    (IncI64, Instr::IncI64, h_arithmetic),
    (NegI64, Instr::NegI64, h_arithmetic),
    (DupI64, Instr::DupI64, h_arithmetic),
    (DupF64, Instr::DupF64, h_arithmetic),
    (Dup, Instr::Dup, h_arithmetic),
    (AddF64, Instr::AddF64, h_arithmetic),
    (SubF64, Instr::SubF64, h_arithmetic),
    (MulF64, Instr::MulF64, h_arithmetic),
    (DivF64, Instr::DivF64, h_arithmetic),
    (NegF64, Instr::NegF64, h_arithmetic),
    (PowF64, Instr::PowF64, h_arithmetic),
    (SqrtF64, Instr::SqrtF64, h_arithmetic),
    (FloorF64, Instr::FloorF64, h_arithmetic),
    (CeilF64, Instr::CeilF64, h_arithmetic),
    (AbsF64, Instr::AbsF64, h_arithmetic),
    (Abs2F64, Instr::Abs2F64, h_arithmetic),
    // I64/F64 comparisons
    (GtI64, Instr::GtI64, h_comparison),
    (LtI64, Instr::LtI64, h_comparison),
    (LeI64, Instr::LeI64, h_comparison),
    (GeI64, Instr::GeI64, h_comparison),
    (EqI64, Instr::EqI64, h_comparison),
    (NeI64, Instr::NeI64, h_comparison),
    (LtF64, Instr::LtF64, h_comparison),
    (GtF64, Instr::GtF64, h_comparison),
    (LeF64, Instr::LeF64, h_comparison),
    (GeF64, Instr::GeF64, h_comparison),
    (EqF64, Instr::EqF64, h_comparison),
    (NeF64, Instr::NeF64, h_comparison),
    // typed scalar conversions (execute_conversion group; `ToF64` runs once
    // or twice per iteration in mixed Int/Float loop bodies)
    (ToF64, Instr::ToF64, h_conversion),
    (ToI64, Instr::ToI64, h_conversion),
    (BoolToI64, Instr::BoolToI64, h_conversion),
    (I64ToBool, Instr::I64ToBool, h_conversion),
    (NotBool, Instr::NotBool, h_conversion),
    // jumps, incl. the fused compare-and-branch / loop-step forms
    (Jump, Instr::Jump(..), h_jump),
    (JumpIfZero, Instr::JumpIfZero(..), h_jump),
    (JumpIfNeI64, Instr::JumpIfNeI64(..), h_jump),
    (JumpIfEqI64, Instr::JumpIfEqI64(..), h_jump),
    (JumpIfLtI64, Instr::JumpIfLtI64(..), h_jump),
    (JumpIfGtI64, Instr::JumpIfGtI64(..), h_jump),
    (JumpIfGtI64Slots, Instr::JumpIfGtI64Slots(..), h_jump),
    (
        AddConstI64SlotAndJumpIfLe,
        Instr::AddConstI64SlotAndJumpIfLe(..),
        h_jump
    ),
    (JumpIfLeI64, Instr::JumpIfLeI64(..), h_jump),
    (JumpIfGeI64, Instr::JumpIfGeI64(..), h_jump),
    (JumpIfEqF64, Instr::JumpIfEqF64(..), h_jump),
    (JumpIfNeF64, Instr::JumpIfNeF64(..), h_jump),
    (JumpIfNotLtF64, Instr::JumpIfNotLtF64(..), h_jump),
    (JumpIfNotGtF64, Instr::JumpIfNotGtF64(..), h_jump),
    (JumpIfNotLeF64, Instr::JumpIfNotLeF64(..), h_jump),
    (JumpIfNotGeF64, Instr::JumpIfNotGeF64(..), h_jump),
    // direct-call family (execute_call group; overflow postlude in h_call)
    (Call, Instr::Call(..), h_call),
    (CallInbounds, Instr::CallInbounds(..), h_call),
    (CallResolved, Instr::CallResolved(..), h_call),
    (CallResolvedI64Slots, Instr::CallResolvedI64Slots(..), h_call),
    (CallInboundsI64Slots, Instr::CallInboundsI64Slots(..), h_call),
    (CallSpecialize, Instr::CallSpecialize(..), h_call),
    (CallSpecializeInbounds, Instr::CallSpecializeInbounds(..), h_call),
    (CallSpecializeI64Slots, Instr::CallSpecializeI64Slots(..), h_call),
    (
        CallSpecializeInboundsI64Slots,
        Instr::CallSpecializeInboundsI64Slots(..),
        h_call
    ),
    (CallSpecializeF64Slots, Instr::CallSpecializeF64Slots(..), h_call),
    (
        CallSpecializeInboundsF64Slots,
        Instr::CallSpecializeInboundsF64Slots(..),
        h_call
    ),
    (CallIntrinsic, Instr::CallIntrinsic(..), h_call),
    (CallBuiltin, Instr::CallBuiltin(..), h_call),
    // dynamic binary-operator dispatch (execute_call_dynamic group): an
    // untyped accumulator (`acc = acc + f(x)` where `f` returns `Any`) puts
    // one `CallDynamicBinaryBoth` in the hottest loop shape per iteration
    (CallDynamicBinary, Instr::CallDynamicBinary(..), h_call_dynamic),
    (
        CallDynamicBinaryBoth,
        Instr::CallDynamicBinaryBoth(..),
        h_call_dynamic
    ),
    (
        CallDynamicBinaryNoFallback,
        Instr::CallDynamicBinaryNoFallback(..),
        h_call_dynamic
    ),
    // returns (overflow postlude in h_return)
    (ReturnI64, Instr::ReturnI64, h_return),
    (ReturnF64, Instr::ReturnF64, h_return),
    (ReturnAny, Instr::ReturnAny, h_return),
    (ReturnNothing, Instr::ReturnNothing, h_return),
}

impl<R: RngLike> Vm<R> {
    /// Per-monomorphization handler table (Issue #8562). `const` so every
    /// dispatch reads the same statically initialized array; no lazy init or
    /// synchronization on the hot path.
    const HANDLER_TABLE: [Handler<R>; HANDLER_TABLE_LEN] = build_table::<R>();

    /// Handler-table dispatch: compute the row, count it, make one indirect
    /// call. Only reached when `self.handler_table` is `Some` (gate armed).
    #[inline(always)]
    pub(super) fn dispatch_instr_handler_table(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        let index = table_row(instr);
        if let Some(state) = self.handler_table.as_deref_mut() {
            state.table_hits += u64::from(index != FALLBACK_INDEX);
            state.fallback_dispatches += u64::from(index == FALLBACK_INDEX);
        }
        Self::HANDLER_TABLE[index](self, instr)
    }

    /// Deterministic handler-table coverage counters
    /// `(table_hits, fallback_dispatches)`, or `None` when the gate is off
    /// for this `Vm` (Issue #8562).
    pub fn handler_table_metrics(&self) -> Option<(u64, u64)> {
        self.handler_table
            .as_deref()
            .map(|state| (state.table_hits, state.fallback_dispatches))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_environment_opcodes_stay_on_fallback_row_11569() {
        for instr in [
            Instr::EnterLexicalScope(vec!["x".to_string()]),
            Instr::LoadLexical("x".to_string()),
            Instr::StoreLexical("x".to_string()),
            Instr::IsLexicalDefined("x".to_string()),
            Instr::ExitLexicalScope,
        ] {
            assert_eq!(
                table_row(&instr),
                FALLBACK_INDEX,
                "{instr:?} must fail closed to the exhaustive stack-VM dispatcher"
            );
        }
    }
}
