//! Stdlib loader for loading standard library modules.
//!
//! This module provides stdlib loading using the unified lowering pipeline.
//! Uses the pure Rust parser which works in WASM without tree-sitter.
//!
//! Also provides a global registry for stdlib macros that can be used by user code
//! after `using ModuleName` statements.

use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use std::sync::RwLock;

use crate::ir::core::{Module, Program, UsingImport};
use crate::lowering::{Lowering, MacroParamType, StoredMacroDef};
use crate::parser::Parser;
use crate::stdlib;

/// Global registry for stdlib macros.
/// Key format: "ModuleName::macro_name" (e.g., "Test::test")
/// These macros are available to user code after `using ModuleName`.
static STDLIB_MACROS: Lazy<RwLock<HashMap<String, StoredMacroDef>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Tracks which stdlib modules have had their macros loaded.
static LOADED_STDLIB_MODULES: Lazy<RwLock<HashSet<String>>> =
    Lazy::new(|| RwLock::new(HashSet::new()));

fn stdlib_macros_write() -> std::sync::RwLockWriteGuard<'static, HashMap<String, StoredMacroDef>> {
    STDLIB_MACROS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn stdlib_macros_read() -> std::sync::RwLockReadGuard<'static, HashMap<String, StoredMacroDef>> {
    STDLIB_MACROS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn loaded_stdlib_modules_write() -> std::sync::RwLockWriteGuard<'static, HashSet<String>> {
    LOADED_STDLIB_MODULES
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn loaded_stdlib_modules_read() -> std::sync::RwLockReadGuard<'static, HashSet<String>> {
    LOADED_STDLIB_MODULES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Register a macro from a stdlib module into the global registry.
fn register_stdlib_macro(module: &str, name: &str, def: StoredMacroDef) {
    let key = format!("{}::{}", module, name);
    stdlib_macros_write().insert(key, def);
}

/// Check if a macro exists in the given stdlib module.
pub fn has_stdlib_macro(module: &str, name: &str) -> bool {
    let key = format!("{}::{}", module, name);
    stdlib_macros_read().contains_key(&key)
}

/// Get a macro from the given stdlib module.
pub fn get_stdlib_macro(module: &str, name: &str) -> Option<StoredMacroDef> {
    let key = format!("{}::{}", module, name);
    stdlib_macros_read().get(&key).cloned()
}

/// Ensure a stdlib module's macros are loaded into the registry.
/// This is called during lowering when a `using ModuleName` is encountered.
/// It loads the module early and registers its macros so they can be expanded.
pub fn ensure_stdlib_macros_loaded(module_name: &str) {
    // Skip non-stdlib modules
    if !stdlib::is_stdlib_module(module_name) {
        return;
    }

    // Skip Base, Core, Main, Pkg (handled separately)
    if matches!(module_name, "Base" | "Core" | "Main" | "Pkg") {
        return;
    }

    // Check if already loaded
    {
        let loaded = loaded_stdlib_modules_read();
        if loaded.contains(module_name) {
            return;
        }
    }

    // Load the module
    if let Ok(module) = load_stdlib_module(module_name) {
        // Register macros from the module
        for macro_def in &module.macros {
            // Default to Any type for all params (stdlib macros don't have type annotations in IR)
            let param_types = vec![MacroParamType::Any; macro_def.params.len()];
            let stored = StoredMacroDef {
                params: macro_def.params.clone(),
                param_types,
                has_varargs: macro_def.has_varargs,
                body: macro_def.body.clone(),
                span: macro_def.span,
            };
            register_stdlib_macro(module_name, &macro_def.name, stored);
        }

        // Mark as loaded
        loaded_stdlib_modules_write().insert(module_name.to_string());
    }
}

/// Error type for stdlib loading
#[derive(Debug)]
pub enum StdlibLoadError {
    ModuleNotFound { module: String },
    ParseError { module: String, error: String },
    LowerError { module: String, error: String },
    InvalidPackageLayout { module: String, reason: String },
}

impl std::fmt::Display for StdlibLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StdlibLoadError::ModuleNotFound { module } => {
                write!(f, "stdlib module '{}' not found", module)
            }
            StdlibLoadError::ParseError { module, error } => {
                write!(f, "parse error in stdlib {}: {}", module, error)
            }
            StdlibLoadError::LowerError { module, error } => {
                write!(f, "lowering error in stdlib {}: {}", module, error)
            }
            StdlibLoadError::InvalidPackageLayout { module, reason } => {
                write!(f, "invalid layout for stdlib {}: {}", module, reason)
            }
        }
    }
}

impl std::error::Error for StdlibLoadError {}

/// Load stdlib modules for the given using imports.
/// Uses the pure Rust parser which works in WASM.
pub fn load_stdlib_modules(usings: &[UsingImport]) -> Vec<Module> {
    let mut loaded = Vec::new();
    let mut loaded_names = HashSet::new();

    for using in usings {
        if loaded_names.contains(&using.module) {
            continue;
        }

        // Skip non-stdlib modules
        if !stdlib::is_stdlib_module(&using.module) {
            continue;
        }

        // Skip Base, Core, Main, Pkg
        if matches!(using.module.as_str(), "Base" | "Core" | "Main" | "Pkg") {
            continue;
        }

        if let Ok(module) = load_stdlib_module(&using.module) {
            loaded_names.insert(using.module.clone());
            loaded.push(module);
        }
    }

    loaded
}

/// Load a single stdlib module by name.
fn load_stdlib_module(module_name: &str) -> Result<Module, StdlibLoadError> {
    // Get the stdlib source
    let source =
        stdlib::get_stdlib_module(module_name).ok_or_else(|| StdlibLoadError::ModuleNotFound {
            module: module_name.to_string(),
        })?;

    // Parse using pure Rust parser
    let mut parser = Parser::new().map_err(|e| StdlibLoadError::ParseError {
        module: module_name.to_string(),
        error: format!("{:?}", e),
    })?;

    let parse_outcome = parser
        .parse(source)
        .map_err(|e| StdlibLoadError::ParseError {
            module: module_name.to_string(),
            error: format!("{:?}", e),
        })?;

    // Lower using unified Lowering (same code path as tree-sitter)
    let mut lowering = Lowering::new(source);
    let program = lowering
        .lower(parse_outcome)
        .map_err(|e| StdlibLoadError::LowerError {
            module: module_name.to_string(),
            error: format!("{:?}", e),
        })?;

    // Extract the module from the program
    extract_module(module_name, program)
}

/// Extract the named module from a Program.
fn extract_module(module_name: &str, program: Program) -> Result<Module, StdlibLoadError> {
    // Find the module definition
    let mut matches: Vec<Module> = program
        .modules
        .into_iter()
        .filter(|m| m.name == module_name)
        .collect();

    if matches.is_empty() {
        return Err(StdlibLoadError::InvalidPackageLayout {
            module: module_name.to_string(),
            reason: format!("module '{}' not found in source", module_name),
        });
    }

    if matches.len() > 1 {
        return Err(StdlibLoadError::InvalidPackageLayout {
            module: module_name.to_string(),
            reason: "multiple modules with the same name found".to_string(),
        });
    }

    Ok(matches.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_statistics() {
        let module = load_stdlib_module("Statistics").expect("Should load Statistics");
        assert_eq!(module.name, "Statistics");
        // Check that some expected functions exist
        let func_names: Vec<_> = module.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"mean"), "Should have mean function");
        assert!(
            func_names.contains(&"median"),
            "Should have median function"
        );
        assert!(func_names.contains(&"var"), "Should have var function");
        assert!(func_names.contains(&"std"), "Should have std function");
    }

    #[test]
    fn test_load_for_usings() {
        let usings = vec![UsingImport {
            module: "Statistics".to_string(),
            is_relative: false,
            symbols: None,
            span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
        }];
        let modules = load_stdlib_modules(&usings);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "Statistics");
    }

    #[test]
    fn test_load_dates() {
        let module = load_stdlib_module("Dates").expect("Should load Dates");
        assert_eq!(module.name, "Dates");
        // Check that some expected functions exist
        let func_names: Vec<_> = module.functions.iter().map(|f| f.name.as_str()).collect();
        println!("Dates functions: {:?}", func_names);
        assert!(func_names.contains(&"value"), "Should have value function");
        assert!(
            func_names.contains(&"isleapyear"),
            "Should have isleapyear function"
        );

        // Check parameter types of value functions
        for func in module.functions.iter().filter(|f| f.name == "value") {
            println!(
                "value function: {:?}",
                func.params
                    .iter()
                    .map(|p| format!("{}: {:?}", p.name, p.type_annotation))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_load_test_module() {
        let module = load_stdlib_module("Test").expect("Should load Test");
        assert_eq!(module.name, "Test");
        // Check that macros are extracted
        let macro_names: Vec<_> = module.macros.iter().map(|m| m.name.as_str()).collect();
        assert!(macro_names.contains(&"test"), "Should have @test macro");
        assert!(
            macro_names.contains(&"testset"),
            "Should have @testset macro"
        );
        assert!(
            macro_names.contains(&"test_broken"),
            "Should have @test_broken macro"
        );
    }

    #[test]
    fn test_ensure_stdlib_macros_loaded() {
        // Call ensure_stdlib_macros_loaded for Test
        ensure_stdlib_macros_loaded("Test");

        // Check that macros are registered
        assert!(
            has_stdlib_macro("Test", "test"),
            "@test should be registered"
        );
        assert!(
            has_stdlib_macro("Test", "testset"),
            "@testset should be registered"
        );
    }
}
