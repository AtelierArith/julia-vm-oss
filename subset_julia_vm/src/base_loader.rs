//! Base loader module - parses Base Julia source at startup.
//!
//! This module provides Base program loading using the unified lowering pipeline.
//! Also provides a global registry for Base macros that can be used by user code.

use crate::ir::core::Program;
use crate::lowering::{Lowering, MacroParamType, StoredMacroDef};
use crate::parser::Parser;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;

/// Global registry for Base macros.
/// These macros are defined in base/macros.jl and are available to all user code.
/// Stores a Vec of macro definitions for each name to support multiple arities.
static BASE_MACROS: Lazy<RwLock<HashMap<String, Vec<StoredMacroDef>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

fn base_macros_write(
) -> std::sync::RwLockWriteGuard<'static, HashMap<String, Vec<StoredMacroDef>>> {
    BASE_MACROS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn base_macros_read() -> std::sync::RwLockReadGuard<'static, HashMap<String, Vec<StoredMacroDef>>> {
    BASE_MACROS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Get the Base program containing rational, complex arithmetic, etc.
/// Returns a reference to the lazily-initialized Base program.
pub fn get_base_program() -> Option<&'static Program> {
    static BASE_PROGRAM: Lazy<Option<Program>> = Lazy::new(|| {
        let source = crate::base::get_base();

        // Parse using pure Rust parser
        let mut parser = Parser::new().ok()?;
        let parse_outcome = parser.parse(&source).ok()?;

        // Lower using unified Lowering
        let mut lowering = Lowering::new(&source);
        let program = lowering.lower(parse_outcome).ok()?;

        // Extract and store macros in the global registry
        register_base_macros(&program);
        Some(program)
    });

    BASE_PROGRAM.as_ref()
}

/// Register macros from the Base program into the global registry.
fn register_base_macros(program: &Program) {
    let mut registry = base_macros_write();
    for macro_def in &program.macros {
        // Default to Any type for all params (base macros don't have type annotations)
        let param_types = vec![MacroParamType::Any; macro_def.params.len()];
        let stored = StoredMacroDef {
            params: macro_def.params.clone(),
            param_types,
            has_varargs: macro_def.has_varargs,
            body: macro_def.body.clone(),
            span: macro_def.span,
        };
        registry
            .entry(macro_def.name.clone())
            .or_default()
            .push(stored);
    }
}

/// Check if a macro with the given name exists in the Base registry.
pub fn has_base_macro(name: &str) -> bool {
    // Ensure base program is loaded (which populates the registry)
    let _ = get_base_program();
    base_macros_read().contains_key(name)
}

/// Get a macro from the Base registry by name (returns first definition).
/// For macros with multiple arities, use get_base_macro_with_arity instead.
pub fn get_base_macro(name: &str) -> Option<StoredMacroDef> {
    // Ensure base program is loaded (which populates the registry)
    let _ = get_base_program();
    base_macros_read().get(name).and_then(|v| v.first()).cloned()
}

/// Get a macro from the Base registry by name and arity.
/// Returns the macro definition that matches the given number of arguments.
pub fn get_base_macro_with_arity(name: &str, arity: usize) -> Option<StoredMacroDef> {
    // Ensure base program is loaded (which populates the registry)
    let _ = get_base_program();
    let registry = base_macros_read();
    if let Some(defs) = registry.get(name) {
        // First, try to find an exact match
        for def in defs {
            if !def.has_varargs && def.params.len() == arity {
                return Some(def.clone());
            }
        }
        // Then, try varargs macros
        for def in defs {
            if def.has_varargs && arity >= def.params.len() - 1 {
                return Some(def.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_program_loads() {
        let program = get_base_program();
        assert!(program.is_some(), "Base program should load");
    }

    #[test]
    fn test_base_function_count() {
        let program = get_base_program().expect("Base program should load");
        println!("Base program has {} functions", program.functions.len());
        for (i, f) in program.functions.iter().enumerate() {
            if (675..=685).contains(&i) {
                println!("  [{:3}] {}", i, f.name);
            }
        }
        // Just print, don't assert anything specific
    }

    #[test]
    fn test_base_macros_registered() {
        let program = get_base_program().expect("Base program should load");
        println!("Base program has {} macros", program.macros.len());
        for m in &program.macros {
            println!("  - {}", m.name);
        }
        assert!(
            !program.macros.is_empty(),
            "Base program should have macros"
        );
    }

    #[test]
    fn test_has_inline_macro() {
        // This should return true because @inline is defined in base/macros.jl
        assert!(
            has_base_macro("inline"),
            "@inline macro should be registered"
        );
    }

    #[test]
    fn test_base_inline_lowering() {
        // Test that @inline from base/macros.jl can be lowered in user code
        let src = "result = @inline 40 + 2\nFloat64(result)";

        let mut parser = crate::parser::Parser::new().expect("parser");
        let outcome = parser.parse(src).expect("parse");

        let mut lowering = crate::lowering::Lowering::new(src);
        let result = lowering.lower(outcome);

        assert!(
            result.is_ok(),
            "Lowering should succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().main.stmts.len(), 2);
    }
}
