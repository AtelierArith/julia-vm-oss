//! REPL session management for persistent variable storage across evaluations.
//!
//! This module provides a REPLSession that maintains state between evaluations,
//! allowing variables defined in one evaluation to be used in subsequent ones.
//!
//! The production session has one persistent evaluation model. The retired
//! migration selector is intentionally not part of the public API:
//!
//! ```compile_fail
//! use subset_julia_vm::repl::EvalModel;
//! ```

pub mod completions;
mod converters;
mod globals;
mod session;

pub use globals::{REPLGlobals, REPLResult};
pub use session::REPLSession;

// Shared with `ffi_support`: bound the host result-echo JSON the same way the REPL
// bounds re-injected globals — both must avoid materializing O(frames²) animation
// values (Issue #9229 / #9218). O(budget) capped estimate, never O(data).
pub(crate) use converters::value_literal_leaf_estimate;

#[cfg(test)]
mod tests;
