//! Julia type hierarchy definitions.
//!
//! This module defines the core type hierarchy used for method dispatch,
//! including `Variance`, `JuliaType`, and helper parsing functions.
//!
//! The hierarchy mirrors Julia's type tree:
//! ```text
//! Any
//!  ├── Number
//!  │    ├── Real
//!  │    │    ├── Integer
//!  │    │    │    ├── Signed
//!  │    │    │    │    ├── Int8, Int16, Int32, Int64, Int128 (concrete)
//!  │    │    │    │    └── BigInt (concrete)
//!  │    │    │    └── Unsigned
//!  │    │    │         └── UInt8, UInt16, UInt32, UInt64, UInt128 (concrete)
//!  │    │    └── AbstractFloat
//!  │    │         └── Float16, Float32, Float64, BigFloat (concrete)
//!  ├── AbstractString
//!  │    └── String (concrete)
//!  └── AbstractArray
//!       └── Array (concrete)
//! ```
//!
//! Note: Complex numbers are implemented as Pure Julia structs (Complex),
//! not as a builtin type.
//!
//! # Sub-modules
//!
//! - `comparison`: Subtype checking, specificity, parametric matching
//! - `display`: Display name and fmt::Display implementation
//! - `parsing`: Type name parsing and construction

mod comparison;
mod display;
pub(crate) mod parsing;

#[cfg(test)]
pub(crate) use parsing::is_type_variable_name;

use serde::{Deserialize, Serialize};

/// Variance annotation for type parameters.
///
/// In Julia's type system:
/// - **Covariant**: Tuple is covariant - `Tuple{Int64} <: Tuple{Number}`
/// - **Invariant**: Array is invariant - `Vector{Int64}` is NOT a subtype of `Vector{Number}`
/// - **Contravariant**: Function argument types (not explicitly used in Julia)
///
/// Most user-defined types are invariant by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Variance {
    /// Covariant: T<:S implies Container{T} <: Container{S}
    /// Example: Tuple is covariant
    Covariant,
    /// Invariant: T<:S does NOT imply Container{T} <: Container{S}
    /// Example: Array, Vector are invariant
    #[default]
    Invariant,
    /// Contravariant: T<:S implies Container{S} <: Container{T}
    /// Example: Function argument types (theoretical)
    Contravariant,
}

/// Julia type representation for SubsetJuliaVM.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JuliaType {
    // Concrete types (leaf nodes in hierarchy)
    // Signed integers
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    BigInt, // Arbitrary precision integer
    // Unsigned integers
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    // Boolean (subtype of Integer in Julia)
    Bool,
    // Floating point
    Float16,
    Float32,
    Float64,
    BigFloat, // Arbitrary precision floating point
    // Note: Complex numbers are Pure Julia structs, not a builtin type
    String,
    Char, // 32-bit Unicode codepoint
    Array,
    VectorOf(Box<JuliaType>), // Parametric Vector{T} (1D array)
    MatrixOf(Box<JuliaType>), // Parametric Matrix{T} (2D array)
    Tuple,
    TupleOf(Vec<JuliaType>), // Parametric Tuple{T1, T2, ...}
    NamedTuple,
    Dict,
    Set,       // Set{Any} type
    UnitRange, // 1:10 (step = 1)
    StepRange, // 1:2:10 (arbitrary step)
    Nothing,   // The type of `nothing`
    Missing,   // The type of `missing`

    // User-defined struct types (concrete)
    Struct(std::string::String), // User-defined struct (e.g., "Point", "Vector3D")

    // Module type
    Module, // Julia module (e.g., Statistics, Base)

    // Type hierarchy (types as first-class values)
    // In Julia: DataType <: Type <: Any
    // typeof(Int64) returns DataType
    // Type{Int64} is a singleton type (subtype of Type)
    Type,     // Abstract supertype of all type objects
    DataType, // The concrete type of type objects (returned by typeof(Int64))

    // Abstract types (non-leaf nodes)
    Any,
    Number,
    Real,
    Integer,
    Signed,   // Abstract type for signed integers
    Unsigned, // Abstract type for unsigned integers
    AbstractFloat,
    AbstractString,
    AbstractChar, // Supertype of Char
    AbstractArray,
    AbstractRange,
    Function, // Abstract supertype of all functions (Function <: Any)
    IO,       // Abstract IO type for custom show methods
    IOBuffer, // Concrete IOBuffer type (subtype of IO)

    // Macro system types
    Symbol,         // Julia Symbol type (:foo)
    Expr,           // Julia Expr type (AST node)
    QuoteNode,      // QuoteNode type (quoted value)
    LineNumberNode, // LineNumberNode type (source location)
    GlobalRef,      // GlobalRef type (module + name reference)

    // Base.Pairs type (for kwargs...)
    Pairs, // Type of kwargs... splatted keyword arguments

    // Base.Generator type (for generator expressions)
    Generator, // Lazy iterator created by generator expressions

    // User-defined abstract type
    /// User-defined abstract type with name and optional parent type name.
    /// Example: `abstract type Animal end` => AbstractUser("Animal", None)
    /// Example: `abstract type Mammal <: Animal end` => AbstractUser("Mammal", Some("Animal"))
    AbstractUser(std::string::String, Option<std::string::String>),

    // Type variable from where clause
    /// Type variable with name and optional upper bound.
    /// Example: `T` (unbounded) => TypeVar("T", None)
    /// Example: `T<:Real` (bounded) => TypeVar("T", Some("Real"))
    TypeVar(std::string::String, Option<std::string::String>),

    // Bottom type (Union{})
    /// The empty union type - subtype of all types, supertype of nothing.
    /// Used by promote_rule to indicate no common type.
    Bottom,

    // Union type (Union{T1, T2, ...})
    /// A union of multiple types. A value of type Union{A, B} can be either A or B.
    /// Subtype rules:
    ///   - T <: Union{T1, T2} iff T <: T1 or T <: T2
    ///   - Union{T1, T2} <: U iff T1 <: U and T2 <: U
    /// Note: Empty union (Union{}) is represented by Bottom, not Union(vec![]).
    Union(Vec<JuliaType>),

    // Type{T} pattern for matching type objects
    /// Matches type objects (values that are types themselves).
    /// Example: `::Type{Int64}` matches the type Int64 (not values of type Int64)
    /// Used in promote_rule, convert signatures.
    TypeOf(Box<JuliaType>),

    // UnionAll type (existentially quantified type)
    /// Represents a type with a free type variable that can be instantiated.
    /// Example: `Vector{T} where T` = UnionAll("T", None, Vector{T})
    /// Example: `Array{T} where T<:Number` = UnionAll("T", Some("Number"), Array{T})
    ///
    /// In Julia, UnionAll types are used for:
    /// - Generic type definitions: `Vector{T} where T`
    /// - Type variable scoping in function signatures
    /// - Representing the "schema" of a parametric type
    ///
    /// The `var` field is the name of the bound type variable.
    /// The `bound` field is the optional upper bound for the type variable.
    /// The `body` field is the type expression that may contain the type variable.
    UnionAll {
        var: std::string::String,
        bound: Option<std::string::String>,
        body: Box<JuliaType>,
    },

    // Enum type (from @enum macro)
    /// User-defined enum type with name.
    /// Example: `@enum Color red green blue` creates JuliaType::Enum("Color")
    /// Enum values are stored as Value::Enum { type_name, value }
    Enum(std::string::String),
}

impl JuliaType {
    /// Check if `self` is a builtin primitive numeric type.
    ///
    /// Returns true for all concrete numeric types that should be handled by
    /// the builtin binary operator path rather than method dispatch. This ensures
    /// type preservation (e.g., Float32 + Bool → Float32) by routing through
    /// `compile_builtin_binary_op` which emits `DynamicToF32`/`DynamicToF16`
    /// back-conversion instructions. (Issue #2203, #2225)
    pub fn is_builtin_numeric(&self) -> bool {
        matches!(
            self,
            JuliaType::Int64
                | JuliaType::Float64
                | JuliaType::Float32
                | JuliaType::Float16
                | JuliaType::Bool
                | JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int128
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
        )
    }

    /// Check if this type is concrete (a leaf in the type hierarchy).
    pub fn is_concrete(&self) -> bool {
        matches!(
            self,
            // Signed integers
            JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int64
                | JuliaType::Int128
                | JuliaType::BigInt
                // Unsigned integers
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
                // Floating point
                | JuliaType::Float16
                | JuliaType::Float32
                | JuliaType::Float64
                | JuliaType::BigFloat
                // Other concrete types
                | JuliaType::String
                | JuliaType::Char
                | JuliaType::Array
                | JuliaType::VectorOf(_)
                | JuliaType::MatrixOf(_)
                | JuliaType::Tuple
                | JuliaType::TupleOf(_)
                | JuliaType::NamedTuple
                | JuliaType::Dict
                | JuliaType::UnitRange
                | JuliaType::StepRange
                | JuliaType::Module     // Module is concrete
                | JuliaType::DataType   // DataType is concrete
                | JuliaType::Struct(_)  // User-defined structs are concrete
                // Macro system types are concrete
                | JuliaType::Symbol
                | JuliaType::Expr
                | JuliaType::QuoteNode
                | JuliaType::LineNumberNode
                | JuliaType::GlobalRef
                | JuliaType::TypeOf(_) // Type{T} patterns are concrete
                                       // Note: Bottom is not concrete - it cannot be instantiated
        )
    }

    /// Check if this type is a concrete primitive type (numeric, bool, etc.).
    /// These are the leaf types in the numeric hierarchy where exact match dispatch
    /// should be strongly preferred. Used for Bool vs Int64 dispatch resolution.
    /// Does NOT include abstract types or struct types like Rational.
    pub fn is_concrete_primitive(&self) -> bool {
        matches!(
            self,
            // Signed integers
            JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int64
                | JuliaType::Int128
                | JuliaType::BigInt
                // Unsigned integers
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
                // Floating point
                | JuliaType::Float16
                | JuliaType::Float32
                | JuliaType::Float64
                | JuliaType::BigFloat
                // Boolean
                | JuliaType::Bool
                // String and Char
                | JuliaType::String
                | JuliaType::Char
        )
    }

    /// Check if this is a narrow integer type (not Int64) that would lose type
    /// precision if coerced to ValueType::I64 via julia_type_to_value_type.
    /// This includes Int8, Int16, Int32, Int128, all unsigned integers, and Bool.
    pub fn is_narrow_integer(&self) -> bool {
        matches!(
            self,
            JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int128
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
                | JuliaType::Bool
        )
    }

    /// Check if this is an abstract integer supertype (Integer, Signed, Unsigned)
    /// or a broader abstract numeric type (Real, Number) that could accept
    /// narrow integer values. When a parameter has one of these types, we should
    /// not coerce arguments to I64 since that would widen narrow integers.
    pub fn is_abstract_integer(&self) -> bool {
        matches!(
            self,
            JuliaType::Integer
                | JuliaType::Signed
                | JuliaType::Unsigned
                | JuliaType::Real
                | JuliaType::Number
        )
    }

    /// Check if this is an abstract numeric type that could accept any numeric value
    /// at runtime (BigInt, BigFloat, etc.). When a parameter has one of these types,
    /// binary operations must use runtime dispatch instead of hardcoded intrinsics.
    pub fn is_abstract_numeric(&self) -> bool {
        matches!(
            self,
            JuliaType::Number
                | JuliaType::Real
                | JuliaType::Integer
                | JuliaType::Signed
                | JuliaType::Unsigned
                | JuliaType::AbstractFloat
        )
    }

    /// Check if this type is a primitive/numeric type.
    /// These are types that can be reasonably matched by Any during compile-time dispatch.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            // Signed integers
            JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int64
                | JuliaType::Int128
                | JuliaType::BigInt
                // Unsigned integers
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
                // Floating point
                | JuliaType::Float16
                | JuliaType::Float32
                | JuliaType::Float64
                | JuliaType::BigFloat
                // Boolean
                | JuliaType::Bool
                // String and Char
                | JuliaType::String
                | JuliaType::Char
                // Abstract numeric types (for matching)
                | JuliaType::Number
                | JuliaType::Real
                | JuliaType::Integer
                | JuliaType::Signed
                | JuliaType::Unsigned
                | JuliaType::AbstractFloat
        )
    }

    /// Get the variance of this parametric type.
    ///
    /// In Julia:
    /// - Tuple is covariant: `Tuple{Int64} <: Tuple{Number}`
    /// - Array/Vector/Matrix are invariant: `Vector{Int64}` is NOT a subtype of `Vector{Number}`
    /// - Most user-defined types are invariant
    ///
    /// Returns `None` for non-parametric types.
    pub fn variance(&self) -> Option<Variance> {
        match self {
            // Tuple is covariant
            JuliaType::Tuple | JuliaType::TupleOf(_) => Some(Variance::Covariant),
            // Array types are invariant
            JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => {
                Some(Variance::Invariant)
            }
            // Dict and Set are invariant
            JuliaType::Dict | JuliaType::Set => Some(Variance::Invariant),
            // User-defined structs are invariant by default
            JuliaType::Struct(_) => Some(Variance::Invariant),
            // Non-parametric types don't have variance
            _ => None,
        }
    }

    /// Substitute a type variable with a concrete type.
    ///
    /// This is used to instantiate UnionAll types by replacing type variables
    /// with specific types. Note that when substituting in a UnionAll, if the
    /// variable name matches the UnionAll's bound variable, the UnionAll is
    /// returned unchanged (shadowing). To instantiate a UnionAll, substitute
    /// in its body directly.
    ///
    /// # Examples
    /// ```
    /// use subset_julia_vm::types::JuliaType;
    ///
    /// // Substitute a type variable in a VectorOf type
    /// let vec_t = JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None)));
    /// let vec_int = vec_t.substitute("T", &JuliaType::Int64);
    /// assert!(matches!(vec_int, JuliaType::VectorOf(elem) if matches!(*elem, JuliaType::Int64)));
    ///
    /// // UnionAll with matching var name returns unchanged (shadowing)
    /// let union_all = JuliaType::UnionAll {
    ///     var: "T".to_string(),
    ///     bound: None,
    ///     body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None)))),
    /// };
    /// let result = union_all.substitute("T", &JuliaType::Int64);
    /// assert!(matches!(result, JuliaType::UnionAll { .. }));
    /// ```
    pub fn substitute(&self, var_name: &str, replacement: &JuliaType) -> JuliaType {
        match self {
            JuliaType::TypeVar(name, _) if name == var_name => replacement.clone(),
            JuliaType::TypeVar(_, _) => self.clone(),
            JuliaType::VectorOf(elem) => {
                JuliaType::VectorOf(Box::new(elem.substitute(var_name, replacement)))
            }
            JuliaType::MatrixOf(elem) => {
                JuliaType::MatrixOf(Box::new(elem.substitute(var_name, replacement)))
            }
            JuliaType::TupleOf(types) => JuliaType::TupleOf(
                types
                    .iter()
                    .map(|t| t.substitute(var_name, replacement))
                    .collect(),
            ),
            JuliaType::Union(types) => JuliaType::Union(
                types
                    .iter()
                    .map(|t| t.substitute(var_name, replacement))
                    .collect(),
            ),
            JuliaType::TypeOf(inner) => {
                JuliaType::TypeOf(Box::new(inner.substitute(var_name, replacement)))
            }
            JuliaType::UnionAll { var, bound, body } => {
                if var == var_name {
                    // The variable is shadowed by this UnionAll, don't substitute in body
                    self.clone()
                } else {
                    JuliaType::UnionAll {
                        var: var.clone(),
                        bound: bound.clone(),
                        body: Box::new(body.substitute(var_name, replacement)),
                    }
                }
            }
            JuliaType::Struct(name) => {
                // Check if the struct name contains the type variable
                // e.g., "Complex{T}" with var_name="T" -> "Complex{Float64}"
                if name.contains(&format!("{{{}}}", var_name)) {
                    let new_name = name.replace(
                        &format!("{{{}}}", var_name),
                        &format!("{{{}}}", replacement.name()),
                    );
                    JuliaType::Struct(new_name)
                } else if name.contains(&format!("{{{}, ", var_name)) {
                    // Multiple params: "Dict{T, S}" -> "Dict{Int64, S}"
                    let new_name = name.replace(
                        &format!("{{{}, ", var_name),
                        &format!("{{{}, ", replacement.name()),
                    );
                    JuliaType::Struct(new_name)
                } else if name.contains(&format!(", {}}}", var_name)) {
                    let new_name = name.replace(
                        &format!(", {}}}", var_name),
                        &format!(", {}}}", replacement.name()),
                    );
                    JuliaType::Struct(new_name)
                } else {
                    self.clone()
                }
            }
            // Other types don't contain type variables
            _ => self.clone(),
        }
    }

    /// Instantiate a UnionAll type with a specific type argument.
    ///
    /// For example, `Vector{T} where T` instantiated with `Int64` gives `Vector{Int64}`.
    pub fn instantiate(&self, arg: &JuliaType) -> JuliaType {
        match self {
            JuliaType::UnionAll {
                var,
                bound: _,
                body,
            } => body.substitute(var, arg),
            _ => self.clone(),
        }
    }
}
