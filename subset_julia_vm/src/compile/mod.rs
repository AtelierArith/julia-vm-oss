//! Compiler for ir/core::Program with multiple dispatch support.
//!
//! This module compiles the new Core IR (from tree-sitter lowering) to bytecode,
//! supporting type-annotated function parameters and multiple dispatch.
//!
//! # Module Organization
//!
//! - `base_functions.rs`: Base function classification and builtin operation mapping
//! - `base_merge.rs`: Base program merging logic
//! - `collect.rs`: Collection and resolution helpers for the compilation driver
//! - `constants.rs`: Math constants and helper functions
//! - `context.rs`: Compilation context
//! - `core_compiler.rs`: CoreCompiler struct, LoopContext, FinallyContext, type predicates
//! - `free_vars.rs`: Free variable analysis for closure capture detection
//! - `inference.rs`: Type inference
//! - `types.rs`: Type definitions and error handling
//! - `utils.rs`: Binary op conversion, literal evaluation, and other utilities
//! - `stmt.rs`: Statement compilation
//! - `expr/`: Expression compilation

pub mod abstract_interp;
mod base_functions;
mod base_merge;
pub mod bridge;
pub mod cache;
mod collect;
pub mod const_prop;
mod constants;
mod context;
mod core_compiler;
pub mod diagnostics;
pub mod effects;
pub(crate) mod embedded_cache;
mod expr;
mod free_vars;
mod inference;
pub mod ipo;
pub mod lattice;
mod method_table;
mod peephole;
pub mod precompile;
pub mod promotion;
mod stmt;
pub mod tfuncs;
mod type_helpers;
pub mod type_stability;
mod types;
pub mod union_split;
mod utils;

#[cfg(test)]
pub(crate) mod test_helpers;

use base_functions::{
    base_function_to_builtin_op, extract_module_path_from_expr, is_base_function,
    is_base_submodule_function, is_random_function, is_reducible_nary_operator,
    type_expr_to_string,
};
use base_merge::merge_with_precompiled_base;
pub use cache::{compile_with_cache, compile_with_cache_with_globals};
pub use constants::needs_specialization;
use constants::{
    get_base_exported_constant_value, get_math_constant_value, is_euler_name, is_math_constant,
    is_pi_name, is_stdlib_module,
};
use context::SharedCompileContext;
pub use context::StructInfo;
use core_compiler::{
    is_float_type, is_numeric_type, is_singleton_type, CoreCompiler, FinallyContext, LoopContext,
};
pub(crate) use free_vars::analyze_free_variables;
use inference::{
    build_shared_inference_engine, collect_global_types_for_inference,
    collect_local_types_with_mixed_tracking,
};
use method_table::{MethodSig, MethodTable};
use type_helpers::{
    check_type_satisfies_bound, is_builtin_type_name, julia_type_to_value_type,
    julia_type_to_value_type_with_table, widen_numeric_types,
};
use types::parse_parametric_call;
pub use types::{err, CResult, CompileError, InstantiationKey, ParametricStructDef};

use collect::{
    collect_block_functions, collect_from_module, collect_module_functions, collect_module_info,
    collect_module_structs, collect_module_usings, collect_module_usings_recursive,
    collect_stmt_functions, collect_struct_literal_types, qualify_type_for_module,
    resolve_abstract_type, resolve_type_alias,
};
pub(super) use utils::{binary_op_to_function_name, function_name_to_binary_op};
use utils::{eval_literal_default, infer_default_type, is_required_kwarg, relocate_jumps};

use crate::ir::core::{Function, Program, Stmt, UsingImport};
use crate::types::{JuliaType, TypeExpr, TypeParam};
use crate::vm::slot::{build_slot_info, slotize_code};
use crate::vm::value::ArrayElementType;
use crate::vm::{
    AbstractTypeDefInfo, CompiledProgram, FunctionInfo, Instr, KwParamInfo, RuntimeCompileContext,
    ShowMethodEntry, SpecializableFunction, StructDefInfo, ValueType,
};
use std::collections::{HashMap, HashSet};

// MergedProgram and merge_with_precompiled_base are now in base_merge.rs module
// Helper functions are now in base_functions.rs module

/// Optional cache inputs for compilation.
/// Groups the precompiled Base bytecode, method tables, and closure captures
/// that can be reused across compilations (Issue #2933).
#[derive(Debug, Default)]
pub(crate) struct CompilerCacheInput<'a> {
    pub precompiled_base: Option<&'a CompiledProgram>,
    pub method_tables: Option<&'a HashMap<String, MethodTable>>,
    pub closure_captures: Option<&'a HashMap<String, std::collections::HashSet<String>>>,
}

/// Compile a Core IR Program into bytecode with multiple dispatch support.
pub fn compile_core_program(program: &Program) -> CResult<CompiledProgram> {
    compile_core_program_with_globals(program, &HashMap::new(), &HashMap::new())
}

pub fn compile_core_program_with_globals(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
) -> CResult<CompiledProgram> {
    let (compiled, _method_tables, _closure_captures) = compile_core_program_internal(
        program,
        global_types,
        global_struct_names,
        CompilerCacheInput::default(),
    )?;
    Ok(compiled)
}

/// Internal compilation with optional precompiled Base cache and method tables
/// Returns (CompiledProgram, method_tables, closure_captures) for caching
pub(crate) fn compile_core_program_internal(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    cache_input: CompilerCacheInput<'_>,
) -> CResult<(
    CompiledProgram,
    HashMap<String, MethodTable>,
    HashMap<String, std::collections::HashSet<String>>,
)> {
    let CompilerCacheInput {
        precompiled_base,
        method_tables: cached_method_tables,
        closure_captures: cached_closure_captures,
    } = cache_input;

    // Use base_function_count from program if already merged by lib.rs,
    // otherwise merge with base (for JSON IR input that doesn't use lib.rs pipeline)
    let (program_ref, base_function_count): (std::borrow::Cow<Program>, usize) =
        if program.base_function_count > 0 {
            // Already merged by lib.rs - use as-is
            (
                std::borrow::Cow::Borrowed(program),
                program.base_function_count,
            )
        } else {
            // Not merged yet (e.g., JSON IR) - merge now
            let merged = merge_with_precompiled_base(program);
            (
                std::borrow::Cow::Owned(merged.program),
                merged.base_function_count,
            )
        };
    let program = program_ref.as_ref();

    // Load stdlib modules for any using statements
    // that reference stdlib modules not already in program.modules
    let existing_module_names: HashSet<String> =
        program.modules.iter().map(|m| m.name.clone()).collect();
    let loaded_modules: Vec<crate::ir::core::Module> = {
        // Collect all using imports from top-level and from within modules
        let mut all_usings: Vec<&UsingImport> = program.usings.iter().collect();

        for module in &program.modules {
            collect_module_usings_recursive(module, &mut all_usings);
        }

        // Use pure Rust stdlib loader for WASM builds
        let usings_to_load: Vec<UsingImport> = all_usings
            .iter()
            .filter(|u| !u.is_relative)
            .filter(|u| !existing_module_names.contains(&u.module))
            .filter(|u| !matches!(u.module.as_str(), "Base" | "Core" | "Main" | "Pkg"))
            .map(|u| (*u).clone())
            .collect();
        crate::stdlib_loader::load_stdlib_modules(&usings_to_load)
    };

    // Combine program modules with loaded stdlib modules
    let all_modules: Vec<&crate::ir::core::Module> = program
        .modules
        .iter()
        .chain(loaded_modules.iter())
        .collect();

    // Build struct table from struct definitions
    // Separate parametric structs from concrete structs
    let mut struct_table: HashMap<String, StructInfo> = HashMap::new();
    let mut parametric_structs: HashMap<String, ParametricStructDef> = HashMap::new();

    // When using cache, initialize struct_defs from cached base to maintain consistent type_ids.
    // This is critical because cached bytecode contains NewStruct instructions with type_ids
    // that must match the struct_defs indices.
    //
    // Also build instantiation_table for parametric instantiations like Complex{Float64}
    // to prevent re-instantiation with different type_ids.
    let mut cached_instantiation_table: HashMap<InstantiationKey, usize> = HashMap::new();
    let (mut struct_defs, mut next_type_id): (Vec<StructDefInfo>, usize) =
        if let Some(base_cache) = precompiled_base {
            let cached_len = base_cache.struct_defs.len();
            // Also rebuild struct_table for cached structs so we can look them up
            for (idx, def) in base_cache.struct_defs.iter().enumerate() {
                struct_table.insert(
                    def.name.clone(),
                    StructInfo {
                        type_id: idx,
                        is_mutable: def.is_mutable,
                        fields: def.fields.clone(),
                        // Base structs with inner constructors are already compiled;
                        // the method_tables cache handles their dispatch.
                        has_inner_constructor: false,
                    },
                );

                // For parametric instantiations like "Complex{Float64}", build instantiation_table entry
                if let Some(brace_idx) = def.name.find('{') {
                    let base_name = def.name[..brace_idx].to_string();
                    let type_args_str = &def.name[brace_idx + 1..def.name.len() - 1];
                    // Parse type arguments - use JuliaType::from_name_or_struct to get
                    // the correct JuliaType variant (e.g., Float64 -> JuliaType::Float64)
                    let type_args: Vec<TypeExpr> = type_args_str
                        .split(", ")
                        .map(|s| TypeExpr::Concrete(JuliaType::from_name_or_struct(s)))
                        .collect();
                    let key = InstantiationKey {
                        base_name,
                        type_args,
                    };
                    cached_instantiation_table.insert(key, idx);
                }
            }
            (base_cache.struct_defs.clone(), cached_len)
        } else {
            (Vec::new(), 0)
        };

    // Collect all structs: top-level (None) + module structs (Some(module_path))
    let mut all_structs: Vec<(&crate::ir::core::StructDef, Option<String>)> =
        program.structs.iter().map(|s| (s, None)).collect();

    for module in &all_modules {
        let mut module_structs = Vec::new();
        collect_module_structs(module, "", &mut module_structs);
        for (struct_def, module_path) in module_structs {
            all_structs.push((struct_def, Some(module_path)));
        }
    }

    // Build a map of module_path -> set of struct names defined in that module.
    // This is used to qualify struct type names in function parameters for module functions.
    let mut module_struct_names: HashMap<String, HashSet<String>> = HashMap::new();
    for (struct_def, module_path) in &all_structs {
        if let Some(path) = module_path {
            module_struct_names
                .entry(path.clone())
                .or_default()
                .insert(struct_def.name.clone());
        }
    }

    // Process all structs (top-level and module structs)
    for (struct_def, module_path) in &all_structs {
        // Determine the struct name (qualified for module structs)
        let struct_name = match module_path {
            Some(path) => format!("{}.{}", path, struct_def.name),
            None => struct_def.name.clone(),
        };

        // When using cache, skip Base structs that are already registered.
        // This prevents re-assigning type_ids and breaking cached bytecode.
        if precompiled_base.is_some() && struct_table.contains_key(&struct_name) {
            // For parametric structs, still register them in parametric_structs
            // but don't modify struct_table or struct_defs
            if struct_def.is_parametric() {
                parametric_structs.insert(
                    struct_name.clone(),
                    ParametricStructDef {
                        def: (*struct_def).clone(),
                    },
                );
            }
            continue;
        }

        if struct_def.is_parametric() {
            // Store parametric struct definition for later instantiation
            // All parametric structs (including Complex) are handled the same way
            parametric_structs.insert(
                struct_name.clone(),
                ParametricStructDef {
                    def: (*struct_def).clone(),
                },
            );
            // Also register with short name for module structs
            // This allows `Point(...)` syntax after `using .MyGeometry`
            if module_path.is_some() {
                parametric_structs.insert(
                    struct_def.name.clone(),
                    ParametricStructDef {
                        def: (*struct_def).clone(),
                    },
                );
            }
        } else {
            // Concrete struct - register immediately with sequential type_id
            let type_id = next_type_id;
            next_type_id += 1;

            let fields: Vec<(String, ValueType)> = struct_def
                .fields
                .iter()
                .map(|f| {
                    let vt = f
                        .as_julia_type()
                        .as_ref()
                        .map(julia_type_to_value_type)
                        .unwrap_or(ValueType::Any); // Untyped fields are Any (Julia semantics)
                    (f.name.clone(), vt)
                })
                .collect();

            let has_inner_ctor = !struct_def.inner_constructors.is_empty();
            struct_table.insert(
                struct_name.clone(),
                StructInfo {
                    type_id,
                    is_mutable: struct_def.is_mutable,
                    fields: fields.clone(),
                    has_inner_constructor: has_inner_ctor,
                },
            );

            // Also register with short name for module structs
            if module_path.is_some() {
                struct_table.insert(
                    struct_def.name.clone(),
                    StructInfo {
                        type_id,
                        is_mutable: struct_def.is_mutable,
                        fields: fields.clone(),
                        has_inner_constructor: has_inner_ctor,
                    },
                );
            }

            // Push to struct_defs for all structs
            // Complex is already at index 0, so update it; others get new indices
            if struct_def.name == "Complex" {
                // Update the placeholder at index 0 with actual definition
                // Use "Complex{Float64}" as the name for proper runtime dispatch matching
                // Methods like +(::Real, ::Complex{Float64}) need to match correctly
                struct_defs[0] = StructDefInfo {
                    name: "Complex{Float64}".to_string(),
                    is_mutable: struct_def.is_mutable,
                    fields,
                    parent_type: struct_def.parent_type.clone(),
                };
            } else {
                struct_defs.push(StructDefInfo {
                    name: struct_name,
                    is_mutable: struct_def.is_mutable,
                    fields,
                    parent_type: struct_def.parent_type.clone(),
                });
            }
        }
    }

    // Build abstract type definitions (Issue #2523: preserve type_params at runtime)
    let abstract_types: Vec<AbstractTypeDefInfo> = program
        .abstract_types
        .iter()
        .map(|at| AbstractTypeDefInfo {
            name: at.name.clone(),
            parent: at.parent.clone(),
            type_params: at.type_params.iter().map(|tp| tp.name.clone()).collect(),
        })
        .collect();

    // Build set of abstract type names for compiler
    let abstract_type_names: HashSet<String> = program
        .abstract_types
        .iter()
        .map(|at| at.name.clone())
        .collect();

    // Create shared compilation context
    // When using cache, pass the cached instantiation table to prevent re-instantiation
    let mut shared_ctx = if !cached_instantiation_table.is_empty() {
        SharedCompileContext::with_instantiation_table(
            struct_table,
            struct_defs,
            parametric_structs,
            abstract_types.clone(),
            next_type_id,
            cached_instantiation_table,
        )
    } else {
        SharedCompileContext::new(
            struct_table,
            struct_defs,
            parametric_structs,
            abstract_types.clone(),
            next_type_id,
        )
    };

    // Populate type aliases from program
    for alias in &program.type_aliases {
        shared_ctx
            .type_aliases
            .insert(alias.name.clone(), alias.target_type.clone());
    }

    // Pre-populate closure captures from cache (Issue #2100)
    // When using the compilation cache, outer Base functions are skipped (cached bytecode).
    // But their inner/nested functions still need to be compiled, and they reference
    // captured variables from the outer scope. Without this, those inner functions
    // would get empty closure_captures and fail with "Undefined variable" errors.
    if let Some(cached_captures) = cached_closure_captures {
        shared_ctx.closure_captures = cached_captures.clone();
    }

    // Store global_types temporarily - will resolve after struct_table is built
    let pending_global_types = global_types.clone();
    let pending_global_struct_names = global_struct_names.clone();

    // Build method tables from functions (including module functions)
    // Start with cached Base method tables if available (Option A optimization)
    let mut method_tables: HashMap<String, MethodTable> = if let Some(cached) = cached_method_tables
    {
        cached.clone()
    } else {
        HashMap::new()
    };

    // When using cache, initialize function_infos from cache to maintain consistent indices.
    // This is critical because cached bytecode contains Call instructions with indices that
    // must match function_infos. User functions are appended at the end.
    //
    // func_index_map: maps all_functions index -> function_infos index
    // - For Base functions (when using cache): identity mapping (0->0, 1->1, etc.)
    // - For user functions: maps to end of cache (e.g., 678->682 if cache has 682 entries)
    let (mut function_infos, mut global_index, cached_base_len): (Vec<FunctionInfo>, usize, usize) =
        if let Some(base_cache) = precompiled_base {
            let len = base_cache.functions.len();
            (base_cache.functions.clone(), len, len)
        } else {
            (Vec::new(), 0, 0)
        };
    let mut func_index_map: Vec<usize> = Vec::new();
    // When using cache, initialize show_methods from cached Base (Issue #2489).
    // Base show methods (e.g., show(io, Complex)) are skipped during the function loop
    // when using cache, so they must be pre-populated from the cached compilation.
    let mut show_methods: Vec<ShowMethodEntry> = if let Some(base_cache) = precompiled_base {
        base_cache.show_methods.clone()
    } else {
        Vec::new()
    };

    // Lazy AoT: Track functions that need specialization
    let mut specializable_functions: Vec<SpecializableFunction> = Vec::new();

    // Build module function mapping: module_path -> set of function names
    // For nested modules, path is "A.B.C"
    let mut module_functions: HashMap<String, HashSet<String>> = HashMap::new();
    let mut module_exports: HashMap<String, HashSet<String>> = HashMap::new();
    // Track module-level constants (variables assigned in module body)
    let mut module_constants: HashMap<String, HashSet<String>> = HashMap::new();

    // Collect info from all top-level modules (including precompiled stdlib)
    for module in &all_modules {
        collect_module_info(
            module,
            "",
            &mut module_functions,
            &mut module_exports,
            &mut module_constants,
        );
    }

    // Build set of function names that are imported via `using`
    // This respects both export restrictions and selective imports
    let mut imported_functions: HashSet<String> = HashSet::new();
    for using_import in &program.usings {
        let module_name = &using_import.module;

        // Get the functions available in this module
        if let Some(module_funcs) = module_functions.get(module_name) {
            // Get the exported functions (empty = all exported)
            let exports = module_exports.get(module_name);
            let all_exported = exports.is_none_or(|e| e.is_empty());

            match &using_import.symbols {
                // Selective import: `using Module: func1, func2`
                Some(symbols) => {
                    for sym in symbols {
                        // Check if the symbol exists in the module
                        if module_funcs.contains(sym) {
                            // Check if it's exported (or all are exported)
                            if all_exported || exports.is_some_and(|e| e.contains(sym)) {
                                imported_functions.insert(sym.clone());
                            }
                        }
                    }
                }
                // Import all exported: `using Module`
                None => {
                    for func_name in module_funcs {
                        // Only import if exported (or all are exported)
                        if all_exported || exports.is_some_and(|e| e.contains(func_name)) {
                            imported_functions.insert(func_name.clone());
                        }
                    }
                }
            }
        }
    }

    // Add top-level functions to imported_functions (they're always available)
    for func in &program.functions {
        imported_functions.insert(func.name.clone());
    }

    // For backward compatibility, also keep track of used module names
    let usings_set: HashSet<String> = program.usings.iter().map(|u| u.module.clone()).collect();

    // Collect all functions: top-level + module functions (including submodules)
    let mut all_functions: Vec<(&Function, Option<String>)> =
        program.functions.iter().map(|f| (f, None)).collect();

    for module in &all_modules {
        collect_module_functions(module, "", &mut all_functions);
    }

    // Collect inline functions from top-level statements (with parent function tracking)
    // inline_functions: Vec<(Function, Option<parent_func_name>)>
    let mut inline_functions: Vec<(Function, Option<String>)> = Vec::new();
    for stmt in &program.main.stmts {
        collect_stmt_functions(stmt, &mut inline_functions, None);
    }
    // Also collect from each top-level function's body
    for func in &program.functions {
        collect_block_functions(&func.body, &mut inline_functions, Some(&func.name));
    }
    // Also collect from module functions
    for module in &all_modules {
        collect_from_module(module, &mut inline_functions);
    }

    // Build maps for nested function tracking (Issue #1743)
    // 1. nested_function_parents: qualified_name -> parent_name (for general reference)
    // 2. func_to_parent: function_name -> parent_name (for lookup during compilation)
    //    Note: When multiple parents have same-named nested functions, we track the index
    let mut nested_function_parents: HashMap<String, String> = HashMap::new();

    // Track inline function indices to their parent functions
    // We use the index in inline_functions as a unique identifier
    let mut inline_func_parent_by_idx: HashMap<usize, String> = HashMap::new();
    for (idx, (func, parent_name)) in inline_functions.iter().enumerate() {
        if let Some(parent) = parent_name {
            // Create qualified name: "parent#nested"
            let qualified_name = format!("{}#{}", parent, func.name);
            nested_function_parents.insert(qualified_name, parent.clone());
            inline_func_parent_by_idx.insert(idx, parent.clone());
        }
    }

    // Track the index in all_functions where inline functions start
    let inline_start_idx = all_functions.len();

    // Add inline functions to all_functions and imported_functions
    for (func, parent_name) in &inline_functions {
        all_functions.push((func, None));
        // Mark inline functions as imported so they can be called
        // For nested functions, use qualified name for disambiguation
        if let Some(parent) = parent_name {
            let qualified_name = format!("{}#{}", parent, func.name);
            imported_functions.insert(qualified_name);
        } else {
            imported_functions.insert(func.name.clone());
        }
    }

    // Build a map from function index in all_functions to parent name (for inline functions only)
    let mut func_idx_to_parent: HashMap<usize, String> = HashMap::new();
    for (inline_idx, parent) in inline_func_parent_by_idx.iter() {
        let all_funcs_idx = inline_start_idx + inline_idx;
        func_idx_to_parent.insert(all_funcs_idx, parent.clone());
    }

    // Pre-populate closure captures for nested functions (Issue #2100)
    //
    // When using prelude cache, parent functions are skipped during compilation,
    // so Stmt::FunctionDef in parent bodies never runs and closure captures are
    // never analyzed. This causes "Undefined variable" errors for captured variables
    // in nested functions that act as closures (e.g., curried string search functions).
    //
    // Fix: analyze free variables for all nested functions upfront by examining
    // each parent function's parameters as the outer scope.
    for (nested_func, parent_name) in &inline_functions {
        if let Some(parent) = parent_name {
            // Find the parent function(s) with matching name to get their parameters
            for parent_func in &program.functions {
                if parent_func.name == *parent {
                    let outer_vars: HashSet<String> =
                        parent_func.params.iter().map(|p| p.name.clone()).collect();
                    let free_vars = analyze_free_variables(nested_func, &outer_vars);
                    if !free_vars.is_empty() {
                        let qname = format!("{}#{}", parent, nested_func.name);
                        shared_ctx.closure_captures.insert(qname, free_vars);
                        break;
                    }
                }
            }
        }
    }

    // Pre-instantiate parametric struct types used in function parameters
    // This ensures that types like Complex{Float64} are in struct_table
    // BEFORE we infer function return types for method tables
    for (func, _) in &all_functions {
        // Collect type parameter names from the function's where clause
        let type_param_names: HashSet<&str> =
            func.type_params.iter().map(|tp| tp.name.as_str()).collect();

        for param in &func.params {
            let param_ty = param.effective_type();
            if let JuliaType::Struct(name) = &param_ty {
                if let Some(brace_idx) = name.find('{') {
                    let base_name = &name[..brace_idx];
                    let type_args_str = &name[brace_idx + 1..name.len() - 1];

                    // Check if any type argument is a type parameter from where clause
                    // e.g., Rational{T} where T - T is a type parameter, not a concrete type
                    let type_arg_names: Vec<&str> =
                        type_args_str.split(',').map(|s| s.trim()).collect();
                    let has_type_param = type_arg_names
                        .iter()
                        .any(|arg| type_param_names.contains(arg));

                    // Skip instantiation if any type arg is a where clause type parameter
                    // These will be instantiated at call sites with concrete types
                    if has_type_param {
                        continue;
                    }

                    let type_args: Vec<TypeExpr> = type_arg_names
                        .iter()
                        .map(|s| TypeExpr::from_name(s, &[]))
                        .collect();
                    // Instantiate the parametric struct type
                    let _ = shared_ctx.resolve_instantiation_with_type_expr(base_name, &type_args);
                }
            }
        }
    }

    // Collect struct literal types from main block and function bodies
    let mut struct_literal_names: HashSet<String> = HashSet::new();
    collect_struct_literal_types(&program.main.stmts, &mut struct_literal_names);
    for (func, _) in &all_functions {
        collect_struct_literal_types(&func.body.stmts, &mut struct_literal_names);
    }

    // Instantiate parametric struct types from literals
    for struct_name in &struct_literal_names {
        if let Some(brace_idx) = struct_name.find('{') {
            let base_name = &struct_name[..brace_idx];
            let type_args_str = &struct_name[brace_idx + 1..struct_name.len() - 1];
            let type_arg_names: Vec<&str> = type_args_str.split(',').map(|s| s.trim()).collect();
            let type_args: Vec<TypeExpr> = type_arg_names
                .iter()
                .map(|s| TypeExpr::from_name(s, &[]))
                .collect();
            // Instantiate the type (ignore errors - may already exist)
            let _ = shared_ctx.resolve_instantiation_with_type_expr(base_name, &type_args);
        }
    }

    // Now that struct_table is fully built, resolve global_types from REPL session
    // Pre-collect global variable types from main block before function compilation.
    // This allows functions to reference top-level const/global variables with proper types.
    // Also collects const struct constructors for inlining in functions.
    {
        let mut global_types_map = std::mem::take(&mut shared_ctx.global_types);
        // Merge with provided global_types (from REPL session)
        // Resolve struct type_ids from struct_names using struct_table (now fully built)
        for (name, ty) in &pending_global_types {
            if let ValueType::Struct(_) = ty {
                // Resolve struct type_id from struct_name
                if let Some(struct_name) = pending_global_struct_names.get(name) {
                    if let Some(struct_info) = shared_ctx.struct_table.get(struct_name) {
                        global_types_map
                            .insert(name.clone(), ValueType::Struct(struct_info.type_id));
                        continue;
                    }
                    // Try to find parametric struct instance (e.g., "Rational{Int64}")
                    if let Some(brace_idx) = struct_name.find('{') {
                        let base_name = &struct_name[..brace_idx];
                        let prefix = format!("{}{{", base_name);
                        for (table_name, struct_info) in &shared_ctx.struct_table {
                            if table_name.starts_with(&prefix) || table_name == struct_name {
                                global_types_map
                                    .insert(name.clone(), ValueType::Struct(struct_info.type_id));
                                break;
                            }
                        }
                        continue;
                    }
                }
            }
            // For non-struct types or if struct resolution failed, use the provided type
            global_types_map.insert(name.clone(), ty.clone());
        }
        let mut global_const_structs = std::mem::take(&mut shared_ctx.global_const_structs);
        collect_global_types_for_inference(
            &program.main.stmts,
            &mut global_types_map,
            &shared_ctx.struct_table,
            &mut global_const_structs,
        );
        shared_ctx.global_types = global_types_map;
        shared_ctx.global_const_structs = global_const_structs;
    }

    // Also collect global types from module bodies (for module-level constants like SHIFTEDMONTHDAYS).
    // This ensures module-level constants are registered before function compilation so they're
    // not flagged as "undefined variable" when referenced from module functions.
    {
        let mut global_types_map = std::mem::take(&mut shared_ctx.global_types);
        let mut global_const_structs = std::mem::take(&mut shared_ctx.global_const_structs);
        for module in &all_modules {
            collect_global_types_for_inference(
                &module.body.stmts,
                &mut global_types_map,
                &shared_ctx.struct_table,
                &mut global_const_structs,
            );
        }
        shared_ctx.global_types = global_types_map;
        shared_ctx.global_const_structs = global_const_structs;
    }

    // Collect module-level using statements to support module-local imports.
    let mut module_usings: HashMap<String, Vec<UsingImport>> = HashMap::new();

    for module in &all_modules {
        collect_module_usings(module, "", &mut module_usings);
    }

    // Resolve module-local imports based on their using statements.
    let mut module_imports_map: HashMap<String, HashSet<String>> = HashMap::new();
    for (module_path, usings) in &module_usings {
        let mut imported = HashSet::new();
        for using_import in usings {
            let using_module = &using_import.module;
            // Skip relative imports (they refer to user modules, handled separately)
            if using_import.is_relative {
                continue;
            }
            if let Some(module_funcs) = module_functions.get(using_module) {
                let exports = module_exports.get(using_module);
                let all_exported = exports.is_none_or(|e| e.is_empty());

                match &using_import.symbols {
                    // Selective import: `using Module: func1, func2`
                    Some(symbols) => {
                        for sym in symbols {
                            if module_funcs.contains(sym.as_str())
                                && (all_exported
                                    || exports.is_some_and(|e| e.contains(sym.as_str())))
                            {
                                imported.insert(sym.clone());
                            }
                        }
                    }
                    // Import all exported functions: `using Module`
                    None => {
                        for func_name in module_funcs {
                            if all_exported || exports.is_some_and(|e| e.contains(func_name)) {
                                imported.insert(func_name.clone());
                            }
                        }
                    }
                }
            }
        }
        module_imports_map.insert(module_path.clone(), imported);
    }

    // Build a map from abstract type name to its parent for converting Struct to AbstractUser
    let abstract_type_parents: HashMap<String, Option<String>> = program
        .abstract_types
        .iter()
        .map(|at| (at.name.clone(), at.parent.clone()))
        .collect();

    let _total_functions = all_functions.len();

    // Build a shared inference engine ONCE before the loop.
    // Previously, infer_function_return_type_v2_with_functions was called inside the loop,
    // creating a new engine and cloning all ~5000 functions on every iteration (O(n^2)).
    // This shared engine clones functions once (O(n)) and reuses the return-type cache.
    let mut inference_engine =
        build_shared_inference_engine(&shared_ctx.struct_table, program.functions.iter());

    for (all_funcs_idx, (func, module_path)) in all_functions.iter().enumerate() {
        // Build params early (needed for both method tables and show methods)
        // For module functions, qualify struct type names to match the qualified struct instances.
        // Also convert Struct types to AbstractUser when the type is actually an abstract type.
        let params: Vec<(String, JuliaType)> = func
            .params
            .iter()
            .map(|p| {
                let ty = p.effective_type();
                let qualified_ty =
                    qualify_type_for_module(ty, module_path.as_ref(), &module_struct_names);
                let resolved_ty = resolve_abstract_type(qualified_ty, &abstract_type_parents);
                // Resolve type aliases (Issue #2527): const IntWrapper = Wrapper{Int64}
                let alias_resolved = resolve_type_alias(resolved_ty, &shared_ctx.type_aliases);
                (p.name.clone(), alias_resolved)
            })
            .collect();

        // Build vm_params, vm_kwparams, and return_type (needed for FunctionInfo)
        let vm_params: Vec<(String, ValueType)> = params
            .iter()
            .map(|(name, jt)| {
                (
                    name.clone(),
                    julia_type_to_value_type_with_table(jt, &shared_ctx.struct_table),
                )
            })
            .collect();

        let vm_kwparams: Vec<KwParamInfo> = func
            .kwparams
            .iter()
            .map(|kw| {
                let required = is_required_kwarg(&kw.default);
                // For varargs kwargs (kwargs...), type is always Pairs (Julia's Base.Pairs)
                // For required kwargs, use type annotation if available; otherwise use Any
                // For optional kwargs, infer from default value
                let ty = if kw.is_varargs {
                    ValueType::Pairs
                } else if required {
                    kw.type_annotation
                        .as_ref()
                        .map(|jt| julia_type_to_value_type_with_table(jt, &shared_ctx.struct_table))
                        .unwrap_or(ValueType::Any)
                } else {
                    infer_default_type(&kw.default)
                };
                KwParamInfo {
                    name: kw.name.clone(),
                    default: eval_literal_default(&kw.default),
                    ty,
                    slot: 0,
                    required,
                    is_varargs: kw.is_varargs,
                }
            })
            .collect();

        // Use declared return type if available, otherwise infer from function body
        // Using the shared inference engine (created once before the loop) for
        // abstract interpretation. The engine caches return types across calls.
        let (return_type, return_julia_type) = if let Some(ref declared_rt) = func.return_type {
            let vt = julia_type_to_value_type_with_table(declared_rt, &shared_ctx.struct_table);
            // Declared return types already carry parametric info via JuliaType
            let jt = if matches!(declared_rt, JuliaType::TupleOf(_)) {
                Some(declared_rt.clone())
            } else {
                None
            };
            (vt, jt)
        } else {
            let rt = inference_engine.infer_function(func);
            let vt = bridge::lattice_to_value_type(&rt);
            // Extract parametric tuple type that ValueType::Tuple would lose (Issue #2317)
            let jt = bridge::lattice_to_parametric_julia_type(&rt);
            (vt, jt)
        };

        // Skip Base functions if we're using cached method tables (Option A optimization)
        // Base methods are already in the cached method tables
        // When using cache, global_index starts at base_function_count, so we use loop counter instead
        // Note: all_funcs_idx is 0-indexed, so we use <= to match 1-indexed behavior
        let is_base_function = (all_funcs_idx + 1) <= base_function_count;
        let skip_method_table_update = is_base_function && cached_method_tables.is_some();
        // When using cache, skip function_infos.push() for Base functions (already in cache)
        let skip_function_info_push = is_base_function && precompiled_base.is_some();

        // Detect varargs parameter early (needed for both MethodSig and FunctionInfo)
        let vararg_param_index = func.params.iter().position(|p| p.is_varargs);
        // For Vararg{T, N}: extract fixed count N (Issue #2525)
        let vararg_fixed_count = func
            .params
            .iter()
            .find(|p| p.is_varargs)
            .and_then(|p| p.vararg_count);

        if !skip_method_table_update {
            let is_stdlib_func = module_path
                .as_ref()
                .map(|path| {
                    is_stdlib_module(path)
                        || path == "Base"
                        || path.starts_with("Base.")
                        || path == "Core"
                        || path.starts_with("Core.")
                })
                .unwrap_or(false);
            if !is_base_function && !is_stdlib_func {
                shared_ctx
                    .function_ir_by_global_index
                    .insert(global_index, (*func).clone());
            }
            let table = method_tables
                .entry(func.name.clone())
                .or_insert_with(|| MethodTable::new(func.name.clone()));

            let method_index = table.methods.len();
            table.add_method(MethodSig {
                _method_index: method_index,
                global_index,
                params: params.clone(),
                return_type: return_type.clone(),
                return_julia_type: return_julia_type.clone(),
                is_base_extension: func.is_base_extension,
                type_params: func.type_params.clone(),
                vararg_param_index,
                vararg_fixed_count,
            });
        }

        // Detect show methods: function Base.show(io::IO, x::SomeStruct)
        // Also detect show methods defined within base library files (e.g., io.jl)
        // Skip for cached Base functions — their show_methods are pre-populated from cache (Issue #2489)
        if !skip_function_info_push
            && (func.is_base_extension || is_base_function)
            && func.name == "show"
            && params.len() >= 2
        {
            // First param must be IO type
            if let JuliaType::IO = &params[0].1 {
                // Second param must be a Struct type
                if let JuliaType::Struct(struct_name) = &params[1].1 {
                    show_methods.push(ShowMethodEntry {
                        type_name: struct_name.clone(),
                        func_index: global_index,
                    });
                }
            }
        }

        // Build func_index_map and function_infos
        // When using cache, Base functions are already in function_infos (from cache clone)
        let func_info_idx = if skip_function_info_push {
            // Base function using cache: identity mapping (index in all_functions = index in function_infos)
            // all_funcs_idx is 0-indexed, same as function_infos
            func_index_map.push(all_funcs_idx);
            all_funcs_idx
        } else {
            // User function or no cache: push to function_infos, map to new index
            let idx = function_infos.len();
            func_index_map.push(idx);

            // Preserve original JuliaTypes for type parameter binding
            let param_julia_types: Vec<JuliaType> =
                params.iter().map(|(_, jt)| jt.clone()).collect();

            // For nested functions, use qualified name (parent#nested) to avoid collisions
            // when multiple parent functions have nested functions with the same name (Issue #1743)
            let function_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                format!("{}#{}", parent, func.name)
            } else {
                func.name.clone()
            };

            function_infos.push(FunctionInfo {
                name: function_name,
                params: vm_params,
                kwparams: vm_kwparams,
                entry: 0,
                return_type,
                type_params: func.type_params.clone(),
                param_julia_types,
                code_start: 0, // Will be set during compilation
                code_end: 0,   // Will be set during compilation
                slot_names: Vec::new(),
                local_slot_count: 0,
                param_slots: Vec::new(),
                vararg_param_index,
                vararg_fixed_count,
            });

            // Register function index for Stmt::FunctionDef lookups
            // Use qualified name for nested functions to avoid collisions (Issue #1743)
            let registration_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                // This is a nested function - use qualified name
                format!("{}#{}", parent, func.name)
            } else {
                // Top-level or module function - use original name
                func.name.clone()
            };
            shared_ctx.function_indices.insert(registration_name, idx);

            global_index += 1;
            idx
        };

        // Lazy AoT: Register function if it needs specialization
        // This must be done for ALL functions (including Base when using cache)
        // because cached bytecode may contain CallSpecialized instructions
        // Lazy AoT specialization enabled for:
        // - Base functions: enabled
        // - User functions: enabled
        // - Stdlib modules: enabled (Statistics, etc.)
        // - Core module: DISABLED (intrinsic wrappers like add_int)
        let is_specializable = if let Some(path) = module_path {
            // Module functions: enable for Stdlib, disable for Core
            path != "Core" && !path.starts_with("Core.")
        } else {
            // Non-module functions (Base + User): all enabled
            true
        };
        if is_specializable && needs_specialization(func) {
            let spec_idx = specializable_functions.len();
            specializable_functions.push(SpecializableFunction {
                ir: (*func).clone(),
                name: func.name.clone(),
                fallback_index: func_info_idx,
            });
            // Map function global_index to specializable index
            shared_ctx.spec_func_mapping.insert(func_info_idx, spec_idx);
        }
    }

    // Debug assertion: verify cache alignment after function merging (Issue #2726).
    // When using precompiled cache, all_functions[i] and function_infos[i] must have the same
    // name for all Base functions. A mismatch indicates that exact signature matching in
    // base function filtering has regressed, which would cause Call instructions in cached
    // bytecode to invoke the wrong function.
    #[cfg(debug_assertions)]
    if precompiled_base.is_some() {
        for i in 0..base_function_count
            .min(all_functions.len())
            .min(function_infos.len())
        {
            let all_func_name = &all_functions[i].0.name;
            let info_name = &function_infos[i].name;
            debug_assert_eq!(
                all_func_name, info_name,
                "Cache alignment mismatch at index {}: all_functions has '{}' but function_infos has '{}'. \
                 Base function filtering must use exact signature matching (Issue #2726).",
                i, all_func_name, info_name
            );
        }
    }

    // Collect inner constructors from struct definitions (both top-level and module structs)
    // These are registered with the struct name, allowing Point(x, y) to call the inner constructor
    struct InnerCtorInfo {
        struct_name: String,
        type_id: usize,
        ctor: crate::ir::core::InnerConstructor,
        func_info_idx: usize, // Index in function_infos where this ctor is registered
    }
    let mut inner_ctors: Vec<InnerCtorInfo> = Vec::new();

    // Use all_structs to include module structs (e.g., Dates.Date, Dates.DateTime)
    for (struct_def, _module_path) in &all_structs {
        if struct_def.inner_constructors.is_empty() {
            continue;
        }

        // Always add struct name to imported_functions (needed for name resolution)
        imported_functions.insert(struct_def.name.clone());

        // When using cache, skip inner constructors that are already in cache
        // (i.e., Base struct inner constructors). User-defined inner constructors
        // need to be registered even when using cache.
        let skip_this_struct = if precompiled_base.is_some() {
            // Check if this struct's constructor is already in cached method_tables
            // If so, it's a Base struct and we can skip
            method_tables
                .get(&struct_def.name)
                .map(|t| !t.methods.is_empty())
                .unwrap_or(false)
        } else {
            false
        };
        if skip_this_struct {
            continue;
        }

        // Get the type_id for this struct (or handle parametric struct)
        // Use short name since both short and qualified names are registered in struct_table
        let (type_id, is_parametric) =
            if let Some(info) = shared_ctx.struct_table.get(&struct_def.name) {
                (info.type_id, false)
            } else if shared_ctx.parametric_structs.contains_key(&struct_def.name) {
                // Parametric struct: use placeholder type_id, actual type resolved at call site
                (0, true)
            } else {
                continue;
            };

        for ctor in &struct_def.inner_constructors {
            let table = method_tables
                .entry(struct_def.name.clone())
                .or_insert_with(|| MethodTable::new(struct_def.name.clone()));

            // Add struct name to imported_functions immediately when registering inner constructor
            imported_functions.insert(struct_def.name.clone());

            let params: Vec<(String, JuliaType)> = ctor
                .params
                .iter()
                .map(|p| (p.name.clone(), p.effective_type()))
                .collect();

            let vm_params: Vec<(String, ValueType)> = params
                .iter()
                .map(|(name, jt)| {
                    (
                        name.clone(),
                        julia_type_to_value_type_with_table(jt, &shared_ctx.struct_table),
                    )
                })
                .collect();

            // Inner constructors return the struct type
            // For parametric structs, use Any since actual type is determined at call site
            let return_type = if is_parametric {
                ValueType::Any
            } else {
                ValueType::Struct(type_id)
            };

            // Preserve original JuliaTypes for type parameter binding (before params is moved)
            let param_julia_types: Vec<JuliaType> =
                params.iter().map(|(_, jt)| jt.clone()).collect();

            // Use type params from the inner constructor's where clause
            let ctor_type_params: Vec<TypeParam> = ctor.type_params.clone();

            let method_index = table.methods.len();
            table.add_method(MethodSig {
                _method_index: method_index,
                global_index,
                params,
                return_type: return_type.clone(),
                return_julia_type: None, // Inner constructors return structs, not parametric tuples
                is_base_extension: false,
                type_params: ctor_type_params.clone(),
                vararg_param_index: None, // Inner constructors don't have varargs
                vararg_fixed_count: None,
            });

            // Record the index where this inner constructor will be stored
            let func_info_idx = function_infos.len();

            function_infos.push(FunctionInfo {
                name: struct_def.name.clone(),
                params: vm_params,
                kwparams: vec![],
                entry: 0,
                return_type,
                type_params: ctor_type_params,
                param_julia_types,
                code_start: 0, // Will be set during compilation
                code_end: 0,   // Will be set during compilation
                slot_names: Vec::new(),
                local_slot_count: 0,
                param_slots: Vec::new(),
                vararg_param_index: None, // Inner constructors don't have varargs
                vararg_fixed_count: None,
            });

            inner_ctors.push(InnerCtorInfo {
                struct_name: struct_def.name.clone(),
                type_id,
                ctor: ctor.clone(),
                func_info_idx,
            });

            global_index += 1;
        }
    }
    // Also add struct names to imported_functions so they can be called
    // Use all_structs to include module structs
    for (struct_def, _module_path) in &all_structs {
        if !struct_def.inner_constructors.is_empty() {
            imported_functions.insert(struct_def.name.clone());
        }
    }

    // Populate struct_parents on all method tables for abstract dispatch tie-breaking (Issue #3144).
    // Build a map from concrete struct name to its declared parent abstract type.
    // This enables `dispatch()` to correctly prefer f(::MotorVehicle) over f(::NonMotorVehicle)
    // when the argument is Car where `struct Car <: MotorVehicle`.
    {
        let struct_parent_map: HashMap<String, Option<String>> = shared_ctx
            .struct_defs
            .iter()
            .map(|def| {
                // Strip type parameters from struct name (e.g., "Complex{Float64}" -> "Complex")
                let base_name = def.name.split('{').next().unwrap_or(&def.name);
                (base_name.to_string(), def.parent_type.clone())
            })
            .collect();

        for table in method_tables.values_mut() {
            table.struct_parents = struct_parent_map.clone();
        }
    }

    // Pre-analyze closure captures for lambda functions defined at module level (Issue #2358)
    //
    // Lambda functions (e.g., `f = () -> x + 1`) in @testset or other module-level blocks
    // are lifted to top-level functions named __lambda_N. They need to capture variables
    // from the outer scope. This must be done BEFORE the function compilation loop.
    //
    // First, collect the module-level locals to know what variables are available.
    {
        let mut main_locals: HashMap<String, ValueType> = HashMap::new();
        let mut main_mixed_type_vars: HashSet<String> = HashSet::new();
        let protected: HashSet<String> = HashSet::new();
        collect_local_types_with_mixed_tracking(
            &program.main.stmts,
            &mut main_locals,
            &protected,
            &shared_ctx.struct_table,
            &shared_ctx.global_types,
            &mut main_mixed_type_vars,
        );

        // Now analyze each __lambda_N function to see if it captures variables
        for func in &program.functions {
            if func.name.starts_with("__lambda_") {
                let outer_scope_vars: HashSet<String> = main_locals.keys().cloned().collect();
                let free_vars = analyze_free_variables(func, &outer_scope_vars);
                if !free_vars.is_empty() {
                    shared_ctx
                        .closure_captures
                        .insert(func.name.clone(), free_vars);
                }
            }
        }
    }

    // Compile each function
    // When using cache, copy the entire bytecode from cache first
    // This ensures all Base functions (including those not in all_functions) are available
    let (mut code, mut reused_base): (Vec<Instr>, Vec<bool>) =
        if let Some(base_cache) = precompiled_base {
            // Copy all bytecode from cache
            let cached_code = base_cache.code.clone();
            // Mark all cached functions as reused
            let reused = vec![true; function_infos.len()];
            (cached_code, reused)
        } else {
            (Vec::new(), vec![false; function_infos.len()])
        };

    for (idx, (func, module_path)) in all_functions.iter().enumerate() {
        // Map all_functions index to function_infos index
        let func_info_idx = func_index_map[idx];

        // When using cache, check if this function already has bytecode from cache
        // A function has valid cache bytecode if its code_start != code_end
        if precompiled_base.is_some() && func_info_idx < cached_base_len {
            let fi = &function_infos[func_info_idx];
            if fi.code_start != fi.code_end {
                // Function has valid bytecode from cache, skip compilation
                continue;
            }
        }

        let entry = code.len();
        function_infos[func_info_idx].entry = entry;
        reused_base[func_info_idx] = false; // This is a user function, not reused from cache

        let mut function_imports = imported_functions.clone();
        if let Some(module_path) = module_path {
            if let Some(module_funcs) = module_functions.get(module_path) {
                function_imports.extend(module_funcs.iter().cloned());
            }
            if let Some(module_imports) = module_imports_map.get(module_path) {
                function_imports.extend(module_imports.iter().cloned());
            }
        }

        // Check if this function is a closure with captured variables
        // Clone the captures before creating the compiler (to avoid borrow conflicts)
        //
        // For nested functions, closure_captures uses qualified names like "parent#nested"
        // We use func_idx_to_parent to find the exact parent for this function index,
        // which allows disambiguating between multiple nested functions with the same name
        // from different parents (Issue #1743).
        let closure_captures = if let Some(parent) = func_idx_to_parent.get(&idx) {
            // This is a nested function - look up by qualified name
            let qualified_name = format!("{}#{}", parent, func.name);
            shared_ctx
                .closure_captures
                .get(&qualified_name)
                .cloned()
                .unwrap_or_default()
        } else {
            // Top-level or module function - look up by simple name
            shared_ctx
                .closure_captures
                .get(&func.name)
                .cloned()
                .unwrap_or_default()
        };

        let mut compiler = CoreCompiler::new_for_function(
            &method_tables,
            &module_functions,
            &module_exports,
            &function_imports,
            &usings_set,
            &mut shared_ctx,
            &abstract_type_names,
            &module_constants,
        );

        // Set captured_vars so that load_local emits LoadCaptured for those variables
        compiler.captured_vars = closure_captures;

        // Set the current function name for nested function disambiguation
        // For nested functions, use the qualified name (parent#nested) so that
        // deeper nesting levels can build the full qualified path (Issue #1744)
        let current_func_name = if let Some(parent) = func_idx_to_parent.get(&idx) {
            format!("{}#{}", parent, func.name)
        } else {
            func.name.clone()
        };
        compiler.current_function_name = Some(current_func_name);

        // Set module path for resolving unqualified struct names inside module functions
        compiler.current_module_path = module_path.clone();

        // Set type parameters from where clause for type binding support
        compiler.current_type_params = func.type_params.clone();
        compiler.current_type_param_index = func
            .type_params
            .iter()
            .enumerate()
            .map(|(i, tp)| (tp.name.clone(), i))
            .collect();

        // Collect type parameter names from the function's where clause
        let func_type_param_names: HashSet<&str> =
            func.type_params.iter().map(|tp| tp.name.as_str()).collect();

        // Detect Val{N} patterns and mark N as a value parameter
        // For parameters like ::Val{N} where N, N should be treated as I64, not DataType
        for param in &func.params {
            if let JuliaType::Struct(type_name) = param.effective_type() {
                if type_name.starts_with("Val{") && type_name.ends_with("}") {
                    // Extract the type argument (e.g., "N" from "Val{N}")
                    let type_arg = &type_name[4..type_name.len() - 1];
                    // If this type arg is a where clause type parameter, it's a value parameter
                    if func_type_param_names.contains(type_arg) {
                        compiler.val_type_params.insert(type_arg.to_string());
                    }
                }
            }
        }

        // Set up parameter types in locals
        for param in &func.params {
            let param_ty = param.effective_type();
            // Ensure parametric struct instantiations exist (e.g., Complex{Float64})
            if let JuliaType::Struct(name) = &param_ty {
                if name.contains('{') && !compiler.shared_ctx.struct_table.contains_key(name) {
                    // Parse type arguments and create instantiation
                    if let Some(brace_idx) = name.find('{') {
                        let base_name = &name[..brace_idx];
                        let type_args_str = &name[brace_idx + 1..name.len() - 1];

                        // Check if any type arg is a where clause type parameter
                        let type_arg_names: Vec<&str> =
                            type_args_str.split(',').map(|s| s.trim()).collect();
                        let has_type_param = type_arg_names
                            .iter()
                            .any(|arg| func_type_param_names.contains(arg));

                        // Skip instantiation if any type arg is a where clause type parameter
                        if !has_type_param {
                            let type_args: Vec<TypeExpr> = type_arg_names
                                .iter()
                                .map(|s| TypeExpr::from_name(s, &[]))
                                .collect();
                            let _ = compiler
                                .shared_ctx
                                .resolve_instantiation_with_type_expr(base_name, &type_args);
                        }
                    }
                }
            }
            let vt = compiler.julia_type_to_value_type_with_ctx(&param_ty);
            compiler.locals.insert(param.name.clone(), vt.clone());
            // Track parameters with narrow integer types in julia_type_locals
            // so that infer_julia_type returns the precise type (e.g., Int32)
            // instead of Int64 (which ValueType::I64 maps to).
            // This is needed for correct compile-time dispatch of calls like
            // gcd(num, den) where num::Int32.
            if param_ty.is_narrow_integer() {
                compiler
                    .julia_type_locals
                    .insert(param.name.clone(), param_ty.clone());
            }
            // Track parameters with TypeVar type annotations (e.g., x::T where T<:Integer)
            // Store the upper bound type in julia_type_locals so that variable references
            // resolve to the bound type for proper dispatch (Issue #2556)
            if let JuliaType::TypeVar(_, Some(bound_name)) = &param_ty {
                if let Some(bound_type) = JuliaType::from_name(bound_name) {
                    compiler
                        .julia_type_locals
                        .insert(param.name.clone(), bound_type.clone());
                    // Also track abstract numeric bounds for runtime dispatch
                    if bound_type.is_abstract_numeric() {
                        compiler.abstract_numeric_params.insert(param.name.clone());
                    }
                }
            }
            // Track parameters with Any type - these should preserve Any on reassignment
            if matches!(param_ty, JuliaType::Any) {
                compiler.any_params.insert(param.name.clone());
            }
            // Track parameters with abstract numeric type annotations (Number, Real, etc.)
            // Binary operations on these must use runtime dispatch (Issue #2498)
            if param_ty.is_abstract_numeric() {
                compiler.abstract_numeric_params.insert(param.name.clone());
            }
        }

        // Set up kwparam types in locals
        // For varargs kwargs (kwargs...), type is always NamedTuple
        // For required kwargs (Undef default), use type annotation if available
        // For kwargs with `nothing` default, use Any since they can receive any type at runtime
        for kwparam in &func.kwparams {
            let vt = if kwparam.is_varargs {
                // Varargs kwargs collects all remaining kwargs as Pairs (Julia's Base.Pairs)
                ValueType::Pairs
            } else {
                let is_required = is_required_kwarg(&kwparam.default);
                if is_required {
                    // Required kwarg - use type annotation if available
                    kwparam
                        .type_annotation
                        .as_ref()
                        .map(|jt| {
                            julia_type_to_value_type_with_table(
                                jt,
                                &compiler.shared_ctx.struct_table,
                            )
                        })
                        .unwrap_or(ValueType::Any)
                } else {
                    let vt = infer_default_type(&kwparam.default);
                    if vt == ValueType::Nothing {
                        ValueType::Any
                    } else {
                        vt
                    }
                }
            };
            compiler.locals.insert(kwparam.name.clone(), vt);
        }

        // Register type parameters from where clause as DataType locals
        // This enables T(x) calls where T is a type parameter: function f(x::T) where T; T(1); end
        for tp in &func.type_params {
            // Skip Val{N} value parameters - they are I64, not DataType
            if !compiler.val_type_params.contains(&tp.name) {
                compiler.locals.insert(tp.name.clone(), ValueType::DataType);
            }
        }

        // Pre-populate locals with inferred types to ensure consistent type usage
        // This prevents bugs where a variable is first assigned as I64 then used as F64
        // Protect function parameters (and kwargs) from being overwritten by local assignments
        // This fixes the bug where parameter reassignment (e.g., a = abs(a)) causes type mismatch
        let protected: HashSet<String> = func
            .params
            .iter()
            .map(|p| p.name.clone())
            .chain(func.kwparams.iter().map(|k| k.name.clone()))
            .collect();
        collect_local_types_with_mixed_tracking(
            &func.body.stmts,
            &mut compiler.locals,
            &protected,
            &compiler.shared_ctx.struct_table,
            &compiler.shared_ctx.global_types,
            &mut compiler.mixed_type_vars,
        );

        // Compile function body with implicit return handling
        // In Julia, the last expression in a function is its return value
        compiler.compile_function_body(
            &func.body,
            function_infos[func_info_idx].return_type.clone(),
        )?;
        // Patch @goto jumps after function body compilation
        compiler.patch_goto_jumps()?;

        let code_start = entry;
        let mut func_code = compiler.code;
        relocate_jumps(&mut func_code, 0, entry);
        code.extend(func_code);
        let code_end = code.len();

        // Update function boundaries for future caching
        function_infos[func_info_idx].code_start = code_start;
        function_infos[func_info_idx].code_end = code_end;
    }

    // Compile inner constructors
    // These run with current_struct_type_id set so new() creates the correct struct type
    for ctor_info in inner_ctors.iter() {
        let entry = code.len();
        let func_info_idx = ctor_info.func_info_idx;
        function_infos[func_info_idx].entry = entry;

        let mut compiler = CoreCompiler::new_for_function(
            &method_tables,
            &module_functions,
            &module_exports,
            &imported_functions,
            &usings_set,
            &mut shared_ctx,
            &abstract_type_names,
            &module_constants,
        );

        // Set current_struct_type_id so new() creates the correct struct type
        compiler.current_struct_type_id = Some(ctor_info.type_id);

        // For parametric structs (type_id=0), set the base name for dynamic struct creation
        if ctor_info.type_id == 0 {
            compiler.current_parametric_struct_name = Some(ctor_info.struct_name.clone());
        }

        // Set type parameters from the constructor's where clause (e.g., where T)
        compiler.current_type_params = ctor_info.ctor.type_params.clone();
        compiler.current_type_param_index = ctor_info
            .ctor
            .type_params
            .iter()
            .enumerate()
            .map(|(i, tp)| (tp.name.clone(), i))
            .collect();

        // Set up parameter types in locals
        for param in &ctor_info.ctor.params {
            let param_ty = param.effective_type();
            let vt = compiler.julia_type_to_value_type_with_ctx(&param_ty);
            compiler.locals.insert(param.name.clone(), vt);
            // Track parameters with Any type - these should preserve Any on reassignment
            if matches!(param_ty, JuliaType::Any) {
                compiler.any_params.insert(param.name.clone());
            }
            // Track parameters with abstract numeric type annotations (Issue #2498)
            if param_ty.is_abstract_numeric() {
                compiler.abstract_numeric_params.insert(param.name.clone());
            }
        }

        // Register type parameters from constructor's where clause as DataType locals
        // This enables T(x) calls inside inner constructors: function Foo{T}(x) where T; T(1); end
        for tp in &ctor_info.ctor.type_params {
            compiler.locals.insert(tp.name.clone(), ValueType::DataType);
        }

        // Protect constructor parameters from being overwritten by local assignments
        // This fixes the bug where parameter reassignment (e.g., num = div(num, g)) causes type mismatch
        let protected: HashSet<String> = ctor_info
            .ctor
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect();
        collect_local_types_with_mixed_tracking(
            &ctor_info.ctor.body.stmts,
            &mut compiler.locals,
            &protected,
            &compiler.shared_ctx.struct_table,
            &compiler.shared_ctx.global_types,
            &mut compiler.mixed_type_vars,
        );

        // Compile constructor body
        let return_type = ValueType::Struct(ctor_info.type_id);
        compiler.compile_function_body(&ctor_info.ctor.body, return_type)?;
        // Patch @goto jumps after constructor body compilation
        compiler.patch_goto_jumps()?;

        let code_start = entry;
        let mut func_code = compiler.code;
        relocate_jumps(&mut func_code, 0, entry);
        code.extend(func_code);
        let code_end = code.len();

        // Update constructor function boundaries
        function_infos[func_info_idx].code_start = code_start;
        function_infos[func_info_idx].code_end = code_end;

        // Mark this inner constructor as not reused from cache (needs slot transformation)
        reused_base[func_info_idx] = false;
    }

    // Record where modules start (this will be the entry point if there are modules)
    let modules_entry = code.len();

    // Compile modules (execute their bodies before main)
    for module in &all_modules {
        let module_offset = code.len();

        // Create module-local imported functions set: includes all functions defined in this module
        // and functions imported via `using` statements in this module
        let mut module_imported_functions = imported_functions.clone();
        for func in &module.functions {
            module_imported_functions.insert(func.name.clone());
        }

        // Add functions imported via module-local using statements
        if let Some(module_imports) = module_imports_map.get(&module.name) {
            module_imported_functions.extend(module_imports.iter().cloned());
        }

        let mut module_compiler = CoreCompiler::new(
            &method_tables,
            &module_functions,
            &module_exports,
            &module_imported_functions,
            &usings_set,
            &mut shared_ctx,
            &abstract_type_names,
            &module_constants,
        );

        // Set module path for qualified constant storage
        module_compiler.current_module_path = Some(module.name.clone());

        // Compile module body
        module_compiler.compile_block(&module.body)?;

        // After compiling the module body, create a ModuleValue and store it
        // This makes the module accessible as a variable (e.g., TestMod)
        module_compiler.emit(Instr::PushModule(
            module.name.clone(),
            module.exports.clone(),
            module.publics.clone(),
        ));
        module_compiler.emit(Instr::StoreAny(module.name.clone()));

        // Don't emit ReturnUnit - let execution flow through to next module or main

        // Patch @goto jumps after module body compilation
        module_compiler.patch_goto_jumps()?;

        let mut module_code = module_compiler.code;
        relocate_jumps(&mut module_code, 0, module_offset);
        code.extend(module_code);
    }

    // Compile main block
    let main_entry = code.len();
    // Entry point: start at modules if there are any, otherwise at main
    let entry = if !all_modules.is_empty() {
        modules_entry
    } else {
        main_entry
    };
    let mut main_compiler = CoreCompiler::new(
        &method_tables,
        &module_functions,
        &module_exports,
        &imported_functions,
        &usings_set,
        &mut shared_ctx,
        &abstract_type_names,
        &module_constants,
    );

    // Pre-populate locals with inferred types to ensure consistent type usage
    // This prevents bugs where a variable is first assigned as I64 then used as F64
    // Also track mixed-type variables for proper dynamic typing at top-level
    let stmts = &program.main.stmts;
    let protected: HashSet<String> = HashSet::new();
    collect_local_types_with_mixed_tracking(
        stmts,
        &mut main_compiler.locals,
        &protected,
        &main_compiler.shared_ctx.struct_table,
        &main_compiler.shared_ctx.global_types,
        &mut main_compiler.mixed_type_vars,
    );

    // Compile all statements except the last one
    if !stmts.is_empty() {
        for stmt in &stmts[..stmts.len() - 1] {
            main_compiler.compile_stmt(stmt)?;
        }

        // For the last statement, if it's an expression, return its value
        // In Julia, assignment is also an expression that returns the assigned value
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Expr { expr, .. } => {
                let ty = main_compiler.compile_expr(expr)?;
                main_compiler.emit_return_for_type(ty);
            }
            // Assignment as last statement returns the assigned value (Julia semantics)
            Stmt::Assign { var, value, .. } => {
                // Check for wider type as in compile_stmt
                let target_ty = main_compiler.locals.get(var).cloned();
                let ty = main_compiler.compile_expr(value)?;

                // Handle widening for consistency with compile_stmt
                // For mixed-type variables, use dynamic typing (don't convert I64 to F64)
                let is_mixed_type = main_compiler.mixed_type_vars.contains(var);
                let final_ty = match (target_ty, ty.clone()) {
                    // For mixed-type variables, preserve the actual type
                    (Some(ValueType::Any), ValueType::I64)
                    | (Some(ValueType::Any), ValueType::F64)
                        if is_mixed_type =>
                    {
                        ValueType::Any
                    }
                    (Some(ValueType::F64), ValueType::I64) if is_mixed_type => ty,
                    (Some(ValueType::I64), ValueType::F64) if is_mixed_type => ty,
                    // For non-mixed variables, apply widening
                    (Some(ValueType::F64), ValueType::I64) => {
                        main_compiler.emit(Instr::ToF64);
                        ValueType::F64
                    }
                    _ => ty,
                };

                // Duplicate the value before storing (for supported types)
                // For other types, store and then load back
                let needs_load_back = !matches!(final_ty, ValueType::I64 | ValueType::F64);

                if !needs_load_back {
                    // For I64 and F64, we have Dup instructions
                    let dup_instr = match final_ty {
                        ValueType::I64 => Instr::DupI64,
                        ValueType::F64 => Instr::DupF64,
                        _ => return err(format!("internal: unexpected type {:?} in Dup path", final_ty)),
                    };
                    main_compiler.emit(dup_instr);
                    main_compiler.store_local(var, final_ty.clone());
                } else {
                    // For other types, store first then load back
                    main_compiler.store_local(var, final_ty.clone());
                    main_compiler.load_local(var)?;
                }

                main_compiler.emit_return_for_type(final_ty);
            }
            other => {
                main_compiler.compile_stmt(other)?;
                main_compiler.emit(Instr::ReturnNothing);
            }
        }
    } else {
        main_compiler.emit(Instr::ReturnNothing);
    }

    // Patch @goto jumps after main code compilation
    main_compiler.patch_goto_jumps()?;

    let mut main_code = main_compiler.code;
    // Use main_entry (where main code actually starts) instead of entry (modules_entry)
    // for jump relocation. This ensures jumps point to correct addresses when modules exist.
    relocate_jumps(&mut main_code, 0, main_entry);
    code.extend(main_code);

    // Peephole optimization: fuse common instruction patterns
    // Cached functions are already optimized, so we protect them from re-optimization
    // and only optimize the non-cached portions (user functions + main code)
    let protected_ranges: Vec<(usize, usize)> = function_infos
        .iter()
        .enumerate()
        .filter_map(|(idx, func_info)| {
            if reused_base.get(idx).copied().unwrap_or(false) {
                Some((func_info.code_start, func_info.code_end))
            } else {
                None
            }
        })
        .collect();

    let (mut code, index_mapping) =
        peephole::optimize_with_protected_ranges(code, &protected_ranges);

    // Update all function boundaries and entry point after optimization
    // The index_mapping includes one extra entry for the end position
    for func_info in &mut function_infos {
        if func_info.code_start < index_mapping.len() {
            func_info.code_start = index_mapping[func_info.code_start];
        }
        if func_info.code_end < index_mapping.len() {
            func_info.code_end = index_mapping[func_info.code_end];
        }
        if func_info.entry < index_mapping.len() {
            func_info.entry = index_mapping[func_info.entry];
        }
    }

    // Update entry point
    let entry = if entry < index_mapping.len() {
        index_mapping[entry]
    } else {
        entry // Keep original if out of bounds (shouldn't happen)
    };

    for (idx, func_info) in function_infos.iter_mut().enumerate() {
        if reused_base[idx] {
            continue;
        }
        let code_start = func_info.code_start;
        let code_end = func_info.code_end;
        if code_start >= code_end || code_end > code.len() {
            continue;
        }
        let slot_info = build_slot_info(
            &func_info.params,
            &func_info.kwparams,
            &code[code_start..code_end],
        );
        slotize_code(&mut code[code_start..code_end], &slot_info.name_to_slot);
        func_info.slot_names = slot_info.slot_names;
        func_info.local_slot_count = func_info.slot_names.len();
        func_info.param_slots = slot_info.param_slots;
        for (kw, slot) in func_info
            .kwparams
            .iter_mut()
            .zip(slot_info.kwparam_slots.into_iter())
        {
            kw.slot = slot;
        }
    }

    let global_slot_info = if entry < code.len() {
        let slot_info = build_slot_info(&[], &[], &code[entry..]);
        slotize_code(&mut code[entry..], &slot_info.name_to_slot);
        slot_info
    } else {
        build_slot_info(&[], &[], &[])
    };
    let global_slot_names = global_slot_info.slot_names;
    let global_slot_count = global_slot_names.len();

    // Lazy AoT: Build RuntimeCompileContext for specialization
    let compile_context = if !specializable_functions.is_empty() {
        Some(RuntimeCompileContext {
            struct_table: shared_ctx.struct_table.clone(),
            struct_defs: shared_ctx.struct_defs.clone(),
            parametric_structs: shared_ctx.parametric_structs.clone(),
        })
    } else {
        None
    };

    let compiled = CompiledProgram {
        code,
        functions: function_infos,
        struct_defs: shared_ctx.struct_defs,
        abstract_types,
        show_methods,
        entry,
        specializable_functions,
        compile_context,
        base_function_count,
        global_slot_names,
        global_slot_count,
    };

    Ok((compiled, method_tables, shared_ctx.closure_captures))
}

// LoopContext, FinallyContext, CoreCompiler struct, impl, and type predicates
// are now in core_compiler.rs module
// Collection helpers are now in collect.rs module
// Utility functions are now in utils.rs module
