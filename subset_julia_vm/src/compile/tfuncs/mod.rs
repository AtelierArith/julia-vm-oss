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
//!     LatticeType::Concrete(ConcreteType::Int64),
//!     LatticeType::Concrete(ConcreteType::Int64),
//! ];
//! let result = registry.infer_return_type("+", &args);
//! ```

pub mod arithmetic;
pub mod array_ops;
pub mod collection_ops;
pub mod complex_ops;
pub mod field_ops;
pub mod intrinsics;
pub mod iterator_ops;
pub mod math_intrinsics;
pub mod registry;
pub mod string_ops;

pub use registry::{ContextualTransferFn, TFuncContext, TransferFn, TransferFunctions};

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
}

/// Registers arithmetic and comparison transfer functions.
pub fn register_arithmetic(registry: &mut TransferFunctions) {
    registry.register("+", arithmetic::tfunc_add);
    registry.register("-", arithmetic::tfunc_sub);
    registry.register("*", arithmetic::tfunc_mul);
    registry.register("/", arithmetic::tfunc_div);
    registry.register("==", arithmetic::tfunc_eq);
    registry.register("<", arithmetic::tfunc_lt);
    registry.register("<=", arithmetic::tfunc_le);
    registry.register(">", arithmetic::tfunc_gt);
    registry.register(">=", arithmetic::tfunc_ge);
    registry.register("!", arithmetic::tfunc_not);
}

/// Registers array operation transfer functions.
pub fn register_array_ops(registry: &mut TransferFunctions) {
    // Basic array operations
    registry.register("getindex", array_ops::tfunc_getindex);
    registry.register("setindex!", array_ops::tfunc_setindex);
    registry.register("length", array_ops::tfunc_length);
    registry.register("first", array_ops::tfunc_first);
    registry.register("last", array_ops::tfunc_last);
    registry.register("size", array_ops::tfunc_size);

    // Array mutation operations
    registry.register("push!", array_ops::tfunc_push);
    registry.register("pop!", array_ops::tfunc_pop);
    registry.register("append!", array_ops::tfunc_append);
    registry.register("prepend!", array_ops::tfunc_prepend);
    registry.register("insert!", array_ops::tfunc_insert);
    registry.register("deleteat!", array_ops::tfunc_deleteat);
    registry.register("popfirst!", array_ops::tfunc_popfirst);
    registry.register("pushfirst!", array_ops::tfunc_pushfirst);
    registry.register("empty!", array_ops::tfunc_empty_bang);
    registry.register("resize!", array_ops::tfunc_resize);
    registry.register("splice!", array_ops::tfunc_splice);
    registry.register("fill!", array_ops::tfunc_fill_bang);

    // Sorting and ordering
    registry.register("sort", array_ops::tfunc_sort);
    registry.register("sort!", array_ops::tfunc_sort_bang);
    registry.register("reverse", array_ops::tfunc_reverse);
    registry.register("reverse!", array_ops::tfunc_reverse_bang);
    registry.register("unique", array_ops::tfunc_unique);
    registry.register("unique!", array_ops::tfunc_unique_bang);

    // Array creation
    registry.register("fill", array_ops::tfunc_fill);
    registry.register("zeros", array_ops::tfunc_zeros);
    registry.register("ones", array_ops::tfunc_ones);
    registry.register("similar", array_ops::tfunc_similar);
    registry.register("copy", array_ops::tfunc_copy);
    registry.register("deepcopy", array_ops::tfunc_deepcopy);

    // Range construction
    registry.register(":", array_ops::tfunc_colon);
    registry.register("colon", array_ops::tfunc_colon);
    registry.register("range", array_ops::tfunc_range);

    // Higher-order functions
    registry.register("map", array_ops::tfunc_map);
    registry.register("filter", array_ops::tfunc_filter);

    // Reduction operations
    registry.register("reduce", array_ops::tfunc_reduce);
    registry.register("foldl", array_ops::tfunc_foldl);
    registry.register("foldr", array_ops::tfunc_foldr);
    registry.register("sum", array_ops::tfunc_sum);
    registry.register("prod", array_ops::tfunc_prod);
    registry.register("maximum", array_ops::tfunc_maximum);
    registry.register("minimum", array_ops::tfunc_minimum);
    registry.register("any", array_ops::tfunc_any);
    registry.register("all", array_ops::tfunc_all);
    registry.register("collect", array_ops::tfunc_collect);
}

/// Registers string operation transfer functions.
pub fn register_string_ops(registry: &mut TransferFunctions) {
    registry.register("string", string_ops::tfunc_string);
    registry.register("uppercase", string_ops::tfunc_uppercase);
    registry.register("lowercase", string_ops::tfunc_lowercase);
    registry.register("replace", string_ops::tfunc_replace);
    registry.register("split", string_ops::tfunc_split);
    registry.register("join", string_ops::tfunc_join);
    registry.register("startswith", string_ops::tfunc_startswith);
    registry.register("endswith", string_ops::tfunc_endswith);
    registry.register("contains", string_ops::tfunc_contains);
}

/// Registers intrinsic and conversion transfer functions.
pub fn register_intrinsics(registry: &mut TransferFunctions) {
    registry.register("isa", intrinsics::tfunc_isa);
    registry.register("typeof", intrinsics::tfunc_typeof);
    registry.register("convert", intrinsics::tfunc_convert);
    registry.register("promote", intrinsics::tfunc_promote);

    // Integer type conversions
    registry.register("Int8", intrinsics::tfunc_to_int8);
    registry.register("Int16", intrinsics::tfunc_to_int16);
    registry.register("Int32", intrinsics::tfunc_to_int32);
    registry.register("Int64", intrinsics::tfunc_to_int64);
    registry.register("Int128", intrinsics::tfunc_to_int128);
    registry.register("UInt8", intrinsics::tfunc_to_uint8);
    registry.register("UInt16", intrinsics::tfunc_to_uint16);
    registry.register("UInt32", intrinsics::tfunc_to_uint32);
    registry.register("UInt64", intrinsics::tfunc_to_uint64);
    registry.register("UInt128", intrinsics::tfunc_to_uint128);

    // Float type conversions
    registry.register("Float32", intrinsics::tfunc_to_float32);
    registry.register("Float64", intrinsics::tfunc_to_float64);

    // Other type conversions
    registry.register("Bool", intrinsics::tfunc_to_bool);
    registry.register("String", intrinsics::tfunc_to_string);
    registry.register("Char", intrinsics::tfunc_to_char);

    // Type value functions
    registry.register("zero", intrinsics::tfunc_zero);
    registry.register("one", intrinsics::tfunc_one);
    registry.register("typemin", intrinsics::tfunc_typemin);
    registry.register("typemax", intrinsics::tfunc_typemax);

    // Mathematical functions
    registry.register("sqrt", intrinsics::tfunc_sqrt);
    registry.register("abs", intrinsics::tfunc_abs);
    registry.register("sin", intrinsics::tfunc_sin);
    registry.register("cos", intrinsics::tfunc_cos);
    registry.register("exp", intrinsics::tfunc_exp);
    registry.register("log", intrinsics::tfunc_log);
    registry.register("min", intrinsics::tfunc_min);
    registry.register("max", intrinsics::tfunc_max);

    // I/O functions
    registry.register("println", intrinsics::tfunc_println);
    registry.register("print", intrinsics::tfunc_println); // Same behavior
}

/// Registers field access transfer functions.
pub fn register_field_ops(registry: &mut TransferFunctions) {
    registry.register("getfield", field_ops::tfunc_getfield);
    registry.register("setfield!", field_ops::tfunc_setfield);
    registry.register("fieldnames", field_ops::tfunc_fieldnames);
    registry.register("fieldtypes", field_ops::tfunc_fieldtypes);

    // Register contextual transfer function for getfield (with struct table access)
    registry.register_contextual("getfield", field_ops::tfunc_getfield_contextual);
}

/// Registers iterator operation transfer functions.
pub fn register_iterator_ops(registry: &mut TransferFunctions) {
    registry.register("iterate", iterator_ops::tfunc_iterate);
    // Note: length is already registered in array_ops, but we provide an alias here
    // registry.register("length", iterator_ops::tfunc_length_iter);
    registry.register("eachindex", iterator_ops::tfunc_eachindex);
    registry.register("enumerate", iterator_ops::tfunc_enumerate);
    registry.register("zip", iterator_ops::tfunc_zip);
}

/// Registers collection operation transfer functions.
pub fn register_collection_ops(registry: &mut TransferFunctions) {
    // Dictionary access
    registry.register("keys", collection_ops::tfunc_keys);
    registry.register("values", collection_ops::tfunc_values);
    registry.register("pairs", collection_ops::tfunc_pairs);
    registry.register("haskey", collection_ops::tfunc_haskey);
    registry.register("get", collection_ops::tfunc_get);
    registry.register("get!", collection_ops::tfunc_get_bang);

    // Dictionary mutation
    registry.register("delete!", collection_ops::tfunc_delete);
    registry.register("merge", collection_ops::tfunc_merge);
    registry.register("merge!", collection_ops::tfunc_merge_bang);

    // Collection queries
    registry.register("isempty", collection_ops::tfunc_isempty);
    registry.register("in", collection_ops::tfunc_in);
    registry.register("∈", collection_ops::tfunc_in);
    registry.register("eltype", collection_ops::tfunc_eltype);
    registry.register("keytype", collection_ops::tfunc_keytype);
    registry.register("valtype", collection_ops::tfunc_valtype);

    // Constructors
    registry.register("Set", collection_ops::tfunc_set);
    registry.register("Dict", collection_ops::tfunc_dict);

    // Set operations
    registry.register("union", collection_ops::tfunc_union);
    registry.register("intersect", collection_ops::tfunc_intersect);
    registry.register("setdiff", collection_ops::tfunc_setdiff);
    registry.register("symdiff", collection_ops::tfunc_symdiff);
    registry.register("issubset", collection_ops::tfunc_issubset);
    registry.register("⊆", collection_ops::tfunc_issubset);
}

/// Registers mathematical intrinsic transfer functions.
pub fn register_math_intrinsics(registry: &mut TransferFunctions) {
    registry.register("sign", math_intrinsics::tfunc_sign);
    registry.register("div", math_intrinsics::tfunc_div);
    registry.register("rem", math_intrinsics::tfunc_rem);
    registry.register("mod", math_intrinsics::tfunc_mod);
    registry.register("floor", math_intrinsics::tfunc_floor);
    registry.register("ceil", math_intrinsics::tfunc_ceil);
    registry.register("round", math_intrinsics::tfunc_round);
    registry.register("<<", math_intrinsics::tfunc_lshift);
    registry.register(">>", math_intrinsics::tfunc_rshift);
    registry.register("&", math_intrinsics::tfunc_bitand);
    registry.register("|", math_intrinsics::tfunc_bitor);
    registry.register("xor", math_intrinsics::tfunc_xor);
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
    registry.register("real", complex_ops::tfunc_real);
    registry.register("imag", complex_ops::tfunc_imag);
    registry.register("conj", complex_ops::tfunc_conj);
    registry.register("abs2", complex_ops::tfunc_abs2);
    registry.register("angle", complex_ops::tfunc_angle);
    registry.register("reim", complex_ops::tfunc_reim);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConcreteType, LatticeType};

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
        assert!(registry.has_function("split"));
    }

    #[test]
    fn test_intrinsics_registration() {
        let mut registry = TransferFunctions::new();
        register_intrinsics(&mut registry);

        assert!(registry.has_function("isa"));
        assert!(registry.has_function("sqrt"));
        assert!(registry.has_function("Int64"));
        assert!(registry.has_function("println"));
    }

    #[test]
    fn test_end_to_end_add() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = registry.infer_return_type("+", &args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_end_to_end_getindex() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Float64),
            }),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = registry.infer_return_type("getindex", &args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_end_to_end_length() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        })];
        let result = registry.infer_return_type("length", &args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_end_to_end_string() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::String),
        ];
        let result = registry.infer_return_type("string", &args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::String));
    }

    #[test]
    fn test_end_to_end_sqrt() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = registry.infer_return_type("sqrt", &args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_end_to_end_isa() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Int64), LatticeType::Top];
        let result = registry.infer_return_type("isa", &args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_unknown_function() {
        let mut registry = TransferFunctions::new();
        register_all(&mut registry);

        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
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
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
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
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }
}
