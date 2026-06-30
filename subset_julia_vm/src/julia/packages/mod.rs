//! Bundled third-party package registry.
//!
//! This module embeds minimal third-party packages used to validate the
//! SubsetJuliaVM package loader.  Each package follows the standard Julia
//! package layout (`Project.toml` + `src/<Name>.jl`) and may also include
//! files loaded via `include()` from within the package source.
//!
//! # Virtual path scheme
//!
//! Embedded packages cannot use real filesystem paths on iOS/WASM.  Instead,
//! each package is assigned a *virtual* source directory of the form:
//!
//! ```text
//! /embedded_packages/<Name>/src
//! ```
//!
//! When the lowering pass encounters `include("helpers.jl")` inside
//! `Example.jl`, the resolved path becomes
//! `/embedded_packages/Example/src/helpers.jl`.  `read_include_file` strips
//! the leading `/` and looks up `embedded_packages/Example/src/helpers.jl`
//! in [`get_package_include`].

/// Virtual directory prefix used for embedded package include resolution.
pub const VIRTUAL_PKG_PREFIX: &str = "/embedded_packages";

// ── Example ──────────────────────────────────────────────────────────────────

pub const EXAMPLE_PROJECT_TOML: &str = include_str!("../../../packages/Example/Project.toml");

pub const EXAMPLE_JL: &str = include_str!("../../../packages/Example/src/Example.jl");

pub const EXAMPLE_HELPERS_JL: &str = include_str!("../../../packages/Example/src/helpers.jl");

// ── Primes ────────────────────────────────────────────────────────────────────

pub const PRIMES_PROJECT_TOML: &str = include_str!("../../../packages/Primes/Project.toml");

pub const PRIMES_JL: &str = include_str!("../../../packages/Primes/src/Primes.jl");

pub const PRIMES_PRIMALITY_JL: &str = include_str!("../../../packages/Primes/src/primality.jl");

pub const PRIMES_GENERATION_JL: &str = include_str!("../../../packages/Primes/src/generation.jl");

pub const PRIMES_FACTORIZATION_JL: &str =
    include_str!("../../../packages/Primes/src/factorization.jl");

pub const PRIMES_ARITHMETIC_JL: &str = include_str!("../../../packages/Primes/src/arithmetic.jl");

// ── Plots ─────────────────────────────────────────────────────────────────────

pub const PLOTS_PROJECT_TOML: &str = include_str!("../../../packages/Plots/Project.toml");

pub const PLOTS_JL: &str = include_str!("../../../packages/Plots/src/Plots.jl");

pub const PLOTS_TYPES_JL: &str = include_str!("../../../packages/Plots/src/types.jl");

pub const PLOTS_API_JL: &str = include_str!("../../../packages/Plots/src/api.jl");

// ── JSXGraph ──────────────────────────────────────────────────────────────────

pub const JSXGRAPH_PROJECT_TOML: &str = include_str!("../../../packages/JSXGraph/Project.toml");

pub const JSXGRAPH_JL: &str = include_str!("../../../packages/JSXGraph/src/JSXGraph.jl");

pub const JSXGRAPH_TYPES_JL: &str = include_str!("../../../packages/JSXGraph/src/types.jl");

pub const JSXGRAPH_API_JL: &str = include_str!("../../../packages/JSXGraph/src/api.jl");

pub const JSXGRAPH_ELEMENTS_JL: &str = include_str!("../../../packages/JSXGraph/src/elements.jl");

// ── Symbolics ─────────────────────────────────────────────────────────────────

pub const SYMBOLICS_PROJECT_TOML: &str = include_str!("../../../packages/Symbolics/Project.toml");

pub const SYMBOLICS_JL: &str = include_str!("../../../packages/Symbolics/src/Symbolics.jl");

pub const SYMBOLICS_TYPES_JL: &str = include_str!("../../../packages/Symbolics/src/types.jl");

pub const SYMBOLICS_ARITHMETIC_JL: &str =
    include_str!("../../../packages/Symbolics/src/arithmetic.jl");

pub const SYMBOLICS_LINEAR_ALGEBRA_JL: &str =
    include_str!("../../../packages/Symbolics/src/linear_algebra.jl");

pub const SYMBOLICS_SHOW_JL: &str = include_str!("../../../packages/Symbolics/src/show.jl");

pub const SYMBOLICS_SUBSTITUTE_JL: &str =
    include_str!("../../../packages/Symbolics/src/substitute.jl");

pub const SYMBOLICS_SIMPLIFY_JL: &str = include_str!("../../../packages/Symbolics/src/simplify.jl");

pub const SYMBOLICS_DIFF_JL: &str = include_str!("../../../packages/Symbolics/src/diff.jl");

pub const SYMBOLICS_VARIABLES_JL: &str =
    include_str!("../../../packages/Symbolics/src/variables.jl");

// ── SpecialFunctions ──────────────────────────────────────────────────────────

pub const SPECIAL_FUNCTIONS_PROJECT_TOML: &str =
    include_str!("../../../packages/SpecialFunctions/Project.toml");

pub const SPECIAL_FUNCTIONS_JL: &str =
    include_str!("../../../packages/SpecialFunctions/src/SpecialFunctions.jl");

// ── StatsBase ─────────────────────────────────────────────────────────────────

pub const STATS_BASE_PROJECT_TOML: &str = include_str!("../../../packages/StatsBase/Project.toml");

pub const STATS_BASE_JL: &str = include_str!("../../../packages/StatsBase/src/StatsBase.jl");

// ── Distributions ─────────────────────────────────────────────────────────────

pub const DISTRIBUTIONS_PROJECT_TOML: &str =
    include_str!("../../../packages/Distributions/Project.toml");

pub const DISTRIBUTIONS_JL: &str =
    include_str!("../../../packages/Distributions/src/Distributions.jl");

pub const DISTRIBUTIONS_CONTINUOUS_JL: &str =
    include_str!("../../../packages/Distributions/src/univariate/continuous.jl");

pub const DISTRIBUTIONS_DISCRETE_JL: &str =
    include_str!("../../../packages/Distributions/src/univariate/discrete.jl");

pub const DISTRIBUTIONS_TRUNCATE_JL: &str =
    include_str!("../../../packages/Distributions/src/truncate.jl");

pub const DISTRIBUTIONS_MVNORMAL_JL: &str =
    include_str!("../../../packages/Distributions/src/multivariate/mvnormal.jl");

pub const DISTRIBUTIONS_FIT_JL: &str = include_str!("../../../packages/Distributions/src/fit.jl");

// ── StatsPlots ────────────────────────────────────────────────────────────────

pub const STATS_PLOTS_PROJECT_TOML: &str =
    include_str!("../../../packages/StatsPlots/Project.toml");

pub const STATS_PLOTS_JL: &str = include_str!("../../../packages/StatsPlots/src/StatsPlots.jl");

pub const STATS_PLOTS_DISTRIBUTIONS_JL: &str =
    include_str!("../../../packages/StatsPlots/src/distributions.jl");

// ── SciMLBase ─────────────────────────────────────────────────────────────────

pub const SCI_ML_BASE_PROJECT_TOML: &str = include_str!("../../../packages/SciMLBase/Project.toml");

pub const SCI_ML_BASE_JL: &str = include_str!("../../../packages/SciMLBase/src/SciMLBase.jl");

// ── OrdinaryDiffEq ───────────────────────────────────────────────────────────

pub const ORDINARY_DIFF_EQ_PROJECT_TOML: &str =
    include_str!("../../../packages/OrdinaryDiffEq/Project.toml");

pub const ORDINARY_DIFF_EQ_JL: &str =
    include_str!("../../../packages/OrdinaryDiffEq/src/OrdinaryDiffEq.jl");

// ── Interact ──────────────────────────────────────────────────────────────────

pub const INTERACT_PROJECT_TOML: &str = include_str!("../../../packages/Interact/Project.toml");

pub const INTERACT_JL: &str = include_str!("../../../packages/Interact/src/Interact.jl");

pub const INTERACT_TYPES_JL: &str = include_str!("../../../packages/Interact/src/types.jl");

// ── Preferences ──────────────────────────────────────────────────────────────

pub const PREFERENCES_PROJECT_TOML: &str =
    include_str!("../../../packages/Preferences/Project.toml");

pub const PREFERENCES_JL: &str = include_str!("../../../packages/Preferences/src/Preferences.jl");

// ── RandomExtensions ─────────────────────────────────────────────────────────

pub const RANDOM_EXTENSIONS_PROJECT_TOML: &str =
    include_str!("../../../packages/RandomExtensions/Project.toml");

pub const RANDOM_EXTENSIONS_JL: &str =
    include_str!("../../../packages/RandomExtensions/src/RandomExtensions.jl");

// ── SparseArrays ─────────────────────────────────────────────────────────────

pub const SPARSE_ARRAYS_PROJECT_TOML: &str =
    include_str!("../../../packages/SparseArrays/Project.toml");

pub const SPARSE_ARRAYS_JL: &str =
    include_str!("../../../packages/SparseArrays/src/SparseArrays.jl");

// ── AbstractAlgebra ──────────────────────────────────────────────────────────

pub const ABSTRACT_ALGEBRA_PROJECT_TOML: &str =
    include_str!("../../../packages/AbstractAlgebra/Project.toml");

pub const ABSTRACT_ALGEBRA_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/AbstractAlgebra.jl");

pub const ABSTRACT_ALGEBRA_IMPORTS_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/imports.jl");

pub const ABSTRACT_ALGEBRA_EXPORTS_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/exports.jl");

pub const ABSTRACT_ALGEBRA_ALIAS_MACRO_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/AliasMacro.jl");

pub const ABSTRACT_ALGEBRA_ALIASES_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Aliases.jl");

pub const ABSTRACT_ALGEBRA_ASSERTIONS_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Assertions.jl");

pub const ABSTRACT_ALGEBRA_ATTRIBUTES_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Attributes.jl");

pub const ABSTRACT_ALGEBRA_ERROR_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/error.jl");

pub const ABSTRACT_ALGEBRA_ABSTRACT_TYPES_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/AbstractTypes.jl");

pub const ABSTRACT_ALGEBRA_JULIA_TYPES_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/julia/JuliaTypes.jl");

pub const ABSTRACT_ALGEBRA_CONCRETE_TYPES_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/ConcreteTypes.jl");

pub const ABSTRACT_ALGEBRA_FUNDAMENTAL_INTERFACE_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/fundamental_interface.jl");

pub const ABSTRACT_ALGEBRA_KNOWN_PROPERTIES_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/KnownProperties.jl");

pub const ABSTRACT_ALGEBRA_INTEGER_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/julia/Integer.jl");

pub const ABSTRACT_ALGEBRA_RATIONAL_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/julia/Rational.jl");

pub const ABSTRACT_ALGEBRA_POLY_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Poly.jl");

pub const ABSTRACT_ALGEBRA_FRACTION_RESIDUE_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/FractionResidue.jl");

pub const ABSTRACT_ALGEBRA_MATRIX_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Matrix.jl");

pub const ABSTRACT_ALGEBRA_MODULE_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Module.jl");

pub const ABSTRACT_ALGEBRA_MAP_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Map.jl");

pub const ABSTRACT_ALGEBRA_PERM_GROUPS_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/PermGroups.jl");

pub const ABSTRACT_ALGEBRA_YOUNG_TABS_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/YoungTabs.jl");

pub const ABSTRACT_ALGEBRA_GENERIC_JL: &str =
    include_str!("../../../packages/AbstractAlgebra/src/Generic.jl");

// ── MacroTools ────────────────────────────────────────────────────────────────

pub const MACRO_TOOLS_PROJECT_TOML: &str =
    include_str!("../../../packages/MacroTools/Project.toml");

pub const MACRO_TOOLS_JL: &str = include_str!("../../../packages/MacroTools/src/MacroTools.jl");

pub const MACRO_TOOLS_MATCH_MATCH_JL: &str =
    include_str!("../../../packages/MacroTools/src/match/match.jl");

pub const MACRO_TOOLS_MATCH_TYPES_JL: &str =
    include_str!("../../../packages/MacroTools/src/match/types.jl");

pub const MACRO_TOOLS_MATCH_UNION_JL: &str =
    include_str!("../../../packages/MacroTools/src/match/union.jl");

pub const MACRO_TOOLS_MATCH_MACRO_JL: &str =
    include_str!("../../../packages/MacroTools/src/match/macro.jl");

pub const MACRO_TOOLS_UTILS_JL: &str = include_str!("../../../packages/MacroTools/src/utils.jl");

pub const MACRO_TOOLS_STRUCTDEF_JL: &str =
    include_str!("../../../packages/MacroTools/src/structdef.jl");

pub const MACRO_TOOLS_EXAMPLES_DESTRUCT_JL: &str =
    include_str!("../../../packages/MacroTools/src/examples/destruct.jl");

pub const MACRO_TOOLS_EXAMPLES_THREADING_JL: &str =
    include_str!("../../../packages/MacroTools/src/examples/threading.jl");

pub const MACRO_TOOLS_EXAMPLES_FORWARD_JL: &str =
    include_str!("../../../packages/MacroTools/src/examples/forward.jl");

pub const MACRO_TOOLS_ANIMALS_TXT: &str = include_str!("../../../packages/MacroTools/animals.txt");

// ── PrecompileTools ──────────────────────────────────────────────────────────

pub const PRECOMPILE_TOOLS_PROJECT_TOML: &str =
    include_str!("../../../packages/PrecompileTools/Project.toml");

pub const PRECOMPILE_TOOLS_JL: &str =
    include_str!("../../../packages/PrecompileTools/src/PrecompileTools.jl");

// ── StaticArraysCore ─────────────────────────────────────────────────────────

pub const STATIC_ARRAYS_CORE_PROJECT_TOML: &str =
    include_str!("../../../packages/StaticArraysCore/Project.toml");

pub const STATIC_ARRAYS_CORE_JL: &str =
    include_str!("../../../packages/StaticArraysCore/src/StaticArraysCore.jl");

pub const STATIC_ARRAYS_CORE_S_ONE_TO_JL: &str =
    include_str!("../../../packages/StaticArraysCore/src/SOneTo.jl");

pub const STATIC_ARRAYS_CORE_TRAITS_JL: &str =
    include_str!("../../../packages/StaticArraysCore/src/traits.jl");

pub const STATIC_ARRAYS_CORE_TYPES_JL: &str =
    include_str!("../../../packages/StaticArraysCore/src/types.jl");

// ── StaticArrays ─────────────────────────────────────────────────────────────

pub const STATIC_ARRAYS_PROJECT_TOML: &str =
    include_str!("../../../packages/StaticArrays/Project.toml");

pub const STATIC_ARRAYS_JL: &str =
    include_str!("../../../packages/StaticArrays/src/StaticArrays.jl");

pub const STATIC_ARRAYS_ABSTRACTARRAY_JL: &str =
    include_str!("../../../packages/StaticArrays/src/abstractarray.jl");

pub const STATIC_ARRAYS_S_ARRAY_JL: &str =
    include_str!("../../../packages/StaticArrays/src/SArray.jl");

pub const STATIC_ARRAYS_S_VECTOR_JL: &str =
    include_str!("../../../packages/StaticArrays/src/SVector.jl");

pub const STATIC_ARRAYS_S_MATRIX_JL: &str =
    include_str!("../../../packages/StaticArrays/src/SMatrix.jl");

pub const STATIC_ARRAYS_INDEXING_JL: &str =
    include_str!("../../../packages/StaticArrays/src/indexing.jl");

pub const STATIC_ARRAYS_BROADCAST_JL: &str =
    include_str!("../../../packages/StaticArrays/src/broadcast.jl");

pub const STATIC_ARRAYS_ARRAYMATH_JL: &str =
    include_str!("../../../packages/StaticArrays/src/arraymath.jl");

pub const STATIC_ARRAYS_COPY_JL: &str = include_str!("../../../packages/StaticArrays/src/copy.jl");

pub const STATIC_ARRAYS_LINALG_JL: &str =
    include_str!("../../../packages/StaticArrays/src/linalg.jl");

// ── NLSolversBase (Optim dependency) ──────────────────────────────────────────

pub const NLSOLVERS_BASE_PROJECT_TOML: &str =
    include_str!("../../../packages/NLSolversBase/Project.toml");

pub const NLSOLVERS_BASE_JL: &str =
    include_str!("../../../packages/NLSolversBase/src/NLSolversBase.jl");

// ── LineSearches (Optim dependency) ───────────────────────────────────────────

pub const LINE_SEARCHES_PROJECT_TOML: &str =
    include_str!("../../../packages/LineSearches/Project.toml");

pub const LINE_SEARCHES_JL: &str =
    include_str!("../../../packages/LineSearches/src/LineSearches.jl");

pub const LINE_SEARCHES_HAGERZHANG_JL: &str =
    include_str!("../../../packages/LineSearches/src/hagerzhang.jl");

// ── ADTypes (Optim dependency stub) ───────────────────────────────────────────

pub const AD_TYPES_PROJECT_TOML: &str = include_str!("../../../packages/ADTypes/Project.toml");

pub const AD_TYPES_JL: &str = include_str!("../../../packages/ADTypes/src/ADTypes.jl");

// ── NaNMath (Optim dependency stub) ───────────────────────────────────────────

pub const NAN_MATH_PROJECT_TOML: &str = include_str!("../../../packages/NaNMath/Project.toml");

pub const NAN_MATH_JL: &str = include_str!("../../../packages/NaNMath/src/NaNMath.jl");

// ── EnumX (Optim dependency stub) ─────────────────────────────────────────────

pub const ENUM_X_PROJECT_TOML: &str = include_str!("../../../packages/EnumX/Project.toml");

pub const ENUM_X_JL: &str = include_str!("../../../packages/EnumX/src/EnumX.jl");

// ── FillArrays (Optim dependency stub) ────────────────────────────────────────

pub const FILL_ARRAYS_PROJECT_TOML: &str =
    include_str!("../../../packages/FillArrays/Project.toml");

pub const FILL_ARRAYS_JL: &str = include_str!("../../../packages/FillArrays/src/FillArrays.jl");

// ── PositiveFactorizations (Optim dependency stub) ────────────────────────────

pub const POSITIVE_FACTORIZATIONS_PROJECT_TOML: &str =
    include_str!("../../../packages/PositiveFactorizations/Project.toml");

pub const POSITIVE_FACTORIZATIONS_JL: &str =
    include_str!("../../../packages/PositiveFactorizations/src/PositiveFactorizations.jl");

// ── DataStructures (QuadGK dependency) ───────────────────────────────────────

pub const DATA_STRUCTURES_PROJECT_TOML: &str =
    include_str!("../../../packages/DataStructures/Project.toml");

pub const DATA_STRUCTURES_JL: &str =
    include_str!("../../../packages/DataStructures/src/DataStructures.jl");

pub const DATA_STRUCTURES_ARRAYS_AS_HEAPS_JL: &str =
    include_str!("../../../packages/DataStructures/src/heaps/arrays_as_heaps.jl");

// ── QuadGK ───────────────────────────────────────────────────────────────────

pub const QUAD_GK_PROJECT_TOML: &str = include_str!("../../../packages/QuadGK/Project.toml");

pub const QUAD_GK_JL: &str = include_str!("../../../packages/QuadGK/src/QuadGK.jl");

pub const QUAD_GK_GAUSS_KRONROD_JL: &str =
    include_str!("../../../packages/QuadGK/src/gausskronrod.jl");

pub const QUAD_GK_EVALRULE_JL: &str = include_str!("../../../packages/QuadGK/src/evalrule.jl");

pub const QUAD_GK_ADAPT_JL: &str = include_str!("../../../packages/QuadGK/src/adapt.jl");

pub const QUAD_GK_API_JL: &str = include_str!("../../../packages/QuadGK/src/api.jl");

pub const QUAD_GK_WEIGHTED_GAUSS_JL: &str =
    include_str!("../../../packages/QuadGK/src/weightedgauss.jl");

pub const QUAD_GK_BATCH_JL: &str = include_str!("../../../packages/QuadGK/src/batch.jl");

// ── Optim ─────────────────────────────────────────────────────────────────────

pub const OPTIM_PROJECT_TOML: &str = include_str!("../../../packages/Optim/Project.toml");

pub const OPTIM_JL: &str = include_str!("../../../packages/Optim/src/Optim.jl");

pub const OPTIM_TYPES_JL: &str = include_str!("../../../packages/Optim/src/types.jl");

pub const OPTIM_API_JL: &str = include_str!("../../../packages/Optim/src/api.jl");

pub const OPTIM_MAXIMIZE_JL: &str = include_str!("../../../packages/Optim/src/maximize.jl");

pub const OPTIM_UTILITIES_GENERIC_JL: &str =
    include_str!("../../../packages/Optim/src/utilities/generic.jl");

pub const OPTIM_UNIVARIATE_TYPES_JL: &str =
    include_str!("../../../packages/Optim/src/univariate/types.jl");

pub const OPTIM_UNIVARIATE_GOLDEN_SECTION_JL: &str =
    include_str!("../../../packages/Optim/src/univariate/solvers/golden_section.jl");

pub const OPTIM_UNIVARIATE_BRENT_JL: &str =
    include_str!("../../../packages/Optim/src/univariate/solvers/brent.jl");

pub const OPTIM_UNIVARIATE_INTERFACE_JL: &str =
    include_str!("../../../packages/Optim/src/univariate/optimize/interface.jl");

pub const OPTIM_NELDER_MEAD_JL: &str =
    include_str!("../../../packages/Optim/src/multivariate/solvers/zeroth_order/nelder_mead.jl");

pub const OPTIM_GRADIENT_DESCENT_JL: &str = include_str!(
    "../../../packages/Optim/src/multivariate/solvers/first_order/gradient_descent.jl"
);

pub const OPTIM_BFGS_JL: &str =
    include_str!("../../../packages/Optim/src/multivariate/solvers/first_order/bfgs.jl");

pub const OPTIM_MULTIVARIATE_INTERFACE_JL: &str =
    include_str!("../../../packages/Optim/src/multivariate/optimize/interface.jl");

// ── Quaternions (Rotations.jl dependency) ───────────────────────────────────────

pub const QUATERNIONS_PROJECT_TOML: &str =
    include_str!("../../../packages/Quaternions/Project.toml");

pub const QUATERNIONS_JL: &str = include_str!("../../../packages/Quaternions/src/Quaternions.jl");

// ── Rotations ────────────────────────────────────────────────────────────────

pub const ROTATIONS_PROJECT_TOML: &str = include_str!("../../../packages/Rotations/Project.toml");

pub const ROTATIONS_JL: &str = include_str!("../../../packages/Rotations/src/Rotations.jl");

pub const ROTATIONS_UTIL_JL: &str = include_str!("../../../packages/Rotations/src/util.jl");

pub const ROTATIONS_CORE_TYPES_JL: &str =
    include_str!("../../../packages/Rotations/src/core_types.jl");

pub const ROTATIONS_EULER_TYPES_JL: &str =
    include_str!("../../../packages/Rotations/src/euler_types.jl");

pub const ROTATIONS_ANGLEAXIS_TYPES_JL: &str =
    include_str!("../../../packages/Rotations/src/angleaxis_types.jl");

pub const ROTATIONS_QUATERNION_TYPES_JL: &str =
    include_str!("../../../packages/Rotations/src/quaternion_types.jl");

pub const ROTATIONS_PARAM3_TYPES_JL: &str =
    include_str!("../../../packages/Rotations/src/param3_types.jl");

pub const ROTATIONS_ROTATION_BETWEEN_JL: &str =
    include_str!("../../../packages/Rotations/src/rotation_between.jl");

pub const ROTATIONS_GENERATOR_TYPES_JL: &str =
    include_str!("../../../packages/Rotations/src/generator_types.jl");

// ── Public API ────────────────────────────────────────────────────────────────

/// An embedded bundled package (Project.toml + main source).
#[derive(Debug, Clone, Copy)]
pub struct BundledPackage {
    pub project_toml: &'static str,
    pub source: &'static str,
}

/// Look up a bundled package by name.
pub fn get_bundled_package(name: &str) -> Option<BundledPackage> {
    match name {
        "Example" => Some(BundledPackage {
            project_toml: EXAMPLE_PROJECT_TOML,
            source: EXAMPLE_JL,
        }),
        "Primes" => Some(BundledPackage {
            project_toml: PRIMES_PROJECT_TOML,
            source: PRIMES_JL,
        }),
        "Plots" => Some(BundledPackage {
            project_toml: PLOTS_PROJECT_TOML,
            source: PLOTS_JL,
        }),
        "JSXGraph" => Some(BundledPackage {
            project_toml: JSXGRAPH_PROJECT_TOML,
            source: JSXGRAPH_JL,
        }),
        "Symbolics" => Some(BundledPackage {
            project_toml: SYMBOLICS_PROJECT_TOML,
            source: SYMBOLICS_JL,
        }),
        "SpecialFunctions" => Some(BundledPackage {
            project_toml: SPECIAL_FUNCTIONS_PROJECT_TOML,
            source: SPECIAL_FUNCTIONS_JL,
        }),
        "StatsBase" => Some(BundledPackage {
            project_toml: STATS_BASE_PROJECT_TOML,
            source: STATS_BASE_JL,
        }),
        "Distributions" => Some(BundledPackage {
            project_toml: DISTRIBUTIONS_PROJECT_TOML,
            source: DISTRIBUTIONS_JL,
        }),
        "StatsPlots" => Some(BundledPackage {
            project_toml: STATS_PLOTS_PROJECT_TOML,
            source: STATS_PLOTS_JL,
        }),
        "SciMLBase" => Some(BundledPackage {
            project_toml: SCI_ML_BASE_PROJECT_TOML,
            source: SCI_ML_BASE_JL,
        }),
        "OrdinaryDiffEq" => Some(BundledPackage {
            project_toml: ORDINARY_DIFF_EQ_PROJECT_TOML,
            source: ORDINARY_DIFF_EQ_JL,
        }),
        "Interact" => Some(BundledPackage {
            project_toml: INTERACT_PROJECT_TOML,
            source: INTERACT_JL,
        }),
        "Preferences" => Some(BundledPackage {
            project_toml: PREFERENCES_PROJECT_TOML,
            source: PREFERENCES_JL,
        }),
        "RandomExtensions" => Some(BundledPackage {
            project_toml: RANDOM_EXTENSIONS_PROJECT_TOML,
            source: RANDOM_EXTENSIONS_JL,
        }),
        "SparseArrays" => Some(BundledPackage {
            project_toml: SPARSE_ARRAYS_PROJECT_TOML,
            source: SPARSE_ARRAYS_JL,
        }),
        "AbstractAlgebra" => Some(BundledPackage {
            project_toml: ABSTRACT_ALGEBRA_PROJECT_TOML,
            source: ABSTRACT_ALGEBRA_JL,
        }),
        "MacroTools" => Some(BundledPackage {
            project_toml: MACRO_TOOLS_PROJECT_TOML,
            source: MACRO_TOOLS_JL,
        }),
        "PrecompileTools" => Some(BundledPackage {
            project_toml: PRECOMPILE_TOOLS_PROJECT_TOML,
            source: PRECOMPILE_TOOLS_JL,
        }),
        "StaticArraysCore" => Some(BundledPackage {
            project_toml: STATIC_ARRAYS_CORE_PROJECT_TOML,
            source: STATIC_ARRAYS_CORE_JL,
        }),
        "StaticArrays" => Some(BundledPackage {
            project_toml: STATIC_ARRAYS_PROJECT_TOML,
            source: STATIC_ARRAYS_JL,
        }),
        "NLSolversBase" => Some(BundledPackage {
            project_toml: NLSOLVERS_BASE_PROJECT_TOML,
            source: NLSOLVERS_BASE_JL,
        }),
        "LineSearches" => Some(BundledPackage {
            project_toml: LINE_SEARCHES_PROJECT_TOML,
            source: LINE_SEARCHES_JL,
        }),
        "ADTypes" => Some(BundledPackage {
            project_toml: AD_TYPES_PROJECT_TOML,
            source: AD_TYPES_JL,
        }),
        "NaNMath" => Some(BundledPackage {
            project_toml: NAN_MATH_PROJECT_TOML,
            source: NAN_MATH_JL,
        }),
        "EnumX" => Some(BundledPackage {
            project_toml: ENUM_X_PROJECT_TOML,
            source: ENUM_X_JL,
        }),
        "FillArrays" => Some(BundledPackage {
            project_toml: FILL_ARRAYS_PROJECT_TOML,
            source: FILL_ARRAYS_JL,
        }),
        "PositiveFactorizations" => Some(BundledPackage {
            project_toml: POSITIVE_FACTORIZATIONS_PROJECT_TOML,
            source: POSITIVE_FACTORIZATIONS_JL,
        }),
        "DataStructures" => Some(BundledPackage {
            project_toml: DATA_STRUCTURES_PROJECT_TOML,
            source: DATA_STRUCTURES_JL,
        }),
        "QuadGK" => Some(BundledPackage {
            project_toml: QUAD_GK_PROJECT_TOML,
            source: QUAD_GK_JL,
        }),
        "Optim" => Some(BundledPackage {
            project_toml: OPTIM_PROJECT_TOML,
            source: OPTIM_JL,
        }),
        "Quaternions" => Some(BundledPackage {
            project_toml: QUATERNIONS_PROJECT_TOML,
            source: QUATERNIONS_JL,
        }),
        "Rotations" => Some(BundledPackage {
            project_toml: ROTATIONS_PROJECT_TOML,
            source: ROTATIONS_JL,
        }),
        _ => None,
    }
}

/// Resolve a file inside an embedded package.
///
/// `virtual_path` is the resolved path with an optional leading `/`, e.g.
/// `"/embedded_packages/Example/src/helpers.jl"`.  `.` and `..` components are
/// normalized so paths produced by `joinpath(@__DIR__, "..", "file")` resolve
/// against the same registry as `include()`.
pub fn get_package_file(virtual_path: &str) -> Option<&'static str> {
    let normalized = normalize_virtual_package_path(virtual_path);
    match normalized.as_str() {
        "embedded_packages/Example/src/helpers.jl" => Some(EXAMPLE_HELPERS_JL),
        "embedded_packages/Primes/src/primality.jl" => Some(PRIMES_PRIMALITY_JL),
        "embedded_packages/Primes/src/generation.jl" => Some(PRIMES_GENERATION_JL),
        "embedded_packages/Primes/src/factorization.jl" => Some(PRIMES_FACTORIZATION_JL),
        "embedded_packages/Primes/src/arithmetic.jl" => Some(PRIMES_ARITHMETIC_JL),
        "embedded_packages/Plots/src/types.jl" => Some(PLOTS_TYPES_JL),
        "embedded_packages/Plots/src/api.jl" => Some(PLOTS_API_JL),
        "embedded_packages/JSXGraph/src/types.jl" => Some(JSXGRAPH_TYPES_JL),
        "embedded_packages/JSXGraph/src/api.jl" => Some(JSXGRAPH_API_JL),
        "embedded_packages/JSXGraph/src/elements.jl" => Some(JSXGRAPH_ELEMENTS_JL),
        "embedded_packages/Symbolics/src/types.jl" => Some(SYMBOLICS_TYPES_JL),
        "embedded_packages/Symbolics/src/arithmetic.jl" => Some(SYMBOLICS_ARITHMETIC_JL),
        "embedded_packages/Symbolics/src/linear_algebra.jl" => Some(SYMBOLICS_LINEAR_ALGEBRA_JL),
        "embedded_packages/Symbolics/src/show.jl" => Some(SYMBOLICS_SHOW_JL),
        "embedded_packages/Symbolics/src/substitute.jl" => Some(SYMBOLICS_SUBSTITUTE_JL),
        "embedded_packages/Symbolics/src/simplify.jl" => Some(SYMBOLICS_SIMPLIFY_JL),
        "embedded_packages/Symbolics/src/diff.jl" => Some(SYMBOLICS_DIFF_JL),
        "embedded_packages/Symbolics/src/variables.jl" => Some(SYMBOLICS_VARIABLES_JL),
        "embedded_packages/Distributions/src/univariate/continuous.jl" => {
            Some(DISTRIBUTIONS_CONTINUOUS_JL)
        }
        "embedded_packages/Distributions/src/univariate/discrete.jl" => {
            Some(DISTRIBUTIONS_DISCRETE_JL)
        }
        "embedded_packages/Distributions/src/truncate.jl" => Some(DISTRIBUTIONS_TRUNCATE_JL),
        "embedded_packages/Distributions/src/multivariate/mvnormal.jl" => {
            Some(DISTRIBUTIONS_MVNORMAL_JL)
        }
        "embedded_packages/Distributions/src/fit.jl" => Some(DISTRIBUTIONS_FIT_JL),
        "embedded_packages/StatsPlots/src/distributions.jl" => Some(STATS_PLOTS_DISTRIBUTIONS_JL),
        "embedded_packages/Interact/src/types.jl" => Some(INTERACT_TYPES_JL),
        "embedded_packages/AbstractAlgebra/src/imports.jl" => Some(ABSTRACT_ALGEBRA_IMPORTS_JL),
        "embedded_packages/AbstractAlgebra/src/exports.jl" => Some(ABSTRACT_ALGEBRA_EXPORTS_JL),
        "embedded_packages/AbstractAlgebra/src/AliasMacro.jl" => {
            Some(ABSTRACT_ALGEBRA_ALIAS_MACRO_JL)
        }
        "embedded_packages/AbstractAlgebra/src/Aliases.jl" => Some(ABSTRACT_ALGEBRA_ALIASES_JL),
        "embedded_packages/AbstractAlgebra/src/Assertions.jl" => {
            Some(ABSTRACT_ALGEBRA_ASSERTIONS_JL)
        }
        "embedded_packages/AbstractAlgebra/src/Attributes.jl" => {
            Some(ABSTRACT_ALGEBRA_ATTRIBUTES_JL)
        }
        "embedded_packages/AbstractAlgebra/src/error.jl" => Some(ABSTRACT_ALGEBRA_ERROR_JL),
        "embedded_packages/AbstractAlgebra/src/AbstractTypes.jl" => {
            Some(ABSTRACT_ALGEBRA_ABSTRACT_TYPES_JL)
        }
        "embedded_packages/AbstractAlgebra/src/julia/JuliaTypes.jl" => {
            Some(ABSTRACT_ALGEBRA_JULIA_TYPES_JL)
        }
        "embedded_packages/AbstractAlgebra/src/ConcreteTypes.jl" => {
            Some(ABSTRACT_ALGEBRA_CONCRETE_TYPES_JL)
        }
        "embedded_packages/AbstractAlgebra/src/fundamental_interface.jl" => {
            Some(ABSTRACT_ALGEBRA_FUNDAMENTAL_INTERFACE_JL)
        }
        "embedded_packages/AbstractAlgebra/src/KnownProperties.jl" => {
            Some(ABSTRACT_ALGEBRA_KNOWN_PROPERTIES_JL)
        }
        "embedded_packages/AbstractAlgebra/src/julia/Integer.jl" => {
            Some(ABSTRACT_ALGEBRA_INTEGER_JL)
        }
        "embedded_packages/AbstractAlgebra/src/julia/Rational.jl" => {
            Some(ABSTRACT_ALGEBRA_RATIONAL_JL)
        }
        "embedded_packages/AbstractAlgebra/src/Poly.jl" => Some(ABSTRACT_ALGEBRA_POLY_JL),
        "embedded_packages/AbstractAlgebra/src/FractionResidue.jl" => {
            Some(ABSTRACT_ALGEBRA_FRACTION_RESIDUE_JL)
        }
        "embedded_packages/AbstractAlgebra/src/Matrix.jl" => Some(ABSTRACT_ALGEBRA_MATRIX_JL),
        "embedded_packages/AbstractAlgebra/src/Module.jl" => Some(ABSTRACT_ALGEBRA_MODULE_JL),
        "embedded_packages/AbstractAlgebra/src/Map.jl" => Some(ABSTRACT_ALGEBRA_MAP_JL),
        "embedded_packages/AbstractAlgebra/src/PermGroups.jl" => {
            Some(ABSTRACT_ALGEBRA_PERM_GROUPS_JL)
        }
        "embedded_packages/AbstractAlgebra/src/YoungTabs.jl" => {
            Some(ABSTRACT_ALGEBRA_YOUNG_TABS_JL)
        }
        "embedded_packages/AbstractAlgebra/src/Generic.jl" => Some(ABSTRACT_ALGEBRA_GENERIC_JL),
        "embedded_packages/MacroTools/src/match/match.jl" => Some(MACRO_TOOLS_MATCH_MATCH_JL),
        "embedded_packages/MacroTools/src/match/types.jl" => Some(MACRO_TOOLS_MATCH_TYPES_JL),
        "embedded_packages/MacroTools/src/match/union.jl" => Some(MACRO_TOOLS_MATCH_UNION_JL),
        "embedded_packages/MacroTools/src/match/macro.jl" => Some(MACRO_TOOLS_MATCH_MACRO_JL),
        "embedded_packages/MacroTools/src/utils.jl" => Some(MACRO_TOOLS_UTILS_JL),
        "embedded_packages/MacroTools/src/structdef.jl" => Some(MACRO_TOOLS_STRUCTDEF_JL),
        "embedded_packages/MacroTools/src/examples/destruct.jl" => {
            Some(MACRO_TOOLS_EXAMPLES_DESTRUCT_JL)
        }
        "embedded_packages/MacroTools/src/examples/threading.jl" => {
            Some(MACRO_TOOLS_EXAMPLES_THREADING_JL)
        }
        "embedded_packages/MacroTools/src/examples/forward.jl" => {
            Some(MACRO_TOOLS_EXAMPLES_FORWARD_JL)
        }
        "embedded_packages/MacroTools/animals.txt" => Some(MACRO_TOOLS_ANIMALS_TXT),
        "embedded_packages/StaticArraysCore/src/SOneTo.jl" => Some(STATIC_ARRAYS_CORE_S_ONE_TO_JL),
        "embedded_packages/StaticArraysCore/src/traits.jl" => Some(STATIC_ARRAYS_CORE_TRAITS_JL),
        "embedded_packages/StaticArraysCore/src/types.jl" => Some(STATIC_ARRAYS_CORE_TYPES_JL),
        "embedded_packages/StaticArrays/src/abstractarray.jl" => {
            Some(STATIC_ARRAYS_ABSTRACTARRAY_JL)
        }
        "embedded_packages/StaticArrays/src/SArray.jl" => Some(STATIC_ARRAYS_S_ARRAY_JL),
        "embedded_packages/StaticArrays/src/SVector.jl" => Some(STATIC_ARRAYS_S_VECTOR_JL),
        "embedded_packages/StaticArrays/src/SMatrix.jl" => Some(STATIC_ARRAYS_S_MATRIX_JL),
        "embedded_packages/StaticArrays/src/indexing.jl" => Some(STATIC_ARRAYS_INDEXING_JL),
        "embedded_packages/StaticArrays/src/broadcast.jl" => Some(STATIC_ARRAYS_BROADCAST_JL),
        "embedded_packages/StaticArrays/src/arraymath.jl" => Some(STATIC_ARRAYS_ARRAYMATH_JL),
        "embedded_packages/StaticArrays/src/copy.jl" => Some(STATIC_ARRAYS_COPY_JL),
        "embedded_packages/StaticArrays/src/linalg.jl" => Some(STATIC_ARRAYS_LINALG_JL),
        "embedded_packages/Optim/src/types.jl" => Some(OPTIM_TYPES_JL),
        "embedded_packages/Optim/src/api.jl" => Some(OPTIM_API_JL),
        "embedded_packages/Optim/src/maximize.jl" => Some(OPTIM_MAXIMIZE_JL),
        "embedded_packages/Optim/src/utilities/generic.jl" => Some(OPTIM_UTILITIES_GENERIC_JL),
        "embedded_packages/Optim/src/univariate/types.jl" => Some(OPTIM_UNIVARIATE_TYPES_JL),
        "embedded_packages/Optim/src/univariate/solvers/golden_section.jl" => {
            Some(OPTIM_UNIVARIATE_GOLDEN_SECTION_JL)
        }
        "embedded_packages/Optim/src/univariate/solvers/brent.jl" => {
            Some(OPTIM_UNIVARIATE_BRENT_JL)
        }
        "embedded_packages/Optim/src/univariate/optimize/interface.jl" => {
            Some(OPTIM_UNIVARIATE_INTERFACE_JL)
        }
        "embedded_packages/Optim/src/multivariate/solvers/zeroth_order/nelder_mead.jl" => {
            Some(OPTIM_NELDER_MEAD_JL)
        }
        "embedded_packages/Optim/src/multivariate/solvers/first_order/gradient_descent.jl" => {
            Some(OPTIM_GRADIENT_DESCENT_JL)
        }
        "embedded_packages/Optim/src/multivariate/solvers/first_order/bfgs.jl" => {
            Some(OPTIM_BFGS_JL)
        }
        "embedded_packages/LineSearches/src/hagerzhang.jl" => Some(LINE_SEARCHES_HAGERZHANG_JL),
        "embedded_packages/DataStructures/src/heaps/arrays_as_heaps.jl" => {
            Some(DATA_STRUCTURES_ARRAYS_AS_HEAPS_JL)
        }
        "embedded_packages/QuadGK/src/gausskronrod.jl" => Some(QUAD_GK_GAUSS_KRONROD_JL),
        "embedded_packages/QuadGK/src/evalrule.jl" => Some(QUAD_GK_EVALRULE_JL),
        "embedded_packages/QuadGK/src/adapt.jl" => Some(QUAD_GK_ADAPT_JL),
        "embedded_packages/QuadGK/src/api.jl" => Some(QUAD_GK_API_JL),
        "embedded_packages/QuadGK/src/weightedgauss.jl" => Some(QUAD_GK_WEIGHTED_GAUSS_JL),
        "embedded_packages/QuadGK/src/batch.jl" => Some(QUAD_GK_BATCH_JL),
        "embedded_packages/Optim/src/multivariate/optimize/interface.jl" => {
            Some(OPTIM_MULTIVARIATE_INTERFACE_JL)
        }
        "embedded_packages/Rotations/src/util.jl" => Some(ROTATIONS_UTIL_JL),
        "embedded_packages/Rotations/src/core_types.jl" => Some(ROTATIONS_CORE_TYPES_JL),
        "embedded_packages/Rotations/src/euler_types.jl" => Some(ROTATIONS_EULER_TYPES_JL),
        "embedded_packages/Rotations/src/angleaxis_types.jl" => Some(ROTATIONS_ANGLEAXIS_TYPES_JL),
        "embedded_packages/Rotations/src/quaternion_types.jl" => {
            Some(ROTATIONS_QUATERNION_TYPES_JL)
        }
        "embedded_packages/Rotations/src/param3_types.jl" => Some(ROTATIONS_PARAM3_TYPES_JL),
        "embedded_packages/Rotations/src/rotation_between.jl" => {
            Some(ROTATIONS_ROTATION_BETWEEN_JL)
        }
        "embedded_packages/Rotations/src/generator_types.jl" => Some(ROTATIONS_GENERATOR_TYPES_JL),
        _ => None,
    }
}

/// Resolve an include file inside an embedded package.
///
/// `virtual_path` is the resolved include path with the leading `/` stripped,
/// e.g. `"embedded_packages/Example/src/helpers.jl"`.
pub fn get_package_include(virtual_path: &str) -> Option<&'static str> {
    get_package_file(virtual_path)
}

fn normalize_virtual_package_path(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// Check whether a package name is bundled.
pub fn is_bundled_package(name: &str) -> bool {
    get_bundled_package(name).is_some()
}

/// List all bundled package names.
pub fn bundled_package_names() -> Vec<&'static str> {
    vec![
        "Example",
        "Primes",
        "Plots",
        "JSXGraph",
        "Symbolics",
        "SpecialFunctions",
        "StatsBase",
        "Distributions",
        "StatsPlots",
        "SciMLBase",
        "OrdinaryDiffEq",
        "Interact",
        "Preferences",
        "RandomExtensions",
        "SparseArrays",
        "AbstractAlgebra",
        "MacroTools",
        "PrecompileTools",
        "StaticArraysCore",
        "StaticArrays",
        "NLSolversBase",
        "LineSearches",
        "ADTypes",
        "NaNMath",
        "EnumX",
        "FillArrays",
        "PositiveFactorizations",
        "DataStructures",
        "QuadGK",
        "Optim",
        "Quaternions",
        "Rotations",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example_package_exists() {
        let pkg = get_bundled_package("Example");
        assert!(pkg.is_some());
        let pkg = pkg.unwrap();
        assert!(pkg.project_toml.contains("name = \"Example\""));
        assert!(pkg.source.contains("module Example"));
    }

    #[test]
    fn test_example_exports() {
        let pkg = get_bundled_package("Example").unwrap();
        assert!(pkg.source.contains("export hello, domath, included_double"));
        assert!(pkg.source.contains("hello(who"));
        assert!(pkg.source.contains("domath(x"));
    }

    #[test]
    fn test_example_include() {
        assert!(EXAMPLE_JL.contains("include(\"helpers.jl\")"));
        assert!(EXAMPLE_HELPERS_JL.contains("included_double"));
    }

    #[test]
    fn test_get_package_include() {
        let content = get_package_include("embedded_packages/Example/src/helpers.jl");
        assert!(content.is_some());
        assert!(content.unwrap().contains("included_double"));
    }

    #[test]
    fn test_unknown_package() {
        assert!(get_bundled_package("NonExistent").is_none());
    }

    #[test]
    fn test_bundled_package_names() {
        let names = bundled_package_names();
        assert!(names.contains(&"Example"));
    }

    #[test]
    fn test_symbolics_package_exists() {
        let pkg = get_bundled_package("Symbolics").expect("Symbolics is bundled");
        assert!(pkg.project_toml.contains("name = \"Symbolics\""));
        assert!(pkg.source.contains("module Symbolics"));
        assert!(pkg.source.contains("@variables"));
        assert!(bundled_package_names().contains(&"Symbolics"));
    }

    #[test]
    fn test_symbolics_includes() {
        assert!(SYMBOLICS_JL.contains("include(\"types.jl\")"));
        assert!(SYMBOLICS_JL.contains("include(\"variables.jl\")"));
        assert!(SYMBOLICS_JL.contains("include(\"linear_algebra.jl\")"));
        assert!(
            get_package_include("embedded_packages/Symbolics/src/types.jl")
                .unwrap()
                .contains("struct Sym")
        );
        assert!(
            get_package_include("embedded_packages/Symbolics/src/variables.jl")
                .unwrap()
                .contains("macro variables")
        );
        // The matrix methods constrain the left operand with the bare imported
        // alias `Num`; the qualified `Symbolics.Num` workaround spelling (W-32)
        // was dropped in #8036 once bare-alias dispatch (#8019/#8025) was fixed.
        assert!(
            get_package_include("embedded_packages/Symbolics/src/linear_algebra.jl")
                .unwrap()
                .contains("AbstractMatrix{<:Num}")
        );
    }

    #[test]
    fn test_distributions_package_exists() {
        let pkg = get_bundled_package("Distributions").expect("Distributions is bundled");
        assert!(pkg.project_toml.contains("name = \"Distributions\""));
        assert!(pkg.source.contains("module Distributions"));
        assert!(pkg.source.contains("abstract type Distribution"));
        assert!(pkg.source.contains("export Normal, Uniform, Exponential"));
        assert!(bundled_package_names().contains(&"Distributions"));
    }

    #[test]
    fn test_distributions_includes() {
        assert!(DISTRIBUTIONS_JL.contains("include(\"univariate/continuous.jl\")"));
        assert!(DISTRIBUTIONS_JL.contains("include(\"univariate/discrete.jl\")"));
        assert!(get_package_include(
            "embedded_packages/Distributions/src/univariate/continuous.jl"
        )
        .unwrap()
        .contains("struct Normal"));
        assert!(
            get_package_include("embedded_packages/Distributions/src/univariate/discrete.jl")
                .unwrap()
                .contains("struct Bernoulli")
        );
        assert!(DISTRIBUTIONS_JL.contains("include(\"multivariate/mvnormal.jl\")"));
        assert!(DISTRIBUTIONS_JL.contains("include(\"truncate.jl\")"));
        assert!(
            get_package_include("embedded_packages/Distributions/src/truncate.jl")
                .unwrap()
                .contains("struct Truncated")
        );
        assert!(get_package_include(
            "embedded_packages/Distributions/src/multivariate/mvnormal.jl"
        )
        .unwrap()
        .contains("struct MvNormal"));
        assert!(DISTRIBUTIONS_JL.contains("include(\"fit.jl\")"));
        assert!(
            get_package_include("embedded_packages/Distributions/src/fit.jl")
                .unwrap()
                .contains("fit_mle")
        );
    }

    #[test]
    fn test_statsplots_package_exists() {
        let pkg = get_bundled_package("StatsPlots").expect("StatsPlots is bundled");
        assert!(pkg.project_toml.contains("name = \"StatsPlots\""));
        assert!(pkg.source.contains("module StatsPlots"));
        assert!(pkg.source.contains("using Plots"));
        assert!(pkg.source.contains("using Distributions"));
        assert!(bundled_package_names().contains(&"StatsPlots"));
    }

    #[test]
    fn test_statsplots_exports() {
        let pkg = get_bundled_package("StatsPlots").unwrap();
        assert!(pkg.source.contains("export plot, plot!"));
        assert!(pkg.source.contains("import Plots: plot, plot!"));
    }

    #[test]
    fn test_scimlbase_package_exists() {
        let pkg = get_bundled_package("SciMLBase").expect("SciMLBase is bundled");
        assert!(pkg.project_toml.contains("name = \"SciMLBase\""));
        assert!(pkg.source.contains("module SciMLBase"));
        assert!(pkg.source.contains("struct ODEProblem"));
        assert!(pkg.source.contains("struct ODESolution"));
        assert!(pkg.source.contains("function solve"));
        // Tsit5 + its solve dispatch live in SciMLBase so the algorithm registers
        // on SciMLBase.solve (PR #8050 review; OrdinaryDiffEq re-exports the type).
        assert!(pkg.source.contains("struct Tsit5"));
        assert!(pkg
            .source
            .contains("function solve(prob::ODEProblem, alg::Tsit5"));
        assert!(bundled_package_names().contains(&"SciMLBase"));
    }

    #[test]
    fn test_ordinarydiffeq_package_exists() {
        let pkg = get_bundled_package("OrdinaryDiffEq").expect("OrdinaryDiffEq is bundled");
        assert!(pkg.project_toml.contains("name = \"OrdinaryDiffEq\""));
        assert!(pkg.project_toml.contains("SciMLBase"));
        assert!(pkg.source.contains("module OrdinaryDiffEq"));
        assert!(pkg.source.contains("import SciMLBase"));
        // Tsit5 is re-exported from SciMLBase (not defined here) so the alg method
        // registers on SciMLBase.solve; the facade still exports the name and the
        // local VelocityVerlet algorithm (PR #8050 review).
        assert!(pkg.source.contains("import SciMLBase: Tsit5"));
        assert!(pkg.source.contains("struct VelocityVerlet"));
        assert!(pkg
            .source
            .contains("export SciMLBase, solve, ODEProblem, ODESolution, Tsit5"));
        assert!(bundled_package_names().contains(&"OrdinaryDiffEq"));
    }

    #[test]
    fn test_interact_package_exists() {
        let pkg = get_bundled_package("Interact").expect("Interact is bundled");
        assert!(pkg.project_toml.contains("name = \"Interact\""));
        assert!(pkg.source.contains("module Interact"));
        assert!(pkg.source.contains("macro manipulate"));
        assert!(pkg.source.contains("using Plots"));
        assert!(pkg.source.contains("export Manipulate, @manipulate"));
        assert!(bundled_package_names().contains(&"Interact"));
    }

    #[test]
    fn test_interact_includes() {
        assert!(INTERACT_JL.contains("include(\"types.jl\")"));
        assert!(
            get_package_include("embedded_packages/Interact/src/types.jl")
                .unwrap()
                .contains("struct Manipulate")
        );
    }

    #[test]
    fn test_abstractalgebra_dependency_shims_exist() {
        let preferences = get_bundled_package("Preferences").expect("Preferences is bundled");
        assert!(preferences.project_toml.contains("name = \"Preferences\""));
        assert!(preferences.source.contains("module Preferences"));
        assert!(preferences.source.contains("macro load_preference"));

        let random_extensions =
            get_bundled_package("RandomExtensions").expect("RandomExtensions is bundled");
        assert!(random_extensions
            .project_toml
            .contains("name = \"RandomExtensions\""));
        assert!(random_extensions.source.contains("module RandomExtensions"));

        let sparse_arrays = get_bundled_package("SparseArrays").expect("SparseArrays is bundled");
        assert!(sparse_arrays
            .project_toml
            .contains("name = \"SparseArrays\""));
        assert!(sparse_arrays.source.contains("module SparseArrays"));
        assert!(sparse_arrays.source.contains("struct SparseMatrixCSC"));

        assert!(bundled_package_names().contains(&"Preferences"));
        assert!(bundled_package_names().contains(&"RandomExtensions"));
        assert!(bundled_package_names().contains(&"SparseArrays"));
    }

    #[test]
    fn test_abstractalgebra_package_exists() {
        let pkg = get_bundled_package("AbstractAlgebra").expect("AbstractAlgebra is bundled");
        assert!(pkg.project_toml.contains("name = \"AbstractAlgebra\""));
        assert!(pkg.project_toml.contains("MacroTools"));
        assert!(pkg.project_toml.contains("Preferences"));
        assert!(pkg.project_toml.contains("RandomExtensions"));
        assert!(pkg.project_toml.contains("SparseArrays"));
        assert!(pkg.source.contains("module AbstractAlgebra"));
        assert!(pkg.source.contains("using MacroTools"));
        assert!(pkg.source.contains("include(\"imports.jl\")"));
        assert!(pkg.source.contains("include(\"exports.jl\")"));
        assert!(pkg.source.contains("include(\"AliasMacro.jl\")"));
        assert!(pkg.source.contains("include(\"ConcreteTypes.jl\")"));
        assert!(bundled_package_names().contains(&"AbstractAlgebra"));
    }

    #[test]
    fn test_abstractalgebra_includes() {
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/imports.jl")
                .unwrap()
                .contains("import Base")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/exports.jl")
                .unwrap()
                .contains("export @alias")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/AliasMacro.jl")
                .unwrap()
                .contains("macro alias")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/Attributes.jl")
                .unwrap()
                .contains("macro attributes")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/ConcreteTypes.jl")
                .unwrap()
                .contains("UniversalRing")
        );
        assert!(get_package_include(
            "embedded_packages/AbstractAlgebra/src/fundamental_interface.jl"
        )
        .unwrap()
        .contains("check_parent"));
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/julia/Integer.jl")
                .unwrap()
                .contains("const JuliaZZ")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/Poly.jl")
                .unwrap()
                .contains("polynomial_ring")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/FractionResidue.jl")
                .unwrap()
                .contains("residue_ring")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/Matrix.jl")
                .unwrap()
                .contains("matrix_space")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/Module.jl")
                .unwrap()
                .contains("free_module")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/Map.jl")
                .unwrap()
                .contains("identity_map")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/PermGroups.jl")
                .unwrap()
                .contains("SymmetricGroup")
        );
        assert!(
            get_package_include("embedded_packages/AbstractAlgebra/src/YoungTabs.jl")
                .unwrap()
                .contains("YoungTableau")
        );
    }

    #[test]
    fn test_statsplots_includes() {
        assert!(STATS_PLOTS_JL.contains("include(\"distributions.jl\")"));
        assert!(
            get_package_include("embedded_packages/StatsPlots/src/distributions.jl")
                .unwrap()
                .contains("plot(d::Normal")
        );
    }

    #[test]
    fn test_macrotools_package_exists() {
        let pkg = get_bundled_package("MacroTools").expect("MacroTools is bundled");
        assert!(pkg.project_toml.contains("name = \"MacroTools\""));
        assert!(pkg.source.contains("module MacroTools"));
        assert!(pkg.source.contains("include(\"match/match.jl\")"));
        assert!(pkg.source.contains("export @match, @capture"));
        assert!(bundled_package_names().contains(&"MacroTools"));
    }

    #[test]
    fn test_macrotools_includes() {
        assert!(MACRO_TOOLS_JL.contains("include(\"utils.jl\")"));
        assert!(
            get_package_include("embedded_packages/MacroTools/src/match/macro.jl")
                .unwrap()
                .contains("macro capture")
        );
        assert!(
            get_package_include("embedded_packages/MacroTools/src/utils.jl")
                .unwrap()
                .contains("postwalk")
        );
        assert!(
            get_package_include("embedded_packages/MacroTools/src/examples/threading.jl")
                .unwrap()
                .contains("macro >")
        );
        assert_eq!(
            get_package_include("embedded_packages/MacroTools/animals.txt")
                .unwrap()
                .lines()
                .count(),
            214
        );
    }

    #[test]
    fn test_precompiletools_package_exists() {
        let pkg = get_bundled_package("PrecompileTools").expect("PrecompileTools is bundled");
        assert!(pkg.project_toml.contains("name = \"PrecompileTools\""));
        assert!(pkg.source.contains("module PrecompileTools"));
        assert!(pkg.source.contains("macro compile_workload"));
        assert!(pkg.source.contains("macro setup_workload"));
        assert!(bundled_package_names().contains(&"PrecompileTools"));
    }

    #[test]
    fn test_staticarrays_core_package_exists() {
        let pkg = get_bundled_package("StaticArraysCore").expect("StaticArraysCore is bundled");
        assert!(pkg.project_toml.contains("name = \"StaticArraysCore\""));
        assert!(pkg.source.contains("module StaticArraysCore"));
        assert!(pkg.source.contains("include(\"SOneTo.jl\")"));
        assert!(pkg.source.contains("export StaticArray"));
        assert!(pkg.source.contains("export @SVector"));
        assert!(pkg.source.contains("export Size, Length"));
        assert!(bundled_package_names().contains(&"StaticArraysCore"));
    }

    #[test]
    fn test_staticarrays_core_includes() {
        assert!(
            get_package_include("embedded_packages/StaticArraysCore/src/SOneTo.jl")
                .unwrap()
                .contains("struct SOneTo")
        );
        assert!(
            get_package_include("embedded_packages/StaticArraysCore/src/traits.jl")
                .unwrap()
                .contains("similar_type")
        );
        assert!(
            get_package_include("embedded_packages/StaticArraysCore/src/types.jl")
                .unwrap()
                .contains("macro SVector")
        );
    }

    #[test]
    fn test_staticarrays_package_exists() {
        let pkg = get_bundled_package("StaticArrays").expect("StaticArrays is bundled");
        assert!(pkg.project_toml.contains("name = \"StaticArrays\""));
        assert!(pkg.project_toml.contains("PrecompileTools"));
        assert!(pkg.source.contains("module StaticArrays"));
        assert!(pkg.source.contains("include(\"SArray.jl\")"));
        assert!(pkg.source.contains("include(\"abstractarray.jl\")"));
        assert!(pkg.source.contains("export @SVector"));
        assert!(pkg.source.contains("export tuple_length"));
        assert!(bundled_package_names().contains(&"StaticArrays"));
    }

    #[test]
    fn test_staticarrays_includes() {
        assert!(STATIC_ARRAYS_JL.contains("include(\"SVector.jl\")"));
        assert!(STATIC_ARRAYS_JL.contains("include(\"SMatrix.jl\")"));
        assert!(
            get_package_include("embedded_packages/StaticArrays/src/abstractarray.jl")
                .unwrap()
                .contains("abstract type StaticArray")
        );
        assert!(
            get_package_include("embedded_packages/StaticArrays/src/SVector.jl")
                .unwrap()
                .contains("macro SVector")
        );
        assert!(
            get_package_include("embedded_packages/StaticArrays/src/indexing.jl")
                .unwrap()
                .contains("Base.size")
        );
    }

    #[test]
    fn test_optim_dependency_packages_exist() {
        for name in [
            "NLSolversBase",
            "LineSearches",
            "ADTypes",
            "NaNMath",
            "EnumX",
            "FillArrays",
            "PositiveFactorizations",
        ] {
            let pkg = get_bundled_package(name)
                .unwrap_or_else(|| panic!("{name} should be a bundled Optim dependency"));
            assert!(pkg.project_toml.contains(&format!("name = \"{name}\"")));
            assert!(pkg.source.contains(&format!("module {name}")));
            assert!(bundled_package_names().contains(&name));
        }
        let nls = get_bundled_package("NLSolversBase").unwrap();
        assert!(nls.source.contains("struct NonDifferentiable"));
        assert!(nls.source.contains("function value_gradient!"));
        let ls = get_bundled_package("LineSearches").unwrap();
        assert!(ls.source.contains("struct BackTracking"));
    }

    #[test]
    fn test_data_structures_package_exists() {
        let pkg = get_bundled_package("DataStructures").expect("DataStructures is bundled");
        assert!(pkg.project_toml.contains("name = \"DataStructures\""));
        assert!(pkg.source.contains("module DataStructures"));
        assert!(pkg.source.contains("include(\"heaps/arrays_as_heaps.jl\")"));
        assert!(bundled_package_names().contains(&"DataStructures"));
        assert!(get_package_include(
            "embedded_packages/DataStructures/src/heaps/arrays_as_heaps.jl"
        )
        .unwrap()
        .contains("function heappush!"));
    }

    #[test]
    fn test_quadgk_package_exists() {
        let pkg = get_bundled_package("QuadGK").expect("QuadGK is bundled");
        assert!(pkg.project_toml.contains("name = \"QuadGK\""));
        assert!(pkg.source.contains("module QuadGK"));
        assert!(pkg.source.contains("include(\"gausskronrod.jl\")"));
        assert!(bundled_package_names().contains(&"QuadGK"));
        assert!(
            get_package_include("embedded_packages/QuadGK/src/gausskronrod.jl")
                .unwrap()
                .contains("function gauss")
        );
        assert!(
            get_package_include("embedded_packages/QuadGK/src/evalrule.jl")
                .unwrap()
                .contains("struct Segment")
        );
    }

    #[test]
    fn test_optim_package_exists() {
        let pkg = get_bundled_package("Optim").expect("Optim is bundled");
        assert!(pkg.project_toml.contains("name = \"Optim\""));
        assert!(pkg.project_toml.contains("NLSolversBase"));
        assert!(pkg.project_toml.contains("LineSearches"));
        assert!(pkg.source.contains("module Optim"));
        assert!(pkg.source.contains("export optimize,"));
        assert!(pkg
            .source
            .contains("include(\"univariate/solvers/golden_section.jl\")"));
        assert!(pkg
            .source
            .contains("include(\"multivariate/solvers/zeroth_order/nelder_mead.jl\")"));
        assert!(bundled_package_names().contains(&"Optim"));
    }

    #[test]
    fn test_optim_nested_includes_resolve() {
        // Loader cache hashing relies on every included Optim source resolving
        // through get_package_include (Issue #7478).
        assert!(
            get_package_include("embedded_packages/Optim/src/univariate/solvers/brent.jl")
                .unwrap()
                .contains("struct Brent")
        );
        assert!(get_package_include(
            "embedded_packages/Optim/src/multivariate/solvers/zeroth_order/nelder_mead.jl"
        )
        .unwrap()
        .contains("struct NelderMead"));
        assert!(get_package_include(
            "embedded_packages/Optim/src/multivariate/solvers/first_order/gradient_descent.jl"
        )
        .unwrap()
        .contains("struct GradientDescent"));
        // BFGS solver + its HagerZhang line search (Issue #8059) must resolve
        // through the loader so the cache hashing covers them.
        assert!(get_package_include(
            "embedded_packages/Optim/src/multivariate/solvers/first_order/bfgs.jl"
        )
        .unwrap()
        .contains("struct BFGS"));
        assert!(
            get_package_include("embedded_packages/LineSearches/src/hagerzhang.jl")
                .unwrap()
                .contains("function hagerzhang_search")
        );
        // `generic.jl` no longer defines a `_sqrt` Newton workaround: the
        // builtin `sqrt` recursion was fixed at the source (Issue #8042, W-34
        // removed), so `_nmobjective` uses `sqrt` directly. Assert on a stable
        // helper that remains in the file instead.
        assert!(
            get_package_include("embedded_packages/Optim/src/utilities/generic.jl")
                .unwrap()
                .contains("function _var")
        );
        assert!(
            get_package_include("embedded_packages/Optim/src/maximize.jl")
                .unwrap()
                .contains("maximize")
        );
    }
}
