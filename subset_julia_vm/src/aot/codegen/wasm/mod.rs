//! Standalone WebAssembly code generation over the backend-neutral AoT IR.

mod emit;
mod lower;
mod types;

use crate::aot::ir::{AotProgram, IrModule};
use crate::aot::AotResult;

pub use emit::emit_module;
pub use types::{ABI_VERSION as WASM_ABI_VERSION, ELEMENT_TAG_TABLE};

pub(crate) fn lower_program(program: &AotProgram) -> AotResult<IrModule> {
    lower::lower_program(program)
}
