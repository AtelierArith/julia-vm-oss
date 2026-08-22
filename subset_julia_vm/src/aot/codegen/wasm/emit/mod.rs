mod aggregate;
mod allocator;
mod array;
mod array_init;
mod array_slice;
mod array_slice_assign;
mod array_slice_copy;
mod array_slice_dispatch;
mod array_slice_validate;
mod assembly;
mod control;
mod conversion;
mod descriptor;
mod descriptor_data;
mod descriptor_shape;
mod drop;
mod free;
mod instruction;
mod layouts;
mod locals;
mod math;
mod memory;
mod ops;
mod rng;
mod rng_array;
mod rng_array_fill;
mod rng_normal;
mod rng_seed;
mod rng_tables;
mod strings;
mod transcendental;
mod transcendental_approx;

use std::collections::HashMap;

use crate::aot::codegen::CAbiExport;
use crate::aot::ir::{BasicBlock, IrFunction, IrModule};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, WasmImport};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, ImportSection, Instruction as W, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

use super::types::{unsupported, value_type, ABI_VERSION};
use assembly::{emit_function_exports, function_indices};
use control::emit_terminator;
use instruction::emit_instruction;
use locals::{build_local_layout, collect_phi_edges, LocalLayout, PhiEdges};
use ops::required_type;

pub fn emit_module(
    ir: &IrModule,
    requested_exports: &[CAbiExport],
    requested_imports: &[WasmImport],
) -> AotResult<Vec<u8>> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    let layouts = layouts::StaticLayouts::collect(ir)?;
    let rng_tables = rng_tables::RngTables::collect(layouts.end()?)?;
    let strings = strings::StaticStrings::collect(ir, rng_tables.end()?)?;
    let mut function_indices = function_indices(ir, requested_imports)?;
    for import in requested_imports {
        let function = ir
            .functions
            .iter()
            .find(|function| function.name == import.function_name)
            .ok_or_else(|| {
                unsupported(format!(
                    "Wasm import `{}.{}` cannot resolve generated function `{}`",
                    import.module, import.name, import.function_name
                ))
            })?;
        let expected_result = import.result.clone().unwrap_or(StaticType::Nothing);
        if function
            .params
            .iter()
            .map(|(_, ty)| ty)
            .ne(import.params.iter())
            || function.return_type != expected_result
        {
            return Err(unsupported(format!(
                "Wasm import `{}.{}` does not match generated function `{}`",
                import.module, import.name, import.function_name
            )));
        }
    }
    let imported_count = u32::try_from(requested_imports.len())
        .map_err(|_| AotError::CodegenError("too many Wasm imports".to_string()))?;
    let defined_functions: Vec<_> = ir
        .functions
        .iter()
        .filter(|function| {
            !requested_imports
                .iter()
                .any(|import| import.function_name == function.name)
        })
        .collect();
    let alloc_index = imported_count
        + u32::try_from(defined_functions.len())
            .map_err(|_| AotError::CodegenError("too many Wasm types".to_string()))?;
    let free_index = alloc_index + 1;
    let rng_next_index = alloc_index + 2;
    let rng_randn_index = alloc_index + 3;
    let rng_seed_index = alloc_index + 4;
    function_indices.insert(allocator::ALLOC_NAME.to_string(), alloc_index);
    function_indices.insert(allocator::FREE_NAME.to_string(), free_index);
    function_indices.insert(rng::NEXT_NAME.to_string(), rng_next_index);
    function_indices.insert(rng_normal::RANDN_NAME.to_string(), rng_randn_index);
    for import in requested_imports {
        let params = import
            .params
            .iter()
            .map(required_type)
            .collect::<AotResult<Vec<_>>>()?;
        let results: Vec<_> = import
            .result
            .as_ref()
            .map(value_type)
            .transpose()?
            .into_iter()
            .flatten()
            .collect();
        types.ty().function(params, results);
    }
    let mut defined_type_index = imported_count;
    for function in &defined_functions {
        let params = function
            .params
            .iter()
            .map(|(_, ty)| required_type(ty))
            .collect::<AotResult<Vec<_>>>()?;
        types
            .ty()
            .function(params, value_type(&function.return_type)?.into_iter());
        functions.function(defined_type_index);
        code.function(&emit_function(function, &function_indices, &strings)?);
        defined_type_index += 1;
    }
    let drop_index = alloc_index + 5;
    let layout_table_index = alloc_index + 6;
    let layout_count_index = alloc_index + 7;
    let abi_index = alloc_index + 8;
    types
        .ty()
        .function([ValType::I64, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], []);
    types.ty().function([], [ValType::I64]);
    types.ty().function([], [ValType::F64]);
    types.ty().function([ValType::I64], []);
    types.ty().function([ValType::I32], []);
    types.ty().function([], [ValType::I32]);
    types.ty().function([], [ValType::I32]);
    types.ty().function([], [ValType::I32]);
    functions.function(alloc_index);
    functions.function(free_index);
    functions.function(rng_next_index);
    functions.function(rng_randn_index);
    functions.function(rng_seed_index);
    functions.function(drop_index);
    functions.function(layout_table_index);
    functions.function(layout_count_index);
    functions.function(abi_index);
    module.section(&types);
    if !requested_imports.is_empty() {
        let mut imports = ImportSection::new();
        for (type_index, import) in requested_imports.iter().enumerate() {
            imports.import(
                &import.module,
                &import.name,
                EntityType::Function(u32::try_from(type_index).map_err(|_| {
                    AotError::CodegenError("too many Wasm function types".to_string())
                })?),
            );
        }
        module.section(&imports);
    }
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
    for state in rng_seed::initial_state() {
        globals.global(
            rng::state_global_type(),
            &ConstExpr::i64_const(state as i64),
        );
    }
    module.section(&globals);
    emit_function_exports(&mut exports, ir, requested_exports, &function_indices)?;
    exports.export("memory", ExportKind::Memory, 0);
    exports.export(allocator::ALLOC_NAME, ExportKind::Func, alloc_index);
    exports.export(allocator::FREE_NAME, ExportKind::Func, free_index);
    exports.export(rng_seed::SEED_NAME, ExportKind::Func, rng_seed_index);
    exports.export(allocator::DROP_NAME, ExportKind::Func, drop_index);
    exports.export(
        "__sjulia_layout_table",
        ExportKind::Func,
        layout_table_index,
    );
    exports.export(
        "__sjulia_layout_count",
        ExportKind::Func,
        layout_count_index,
    );
    exports.export("__sjulia_wasm_abi_version", ExportKind::Func, abi_index);
    module.section(&exports);
    code.function(&allocator::emit_alloc(0, strings.heap_base()));
    code.function(&free::emit_free(0, strings.heap_base()));
    code.function(&rng::emit_next([1, 2, 3, 4]));
    code.function(&rng_normal::emit_randn(rng_next_index, &rng_tables));
    code.function(&rng_seed::emit_seed([1, 2, 3, 4]));
    code.function(&drop::emit_drop(free_index));
    code.function(&constant_i32_function(layouts.table_address()));
    code.function(&constant_i32_function(layouts.count()));
    let mut abi = Function::new([]);
    abi.instruction(&W::I32Const(ABI_VERSION));
    abi.instruction(&W::End);
    code.function(&abi);
    module.section(&code);
    if let Some(data) = layouts.data_section() {
        module.section(&data);
    }
    module.section(&rng_tables.data_section());
    if let Some(data) = strings.data_section() {
        module.section(&data);
    }
    Ok(module.finish())
}

fn constant_i32_function(value: i32) -> Function {
    let mut body = Function::new([]);
    body.instruction(&W::I32Const(value));
    body.instruction(&W::End);
    body
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
