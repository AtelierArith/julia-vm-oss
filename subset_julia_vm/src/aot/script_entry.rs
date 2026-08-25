use std::sync::Arc;

use crate::ir::core::{Block, Function, Module, Program, Stmt};

use super::{codegen::CAbiExport, AotError, AotResult, CompileConfig};

/// Reserved generated function/export name for top-level script execution.
pub const SCRIPT_ENTRY_NAME: &str = "__sjulia_script_entry";

impl CompileConfig {
    /// Request structural lifting and export of the top-level script body.
    pub fn enable_script_entry(&mut self) {
        self.c_abi_exports
            .push(CAbiExport::new(SCRIPT_ENTRY_NAME, SCRIPT_ENTRY_NAME));
    }

    pub(super) fn requests_script_entry(&self) -> bool {
        self.c_abi_exports
            .iter()
            .any(|export| export.function_name == SCRIPT_ENTRY_NAME)
    }
}

fn module_contains_function(module: &Module, name: &str) -> bool {
    module
        .functions
        .iter()
        .any(|function| function.name == name)
        || module
            .submodules
            .iter()
            .any(|submodule| module_contains_function(submodule, name))
}

pub(super) fn lift_script_entry(program: &mut Program) -> AotResult<()> {
    let reserved_name_exists = program
        .functions
        .iter()
        .any(|function| function.name == SCRIPT_ENTRY_NAME)
        || program
            .modules
            .iter()
            .any(|module| module_contains_function(module, SCRIPT_ENTRY_NAME));
    if reserved_name_exists {
        return Err(AotError::CodegenError(format!(
            "script entry name `{SCRIPT_ENTRY_NAME}` is reserved"
        )));
    }

    let span = program.main.span;
    let mut statements = std::mem::take(&mut program.main.stmts);
    statements.push(Stmt::Return { value: None, span });
    program.functions.push(Arc::new(Function {
        name: SCRIPT_ENTRY_NAME.to_string(),
        params: Vec::new(),
        kwparams: Vec::new(),
        type_params: Vec::new(),
        return_type: None,
        body: Block {
            stmts: statements,
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        new_struct_name: None,
        span,
    }));
    Ok(())
}
