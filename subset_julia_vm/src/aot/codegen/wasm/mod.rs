//! Standalone WebAssembly code generation over the backend-neutral AoT IR.

mod emit;
mod layout;
mod lower;
mod types;

use crate::aot::ir::{AotProgram, IrModule};
use crate::aot::{AotResult, WasmImport};

pub use emit::emit_module;
pub use types::{ABI_VERSION as WASM_ABI_VERSION, ELEMENT_TAG_TABLE};

pub(crate) fn lower_program_with_imports(
    program: &AotProgram,
    imports: &[WasmImport],
) -> AotResult<IrModule> {
    lower::lower_program(program, imports)
}
