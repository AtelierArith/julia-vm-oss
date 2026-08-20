mod allocator;
mod control;
mod conversion;
mod descriptor;
mod descriptor_data;
mod descriptor_shape;
mod drop;
mod free;
mod instruction;
mod locals;
mod math;
mod memory;
mod ops;
mod strings;
mod transcendental;
mod transcendental_approx;

use std::collections::HashMap;

use crate::aot::codegen::CAbiExport;
use crate::aot::ir::{BasicBlock, IrFunction, IrModule};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
    GlobalSection, Instruction as W, MemorySection, MemoryType, Module, TypeSection, ValType,
};

use super::types::{unsupported, value_type, ABI_VERSION};
use control::emit_terminator;
use instruction::emit_instruction;
use locals::{build_local_layout, collect_phi_edges, LocalLayout, PhiEdges};
use ops::required_type;

const RESERVED_EXPORT_NAMES: [&str; 5] = [
    "memory",
    "__sjulia_wasm_abi_version",
    allocator::ALLOC_NAME,
    allocator::FREE_NAME,
    allocator::DROP_NAME,
];

pub fn emit_module(ir: &IrModule, requested_exports: &[CAbiExport]) -> AotResult<Vec<u8>> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    let strings = strings::StaticStrings::collect(ir)?;
    let function_indices = function_indices(ir)?;
    for (index, function) in ir.functions.iter().enumerate() {
        let params = function
            .params
            .iter()
            .map(|(_, ty)| required_type(ty))
            .collect::<AotResult<Vec<_>>>()?;
        types
            .ty()
            .function(params, value_type(&function.return_type)?.into_iter());
        let index = u32::try_from(index)
            .map_err(|_| AotError::CodegenError("too many Wasm functions".to_string()))?;
        functions.function(index);
        code.function(&emit_function(function, &function_indices, &strings)?);
    }
    let alloc_index = u32::try_from(ir.functions.len())
        .map_err(|_| AotError::CodegenError("too many Wasm types".to_string()))?;
    let free_index = alloc_index + 1;
    let drop_index = alloc_index + 2;
    let abi_index = alloc_index + 3;
    types
        .ty()
        .function([ValType::I64, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], []);
    types.ty().function([ValType::I32], []);
    types.ty().function([], [ValType::I32]);
    functions.function(alloc_index);
    functions.function(free_index);
    functions.function(drop_index);
    functions.function(abi_index);
    module.section(&types);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 64,
        maximum: Some(allocator::MAX_MEMORY_PAGES),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(
        allocator::heap_global_type(),
        &ConstExpr::i32_const(strings.heap_base()),
    );
    module.section(&globals);
    emit_function_exports(&mut exports, ir, requested_exports, &function_indices)?;
    exports.export("memory", ExportKind::Memory, 0);
    exports.export(allocator::ALLOC_NAME, ExportKind::Func, alloc_index);
    exports.export(allocator::FREE_NAME, ExportKind::Func, free_index);
    exports.export(allocator::DROP_NAME, ExportKind::Func, drop_index);
    exports.export("__sjulia_wasm_abi_version", ExportKind::Func, abi_index);
    module.section(&exports);
    code.function(&allocator::emit_alloc(0, strings.heap_base()));
    code.function(&free::emit_free(0, strings.heap_base()));
    code.function(&drop::emit_drop(free_index));
    let mut abi = Function::new([]);
    abi.instruction(&W::I32Const(ABI_VERSION));
    abi.instruction(&W::End);
    code.function(&abi);
    module.section(&code);
    if let Some(data) = strings.data_section() {
        module.section(&data);
    }
    Ok(module.finish())
}

fn function_indices(ir: &IrModule) -> AotResult<HashMap<String, u32>> {
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

fn emit_function_exports(
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

    let mut public_names = std::collections::HashSet::with_capacity(requested.len());
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

fn emit_function(
    function: &IrFunction,
    functions: &HashMap<String, u32>,
    strings: &strings::StaticStrings,
) -> AotResult<Function> {
    let layout = build_local_layout(function)?;
    let block_indices: HashMap<_, _> = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.label.clone(), index as i32))
        .collect();
    let entry = *block_indices
        .get(&function.entry)
        .ok_or_else(|| AotError::InvalidIR(format!("missing entry block `{}`", function.entry)))?;
    let phi_edges = collect_phi_edges(function);
    let mut body = Function::new(layout.declarations.clone());
    body.instruction(&W::I32Const(entry));
    body.instruction(&W::LocalSet(layout.pc));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    for block in &function.blocks {
        emit_dispatch_block(
            &mut body,
            block,
            functions,
            &block_indices,
            &phi_edges,
            &layout,
            strings,
        )?;
    }
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body.instruction(&W::End);
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    Ok(body)
}

fn emit_dispatch_block(
    body: &mut Function,
    block: &BasicBlock,
    functions: &HashMap<String, u32>,
    blocks: &HashMap<String, i32>,
    phi_edges: &PhiEdges,
    layout: &LocalLayout,
    strings: &strings::StaticStrings,
) -> AotResult<()> {
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::LocalGet(layout.pc));
    body.instruction(&W::I32Const(blocks[&block.label]));
    body.instruction(&W::I32Ne);
    body.instruction(&W::BrIf(0));
    for instruction in &block.instructions {
        emit_instruction(body, instruction, layout, functions, strings)?;
    }
    let terminator = block.terminator.as_ref().ok_or_else(|| {
        AotError::InvalidIR(format!("Wasm IR block `{}` has no terminator", block.label))
    })?;
    emit_terminator(body, &block.label, terminator, blocks, phi_edges, layout)?;
    body.instruction(&W::End);
    Ok(())
}
