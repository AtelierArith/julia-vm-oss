//! AoT IR types and operations.
//!
//! # Module Organization
//!
//! - `basic_types.rs`: Low-level SSA IR types (BasicBlock, Instruction, VarRef, etc.)
//! - `aot_types.rs`: High-level AoT IR types (AotProgram, AotFunction, AotStmt, etc.)
//! - `ops.rs`: Operator types (AotBinOp, AotUnaryOp, AotBuiltinOp) + Display/From
//! - `tests.rs`: Tests

mod aggregate_types;
mod aot_types;
mod basic_types;
mod ops;
#[cfg(test)]
mod tests;
mod values;

// Re-export all public types
pub use aggregate_types::{AggregateField, AggregateLayout};
pub use aot_types::{
    AotEnum, AotExpr, AotFunction, AotGlobal, AotInlinePolicy, AotProgram, AotStmt, AotStruct,
    DynamicOpDiagnostic,
};
pub use basic_types::{
    ArrayInit, ArraySelector, BasicBlock, BinOpKind, Instruction, IrFunction, IrModule,
    StructFieldInit, Terminator, UnaryOpKind, VarRef,
};
pub use ops::{AotBinOp, AotBuiltinOp, AotUnaryOp, CompoundAssignOp};
pub use values::ConstValue;
