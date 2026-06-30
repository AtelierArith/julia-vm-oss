//! Higher-order function (HOF) execution helpers for the VM.
//!
//! This module contains helper methods for executing higher-order functions
//! like map, filter, reduce, sum, any, all, and count.

mod dispatch;
mod helpers;
mod sprint;
mod start;
// HOF/broadcast/generator runtime state lives in `state` (moved out of
// `vm/frame.rs`, Issue #6828) so the frame module holds only call-frame/slot
// machinery. Consumers import it via `crate::vm::hof_exec::state::*`.
pub(crate) mod state;
mod value_mode;
