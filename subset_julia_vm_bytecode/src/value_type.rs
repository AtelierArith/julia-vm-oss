//! Bytecode value type tags.

use crate::ArrayElementType;

/// Runtime type tags for Julia values in compiled bytecode.
///
/// This enum is used for:
/// - Method dispatch (matching argument types to method signatures)
/// - Type inference results (what type a variable or expression has)
/// - Code generation (selecting appropriate VM instructions)
///
/// # Related Types
///
/// - `LatticeType` in `compile/lattice/types.rs`: Compile-time abstract type (more precise)
/// - `JuliaType` in `types.rs`: Julia type representation for typeof() results
/// - `Value` in `subset_julia_vm::vm::value`: The actual runtime value (ValueType is just the type tag)
///
/// # Adding New Variants
///
/// When adding a new variant to this enum, update all consumers that map
/// between bytecode tags, compiler lattice facts, runtime values, and display.
/// Key files include:
///
/// 1. `compile/bridge.rs`:
///    - `value_type_to_lattice` (ValueType -> LatticeType conversion)
///    - `lattice_to_value_type` (LatticeType -> ValueType conversion)
///    - Add the variant to `test_all_valuetype_variants_to_lattice`
///
/// 2. `compile/stmt.rs`:
///    - `emit_return_for_type`
///    - Store instruction matches in loop lowering
///    - Return instruction matches
///
/// 3. `compile/expr/infer.rs` and related type helpers:
///    - literal type inference, JuliaType conversion, and array element mapping
///
/// 4. `vm/builtins_reflection.rs`, `vm/formatting.rs`, and `bin/sjulia.rs`:
///    - runtime `typeof` / display / REPL formatting
///
/// 5. FFI format/basic boundaries:
///    - host-facing formatting of returned `Value` results
///
/// Note: Copy removed because ArrayOf contains ArrayElementType which can contain Vec
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ValueType {
    // Signed integers
    I8,
    I16,
    I32,
    I64,
    I128,
    BigInt, // Arbitrary precision integer
    // Unsigned integers
    U8,
    U16,
    U32,
    U64,
    U128,
    // Boolean
    Bool,
    // Floating point
    F16,
    F32,
    F64,
    BigFloat, // Arbitrary precision float
    // Collections
    Array, // Legacy array type (treated as F64 for backward compatibility)
    ArrayOf(ArrayElementType, Option<usize>), // Array with known element type; 2nd field = rank (ndims), None = unknown→Vector (Issue #6817)
    Memory,                                   // Memory{T} flat typed buffer (element type unknown)
    MemoryOf(ArrayElementType),               // Memory{T} with known element type
    Range,                                    // Lazy range type
    // String types
    Str,
    Char, // Julia's Char type (32-bit Unicode codepoint)
    // Special types
    Nothing,       // Julia's Nothing type (type of `nothing`)
    Missing,       // Julia's Missing type (type of `missing`)
    Struct(usize), // type_id - includes Complex which is now a Pure Julia struct
    Rng,           // RNG instance type
    Tuple,         // Tuple type
    NamedTuple,    // Named tuple type
    Pairs,         // Base.Pairs type (for kwargs...)
    Dict,          // Dictionary type
    Set,           // Set type (unique elements)
    Generator,     // Generator type (lazy map)
    DataType,      // Type of types (returned by typeof)
    Module,        // Module type (e.g., Statistics, Base)
    Function,      // Function type (abstract supertype of all functions)
    IO,            // IO stream type
    // Macro system types
    Symbol,         // Julia Symbol type
    Expr,           // Julia Expr type
    QuoteNode,      // QuoteNode type
    LineNumberNode, // LineNumberNode type
    GlobalRef,      // GlobalRef type
    // Regex types
    Regex,      // Julia Regex type
    RegexMatch, // Julia RegexMatch type
    Any,        // Dynamic type - determined at runtime
    // Union type (preserves type information for optimization)
    Union(Vec<ValueType>), // Union of multiple types, e.g., Union{Int64, Float64}
    // Enum type (from @enum macro)
    Enum, // Enum type
    // Concrete Complex scalar tags used by runtime specialization. The runtime
    // value remains a Pure Julia StructRef/Struct; this avoids losing the
    // Complex element type in erased call paths.
    ComplexF32,
    ComplexF64,
}
