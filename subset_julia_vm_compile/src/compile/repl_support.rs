//! Public compiler surface used by the REPL integration layer.
//!
//! Keeping these entry points behind one facade makes the eventual
//! `subset_julia_vm_compile` extraction reviewable: REPL code should depend on
//! this surface, not on cache/pipeline/collector internals directly.

use std::collections::{HashMap, HashSet};

use crate::bytecode::{CompiledProgram, ValueType};
use crate::compile::types::CResult;
use crate::ir::core::{Block, Function, Program};

pub use super::cache::{AppendableDelta, AppendableFunction, ReplPersistentCompile};

pub fn current_runtime_nominal_names(program: &Program) -> HashSet<String> {
    super::cache::repl_current_runtime_nominal_names(program)
}

pub fn current_type_names(program: &Program) -> HashSet<String> {
    super::cache::repl_current_type_names(program)
}

pub fn full_compile(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    current_input_function_count: usize,
    current_input_struct_count: usize,
    current_input_type_names: &HashSet<String>,
    current_input_runtime_nominal_names: &HashSet<String>,
) -> CResult<(CompiledProgram, ReplPersistentCompile)> {
    super::cache::repl_full_compile(
        program,
        global_types,
        global_struct_names,
        current_input_function_count,
        current_input_struct_count,
        current_input_type_names,
        current_input_runtime_nominal_names,
    )
}

pub fn delta_compile(
    prev: &ReplPersistentCompile,
    input: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
) -> CResult<(CompiledProgram, ReplPersistentCompile)> {
    super::cache::repl_delta_compile(prev, input, global_types, global_struct_names)
}

pub fn relocatable_delta_compile(
    prev: &ReplPersistentCompile,
    input: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    global_slot_seed: &[String],
    live_code_len: usize,
) -> CResult<Option<AppendableDelta>> {
    super::cache::repl_relocatable_delta_compile(
        prev,
        input,
        global_types,
        global_struct_names,
        global_slot_seed,
        live_code_len,
    )
}

pub fn program_defines_via_opaque_eval(program: &Program) -> bool {
    super::pipeline_ctx::program_defines_via_opaque_eval(program)
}

pub fn program_main_has_hard_scope_block(program: &Program) -> bool {
    super::pipeline_ctx::program_main_has_hard_scope_block(program)
}

/// Whether lowering synthesized this function without a source-definition
/// activation marker. REPL/session code must use provenance rather than the
/// helper's generated spelling when separating Julia methods from helpers.
pub fn is_markerless_lowered_function(function: &Function) -> bool {
    super::ir_inline::is_markerless_lowered_function(function)
}

/// Number of Julia-visible source methods in a fragment. Marker-less lowering
/// helpers do not consume source-order activation budgets.
pub fn source_function_count(program: &Program) -> usize {
    program
        .functions
        .iter()
        .filter(|function| !is_markerless_lowered_function(function))
        .count()
}

pub fn collect_module_body_binding_names(block: &Block, names: &mut HashSet<String>) {
    super::collect::collect_module_body_binding_names(block, names);
}

/// Julia-visible bindings introduced into Main by the program's resolved
/// `using`/`import` graph. These are import publication, not value assignments
/// owned by the current REPL input.
pub fn main_import_binding_names(program: &Program) -> HashSet<String> {
    runtime_import_binding_names(program)
        .into_keys()
        .filter_map(|qualified| qualified.strip_prefix("Main.").map(str::to_string))
        .collect()
}

/// Qualified runtime storage names for visible imports in every module scope.
/// The compiler materializes these stores while executing `using`/`import`, but
/// they are binding publication rather than mutable module values.
pub fn runtime_import_binding_names(program: &Program) -> HashMap<String, String> {
    super::pipeline_ctx::collect_live_import_bindings(program)
}

/// Compiler-owned frame-0 storage used to implement import provenance and
/// ambiguity. The REPL must never expose or persist these as Julia values.
pub fn is_runtime_import_metadata_binding(name: &str) -> bool {
    super::module_alias::is_runtime_import_metadata_binding(name)
}

/// Anonymous helper IR declared directly in user main. The REPL retains these
/// definitions so a later full rebuild can reconstruct value-carried closures
/// that were originally installed by a live delta.
pub fn collect_main_inline_anonymous_functions(program: &Program) -> Vec<Function> {
    let mut functions = Vec::new();
    super::collect::collect_block_functions(&program.main, &mut functions, None);
    functions
        .into_iter()
        .filter(|(function, parent)| parent.is_none() && is_markerless_lowered_function(function))
        .map(|(function, _)| function)
        .collect()
}

/// Named root methods represented by `Stmt::FunctionDef` inside user main
/// rather than by `Program.functions` (for example a method inside `begin`).
/// These participate in REPL definition persistence and full-compile method
/// snapshots just like ordinary hoisted source methods (Issue #11683).
pub fn collect_main_inline_named_functions(program: &Program) -> Vec<Function> {
    let mut functions = Vec::new();
    super::collect::collect_block_functions(&program.main, &mut functions, None);
    functions
        .into_iter()
        .filter(|(function, parent)| {
            let leaf = function.name.rsplit('#').next().unwrap_or(&function.name);
            parent.is_none()
                && !leaf.starts_with("__lambda_")
                && !leaf.starts_with("__do_block_")
                && !leaf.starts_with("__gen_body_")
        })
        .map(|(function, _)| function)
        .collect()
}
