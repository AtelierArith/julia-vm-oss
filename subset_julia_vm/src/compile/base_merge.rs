//! Base program merging for the compiler.
//!
//! This module handles merging user programs with precompiled base functions
//! (rational.jl, complex.jl, etc.).

use std::collections::HashSet;

use crate::ir::core::Program;

/// Result of merging a user program with base functions.
/// Contains the merged program and the count of base functions (for specialization filtering).
#[derive(Debug)]
pub(super) struct MergedProgram {
    pub program: Program,
    /// Number of base functions at the start of program.functions.
    /// Functions with index < base_function_count are from Base and should NOT be specialized.
    pub base_function_count: usize,
}

/// Merge a user program with the precompiled base functions.
/// Base functions (rational, complex arithmetic, etc.) are added unless the user defines them.
///
/// IMPORTANT: For operator overloading (e.g., `function Base.:+(a::MyType, b::MyType)`),
/// we keep ALL base methods and add user methods. This is Julia semantics: defining a new
/// method for an existing function ADDS to the method table, it doesn't replace all methods.
pub(super) fn merge_with_precompiled_base(program: &Program) -> MergedProgram {
    // Get base program (contains rational.jl, complex.jl, etc.)
    let base_program = match crate::base_loader::get_base_program() {
        Some(p) => p.clone(),
        None => {
            return MergedProgram {
                program: program.clone(),
                base_function_count: 0,
            }
        }
    };

    {
        // Collect user-defined function signatures to detect exact duplicates
        // A function is a duplicate if it has the same name AND same parameter types
        let user_signatures: HashSet<String> = program
            .functions
            .iter()
            .map(|f| {
                let param_types: Vec<String> = f
                    .params
                    .iter()
                    .map(|p: &_| format!("{:?}", p.effective_type()))
                    .collect();
                format!("{}({})", f.name, param_types.join(","))
            })
            .collect();

        let user_struct_names: HashSet<String> =
            program.structs.iter().map(|s| s.name.clone()).collect();
        let user_abstract_names: HashSet<String> = program
            .abstract_types
            .iter()
            .map(|a| a.name.clone())
            .collect();

        // Filter base functions by exact signature to support multiple dispatch (Issue #2719).
        // User-defined methods only replace base methods with the SAME signature,
        // preserving all other overloads.
        let mut merged_functions: Vec<crate::ir::core::Function> = base_program
            .functions
            .into_iter()
            .filter(|f| {
                let param_types: Vec<String> = f
                    .params
                    .iter()
                    .map(|p: &_| format!("{:?}", p.effective_type()))
                    .collect();
                let sig = format!("{}({})", f.name, param_types.join(","));
                !user_signatures.contains(&sig)
            })
            .collect();

        let mut merged_structs: Vec<crate::ir::core::StructDef> = base_program
            .structs
            .into_iter()
            .filter(|s| !user_struct_names.contains(&s.name))
            .collect();
        let mut merged_abstract_types: Vec<crate::ir::core::AbstractTypeDef> = base_program
            .abstract_types
            .into_iter()
            .filter(|a| !user_abstract_names.contains(&a.name))
            .collect();

        // Count base functions BEFORE adding user functions
        // This tells the compiler which functions are from Base and should NOT be specialized
        let base_function_count = merged_functions.len();

        // Add user definitions
        merged_functions.extend(program.functions.clone());
        merged_structs.extend(program.structs.clone());
        merged_abstract_types.extend(program.abstract_types.clone());

        // Merge main blocks: base_program.main statements go first (defines globals like `im`)
        // then user program.main statements follow
        let mut merged_main_stmts = base_program.main.stmts.clone();
        merged_main_stmts.extend(program.main.stmts.clone());
        let merged_main = crate::ir::core::Block {
            stmts: merged_main_stmts,
            span: program.main.span,
        };

        MergedProgram {
            program: Program {
                functions: merged_functions,
                structs: merged_structs,
                abstract_types: merged_abstract_types,
                type_aliases: program.type_aliases.clone(),
                base_function_count,
                main: merged_main,
                modules: program.modules.clone(),
                usings: program.usings.clone(),
                macros: program.macros.clone(),
                enums: program.enums.clone(),
            },
            base_function_count,
        }
    }
}
