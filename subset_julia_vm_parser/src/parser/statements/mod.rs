//! Statement and definition parsers
//!
//! Handles parsing of:
//! - Function and macro definitions
//! - Type definitions (struct, abstract, primitive, module)
//! - Control flow statements (if, for, while, try)
//! - Variable declarations (const, global, local)
//! - Import/export statements

mod control_flow;
mod declarations;
mod definitions;
mod imports;
mod jumps;
mod types;
