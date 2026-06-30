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

pub(crate) use parsing::unbounded_vararg_element;

pub(crate) use parsing::canonicalize_union;

use serde::{Deserialize, Serialize};

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

    // Enum type (from @enum macro)
    /// User-defined enum type with name.
    /// Example: `@enum Color red green blue` creates JuliaType::Enum("Color")
    /// Enum values are stored as Value::Enum { type_name, value }
    Enum(std::string::String),
}

impl JuliaType {
    /// Julia-compatible type equality for VM type-object comparisons.
    ///
    /// `Union` members are canonicalized by Julia's type system, so equality is
    /// independent of construction order (e.g. `Union{Int64,String}` equals
    /// `Union{String,Int64}`).
    pub fn type_eq(&self, other: &Self) -> bool {
        // Issue #5105: `rewrap_unionall(unwrap_unionall(X), X)` reconstructs a
        // generic `UnionAll { var, body }` (e.g. `Vector{T} where T`) that must
        // compare `===` equal to the canonical builtin alias `X` (e.g.
        // `Vector` ≡ `JuliaType::Array`). The alias variants do not carry their
        // wrapping `UnionAll` in the enum, so normalize a fully-generic
        // `UnionAll` back to its alias before the structural match below.
        if let Some(alias) = self.canonical_generic_unionall_alias() {
            return alias.type_eq(other);
        }
        if let Some(alias) = other.canonical_generic_unionall_alias() {
            return self.type_eq(&alias);
        }
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
                struct_name_eq(strip_base_type_prefix(left), strip_base_type_prefix(right))
            }
            (Self::TupleOf(left), Self::TupleOf(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(left_ty, right_ty)| left_ty.type_eq(right_ty))
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
        let JuliaType::UnionAll {
            var,
            lower_bound: _,
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
            JuliaType::UnionAll {
                var, bound, body, ..
            } => {
                if var == var_name {
                    // The variable is shadowed by this UnionAll, don't substitute in body
                    self.clone()
                } else if replacement.mentions_free_var(var) {
                    // Issue #5054: capture-avoiding substitution. The bound
                    // variable `var` appears as a free variable in `replacement`,
                    // so substituting naively into `body` would wrongly capture
                    // it under this binder. α-rename `var` to a fresh name that
                    // is free in neither `body` nor `replacement` first, mirroring
                    // upstream `inst_type_w_`'s `jl_new_typevar` rename (which is
                    // implicit there because typevars have pointer identity).
                    let fresh = JuliaType::fresh_type_var_name(var, body, replacement);
                    let fresh_var =
                        JuliaType::TypeVar(fresh.clone(), bound.as_ref().map(|b| (**b).clone()));
                    let renamed_body = body.substitute(var, &fresh_var);
                    JuliaType::UnionAll {
                        lower_bound: None,
                        var: fresh,
                        bound: bound.clone(),
                        body: Box::new(renamed_body.substitute(var_name, replacement)),
                    }
                } else {
                    JuliaType::UnionAll {
                        lower_bound: None,
                        var: var.clone(),
                        bound: bound.clone(),
                        body: Box::new(body.substitute(var_name, replacement)),
                    }
                }
            }
            JuliaType::Struct(name) => {
                if name == var_name {
                    replacement.clone()
                } else if struct_name_mentions_param(name, var_name) {
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
                if matches!(instantiated, JuliaType::UnionAll { .. }) {
                    instantiated
                } else {
                    JuliaType::from_name_or_struct(instantiated.name().as_ref())
                }
            }
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
            // A parametric `Struct` encodes its arguments by name in the brace
            // form (e.g. "Dict{K, V}"). Scan the comma-separated parameter list
            // for a whole-token occurrence of `name`.
            JuliaType::Struct(struct_name) => struct_name_mentions_param(struct_name, name),
            _ => false,
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
/// (i.e. `TypeVar(var, None)` or `Struct(var)` standing in for that variable),
/// used to confirm a `UnionAll` body is the plain generic wrapping of its alias.
fn is_generic_typevar(ty: &JuliaType, var: &str) -> bool {
    match ty {
        JuliaType::TypeVar(name, None) => name == var,
        JuliaType::Struct(name) => name == var,
        _ => false,
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
}
