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
// Changed to `pub mod` so downstream crates can reach `parsing::` items.
pub mod parsing;

// Changed from pub(crate) to pub (Issue #8655 — main crate accesses these via
// crate::types::unbounded_vararg_element / canonicalize_union).
pub use parsing::unbounded_vararg_element;

pub use parsing::{canonicalize_union, canonicalize_union_with_identity};

use serde::{Deserialize, Serialize};

/// Internal TypeVar name used while lowering source `<:Bound` / `>:Bound`
/// shorthand. It distinguishes source existentials from an explicit
/// identity-bearing `TypeVar(:_)` until the enclosing type is constructed.
pub const SOURCE_ANONYMOUS_TYPEVAR_NAME: &str = "__sjulia_source_anonymous_typevar";

use crate::inference_core::CoreType;

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
    Set,       // Bare Set type
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

    /// An identity-bearing TypeVar created at runtime by `TypeVar(...)`.
    /// Unlike a source-level [`TypeVar`](Self::TypeVar), two values with the
    /// same rendered name and bounds remain distinct type parameters.
    RuntimeTypeVar {
        id: u64,
        name: std::string::String,
        lower_bound: Box<JuliaType>,
        upper_bound: Box<JuliaType>,
    },

    /// A nominal parametric application whose arguments include an
    /// identity-bearing runtime TypeVar. Ordinary applications use the legacy
    /// compact variants/`Struct` spelling; this structured form prevents the
    /// runtime TypeVar id from being erased by a name round-trip.
    RuntimeParametric {
        base: std::string::String,
        params: Vec<JuliaType>,
    },

    // Bottom type (Union{})
    /// The empty union type - subtype of all types, supertype of nothing.
    /// Used by promote_rule to indicate no common type.
    Bottom,

    // Union type (Union{T1, T2, ...})
    /// A union of multiple types. A value of type Union{A, B} can be either A or B.
    /// Subtype rules:
    ///   - T <: Union{T1, T2} iff T <: T1 or T <: T2
    ///   - Union{T1, T2} <: U iff T1 <: U and T2 <: U
    ///
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
    /// The `bound` field is the optional UPPER bound for the type variable.
    /// The `lower_bound` field is the optional lower bound (`where Lower<:var<:..`
    /// or `where var>:Lower`); `None` means no lower bound (i.e. `Union{}`) (#5650).
    /// The `body` field is the type expression that may contain the type variable.
    UnionAll {
        var: std::string::String,
        // Boxed so adding the lower bound does not grow `JuliaType` (and thus the
        // inline `Value::DataType`) past its compact-size guard (#5650, #5171).
        lower_bound: Option<Box<std::string::String>>,
        bound: Option<Box<std::string::String>>,
        body: Box<JuliaType>,
    },

    /// A `UnionAll` constructed from a runtime `TypeVar` value. The binder's
    /// object identity is semantic, so same-name nested binders remain
    /// alpha-distinct and reflection can return the original object.
    RuntimeUnionAll {
        var: Box<JuliaType>,
        body: Box<JuliaType>,
    },

    // Enum type (from @enum macro)
    /// User-defined enum type with name.
    /// Example: `@enum Color red green blue` creates JuliaType::Enum("Color")
    /// Enum values are stored as Value::Enum { type_name, value }
    Enum(std::string::String),
}

impl JuliaType {
    /// Construct a parametric type from already-structured arguments without a
    /// display-string round trip. Canonical builtin projections must stay in
    /// their dedicated variants so subtype, reflection, and runtime-applied
    /// UnionAll paths observe the same semantics (Issue #10861).
    pub fn from_structured_parametric(
        base: impl Into<String>,
        params: Vec<JuliaType>,
    ) -> JuliaType {
        let base = base.into();
        match (base.as_str(), params.as_slice()) {
            ("Array", [element, JuliaType::Struct(rank)]) if rank == "1" => {
                JuliaType::VectorOf(Box::new(element.clone()))
            }
            ("Array", [element, JuliaType::Struct(rank)]) if rank == "2" => {
                JuliaType::MatrixOf(Box::new(element.clone()))
            }
            ("Vector", [element]) => JuliaType::VectorOf(Box::new(element.clone())),
            ("Matrix", [element]) => JuliaType::MatrixOf(Box::new(element.clone())),
            ("Type", [element]) => JuliaType::TypeOf(Box::new(element.clone())),
            ("Tuple", _) => JuliaType::TupleOf(params),
            _ => JuliaType::RuntimeParametric { base, params },
        }
    }

    /// Julia-compatible type equality for VM type-object comparisons.
    ///
    /// `Union` members are canonicalized by Julia's type system, so equality is
    /// independent of construction order (e.g. `Union{Int64,String}` equals
    /// `Union{String,Int64}`).
    pub fn type_eq(&self, other: &Self) -> bool {
        // A runtime-built UnionAll carries object-identity-bearing TypeVars, but
        // a fully generic wrapper still denotes its canonical builtin alias.
        // Normalize that wrapper before the general runtime-TypeVar comparison:
        // mutual subtyping intentionally preserves the explicit wrapper and
        // therefore cannot by itself recognize `Vector{X} where X == Vector`
        // (Issue #11013).
        if let Some(alias) = self.canonical_generic_unionall_alias() {
            return alias.type_eq(other);
        }
        if let Some(alias) = other.canonical_generic_unionall_alias() {
            return self.type_eq(&alias);
        }
        if self.contains_runtime_typevar() || other.contains_runtime_typevar() {
            let left = crate::inference_core::CoreType::from_julia_type_preserving_owner(self);
            let right = crate::inference_core::CoreType::from_julia_type_preserving_owner(other);
            return left.is_semantically_equal_with_compatible_nominals(&right);
        }
        if let Some(alpha) = self.semantic_alpha_projection() {
            return alpha.type_eq(other);
        }
        if let Some(alpha) = other.semantic_alpha_projection() {
            return self.type_eq(&alpha);
        }
        // Issue #5105: `rewrap_unionall(unwrap_unionall(X), X)` reconstructs a
        // generic `UnionAll { var, body }` (e.g. `Vector{T} where T`) that must
        // compare equal to the canonical builtin alias `X` (e.g. `Vector`). The
        // alias variants do not carry their wrapping `UnionAll` in the enum, so
        // normalize a fully-generic `UnionAll` back to its alias before the
        // structural match below. Runtime `===` uses a stricter identity helper.
        match (self, other) {
            (Self::Union(left), Self::Union(right)) => {
                if left.len() != right.len() {
                    return false;
                }
                let mut matched = vec![false; right.len()];
                left.iter().all(|left_ty| {
                    let Some(index) = right.iter().enumerate().find_map(|(index, right_ty)| {
                        (!matched[index] && left_ty.type_eq(right_ty)).then_some(index)
                    }) else {
                        return false;
                    };
                    matched[index] = true;
                    true
                })
            }
            (Self::VectorOf(left), Self::VectorOf(right))
            | (Self::MatrixOf(left), Self::MatrixOf(right))
            | (Self::TypeOf(left), Self::TypeOf(right)) => left.type_eq(right),
            (Self::Struct(left), Self::Struct(right)) => {
                struct_owners_compatible(left, right)
                    && struct_name_eq(strip_base_type_prefix(left), strip_base_type_prefix(right))
            }
            (Self::TupleOf(left), Self::TupleOf(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(left_ty, right_ty)| left_ty.type_eq(right_ty))
            }
            (
                Self::RuntimeParametric {
                    base: left_base,
                    params: left_params,
                },
                Self::RuntimeParametric {
                    base: right_base,
                    params: right_params,
                },
            ) => {
                struct_owners_compatible(left_base, right_base)
                    && struct_name_eq(left_base, right_base)
                    && left_params.len() == right_params.len()
                    && left_params
                        .iter()
                        .zip(right_params)
                        .all(|(left, right)| left.type_eq(right))
            }
            (
                Self::UnionAll {
                    lower_bound: None,
                    var: left_var,
                    bound: left_bound,
                    body: left_body,
                },
                Self::UnionAll {
                    lower_bound: None,
                    var: right_var,
                    bound: right_bound,
                    body: right_body,
                },
            ) => {
                left_var == right_var && left_bound == right_bound && left_body.type_eq(right_body)
            }
            _ => self == other,
        }
    }

    /// Issue #5105: recognize a fully-generic `UnionAll` produced by
    /// `rewrap_unionall(unwrap_unionall(X), X)` and map it back to the
    /// canonical builtin alias `X`.
    ///
    /// `unwrap_unionall(Vector)` peels `Vector`'s `UnionAll` layer down to the
    /// body `Vector{T}`; re-wrapping with `UnionAll(T, Vector{T})` rebuilds
    /// `Vector{T} where T`. Upstream Julia interns that back to the `Vector`
    /// alias so the round-trip is `===` identical. sjulia stores the alias as a
    /// dedicated `JuliaType` variant (`Array`/`Dict`/`Set`/parametric `Struct`)
    /// rather than as an explicit `UnionAll`, so this normalizes the explicit
    /// form to the alias for identity comparison.
    ///
    /// Returns `None` when `self` is not such a generic wrapping (e.g. a
    /// bounded `UnionAll`, or a body that is not the plain generic alias),
    /// leaving the structural comparison in `type_eq` unchanged.
    fn canonical_generic_unionall_alias(&self) -> Option<JuliaType> {
        if matches!(self, JuliaType::RuntimeUnionAll { .. }) {
            return self
                .semantic_alpha_projection()?
                .canonical_generic_unionall_alias();
        }

        let JuliaType::UnionAll {
            var,
            lower_bound: None,
            bound: None,
            body,
        } = self
        else {
            return None;
        };

        match body.as_ref() {
            // `Vector{T} where T` ≡ `Vector`. Keep this legacy shape accepted
            // because older lowering paths and tests can still construct it
            // directly, even though the runtime UnionAll body now uses
            // `Array{T,1}` for upstream parity (Issue #5593).
            JuliaType::VectorOf(inner) if is_generic_typevar(inner, var) => {
                Some(JuliaType::Struct("Vector".to_string()))
            }
            JuliaType::MatrixOf(inner) if is_generic_typevar(inner, var) => {
                Some(JuliaType::Struct("Matrix".to_string()))
            }
            JuliaType::Struct(name) if struct_name_eq(name, &format!("Array{{{var}, 1}}")) => {
                Some(JuliaType::Struct("Vector".to_string()))
            }
            JuliaType::Struct(name) if struct_name_eq(name, &format!("Array{{{var}, 2}}")) => {
                Some(JuliaType::Struct("Matrix".to_string()))
            }
            JuliaType::Struct(name) if struct_name_eq(name, &format!("DenseArray{{{var}, 1}}")) => {
                Some(JuliaType::Struct("DenseVector".to_string()))
            }
            JuliaType::Struct(name) if struct_name_eq(name, &format!("DenseArray{{{var}, 2}}")) => {
                Some(JuliaType::Struct("DenseMatrix".to_string()))
            }
            // `Set{T} where T` ≡ `Set`.
            JuliaType::Struct(name) if name == &format!("Set{{{var}}}") => Some(JuliaType::Set),
            JuliaType::Struct(name)
                if split_parametric_identity_name(name).is_some_and(|(_, args)| {
                    args.len() == 1 && args.first().is_some_and(|arg| arg == var)
                }) =>
            {
                let (base, _) = split_parametric_identity_name(name)?;
                Some(JuliaType::from_name_or_struct(&base))
            }
            // `Dict{K, V} where V where K` ≡ `Dict` and `Array{T, N} where N
            // where T` ≡ `Array`. The inner `UnionAll` over the second variable
            // must itself reduce to the generic two-parameter body. The
            // value-position `where`-expression lowering (Issue #5047) emits the
            // `Array{T, N}` body as a parametric `Struct` (not the dedicated
            // `Array` alias variant), so normalize it back here for `===` parity
            // with the builtin `Array`.
            JuliaType::UnionAll {
                lower_bound: None,
                var: inner_var,
                bound: None,
                body: inner_body,
            } => match inner_body.as_ref() {
                JuliaType::Struct(name) if name == &format!("Dict{{{var}, {inner_var}}}") => {
                    Some(JuliaType::Dict)
                }
                JuliaType::Struct(name) if name == &format!("Array{{{var}, {inner_var}}}") => {
                    Some(JuliaType::Array)
                }
                JuliaType::Struct(name) if name == &format!("DenseArray{{{var}, {inner_var}}}") => {
                    Some(JuliaType::Struct("DenseArray".to_string()))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if `self` is a builtin primitive numeric type.
    ///
    /// Returns true for all concrete numeric types that should be handled by
    /// the builtin binary operator path rather than method dispatch. This ensures
    /// type preservation (e.g., Float32 + Bool → Float32) by routing through
    /// `compile_builtin_binary_op` which emits `DynamicToF32`/`DynamicToF16`
    /// back-conversion instructions. (Issue #2203, #2225)
    pub fn is_builtin_numeric(&self) -> bool {
        crate::inference_core::CoreType::from(self).is_primitive_numeric()
    }

    /// Check if this type is concrete (a leaf in the type hierarchy).
    pub fn is_concrete(&self) -> bool {
        CoreType::from(self).is_concrete_type()
    }

    /// Check if this type is a concrete primitive type (numeric, bool, etc.).
    /// These are the leaf types in the numeric hierarchy where exact match dispatch
    /// should be strongly preferred. Used for Bool vs Int64 dispatch resolution.
    /// Does NOT include abstract types or struct types like Rational.
    pub fn is_concrete_primitive(&self) -> bool {
        CoreType::from(self).is_builtin_dispatch_primitive()
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
        CoreType::from(self).is_builtin_abstract_integer_accepting()
    }

    /// Check if this is an abstract container type (`AbstractArray` / `AbstractRange`).
    /// `julia_type_to_value_type` maps these to a single concrete `ValueType`
    /// (`Array` / `Range`), but their concrete subtypes have heterogeneous VM
    /// representations — e.g. a `OneTo` value is a `StructRef`, not a native
    /// `Value::Range`, and a `SubArray` is a struct, not a native array carrier.
    /// When a parameter has one of these abstract types, an argument must be
    /// compiled as-is (like `Any`), never coerced to the concrete `ValueType`,
    /// because there is no value-level conversion (and none is needed — the
    /// method body dispatches polymorphically). Issue #5842.
    pub fn is_abstract_container(&self) -> bool {
        matches!(self, JuliaType::AbstractArray | JuliaType::AbstractRange)
    }

    /// Abstract annotations whose concrete subtype values may be user-declared
    /// STRUCTS (`struct S <: IO / Function / AbstractString / AbstractChar`,
    /// Issue #8560) rather than the native representation
    /// `julia_type_to_value_type` maps the annotation to (IO stream, function
    /// value, `Str`, `Char`). Call sites must compile such arguments as-is
    /// instead of coercing them into the native `ValueType` — the coercion
    /// would reject the struct at compile time even though dispatch correctly
    /// selected the method (companion to [`Self::is_abstract_container`],
    /// Issues #5842 / #6619).
    pub fn is_abstract_with_struct_subtypes(&self) -> bool {
        matches!(
            self,
            JuliaType::IO
                | JuliaType::Function
                | JuliaType::AbstractString
                | JuliaType::AbstractChar
        )
    }

    /// Check if this is an abstract numeric type that could accept any numeric value
    /// at runtime (BigInt, BigFloat, etc.). When a parameter has one of these types,
    /// binary operations must use runtime dispatch instead of hardcoded intrinsics.
    pub fn is_abstract_numeric(&self) -> bool {
        CoreType::from(self).is_builtin_abstract_numeric()
    }

    /// Check if this type is a primitive/numeric type.
    /// These are types that can be reasonably matched by Any during compile-time dispatch.
    pub fn is_primitive(&self) -> bool {
        CoreType::from(self).is_builtin_dispatch_primitive_or_abstract_numeric()
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
    /// use subset_julia_vm_types::types::JuliaType;
    ///
    /// // Substitute a type variable in a VectorOf type
    /// let vec_t = JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None)));
    /// let vec_int = vec_t.substitute("T", &JuliaType::Int64);
    /// assert!(matches!(vec_int, JuliaType::VectorOf(elem) if matches!(*elem, JuliaType::Int64)));
    ///
    /// // UnionAll with matching var name returns unchanged (shadowing)
    /// let union_all = JuliaType::UnionAll {
    ///     var: "T".to_string(),
    ///     lower_bound: None,
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
            JuliaType::RuntimeTypeVar { .. } => self.clone(),
            JuliaType::RuntimeParametric { base, params } => {
                let params = params
                    .iter()
                    .map(|param| param.substitute(var_name, replacement))
                    .collect();
                JuliaType::from_structured_parametric(base.clone(), params)
            }
            JuliaType::RuntimeUnionAll { var, body } => JuliaType::RuntimeUnionAll {
                var: var.clone(),
                body: Box::new(body.substitute(var_name, replacement)),
            },
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
            JuliaType::UnionAll {
                var,
                lower_bound,
                bound,
                body,
            } => {
                if var == var_name {
                    // The variable is shadowed by this UnionAll, don't substitute in body
                    self.clone()
                } else {
                    let substitute_bound = |rendered: &Option<Box<String>>| {
                        rendered.as_ref().map(|rendered| {
                            let resolved = JuliaType::from_name_or_struct(rendered)
                                .substitute(var_name, replacement);
                            Box::new(resolved.name().into_owned())
                        })
                    };
                    let substituted_lower = substitute_bound(lower_bound);
                    let substituted_upper = substitute_bound(bound);

                    if replacement.mentions_free_var(var) {
                        // Issue #5054: capture-avoiding substitution. The bound
                        // variable `var` appears as a free variable in `replacement`,
                        // so substituting naively into `body` would wrongly capture
                        // it under this binder. α-rename `var` to a fresh name that
                        // is free in neither `body` nor `replacement` first, mirroring
                        // upstream `inst_type_w_`'s `jl_new_typevar` rename (which is
                        // implicit there because typevars have pointer identity).
                        let fresh = JuliaType::fresh_type_var_name(var, body, replacement);
                        let fresh_var = JuliaType::TypeVar(
                            fresh.clone(),
                            substituted_upper.as_ref().map(|b| (**b).clone()),
                        );
                        let renamed_body = body.substitute(var, &fresh_var);
                        JuliaType::UnionAll {
                            lower_bound: substituted_lower,
                            var: fresh,
                            bound: substituted_upper,
                            body: Box::new(renamed_body.substitute(var_name, replacement)),
                        }
                    } else {
                        JuliaType::UnionAll {
                            lower_bound: substituted_lower,
                            var: var.clone(),
                            bound: substituted_upper,
                            body: Box::new(body.substitute(var_name, replacement)),
                        }
                    }
                }
            }
            JuliaType::Struct(name) => {
                if name == var_name {
                    replacement.clone()
                } else if struct_name_mentions_param(name, var_name) {
                    if replacement.contains_runtime_typevar() {
                        let (base, params) = parsing::parse_parametric_name(name);
                        let params = params
                            .into_iter()
                            .map(|param| {
                                if param == var_name {
                                    replacement.clone()
                                } else {
                                    JuliaType::from_name_or_struct(param)
                                        .substitute(var_name, replacement)
                                }
                            })
                            .collect();
                        // Keep an explicitly declared `Array{T, N}` body as
                        // Array while its reflected RuntimeTypeVar identity is
                        // present. Canonicalizing rank 1/2 here changes
                        // `unwrap_unionall(Vector/Matrix)` into Vector/Matrix;
                        // concrete applications still take the ordinary
                        // canonical path below.
                        return if base == "Array" {
                            JuliaType::RuntimeParametric {
                                base: base.to_string(),
                                params,
                            }
                        } else {
                            JuliaType::from_structured_parametric(base.to_string(), params)
                        };
                    }
                    let replacement_name = replacement.name();
                    JuliaType::from_name_or_struct(&substitute_struct_typevar_name(
                        name,
                        var_name,
                        replacement_name.as_ref(),
                    ))
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
    ///
    /// Substitution is capture-avoiding (Issue #5054): when `arg` mentions a name
    /// bound by an inner `UnionAll`, that inner binder is α-renamed first so the
    /// free occurrence in `arg` is not captured.
    pub fn instantiate(&self, arg: &JuliaType) -> JuliaType {
        match self {
            JuliaType::UnionAll {
                var,
                lower_bound: _,
                bound: _,
                body,
            } => {
                let instantiated = body.substitute(var, arg);
                if matches!(instantiated, JuliaType::UnionAll { .. })
                    || instantiated.contains_runtime_typevar()
                {
                    instantiated
                } else {
                    JuliaType::from_name_or_struct(instantiated.name().as_ref())
                }
            }
            JuliaType::RuntimeUnionAll { var, body } => {
                let JuliaType::RuntimeTypeVar { id, .. } = var.as_ref() else {
                    return self.clone();
                };
                body.substitute_runtime_typevar(*id, arg)
            }
            _ => self.clone(),
        }
    }

    /// Substitute one runtime TypeVar by object identity.
    pub fn substitute_runtime_typevar(&self, id: u64, replacement: &JuliaType) -> JuliaType {
        match self {
            JuliaType::RuntimeTypeVar { id: current, .. } if *current == id => replacement.clone(),
            JuliaType::RuntimeTypeVar {
                id: current,
                name,
                lower_bound,
                upper_bound,
            } => JuliaType::RuntimeTypeVar {
                id: *current,
                name: name.clone(),
                lower_bound: Box::new(lower_bound.substitute_runtime_typevar(id, replacement)),
                upper_bound: Box::new(upper_bound.substitute_runtime_typevar(id, replacement)),
            },
            JuliaType::RuntimeParametric { base, params } => {
                let params: Vec<_> = params
                    .iter()
                    .map(|param| param.substitute_runtime_typevar(id, replacement))
                    .collect();
                if params.iter().any(JuliaType::contains_runtime_typevar) {
                    JuliaType::from_structured_parametric(base.clone(), params)
                } else {
                    JuliaType::from_name_or_struct(&format!(
                        "{base}{{{}}}",
                        params
                            .iter()
                            .map(|param| param.name().into_owned())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }
            }
            JuliaType::RuntimeUnionAll { var, .. } if matches!(var.as_ref(), JuliaType::RuntimeTypeVar { id: current, .. } if *current == id) => {
                self.clone()
            }
            JuliaType::RuntimeUnionAll { var, body } => JuliaType::RuntimeUnionAll {
                var: Box::new(var.substitute_runtime_typevar(id, replacement)),
                body: Box::new(body.substitute_runtime_typevar(id, replacement)),
            },
            JuliaType::VectorOf(inner) => {
                JuliaType::VectorOf(Box::new(inner.substitute_runtime_typevar(id, replacement)))
            }
            JuliaType::MatrixOf(inner) => {
                JuliaType::MatrixOf(Box::new(inner.substitute_runtime_typevar(id, replacement)))
            }
            JuliaType::TupleOf(types) => JuliaType::TupleOf(
                types
                    .iter()
                    .map(|ty| ty.substitute_runtime_typevar(id, replacement))
                    .collect(),
            ),
            JuliaType::Union(types) => JuliaType::Union(
                types
                    .iter()
                    .map(|ty| ty.substitute_runtime_typevar(id, replacement))
                    .collect(),
            ),
            JuliaType::TypeOf(inner) => {
                JuliaType::TypeOf(Box::new(inner.substitute_runtime_typevar(id, replacement)))
            }
            JuliaType::UnionAll {
                var,
                lower_bound,
                bound,
                body,
            } => JuliaType::UnionAll {
                var: var.clone(),
                lower_bound: lower_bound.clone(),
                bound: bound.clone(),
                body: Box::new(body.substitute_runtime_typevar(id, replacement)),
            },
            _ => self.clone(),
        }
    }

    /// Returns true when the type variable named `name` occurs *free* in `self`,
    /// i.e. unbound by any enclosing `UnionAll` that introduces the same name.
    ///
    /// Used by capture-avoiding [`substitute`](Self::substitute) (Issue #5054)
    /// to detect when an inner `UnionAll` binder collides with a free variable
    /// carried by the replacement type.
    pub fn mentions_free_var(&self, name: &str) -> bool {
        match self {
            JuliaType::TypeVar(var_name, _) => var_name == name,
            JuliaType::RuntimeTypeVar { name: var_name, .. } => var_name == name,
            JuliaType::RuntimeParametric { params, .. } => {
                params.iter().any(|param| param.mentions_free_var(name))
            }
            JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) | JuliaType::TypeOf(elem) => {
                elem.mentions_free_var(name)
            }
            JuliaType::TupleOf(types) | JuliaType::Union(types) => {
                types.iter().any(|t| t.mentions_free_var(name))
            }
            JuliaType::UnionAll { var, body, .. } => {
                // The inner binder shadows `name`, so it is no longer free below.
                var != name && body.mentions_free_var(name)
            }
            JuliaType::RuntimeUnionAll { var, body } => {
                var.mentions_free_var(name) || body.mentions_free_var(name)
            }
            // Parametric nominal spellings encode their arguments by name in the
            // brace form (e.g. "Dict{K, V}" or a user abstract
            // "AbsM{2, 2, T}"). Scan the comma-separated parameter list for a
            // whole-token occurrence of `name`.
            JuliaType::Struct(struct_name) => struct_name_mentions_param(struct_name, name),
            JuliaType::AbstractUser(abstract_name, parent) => {
                struct_name_mentions_param(abstract_name, name)
                    || parent
                        .as_deref()
                        .is_some_and(|parent| struct_name_mentions_param(parent, name))
            }
            _ => false,
        }
    }

    pub fn contains_runtime_typevar(&self) -> bool {
        match self {
            JuliaType::RuntimeTypeVar { .. } => true,
            JuliaType::RuntimeParametric { params, .. } => {
                params.iter().any(JuliaType::contains_runtime_typevar)
            }
            JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
                inner.contains_runtime_typevar()
            }
            JuliaType::TupleOf(types) | JuliaType::Union(types) => {
                types.iter().any(JuliaType::contains_runtime_typevar)
            }
            JuliaType::UnionAll { body, .. } => body.contains_runtime_typevar(),
            JuliaType::RuntimeUnionAll { .. } => true,
            _ => false,
        }
    }

    /// Whether this type contains an existential wrapper that cannot survive
    /// display-name rendering and reparsing without losing binder semantics.
    pub fn contains_unionall(&self) -> bool {
        match self {
            JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. } => true,
            JuliaType::RuntimeParametric { params, .. } => {
                params.iter().any(JuliaType::contains_unionall)
            }
            JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
                inner.contains_unionall()
            }
            JuliaType::TupleOf(types) | JuliaType::Union(types) => {
                types.iter().any(JuliaType::contains_unionall)
            }
            _ => false,
        }
    }

    pub fn references_runtime_typevar(&self, id: u64) -> bool {
        match self {
            JuliaType::RuntimeTypeVar {
                id: current,
                lower_bound,
                upper_bound,
                ..
            } => {
                *current == id
                    || lower_bound.references_runtime_typevar(id)
                    || upper_bound.references_runtime_typevar(id)
            }
            JuliaType::RuntimeParametric { params, .. } => {
                params.iter().any(|ty| ty.references_runtime_typevar(id))
            }
            JuliaType::RuntimeUnionAll { var, .. } if matches!(var.as_ref(), JuliaType::RuntimeTypeVar { id: current, .. } if *current == id) => {
                false
            }
            JuliaType::RuntimeUnionAll { var, body } => {
                var.references_runtime_typevar(id) || body.references_runtime_typevar(id)
            }
            JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
                inner.references_runtime_typevar(id)
            }
            JuliaType::TupleOf(types) | JuliaType::Union(types) => {
                types.iter().any(|ty| ty.references_runtime_typevar(id))
            }
            JuliaType::UnionAll { body, .. } => body.references_runtime_typevar(id),
            _ => false,
        }
    }

    /// Project an identity-bearing runtime wrapper to an ordinary alpha-renamed
    /// `UnionAll` for display and canonical builtin-alias detection. This is a
    /// deliberately lossy surface projection: `UnionAll` stores bounds as
    /// strings, so semantic consumers must convert the original value directly
    /// to `CoreType` to retain free runtime TypeVar identity (Issue #10613).
    pub fn semantic_alpha_projection(&self) -> Option<JuliaType> {
        if !matches!(self, JuliaType::RuntimeUnionAll { .. }) {
            return match self {
                JuliaType::UnionAll {
                    var,
                    lower_bound,
                    bound,
                    body,
                } => body
                    .semantic_alpha_projection()
                    .map(|body| JuliaType::UnionAll {
                        var: var.clone(),
                        lower_bound: lower_bound.clone(),
                        bound: bound.clone(),
                        body: Box::new(body),
                    }),
                JuliaType::RuntimeParametric { base, params } => {
                    let mut changed = false;
                    let params = params
                        .iter()
                        .map(|param| {
                            param.semantic_alpha_projection().map_or_else(
                                || param.clone(),
                                |projected| {
                                    changed = true;
                                    projected
                                },
                            )
                        })
                        .collect();
                    changed.then(|| JuliaType::RuntimeParametric {
                        base: base.clone(),
                        params,
                    })
                }
                JuliaType::VectorOf(inner) => inner
                    .semantic_alpha_projection()
                    .map(|inner| JuliaType::VectorOf(Box::new(inner))),
                JuliaType::MatrixOf(inner) => inner
                    .semantic_alpha_projection()
                    .map(|inner| JuliaType::MatrixOf(Box::new(inner))),
                JuliaType::TypeOf(inner) => inner
                    .semantic_alpha_projection()
                    .map(|inner| JuliaType::TypeOf(Box::new(inner))),
                JuliaType::TupleOf(types) | JuliaType::Union(types) => {
                    let mut changed = false;
                    let projected = types
                        .iter()
                        .map(|ty| {
                            ty.semantic_alpha_projection().map_or_else(
                                || ty.clone(),
                                |projected| {
                                    changed = true;
                                    projected
                                },
                            )
                        })
                        .collect();
                    changed.then_some(match self {
                        JuliaType::TupleOf(_) => JuliaType::TupleOf(projected),
                        _ => JuliaType::Union(projected),
                    })
                }
                _ => None,
            };
        }

        let mut binders = Vec::new();
        let mut current = self;
        while let JuliaType::RuntimeUnionAll { var, body } = current {
            let JuliaType::RuntimeTypeVar { id, name, .. } = var.as_ref() else {
                return None;
            };
            binders.push((*id, name.clone(), var.as_ref().clone()));
            current = body;
        }
        if binders.is_empty() {
            return None;
        }

        let reserved: std::collections::HashSet<_> =
            binders.iter().map(|(_, name, _)| name.clone()).collect();
        fn runtime_id_occurs_in_body(ty: &JuliaType, target: u64) -> bool {
            match ty {
                JuliaType::RuntimeTypeVar { id, .. } => *id == target,
                JuliaType::RuntimeParametric { params, .. }
                | JuliaType::TupleOf(params)
                | JuliaType::Union(params) => params
                    .iter()
                    .any(|param| runtime_id_occurs_in_body(param, target)),
                JuliaType::RuntimeUnionAll { body, .. } => runtime_id_occurs_in_body(body, target),
                JuliaType::VectorOf(inner)
                | JuliaType::MatrixOf(inner)
                | JuliaType::TypeOf(inner) => runtime_id_occurs_in_body(inner, target),
                _ => false,
            }
        }
        let mut assigned = std::collections::HashSet::new();
        let aliases: Vec<_> = binders
            .iter()
            .map(|(id, name, _)| {
                let same_name_cooccurs_in_body = binders.iter().any(|(other_id, other_name, _)| {
                    other_id != id
                        && other_name == name
                        && runtime_id_occurs_in_body(current, *id)
                        && runtime_id_occurs_in_body(current, *other_id)
                });
                let alias = if !same_name_cooccurs_in_body || assigned.insert(name.clone()) {
                    name.clone()
                } else {
                    let mut suffix = 1;
                    loop {
                        let candidate = format!("{name}{suffix}");
                        if !reserved.contains(&candidate) && assigned.insert(candidate.clone()) {
                            break candidate;
                        }
                        suffix += 1;
                    }
                };
                (*id, alias)
            })
            .collect();
        let render_runtime_ids = |mut value: JuliaType| {
            for (id, alias) in &aliases {
                value =
                    value.substitute_runtime_typevar(*id, &JuliaType::TypeVar(alias.clone(), None));
            }
            value
        };

        let semantic_body = current
            .semantic_alpha_projection()
            .unwrap_or_else(|| current.clone());
        let mut rendered = render_runtime_ids(semantic_body);
        for ((_, _, binder), (_, alias)) in binders.iter().zip(&aliases).rev() {
            let JuliaType::RuntimeTypeVar {
                lower_bound,
                upper_bound,
                ..
            } = binder
            else {
                return None;
            };
            let lower = render_runtime_ids(lower_bound.as_ref().clone());
            let upper = render_runtime_ids(upper_bound.as_ref().clone());
            rendered = JuliaType::UnionAll {
                var: alias.clone(),
                lower_bound: (!matches!(lower, JuliaType::Bottom))
                    .then(|| Box::new(lower.name().into_owned())),
                bound: (!matches!(upper, JuliaType::Any))
                    .then(|| Box::new(upper.name().into_owned())),
                body: Box::new(rendered),
            };
        }
        Some(rendered)
    }

    /// Replace occurrences of one runtime TypeVar identity with a source-level
    /// binder reference while constructing `UnionAll(var, body)`.
    pub fn bind_runtime_typevar(&self, id: u64, binder_name: &str) -> JuliaType {
        match self {
            JuliaType::RuntimeTypeVar { id: current, .. } if *current == id => {
                JuliaType::TypeVar(binder_name.to_string(), None)
            }
            JuliaType::RuntimeTypeVar {
                id: current,
                name,
                lower_bound,
                upper_bound,
            } => JuliaType::RuntimeTypeVar {
                id: *current,
                name: name.clone(),
                lower_bound: Box::new(lower_bound.bind_runtime_typevar(id, binder_name)),
                upper_bound: Box::new(upper_bound.bind_runtime_typevar(id, binder_name)),
            },
            JuliaType::RuntimeParametric { base, params } => {
                let params: Vec<_> = params
                    .iter()
                    .map(|param| param.bind_runtime_typevar(id, binder_name))
                    .collect();
                if params.iter().any(JuliaType::contains_runtime_typevar) {
                    JuliaType::RuntimeParametric {
                        base: base.clone(),
                        params,
                    }
                } else {
                    JuliaType::from_name_or_struct(&format!(
                        "{base}{{{}}}",
                        params
                            .iter()
                            .map(|param| param.name().into_owned())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }
            }
            JuliaType::VectorOf(inner) => {
                JuliaType::VectorOf(Box::new(inner.bind_runtime_typevar(id, binder_name)))
            }
            JuliaType::MatrixOf(inner) => {
                JuliaType::MatrixOf(Box::new(inner.bind_runtime_typevar(id, binder_name)))
            }
            JuliaType::TupleOf(types) => JuliaType::TupleOf(
                types
                    .iter()
                    .map(|ty| ty.bind_runtime_typevar(id, binder_name))
                    .collect(),
            ),
            JuliaType::Union(types) => JuliaType::Union(
                types
                    .iter()
                    .map(|ty| ty.bind_runtime_typevar(id, binder_name))
                    .collect(),
            ),
            JuliaType::TypeOf(inner) => {
                JuliaType::TypeOf(Box::new(inner.bind_runtime_typevar(id, binder_name)))
            }
            JuliaType::UnionAll {
                var,
                lower_bound,
                bound,
                body,
            } => JuliaType::UnionAll {
                var: var.clone(),
                lower_bound: lower_bound.clone(),
                bound: bound.clone(),
                body: Box::new(body.bind_runtime_typevar(id, binder_name)),
            },
            JuliaType::RuntimeUnionAll { var, .. } if matches!(var.as_ref(), JuliaType::RuntimeTypeVar { id: current, .. } if *current == id) => {
                self.clone()
            }
            JuliaType::RuntimeUnionAll { var, body } => JuliaType::RuntimeUnionAll {
                var: Box::new(var.bind_runtime_typevar(id, binder_name)),
                body: Box::new(body.bind_runtime_typevar(id, binder_name)),
            },
            _ => self.clone(),
        }
    }

    /// Produce a type-variable name derived from `base` that does not occur free
    /// in either `body` or `replacement`, used to α-rename a colliding `UnionAll`
    /// binder during capture-avoiding substitution (Issue #5054).
    fn fresh_type_var_name(base: &str, body: &JuliaType, replacement: &JuliaType) -> String {
        // Try `base`, then `base#1`, `base#2`, ... until free in both. The `#`
        // separator never appears in a source-level type-variable name, so the
        // generated name cannot clash with a user-written variable.
        for suffix in 1.. {
            let candidate = format!("{base}#{suffix}");
            if !body.mentions_free_var(&candidate) && !replacement.mentions_free_var(&candidate) {
                return candidate;
            }
        }
        // The 1.. range is effectively unbounded; this is unreachable in practice.
        format!("{base}#fresh")
    }
}

/// Returns true when the parametric `Struct` name `struct_name` (e.g.
/// "Dict{K, V}" or "Vector{T}") references a type parameter named `param` as a
/// whole token inside its brace list. Used by [`JuliaType::mentions_free_var`]
/// (Issue #5054); a bare struct name without braces never mentions a type var.
fn struct_name_mentions_param(struct_name: &str, param: &str) -> bool {
    let Some(open) = struct_name.find('{') else {
        return false;
    };
    let Some(close) = struct_name.rfind('}') else {
        return false;
    };
    if close <= open {
        return false;
    }
    let inner = &struct_name[open + 1..close];
    // Split on the structural separators that delimit type arguments and trim
    // whitespace so "Dict{K, V}" yields tokens "K" and "V". Nested parametric
    // arguments (e.g. "Array{Vector{T}}") also split here, exposing inner names.
    inner
        .split(['{', '}', ',', ' '])
        .map(str::trim)
        .any(|token| token == param)
}

fn substitute_struct_typevar_name(struct_name: &str, param: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(struct_name.len());
    let mut token = String::new();
    let flush = |result: &mut String, token: &mut String| {
        if token == param {
            result.push_str(replacement);
        } else {
            result.push_str(token);
        }
        token.clear();
    };

    for ch in struct_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '#' {
            token.push(ch);
        } else {
            if !token.is_empty() {
                flush(&mut result, &mut token);
            }
            result.push(ch);
        }
    }
    if !token.is_empty() {
        flush(&mut result, &mut token);
    }
    result
}

/// Compare two parametric `Struct` type names for `===` identity, ignoring
/// the cosmetic whitespace that the pretty renderer inserts after commas
/// (Issue #5210). A parametric type built by binding type variables renders
/// its arguments with `", "` (e.g. `Pair{String, Int64}`), while the same type
/// written as a source literal is stored verbatim without the space
/// (`Pair{String,Int64}`). Upstream Julia treats these as the same `DataType`,
/// so identity must not depend on that spacing. Type-parameter names never
/// contain semantically meaningful ASCII spaces, so dropping all spaces yields
/// a stable canonical form for the comparison.
fn struct_name_eq(left: &str, right: &str) -> bool {
    let canon = |s: &str| -> String {
        canonical_struct_identity_name(s)
            .chars()
            .filter(|c| *c != ' ')
            .collect()
    };
    canon(left) == canon(right)
}

/// The module-qualification prefix of a struct display name, if any
/// (everything before the LAST `.` in the base-name segment preceding any
/// `{` type-parameter list). Mirrors `strip_base_type_prefix`'s own dot
/// search (Issue #11021).
fn struct_owner_prefix(name: &str) -> Option<&str> {
    let base_end = name.find('{').unwrap_or(name.len());
    let base = &name[..base_end];
    base.rfind('.').map(|dot_idx| &name[..dot_idx])
}

/// Whether two struct display names' owners are compatible for identity
/// purposes (Issue #11021). Stripping a module qualifier is only safe in one
/// direction — a BARE reference legitimately denotes the same type as a
/// QUALIFIED reference to it (Issue #8100), but two DIFFERENT modules can
/// declare same-named structs that must stay distinct. So: incompatible only
/// when BOTH sides carry a (different) owner.
///
/// `pub` (not `pub(crate)`): Issue #11076 reuses this exact rule on the
/// dispatch-matching path (`subset_julia_vm_vm/src/vm/dispatch.rs`'s
/// `type_matches`), a different crate. One canonical helper, not a
/// per-crate re-derivation of the same owner-prefix comparison.
pub fn struct_owners_compatible(left: &str, right: &str) -> bool {
    match (struct_owner_prefix(left), struct_owner_prefix(right)) {
        (Some(lo), Some(ro)) => lo == ro,
        _ => true,
    }
}

fn strip_base_type_prefix(name: &str) -> &str {
    let base_end = name.find('{').unwrap_or(name.len());
    let base = &name[..base_end];
    // Issue #4348: imported module structs can be represented by their
    // module-qualified runtime name (`M.T{...}`) while the imported type binding
    // is compiled as `T{...}`. Julia compares those DataType objects by identity.
    if let Some(dot_idx) = base.rfind('.') {
        &name[dot_idx + 1..]
    } else {
        name
    }
}

fn canonical_struct_identity_name(name: &str) -> String {
    let stripped = strip_base_type_prefix(name);
    let Some((base, args)) = split_parametric_identity_name(stripped) else {
        return stripped.to_string();
    };
    if args.len() != 1 && args.len() != 2 {
        return stripped.to_string();
    }

    match (base.as_str(), args.as_slice()) {
        ("Vector", [elem]) => format!("Array{{{elem},1}}"),
        ("Matrix", [elem]) => format!("Array{{{elem},2}}"),
        ("DenseVector", [elem]) => format!("DenseArray{{{elem},1}}"),
        ("DenseMatrix", [elem]) => format!("DenseArray{{{elem},2}}"),
        ("Array", [elem, rank]) => format!("Array{{{elem},{rank}}}"),
        ("DenseArray", [elem, rank]) => format!("DenseArray{{{elem},{rank}}}"),
        _ => stripped.to_string(),
    }
}

fn split_parametric_identity_name(name: &str) -> Option<(String, Vec<String>)> {
    let open = name.find('{')?;
    let close = name.rfind('}')?;
    if close <= open {
        return None;
    }
    let base = name[..open].trim().to_string();
    let args = split_identity_args(&name[open + 1..close])
        .into_iter()
        .map(|arg| canonical_struct_identity_name(arg.trim()))
        .collect();
    Some((base, args))
}

fn split_identity_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;

    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].trim());
    args
}

/// Issue #5105: true when `ty` is the unbounded type variable named `var`
/// (including a nominal leaf rebound by an explicit source `UnionAll`),
/// used to confirm a `UnionAll` body is the plain generic wrapping of its alias.
fn is_generic_typevar(ty: &JuliaType, var: &str) -> bool {
    match ty {
        JuliaType::TypeVar(name, None) => name == var,
        JuliaType::TypeVar(_, Some(_)) | JuliaType::RuntimeTypeVar { .. } => false,
        _ => ty.name() == var,
    }
}

#[cfg(test)]
mod type_eq_tests {
    use super::*;

    // Issue #5210: parametric Struct identity must ignore the cosmetic space
    // the pretty renderer inserts after commas, so a type built by binding type
    // variables (`Pair{String, Int64}`) is `===` to the literal form
    // (`Pair{String,Int64}`).
    #[test]
    fn struct_type_eq_ignores_comma_whitespace() {
        let spaced = JuliaType::Struct("Pair{String, Int64}".to_string());
        let tight = JuliaType::Struct("Pair{String,Int64}".to_string());
        assert!(spaced.type_eq(&tight));
        assert!(tight.type_eq(&spaced));

        // Nested parameters with mixed spacing also match.
        let nested_spaced = JuliaType::Struct("Dict{Symbol, Vector{Int64}}".to_string());
        let nested_tight = JuliaType::Struct("Dict{Symbol,Vector{Int64}}".to_string());
        assert!(nested_spaced.type_eq(&nested_tight));
    }

    #[test]
    fn struct_type_eq_still_distinguishes_different_params() {
        let a = JuliaType::Struct("Pair{String, Int64}".to_string());
        let b = JuliaType::Struct("Pair{String, Int32}".to_string());
        assert!(!a.type_eq(&b));
    }

    // Issue #5105: `rewrap_unionall(unwrap_unionall(X), X) === X`. The rebuilt
    // generic `UnionAll` must compare equal to the canonical builtin alias.
    #[test]
    fn generic_unionall_roundtrips_to_vector_alias() {
        // `Vector{T} where T` ≡ `Vector`; `Vector` itself is not identical to
        // the two-parameter `Array` UnionAll upstream (Issue #5593).
        let rewrapped = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "T".to_string(),
                None,
            )))),
        };
        let vector = JuliaType::Struct("Vector".to_string());
        assert!(rewrapped.type_eq(&vector));
        assert!(vector.type_eq(&rewrapped));
        assert!(!JuliaType::Array.type_eq(&vector));
    }

    #[test]
    fn nominal_named_source_binder_roundtrips_to_vector_alias_10613() {
        let shadowed = JuliaType::UnionAll {
            lower_bound: None,
            var: "Module".to_string(),
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::Module))),
        };
        let vector = JuliaType::Struct("Vector".to_string());

        assert!(shadowed.type_eq(&vector));
        assert!(shadowed.is_subtype_of(&vector));
        assert!(vector.is_subtype_of(&shadowed));
    }

    #[test]
    fn runtime_unionall_alias_is_independent_of_binder_spelling_11013() {
        let binder = JuliaType::RuntimeTypeVar {
            id: 11013,
            name: "T##m#123_0".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let wrapped = JuliaType::RuntimeUnionAll {
            var: Box::new(binder.clone()),
            body: Box::new(JuliaType::VectorOf(Box::new(binder))),
        };
        let vector = JuliaType::Struct("Vector".to_string());

        assert!(wrapped.type_eq(&vector));
        assert!(vector.type_eq(&wrapped));
        assert!(wrapped.is_subtype_of(&vector));
        assert!(vector.is_subtype_of(&wrapped));
        assert_eq!(wrapped.name(), "Vector");
    }

    #[test]
    fn lower_bounded_runtime_unionall_does_not_collapse_to_alias_11013() {
        let binder = JuliaType::RuntimeTypeVar {
            id: 11014,
            name: "X".to_string(),
            lower_bound: Box::new(JuliaType::Int64),
            upper_bound: Box::new(JuliaType::Any),
        };
        let wrapped = JuliaType::RuntimeUnionAll {
            var: Box::new(binder.clone()),
            body: Box::new(JuliaType::VectorOf(Box::new(binder))),
        };
        let vector = JuliaType::Struct("Vector".to_string());

        assert!(!wrapped.type_eq(&vector));
        assert!(!vector.type_eq(&wrapped));
        assert!(wrapped.is_subtype_of(&vector));
        assert!(!vector.is_subtype_of(&wrapped));
        assert_eq!(wrapped.name(), "Vector{X} where X>:Int64");
    }

    #[test]
    fn runtime_unionall_type_eq_is_alpha_equivalent_but_keeps_free_vars_rigid_10460() {
        let wrapper = |bound_id, free_id| {
            let bound = JuliaType::RuntimeTypeVar {
                id: bound_id,
                name: "T".to_string(),
                lower_bound: Box::new(JuliaType::Bottom),
                upper_bound: Box::new(JuliaType::Real),
            };
            JuliaType::RuntimeUnionAll {
                var: Box::new(bound.clone()),
                body: Box::new(JuliaType::TupleOf(vec![
                    bound,
                    JuliaType::RuntimeTypeVar {
                        id: free_id,
                        name: "T".to_string(),
                        lower_bound: Box::new(JuliaType::Bottom),
                        upper_bound: Box::new(JuliaType::String),
                    },
                ])),
            }
        };

        assert!(wrapper(1, 10).type_eq(&wrapper(2, 10)));
        assert!(!wrapper(1, 10).type_eq(&wrapper(2, 11)));
    }

    #[test]
    fn runtime_unionall_type_eq_preserves_qualified_nominal_owners_10460() {
        let bound = JuliaType::RuntimeTypeVar {
            id: 1,
            name: "Builtin".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Function),
        };
        let qualified = JuliaType::RuntimeUnionAll {
            var: Box::new(bound.clone()),
            body: Box::new(JuliaType::TupleOf(vec![
                bound.clone(),
                JuliaType::Struct("Core.Builtin".to_string()),
            ])),
        };
        let shadowed = JuliaType::RuntimeUnionAll {
            var: Box::new(bound.clone()),
            body: Box::new(JuliaType::TupleOf(vec![bound.clone(), bound])),
        };

        assert!(!qualified.type_eq(&shadowed));
        assert!(!shadowed.type_eq(&qualified));
    }

    #[test]
    fn dense_vector_identity_matches_dense_array_rank_alias() {
        let dense_vector = JuliaType::Struct("DenseVector{Int64}".to_string());
        let dense_array_rank_one = JuliaType::Struct("DenseArray{Int64, 1}".to_string());
        assert!(dense_vector.type_eq(&dense_array_rank_one));
        assert!(dense_array_rank_one.type_eq(&dense_vector));
    }

    #[test]
    fn generic_unionall_roundtrips_to_set_alias() {
        let rewrapped = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::Struct("Set{T}".to_string())),
        };
        assert!(rewrapped.type_eq(&JuliaType::Set));
        assert!(JuliaType::Set.type_eq(&rewrapped));
    }

    #[test]
    fn generic_unionall_roundtrips_to_dict_alias() {
        // `Dict{K, V} where V where K` ≡ `Dict`.
        let inner = JuliaType::UnionAll {
            lower_bound: None,
            var: "V".to_string(),
            bound: None,
            body: Box::new(JuliaType::Struct("Dict{K, V}".to_string())),
        };
        let rewrapped = JuliaType::UnionAll {
            lower_bound: None,
            var: "K".to_string(),
            bound: None,
            body: Box::new(inner),
        };
        assert!(rewrapped.type_eq(&JuliaType::Dict));
        assert!(JuliaType::Dict.type_eq(&rewrapped));
    }

    #[test]
    fn nested_unionall_display_uses_braced_where_clauses_issue_7924() {
        let inner = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: Some(Box::new("T".to_string())),
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::TypeVar(
                "S".to_string(),
                None,
            )])),
        };
        let outer = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(inner),
        };

        assert_eq!(outer.name(), "Tuple{S} where {T, S<:T}");
    }

    /// Issue #10505 (upstream `show_can_elide`): a TRAILING unbounded binder
    /// that is exactly the struct's last parameter and occurs nowhere else is
    /// elided from the printed form, leaving the remaining bounded binders —
    /// `Array{T,N} where {T<:Real, N}` prints `Array{T} where T<:Real`.
    #[test]
    fn trailing_unbounded_where_var_is_elided_issue_10505() {
        let make = |inner_bound: Option<&str>, outer_bound: Option<&str>| JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: outer_bound.map(|b| Box::new(b.to_string())),
            body: Box::new(JuliaType::UnionAll {
                lower_bound: None,
                var: "N".to_string(),
                bound: inner_bound.map(|b| Box::new(b.to_string())),
                body: Box::new(JuliaType::Struct("Array{T, N}".to_string())),
            }),
        };

        // Mixed: trailing unbounded N elides in the DISPLAY name, bounded T
        // stays; `name()` keeps the full rendering because the subtype/isa
        // engines compare through it (#10635 — see display_name's doc).
        assert_eq!(
            make(None, Some("Real")).display_name(),
            "Array{T} where T<:Real",
            "trailing unbounded binder must be elided in display_name (Issue #10505)"
        );
        assert_eq!(
            make(None, Some("Real")).name(),
            "Array{T, N} where {T<:Real, N}",
            "name() must keep the semantic full rendering (Issue #10505/#10635)"
        );
        // Innermost binder bounded: nothing elides even in display_name.
        assert_eq!(
            make(Some("Integer"), None).display_name(),
            "Array{T, N} where {T, N<:Integer}"
        );
    }

    // A bounded `UnionAll` is NOT the generic alias and must stay distinct.
    #[test]
    fn bounded_unionall_is_not_generic_alias() {
        let bounded = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: Some(Box::new("Number".to_string())),
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "T".to_string(),
                Some("Number".to_string()),
            )))),
        };
        assert!(!bounded.type_eq(&JuliaType::Array));
    }

    // A concrete element type does not collapse to the alias.
    #[test]
    fn concrete_vector_unionall_is_not_generic_alias() {
        let concrete = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::Int64))),
        };
        assert!(!concrete.type_eq(&JuliaType::Array));
    }
}

/// Issue #5054: capture-avoiding substitution / `instantiate` for `UnionAll`.
///
/// sjulia identifies type variables by name (`TypeVar(name, _)`), unlike upstream
/// Julia which uses identity-based `jl_tvar_t` pointers. Naming makes capture
/// possible: substituting a variable with a replacement that mentions a name
/// bound by an inner `UnionAll` would wrongly bind (capture) that free name.
/// These tests pin the correct, capture-avoiding behaviour.
#[cfg(test)]
mod substitute_capture_tests {
    use super::*;

    fn tvar(name: &str) -> JuliaType {
        JuliaType::TypeVar(name.to_string(), None)
    }

    // `instantiate` of the outer var must replace only the bound outer variable
    // and leave the inner `UnionAll`'s variable untouched (issue property test):
    // `(Tuple{T, S} where S){Int}` => `Tuple{Int64, S} where S`.
    #[test]
    fn instantiate_outer_does_not_touch_inner_bound_var() {
        // UnionAll T. body = (UnionAll S. Tuple{T, S})
        let inner = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: None,
            body: Box::new(JuliaType::TupleOf(vec![tvar("T"), tvar("S")])),
        };
        let outer = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(inner),
        };

        let result = outer.instantiate(&JuliaType::Int64);

        // Expect: Tuple{Int64, S} where S — inner S stays a free, S-named typevar.
        let JuliaType::UnionAll {
            lower_bound: None,
            var: result_var,
            bound: result_bound,
            body: result_body,
        } = result
        else {
            panic!("expected UnionAll, got something else");
        };
        assert_eq!(result_var, "S");
        assert_eq!(result_bound, None);
        assert_eq!(
            *result_body,
            JuliaType::TupleOf(vec![JuliaType::Int64, tvar("S")])
        );
    }

    #[test]
    fn instantiate_outer_substitutes_dependent_inner_bounds_issue_10570() {
        let upper = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                lower_bound: None,
                var: "U".to_string(),
                bound: Some(Box::new("T".to_string())),
                body: Box::new(JuliaType::Struct("Upper{T, U}".to_string())),
            }),
        };
        let lower = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                lower_bound: Some(Box::new("T".to_string())),
                var: "U".to_string(),
                bound: None,
                body: Box::new(JuliaType::Struct("Lower{T, U}".to_string())),
            }),
        };

        let JuliaType::UnionAll { bound, .. } = upper.instantiate(&JuliaType::Real) else {
            panic!("expected remaining upper-bound UnionAll");
        };
        assert_eq!(bound.as_deref().map(String::as_str), Some("Real"));

        let JuliaType::UnionAll { lower_bound, .. } = lower.instantiate(&JuliaType::Real) else {
            panic!("expected remaining lower-bound UnionAll");
        };
        assert_eq!(lower_bound.as_deref().map(String::as_str), Some("Real"));
    }

    // The crux of capture-avoidance: substituting `T` with a replacement that
    // *mentions* the inner UnionAll's bound name `S` must NOT capture that free
    // `S`. `(Tuple{T, S} where S)` with `T := S` must α-rename the inner binder
    // so the substituted free `S` and the bound one stay distinct.
    #[test]
    fn substitute_avoids_capturing_free_var_with_alpha_rename() {
        // UnionAll S. Tuple{T, S}
        let union_all = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: None,
            body: Box::new(JuliaType::TupleOf(vec![tvar("T"), tvar("S")])),
        };

        // Substitute T := S (a free variable named S).
        let result = union_all.substitute("T", &tvar("S"));

        let JuliaType::UnionAll {
            lower_bound: None,
            var: fresh,
            bound: _,
            body,
        } = result
        else {
            panic!("expected UnionAll, got something else");
        };
        // The inner binder must have been renamed away from `S` to avoid capture.
        assert_ne!(fresh, "S", "inner binder must be α-renamed to a fresh name");
        let JuliaType::TupleOf(elems) = *body else {
            panic!("expected TupleOf body");
        };
        // First element: the substituted free `S`.
        assert_eq!(elems[0], tvar("S"));
        // Second element: the (renamed) inner binder, distinct from the free `S`.
        assert_eq!(elems[1], tvar(&fresh));
    }

    // Substituting a variable that does not collide with the inner binder is a
    // plain structural replacement (no rename needed).
    #[test]
    fn substitute_without_collision_is_plain() {
        // UnionAll S. Tuple{T, S}; substitute T := Int64 (no free S).
        let union_all = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: None,
            body: Box::new(JuliaType::TupleOf(vec![tvar("T"), tvar("S")])),
        };
        let result = union_all.substitute("T", &JuliaType::Int64);
        let expected = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: None,
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::Int64, tvar("S")])),
        };
        assert_eq!(result, expected);
    }

    // Shadowing is preserved: substituting the very variable the UnionAll binds
    // leaves the body unchanged (the inner binding shadows the outer name).
    #[test]
    fn substitute_shadowed_var_is_noop_in_body() {
        let union_all = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(tvar("T")))),
        };
        let result = union_all.substitute("T", &JuliaType::Int64);
        assert_eq!(result, union_all);
    }

    #[test]
    fn substitute_runtime_typevar_preserves_declared_array_base() {
        let replacement = JuliaType::RuntimeTypeVar {
            id: 10_861,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };

        for rank in ["1", "2"] {
            let declared = JuliaType::Struct(format!("Array{{S, {rank}}}"));
            let substituted = declared.substitute("S", &replacement);

            assert_eq!(
                substituted,
                JuliaType::RuntimeParametric {
                    base: "Array".to_string(),
                    params: vec![replacement.clone(), JuliaType::Struct(rank.to_string())],
                }
            );
            assert_eq!(substituted.name(), format!("Array{{T, {rank}}}"));
        }
    }

    #[test]
    fn instantiate_nested_vector_tuple_multi_unionall_issue_5053() {
        let nested = JuliaType::UnionAll {
            lower_bound: None,
            var: "T".to_string(),
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                lower_bound: None,
                var: "U".to_string(),
                bound: None,
                body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TupleOf(vec![
                    tvar("T"),
                    tvar("U"),
                ])))),
            }),
        };

        let result = nested
            .instantiate(&JuliaType::Int64)
            .instantiate(&JuliaType::String);

        assert_eq!(
            result,
            JuliaType::VectorOf(Box::new(JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::String,
            ])))
        );
    }

    #[test]
    fn runtime_typevar_ids_rebind_through_dependent_nominal_params_10613() {
        let outer = JuliaType::RuntimeTypeVar {
            id: 1,
            name: "A".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let applied = JuliaType::RuntimeParametric {
            base: "Dependent".to_string(),
            params: vec![
                outer.clone(),
                JuliaType::RuntimeTypeVar {
                    id: 2,
                    name: "B".to_string(),
                    lower_bound: Box::new(JuliaType::Bottom),
                    upper_bound: Box::new(outer),
                },
            ],
        };

        let inner_bound = applied.bind_runtime_typevar(2, "B");
        assert!(matches!(inner_bound, JuliaType::RuntimeParametric { .. }));
        assert_eq!(
            inner_bound.bind_runtime_typevar(1, "A"),
            JuliaType::Struct("Dependent{A, B}".to_string())
        );
    }

    #[test]
    fn runtime_unionall_substitutes_same_name_binders_by_id_10613() {
        let outer = JuliaType::RuntimeTypeVar {
            id: 10,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let inner = JuliaType::RuntimeTypeVar {
            id: 11,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(outer.clone()),
        };
        let body = JuliaType::RuntimeParametric {
            base: "Pair".to_string(),
            params: vec![outer.clone(), inner.clone()],
        };
        let nested = JuliaType::RuntimeUnionAll {
            var: Box::new(outer),
            body: Box::new(JuliaType::RuntimeUnionAll {
                var: Box::new(inner),
                body: Box::new(body),
            }),
        };

        let partial = nested.instantiate(&JuliaType::Real);
        let JuliaType::RuntimeUnionAll { var, .. } = &partial else {
            panic!("outer application must leave the inner runtime binder");
        };
        let JuliaType::RuntimeTypeVar { upper_bound, .. } = var.as_ref() else {
            panic!("inner binder must remain identity-bearing");
        };
        assert_eq!(upper_bound.as_ref(), &JuliaType::Real);
        assert_eq!(
            partial.instantiate(&JuliaType::Int64),
            JuliaType::Struct("Pair{Real, Int64}".to_string())
        );
    }

    #[test]
    fn structured_tuple_and_vector_reference_runtime_typevar_id_10613() {
        let runtime = JuliaType::RuntimeTypeVar {
            id: 21,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        assert!(JuliaType::TupleOf(vec![runtime.clone(), runtime.clone()])
            .references_runtime_typevar(21));
        assert!(JuliaType::VectorOf(Box::new(runtime)).references_runtime_typevar(21));

        let outer = JuliaType::RuntimeTypeVar {
            id: 22,
            name: "Outer".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let inner = JuliaType::RuntimeTypeVar {
            id: 23,
            name: "Inner".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(outer),
        };
        assert!(inner.references_runtime_typevar(22));
    }
}
