//! Canonical registry for exact builtin type names (Issue #10954).
//!
//! Dynamic type expressions such as `Union{T...}` and `Vector{T}` remain in
//! the type parser. Every exact builtin spelling, its nominal [`JuliaType`]
//! projection, and the layers that may resolve it live here once.

use super::JuliaType;

const PARSER: u8 = 1 << 0;
const COMPILER: u8 = 1 << 1;
const REFLECTION: u8 = 1 << 2;
const PARSER_COMPILER: u8 = PARSER | COMPILER;
const COMPILER_REFLECTION: u8 = COMPILER | REFLECTION;
const ALL: u8 = PARSER | COMPILER | REFLECTION;

#[derive(Debug)]
enum JuliaTypeProjection {
    Direct(JuliaType),
    NativeInt,
    NativeUInt,
    Nominal(&'static str),
}

/// Source module that owns a builtin type binding visible to reflection.
///
/// Every source module imports Core, including `baremodule`; Base-owned types
/// are visible only through Base itself or a module that receives Base's
/// exports (Issue #11410).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTypeBindingAuthority {
    Core,
    Base,
}

impl JuliaTypeProjection {
    fn project(&self) -> JuliaType {
        match self {
            Self::Direct(ty) => ty.clone(),
            Self::NativeInt => super::native_int_julia_type(),
            Self::NativeUInt => super::native_uint_julia_type(),
            Self::Nominal(name) => JuliaType::Struct((*name).to_string()),
        }
    }
}

/// One exact builtin spelling and its checked cross-layer projections.
#[derive(Debug)]
struct BuiltinTypeSpec {
    name: &'static str,
    projection: JuliaTypeProjection,
    consumers: u8,
    binding_authority: Option<BuiltinTypeBindingAuthority>,
}

impl BuiltinTypeSpec {
    const fn new(
        name: &'static str,
        projection: JuliaTypeProjection,
        consumers: u8,
        binding_authority: Option<BuiltinTypeBindingAuthority>,
    ) -> Self {
        Self {
            name,
            projection,
            consumers,
            binding_authority,
        }
    }

    /// Exact source spelling recognized by this entry.
    #[cfg(test)]
    const fn name(&self) -> &'static str {
        self.name
    }

    /// Canonical nominal type represented by this spelling.
    fn julia_type(&self) -> JuliaType {
        self.projection.project()
    }

    /// Whether `JuliaType::from_name` accepts this exact spelling.
    #[cfg(test)]
    const fn parser_visible(&self) -> bool {
        self.consumers & PARSER != 0
    }

    /// Whether compiler value resolution treats this spelling as a type object.
    #[cfg(test)]
    const fn compiler_visible(&self) -> bool {
        self.consumers & COMPILER != 0
    }

    /// Whether Base/Core module reflection recognizes this type binding.
    #[cfg(test)]
    const fn reflection_visible(&self) -> bool {
        self.consumers & REFLECTION != 0
    }
}

macro_rules! builtin_type {
    ($name:literal, $projection:expr, $consumers:expr) => {
        BuiltinTypeSpec::new($name, $projection, $consumers, None)
    };
    ($name:literal, $projection:expr, $consumers:expr, $authority:ident) => {
        BuiltinTypeSpec::new(
            $name,
            $projection,
            $consumers,
            Some(BuiltinTypeBindingAuthority::$authority),
        )
    };
}

use JuliaTypeProjection::{Direct, NativeInt, NativeUInt, Nominal};

static BUILTIN_TYPE_SPECS: &[BuiltinTypeSpec] = &[
    // Signed integers.
    builtin_type!("Int8", Direct(JuliaType::Int8), ALL, Core),
    builtin_type!("Int16", Direct(JuliaType::Int16), ALL, Core),
    builtin_type!("Int32", Direct(JuliaType::Int32), ALL, Core),
    builtin_type!("Int64", Direct(JuliaType::Int64), ALL, Core),
    builtin_type!("Int", NativeInt, ALL, Core),
    builtin_type!("Int128", Direct(JuliaType::Int128), ALL, Core),
    builtin_type!("BigInt", Direct(JuliaType::BigInt), ALL, Base),
    // Unsigned integers.
    builtin_type!("UInt8", Direct(JuliaType::UInt8), ALL, Core),
    builtin_type!("UInt16", Direct(JuliaType::UInt16), ALL, Core),
    builtin_type!("UInt32", Direct(JuliaType::UInt32), ALL, Core),
    builtin_type!("UInt64", Direct(JuliaType::UInt64), ALL, Core),
    builtin_type!("UInt", NativeUInt, ALL, Core),
    builtin_type!("UInt128", Direct(JuliaType::UInt128), ALL, Core),
    builtin_type!("Bool", Direct(JuliaType::Bool), ALL, Core),
    // Floating point and numeric families.
    builtin_type!("Float16", Direct(JuliaType::Float16), ALL, Core),
    builtin_type!("Float32", Direct(JuliaType::Float32), ALL, Core),
    builtin_type!("Float64", Direct(JuliaType::Float64), ALL, Core),
    builtin_type!("BigFloat", Direct(JuliaType::BigFloat), ALL, Base),
    builtin_type!("Complex", Nominal("Complex"), COMPILER_REFLECTION, Base),
    builtin_type!("ComplexF64", Nominal("Complex{Float64}"), ALL, Base),
    builtin_type!("ComplexF32", Nominal("Complex{Float32}"), ALL, Base),
    builtin_type!("Rational", Nominal("Rational"), COMPILER_REFLECTION, Base),
    // Strings and characters.
    builtin_type!("String", Direct(JuliaType::String), ALL, Core),
    builtin_type!("SubString", Nominal("SubString"), ALL, Base),
    builtin_type!(
        "AbstractString",
        Direct(JuliaType::AbstractString),
        ALL,
        Core
    ),
    builtin_type!("Char", Direct(JuliaType::Char), ALL, Core),
    builtin_type!("AbstractChar", Direct(JuliaType::AbstractChar), ALL, Core),
    // Arrays and collection families.
    builtin_type!("Array", Direct(JuliaType::Array), ALL, Core),
    builtin_type!("Vector", Nominal("Vector"), ALL, Base),
    builtin_type!("Matrix", Nominal("Matrix"), ALL, Base),
    builtin_type!("DenseArray", Nominal("DenseArray"), ALL, Core),
    builtin_type!("DenseVector", Nominal("DenseVector"), ALL, Base),
    builtin_type!("DenseMatrix", Nominal("DenseMatrix"), ALL, Base),
    builtin_type!("BitArray", Nominal("BitArray"), ALL, Base),
    builtin_type!("BitVector", Nominal("BitVector"), ALL, Base),
    builtin_type!("BitMatrix", Nominal("BitMatrix"), ALL, Base),
    builtin_type!("AbstractArray", Direct(JuliaType::AbstractArray), ALL, Core),
    builtin_type!("AbstractVector", Nominal("AbstractVector"), ALL, Base),
    builtin_type!("AbstractMatrix", Nominal("AbstractMatrix"), ALL, Base),
    builtin_type!("Tuple", Direct(JuliaType::Tuple), ALL, Core),
    builtin_type!("NTuple", Nominal("NTuple"), PARSER_COMPILER),
    builtin_type!("NamedTuple", Direct(JuliaType::NamedTuple), ALL, Core),
    builtin_type!("Dict", Direct(JuliaType::Dict), ALL, Base),
    builtin_type!("Dictionary", Direct(JuliaType::Dict), PARSER),
    builtin_type!("Set", Direct(JuliaType::Set), ALL, Base),
    builtin_type!("Pair", Nominal("Pair"), REFLECTION, Core),
    // Ranges.
    builtin_type!("AbstractRange", Direct(JuliaType::AbstractRange), ALL, Base),
    builtin_type!("UnitRange", Direct(JuliaType::UnitRange), ALL, Base),
    builtin_type!("StepRange", Direct(JuliaType::StepRange), ALL, Base),
    builtin_type!("StepRangeLen", Nominal("StepRangeLen"), REFLECTION, Base),
    builtin_type!("LinRange", Nominal("LinRange"), REFLECTION, Base),
    // Pointer, reference, and runtime memory families. Bare Ref/Memory names
    // are first-class type objects in compiler value position.
    builtin_type!("Ptr", Nominal("Ptr"), PARSER_COMPILER),
    builtin_type!("Ref", Nominal("Ref"), COMPILER),
    builtin_type!("RefValue", Nominal("RefValue"), COMPILER),
    builtin_type!("Memory", Nominal("Memory"), COMPILER),
    builtin_type!("MemoryRef", Nominal("MemoryRef"), COMPILER),
    builtin_type!("GenericMemory", Nominal("GenericMemory"), COMPILER),
    builtin_type!("GenericMemoryRef", Nominal("GenericMemoryRef"), COMPILER),
    builtin_type!("AtomicMemory", Nominal("AtomicMemory"), COMPILER),
    // IO.
    builtin_type!("IO", Direct(JuliaType::IO), ALL, Core),
    builtin_type!("IOBuffer", Direct(JuliaType::IOBuffer), ALL, Base),
    // Type lattice and other fundamental bindings.
    builtin_type!("Any", Direct(JuliaType::Any), ALL, Core),
    builtin_type!("Nothing", Direct(JuliaType::Nothing), ALL, Core),
    builtin_type!("Missing", Direct(JuliaType::Missing), ALL, Base),
    builtin_type!("Number", Direct(JuliaType::Number), ALL, Core),
    builtin_type!("Real", Direct(JuliaType::Real), ALL, Core),
    builtin_type!("Integer", Direct(JuliaType::Integer), ALL, Core),
    builtin_type!("Signed", Direct(JuliaType::Signed), ALL, Core),
    builtin_type!("Unsigned", Direct(JuliaType::Unsigned), ALL, Core),
    builtin_type!("AbstractFloat", Direct(JuliaType::AbstractFloat), ALL, Core),
    builtin_type!("Function", Direct(JuliaType::Function), ALL, Core),
    builtin_type!("Type", Direct(JuliaType::Type), ALL, Core),
    builtin_type!("DataType", Direct(JuliaType::DataType), ALL, Core),
    builtin_type!("Union", Nominal("Union"), PARSER_COMPILER),
    builtin_type!("UnionAll", Nominal("UnionAll"), ALL, Core),
    builtin_type!("TypeVar", Nominal("TypeVar"), ALL, Core),
    builtin_type!("Module", Direct(JuliaType::Module), ALL, Core),
    // Exception families defined during upstream Core bootstrap
    // (`julia/base/boot.jl`). sjulia models these in pure Julia, but their
    // lexical owner is still Core, so every module (including `baremodule`)
    // receives the binding implicitly (Issues #11168/#11410).
    builtin_type!("Exception", Nominal("Exception"), REFLECTION, Core),
    builtin_type!(
        "ErrorException",
        Nominal("ErrorException"),
        REFLECTION,
        Core
    ),
    builtin_type!("BoundsError", Nominal("BoundsError"), REFLECTION, Core),
    builtin_type!("DivideError", Nominal("DivideError"), REFLECTION, Core),
    builtin_type!(
        "OutOfMemoryError",
        Nominal("OutOfMemoryError"),
        REFLECTION,
        Core
    ),
    builtin_type!(
        "StackOverflowError",
        Nominal("StackOverflowError"),
        REFLECTION,
        Core
    ),
    builtin_type!("UndefRefError", Nominal("UndefRefError"), REFLECTION, Core),
    builtin_type!("UndefVarError", Nominal("UndefVarError"), REFLECTION, Core),
    builtin_type!("DomainError", Nominal("DomainError"), REFLECTION, Core),
    builtin_type!("TypeError", Nominal("TypeError"), REFLECTION, Core),
    builtin_type!("InexactError", Nominal("InexactError"), REFLECTION, Core),
    builtin_type!("OverflowError", Nominal("OverflowError"), REFLECTION, Core),
    builtin_type!("ArgumentError", Nominal("ArgumentError"), REFLECTION, Core),
    builtin_type!(
        "UndefKeywordError",
        Nominal("UndefKeywordError"),
        REFLECTION,
        Core
    ),
    builtin_type!("MethodError", Nominal("MethodError"), REFLECTION, Core),
    builtin_type!(
        "AssertionError",
        Nominal("AssertionError"),
        REFLECTION,
        Core
    ),
    builtin_type!("FieldError", Nominal("FieldError"), REFLECTION, Core),
    builtin_type!(
        "WrappedException",
        Nominal("WrappedException"),
        REFLECTION,
        Core
    ),
    builtin_type!("LoadError", Nominal("LoadError"), REFLECTION, Core),
    // Regex and RNG values are runtime type objects, not parser primitives.
    builtin_type!("Regex", Nominal("Regex"), COMPILER_REFLECTION, Base),
    builtin_type!(
        "RegexMatch",
        Nominal("RegexMatch"),
        COMPILER_REFLECTION,
        Base
    ),
    builtin_type!("AbstractRNG", Nominal("AbstractRNG"), COMPILER),
    builtin_type!("TaskLocalRNG", Nominal("TaskLocalRNG"), COMPILER),
    builtin_type!("MersenneTwister", Nominal("MersenneTwister"), COMPILER),
    builtin_type!("Xoshiro", Nominal("Xoshiro"), COMPILER),
    builtin_type!("StableRNG", Nominal("StableRNG"), COMPILER),
    // Metaprogramming and reflection values.
    builtin_type!("Symbol", Direct(JuliaType::Symbol), ALL, Core),
    builtin_type!("Expr", Direct(JuliaType::Expr), ALL, Core),
    builtin_type!("QuoteNode", Direct(JuliaType::QuoteNode), ALL, Core),
    builtin_type!(
        "LineNumberNode",
        Direct(JuliaType::LineNumberNode),
        ALL,
        Core
    ),
    builtin_type!("GlobalRef", Direct(JuliaType::GlobalRef), ALL, Core),
    builtin_type!("Method", Nominal("Method"), REFLECTION, Core),
    // Display tags and canonical aliases used by internal type producers.
    builtin_type!("Generator", Direct(JuliaType::Generator), PARSER),
    builtin_type!("Base.Generator", Direct(JuliaType::Generator), PARSER),
    // Only the canonical empty-Union spelling is static. Bare `Bottom` must
    // remain an ordinary undefined binding (Issue #10304).
    builtin_type!("Union{}", Direct(JuliaType::Bottom), PARSER_COMPILER),
];

/// All exact builtin name specifications, in deterministic declaration order.
#[cfg(test)]
fn builtin_type_specs() -> &'static [BuiltinTypeSpec] {
    BUILTIN_TYPE_SPECS
}

fn builtin_type_for(name: &str, consumer: u8) -> Option<JuliaType> {
    BUILTIN_TYPE_SPECS
        .iter()
        .find(|spec| spec.name == name && spec.consumers & consumer != 0)
        .map(BuiltinTypeSpec::julia_type)
}

/// Resolve an exact builtin spelling accepted by `JuliaType::from_name`.
pub fn builtin_type_for_parser(name: &str) -> Option<JuliaType> {
    builtin_type_for(name, PARSER)
}

/// Resolve an exact builtin spelling emitted as a compiler type object.
pub fn builtin_type_for_compiler(name: &str) -> Option<JuliaType> {
    builtin_type_for(name, COMPILER)
}

/// Resolve an exact builtin spelling visible to Base/Core reflection.
pub fn builtin_type_for_reflection(name: &str) -> Option<JuliaType> {
    builtin_type_for(name, REFLECTION)
}

/// Return the upstream module that owns a reflection-visible builtin type.
/// Kept in the same exact-name registry as the parser/compiler/reflection
/// projection so namespace authority cannot drift from type recognition.
pub fn builtin_type_binding_authority(name: &str) -> Option<BuiltinTypeBindingAuthority> {
    BUILTIN_TYPE_SPECS
        .iter()
        .find(|spec| spec.name == name && spec.consumers & REFLECTION != 0)
        .and_then(|spec| spec.binding_authority)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_names_are_unique_and_every_projection_is_materialized() {
        let mut names = HashSet::new();
        for spec in builtin_type_specs() {
            assert!(
                names.insert(spec.name()),
                "duplicate builtin name: {}",
                spec.name()
            );
            let projected = spec.julia_type();
            assert!(
                !projected.name().is_empty(),
                "empty projection for {}",
                spec.name()
            );
            assert!(
                spec.parser_visible() || spec.compiler_visible() || spec.reflection_visible(),
                "{} has no consumer projection",
                spec.name()
            );
            assert_eq!(
                spec.binding_authority.is_some(),
                spec.reflection_visible(),
                "{} must declare authority exactly when reflection-visible",
                spec.name()
            );
        }
    }

    #[test]
    fn parser_consumer_projects_all_registry_rows_issue_10954() {
        for spec in builtin_type_specs()
            .iter()
            .filter(|spec| spec.parser_visible())
        {
            let expected = spec.julia_type();
            let actual = JuliaType::from_name(spec.name());
            assert_eq!(
                actual,
                Some(expected.clone()),
                "from_name({:?}) should return the registry projection {:?}, but got {:?}",
                spec.name(),
                expected,
                actual
            );
        }

        // A bare `Bottom` is NOT a recognized static type name: upstream's
        // `Base.Bottom` const is unexported (Issue #10304).
        assert_eq!(JuliaType::from_name("Bottom"), None);
    }

    #[test]
    fn representative_projection_categories_are_registry_owned_issue_10954() {
        // Concrete, abstract, parametric-family, runtime-only, and display-tag.
        assert_eq!(builtin_type_for_parser("Int64"), Some(JuliaType::Int64));
        assert_eq!(builtin_type_for_parser("Number"), Some(JuliaType::Number));
        assert_eq!(
            builtin_type_for_compiler("Vector"),
            Some(JuliaType::Struct("Vector".to_string()))
        );
        assert_eq!(
            builtin_type_for_compiler("MemoryRef"),
            Some(JuliaType::Struct("MemoryRef".to_string()))
        );
        assert_eq!(
            builtin_type_for_parser("Base.Generator"),
            Some(JuliaType::Generator)
        );
        assert_eq!(
            builtin_type_for_reflection("ComplexF64"),
            Some(JuliaType::Struct("Complex{Float64}".to_string()))
        );
        assert_eq!(
            builtin_type_binding_authority("Int"),
            Some(BuiltinTypeBindingAuthority::Core)
        );
        assert_eq!(
            builtin_type_binding_authority("Vector"),
            Some(BuiltinTypeBindingAuthority::Base)
        );
        assert_eq!(
            builtin_type_binding_authority("AbstractChar"),
            Some(BuiltinTypeBindingAuthority::Core)
        );
        assert_eq!(builtin_type_binding_authority("MemoryRef"), None);
    }

    #[test]
    fn core_boot_exception_families_have_binding_authority_11168_11410() {
        for name in [
            "Exception",
            "ErrorException",
            "BoundsError",
            "DivideError",
            "OutOfMemoryError",
            "StackOverflowError",
            "UndefRefError",
            "UndefVarError",
            "DomainError",
            "TypeError",
            "InexactError",
            "OverflowError",
            "ArgumentError",
            "UndefKeywordError",
            "MethodError",
            "AssertionError",
            "FieldError",
            "WrappedException",
            "LoadError",
        ] {
            assert_eq!(
                builtin_type_binding_authority(name),
                Some(BuiltinTypeBindingAuthority::Core),
                "{name} must remain an implicit Core binding"
            );
        }
    }

    #[test]
    fn substring_sentinel_reaches_all_three_consumers_issue_10954() {
        let expected = Some(JuliaType::Struct("SubString".to_string()));
        assert_eq!(builtin_type_for_parser("SubString"), expected);
        assert_eq!(builtin_type_for_compiler("SubString"), expected);
        assert_eq!(builtin_type_for_reflection("SubString"), expected);
    }
}
