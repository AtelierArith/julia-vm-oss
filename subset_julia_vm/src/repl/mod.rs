//! REPL session management for persistent variable storage across evaluations.
//!
//! This module provides a REPLSession that maintains state between evaluations,
//! allowing variables defined in one evaluation to be used in subsequent ones.

pub mod completions;
mod converters;
mod globals;
mod session;

pub use globals::{REPLGlobals, REPLResult};
pub use session::REPLSession;

#[cfg(test)]
mod tests;
