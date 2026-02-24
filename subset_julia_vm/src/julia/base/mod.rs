//! Pure Julia implementations of Base functions.
//!
//! This module provides Julia source code for functions that are
//! compiled before user code, allowing them to be called like
//! regular functions.
//!
//! IMPORTANT: This module only contains functions that exist in Julia Base.
//! All function names MUST match Julia exactly to ensure subset compatibility.
//!
//! The structure mirrors Julia's Base module:
//! - boot.jl: Core.Intrinsics wrappers + abstract type hierarchy
//! - operators.jl: Pairwise comparison operations (min, max, etc.)
//! - number.jl: Number predicates (iszero, isone, etc.)
//! - bool.jl: Boolean operations (xor, nand, nor)
//! - math.jl: Mathematical functions (sign, clamp, mod, etc.)
//! - intfuncs.jl: Integer/number-theoretic functions (gcd, lcm, etc.)
//! - floatfuncs.jl: Floating-point utilities (isinteger, modf, etc.)
//! - mathconstants.jl: Mathematical constants (π, ℯ, etc.)
//! - rational.jl: Rational number type (Rational struct, arithmetic)
//! - complex.jl: Complex number type (Complex struct, arithmetic)
//! - array.jl: Array manipulation functions (minimum, maximum, prod, etc.)
//! - range.jl: Range utilities (first, last, step, etc.)
//! - generator.jl: Generator type and iterator traits
//! - iterators.jl: Iterator types (Enumerate, Zip, Take, Drop, etc.)
//! - reduce.jl: Reduction operations (extrema, count, diff, etc.)
//! - accumulate.jl: Cumulative operations (cumsum, cumprod, accumulate)
//! - combinatorics.jl: Combinatorial functions (binomial)
//! - sort.jl: Sorting algorithms (sort, sortperm, searchsorted, etc.)
//! - strings/: String/character utilities
//! - tuple.jl: Tuple utilities (empty - use standard functions)
//! - set.jl: Set operations (unique, union, intersect, setdiff, etc.)

/// Core.Intrinsics wrappers (add_int, sub_int, sdiv_int, etc.)
/// + Abstract type hierarchy (Number, Real, Integer, etc.)
/// + Val{x} type for value-as-type-parameter
/// This must be loaded first as other modules depend on these intrinsics and types.
pub const BOOT_JL: &str = include_str!("boot.jl");

/// Exception types (Exception, DimensionMismatch, KeyError, StringIndexError, etc.)
/// Based on julia/base/boot.jl, julia/base/array.jl, julia/base/abstractdict.jl
pub const ERROR_JL: &str = include_str!("error.jl");

/// Error display with showerror function
/// Based on julia/base/errorshow.jl
pub const ERRORSHOW_JL: &str = include_str!("errorshow.jl");

/// Type promotion system (promote_rule, promote_type, promote, convert)
/// Enabled with runtime Type{T} dispatch support via CallTypedDispatch instruction.
pub const PROMOTION_JL: &str = include_str!("promotion.jl");

/// Pairwise comparison operations (min, max, minmax, copysign, flipsign)
pub const OPERATORS_JL: &str = include_str!("operators.jl");

/// Number predicates and operations (iszero, isone, identity)
pub const NUMBER_JL: &str = include_str!("number.jl");

/// Boolean operations (xor, nand, nor)
pub const BOOL_JL: &str = include_str!("bool.jl");

/// Missing value support (ismissing, coalesce)
pub const MISSING_JL: &str = include_str!("missing.jl");

/// Mathematical functions (sign, clamp, mod, etc.)
pub const MATH_JL: &str = include_str!("math.jl");

/// Integer arithmetic using intrinsics (specialized div, gcd, etc.)
pub const INT_JL: &str = include_str!("int.jl");

/// Integer/number-theoretic functions (gcd, lcm, factorial, etc.)
pub const INTFUNCS_JL: &str = include_str!("intfuncs.jl");

/// Floating-point utilities (isinteger, modf, ldexp, etc.)
pub const FLOATFUNCS_JL: &str = include_str!("floatfuncs.jl");

/// Irrational type for exact representation of irrational constants
/// Based on Julia's base/irrationals.jl
pub const IRRATIONALS_JL: &str = include_str!("irrationals.jl");

/// Mathematical constants (π, ℯ, etc.)
/// Based on Julia's base/mathconstants.jl
pub const MATHCONSTANTS_JL: &str = include_str!("mathconstants.jl");

/// Rational number type (Rational struct, arithmetic operations)
pub const RATIONAL_JL: &str = include_str!("rational.jl");

/// Complex number type (Complex struct, arithmetic operations)
pub const COMPLEX_JL: &str = include_str!("complex.jl");

/// Array manipulation functions (minimum, maximum, prod, etc.)
pub const ARRAY_JL: &str = include_str!("array.jl");

/// SubArray type for array views (view, @view, @views)
/// Based on Julia's base/subarray.jl and base/views.jl
pub const SUBARRAY_JL: &str = include_str!("subarray.jl");

/// Memory{T} typed memory buffer
/// Based on Julia's base/genericmemory.jl
/// Low-level fixed-size typed buffer used internally by Vector, Dict, etc.
pub const GENERICMEMORY_JL: &str = include_str!("genericmemory.jl");

/// Range utilities (first, last, step, eachindex, etc.)
pub const RANGE_JL: &str = include_str!("range.jl");

/// Generator type and iterator traits (IteratorSize, IteratorEltype)
/// Based on Julia's base/generator.jl
pub const GENERATOR_JL: &str = include_str!("generator.jl");

/// Iterator types (Enumerate, Zip, Take, Drop, etc.)
/// Based on Julia's base/iterators.jl
pub const ITERATORS_JL: &str = include_str!("iterators.jl");

/// Abstract array utilities (foreach, etc.)
/// Based on Julia's base/abstractarray.jl
pub const ABSTRACTARRAY_JL: &str = include_str!("abstractarray.jl");

/// Reduction operations (extrema, count, findmax, findmin, diff)
pub const REDUCE_JL: &str = include_str!("reduce.jl");

/// Cumulative operations (cumsum, cumprod)
/// Based on Julia's base/accumulate.jl
pub const ACCUMULATE_JL: &str = include_str!("accumulate.jl");

/// Combinatorial functions (binomial)
pub const COMBINATORICS_JL: &str = include_str!("combinatorics.jl");

/// Sorting algorithms (sort, sortperm, searchsorted, etc.)
pub const SORT_JL: &str = include_str!("sort.jl");

/// Character classification functions (isdigit, isletter, etc.)
/// Based on Julia's base/strings/basic.jl
pub const STRINGS_BASIC_JL: &str = include_str!("strings/basic.jl");

/// String search functions (occursin, contains, startswith, endswith)
/// Based on Julia's base/strings/search.jl
pub const STRINGS_SEARCH_JL: &str = include_str!("strings/search.jl");

/// String manipulation functions (replace, join, strip, etc.)
/// Based on Julia's base/strings/util.jl
pub const STRINGS_UTIL_JL: &str = include_str!("strings/util.jl");

/// Unicode string functions (uppercase, lowercase)
/// Based on Julia's base/strings/unicode.jl
pub const STRINGS_UNICODE_JL: &str = include_str!("strings/unicode.jl");

/// Tuple utilities (empty - use standard functions like sum, prod)
pub const TUPLE_JL: &str = include_str!("tuple.jl");

/// Set operations (unique, union, intersect, setdiff, etc.)
pub const SET_JL: &str = include_str!("set.jl");

/// Dict utilities (keytype, valtype)
pub const DICT_JL: &str = include_str!("dict.jl");

/// Macros (user-defined macros for Base, e.g., @inline, @noinline)
pub const MACROS_JL: &str = include_str!("macros.jl");

/// Timing macros (@time, @elapsed, @timed, @timev, @showtime, @allocated, @allocations)
/// Based on Julia's base/timing.jl
pub const TIMING_JL: &str = include_str!("timing.jl");

/// Printf macros (@sprintf, @printf)
/// Based on Julia's stdlib/Printf
pub const PRINTF_JL: &str = include_str!("printf.jl");

/// Hash functions (hash, object_id)
/// Based on Julia's base/hashing.jl
pub const HASHING_JL: &str = include_str!("hashing.jl");

/// IO operations (IOBuffer, sprint)
pub const IO_JL: &str = include_str!("io.jl");

/// Multimedia (MIME type system for rich display)
/// Based on Julia's base/multimedia.jl
pub const MULTIMEDIA_JL: &str = include_str!("multimedia.jl");

/// BigInt/BigFloat utilities (big function, precision, rounding)
/// Based on Julia's base/gmp.jl and base/mpfr.jl
pub const GMP_JL: &str = include_str!("gmp.jl");

/// Meta module (metaprogramming utilities: parse, isexpr, quot)
pub const META_JL: &str = include_str!("meta.jl");

/// Reflection module (fieldnames, fieldtypes, nfields, Method struct)
pub const REFLECTION_JL: &str = include_str!("reflection.jl");

/// Version number type and VERSION constant
/// Based on Julia's base/version.jl
pub const VERSION_JL: &str = include_str!("version.jl");

/// Pair type (key => value pairs)
/// Based on Julia's base/pair.jl
pub const PAIR_JL: &str = include_str!("pair.jl");

/// Path manipulation functions (basename, dirname, splitdir, etc.)
/// Based on Julia's base/path.jl
pub const PATH_JL: &str = include_str!("path.jl");

/// Floating-point arithmetic and comparisons using intrinsics
/// Based on Julia's base/float.jl
pub const FLOAT_JL: &str = include_str!("float.jl");

/// Trigonometric functions (sin, cos, tan, asin, acos, atan)
/// Based on Julia's base/special/trig.jl (FDLIBM polynomial approximations)
pub const TRIG_JL: &str = include_str!("special/trig.jl");

/// Exponential function (exp)
/// Based on Julia's base/special/exp.jl (FDLIBM polynomial approximation)
pub const SPECIAL_EXP_JL: &str = include_str!("special/exp.jl");

/// Logarithmic function (log)
/// Based on Julia's base/special/log.jl (FDLIBM polynomial approximation)
pub const SPECIAL_LOG_JL: &str = include_str!("special/log.jl");

/// Utility functions (printstyled, etc.)
/// Based on Julia's base/util.jl
pub const UTIL_JL: &str = include_str!("util.jl");

/// Rounding mode types (RoundingMode, RoundNearest, RoundToZero, etc.)
/// Based on Julia's base/rounding.jl
pub const ROUNDING_JL: &str = include_str!("rounding.jl");

/// Trait types (OrderStyle, ArithmeticStyle, RangeStepStyle)
/// Based on Julia's base/traits.jl
pub const TRAITS_JL: &str = include_str!("traits.jl");

/// Runtime introspection functions (isexported, ispublic)
/// Based on Julia's base/runtime_internals.jl
pub const RUNTIME_INTERNALS_JL: &str = include_str!("runtime_internals.jl");

/// Docs utilities (HTML, Text types and string macros)
/// Based on Julia's base/docs/utils.jl
pub const DOCS_UTILS_JL: &str = include_str!("docs/utils.jl");

/// Some type and something() function
/// Based on Julia's base/some.jl
pub const SOME_JL: &str = include_str!("some.jl");

/// Essential functions (ifelse, oftype)
/// Based on Julia's base/essentials.jl
pub const ESSENTIALS_JL: &str = include_str!("essentials.jl");

/// Lock types and synchronization primitives (ReentrantLock, SpinLock, Condition)
/// Based on Julia's base/lock.jl
pub const LOCK_JL: &str = include_str!("lock.jl");

/// Task type and functions (Task, schedule, fetch, wait)
/// Based on Julia's base/task.jl
/// Implements cooperative multitasking for single-threaded execution
pub const TASK_JL: &str = include_str!("task.jl");

/// Channel type for producer/consumer patterns
/// Based on Julia's base/channels.jl
pub const CHANNELS_JL: &str = include_str!("channels.jl");

/// String to number parsing (parse, tryparse for Int64)
/// Based on Julia's base/parse.jl
/// Note: Float64 parsing remains as Rust intrinsic.
pub const PARSE_JL: &str = include_str!("parse.jl");

/// Broadcast infrastructure (BroadcastStyle, Broadcasted, Extruded, materialize, etc.)
/// Based on Julia's base/broadcast.jl
/// Phase 1-2: BroadcastStyle type hierarchy, binary rules, shape computation
/// Phase 3-4: Indexing + Materialization (Issue #2537-#2543)
pub const BROADCAST_JL: &str = include_str!("broadcast.jl");

/// Get the complete Base source code.
/// This concatenates all Julia source files in the correct order.
/// Order matters: abstract type hierarchy first, then basic types, math, arrays, and higher-order functions.
pub fn get_base() -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        BOOT_JL,              // 1. Intrinsics + abstract types + Val
        ERROR_JL,             // 2. Exceptions
        PROMOTION_JL,         // 3. Type promotion
        IRRATIONALS_JL,       // 4. Irrational type
        MATHCONSTANTS_JL,     // 5. Math constants
        ROUNDING_JL,          // 6. Rounding modes
        TRAITS_JL,            // 7. Trait types
        SOME_JL,              // 8. Some/something
        ESSENTIALS_JL,        // 9. ifelse, oftype
        OPERATORS_JL,         // 10. Operators + Returns
        NUMBER_JL,            // 11. Number predicates
        BOOL_JL,              // 12. Boolean ops
        MISSING_JL,           // 13. Missing
        MATH_JL,              // 14. Math
        INT_JL,               // 15. Integer
        INTFUNCS_JL,          // 16. Integer functions
        FLOATFUNCS_JL,        // 17. Float functions
        FLOAT_JL,             // 18. Float arithmetic/comparisons
        TRIG_JL,              // 18a. Special: trig functions (sin, cos, tan, asin, acos, atan)
        SPECIAL_EXP_JL,       // 18b. Special: exp function
        SPECIAL_LOG_JL,       // 18c. Special: log function
        RATIONAL_JL,          // 19. Rational
        COMPLEX_JL,           // 20. Complex
        ARRAY_JL,             // 21. Array
        SUBARRAY_JL,          // 22. SubArray
        GENERICMEMORY_JL,     // 22a. Memory{T} buffer
        RANGE_JL,             // 23. Range
        GENERATOR_JL,         // 24. Generator + traits
        ITERATORS_JL,         // 25. Iterators
        ABSTRACTARRAY_JL,     // 26. Abstract array utils (foreach)
        REDUCE_JL,            // 27. Reductions (+ any/all)
        ACCUMULATE_JL,        // 28. cumsum/cumprod
        COMBINATORICS_JL,     // 29. Combinatorics
        SORT_JL,              // 30. Sort
        STRINGS_BASIC_JL,     // 31. Char functions
        STRINGS_SEARCH_JL,    // 32. String search
        STRINGS_UTIL_JL,      // 33. String utils
        STRINGS_UNICODE_JL,   // 33a. Unicode (uppercase, lowercase)
        TUPLE_JL,             // 34. Tuple
        SET_JL,               // 35. Set
        DICT_JL,              // 36. Dict
        MACROS_JL,            // 37. Macros
        TIMING_JL,            // 38. Timing macros
        PRINTF_JL,            // 39. Printf macros
        HASHING_JL,           // 40. Hash functions
        IO_JL,                // 41. IO
        ERRORSHOW_JL,         // 42. Error display
        MULTIMEDIA_JL,        // 43. MIME
        GMP_JL,               // 44. BigInt/BigFloat
        META_JL,              // 45. Meta
        REFLECTION_JL,        // 46. Reflection
        RUNTIME_INTERNALS_JL, // 47. Runtime
        VERSION_JL,           // 48. Version
        PAIR_JL,              // 49. Pair
        PATH_JL,              // 50. Path
        UTIL_JL,              // 51. Util
        DOCS_UTILS_JL,        // 52. Docs
        LOCK_JL,              // 53. Lock primitives
        TASK_JL,              // 54. Task type
        CHANNELS_JL,          // 55. Channel type
        PARSE_JL,             // 56. Parse/tryparse for Int64
        BROADCAST_JL,         // 57. Broadcast infrastructure
    )
}

/// Check if Base is enabled.
pub fn is_base_enabled() -> bool {
    true
}

/// Get the minimal AoT prelude for pure Rust code generation.
/// This prelude contains only fully-typed functions without runtime Value type dependency.
/// It includes essential operations for Int64, Float64, and Bool types.
///
/// Note: The prelude is now located in the `internal` module.
/// This function is a compatibility wrapper.
pub fn get_aot_prelude() -> String {
    super::internal::get_aot_prelude()
}

// Backward compatibility aliases
pub fn get_prelude() -> String {
    get_base()
}

pub fn is_prelude_enabled() -> bool {
    is_base_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_not_empty() {
        let base = get_base();
        assert!(!base.is_empty());
    }

    #[test]
    fn test_operators_functions() {
        assert!(OPERATORS_JL.contains("function min"));
        assert!(OPERATORS_JL.contains("function max"));
        assert!(OPERATORS_JL.contains("function minmax"));
        assert!(OPERATORS_JL.contains("function copysign"));
    }

    #[test]
    fn test_number_functions() {
        assert!(NUMBER_JL.contains("function iszero"));
        assert!(NUMBER_JL.contains("function isone"));
        assert!(NUMBER_JL.contains("function identity"));
        // ispositive/isnegative added in Julia 1.12 (PR #53677)
        assert!(NUMBER_JL.contains("function ispositive"));
        assert!(NUMBER_JL.contains("function isnegative"));
    }

    #[test]
    fn test_bool_functions() {
        assert!(BOOL_JL.contains("function xor"));
        assert!(BOOL_JL.contains("function nand"));
        assert!(BOOL_JL.contains("function nor"));
        // implies was removed - not in Julia Base
        assert!(!BOOL_JL.contains("function implies"));
    }

    #[test]
    fn test_math_functions() {
        assert!(MATH_JL.contains("function sign"));
        assert!(MATH_JL.contains("function clamp"));
        assert!(MATH_JL.contains("function hypot"));
    }

    #[test]
    fn test_intfuncs_functions() {
        // Note: gcd(Int64, Int64) is in int.jl, builtin gcd handles BigInt
        // lcm is implemented as a Rust builtin for BigInt support
        assert!(INTFUNCS_JL.contains("function factorial"));
        assert!(INTFUNCS_JL.contains("function isqrt"));
        assert!(INTFUNCS_JL.contains("function powermod"));
        assert!(INTFUNCS_JL.contains("function trailing_zeros"));
        // count_digits was removed - use ndigits(n) (Julia standard)
        assert!(!INTFUNCS_JL.contains("function count_digits"));
        // ctz was removed - use trailing_zeros(n) (Julia standard)
        assert!(!INTFUNCS_JL.contains("function ctz"));
    }

    #[test]
    fn test_floatfuncs_functions() {
        assert!(FLOATFUNCS_JL.contains("function isinteger"));
        assert!(FLOATFUNCS_JL.contains("function modf"));
        assert!(FLOATFUNCS_JL.contains("function ldexp"));
        // round_digits was removed - not in Julia (use round(x, digits=n))
        assert!(!FLOATFUNCS_JL.contains("function round_digits"));
    }

    #[test]
    fn test_float_functions() {
        // Float64 arithmetic operators
        assert!(FLOAT_JL.contains("function Base.:(+)(x::Float64, y::Float64)"));
        assert!(FLOAT_JL.contains("function Base.:(-)(x::Float64, y::Float64)"));
        assert!(FLOAT_JL.contains("function Base.:(*)(x::Float64, y::Float64)"));
        assert!(FLOAT_JL.contains("function Base.:(/)(x::Float64, y::Float64)"));
        // Float32 arithmetic operators
        assert!(FLOAT_JL.contains("function Base.:(+)(x::Float32, y::Float32)"));
        assert!(FLOAT_JL.contains("function Base.:(-)(x::Float32, y::Float32)"));
        assert!(FLOAT_JL.contains("function Base.:(*)(x::Float32, y::Float32)"));
        assert!(FLOAT_JL.contains("function Base.:(/)(x::Float32, y::Float32)"));
        // signbit and abs
        assert!(FLOAT_JL.contains("function signbit(x::Float64)"));
        assert!(FLOAT_JL.contains("function abs(x::Float64)"));
    }

    #[test]
    fn test_irrationals() {
        // Irrational type definition for user-defined irrational constants
        assert!(IRRATIONALS_JL.contains("abstract type AbstractIrrational <: Real"));
        assert!(IRRATIONALS_JL.contains("struct Irrational{sym} <: AbstractIrrational"));
    }

    #[test]
    fn test_mathconstants() {
        // Mathematical constants as Float64 values
        assert!(MATHCONSTANTS_JL.contains("const π = 3.141592653589793"));
        assert!(MATHCONSTANTS_JL.contains("const pi = π"));
        assert!(MATHCONSTANTS_JL.contains("const ℯ = 2.718281828459045"));
        assert!(MATHCONSTANTS_JL.contains("const e = ℯ"));
    }

    #[test]
    fn test_rational_functions() {
        // Parametric struct with type constraint and supertype
        assert!(RATIONAL_JL.contains("struct Rational{T<:Integer} <: Real"));
        assert!(RATIONAL_JL.contains("function numerator"));
        assert!(RATIONAL_JL.contains("function denominator"));
        // Now uses Julia-standard operator overloading (bare Rational, not where T)
        assert!(RATIONAL_JL.contains("function Base.:+(x::Rational, y::Rational)"));
        assert!(RATIONAL_JL.contains("function Base.:-(x::Rational, y::Rational)"));
        assert!(RATIONAL_JL.contains("function Base.:*(x::Rational, y::Rational)"));
        assert!(RATIONAL_JL.contains("function Base.:/(x::Rational, y::Rational)"));
        assert!(RATIONAL_JL.contains("function inv(x::Rational)"));
        // Old non-standard names should not exist
        assert!(!RATIONAL_JL.contains("function add_rational"));
        assert!(!RATIONAL_JL.contains("function mul_rational"));
        assert!(!RATIONAL_JL.contains("function inv_rational"));
        assert!(!RATIONAL_JL.contains("function float_rational"));
        assert!(!RATIONAL_JL.contains("function rationalize_simple"));
    }

    #[test]
    fn test_complex_functions() {
        assert!(COMPLEX_JL.contains("struct Complex{T<:Real}"));
        // Typed complex constructors
        assert!(COMPLEX_JL.contains("function complex(r::Int64, i::Int64)"));
        assert!(COMPLEX_JL.contains("function complex(r::Float64, i::Float64)"));
        // Parametric accessors with where clause (matches official Julia style)
        assert!(COMPLEX_JL.contains("function real(z::Complex{T}) where T<:Real"));
        assert!(COMPLEX_JL.contains("function imag(z::Complex{T}) where T<:Real"));
        assert!(COMPLEX_JL.contains("function abs(z::Complex{T}) where {T<:Real}"));
        assert!(COMPLEX_JL.contains("function abs2(z::Complex{T}) where {T<:Real}"));
        // Functions with where clause
        assert!(COMPLEX_JL.contains("function conj(z::Complex{T}) where {T<:Real}"));
        // Generic arithmetic operators for Complex types (Issue #2427)
        assert!(COMPLEX_JL.contains("Base.:+(z::Complex, w::Complex)"));
        assert!(COMPLEX_JL.contains("Base.:-(z::Complex, w::Complex)"));
        assert!(COMPLEX_JL.contains("Base.:*(z::Complex, w::Complex)"));
        assert!(COMPLEX_JL.contains("function Base.:/(z::Complex, w::Complex)"));
    }

    #[test]
    fn test_array_functions() {
        // Array{T} Pure Julia struct definition (Issue #2760)
        // Fields are untyped due to type coercion limitations with parametric field types
        assert!(ARRAY_JL.contains("mutable struct Array{T}"));
        assert!(ARRAY_JL.contains("_mem"));
        assert!(ARRAY_JL.contains("_size"));
        assert!(ARRAY_JL.contains("function prod"));
        assert!(ARRAY_JL.contains("function minimum"));
        assert!(ARRAY_JL.contains("function maximum"));
        assert!(ARRAY_JL.contains("function cat"));
        assert!(ARRAY_JL.contains("function mapslices"));
        // dims keyword argument support (merged into single functions)
        assert!(ARRAY_JL.contains("function sum(arr; dims=0)"));
        assert!(ARRAY_JL.contains("function prod(arr; dims=0)"));
        assert!(ARRAY_JL.contains("function minimum(arr; dims=0)"));
        assert!(ARRAY_JL.contains("function maximum(arr; dims=0)"));
        assert!(ARRAY_JL.contains("function sortslices"));
        // In-place reduction functions (Issue #1964)
        assert!(ARRAY_JL.contains("function sum!"));
        assert!(ARRAY_JL.contains("function prod!"));
        assert!(ARRAY_JL.contains("function maximum!"));
        assert!(ARRAY_JL.contains("function minimum!"));
        assert!(ARRAY_JL.contains("function permutedims!"));
    }

    #[test]
    fn test_range_functions() {
        assert!(RANGE_JL.contains("function first"));
        assert!(RANGE_JL.contains("function last"));
        assert!(RANGE_JL.contains("function step"));
        assert!(RANGE_JL.contains("function eachindex"));
        // linspace, logspace, geomspace were removed - not in Julia Base
        assert!(!RANGE_JL.contains("function linspace"));
        assert!(!RANGE_JL.contains("function logspace"));
        assert!(!RANGE_JL.contains("function geomspace"));
    }

    #[test]
    fn test_reduce_functions() {
        assert!(REDUCE_JL.contains("function extrema"));
        assert!(REDUCE_JL.contains("function findmax"));
        assert!(REDUCE_JL.contains("function findmin"));
        assert!(REDUCE_JL.contains("function diff"));
        // any and all are now in reduce.jl
        assert!(REDUCE_JL.contains("function any(arr)"));
        assert!(REDUCE_JL.contains("function all(arr)"));
        // dims keyword argument support (merged into single function)
        assert!(REDUCE_JL.contains("function extrema(arr; dims=0)"));
    }

    #[test]
    fn test_accumulate_functions() {
        assert!(ACCUMULATE_JL.contains("function cumsum"));
        assert!(ACCUMULATE_JL.contains("function cumprod"));
        assert!(ACCUMULATE_JL.contains("function accumulate"));
        // In-place cumulative functions (Issue #1966)
        assert!(ACCUMULATE_JL.contains("function cumsum!"));
        assert!(ACCUMULATE_JL.contains("function cumprod!"));
        assert!(ACCUMULATE_JL.contains("function accumulate!"));
    }

    #[test]
    fn test_combinatorics_functions() {
        assert!(COMBINATORICS_JL.contains("function binomial"));
        // fibonacci, catalan, stirling2, bell were removed - not in Julia Base
        assert!(!COMBINATORICS_JL.contains("function fibonacci"));
        assert!(!COMBINATORICS_JL.contains("function catalan"));
        assert!(!COMBINATORICS_JL.contains("function stirling2"));
        assert!(!COMBINATORICS_JL.contains("function bell"));
    }

    #[test]
    fn test_generator_functions() {
        assert!(GENERATOR_JL.contains("struct Generator"));
        assert!(GENERATOR_JL.contains("abstract type IteratorSize"));
        assert!(GENERATOR_JL.contains("struct HasLength"));
        assert!(GENERATOR_JL.contains("struct SizeUnknown"));
    }

    #[test]
    fn test_iterators_functions() {
        assert!(ITERATORS_JL.contains("struct Enumerate"));
        assert!(ITERATORS_JL.contains("struct Zip"));
        assert!(ITERATORS_JL.contains("struct Take"));
        assert!(ITERATORS_JL.contains("struct Drop"));
        assert!(ITERATORS_JL.contains("struct EachSlice"));
        assert!(ITERATORS_JL.contains("function collect"));
    }

    #[test]
    fn test_sort_functions() {
        assert!(SORT_JL.contains("function sort"));
        assert!(SORT_JL.contains("function sort!"));
        assert!(SORT_JL.contains("function sortperm"));
        assert!(SORT_JL.contains("function partialsortperm"));
        assert!(SORT_JL.contains("function partialsortperm!"));
        assert!(SORT_JL.contains("function searchsortedfirst"));
        assert!(SORT_JL.contains("function searchsortedlast"));
        assert!(SORT_JL.contains("function insorted"));
    }

    #[test]
    fn test_strings_functions() {
        // Julia uses isdigit, not is_digit
        assert!(STRINGS_BASIC_JL.contains("function first(s::String)"));
        assert!(STRINGS_BASIC_JL.contains("function last(s::String)"));
        assert!(STRINGS_BASIC_JL.contains("function isdigit"));
        assert!(STRINGS_BASIC_JL.contains("function isletter"));
        assert!(STRINGS_BASIC_JL.contains("function isuppercase"));
        assert!(STRINGS_BASIC_JL.contains("function islowercase"));
        // Search functions in strings/search.jl
        assert!(STRINGS_SEARCH_JL.contains("function occursin"));
        assert!(STRINGS_SEARCH_JL.contains("function startswith"));
        assert!(STRINGS_SEARCH_JL.contains("function endswith"));
        // Util functions in strings/util.jl
        assert!(STRINGS_UTIL_JL.contains("function strip"));
        assert!(STRINGS_UTIL_JL.contains("function join"));
        assert!(STRINGS_UTIL_JL.contains("function ascii"));
        // Unicode functions in strings/unicode.jl
        assert!(STRINGS_UNICODE_JL.contains("function uppercase(c::Char)"));
        assert!(STRINGS_UNICODE_JL.contains("function lowercase(c::Char)"));
        assert!(STRINGS_UNICODE_JL.contains("function uppercase(s::String)"));
        assert!(STRINGS_UNICODE_JL.contains("function lowercase(s::String)"));
    }

    #[test]
    fn test_tuple_functions() {
        // All tuple_* functions were removed - use standard functions (sum, prod, etc.)
        assert!(!TUPLE_JL.contains("function ntuple_indices"));
        assert!(!TUPLE_JL.contains("function tuple_sum"));
        assert!(!TUPLE_JL.contains("function tuple_min"));
        assert!(!TUPLE_JL.contains("function tuple_max"));
        assert!(!TUPLE_JL.contains("function tuple_contains"));
    }

    #[test]
    fn test_set_functions() {
        // Array utility functions (unique, allunique, allequal)
        assert!(SET_JL.contains("function unique"));
        assert!(SET_JL.contains("function allunique"));
        assert!(SET_JL.contains("function allequal"));
        // Set algebra operations — Pure Julia (Phase 4-4, Issue #2575)
        assert!(SET_JL.contains("function union"));
        assert!(SET_JL.contains("function intersect"));
        assert!(SET_JL.contains("function setdiff"));
        assert!(SET_JL.contains("function issubset"));
        assert!(SET_JL.contains("function symdiff"));
    }

    #[test]
    fn test_reflection_functions() {
        // Reflection functions (Pure Julia wrappers over VM builtins)
        assert!(REFLECTION_JL.contains("function fieldnames"));
        assert!(REFLECTION_JL.contains("function fieldtypes"));
        assert!(REFLECTION_JL.contains("function nfields"));
        // These should call the internal VM builtins
        assert!(REFLECTION_JL.contains("_fieldnames"));
        assert!(REFLECTION_JL.contains("_fieldtypes"));
    }

    #[test]
    fn test_error_types() {
        // Exception types (DimensionMismatch, KeyError, StringIndexError)
        assert!(ERROR_JL.contains("abstract type Exception"));
        assert!(ERROR_JL.contains("struct DimensionMismatch <: Exception"));
        assert!(ERROR_JL.contains("struct KeyError <: Exception"));
        assert!(ERROR_JL.contains("struct StringIndexError <: Exception"));
    }

    #[test]
    fn test_errorshow_functions() {
        // showerror function for exception display
        assert!(ERRORSHOW_JL.contains("function showerror(io::IO, ex)"));
        assert!(ERRORSHOW_JL.contains("function showerror(io::IO, ex::ErrorException)"));
        assert!(ERRORSHOW_JL.contains("function showerror(io::IO, ex::DimensionMismatch)"));
        assert!(ERRORSHOW_JL.contains("function showerror(io::IO, ex::KeyError)"));
        assert!(ERRORSHOW_JL.contains("function showerror(io::IO, ex::BoundsError)"));
        assert!(ERRORSHOW_JL.contains("function showerror(io::IO, ex::MethodError)"));
        assert!(ERRORSHOW_JL.contains("function sprint_showerror(ex)"));
    }

    #[test]
    fn test_rounding_types() {
        // RoundingMode type and constants
        assert!(ROUNDING_JL.contains("struct RoundingMode"));
        assert!(ROUNDING_JL.contains("const RoundNearest"));
        assert!(ROUNDING_JL.contains("const RoundToZero"));
        assert!(ROUNDING_JL.contains("const RoundUp"));
        assert!(ROUNDING_JL.contains("const RoundDown"));
        assert!(ROUNDING_JL.contains("const RoundFromZero"));
        assert!(ROUNDING_JL.contains("const RoundNearestTiesAway"));
        assert!(ROUNDING_JL.contains("const RoundNearestTiesUp"));
    }

    #[test]
    fn test_subarray_functions() {
        // SubArray type for array views
        assert!(SUBARRAY_JL.contains("struct SubArray"));
        assert!(SUBARRAY_JL.contains("function view(A::Vector{Float64}, indices::UnitRange)"));
        assert!(SUBARRAY_JL.contains("function getindex(v::SubArray"));
        assert!(SUBARRAY_JL.contains("function setindex!(v::SubArray"));
        assert!(SUBARRAY_JL.contains("function length(v::SubArray"));
        assert!(SUBARRAY_JL.contains("function parent(v::SubArray"));
    }

    #[test]
    fn test_multimedia_display_stack() {
        // Display stack functionality (Issue #376)
        // Note: `const displays` removed due to Issue #1440 (global array access limitation)
        assert!(MULTIMEDIA_JL.contains("abstract type AbstractDisplay"));
        assert!(MULTIMEDIA_JL.contains("struct TextDisplay <: AbstractDisplay"));
        assert!(MULTIMEDIA_JL.contains("function pushdisplay"));
        assert!(MULTIMEDIA_JL.contains("function popdisplay"));
        assert!(MULTIMEDIA_JL.contains("function display(x)"));
        assert!(MULTIMEDIA_JL.contains("function display(d::AbstractDisplay, x)"));
        assert!(MULTIMEDIA_JL.contains("function display(m::MIME, x)"));
    }

    #[test]
    fn test_hashing_functions() {
        // Hash functions (Pure Julia wrappers over _hash intrinsic)
        assert!(HASHING_JL.contains("hash(x) = _hash(x)"));
        assert!(HASHING_JL.contains("hash(x, h)"));
        assert!(HASHING_JL.contains("hash(x::Float64)"));
        assert!(HASHING_JL.contains("hash(x::Bool)"));
    }

    #[test]
    fn test_some_functions() {
        // Some type and something function
        assert!(SOME_JL.contains("struct Some"));
        assert!(SOME_JL.contains("function something"));
    }

    #[test]
    fn test_essentials_functions() {
        // Essential functions
        assert!(ESSENTIALS_JL.contains("function ifelse"));
        assert!(ESSENTIALS_JL.contains("function oftype"));
    }

    #[test]
    fn test_timing_macros() {
        // Timing macros
        assert!(TIMING_JL.contains("macro time"));
        assert!(TIMING_JL.contains("macro elapsed"));
        assert!(TIMING_JL.contains("macro timed"));
        assert!(TIMING_JL.contains("macro timev"));
        assert!(TIMING_JL.contains("macro showtime"));
        assert!(TIMING_JL.contains("macro allocated"));
        assert!(TIMING_JL.contains("macro allocations"));
    }

    #[test]
    fn test_printf_macros() {
        // Printf macros
        assert!(PRINTF_JL.contains("macro sprintf"));
        assert!(PRINTF_JL.contains("macro printf"));
    }

    #[test]
    fn test_gmp_functions() {
        // GMP/BigInt functions (renamed from bigint.jl)
        assert!(GMP_JL.contains("precision"));
        assert!(GMP_JL.contains("setprecision"));
    }

    #[test]
    fn test_lock_types() {
        // Lock types and synchronization primitives
        assert!(LOCK_JL.contains("abstract type AbstractLock"));
        assert!(LOCK_JL.contains("mutable struct ReentrantLock <: AbstractLock"));
        assert!(LOCK_JL.contains("mutable struct SpinLock <: AbstractLock"));
        assert!(LOCK_JL.contains("mutable struct Condition"));
        assert!(LOCK_JL.contains("function lock(lk::ReentrantLock)"));
        assert!(LOCK_JL.contains("function unlock(lk::ReentrantLock)"));
        assert!(LOCK_JL.contains("function trylock(lk::ReentrantLock)"));
        assert!(LOCK_JL.contains("islocked(lk::ReentrantLock)"));
    }

    #[test]
    fn test_task_functions() {
        // Task type and functions
        assert!(TASK_JL.contains("mutable struct Task"));
        assert!(TASK_JL.contains("istaskdone(t::Task)"));
        assert!(TASK_JL.contains("istaskstarted(t::Task)"));
        assert!(TASK_JL.contains("istaskfailed(t::Task)"));
        assert!(TASK_JL.contains("function schedule(t::Task)"));
        assert!(TASK_JL.contains("function fetch(t::Task)"));
        assert!(TASK_JL.contains("function wait(t::Task)"));
        assert!(TASK_JL.contains("function yield()"));
    }

    #[test]
    fn test_channel_types() {
        // Channel type and functions (simplified without type parameters)
        assert!(CHANNELS_JL.contains("mutable struct Channel"));
        assert!(CHANNELS_JL.contains("isopen(c::Channel)"));
        assert!(CHANNELS_JL.contains("isbuffered(c::Channel)"));
        assert!(CHANNELS_JL.contains("isfull(c::Channel)"));
        assert!(CHANNELS_JL.contains("isready(c::Channel)"));
        assert!(CHANNELS_JL.contains("function close(c::Channel)"));
        assert!(CHANNELS_JL.contains("function put!(c::Channel, v)"));
        assert!(CHANNELS_JL.contains("function take!(c::Channel)"));
        assert!(CHANNELS_JL.contains("function fetch(c::Channel)"));
    }

    #[test]
    fn test_genericmemory_functions() {
        // Memory{T} is now a native Rust primitive type (no struct definition)
        // Only Pure Julia functions that build on top of the native type remain
        assert!(GENERICMEMORY_JL.contains("function copy(m::Memory)"));
    }

    /// Test that all .jl files in src/julia/base/ are either loaded or explicitly excluded.
    /// This prevents orphaned Julia source files from going unnoticed (Issue #1765, #1770).
    ///
    /// If this test fails, either:
    /// 1. Add the file to BASE_SOURCES (add include_str! const and include in get_base())
    /// 2. Add the file to EXCLUDED_FILES below with a justification
    #[test]
    fn test_no_orphaned_jl_files() {
        use std::collections::HashSet;
        use std::path::Path;

        // Files that are intentionally NOT loaded into BASE_SOURCES
        // Each exclusion must have a documented reason
        let excluded_files: HashSet<&str> = [
            // exports.jl contains export declarations (metadata), not executable code.
            // The VM handles exports differently than Julia's module system.
            "exports.jl",
        ]
        .into_iter()
        .collect();

        // All files loaded via include_str! in this module
        let loaded_files: HashSet<&str> = [
            "abstractarray.jl",
            "accumulate.jl",
            "array.jl",
            "bool.jl",
            "boot.jl",
            "broadcast.jl",
            "channels.jl",
            "combinatorics.jl",
            "complex.jl",
            "dict.jl",
            "docs/utils.jl",
            "error.jl",
            "errorshow.jl",
            "essentials.jl",
            "float.jl",
            "floatfuncs.jl",
            "genericmemory.jl",
            "generator.jl",
            "gmp.jl",
            "hashing.jl",
            "int.jl",
            "intfuncs.jl",
            "io.jl",
            "irrationals.jl",
            "iterators.jl",
            "lock.jl",
            "macros.jl",
            "math.jl",
            "mathconstants.jl",
            "meta.jl",
            "missing.jl",
            "multimedia.jl",
            "number.jl",
            "operators.jl",
            "pair.jl",
            "parse.jl",
            "path.jl",
            "printf.jl",
            "promotion.jl",
            "range.jl",
            "rational.jl",
            "reduce.jl",
            "reflection.jl",
            "rounding.jl",
            "runtime_internals.jl",
            "set.jl",
            "some.jl",
            "sort.jl",
            "special/trig.jl",
            "special/exp.jl",
            "special/log.jl",
            "strings/basic.jl",
            "strings/search.jl",
            "strings/unicode.jl",
            "strings/util.jl",
            "subarray.jl",
            "task.jl",
            "timing.jl",
            "traits.jl",
            "tuple.jl",
            "util.jl",
            "version.jl",
        ]
        .into_iter()
        .collect();

        // Find all .jl files in src/julia/base/
        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/julia/base");

        fn find_jl_files(dir: &Path, base: &Path, files: &mut Vec<String>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        find_jl_files(&path, base, files);
                    } else if path.extension().is_some_and(|e| e == "jl") {
                        if let Ok(relative) = path.strip_prefix(base) {
                            files.push(relative.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        let mut all_files = Vec::new();
        find_jl_files(&base_dir, &base_dir, &mut all_files);

        // Check for orphaned files
        let mut orphaned = Vec::new();
        for file in &all_files {
            let file_str = file.as_str();
            if !loaded_files.contains(file_str) && !excluded_files.contains(file_str) {
                orphaned.push(file_str);
            }
        }

        if !orphaned.is_empty() {
            panic!(
                "Found orphaned Julia source files in src/julia/base/:\n  {}\n\n\
                To fix this, either:\n\
                1. Add the file to BASE_SOURCES (include_str! const + get_base())\n\
                2. Add the file to excluded_files in this test with a justification\n\n\
                See Issue #1765 and #1770 for context.",
                orphaned.join("\n  ")
            );
        }

        // Also verify that loaded_files list matches actual include_str! calls
        // This catches typos in the loaded_files list
        assert_eq!(
            loaded_files.len(),
            62,
            "loaded_files count mismatch - update test when adding new files"
        );
    }
}
