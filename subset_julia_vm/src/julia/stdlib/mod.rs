//! Julia Standard Library implementations.
//!
//! This module provides Julia source code for standard library packages
//! that can be loaded with `using` statements.
//!
//! The structure mirrors Julia's stdlib:
//! - Statistics: Basic statistics functionality (mean, var, std, cor, cov, median, quantile)
//! - Test: Simple unit testing functionality (test, testset, assert)
//! - Random: Random number generation utilities (seed!)
//! - InteractiveUtils: Utilities for interactive use (versioninfo, supertypes)
//! - LinearAlgebra: Linear algebra operations (tr, dot, norm)
//! - Dates: Date and time handling
//! - Iterators: Iterator utilities (enumerate, zip, take, drop, etc.)
//! - Broadcast: Broadcasting operations (broadcast, broadcast!)
//! - Printf: Formatted printing (@printf, @sprintf)
//! - Base64: Base64 encoding and decoding (base64encode, base64decode)

use std::collections::HashMap;

/// Statistics module source code
pub const STATISTICS_JL: &str = include_str!("Statistics/src/Statistics.jl");
/// Statistics module Project.toml
pub const STATISTICS_PROJECT_TOML: &str = include_str!("Statistics/Project.toml");

/// Test module source code
pub const TEST_JL: &str = include_str!("Test/src/Test.jl");
/// Test module Project.toml
pub const TEST_PROJECT_TOML: &str = include_str!("Test/Project.toml");

/// Random module source code
pub const RANDOM_JL: &str = include_str!("Random/src/Random.jl");
/// Random module Project.toml
pub const RANDOM_PROJECT_TOML: &str = include_str!("Random/Project.toml");

/// InteractiveUtils module source code
pub const INTERACTIVE_UTILS_JL: &str = include_str!("InteractiveUtils/src/InteractiveUtils.jl");
/// InteractiveUtils module Project.toml
pub const INTERACTIVE_UTILS_PROJECT_TOML: &str = include_str!("InteractiveUtils/Project.toml");

/// Dates module source code
pub const DATES_JL: &str = include_str!("Dates/src/Dates.jl");
/// Dates module Project.toml
pub const DATES_PROJECT_TOML: &str = include_str!("Dates/Project.toml");

/// LinearAlgebra module source code
pub const LINEAR_ALGEBRA_JL: &str = include_str!("LinearAlgebra/src/LinearAlgebra.jl");
/// LinearAlgebra module Project.toml
pub const LINEAR_ALGEBRA_PROJECT_TOML: &str = include_str!("LinearAlgebra/Project.toml");

/// Iterators module source code
pub const ITERATORS_JL: &str = include_str!("Iterators/src/Iterators.jl");
/// Iterators module Project.toml
pub const ITERATORS_PROJECT_TOML: &str = include_str!("Iterators/Project.toml");

/// Broadcast module source code
pub const BROADCAST_JL: &str = include_str!("Broadcast/src/Broadcast.jl");
/// Broadcast module Project.toml
pub const BROADCAST_PROJECT_TOML: &str = include_str!("Broadcast/Project.toml");

/// Printf module source code
pub const PRINTF_JL: &str = include_str!("Printf/src/Printf.jl");
/// Printf module Project.toml
pub const PRINTF_PROJECT_TOML: &str = include_str!("Printf/Project.toml");

/// Base64 module source code
pub const BASE64_JL: &str = include_str!("Base64/src/Base64.jl");
/// Base64 module Project.toml
pub const BASE64_PROJECT_TOML: &str = include_str!("Base64/Project.toml");

/// Embedded stdlib package (Project.toml + source).
#[derive(Debug, Clone, Copy)]
pub struct StdlibPackage {
    pub project_toml: &'static str,
    pub source: &'static str,
}

/// Get an embedded stdlib package by name.
pub fn get_stdlib_package(name: &str) -> Option<StdlibPackage> {
    match name {
        "Statistics" => Some(StdlibPackage {
            project_toml: STATISTICS_PROJECT_TOML,
            source: STATISTICS_JL,
        }),
        "Test" => Some(StdlibPackage {
            project_toml: TEST_PROJECT_TOML,
            source: TEST_JL,
        }),
        "Random" => Some(StdlibPackage {
            project_toml: RANDOM_PROJECT_TOML,
            source: RANDOM_JL,
        }),
        "InteractiveUtils" => Some(StdlibPackage {
            project_toml: INTERACTIVE_UTILS_PROJECT_TOML,
            source: INTERACTIVE_UTILS_JL,
        }),
        "Dates" => Some(StdlibPackage {
            project_toml: DATES_PROJECT_TOML,
            source: DATES_JL,
        }),
        "LinearAlgebra" => Some(StdlibPackage {
            project_toml: LINEAR_ALGEBRA_PROJECT_TOML,
            source: LINEAR_ALGEBRA_JL,
        }),
        "Iterators" => Some(StdlibPackage {
            project_toml: ITERATORS_PROJECT_TOML,
            source: ITERATORS_JL,
        }),
        "Broadcast" => Some(StdlibPackage {
            project_toml: BROADCAST_PROJECT_TOML,
            source: BROADCAST_JL,
        }),
        "Printf" => Some(StdlibPackage {
            project_toml: PRINTF_PROJECT_TOML,
            source: PRINTF_JL,
        }),
        "Base64" => Some(StdlibPackage {
            project_toml: BASE64_PROJECT_TOML,
            source: BASE64_JL,
        }),
        _ => None,
    }
}

/// Get a map of all available stdlib modules and their source code.
pub fn get_stdlib_modules() -> HashMap<&'static str, &'static str> {
    let mut modules = HashMap::new();
    modules.insert("Statistics", STATISTICS_JL);
    modules.insert("Test", TEST_JL);
    modules.insert("Random", RANDOM_JL);
    modules.insert("InteractiveUtils", INTERACTIVE_UTILS_JL);
    modules.insert("Dates", DATES_JL);
    modules.insert("LinearAlgebra", LINEAR_ALGEBRA_JL);
    modules.insert("Iterators", ITERATORS_JL);
    modules.insert("Broadcast", BROADCAST_JL);
    modules.insert("Printf", PRINTF_JL);
    modules.insert("Base64", BASE64_JL);
    modules
}

/// Get the source code for a specific stdlib module by name.
pub fn get_stdlib_module(name: &str) -> Option<&'static str> {
    match name {
        "Statistics" => Some(STATISTICS_JL),
        "Test" => Some(TEST_JL),
        "Random" => Some(RANDOM_JL),
        "InteractiveUtils" => Some(INTERACTIVE_UTILS_JL),
        "Dates" => Some(DATES_JL),
        "LinearAlgebra" => Some(LINEAR_ALGEBRA_JL),
        "Iterators" => Some(ITERATORS_JL),
        "Broadcast" => Some(BROADCAST_JL),
        "Printf" => Some(PRINTF_JL),
        "Base64" => Some(BASE64_JL),
        _ => None,
    }
}

/// Check if a module is available in stdlib.
pub fn is_stdlib_module(name: &str) -> bool {
    matches!(
        name,
        "Statistics"
            | "Test"
            | "Random"
            | "InteractiveUtils"
            | "Dates"
            | "LinearAlgebra"
            | "Iterators"
            | "Broadcast"
            | "Printf"
            | "Base64"
    )
}

/// Get a list of all available stdlib module names.
pub fn available_modules() -> Vec<&'static str> {
    vec![
        "Statistics",
        "Test",
        "Random",
        "InteractiveUtils",
        "Dates",
        "LinearAlgebra",
        "Iterators",
        "Broadcast",
        "Printf",
        "Base64",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistics_module_exists() {
        assert!(!STATISTICS_JL.is_empty());
        assert!(STATISTICS_JL.contains("module Statistics"));
        assert!(STATISTICS_JL.contains("export"));
    }

    #[test]
    fn test_statistics_exports() {
        assert!(STATISTICS_JL.contains("function mean"));
        assert!(STATISTICS_JL.contains("function var"));
        assert!(STATISTICS_JL.contains("function std"));
        assert!(STATISTICS_JL.contains("function cov"));
        assert!(STATISTICS_JL.contains("function cor"));
        assert!(STATISTICS_JL.contains("function median"));
        assert!(STATISTICS_JL.contains("function quantile"));
    }

    #[test]
    fn test_statistics_additional_functions() {
        // Functions that exist in Julia's Statistics.jl
        assert!(STATISTICS_JL.contains("function varm"));
        assert!(STATISTICS_JL.contains("function stdm"));
        assert!(STATISTICS_JL.contains("function middle"));
    }

    #[test]
    fn test_statistics_julia_compatibility() {
        // Verify that non-Statistics.jl functions are NOT present
        // These belong to StatsBase.jl, not Statistics.jl
        assert!(!STATISTICS_JL.contains("function wmean"));
        assert!(!STATISTICS_JL.contains("function wvar"));
        assert!(!STATISTICS_JL.contains("function wstd"));
        assert!(!STATISTICS_JL.contains("function mode("));
        assert!(!STATISTICS_JL.contains("function skewness"));
        assert!(!STATISTICS_JL.contains("function kurtosis"));
        assert!(!STATISTICS_JL.contains("function iqr"));
        assert!(!STATISTICS_JL.contains("function zscore"));
    }

    #[test]
    fn test_get_stdlib_module() {
        assert!(get_stdlib_module("Statistics").is_some());
        assert!(get_stdlib_module("NonExistent").is_none());
    }

    #[test]
    fn test_is_stdlib_module() {
        assert!(is_stdlib_module("Statistics"));
        assert!(!is_stdlib_module("NonExistent"));
    }

    #[test]
    fn test_available_modules() {
        let modules = available_modules();
        assert!(modules.contains(&"Statistics"));
    }

    #[test]
    fn test_get_stdlib_modules_map() {
        let map = get_stdlib_modules();
        assert!(map.contains_key("Statistics"));
        assert!(map.contains_key("Test"));
        assert!(map.contains_key("Printf"));
    }

    // Test module tests
    // Julia's Test module provides @test and @testset macros.
    // In SubsetJuliaVM, these macros are implemented at the VM level.
    // Test.jl implements @test and @testset as Pure Julia macros.
    // Note: @test_throws not yet implemented as Pure Julia (requires quote of TryStatement)
    #[test]
    fn test_test_module_exists() {
        assert!(!TEST_JL.is_empty());
        assert!(TEST_JL.contains("module Test"));
        // Test module defines @test and @testset macros
        assert!(TEST_JL.contains("macro test(ex)"));
        assert!(TEST_JL.contains("macro testset(name, body)"));
    }

    #[test]
    fn test_test_no_nonstandard_functions() {
        // Verify that non-Julia functions are NOT present
        // Julia's Test module exports only macros, not functions like these
        assert!(!TEST_JL.contains("function test("));
        assert!(!TEST_JL.contains("function test_true"));
        assert!(!TEST_JL.contains("function test_equal"));
        assert!(!TEST_JL.contains("function test_approx"));
        assert!(!TEST_JL.contains("function testset"));
        assert!(!TEST_JL.contains("function test_array_equal"));
    }

    #[test]
    fn test_test_macro_documentation() {
        // The Test.jl stub should document that @test/@testset are VM-level macros
        assert!(TEST_JL.contains("@test"));
        assert!(TEST_JL.contains("@testset"));
    }

    // InteractiveUtils module tests
    #[test]
    fn test_interactive_utils_module_exists() {
        assert!(!INTERACTIVE_UTILS_JL.is_empty());
        assert!(INTERACTIVE_UTILS_JL.contains("module InteractiveUtils"));
        assert!(INTERACTIVE_UTILS_JL.contains("export"));
    }

    #[test]
    fn test_interactive_utils_exports() {
        // Check exported functions
        assert!(INTERACTIVE_UTILS_JL.contains("export versioninfo"));
        assert!(INTERACTIVE_UTILS_JL.contains("function versioninfo"));
    }

    #[test]
    fn test_interactive_utils_no_unsupported_features() {
        // Verify that functions requiring compiler introspection are NOT present
        // These require LLVM/Julia compiler internals
        assert!(!INTERACTIVE_UTILS_JL.contains("function code_warntype"));
        assert!(!INTERACTIVE_UTILS_JL.contains("function code_llvm"));
        assert!(!INTERACTIVE_UTILS_JL.contains("function code_native"));
        assert!(!INTERACTIVE_UTILS_JL.contains("function code_typed"));
        assert!(!INTERACTIVE_UTILS_JL.contains("function methodswith"));
        assert!(!INTERACTIVE_UTILS_JL.contains("function varinfo"));
        assert!(!INTERACTIVE_UTILS_JL.contains("function clipboard"));
    }

    #[test]
    fn test_interactive_utils_available() {
        assert!(is_stdlib_module("InteractiveUtils"));
        assert!(get_stdlib_module("InteractiveUtils").is_some());
        let modules = available_modules();
        assert!(modules.contains(&"InteractiveUtils"));
    }

    // Iterators module tests
    #[test]
    fn test_iterators_module_exists() {
        assert!(!ITERATORS_JL.is_empty());
        assert!(ITERATORS_JL.contains("module Iterators"));
        assert!(ITERATORS_JL.contains("export"));
    }

    #[test]
    fn test_iterators_exports() {
        // Verify exported iterator functions
        assert!(ITERATORS_JL.contains("enumerate"));
        assert!(ITERATORS_JL.contains("zip"));
        assert!(ITERATORS_JL.contains("take"));
        assert!(ITERATORS_JL.contains("drop"));
        assert!(ITERATORS_JL.contains("cycle"));
        assert!(ITERATORS_JL.contains("repeated"));
        assert!(ITERATORS_JL.contains("flatten"));
        assert!(ITERATORS_JL.contains("partition"));
        assert!(ITERATORS_JL.contains("product"));
        assert!(ITERATORS_JL.contains("countfrom"));
        assert!(ITERATORS_JL.contains("peel"));
    }

    #[test]
    fn test_iterators_available() {
        assert!(is_stdlib_module("Iterators"));
        assert!(get_stdlib_module("Iterators").is_some());
        let modules = available_modules();
        assert!(modules.contains(&"Iterators"));
    }

    #[test]
    fn test_iterators_package() {
        let pkg = get_stdlib_package("Iterators");
        assert!(pkg.is_some());
        let pkg = pkg.unwrap();
        assert!(pkg.project_toml.contains("name = \"Iterators\""));
        assert!(pkg.source.contains("module Iterators"));
    }

    // Broadcast module tests
    #[test]
    fn test_broadcast_module_exists() {
        assert!(!BROADCAST_JL.is_empty());
        assert!(BROADCAST_JL.contains("module Broadcast"));
        assert!(BROADCAST_JL.contains("export"));
    }

    #[test]
    fn test_broadcast_exports() {
        // Verify exported broadcast functions
        assert!(BROADCAST_JL.contains("broadcast"));
        assert!(BROADCAST_JL.contains("broadcast!"));
    }

    #[test]
    fn test_broadcast_available() {
        assert!(is_stdlib_module("Broadcast"));
        assert!(get_stdlib_module("Broadcast").is_some());
        let modules = available_modules();
        assert!(modules.contains(&"Broadcast"));
    }

    #[test]
    fn test_broadcast_package() {
        let pkg = get_stdlib_package("Broadcast");
        assert!(pkg.is_some());
        let pkg = pkg.unwrap();
        assert!(pkg.project_toml.contains("name = \"Broadcast\""));
        assert!(pkg.source.contains("module Broadcast"));
    }

    // Base64 module tests
    #[test]
    fn test_base64_module_exists() {
        assert!(!BASE64_JL.is_empty());
        assert!(BASE64_JL.contains("module Base64"));
        assert!(BASE64_JL.contains("export"));
    }

    #[test]
    fn test_base64_exports() {
        assert!(BASE64_JL.contains("base64encode"));
        assert!(BASE64_JL.contains("base64decode"));
    }

    #[test]
    fn test_base64_available() {
        assert!(is_stdlib_module("Base64"));
        assert!(get_stdlib_module("Base64").is_some());
        let modules = available_modules();
        assert!(modules.contains(&"Base64"));
    }

    #[test]
    fn test_base64_package() {
        let pkg = get_stdlib_package("Base64");
        assert!(pkg.is_some());
        let pkg = pkg.unwrap();
        assert!(pkg.project_toml.contains("name = \"Base64\""));
        assert!(pkg.source.contains("module Base64"));
    }

    #[test]
    fn test_base64_internal_functions() {
        assert!(BASE64_JL.contains("function _b64_encode_char"));
        assert!(BASE64_JL.contains("function _b64_decode_char"));
        assert!(BASE64_JL.contains("function base64encode"));
        assert!(BASE64_JL.contains("function base64decode"));
    }

    // Printf module tests
    #[test]
    fn test_printf_module_exists() {
        assert!(!PRINTF_JL.is_empty());
        assert!(PRINTF_JL.contains("module Printf"));
        assert!(PRINTF_JL.contains("@printf"));
        assert!(PRINTF_JL.contains("@sprintf"));
    }

    #[test]
    fn test_printf_available() {
        assert!(is_stdlib_module("Printf"));
        assert!(get_stdlib_module("Printf").is_some());
        let modules = available_modules();
        assert!(modules.contains(&"Printf"));
    }

    #[test]
    fn test_printf_package() {
        let pkg = get_stdlib_package("Printf");
        assert!(pkg.is_some());
        let pkg = pkg.unwrap();
        assert!(pkg.project_toml.contains("name = \"Printf\""));
        assert!(pkg.source.contains("module Printf"));
    }
}
