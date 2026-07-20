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
use crate::lowering::{LambdaContext, Lowering, MacroHygieneInfo, MacroParamType, StoredMacroDef};
use crate::parser::Parser;
use crate::stdlib;

/// Global registry for stdlib macros.
/// Key format: "ModuleName::macro_name" (e.g., "Test::test")
/// These macros are available to user code after `using ModuleName`.
static STDLIB_MACROS: Lazy<RwLock<HashMap<String, StoredMacroDef>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Default)]
struct MacroLoadEntries {
    loaded: HashSet<String>,
    loading: HashSet<String>,
}

#[derive(Default)]
struct MacroLoadState {
    entries: RwLock<MacroLoadEntries>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacroLoadOutcome {
    AlreadyLoaded,
    Reentrant,
    Loaded,
}

struct MacroLoadingGuard<'a> {
    state: &'a MacroLoadState,
    module_name: &'a str,
    active: bool,
}

impl MacroLoadState {
    fn entries_write(&self) -> std::sync::RwLockWriteGuard<'_, MacroLoadEntries> {
        self.entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn ensure_loaded<E>(
        &self,
        module_name: &str,
        load_and_register: impl FnOnce() -> Result<(), E>,
    ) -> Result<MacroLoadOutcome, E> {
        {
            let mut entries = self.entries_write();
            if entries.loaded.contains(module_name) {
                return Ok(MacroLoadOutcome::AlreadyLoaded);
            }
            if !entries.loading.insert(module_name.to_string()) {
                return Ok(MacroLoadOutcome::Reentrant);
            }
        }

        let guard = MacroLoadingGuard {
            state: self,
            module_name,
            active: true,
        };
        load_and_register()?;
        guard.publish_loaded();
        Ok(MacroLoadOutcome::Loaded)
    }

    #[cfg(test)]
    fn is_loaded(&self, module_name: &str) -> bool {
        self.entries_write().loaded.contains(module_name)
    }

    #[cfg(test)]
    fn is_loading(&self, module_name: &str) -> bool {
        self.entries_write().loading.contains(module_name)
    }
}

impl MacroLoadingGuard<'_> {
    fn publish_loaded(mut self) {
        let mut entries = self.state.entries_write();
        entries.loaded.insert(self.module_name.to_string());
        entries.loading.remove(self.module_name);
        self.active = false;
    }
}

impl Drop for MacroLoadingGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.state.entries_write().loading.remove(self.module_name);
        }
    }
}

/// Stdlib and bundled packages share one transition implementation but retain
/// independent module states and macro registries.
static STDLIB_MACRO_LOAD_STATE: Lazy<MacroLoadState> = Lazy::new(MacroLoadState::default);

static BUNDLED_MACRO_LOAD_STATE: Lazy<MacroLoadState> = Lazy::new(MacroLoadState::default);

/// Registry for bundled-package macros (e.g. Plots' `@animate` / `@gif`).
///
/// Kept separate from [`STDLIB_MACROS`] because the two are expanded by different
/// engines: stdlib/Base macros use the template substitution path, whereas
/// bundled-package macros are expanded through the full `macro_runtime` path that
/// user-defined macros use (so they may build AST via `Expr(...)`, mutate `.args`,
/// etc.). Key format: "ModuleName::macro_name".
static BUNDLED_PACKAGE_MACROS: Lazy<RwLock<HashMap<String, StoredMacroDef>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

fn bundled_macros_write() -> std::sync::RwLockWriteGuard<'static, HashMap<String, StoredMacroDef>> {
    BUNDLED_PACKAGE_MACROS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn bundled_macros_read() -> std::sync::RwLockReadGuard<'static, HashMap<String, StoredMacroDef>> {
    BUNDLED_PACKAGE_MACROS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Check if a bundled-package macro exists in the given module.
pub fn has_bundled_package_macro(module: &str, name: &str) -> bool {
    bundled_macros_read().contains_key(&format!("{}::{}", module, name))
}

/// Get a bundled-package macro from the given module.
pub fn get_bundled_package_macro(module: &str, name: &str) -> Option<StoredMacroDef> {
    bundled_macros_read()
        .get(&format!("{}::{}", module, name))
        .cloned()
}

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

    // Issue #7735: a stdlib source can import its parent while the parent is
    // still lowering. The shared state machine suppresses that second scan.
    let _ = STDLIB_MACRO_LOAD_STATE.ensure_loaded(module_name, || -> Result<_, StdlibLoadError> {
        let module = load_stdlib_module(module_name)?;
        // Register macros from the module
        for macro_def in &module.macros {
            // Default to Any type for all params (stdlib macros don't have type annotations in IR)
            let param_types = vec![MacroParamType::Any; macro_def.params.len()];
            let stored = StoredMacroDef {
                params: macro_def.params.clone(),
                param_types,
                has_varargs: macro_def.has_varargs,
                body: macro_def.body.clone(),
                expansion_functions: vec![],
                expansion_structs: vec![],
                hygiene: None,
                span: macro_def.span,
            };
            register_stdlib_macro(module_name, &macro_def.name, stored);
        }
        Ok(())
    });
}

/// Ensure a bundled package's macros are registered so user code can expand them
/// after `using <Package>`.
///
/// Bundled packages (e.g. Plots) are not stdlib, so [`ensure_stdlib_macros_loaded`]
/// skips them. This mirrors that registration for the embedded package registry:
/// the package is loaded through the normal [`crate::loader::PackageLoader`] (which
/// resolves `include()` and populates `Module::macros`), and each macro is added to
/// the shared macro registry under the package name. Issue #6355: `@animate` /
/// `@gif` are defined in the Plots package and must be reachable through
/// `using Plots`, exactly like `@testset` is through `using Test`.
pub fn ensure_bundled_package_macros_loaded(module_name: &str) {
    if !crate::packages::is_bundled_package(module_name) {
        return;
    }

    // Issue #11141: cold package-source lowering can expand one of the
    // package's own macros. Only the outer scan registers the full surface.
    let _ = BUNDLED_MACRO_LOAD_STATE.ensure_loaded(
        module_name,
        || -> Result<_, crate::loader::LoadError> {
            let usings = vec![UsingImport {
                module: module_name.to_string(),
                is_import: false,
                is_relative: false,
                relative_level: 0,
                symbols: None,
                alias_bindings: Vec::new(),
                span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
            }];
            let mut loader =
                crate::loader::PackageLoader::new(crate::loader::LoaderConfig::from_env());
            let modules = loader.load_for_usings(&usings)?;
            for module in &modules {
                let mut members: HashSet<String> = HashSet::new();
                members.extend(
                    module
                        .functions
                        .iter()
                        .filter(|f| !f.is_base_extension)
                        .map(|f| f.name.clone()),
                );
                members.extend(module.structs.iter().map(|s| s.name.clone()));
                members.extend(module.abstract_types.iter().map(|a| a.name.clone()));
                members.extend(module.primitive_types.iter().map(|p| p.name.clone()));
                members.extend(module.type_aliases.iter().map(|t| t.name.clone()));
                let exports: HashSet<String> = module.exports.iter().cloned().collect();
                for macro_def in &module.macros {
                    // Bundled-package macros carry no IR type annotations, so every
                    // parameter defaults to `Any` (matching the stdlib path).
                    let param_types = vec![MacroParamType::Any; macro_def.params.len()];
                    let stored = StoredMacroDef {
                        params: macro_def.params.clone(),
                        param_types,
                        has_varargs: macro_def.has_varargs,
                        body: macro_def.body.clone(),
                        expansion_functions: module.functions.clone(),
                        expansion_structs: module.structs.clone(),
                        hygiene: Some(MacroHygieneInfo {
                            module: module.name.clone(),
                            members: members.clone(),
                            exports: exports.clone(),
                        }),
                        span: macro_def.span,
                    };
                    bundled_macros_write()
                        .insert(format!("{}::{}", module.name, macro_def.name), stored);
                }
            }
            Ok(())
        },
    );
}

/// Add bundled-package helper functions/types to a caller macro context before
/// expanding a macro from that package. The global bundled macro registry stores
/// `StoredMacroDef`s, but MacroTools-style macros call private helpers such as
/// `allbindings` while expanding (Issue #7535), so the expansion program must
/// include the defining module's compile-time surface too.
pub fn add_bundled_package_macro_context(module_name: &str, lambda_ctx: &LambdaContext) {
    if !crate::packages::is_bundled_package(module_name) {
        return;
    }

    let usings = vec![UsingImport {
        module: module_name.to_string(),
        is_import: false,
        is_relative: false,
        relative_level: 0,
        symbols: None,
        alias_bindings: Vec::new(),
        span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
    }];
    let mut loader = crate::loader::PackageLoader::new(crate::loader::LoaderConfig::from_env());
    let Ok(modules) = loader.load_for_usings(&usings) else {
        return;
    };

    for module in &modules {
        lambda_ctx.add_compile_time_functions(&module.functions);
        lambda_ctx.add_compile_time_structs(&module.structs);
        lambda_ctx.add_compile_time_abstract_types(&module.abstract_types);
        lambda_ctx.add_compile_time_primitive_types(&module.primitive_types);

        if module.macros.is_empty() {
            continue;
        }
        let mut members: HashSet<String> = HashSet::new();
        members.extend(
            module
                .functions
                .iter()
                .filter(|f| !f.is_base_extension)
                .map(|f| f.name.clone()),
        );
        members.extend(module.structs.iter().map(|s| s.name.clone()));
        members.extend(module.abstract_types.iter().map(|a| a.name.clone()));
        members.extend(module.primitive_types.iter().map(|p| p.name.clone()));
        members.extend(module.type_aliases.iter().map(|t| t.name.clone()));
        let exports: HashSet<String> = module.exports.iter().cloned().collect();
        for macro_def in &module.macros {
            lambda_ctx.register_module_macro_hygiene(
                &macro_def.name,
                &module.name,
                members.clone(),
                exports.clone(),
            );
        }
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

    // Lower using the same unified Lowering pipeline shared by every entry point
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
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
mod macro_load_state_tests_11145 {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn exercise_macro_load_state_machine(state: &MacroLoadState, namespace: &str) {
        let recursive_module = format!("{namespace}Recursive");
        let load_calls = Cell::new(0);
        let registered = RefCell::new(Vec::new());

        let outer = state.ensure_loaded(&recursive_module, || {
            load_calls.set(load_calls.get() + 1);
            assert!(state.is_loading(&recursive_module));
            assert!(!state.is_loaded(&recursive_module));

            let nested = state.ensure_loaded(&recursive_module, || {
                load_calls.set(load_calls.get() + 1);
                Ok::<(), &'static str>(())
            });
            assert_eq!(nested, Ok(MacroLoadOutcome::Reentrant));

            registered.borrow_mut().push("first_macro");
            assert!(
                !state.is_loaded(&recursive_module),
                "loaded must stay unpublished until the complete macro surface is registered"
            );
            registered.borrow_mut().push("second_macro");
            Ok::<(), &'static str>(())
        });

        assert_eq!(outer, Ok(MacroLoadOutcome::Loaded));
        assert_eq!(load_calls.get(), 1, "re-entry invoked a second load");
        assert_eq!(&*registered.borrow(), &["first_macro", "second_macro"]);
        assert!(state.is_loaded(&recursive_module));
        assert!(!state.is_loading(&recursive_module));

        let already_loaded = state.ensure_loaded(&recursive_module, || {
            load_calls.set(load_calls.get() + 1);
            Ok::<(), &'static str>(())
        });
        assert_eq!(already_loaded, Ok(MacroLoadOutcome::AlreadyLoaded));
        assert_eq!(load_calls.get(), 1, "loaded module invoked the callback");

        let retry_module = format!("{namespace}Retry");
        let failed = state.ensure_loaded(&retry_module, || Err::<(), _>("cold load failed"));
        assert_eq!(failed, Err("cold load failed"));
        assert!(!state.is_loaded(&retry_module));
        assert!(
            !state.is_loading(&retry_module),
            "failed load retained the loading marker and blocked retry"
        );
        let retried = state.ensure_loaded(&retry_module, || Ok::<(), &'static str>(()));
        assert_eq!(retried, Ok(MacroLoadOutcome::Loaded));

        let first_module = format!("{namespace}First");
        let second_module = format!("{namespace}Second");
        let order = RefCell::new(Vec::new());
        let first = state.ensure_loaded(&first_module, || {
            order.borrow_mut().push("first:start");
            let second = state.ensure_loaded(&second_module, || {
                order.borrow_mut().push("second");
                Ok::<(), &'static str>(())
            });
            assert_eq!(second, Ok(MacroLoadOutcome::Loaded));
            assert!(state.is_loaded(&second_module));
            assert!(!state.is_loaded(&first_module));
            order.borrow_mut().push("first:end");
            Ok::<(), &'static str>(())
        });
        assert_eq!(first, Ok(MacroLoadOutcome::Loaded));
        assert_eq!(&*order.borrow(), &["first:start", "second", "first:end"]);
    }

    #[test]
    fn test_macro_load_state_machines_cold_reentry_11145() {
        // Fresh local state and injected callbacks are intentional: persistent
        // and preload caches can make real macro registries warm, bypassing the
        // cold source-load re-entry that regressed in Issues #11141/#11132.
        exercise_macro_load_state_machine(&MacroLoadState::default(), "Stdlib");
        exercise_macro_load_state_machine(&MacroLoadState::default(), "Bundled");
    }
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
            is_import: false,
            is_relative: false,
            relative_level: 0,
            symbols: None,
            alias_bindings: Vec::new(),
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
        assert!(
            macro_names.contains(&"test_skip"),
            "Should have @test_skip macro (Issue #10350)"
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

    #[test]
    fn test_ensure_stdlib_macros_loaded_linear_algebra_no_recursion_7735() {
        // LinearAlgebra.LAPACK imports from ..LinearAlgebra while the parent
        // module is still being lowered. The macro scan must not recurse into
        // the same module until the stack overflows.
        ensure_stdlib_macros_loaded("LinearAlgebra");
        assert!(!has_stdlib_macro("LinearAlgebra", "test"));
    }
}
