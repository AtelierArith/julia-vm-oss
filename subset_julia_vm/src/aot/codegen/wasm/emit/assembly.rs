use std::collections::{HashMap, HashSet};

use crate::aot::codegen::CAbiExport;
use crate::aot::ir::IrModule;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{ExportKind, ExportSection};

use super::super::types::unsupported;
use super::allocator;

const RESERVED_EXPORT_NAMES: [&str; 8] = [
    "memory",
    "__sjulia_wasm_abi_version",
    allocator::ALLOC_NAME,
    allocator::FREE_NAME,
    allocator::DROP_NAME,
    "__sjulia_layout_table",
    "__sjulia_layout_count",
    super::rng_seed::SEED_NAME,
];

pub(super) fn function_indices(ir: &IrModule) -> AotResult<HashMap<String, u32>> {
    let mut indices = HashMap::with_capacity(ir.functions.len());
    for (index, function) in ir.functions.iter().enumerate() {
        if RESERVED_EXPORT_NAMES.contains(&function.name.as_str()) {
            return Err(unsupported(format!(
                "Wasm function name `{}` is reserved by the generated-module ABI",
                function.name
            )));
        }
        let index = u32::try_from(index)
            .map_err(|_| AotError::CodegenError("too many Wasm functions".to_string()))?;
        if indices.insert(function.name.clone(), index).is_some() {
            return Err(unsupported(format!(
                "Wasm AoT does not support duplicate or overloaded function name `{}`",
                function.name
            )));
        }
    }
    Ok(indices)
}

pub(super) fn emit_function_exports(
    exports: &mut ExportSection,
    ir: &IrModule,
    requested: &[CAbiExport],
    indices: &HashMap<String, u32>,
) -> AotResult<()> {
    if requested.is_empty() {
        for function in &ir.functions {
            exports.export(&function.name, ExportKind::Func, indices[&function.name]);
        }
        return Ok(());
    }

    let mut public_names = HashSet::with_capacity(requested.len());
    for request in requested {
        if RESERVED_EXPORT_NAMES.contains(&request.export_name.as_str()) {
            return Err(unsupported(format!(
                "Wasm export name `{}` is reserved by the generated-module ABI",
                request.export_name
            )));
        }
        if !public_names.insert(request.export_name.as_str()) {
            return Err(unsupported(format!(
                "duplicate Wasm export name `{}`",
                request.export_name
            )));
        }
        let candidates: Vec<_> = ir
            .functions
            .iter()
            .filter(|function| {
                function.name == request.function_name
                    && request.arg_types.as_ref().is_none_or(|arg_types| {
                        function
                            .params
                            .iter()
                            .map(|(_, ty)| ty)
                            .eq(arg_types.iter())
                    })
            })
            .collect();
        let [target] = candidates.as_slice() else {
            return Err(unsupported(format!(
                "Wasm export `{}` must resolve to exactly one function `{}`; found {}",
                request.export_name,
                request.function_name,
                candidates.len()
            )));
        };
        exports.export(
            &request.export_name,
            ExportKind::Func,
            indices[&target.name],
        );
    }
    Ok(())
}
