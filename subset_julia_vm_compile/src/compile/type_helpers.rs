//! Type helper functions for the compiler.
//!
//! Functions for type hierarchy, bounds checking, and type conversion.

use crate::inference_core::{CoreSubtypeEngine, CoreType};
use crate::runtime_types::{ArrayElementType, ValueType};
use crate::types::{builtin_type_for_compiler, JuliaType};

use crate::compile::context::StructRegistry;

/// Check if a name is a built-in type name (for isa() and <: checks)
pub(super) fn is_builtin_type_name(name: &str) -> bool {
    // Handle Union{...} types dynamically
    if name.starts_with("Union{") && name.ends_with('}') {
        return true;
    }
    builtin_type_for_compiler(name).is_some()
}

/// Get the abstract type ancestors for a built-in concrete type.
/// Returns the chain of abstract types that the concrete type is a subtype of.
pub(super) fn get_builtin_type_ancestors(jt: &JuliaType) -> Vec<&'static str> {
    if let Some(chain) = CoreType::from(jt).builtin_supertype_chain_names() {
        return chain;
    }

    match jt {
        // Signed integers
        JuliaType::Int8 => vec!["Int8", "Signed", "Integer", "Real", "Number", "Any"],
        JuliaType::Int16 => vec!["Int16", "Signed", "Integer", "Real", "Number", "Any"],
        JuliaType::Int32 => vec!["Int32", "Signed", "Integer", "Real", "Number", "Any"],
        JuliaType::Int64 => vec!["Int64", "Signed", "Integer", "Real", "Number", "Any"],
        JuliaType::Int128 => vec!["Int128", "Signed", "Integer", "Real", "Number", "Any"],
        JuliaType::BigInt => vec!["BigInt", "Signed", "Integer", "Real", "Number", "Any"],
        // Unsigned integers
        JuliaType::UInt8 => vec!["UInt8", "Unsigned", "Integer", "Real", "Number", "Any"],
        JuliaType::UInt16 => vec!["UInt16", "Unsigned", "Integer", "Real", "Number", "Any"],
        JuliaType::UInt32 => vec!["UInt32", "Unsigned", "Integer", "Real", "Number", "Any"],
        JuliaType::UInt64 => vec!["UInt64", "Unsigned", "Integer", "Real", "Number", "Any"],
        JuliaType::UInt128 => vec!["UInt128", "Unsigned", "Integer", "Real", "Number", "Any"],
        // Boolean (subtype of Integer in Julia)
        JuliaType::Bool => vec!["Bool", "Integer", "Real", "Number", "Any"],
        // Floating point
        JuliaType::Float16 => vec!["Float16", "AbstractFloat", "Real", "Number", "Any"],
        JuliaType::Float32 => vec!["Float32", "AbstractFloat", "Real", "Number", "Any"],
        JuliaType::Float64 => vec!["Float64", "AbstractFloat", "Real", "Number", "Any"],
        JuliaType::BigFloat => vec!["BigFloat", "AbstractFloat", "Real", "Number", "Any"],
        // Complex
        // Complex numbers are now Pure Julia structs - handled in Struct arm
        // Other concrete types
        JuliaType::String => vec!["String", "AbstractString", "Any"],
        JuliaType::Char => vec!["Char", "AbstractChar", "Any"],
        JuliaType::Array => vec!["Array", "AbstractArray", "Any"],
        JuliaType::VectorOf(_) => vec!["Vector", "Array", "AbstractVector", "AbstractArray", "Any"],
        JuliaType::MatrixOf(_) => vec!["Matrix", "Array", "AbstractMatrix", "AbstractArray", "Any"],
        JuliaType::Tuple | JuliaType::TupleOf(_) => vec!["Tuple", "Any"],
        JuliaType::NamedTuple => vec!["NamedTuple", "Any"],
        JuliaType::Dict => vec!["Dict", "AbstractDict", "Any"],
        JuliaType::Set => vec!["Set", "AbstractSet", "Any"],
        JuliaType::UnitRange => vec!["UnitRange", "AbstractUnitRange", "AbstractRange", "Any"],
        JuliaType::StepRange => vec!["StepRange", "AbstractRange", "Any"],
        JuliaType::Nothing => vec!["Nothing", "Any"],
        JuliaType::Missing => vec!["Missing", "Any"],
        JuliaType::Any => vec!["Any"],
        JuliaType::Module => vec!["Module", "Any"],
        JuliaType::DataType => vec!["DataType", "Type", "Any"],
        // Abstract types - they are their own ancestors plus parents
        JuliaType::Number => vec!["Number", "Any"],
        JuliaType::Real => vec!["Real", "Number", "Any"],
        JuliaType::Integer => vec!["Integer", "Real", "Number", "Any"],
        JuliaType::Signed => vec!["Signed", "Integer", "Real", "Number", "Any"],
        JuliaType::Unsigned => vec!["Unsigned", "Integer", "Real", "Number", "Any"],
        JuliaType::AbstractFloat => vec!["AbstractFloat", "Real", "Number", "Any"],
        JuliaType::AbstractString => vec!["AbstractString", "Any"],
        JuliaType::AbstractChar => vec!["AbstractChar", "Any"],
        JuliaType::AbstractArray => vec!["AbstractArray", "Any"],
        JuliaType::AbstractRange => vec!["AbstractRange", "Any"],
        JuliaType::IO => vec!["IO", "Any"],
        JuliaType::IOBuffer => vec!["IOBuffer", "IO", "Any"],
        JuliaType::Function => vec!["Function", "Any"],
        // User-defined types - handled separately. Issue #3550: typed range
        // values now report `UnitRange{T}` / `StepRange{T,T}` /
        // `StepRangeLen{...}` via `JuliaType::Struct`, so we recover their
        // builtin ancestor chain here so method dispatch (e.g. `*` on
        // `Int64 × UnitRange{Int64}` falling through to `Base.*` overloads)
        // continues to find the right overload.
        JuliaType::Struct(name) => {
            let base = name
                .split('{')
                .next()
                .unwrap_or(name.as_str())
                .rsplit('.')
                .next()
                .unwrap_or(name.as_str());
            match base {
                "BitVector" => vec![
                    "BitVector",
                    "BitArray",
                    "AbstractVector",
                    "AbstractArray",
                    "Any",
                ],
                "BitMatrix" => vec![
                    "BitMatrix",
                    "BitArray",
                    "AbstractMatrix",
                    "AbstractArray",
                    "Any",
                ],
                "BitArray" => vec!["BitArray", "AbstractArray", "Any"],
                "UnitRange" => vec!["UnitRange", "AbstractUnitRange", "AbstractRange", "Any"],
                "StepRange" => vec!["StepRange", "OrdinalRange", "AbstractRange", "Any"],
                "OneTo" => vec!["OneTo", "AbstractUnitRange", "AbstractRange", "Any"],
                "StepRangeLen" => vec!["StepRangeLen", "AbstractRange", "Any"],
                "LinRange" => vec!["LinRange", "AbstractRange", "Any"],
                _ => vec!["Any"],
            }
        }
        JuliaType::AbstractUser(_, _) => vec!["Any"],
        // Type variable from where clause
        JuliaType::TypeVar(_, _) | JuliaType::RuntimeTypeVar { .. } => vec!["Any"],
        JuliaType::RuntimeParametric { .. } => vec!["Any"],
        // Bottom type (Union{}) - subtype of everything
        JuliaType::Bottom => vec!["Any"],
        // Union type - treat as Any for ancestor lookup
        JuliaType::Union(_) => vec!["Any"],
        // Type hierarchy
        JuliaType::Type => vec!["Type", "Any"],
        // Type{T} pattern - for type matching
        // TypeOf{T} is a subtype of Type (Type{Int64} <: Type)
        JuliaType::TypeOf(_) => vec!["Type", "Any"],
        // Macro system types
        JuliaType::Symbol => vec!["Symbol", "Any"],
        JuliaType::Expr => vec!["Expr", "Any"],
        JuliaType::QuoteNode => vec!["QuoteNode", "Any"],
        JuliaType::LineNumberNode => vec!["LineNumberNode", "Any"],
        JuliaType::GlobalRef => vec!["GlobalRef", "Any"],
        // Base.Pairs type (for kwargs...)
        JuliaType::Pairs => vec!["Pairs", "Any"],
        // Base.Generator type (for generator expressions)
        JuliaType::Generator => vec!["Generator", "Any"],
        // UnionAll type - treat as Any for ancestor lookup
        JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. } => vec!["Any"],
        // Enum type - enums are integer-backed types
        JuliaType::Enum(_) => vec!["Enum", "Any"],
    }
}

/// Check if a concrete type satisfies a type parameter bound.
/// The bound is a string (type name) to support both built-in and user-defined abstract types.
pub(super) fn check_type_satisfies_bound(concrete_type: &JuliaType, bound_name: &str) -> bool {
    // If bound is "Any", everything satisfies it
    if bound_name == "Any" {
        return true;
    }

    // If concrete type is Any, it satisfies any bound (runtime check will enforce)
    if matches!(concrete_type, JuliaType::Any) {
        return true;
    }

    if CoreSubtypeEngine::new().is_subtype(
        &CoreType::from(concrete_type),
        &CoreType::from_julia_name(bound_name),
    ) {
        return true;
    }

    // Get ancestors for the concrete type
    let ancestors = get_builtin_type_ancestors(concrete_type);

    // Check if bound_name is in the ancestor chain
    ancestors.contains(&bound_name)
}

/// Whether a (possibly module-qualified) type name denotes an RNG type that the
/// VM models with `Value::Rng` / `ValueType::Rng` (Issue #7231).
///
/// Covers the concrete RNG types the VM can construct (`Xoshiro`, `StableRNG`),
/// the global-handle type (`TaskLocalRNG`, from `default_rng()`/`GLOBAL_RNG`),
/// the not-yet-constructible-but-annotatable `MersenneTwister`, and the abstract
/// supertype `AbstractRNG`. Mapping these annotations to `ValueType::Rng` lets a
/// param like `rng::Xoshiro` or `rng::AbstractRNG` compile `randn(rng)` /
/// `rand(rng)` to the scalar-from-rng form instead of the `randn(dims...)` form.
pub(super) fn is_rng_type_name(name: &str) -> bool {
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    matches!(
        base,
        "Xoshiro" | "StableRNG" | "MersenneTwister" | "TaskLocalRNG" | "AbstractRNG"
    )
}

/// Convert JuliaType to ValueType.
pub(super) fn julia_type_to_value_type(jt: &JuliaType) -> ValueType {
    match jt {
        // Signed integers
        JuliaType::Int8
        | JuliaType::Int16
        | JuliaType::Int32
        | JuliaType::Int64
        | JuliaType::Int128 => ValueType::I64,
        JuliaType::BigInt => ValueType::BigInt,
        // Unsigned integers
        JuliaType::UInt8
        | JuliaType::UInt16
        | JuliaType::UInt32
        | JuliaType::UInt64
        | JuliaType::UInt128 => ValueType::I64,
        // Boolean: keep as ValueType::Bool so caller-side static dispatch
        // routes to Bool methods (println/show, user `foo(::Bool)`, etc.).
        // The runtime `Value::Bool` tag is already preserved through
        // `Instr::ReturnI64` (`vm/exec/return_ops.rs::preserved_val`), so the
        // VM bytecode for a `::Bool`-annotated function does not change.
        // Issue #4376.
        JuliaType::Bool => ValueType::Bool,
        // Floating point
        JuliaType::Float16 => ValueType::F16,
        JuliaType::Float32 => ValueType::F32,
        JuliaType::Float64 => ValueType::F64,
        JuliaType::BigFloat => ValueType::BigFloat,
        // Complex is now a Pure Julia struct - falls through to Struct case
        // String/Char
        JuliaType::String | JuliaType::AbstractString => ValueType::Str,
        JuliaType::Char | JuliaType::AbstractChar => ValueType::Char,
        // Collections
        JuliaType::Array
        | JuliaType::AbstractArray
        | JuliaType::VectorOf(_)
        | JuliaType::MatrixOf(_) => ValueType::Array,
        JuliaType::Tuple | JuliaType::TupleOf(_) => ValueType::Tuple,
        JuliaType::NamedTuple => ValueType::NamedTuple,
        JuliaType::Dict => ValueType::Dict,
        JuliaType::Set => ValueType::Set,
        // Special types
        JuliaType::Nothing => ValueType::Nothing,
        JuliaType::Missing => ValueType::Missing,
        // Range types
        JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange => ValueType::Range,
        // Abstract types
        JuliaType::Number => ValueType::F64, // Number is abstract numeric
        JuliaType::Real | JuliaType::AbstractFloat => ValueType::F64,
        JuliaType::Integer | JuliaType::Signed | JuliaType::Unsigned => ValueType::I64,
        JuliaType::Any => ValueType::Any, // Dynamic type - determined at runtime
        JuliaType::Struct(name) => {
            // RNG type annotations (Xoshiro/StableRNG/MersenneTwister/
            // TaskLocalRNG/AbstractRNG) map to ValueType::Rng so randn(rng) /
            // rand(rng) on such a param compile to the scalar-from-rng form
            // (Issue #7231).
            if is_rng_type_name(name) {
                return ValueType::Rng;
            }
            if let Some(memory_type) = memory_struct_name_to_value_type(name) {
                return memory_type;
            }
            // Without struct_table context, we can't resolve type_id for user-defined structs
            // Use julia_type_to_value_type_with_table for contexts where struct_table is available

            // Debug assertion: Warn if struct name matches a known builtin type name.
            // This can indicate a bug where from_name() is missing a mapping for a builtin type,
            // causing type annotations like `f::Function` to be incorrectly parsed as structs.
            // See Issue #1328 for an example of this bug.
            #[cfg(debug_assertions)]
            {
                const BUILTIN_TYPE_NAMES: &[&str] = &[
                    // These are builtin Julia types that should NEVER appear as JuliaType::Struct
                    // If one does, it means from_name() is missing a mapping
                    "Function",
                    "Symbol",
                    "Expr",
                    "QuoteNode",
                    "LineNumberNode",
                    "GlobalRef",
                    "Int8",
                    "Int16",
                    "Int32",
                    "Int64",
                    "Int128",
                    "BigInt",
                    "UInt8",
                    "UInt16",
                    "UInt32",
                    "UInt64",
                    "UInt128",
                    "Float16",
                    "Float32",
                    "Float64",
                    "BigFloat",
                    "Bool",
                    "String",
                    "Char",
                    "Array",
                    "Vector",
                    "Tuple",
                    "NamedTuple",
                    "Dict",
                    "Set",
                    "UnitRange",
                    "StepRange",
                    "Nothing",
                    "Missing",
                    "Any",
                    "Number",
                    "Real",
                    "Integer",
                    "Signed",
                    "Unsigned",
                    "AbstractFloat",
                    "AbstractString",
                    "AbstractChar",
                    "AbstractArray",
                    "AbstractRange",
                    "IO",
                    "IOBuffer",
                    "Module",
                    "Type",
                    "DataType",
                ];
                if BUILTIN_TYPE_NAMES.contains(&name.as_str()) {
                    use std::io::Write;
                    let _ = writeln!(
                        std::io::stderr(),
                        "WARNING: JuliaType::Struct({:?}) detected, but {:?} is a builtin type. \
                         This likely means JuliaType::from_name() is missing a mapping. \
                         See Issue #1328.",
                        name,
                        name
                    );
                }
            }
            #[cfg(not(debug_assertions))]
            let _ = name;

            ValueType::Any
        }
        JuliaType::Type => ValueType::DataType, // Abstract supertype of all type objects
        JuliaType::DataType => ValueType::DataType, // Type type (returned by typeof for types)
        JuliaType::Module => ValueType::Module, // Module type
        JuliaType::IO => ValueType::IO,         // IO stream type (abstract)
        JuliaType::IOBuffer => ValueType::IO,   // IOBuffer (concrete subtype of IO)
        JuliaType::AbstractUser(_, _) => ValueType::Any, // User-defined abstract types
        JuliaType::TypeVar(_, _) | JuliaType::RuntimeTypeVar { .. } => ValueType::Any,
        JuliaType::RuntimeParametric { .. } => ValueType::Any,
        JuliaType::Bottom => ValueType::Union(Vec::new()), // Union{} - bottom type
        JuliaType::Union(types) => ValueType::Union(
            types
                .iter()
                .map(julia_type_to_value_type)
                .collect::<Vec<_>>(),
        ),
        JuliaType::TypeOf(_) => ValueType::DataType, // Type{T} pattern
        // Macro system types
        JuliaType::Symbol => ValueType::Symbol,
        JuliaType::Expr => ValueType::Expr,
        JuliaType::QuoteNode => ValueType::QuoteNode,
        JuliaType::LineNumberNode => ValueType::LineNumberNode,
        JuliaType::GlobalRef => ValueType::GlobalRef,
        // Base.Pairs type (for kwargs...)
        JuliaType::Pairs => ValueType::Pairs,
        // Base.Generator type (for generator expressions)
        JuliaType::Generator => ValueType::Generator,
        // Function type (abstract supertype of all functions)
        JuliaType::Function => ValueType::Function,
        // UnionAll type - runtime dependent
        JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. } => ValueType::Any,
        // Enum type
        JuliaType::Enum(_) => ValueType::Enum,
    }
}

pub(super) fn memory_struct_name_to_value_type(name: &str) -> Option<ValueType> {
    let name = name.strip_prefix("Base.").unwrap_or(name);
    if name == "Memory" {
        return Some(ValueType::Memory);
    }
    let elem_name = name
        .strip_prefix("Memory{")
        .and_then(|inner| inner.strip_suffix('}'))?;
    let elem_type = match elem_name {
        "Int" if crate::types::native_int_type_name() == "Int32" => ArrayElementType::I32,
        "Int64" | "Int" => ArrayElementType::I64,
        "Int8" => ArrayElementType::I8,
        "Int16" => ArrayElementType::I16,
        "Int32" => ArrayElementType::I32,
        "Int128" => ArrayElementType::I128,
        "UInt8" => ArrayElementType::U8,
        "UInt16" => ArrayElementType::U16,
        "UInt32" => ArrayElementType::U32,
        "UInt" if crate::types::native_uint_type_name() == "UInt32" => ArrayElementType::U32,
        "UInt64" | "UInt" => ArrayElementType::U64,
        "UInt128" => ArrayElementType::U128,
        "Float64" => ArrayElementType::F64,
        "Float32" => ArrayElementType::F32,
        // Issue #9301: Memory{Float16} keeps its concrete eltype (boxed storage
        // tag, like Int128/UInt128) so it does not widen to Any.
        "Float16" => ArrayElementType::F16,
        "Bool" => ArrayElementType::Bool,
        "Char" => ArrayElementType::Char,
        "Symbol" => ArrayElementType::Symbol,
        "String" => ArrayElementType::String,
        "Union{}" => ArrayElementType::UnionOf(Vec::new()),
        _ if elem_name.starts_with("Union{") && elem_name.ends_with('}') => {
            ArrayElementType::union_from_body(&elem_name[6..elem_name.len() - 1])
        }
        "Number" | "Real" | "Integer" | "Signed" | "Unsigned" | "AbstractFloat" => {
            ArrayElementType::Abstract(elem_name.to_string())
        }
        "Any" => ArrayElementType::Any,
        _ => ArrayElementType::Any,
    };
    Some(ValueType::MemoryOf(elem_type))
}

/// Convert JuliaType to ValueType with struct table context for resolving user-defined struct type_ids.
pub(super) fn julia_type_to_value_type_with_table(
    jt: &JuliaType,
    struct_table: &StructRegistry,
) -> ValueType {
    match jt {
        JuliaType::Bottom => ValueType::Union(Vec::new()),
        JuliaType::Union(types) => ValueType::Union(
            types
                .iter()
                .map(|ty| julia_type_to_value_type_with_table(ty, struct_table))
                .collect::<Vec<_>>(),
        ),
        JuliaType::Struct(name) => {
            // RNG type annotations map to ValueType::Rng (Issue #7231).
            if is_rng_type_name(name) {
                return ValueType::Rng;
            }
            // Memory{T} annotations — resolve element type from the name,
            // same as julia_type_to_value_type does (Issue #9009).
            if let Some(memory_type) = memory_struct_name_to_value_type(name) {
                return memory_type;
            }
            // Look up type_id from struct_table
            if let Some((_, info)) = struct_table.resolve(name) {
                return ValueType::Struct(info.type_id);
            }
            // Handle parametric struct names like "Complex{Float64}"
            // Extract base name and look up
            let base_name = if let Some(brace_idx) = name.find('{') {
                &name[..brace_idx]
            } else {
                name.as_str()
            };
            if let Some((_, info)) = struct_table.resolve(base_name) {
                ValueType::Struct(info.type_id)
            } else {
                ValueType::Any // Unknown struct - fallback to Any
            }
        }
        _ => julia_type_to_value_type(jt),
    }
}

/// Convert JuliaType to ValueType for method signatures whose origin may need
/// the owner-exact Main/Base declaration (Issues #10258/#11046).
pub(super) fn julia_type_to_value_type_scoped(
    jt: &JuliaType,
    struct_table: &StructRegistry,
    prefer_main_owner: bool,
) -> ValueType {
    match jt {
        JuliaType::Bottom => ValueType::Union(Vec::new()),
        JuliaType::Union(types) => ValueType::Union(
            types
                .iter()
                .map(|ty| julia_type_to_value_type_scoped(ty, struct_table, prefer_main_owner))
                .collect::<Vec<_>>(),
        ),
        JuliaType::Struct(name) => {
            if is_rng_type_name(name) {
                return ValueType::Rng;
            }
            if let Some(memory_type) = memory_struct_name_to_value_type(name) {
                return memory_type;
            }
            if let Some(type_id) = struct_table
                .resolve_scoped(name, None, prefer_main_owner)
                .map(|(_, info)| info.type_id)
            {
                return ValueType::Struct(type_id);
            }
            let base_name = if let Some(brace_idx) = name.find('{') {
                &name[..brace_idx]
            } else {
                name.as_str()
            };
            struct_table
                .resolve_scoped(base_name, None, prefer_main_owner)
                .map(|(_, info)| info.type_id)
                .map(ValueType::Struct)
                .unwrap_or(ValueType::Any)
        }
        _ => julia_type_to_value_type(jt),
    }
}

/// Storage tag for a struct FIELD with the given declared Julia type.
///
/// Abstract numeric annotations must not force a concrete unboxed
/// representation: `struct S; x::Number end; S(1)` stores the original
/// `Int64` upstream (no conversion happens because `1 isa Number`), so the
/// field's static tag has to stay `Any` (boxed). Mapping `Number`/`Real`/
/// `AbstractFloat` to `ValueType::F64` (and `Integer`/`Signed`/`Unsigned`
/// to `I64`) made downstream consumers reinterpret the stored value —
/// `println(S(1).x)` printed `1.0` via the F64-specialized print
/// instruction and function-local slot stores converted the value
/// (Issue #11407).
pub(super) fn field_declared_value_type_scoped(
    jt: &JuliaType,
    struct_table: &StructRegistry,
    prefer_main_owner: bool,
) -> ValueType {
    if abstract_numeric_annotation(jt) {
        return ValueType::Any;
    }
    julia_type_to_value_type_scoped(jt, struct_table, prefer_main_owner)
}

/// Same field-storage widening for contexts without a struct table
/// (parametric field substitution). See `field_declared_value_type_scoped`.
pub(super) fn field_declared_value_type(jt: &JuliaType) -> ValueType {
    if abstract_numeric_annotation(jt) {
        return ValueType::Any;
    }
    julia_type_to_value_type(jt)
}

fn abstract_numeric_annotation(jt: &JuliaType) -> bool {
    matches!(
        jt,
        JuliaType::Number
            | JuliaType::Real
            | JuliaType::AbstractFloat
            | JuliaType::Integer
            | JuliaType::Signed
            | JuliaType::Unsigned
    )
}

/// Compute the join (least upper bound) of two `ValueType`s at a control-flow merge point.
///
/// If both paths agree on the type, the merged type is that exact type. If they
/// disagree — or if the variable doesn't exist on one path — the merged type is
/// `Any`, because the compiler cannot statically determine which branch ran.
///
/// # Usage
///
/// Use this whenever a variable may have been assigned different types in different
/// control-flow branches (try/catch, if/else, loops):
///
/// ```rust,ignore
/// // Private API — cannot be imported externally; illustration only.
/// # use subset_julia_vm::compile::type_helpers::join_type;
/// # use subset_julia_vm_bytecode::ValueType;
/// let try_ty  = ValueType::I64;
/// let catch_ty = ValueType::Any;
/// let merged  = join_type(&try_ty, &catch_ty); // → ValueType::Any
/// let same    = join_type(&ValueType::F64, &ValueType::F64); // → ValueType::F64
/// ```
///
/// # Anti-pattern to avoid
///
/// Do NOT reset to one branch's type when both paths are present:
/// ```rust,ignore
/// // WRONG: resets to try-path type instead of widening
/// if self.locals.get(name) != Some(try_ty) {
///     self.locals.insert(name.clone(), try_ty.clone());
/// }
/// ```
///
/// This was the root cause of Issue #3044 (try/catch type widening bug).
pub(super) fn join_type(a: &ValueType, b: &ValueType) -> ValueType {
    if a == b {
        a.clone()
    } else {
        ValueType::Any
    }
}

/// Result type of a value-position `&&` / `||` whose final operand has type
/// `right_ty`. In Julia `a && b` yields `b` (when `a` is true) or `false`, and
/// `a || b` yields `true` or `b` — so one branch contributes the operand's value
/// and the other a constant `Bool`. The expression is therefore `Bool` only when
/// the operand is itself `Bool`; otherwise it widens to `Any`. Both the codegen
/// (`compile_and_expr`/`compile_or_expr`) and every binary-op type-inference path
/// must agree on this, or an inline `(a && b) == literal` comparison mis-compiles
/// against a stale `Bool` left type (Issue #6278; follow-up to #6162).
pub(super) fn short_circuit_result_type(right_ty: ValueType) -> ValueType {
    if right_ty == ValueType::Bool {
        ValueType::Bool
    } else {
        ValueType::Any
    }
}

#[cfg(test)]
mod builtin_type_name_tests {
    #[test]
    fn numeric_and_rational_are_builtin() {
        for name in [
            "Int64", "Float64", "Bool", "BigInt", "BigFloat", "UInt8", "Rational",
        ] {
            assert!(
                super::is_builtin_type_name(name),
                "{name} should be builtin"
            );
        }
    }

    #[test]
    fn user_types_are_not_builtin() {
        for name in ["MyStruct", "Point", "Dog", "Cat"] {
            assert!(
                !super::is_builtin_type_name(name),
                "{name} should NOT be builtin"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::compile::context::StructInfo;
    use crate::runtime_types::ValueType;
    use crate::types::JuliaType;

    // ── is_builtin_type_name ─────────────────────────────────────────────────

    #[test]
    fn test_is_builtin_type_name_abstract() {
        for name in &["Number", "Real", "Integer", "Signed", "AbstractFloat"] {
            assert!(is_builtin_type_name(name), "{name} should be builtin");
        }
    }

    #[test]
    fn test_is_builtin_type_name_runtime_type_objects() {
        for name in &["DataType", "UnionAll", "TypeVar"] {
            assert!(is_builtin_type_name(name), "{name} should be builtin");
        }
    }

    #[test]
    fn test_is_builtin_type_name_collections() {
        for name in &[
            "Array",
            "Vector",
            "BitArray",
            "BitVector",
            "BitMatrix",
            "Dict",
            "Set",
            "Tuple",
            "NamedTuple",
        ] {
            assert!(is_builtin_type_name(name), "{name} should be builtin");
        }
    }

    #[test]
    fn test_is_builtin_type_name_union_dynamic() {
        // Union{...} is handled dynamically
        assert!(is_builtin_type_name("Union{Int64, Float64}"));
        assert!(is_builtin_type_name("Union{}"));
    }

    // ── get_builtin_type_ancestors ───────────────────────────────────────────

    #[test]
    fn test_builtin_ancestors_int64() {
        let ancestors = get_builtin_type_ancestors(&JuliaType::Int64);
        assert!(ancestors.contains(&"Int64"));
        assert!(ancestors.contains(&"Signed"));
        assert!(ancestors.contains(&"Integer"));
        assert!(ancestors.contains(&"Real"));
        assert!(ancestors.contains(&"Number"));
        assert!(ancestors.contains(&"Any"));
    }

    #[test]
    fn test_builtin_ancestors_float64() {
        let ancestors = get_builtin_type_ancestors(&JuliaType::Float64);
        assert!(ancestors.contains(&"Float64"));
        assert!(ancestors.contains(&"AbstractFloat"));
        assert!(ancestors.contains(&"Real"));
        assert!(ancestors.contains(&"Number"));
        assert!(ancestors.contains(&"Any"));
        // Float64 is NOT Signed
        assert!(!ancestors.contains(&"Signed"));
    }

    #[test]
    fn test_builtin_ancestors_bool_is_integer() {
        // Bool <: Integer <: Real in Julia
        let ancestors = get_builtin_type_ancestors(&JuliaType::Bool);
        assert!(ancestors.contains(&"Bool"));
        assert!(ancestors.contains(&"Integer"));
        assert!(ancestors.contains(&"Real"));
        assert!(ancestors.contains(&"Number"));
    }

    #[test]
    fn test_builtin_ancestors_string() {
        let ancestors = get_builtin_type_ancestors(&JuliaType::String);
        assert!(ancestors.contains(&"String"));
        assert!(ancestors.contains(&"AbstractString"));
        assert!(ancestors.contains(&"Any"));
        assert!(!ancestors.contains(&"Number"));
    }

    // ── check_type_satisfies_bound ───────────────────────────────────────────

    #[test]
    fn test_bound_any_always_satisfied() {
        assert!(check_type_satisfies_bound(&JuliaType::Int64, "Any"));
        assert!(check_type_satisfies_bound(&JuliaType::Float64, "Any"));
        assert!(check_type_satisfies_bound(&JuliaType::String, "Any"));
        assert!(check_type_satisfies_bound(&JuliaType::Any, "Any"));
    }

    #[test]
    fn test_bound_any_type_satisfies_everything() {
        // JuliaType::Any satisfies any bound (runtime check will enforce)
        assert!(check_type_satisfies_bound(&JuliaType::Any, "Integer"));
        assert!(check_type_satisfies_bound(&JuliaType::Any, "AbstractFloat"));
    }

    #[test]
    fn test_bound_concrete_satisfies_parent() {
        assert!(check_type_satisfies_bound(&JuliaType::Int64, "Signed"));
        assert!(check_type_satisfies_bound(&JuliaType::Int64, "Integer"));
        assert!(check_type_satisfies_bound(&JuliaType::Int64, "Real"));
        assert!(check_type_satisfies_bound(&JuliaType::Int64, "Number"));
    }

    #[test]
    fn test_bound_structured_builtin_families_use_core_type() {
        assert!(check_type_satisfies_bound(
            &JuliaType::Struct("Dict{String, Int64}".to_string()),
            "AbstractDict"
        ));
        assert!(check_type_satisfies_bound(
            &JuliaType::Struct("Set{Int64}".to_string()),
            "AbstractSet"
        ));
        assert!(check_type_satisfies_bound(
            &JuliaType::Struct("UnitRange{Int64}".to_string()),
            "AbstractUnitRange"
        ));
        assert!(check_type_satisfies_bound(&JuliaType::IOBuffer, "IO"));
    }

    #[test]
    fn test_bound_concrete_not_sibling_abstract() {
        // Int64 is NOT a Float, so it doesn't satisfy AbstractFloat
        assert!(!check_type_satisfies_bound(
            &JuliaType::Int64,
            "AbstractFloat"
        ));
        // Float64 is NOT Signed
        assert!(!check_type_satisfies_bound(&JuliaType::Float64, "Signed"));
    }

    // ── julia_type_to_value_type ─────────────────────────────────────────────

    #[test]
    fn test_julia_type_to_value_type_primitives() {
        assert_eq!(julia_type_to_value_type(&JuliaType::Int64), ValueType::I64);
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Float64),
            ValueType::F64
        );
        assert_eq!(julia_type_to_value_type(&JuliaType::String), ValueType::Str);
        // Bool keeps its distinct ValueType so caller-side static dispatch
        // (println/show, user `foo(::Bool)`) lands on Bool methods. The
        // runtime Value tag is still preserved through Instr::ReturnI64.
        // Issue #4376.
        assert_eq!(julia_type_to_value_type(&JuliaType::Bool), ValueType::Bool);
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Nothing),
            ValueType::Nothing
        );
    }

    #[test]
    fn test_julia_type_to_value_type_memory_struct_issue_4052() {
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Struct("Memory".to_string())),
            ValueType::Memory
        );
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Struct("Memory{Int64}".to_string())),
            ValueType::MemoryOf(ArrayElementType::I64)
        );
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Struct("Memory{Real}".to_string())),
            ValueType::MemoryOf(ArrayElementType::Abstract("Real".to_string()))
        );
    }

    #[test]
    fn test_julia_type_to_value_type_abstract_numeric() {
        // Abstract types fall back to their "natural" concrete representation
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Integer),
            ValueType::I64
        );
        assert_eq!(julia_type_to_value_type(&JuliaType::Real), ValueType::F64);
        assert_eq!(julia_type_to_value_type(&JuliaType::Number), ValueType::F64);
    }

    #[test]
    fn test_julia_type_to_value_type_preserves_union_fields_issue_4270() {
        assert_eq!(
            julia_type_to_value_type(&JuliaType::Union(vec![
                JuliaType::Int64,
                JuliaType::Nothing,
            ])),
            ValueType::Union(vec![ValueType::I64, ValueType::Nothing])
        );
    }

    fn struct_info(type_id: usize) -> StructInfo {
        StructInfo {
            type_id,
            is_mutable: false,
            fields: vec![],
            has_inner_constructor: false,
        }
    }

    #[test]
    fn test_julia_type_to_value_type_with_table_preserves_struct_union_issue_4270() {
        let mut struct_table = StructRegistry::new();
        struct_table.insert("A4270".to_string(), struct_info(10));
        struct_table.insert("B4270".to_string(), struct_info(11));

        assert_eq!(
            julia_type_to_value_type_with_table(
                &JuliaType::Union(vec![
                    JuliaType::Struct("A4270".to_string()),
                    JuliaType::Struct("B4270".to_string()),
                ]),
                &struct_table,
            ),
            ValueType::Union(vec![ValueType::Struct(10), ValueType::Struct(11)])
        );
    }

    /// The Base-origin bare-name collision of Issues #10078/#10258 resolved
    /// directly by owner rather than a parallel fallback table (#11046).
    #[test]
    fn test_julia_type_to_value_type_origin_table_prefers_base_signature_issue_10258() {
        let mut struct_table = StructRegistry::new();
        // The Base-origin struct owns the bare name first...
        struct_table.insert("Partition".to_string(), struct_info(100));
        let base_id = struct_table
            .resolve_in_owner("Main", "Partition")
            .map(|(id, _)| id)
            .expect("Base-origin Partition registered");
        // ...then a module's same-named struct clobbers the bare alias.
        struct_table.insert("M.Partition".to_string(), struct_info(200));
        assert!(struct_table.insert_alias("Partition", "M.Partition"));

        // The shadowed Base entry survived the collision, by id.
        assert_eq!(
            struct_table
                .info_by_id(base_id)
                .expect("shadowed Base entry is still in the registry")
                .type_id,
            100
        );
        // ...while the NAME now resolves to the module's struct.
        assert_eq!(
            struct_table.get("Partition").expect("bare name").type_id,
            200
        );

        let ty = JuliaType::Struct("Partition".to_string());
        assert_eq!(
            julia_type_to_value_type_scoped(&ty, &struct_table, true),
            ValueType::Struct(100)
        );
        assert_eq!(
            julia_type_to_value_type_scoped(&ty, &struct_table, false),
            ValueType::Struct(200)
        );
    }

    // ── join_type ────────────────────────────────────────────────────────────

    #[test]
    fn test_join_type_same() {
        assert_eq!(join_type(&ValueType::I64, &ValueType::I64), ValueType::I64);
        assert_eq!(join_type(&ValueType::F64, &ValueType::F64), ValueType::F64);
        assert_eq!(join_type(&ValueType::Str, &ValueType::Str), ValueType::Str);
    }

    #[test]
    fn test_join_type_different_widens_to_any() {
        assert_eq!(join_type(&ValueType::I64, &ValueType::F64), ValueType::Any);
        assert_eq!(join_type(&ValueType::Str, &ValueType::I64), ValueType::Any);
        assert_eq!(join_type(&ValueType::I64, &ValueType::Any), ValueType::Any);
    }
}
