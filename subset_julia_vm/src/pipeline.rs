//! Pipeline logic for parsing and lowering Julia source code.
//!
//! This module handles the transformation pipeline:
//! Julia source → Parser → CST → Lowering → Core IR

use crate::error::{SyntaxError, UnsupportedFeature};
use crate::ir::core::Program;
use crate::julia::base;
use crate::loader::{LoadError, LoaderConfig, PackageLoader};
use crate::lowering::{Lowering, LoweringWithInclude};
use crate::parser::Parser;

use once_cell::sync::Lazy;
use std::path::PathBuf;

/// Error variants produced by the parse-and-lower pipeline.
#[derive(Debug)]
pub enum PipelineError {
    /// Source code failed to parse.
    Parse(SyntaxError),
    /// Lowering to Core IR failed due to an unsupported feature.
    Lower(UnsupportedFeature),
    /// Loading a stdlib/package referenced by `using` failed.
    Load(LoadError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Parse(e) => write!(f, "parse error: {}", e),
            PipelineError::Lower(e) => write!(f, "lowering error: {:?}", e),
            PipelineError::Load(e) => write!(f, "load error: {}", e),
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

/// Result of parsing and lowering Julia source code.
pub type PipelineResult = Result<Program, PipelineError>;

/// Cached prelude program (parsed and lowered once)
static PRELUDE_PROGRAM: Lazy<Option<Program>> = Lazy::new(|| {
    let prelude_src = base::get_prelude();
    parse_source(&prelude_src).ok()
});

/// Get the Base program (used by compile_core_program)
pub fn get_prelude_program() -> Option<&'static Program> {
    PRELUDE_PROGRAM.as_ref()
}

/// Parse source code without prelude merging (used for prelude itself)
pub fn parse_source(src: &str) -> PipelineResult {
    let mut parser = Parser::new().map_err(|e| {
        PipelineError::Parse(SyntaxError::parse_failed(format!(
            "Parser initialization failed: {}",
            e
        )))
    })?;

    let outcome = parser.parse(src).map_err(PipelineError::Parse)?;

    let mut lowering = Lowering::new(src);
    lowering.lower(outcome).map_err(PipelineError::Lower)
}

/// Parse and lower Julia source code using tree-sitter pipeline.
/// Merges prelude functions and structs with user code.
pub fn parse_and_lower(src: &str) -> PipelineResult {
    parse_and_lower_with_base_dir(src, None)
}

/// Parse and lower Julia source code with include support.
/// The base_dir is used to resolve relative paths in include() calls.
pub fn parse_and_lower_with_base_dir(src: &str, base_dir: Option<PathBuf>) -> PipelineResult {
    // Parse user source code with include support
    let mut user_program = parse_source_with_include(src, base_dir)?;

    // Merge prelude program (structs first, then functions)
    if let Some(prelude) = PRELUDE_PROGRAM.as_ref() {
        // Helper function to get method signature (name + parameter types)
        // This allows multiple dispatch - same name with different parameter types
        fn get_method_signature(func: &crate::ir::core::Function) -> String {
            let param_types: Vec<String> = func
                .params
                .iter()
                .map(|p| p.effective_type().to_string())
                .collect();
            format!("{}({})", func.name, param_types.join(", "))
        }

        // Collect user method signatures to avoid conflicts (for Base extensions)
        let user_method_sigs: std::collections::HashSet<_> = user_program
            .functions
            .iter()
            .map(get_method_signature)
            .collect();

        // Collect user struct names to avoid conflicts
        let user_struct_names: std::collections::HashSet<_> = user_program
            .structs
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        // Merge structs (prelude first, but skip if user defines same name)
        let mut all_structs: Vec<crate::ir::core::StructDef> = prelude
            .structs
            .iter()
            .filter(|s| !user_struct_names.contains(s.name.as_str()))
            .cloned()
            .collect();
        all_structs.extend(user_program.structs);
        user_program.structs = all_structs;

        // Merge functions:
        // Filter by exact signature to support multiple dispatch (Issue #2719).
        // User-defined methods only replace base methods with the SAME signature,
        // preserving all other overloads. This matches Julia semantics where
        // defining a new method adds to (or replaces an exact match in) the
        // method table, never removing unrelated overloads.
        let mut all_functions: Vec<crate::ir::core::Function> = prelude
            .functions
            .iter()
            .filter(|f| !user_method_sigs.contains(&get_method_signature(f)))
            .cloned()
            .collect();
        // Track base function count BEFORE adding user functions
        let base_function_count = all_functions.len();
        all_functions.extend(user_program.functions);
        user_program.functions = all_functions;
        user_program.base_function_count = base_function_count;

        // Merge abstract types (prelude first, skip if user defines same name)
        let user_abstract_type_names: std::collections::HashSet<_> = user_program
            .abstract_types
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        let mut all_abstract_types: Vec<crate::ir::core::AbstractTypeDef> = prelude
            .abstract_types
            .iter()
            .filter(|a| !user_abstract_type_names.contains(a.name.as_str()))
            .cloned()
            .collect();
        all_abstract_types.extend(user_program.abstract_types);
        user_program.abstract_types = all_abstract_types;

        // Merge main blocks: prelude main block first (defines globals like `im`, const arrays, etc.)
        // then user program main block follows.
        // This ensures prelude const definitions are available to all functions.
        let mut merged_main_stmts = prelude.main.stmts.clone();
        merged_main_stmts.extend(user_program.main.stmts);
        user_program.main = crate::ir::core::Block {
            stmts: merged_main_stmts,
            span: user_program.main.span,
        };
    }

    let existing_modules: std::collections::HashSet<String> = user_program
        .modules
        .iter()
        .map(|m| m.name.clone())
        .collect();
    let usings_to_load: Vec<crate::ir::core::UsingImport> = user_program
        .usings
        .iter()
        .filter(|u| !existing_modules.contains(&u.module))
        .cloned()
        .collect();

    // Load stdlib/packages referenced by `using` statements that are not defined inline.
    let mut package_loader = PackageLoader::new(LoaderConfig::from_env());
    let loaded_modules = package_loader
        .load_for_usings(&usings_to_load)
        .map_err(PipelineError::Load)?;

    for module in loaded_modules {
        if !existing_modules.contains(&module.name) {
            user_program.modules.push(module);
        }
    }

    Ok(user_program)
}

/// Parse source code with include support.
pub fn parse_source_with_include(src: &str, base_dir: Option<PathBuf>) -> PipelineResult {
    let mut parser = Parser::new().map_err(|e| {
        PipelineError::Parse(SyntaxError::parse_failed(format!(
            "Parser initialization failed: {}",
            e
        )))
    })?;

    let outcome = parser.parse(src).map_err(PipelineError::Parse)?;

    let mut lowering = LoweringWithInclude::with_base_dir(src, base_dir);
    lowering.lower(outcome).map_err(PipelineError::Lower)
}
