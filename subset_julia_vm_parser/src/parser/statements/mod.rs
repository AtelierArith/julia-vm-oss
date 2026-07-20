//! Statement and definition parsers
//!
//! Handles parsing of:
//! - Function and macro definitions
//! - Type definitions (struct, abstract, primitive, module)
//! - Control flow statements (if, for, while, try)
//! - Variable declarations (const, global, local)
//! - Import/export statements

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod control_flow;
mod declarations;
mod definitions;
mod imports;
mod jumps;
mod types;
