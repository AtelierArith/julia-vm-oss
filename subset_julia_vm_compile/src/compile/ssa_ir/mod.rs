//! SSA IR for the compiler middle end (Issues #8440, #8550).
//!
//! This module hosts three layers:
//!
//! * The **durable SSA model** (`model`, `build`, `verify`, `dom`): real
//!   [`SsaFunction`] / [`SsaBlock`] / [`SsaValue`] / [`PhiNode`] structures and
//!   a Core IR → SSA conversion for structured control flow.
//! * The **optimization passes** (`opt`, Issue #8551): constant
//!   folding/propagation, unreachable-block and dead-definition elimination,
//!   and dominator-scoped pure-call CSE, gated by the body-derived effect
//!   summaries of `compile::effects` (Issue #8441).
//! * The **stack-bytecode lowering** (`lower`, Issue #8552): the SSA pipeline
//!   is ON by default; eligible user function bodies go Core IR → SSA build →
//!   opt passes → lowering, falling back to the legacy
//!   `CoreCompiler::compile_function_body` per function. Set
//!   `SJULIA_SSA_PIPELINE=0` to force the legacy path.
//!
//! The temporary bridge (`bridge`) that forwarded the Phi fold to `ir_opt` has
//! been retired as part of Issue #8832 (default flip).
//!
//! Design notes and limitations are documented in `docs/vm/SSA_IR.md`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod build;
mod dom;
mod lower;
mod model;
mod opt;
pub mod plan;
mod scan;
mod verify;

pub use build::{build_function, SsaBuildError};
pub(super) use lower::{lower_function_body_via_ssa, ssa_pipeline_enabled};
pub use lower::{take_ssa_pipeline_stats, SsaPipelineStats};
pub use model::{
    BlockId, PhiNode, SsaBlock, SsaFunction, SsaOp, SsaParam, SsaStatement, SsaValue, SsaValueId,
    Terminator,
};
pub use opt::{
    cse_pure_calls, cse_pure_calls_scoped, eliminate_dead_defs, eliminate_dead_defs_scoped,
    eliminate_unreachable_blocks, fold_constants, optimize, optimize_scoped, optimize_with_effects,
};
pub use verify::verify;

#[cfg(test)]
mod test_util;
#[cfg(test)]
mod tests;
