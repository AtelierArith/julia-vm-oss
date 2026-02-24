//! AoT IR types and operations.
//!
//! # Module Organization
//!
//! - `basic_types.rs`: Low-level SSA IR types (BasicBlock, Instruction, VarRef, etc.)
//! - `aot_types.rs`: High-level AoT IR types (AotProgram, AotFunction, AotStmt, etc.)
//! - `ops.rs`: Operator types (AotBinOp, AotUnaryOp, AotBuiltinOp) + Display/From
//! - `tests.rs`: Tests

mod aot_types;
mod basic_types;
mod ops;
#[cfg(test)]
mod tests;

// Re-export all public types
pub use aot_types::{
    AotEnum, AotExpr, AotFunction, AotGlobal, AotProgram, AotStmt, AotStruct, DynamicOpDiagnostic,
};
pub use basic_types::{
    BasicBlock, BinOpKind, ConstValue, Instruction, IrFunction, IrModule, Terminator, UnaryOpKind,
    VarRef,
};
pub use ops::{AotBinOp, AotBuiltinOp, AotUnaryOp, CompoundAssignOp};
