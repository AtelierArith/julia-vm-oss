//! Transfer functions for type inference.
//!
//! This module provides the infrastructure for inferring the return types of
//! function calls during abstract interpretation. Transfer functions (tfuncs)
//! encode the type-level semantics of Julia operations.
//!
//! # Architecture
//!
//! The tfuncs system consists of:
//! - `registry`: Central registry mapping function names to transfer functions
//! - `arithmetic`: Transfer functions for arithmetic and comparison operations
//! - `array_ops`: Transfer functions for array operations
//! - `string_ops`: Transfer functions for string operations
//! - `intrinsics`: Transfer functions for intrinsic operations and conversions
//! - `field_ops`: Transfer functions for field access operations (getfield, setfield!, etc.)
//! - `iterator_ops`: Transfer functions for iterator operations (iterate, length, eachindex, etc.)
//! - `collection_ops`: Transfer functions for collection operations (keys, values, pairs, etc.)
//! - `math_intrinsics`: Transfer functions for mathematical intrinsics (sign, div, rem, mod, etc.)
//!
//! # Usage
//!
//! ```
//! use subset_julia_vm::compile::tfuncs::{TransferFunctions, register_all};
//! use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
//!
//! // Create and populate the registry
//! let mut registry = TransferFunctions::new();
//! register_all(&mut registry);
//!
//! // Use the registry to infer types
//! let args = vec![
//!     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))),
//!     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))),
//! ];
//! let result = registry.infer_return_type("+", &args);
//! ```

pub mod arithmetic;
pub mod array_ops;
pub mod collection_ops;
pub mod complex_ops;
pub mod field_ops;
pub mod hof_ops;
pub mod intrinsics;
pub mod iterator_ops;
pub mod linear_algebra_ops;
pub mod math_intrinsics;
pub mod registry;
pub mod string_ops;

pub use registry::{
    ContextualTransferFn, HofLambdaAnalyzer, StructIdLookup, TFuncContext, TransferFn,
    TransferFunctions, TransferRule, COST_CHEAP, COST_EXPENSIVE, COST_MEDIUM,
};

/// Registers all standard transfer functions.
///
/// This convenience function registers transfer functions for all supported
/// operations: arithmetic, array operations, string operations, intrinsics,
/// field operations, iterator operations, collection operations, and mathematical intrinsics.
///
/// # Example
/// ```
/// use subset_julia_vm::compile::tfuncs::{TransferFunctions, register_all};
///
/// let mut registry = TransferFunctions::new();
/// register_all(&mut registry);
/// ```
pub fn register_all(registry: &mut TransferFunctions) {
    register_arithmetic(registry);
    register_array_ops(registry);
    register_string_ops(registry);
    register_intrinsics(registry);
    register_field_ops(registry);
    register_iterator_ops(registry);
    register_collection_ops(registry);
    register_math_intrinsics(registry);
    register_complex_ops(registry);
    register_linear_algebra(registry);
}

/// Registers arithmetic and comparison transfer functions.
///
/// Migrated to metadata-bearing rules (Issue #3509):
/// - Binary operators (`+`, `*`, `/`, `==`, `<`, `<=`, `>`, `>=`) take exactly
///   two arguments and are cheap.
/// - `-` is special: Julia uses it as both unary negation and binary
///   subtraction, so its rule accepts arity 1..=2.
/// - `!` is unary boolean negation.
pub fn register_arithmetic(registry: &mut TransferFunctions) {
    registry.register_exact("+", 2, COST_CHEAP, arithmetic::tfunc_add);
    // `-` covers both unary negation and binary subtraction (see
    // `unary_op_to_function`/`binary_op_to_function` in the engine).
    registry.register_ranged("-", 1, Some(2), COST_CHEAP, arithmetic::tfunc_sub);
    registry.register_exact("*", 2, COST_CHEAP, arithmetic::tfunc_mul);
    registry.register_exact("/", 2, COST_CHEAP, arithmetic::tfunc_div);
    registry.register_exact("^", 2, COST_CHEAP, arithmetic::tfunc_pow);
    registry.register_exact("==", 2, COST_CHEAP, arithmetic::tfunc_eq);
    registry.register_exact("<", 2, COST_CHEAP, arithmetic::tfunc_lt);
    registry.register_exact("<=", 2, COST_CHEAP, arithmetic::tfunc_le);
    registry.register_exact(">", 2, COST_CHEAP, arithmetic::tfunc_gt);
    registry.register_exact(">=", 2, COST_CHEAP, arithmetic::tfunc_ge);
    registry.register_exact("!", 1, COST_CHEAP, arithmetic::tfunc_not);

    // BigInt-aware integer helpers (Issues #5922, #2383).
    registry.register_ranged("gcd", 0, None, COST_MEDIUM, arithmetic::tfunc_gcd);
    registry.register_ranged("lcm", 0, None, COST_MEDIUM, arithmetic::tfunc_gcd);
    registry.register_exact("fld", 2, COST_CHEAP, arithmetic::tfunc_fld);
    registry.register_exact("cld", 2, COST_CHEAP, arithmetic::tfunc_fld);
}

/// Registers array operation transfer functions.
pub fn register_array_ops(registry: &mut TransferFunctions) {
    // Basic array operations
    registry.register_ranged("getindex", 2, None, COST_CHEAP, array_ops::tfunc_getindex);
    registry.register_ranged("setindex!", 3, None, COST_CHEAP, array_ops::tfunc_setindex);
    registry.register_exact("length", 1, COST_CHEAP, array_ops::tfunc_length);
    registry.register_exact("first", 1, COST_CHEAP, array_ops::tfunc_first);
    registry.register_exact("last", 1, COST_CHEAP, array_ops::tfunc_last);
    registry.register_ranged("size", 1, None, COST_CHEAP, array_ops::tfunc_size);

    // Array mutation operations
    registry.register_ranged("push!", 2, None, COST_CHEAP, array_ops::tfunc_push);
    registry.register_exact("pop!", 1, COST_CHEAP, array_ops::tfunc_pop);
    registry.register_ranged("append!", 2, None, COST_CHEAP, array_ops::tfunc_append);
    registry.register_ranged("prepend!", 2, None, COST_CHEAP, array_ops::tfunc_prepend);
    registry.register_exact("insert!", 3, COST_CHEAP, array_ops::tfunc_insert);
    registry.register_exact("deleteat!", 2, COST_CHEAP, array_ops::tfunc_deleteat);
    registry.register_exact("popfirst!", 1, COST_CHEAP, array_ops::tfunc_popfirst);
    registry.register_ranged(
        "pushfirst!",
        2,
        None,
        COST_CHEAP,
        array_ops::tfunc_pushfirst,
    );
    registry.register_exact("empty!", 1, COST_CHEAP, array_ops::tfunc_empty_bang);
    registry.register_exact("resize!", 2, COST_CHEAP, array_ops::tfunc_resize);
    registry.register_ranged("splice!", 2, Some(3), COST_CHEAP, array_ops::tfunc_splice);
    registry.register_exact("fill!", 2, COST_CHEAP, array_ops::tfunc_fill_bang);

    // Sorting and ordering
    registry.register_ranged("sort", 1, None, COST_EXPENSIVE, array_ops::tfunc_sort);
    registry.register_ranged("sort!", 1, None, COST_EXPENSIVE, array_ops::tfunc_sort_bang);
    registry.register_exact("reverse", 1, COST_EXPENSIVE, array_ops::tfunc_reverse);
    registry.register_exact("reverse!", 1, COST_EXPENSIVE, array_ops::tfunc_reverse_bang);
    registry.register_ranged("unique", 1, None, COST_EXPENSIVE, array_ops::tfunc_unique);
    registry.register_ranged(
        "unique!",
        1,
        None,
        COST_EXPENSIVE,
        array_ops::tfunc_unique_bang,
    );

    // Array creation
    registry.register_ranged("fill", 2, None, COST_EXPENSIVE, array_ops::tfunc_fill);
    registry.register_ranged("zeros", 1, None, COST_EXPENSIVE, array_ops::tfunc_zeros);
    registry.register_ranged("ones", 1, None, COST_EXPENSIVE, array_ops::tfunc_ones);
    registry.register_ranged("similar", 1, None, COST_EXPENSIVE, array_ops::tfunc_similar);
    registry.register_exact("copy", 1, COST_MEDIUM, array_ops::tfunc_copy);
    registry.register_exact("deepcopy", 1, COST_MEDIUM, array_ops::tfunc_deepcopy);

    // Range construction
    registry.register_ranged(":", 2, Some(3), COST_CHEAP, array_ops::tfunc_colon);
    registry.register_ranged("colon", 2, Some(3), COST_CHEAP, array_ops::tfunc_colon);
    registry.register_ranged("range", 1, None, COST_CHEAP, array_ops::tfunc_range);

    // Higher-order functions
    registry.register_ranged("map", 2, None, COST_EXPENSIVE, array_ops::tfunc_map);
    registry.register_ranged("filter", 2, None, COST_EXPENSIVE, array_ops::tfunc_filter);

    // Reduction operations
    registry.register_ranged("reduce", 2, None, COST_EXPENSIVE, array_ops::tfunc_reduce);
    registry.register_ranged("foldl", 2, None, COST_EXPENSIVE, array_ops::tfunc_foldl);
    registry.register_ranged("foldr", 2, None, COST_EXPENSIVE, array_ops::tfunc_foldr);
    registry.register_ranged("sum", 1, None, COST_EXPENSIVE, array_ops::tfunc_sum);
    registry.register_ranged("prod", 1, None, COST_EXPENSIVE, array_ops::tfunc_prod);
    registry.register_ranged("maximum", 1, None, COST_EXPENSIVE, array_ops::tfunc_maximum);
    registry.register_ranged("minimum", 1, None, COST_EXPENSIVE, array_ops::tfunc_minimum);
    registry.register_ranged("any", 1, Some(2), COST_EXPENSIVE, array_ops::tfunc_any);
    registry.register_ranged("all", 1, Some(2), COST_EXPENSIVE, array_ops::tfunc_all);
    registry.register_exact("collect", 1, COST_EXPENSIVE, array_ops::tfunc_collect);

    // BitArray-family constructors (Issue #5922).
    registry.register_ranged("trues", 0, None, COST_EXPENSIVE, array_ops::tfunc_trues);
    registry.register_ranged("falses", 0, None, COST_EXPENSIVE, array_ops::tfunc_trues);
}

/// Registers string operation transfer functions.
pub fn register_string_ops(registry: &mut TransferFunctions) {
    registry.register_ranged("string", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_exact("uppercase", 1, COST_CHEAP, string_ops::tfunc_uppercase);
    registry.register_exact("lowercase", 1, COST_CHEAP, string_ops::tfunc_lowercase);
    registry.register_ranged("replace", 2, None, COST_MEDIUM, string_ops::tfunc_replace);
    registry.register_ranged("repeat", 1, None, COST_MEDIUM, string_ops::tfunc_repeat);
    registry.register_ranged("split", 1, None, COST_MEDIUM, string_ops::tfunc_split);
    registry.register_ranged("join", 1, Some(2), COST_MEDIUM, string_ops::tfunc_join);
    registry.register_exact("startswith", 2, COST_CHEAP, string_ops::tfunc_startswith);
    registry.register_exact("endswith", 2, COST_CHEAP, string_ops::tfunc_endswith);
    registry.register_exact("contains", 2, COST_CHEAP, string_ops::tfunc_contains);
    registry.register_exact("occursin", 2, COST_CHEAP, string_ops::tfunc_contains);
    registry.register_ranged("repr", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("strip", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("lstrip", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("rstrip", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("chomp", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("chop", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged(
        "takestring!",
        0,
        None,
        COST_MEDIUM,
        string_ops::tfunc_string,
    );
    registry.register_ranged("sprint", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("sprintf", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged(
        "lowercasefirst",
        0,
        None,
        COST_MEDIUM,
        string_ops::tfunc_string,
    );
    registry.register_ranged(
        "uppercasefirst",
        0,
        None,
        COST_MEDIUM,
        string_ops::tfunc_string,
    );
    registry.register_ranged(
        "escape_string",
        0,
        None,
        COST_MEDIUM,
        string_ops::tfunc_string,
    );
    registry.register_ranged("chopprefix", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("chopsuffix", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("lpad", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("rpad", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("bitstring", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged("ascii", 0, None, COST_MEDIUM, string_ops::tfunc_string);
    registry.register_ranged(
        "unescape_string",
        0,
        None,
        COST_MEDIUM,
        string_ops::tfunc_string,
    );
}

/// Registers intrinsic and conversion transfer functions.
///
/// All production registrations use metadata-bearing rules (Issue #4275),
/// mirroring Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)` shape.
pub fn register_intrinsics(registry: &mut TransferFunctions) {
    // Migrated entries (arity-aware, cost-aware).
    registry.register_exact("isa", 2, COST_CHEAP, intrinsics::tfunc_isa);
    registry.register_exact("typeof", 1, COST_CHEAP, intrinsics::tfunc_typeof);
    registry.register_exact("isless", 2, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("isnan", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("isinf", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("isfinite", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("isinteger", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("iseven", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("isodd", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("isnothing", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    registry.register_exact("ismissing", 1, COST_CHEAP, intrinsics::tfunc_bool_predicate);
    // 2-arg isequal returns Bool; the curried 1-arg form is a function, so it
    // is arity-gated in the expression adapter (Issues #5922, #5662).
    registry.register_exact("isequal", 2, COST_CHEAP, intrinsics::tfunc_bool_predicate);

    // Never-returning raisers: `throw(x)` has return type `Union{}` (Bottom),
    // mirroring upstream `add_tfunc(throw, 1, 1, ->Bottom, 0)` in
    // `julia/Compiler/src/tfuncs.jl`. `rethrow()` / `rethrow(e)` is declared
    // `Bottom`-returning upstream (`rethrow() = ccall(:jl_rethrow, Bottom, ())`)
    // and is a builtin here, so it gets the same rule (Issue #6532).
    registry.register_exact("throw", 1, COST_CHEAP, intrinsics::tfunc_throw);
    registry.register_ranged("rethrow", 0, Some(1), COST_CHEAP, intrinsics::tfunc_throw);
    // `error(...)` is pure Julia (`base/error.jl`) and every method's body is a
    // `throw`, so a fresh full compile infers `Union{}` transitively. The
    // cached-Base compile path cannot: a multi-method Base callee is excluded
    // from the engine's `function_table` (differing signatures → ambiguous) and
    // cached Base methods are not registered into the engine's method tables,
    // so the call falls through to this registry. Upstream infers `error`'s
    // `Union{}` from its body; this registration encodes the same fact for the
    // table-less cached path (Issue #6532; cached-path gap tracked in #6538).
    registry.register_ranged("error", 0, None, COST_CHEAP, intrinsics::tfunc_throw);

    // Type-value primitives.
    registry.register_exact("zero", 1, COST_CHEAP, intrinsics::tfunc_zero);
    registry.register_exact("one", 1, COST_CHEAP, intrinsics::tfunc_one);
    registry.register_exact("typemin", 1, COST_CHEAP, intrinsics::tfunc_typemin);
    registry.register_exact("typemax", 1, COST_CHEAP, intrinsics::tfunc_typemax);

    // Unary math functions.
    registry.register_exact("sqrt", 1, COST_MEDIUM, intrinsics::tfunc_sqrt);
    registry.register_exact("abs", 1, COST_CHEAP, intrinsics::tfunc_abs);
    // Julia Compiler registers the underlying float intrinsic separately; the
    // Pure Julia `abs(::Float64)` method delegates through this wrapper.
    registry.register_exact("abs_float", 1, 2, intrinsics::tfunc_abs);
    registry.register_exact("Core.Intrinsics.abs_float", 1, 2, intrinsics::tfunc_abs);
    registry.register_exact("sin", 1, COST_MEDIUM, intrinsics::tfunc_sin);
    registry.register_exact("cos", 1, COST_MEDIUM, intrinsics::tfunc_cos);
    registry.register_exact("exp", 1, COST_MEDIUM, intrinsics::tfunc_exp);
    registry.register_exact("log", 1, COST_MEDIUM, intrinsics::tfunc_log);
    registry.register_exact("tan", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("asin", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("acos", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("atan", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("sinh", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("cosh", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("tanh", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("asinh", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("acosh", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("atanh", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("log2", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("log10", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("log1p", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);
    registry.register_exact("expm1", 1, COST_MEDIUM, intrinsics::tfunc_unary_float64);

    // min/max take 2 args today; widen if/when we add varargs handling.
    registry.register_exact("min", 2, COST_CHEAP, intrinsics::tfunc_min);
    registry.register_exact("max", 2, COST_CHEAP, intrinsics::tfunc_max);

    // Statistics-style reductions.
    registry.register_ranged(
        "mean",
        1,
        None,
        COST_EXPENSIVE,
        intrinsics::tfunc_float64_result,
    );
    registry.register_ranged(
        "std",
        1,
        None,
        COST_EXPENSIVE,
        intrinsics::tfunc_float64_result,
    );
    registry.register_ranged(
        "var",
        1,
        None,
        COST_EXPENSIVE,
        intrinsics::tfunc_float64_result,
    );

    // Int64-result collection queries/reductions.
    registry.register_exact("ndims", 1, COST_CHEAP, intrinsics::tfunc_int64_result);
    registry.register_ranged(
        "count",
        1,
        None,
        COST_EXPENSIVE,
        intrinsics::tfunc_int64_result,
    );

    // I/O functions accept any number of args (Julia's `print(args...)`).
    registry.register_ranged("println", 0, None, COST_MEDIUM, intrinsics::tfunc_println);
    registry.register_ranged("print", 0, None, COST_MEDIUM, intrinsics::tfunc_println);

    // Method-dispatched conversion generic with a Rust fallback.
    registry.register_exact("convert", 2, COST_CHEAP, intrinsics::tfunc_convert);

    // Arbitrary-precision widening (Issues #5922, #1910, #2383).
    registry.register_ranged("big", 0, None, COST_CHEAP, intrinsics::tfunc_big);

    // In-memory IO stream constructor.
    registry.register_ranged("IOBuffer", 0, None, COST_MEDIUM, intrinsics::tfunc_iobuffer);

    // Type-returning helpers (typeof/eltype/keytype/valtype are registered
    // separately with precise rules).
    registry.register_ranged(
        "promote_type",
        1,
        None,
        COST_CHEAP,
        intrinsics::tfunc_promote_type,
    );
    registry.register_exact(
        "promote_rule",
        2,
        COST_CHEAP,
        intrinsics::tfunc_datatype_result,
    );

    // Int64-result helpers: hash plus date/time accessors (Issue #5922).
    registry.register_ranged(
        "hash",
        1,
        Some(2),
        COST_MEDIUM,
        intrinsics::tfunc_int64_result,
    );
    for accessor in [
        "year",
        "month",
        "day",
        "hour",
        "minute",
        "second",
        "dayofweek",
        "dayofyear",
        "week",
        "days",
    ] {
        registry.register_exact(accessor, 1, COST_CHEAP, intrinsics::tfunc_int64_result);
    }

    // Vararg promotion.
    registry.register_ranged("promote", 1, None, COST_MEDIUM, intrinsics::tfunc_promote);

    // Integer type conversions
    registry.register_exact("Int8", 1, COST_CHEAP, intrinsics::tfunc_to_int8);
    registry.register_exact("Int16", 1, COST_CHEAP, intrinsics::tfunc_to_int16);
    registry.register_exact("Int32", 1, COST_CHEAP, intrinsics::tfunc_to_int32);
    registry.register_exact("Int64", 1, COST_CHEAP, intrinsics::tfunc_to_int64);
    registry.register_exact("Int128", 1, COST_CHEAP, intrinsics::tfunc_to_int128);
    registry.register_exact("UInt8", 1, COST_CHEAP, intrinsics::tfunc_to_uint8);
    registry.register_exact("UInt16", 1, COST_CHEAP, intrinsics::tfunc_to_uint16);
    registry.register_exact("UInt32", 1, COST_CHEAP, intrinsics::tfunc_to_uint32);
    registry.register_exact("UInt64", 1, COST_CHEAP, intrinsics::tfunc_to_uint64);
    registry.register_exact("UInt128", 1, COST_CHEAP, intrinsics::tfunc_to_uint128);

    // Float type conversions
    registry.register_exact("Float16", 1, COST_CHEAP, intrinsics::tfunc_to_float16);
    registry.register_exact("Float32", 1, COST_CHEAP, intrinsics::tfunc_to_float32);
    registry.register_exact("Float64", 1, COST_CHEAP, intrinsics::tfunc_to_float64);
    registry.register_exact("BigInt", 1, COST_CHEAP, intrinsics::tfunc_to_bigint);
    registry.register_exact("BigFloat", 1, COST_CHEAP, intrinsics::tfunc_to_bigfloat);

    // Other type conversions
    registry.register_exact("Bool", 1, COST_CHEAP, intrinsics::tfunc_to_bool);
    registry.register_exact("String", 1, COST_CHEAP, intrinsics::tfunc_to_string);
    registry.register_exact("Char", 1, COST_CHEAP, intrinsics::tfunc_to_char);
}

/// Registers field access transfer functions.
pub fn register_field_ops(registry: &mut TransferFunctions) {
    registry.register_ranged(
        "getfield",
        2,
        Some(3),
        COST_CHEAP,
        field_ops::tfunc_getfield,
    );
    registry.register_exact("setfield!", 3, COST_CHEAP, field_ops::tfunc_setfield);
    registry.register_exact("fieldnames", 1, COST_CHEAP, field_ops::tfunc_fieldnames);
    registry.register_exact("fieldtypes", 1, COST_CHEAP, field_ops::tfunc_fieldtypes);

    // Register contextual transfer function for getfield (with struct table access)
    registry.register_contextual("getfield", field_ops::tfunc_getfield_contextual);
}

/// Registers iterator operation transfer functions.
pub fn register_iterator_ops(registry: &mut TransferFunctions) {
    registry.register_ranged(
        "iterate",
        1,
        Some(2),
        COST_CHEAP,
        iterator_ops::tfunc_iterate,
    );
    // Note: length is already registered in array_ops, but we provide an alias here
    // registry.register("length", iterator_ops::tfunc_length_iter);
    registry.register_ranged(
        "eachindex",
        1,
        None,
        COST_CHEAP,
        iterator_ops::tfunc_eachindex,
    );
    registry.register_exact("enumerate", 1, COST_CHEAP, iterator_ops::tfunc_enumerate);
    registry.register_ranged("zip", 2, None, COST_CHEAP, iterator_ops::tfunc_zip);
}

/// Registers collection operation transfer functions.
pub fn register_collection_ops(registry: &mut TransferFunctions) {
    // Dictionary access
    registry.register_exact("keys", 1, COST_CHEAP, collection_ops::tfunc_keys);
    registry.register_exact("values", 1, COST_CHEAP, collection_ops::tfunc_values);
    registry.register_exact("pairs", 1, COST_CHEAP, collection_ops::tfunc_pairs);
    registry.register_exact("haskey", 2, COST_CHEAP, collection_ops::tfunc_haskey);
    registry.register_exact("get", 3, COST_CHEAP, collection_ops::tfunc_get);
    registry.register_exact("get!", 3, COST_CHEAP, collection_ops::tfunc_get_bang);

    // Dictionary mutation
    registry.register_exact("delete!", 2, COST_CHEAP, collection_ops::tfunc_delete);
    registry.register_ranged("merge", 1, None, COST_MEDIUM, collection_ops::tfunc_merge);
    registry.register_ranged(
        "merge!",
        1,
        None,
        COST_MEDIUM,
        collection_ops::tfunc_merge_bang,
    );

    // Collection queries
    registry.register_exact("isempty", 1, COST_CHEAP, collection_ops::tfunc_isempty);
    registry.register_exact("in", 2, COST_CHEAP, collection_ops::tfunc_in);
    registry.register_exact("∈", 2, COST_CHEAP, collection_ops::tfunc_in);
    registry.register_exact("eltype", 1, COST_CHEAP, collection_ops::tfunc_eltype);
    registry.register_exact("keytype", 1, COST_CHEAP, collection_ops::tfunc_keytype);
    registry.register_exact("valtype", 1, COST_CHEAP, collection_ops::tfunc_valtype);

    // Constructors
    registry.register_ranged("Set", 0, Some(1), COST_EXPENSIVE, collection_ops::tfunc_set);
    registry.register_ranged("Dict", 0, None, COST_EXPENSIVE, collection_ops::tfunc_dict);

    // Set operations
    registry.register_ranged("union", 1, None, COST_MEDIUM, collection_ops::tfunc_union);
    registry.register_ranged(
        "intersect",
        1,
        None,
        COST_MEDIUM,
        collection_ops::tfunc_intersect,
    );
    registry.register_ranged(
        "setdiff",
        1,
        None,
        COST_MEDIUM,
        collection_ops::tfunc_setdiff,
    );
    registry.register_ranged(
        "symdiff",
        1,
        None,
        COST_MEDIUM,
        collection_ops::tfunc_symdiff,
    );
    registry.register_exact("issubset", 2, COST_CHEAP, collection_ops::tfunc_issubset);
    registry.register_exact("⊆", 2, COST_CHEAP, collection_ops::tfunc_issubset);
}

/// Registers mathematical intrinsic transfer functions.
pub fn register_math_intrinsics(registry: &mut TransferFunctions) {
    registry.register_exact("sign", 1, COST_CHEAP, math_intrinsics::tfunc_sign);
    registry.register_exact("signbit", 1, COST_CHEAP, math_intrinsics::tfunc_signbit);
    registry.register_exact("clamp", 3, COST_CHEAP, math_intrinsics::tfunc_clamp);
    registry.register_exact("copysign", 2, COST_CHEAP, math_intrinsics::tfunc_copysign);
    registry.register_exact("binomial", 2, COST_CHEAP, math_intrinsics::tfunc_binomial);
    registry.register_exact("ndigits", 1, COST_CHEAP, math_intrinsics::tfunc_ndigits);
    registry.register_exact("widen", 1, COST_CHEAP, math_intrinsics::tfunc_widen);
    registry.register_exact("div", 2, COST_CHEAP, math_intrinsics::tfunc_div);
    registry.register_exact("rem", 2, COST_CHEAP, math_intrinsics::tfunc_rem);
    registry.register_exact("mod", 2, COST_CHEAP, math_intrinsics::tfunc_mod);
    registry.register_exact("floor", 1, COST_CHEAP, math_intrinsics::tfunc_floor);
    registry.register_exact("ceil", 1, COST_CHEAP, math_intrinsics::tfunc_ceil);
    registry.register_ranged("round", 1, None, COST_CHEAP, math_intrinsics::tfunc_round);
    registry.register_ranged("trunc", 1, None, COST_CHEAP, math_intrinsics::tfunc_trunc);
    registry.register_exact("<<", 2, COST_CHEAP, math_intrinsics::tfunc_lshift);
    registry.register_exact(">>", 2, COST_CHEAP, math_intrinsics::tfunc_rshift);
    registry.register_exact("&", 2, COST_CHEAP, math_intrinsics::tfunc_bitand);
    registry.register_exact("|", 2, COST_CHEAP, math_intrinsics::tfunc_bitor);
    registry.register_exact("xor", 2, COST_CHEAP, math_intrinsics::tfunc_xor);
    // rand()/randn() with no arguments sample a Float64; dimension/collection
    // forms stay `Top` here and the expression adapter pins the legacy
    // unparameterized `Array` fallback (Issue #5922).
    registry.register_ranged("rand", 0, None, COST_MEDIUM, math_intrinsics::tfunc_rand);
    registry.register_ranged("randn", 0, None, COST_MEDIUM, math_intrinsics::tfunc_rand);
}

/// Registers `LinearAlgebra` module-call result-shape rules (Issue #5922).
///
/// Keys are `LinearAlgebra.`-qualified on purpose: the rules cover stdlib
/// module calls without affecting bare-name calls (`det(A)`,
/// `transpose(A)`, ...), which keep their builtin / method-dispatch routing.
pub fn register_linear_algebra(registry: &mut TransferFunctions) {
    for name in ["LinearAlgebra.det", "LinearAlgebra.cond"] {
        registry.register_ranged(
            name,
            0,
            None,
            COST_EXPENSIVE,
            linear_algebra_ops::tfunc_la_float64,
        );
    }
    registry.register_ranged(
        "LinearAlgebra.rank",
        0,
        None,
        COST_EXPENSIVE,
        linear_algebra_ops::tfunc_la_int64,
    );
    for name in [
        "LinearAlgebra.svd",
        "LinearAlgebra.qr",
        "LinearAlgebra.eigen",
        "LinearAlgebra.cholesky",
    ] {
        registry.register_ranged(
            name,
            0,
            None,
            COST_EXPENSIVE,
            linear_algebra_ops::tfunc_la_named_tuple,
        );
    }
    registry.register_ranged(
        "LinearAlgebra.lu",
        0,
        None,
        COST_EXPENSIVE,
        linear_algebra_ops::tfunc_la_tuple,
    );
    for name in [
        "LinearAlgebra.inv",
        "LinearAlgebra.eigvals",
        "LinearAlgebra.transpose",
    ] {
        registry.register_ranged(
            name,
            0,
            None,
            COST_EXPENSIVE,
            linear_algebra_ops::tfunc_la_array,
        );
    }
}

/// Registers complex number operation transfer functions.
///
/// Includes accessor functions for complex numbers:
/// - `real`: extract real part (Complex{T} → T)
/// - `imag`: extract imaginary part (Complex{T} → T)
/// - `conj`: complex conjugate (Complex{T} → Complex{T})
/// - `abs2`: squared magnitude (Complex{T} → T)
/// - `angle`: phase/argument (Complex{T} → Float64)
/// - `reim`: decompose into tuple (Complex{T} → Tuple{T, T})
pub fn register_complex_ops(registry: &mut TransferFunctions) {
    registry.register_exact("real", 1, COST_CHEAP, complex_ops::tfunc_real);
    registry.register_exact("imag", 1, COST_CHEAP, complex_ops::tfunc_imag);
    registry.register_exact("conj", 1, COST_CHEAP, complex_ops::tfunc_conj);
    registry.register_exact("abs2", 1, COST_CHEAP, complex_ops::tfunc_abs2);
    registry.register_exact("angle", 1, COST_CHEAP, complex_ops::tfunc_angle);
    registry.register_exact("reim", 1, COST_CHEAP, complex_ops::tfunc_reim);

    // Lowercase `complex` constructor resolves a Complex struct instantiation
    // through the struct-identity lookup in the context (Issue #5922).
    registry.register_contextual("complex", complex_ops::tfunc_complex_contextual);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConcreteType, LatticeType};
    use crate::inference_core::{CorePrimitive, CoreType};

    #[test]
    fn test_register_all() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // Should have many functions registered
        assert!(registry.len() > 20);

        // Check some key functions are present
        assert!(registry.has_function("+"));
        assert!(registry.has_function("getindex"));
        assert!(registry.has_function("length"));
        assert!(registry.has_function("string"));
        assert!(registry.has_function("sqrt"));
        assert!(registry.has_function("isa"));
    }

    #[test]
    fn register_all_has_no_legacy_metadata_free_rules() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let legacy = registry.legacy_rule_names();
        assert!(
            legacy.is_empty(),
            "production tfuncs must use explicit arity/cost metadata: {legacy:?}"
        );
    }

    #[test]
    fn intrinsic_float_wrappers_have_julia_compiler_arity_metadata() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        assert_eq!(registry.arity_bounds("abs_float"), Some((1, Some(1))));
        assert_eq!(
            registry.arity_bounds("Core.Intrinsics.abs_float"),
            Some((1, Some(1)))
        );
        assert_eq!(registry.cost("abs_float"), Some(2));
    }

    #[test]
    fn raisers_registered_as_bottom_issue_6532() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // `throw` mirrors upstream `add_tfunc(throw, 1, 1, ->Bottom, 0)`.
        assert_eq!(registry.arity_bounds("throw"), Some((1, Some(1))));
        assert_eq!(
            registry.infer_return_type(
                "throw",
                &[LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Primitive(CorePrimitive::String)
                ))]
            ),
            LatticeType::Bottom
        );
        // `rethrow()` / `rethrow(e)`.
        assert_eq!(registry.arity_bounds("rethrow"), Some((0, Some(1))));
        assert_eq!(
            registry.infer_return_type("rethrow", &[]),
            LatticeType::Bottom
        );
        // `error(...)` (any arity): every Base `error` method throws; the
        // cached-Base path resolves it only through this registry (Issue #6538).
        assert_eq!(registry.arity_bounds("error"), Some((0, None)));
        assert_eq!(
            registry.infer_return_type(
                "error",
                &[LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Primitive(CorePrimitive::String)
                ))]
            ),
            LatticeType::Bottom
        );
    }

    #[test]
    fn test_arithmetic_registration() {
        let mut registry = TransferFunctions::new();
        register_arithmetic(&mut registry);

        assert!(registry.has_function("+"));
        assert!(registry.has_function("-"));
        assert!(registry.has_function("*"));
        assert!(registry.has_function("/"));
        assert!(registry.has_function("=="));
        assert!(registry.has_function("<"));
    }

    #[test]
    fn test_array_ops_registration() {
        let mut registry = TransferFunctions::new();
        register_array_ops(&mut registry);

        assert!(registry.has_function("getindex"));
        assert!(registry.has_function("length"));
        assert!(registry.has_function("push!"));
    }

    #[test]
    fn test_string_ops_registration() {
        let mut registry = TransferFunctions::new();
        register_string_ops(&mut registry);

        assert!(registry.has_function("string"));
        assert!(registry.has_function("uppercase"));
        assert!(registry.has_function("replace"));
        assert!(registry.has_function("repeat"));
        assert!(registry.has_function("split"));
        assert!(registry.has_function("occursin"));
    }

    #[test]
    fn test_intrinsics_registration() {
        let mut registry = TransferFunctions::new();
        register_intrinsics(&mut registry);

        assert!(registry.has_function("isa"));
        assert!(registry.has_function("sqrt"));
        assert!(registry.has_function("Int64"));
        assert!(registry.has_function("Float16"));
        assert!(registry.has_function("BigInt"));
        assert!(registry.has_function("BigFloat"));
        assert!(registry.has_function("println"));
    }

    #[test]
    fn test_end_to_end_add() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = registry.infer_return_type("+", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_end_to_end_getindex() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                ))),
                ndims: None,
            }),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = registry.infer_return_type("getindex", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_end_to_end_length() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        })];
        let result = registry.infer_return_type("length", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_end_to_end_string() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        ];
        let result = registry.infer_return_type("string", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
    }

    #[test]
    fn test_end_to_end_sqrt() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = registry.infer_return_type("sqrt", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_end_to_end_isa() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Top,
        ];
        let result = registry.infer_return_type("isa", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_unknown_function() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = registry.infer_return_type("unknown_function", &args);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_complex_ops_registration() {
        let mut registry = TransferFunctions::new();
        register_complex_ops(&mut registry);

        assert!(registry.has_function("real"));
        assert!(registry.has_function("imag"));
        assert!(registry.has_function("conj"));
        assert!(registry.has_function("abs2"));
        assert!(registry.has_function("angle"));
        assert!(registry.has_function("reim"));
    }

    #[test]
    fn test_end_to_end_real_complex() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // Test real(Complex{Float64}) → Float64
        let args = vec![LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        })];
        let result = registry.infer_return_type("real", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_end_to_end_imag_complex() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // Test imag(Complex{Int64}) → Int64
        let args = vec![LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Int64}".to_string(),
            type_id: 0,
        })];
        let result = registry.infer_return_type("imag", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    // -----------------------------------------------------------------
    // Issue #3509: metadata-bearing rule migration
    // -----------------------------------------------------------------

    #[test]
    fn test_migrated_arithmetic_carries_metadata() {
        let mut registry = TransferFunctions::new();
        register_arithmetic(&mut registry);

        // Migrated rules expose explicit arity + cost.
        assert_eq!(registry.arity_bounds("+"), Some((2, Some(2))));
        assert_eq!(registry.arity_bounds("-"), Some((1, Some(2))));
        assert_eq!(registry.arity_bounds("!"), Some((1, Some(1))));
        assert_eq!(registry.arity_bounds("=="), Some((2, Some(2))));

        assert_eq!(registry.cost("+"), Some(COST_CHEAP));
        assert_eq!(registry.cost("-"), Some(COST_CHEAP));
        assert_eq!(registry.cost("!"), Some(COST_CHEAP));
    }

    #[test]
    fn test_arithmetic_dispatch_unchanged_for_correct_arity() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // Binary `+` Int64 + Int64 → Int64
        let result = registry.infer_return_type(
            "+",
            &[
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            ],
        );
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );

        // Unary `-` Int64 → Int64 (uses the (1, Some(2)) range)
        let result = registry.infer_return_type(
            "-",
            &[LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))],
        );
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );

        // Binary `-` Int64 - Int64 → Int64
        let result = registry.infer_return_type(
            "-",
            &[
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            ],
        );
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_arity_mismatch_falls_back_to_top() {
        use crate::compile::diagnostics::DiagnosticsCollector;

        DiagnosticsCollector::clear();
        DiagnosticsCollector::enable();

        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // `+` is exact arity 2; passing 3 args should yield Top + diagnostic.
        let result = registry.infer_return_type(
            "+",
            &[
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            ],
        );
        assert_eq!(result, LatticeType::Top);

        // `!` is exact arity 1; passing 2 args should yield Top + diagnostic.
        let result = registry.infer_return_type(
            "!",
            &[
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            ],
        );
        assert_eq!(result, LatticeType::Top);

        let diags = DiagnosticsCollector::take();
        assert!(
            diags.iter().any(|d| matches!(
                &d.reason,
                crate::compile::diagnostics::DiagnosticReason::Other(msg)
                    if msg.contains("arity mismatch")
            )),
            "expected at least one arity-mismatch diagnostic, got: {:?}",
            diags,
        );

        DiagnosticsCollector::disable();
    }

    #[test]
    fn test_metadata_migrated_array_tfunc_dispatch_remains_correct() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        // `getindex` is metadata-bearing now, while dispatch remains unchanged.
        assert_eq!(registry.arity_bounds("getindex"), Some((2, None)));
        assert_eq!(registry.cost("getindex"), Some(COST_CHEAP));
        let args = vec![
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                ))),
                ndims: None,
            }),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = registry.infer_return_type("getindex", &args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_migrated_intrinsics_metadata() {
        let mut registry = TransferFunctions::new();
        register_intrinsics(&mut registry);

        // Migrated unary maths.
        assert_eq!(registry.arity_bounds("sqrt"), Some((1, Some(1))));
        assert_eq!(registry.cost("sqrt"), Some(COST_MEDIUM));
        assert_eq!(registry.arity_bounds("abs"), Some((1, Some(1))));
        assert_eq!(registry.cost("abs"), Some(COST_CHEAP));

        // `isa` and `typeof`.
        assert_eq!(registry.arity_bounds("isa"), Some((2, Some(2))));
        assert_eq!(registry.arity_bounds("typeof"), Some((1, Some(1))));

        // `print` / `println` are variadic.
        assert_eq!(registry.arity_bounds("println"), Some((0, None)));
        assert_eq!(registry.arity_bounds("print"), Some((0, None)));

        // `convert` is exact arity 2 and uses method dispatch before fallback.
        assert_eq!(registry.arity_bounds("convert"), Some((2, Some(2))));
        assert_eq!(registry.cost("convert"), Some(COST_CHEAP));
    }
}
