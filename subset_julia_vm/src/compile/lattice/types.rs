//! Lattice type system for abstract interpretation.
//!
//! This module defines the type lattice used for type inference in SubsetJuliaVM.
//! The lattice hierarchy is:
//!
//! ```text
//! Top (Any - most general)
//!   ↑
//! Conditional (control-flow sensitive types)
//!   ↑
//! Union (union of concrete types)
//!   ↑
//! Concrete (specific types like Int64, Float64, etc.)
//!   ↑
//! Const (specific constant values like Const(42), Const(true))
//!   ↑
//! Bottom (unreachable/empty set - most specific)
//! ```

use super::widening::MAX_UNION_LENGTH;
use crate::inference_core::{CoreAbstract, CorePrimitive, CoreSubtypeEngine, CoreType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A constant value known at compile time.
///
/// Used for constant propagation during type inference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstValue {
    /// Integer constant (64-bit signed)
    Int64(i64),
    /// Float constant (64-bit)
    Float64(f64),
    /// Boolean constant
    Bool(bool),
    /// String constant
    String(String),
    /// Symbol constant (for field names in NamedTuple access)
    Symbol(String),
    /// Nothing constant
    Nothing,
}

impl Eq for ConstValue {}

impl std::hash::Hash for ConstValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            ConstValue::Int64(v) => v.hash(state),
            ConstValue::Float64(v) => v.to_bits().hash(state),
            ConstValue::Bool(v) => v.hash(state),
            ConstValue::String(v) => v.hash(state),
            ConstValue::Symbol(v) => v.hash(state),
            ConstValue::Nothing => {}
        }
    }
}

impl ConstValue {
    /// Get the concrete type of this constant value.
    pub fn to_concrete_type(&self) -> ConcreteType {
        match self {
            ConstValue::Int64(_) => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConstValue::Float64(_) => {
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
            }
            ConstValue::Bool(_) => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConstValue::String(_) => ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConstValue::Symbol(_) => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
            ConstValue::Nothing => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        }
    }

    /// Try to extract an integer value from this constant.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConstValue::Int64(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract a symbol name from this constant.
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            ConstValue::Symbol(s) => Some(s),
            _ => None,
        }
    }
}

/// A type in the lattice hierarchy used for abstract interpretation.
///
/// The lattice supports type refinement through control flow and union types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LatticeType {
    /// Bottom type (empty set, unreachable code).
    /// This is the most specific type in the lattice.
    Bottom,

    /// A concrete constant value known at compile time.
    /// More specific than Concrete - represents an exact value.
    /// Example: Const(42) is more specific than Concrete(Int64)
    Const(ConstValue),

    /// A concrete Julia type.
    Concrete(ConcreteType),

    /// Union of multiple concrete types.
    /// Example: Union{Int64, Float64}
    /// Uses BTreeSet to maintain sorted order and ensure uniqueness.
    Union(BTreeSet<ConcreteType>),

    /// Conditional type for control-flow sensitive type narrowing.
    ///
    /// Used after type tests like `isa` or comparisons like `=== nothing`.
    /// The `then_type` is the type in the true branch, and `else_type` is
    /// the type in the false branch.
    Conditional {
        /// Variable slot being tested
        slot: String,
        /// Type in the then branch
        then_type: Box<LatticeType>,
        /// Type in the else branch
        else_type: Box<LatticeType>,
    },

    /// Top type (Any - accepts any value).
    /// This is the most general type in the lattice.
    Top,
}

impl Eq for LatticeType {}

impl std::hash::Hash for LatticeType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            LatticeType::Bottom | LatticeType::Top => {}
            LatticeType::Const(cv) => cv.hash(state),
            LatticeType::Concrete(ct) => ct.hash(state),
            LatticeType::Union(types) => {
                for ty in types {
                    ty.hash(state);
                }
            }
            LatticeType::Conditional {
                slot,
                then_type,
                else_type,
            } => {
                slot.hash(state);
                then_type.hash(state);
                else_type.hash(state);
            }
        }
    }
}

/// A concrete Julia type in SubsetJuliaVM.
///
/// These represent specific runtime types that values can have.
/// Implements Ord to allow use in BTreeSet for Union types.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConcreteType {
    /// Semantic types delegated to the shared, structurally-complete core
    /// (Issue #6720, Phase 6, Slice 2 / Commit B). All nullary primitive /
    /// abstract / `Any` types now live here (`Int*`, `Float*`, `Bool`,
    /// `String`, `Char`, `Symbol`, `Nothing`, `Missing`, `Number`, `Integer`,
    /// `AbstractFloat`, `IO`, `Any`); only parametric and lattice-only carrier
    /// variants remain dedicated below.
    Core(CoreType),

    // Composite types
    Array {
        element: Box<ConcreteType>,
        /// Array rank (the `N` in `Array{T,N}`). `None` means the rank is
        /// unknown and defaults to a 1-D `Vector` projection (the historical
        /// behavior); `Some(n)` pins the rank so multi-dimensional results
        /// (e.g. a 2-D comprehension `Matrix`) dispatch correctly (Issue #6817).
        ndims: Option<usize>,
    },
    Tuple {
        elements: Vec<ConcreteType>,
    },
    /// Tuple with a homogeneous variadic tail (Issue #3511).
    ///
    /// Represents Julia's `Tuple{T1, T2, ..., Tn, Vararg{Tail}}` shape — a
    /// fixed prefix followed by zero-or-more values whose element type is
    /// `tail`. Used by inference to keep call-site argtypes for varargs
    /// methods bounded in size when many homogeneous arguments are passed.
    ///
    /// Note (Issue #3511): only the inference layer currently produces or
    /// consumes this variant. Codegen / VM still see `ConcreteType::Tuple`
    /// (or fall back to the generic tuple `ValueType`).
    TupleVararg {
        /// Fixed prefix element types in declaration order.
        elements: Vec<ConcreteType>,
        /// Element type of the variadic tail (`Vararg{tail}`).
        tail: Box<ConcreteType>,
    },
    NamedTuple {
        fields: Vec<(String, ConcreteType)>,
    },
    /// Range type (e.g., 1:10, 1:2:10)
    Range {
        element: Box<ConcreteType>,
    },
    /// Dictionary type with key and value types
    Dict {
        key: Box<ConcreteType>,
        value: Box<ConcreteType>,
    },
    /// Set type with element type
    Set {
        element: Box<ConcreteType>,
    },
    /// Generator type (lazy map)
    Generator {
        element: Box<ConcreteType>,
    },
    /// Pairs type (for kwargs...)
    Pairs,

    // User-defined types
    Struct {
        name: String,
        type_id: usize,
    },

    // Callable types
    Function {
        name: String,
    },
    Closure {
        name: String,
        captures: Vec<(String, ConcreteType)>,
    },
    ComposedFunction {
        outer: Box<ConcreteType>,
        inner: Box<ConcreteType>,
    },

    // Type system types
    /// DataType - the type of types (returned by typeof)
    DataType {
        name: String,
    },
    /// Module type (e.g., Statistics, Base)
    Module {
        name: String,
    },

    // Metaprogramming types
    /// Julia Expr - AST node
    Expr,
    /// QuoteNode - wrapped quoted value
    QuoteNode,
    /// LineNumberNode - source location
    LineNumberNode,
    /// GlobalRef - reference to global variable
    GlobalRef,

    // Pattern matching types
    /// Julia Regex - compiled regular expression
    Regex,
    /// Julia RegexMatch - match result from regex matching
    RegexMatch,

    // Type unions for element types
    /// Union of concrete types (e.g., Union{Int64, Float64} for heterogeneous collections)
    /// This allows container element types to preserve Union information instead of
    /// collapsing to a representative type. (Issue #1637)
    UnionOf(Vec<ConcreteType>),

    /// Enum type (from Julia @enum macro, e.g., @enum Color Red Green Blue).
    /// Stores the enum type name. Enum values are integers internally, but
    /// treated as a distinct type in the lattice for dispatch correctness.
    /// Added to replace the `LatticeType::Top` workaround (Issue #2863).
    Enum {
        name: String,
    },
}

impl LatticeType {
    /// Construct a `LatticeType::Conditional` for the given variable slot.
    ///
    /// Mirrors Julia's `Conditional` constructor in
    /// `julia/Compiler/src/typelattice.jl`. The two branch types describe
    /// what the slot's type would be in the `then` and `else` branch of a
    /// conditional that uses this value as its predicate.
    ///
    /// If both branches collapse to the same widened type (`then == else`)
    /// the conditional is no more informative than that type, so we return
    /// the widened form directly. This keeps the lattice from spuriously
    /// producing `Conditional` values that don't actually distinguish the
    /// branches — matching Julia's "if `vtype === elsetype`, no Conditional
    /// is created" rule.
    ///
    /// Issue #3503.
    pub fn make_conditional(
        slot: impl Into<String>,
        then_type: LatticeType,
        else_type: LatticeType,
    ) -> LatticeType {
        if then_type == else_type {
            return then_type;
        }
        if matches!(then_type, LatticeType::Top) && matches!(else_type, LatticeType::Top) {
            return LatticeType::Top;
        }
        LatticeType::Conditional {
            slot: slot.into(),
            then_type: Box::new(then_type),
            else_type: Box::new(else_type),
        }
    }

    /// Drop the conditional information, keeping the join of the two
    /// branch types. Mirrors Julia's `widenconditional` in
    /// `julia/Compiler/src/typelattice.jl`. For non-`Conditional` inputs
    /// this is the identity.
    ///
    /// `widenconditional` is what `join` / `meet` fall back to when the
    /// two operands' branch information cannot be combined precisely
    /// (e.g., different slots, or one operand carries no slot at all).
    ///
    /// Issue #3503.
    pub fn widen_conditional(&self) -> LatticeType {
        match self {
            LatticeType::Conditional {
                then_type,
                else_type,
                ..
            } => then_type.join_raw(else_type),
            _ => self.clone(),
        }
    }

    /// Returns `true` when this lattice value carries `Conditional` branch
    /// information. Convenience used by inference call sites that want to
    /// strip the Conditional wrapper before performing arithmetic on the
    /// type. Issue #3503.
    pub fn is_conditional(&self) -> bool {
        matches!(self, LatticeType::Conditional { .. })
    }

    /// Returns true if this is a numeric type (Int*, UInt*, Float*).
    pub fn is_numeric(&self) -> bool {
        match self {
            LatticeType::Const(cv) => matches!(cv, ConstValue::Int64(_) | ConstValue::Float64(_)),
            LatticeType::Concrete(ct) => ct.is_numeric(),
            LatticeType::Union(types) => types.iter().all(|t| t.is_numeric()),
            _ => false,
        }
    }

    /// Returns true if this is an integer type (Int*, UInt*).
    pub fn is_integer(&self) -> bool {
        match self {
            LatticeType::Const(cv) => matches!(cv, ConstValue::Int64(_)),
            LatticeType::Concrete(ct) => ct.is_integer(),
            LatticeType::Union(types) => types.iter().all(|t| t.is_integer()),
            _ => false,
        }
    }

    /// Returns true if this is a floating-point type (Float*).
    pub fn is_float(&self) -> bool {
        match self {
            LatticeType::Const(cv) => matches!(cv, ConstValue::Float64(_)),
            LatticeType::Concrete(ct) => ct.is_float(),
            LatticeType::Union(types) => types.iter().all(|t| t.is_float()),
            _ => false,
        }
    }
}

impl ConcreteType {
    /// Maximum length of a flat (non-`TupleVararg`) tuple before inference
    /// normalizes it into a `Tuple{prefix..., Vararg{join(rest)}}` form
    /// (Issue #3511). Picked to keep call-site argtypes for varargs methods
    /// from growing unboundedly while still preserving precision for typical
    /// short calls.
    pub const TUPLE_VARARG_NORMALIZE_THRESHOLD: usize = 8;

    /// Construct a `Tuple{prefix..., Vararg{tail}}` shape, collapsing to
    /// the plain flat-tuple form when `tail` is `Bottom`-like and there is
    /// no benefit to carrying the variadic marker.
    ///
    /// `tail` here represents the element type of the trailing `Vararg{T}`;
    /// callers compute it by joining the homogeneous tail.
    pub fn tuple_with_vararg(elements: Vec<ConcreteType>, tail: ConcreteType) -> ConcreteType {
        ConcreteType::TupleVararg {
            elements,
            tail: Box::new(tail),
        }
    }

    /// Normalize a long flat tuple into `Tuple{prefix..., Vararg{join(rest)}}`
    /// when the call-site arity exceeds [`TUPLE_VARARG_NORMALIZE_THRESHOLD`].
    ///
    /// Short tuples are returned unchanged so that fixed-arity inference
    /// remains exactly as precise as before (Issue #3511).
    ///
    /// The threshold is shared with the inference engine so that both the
    /// cache key canonicalization and parameter binding apply the same
    /// rule (mirrors Julia's `most_general_argtypes` /
    /// `va_process_argtypes` behavior).
    pub fn normalize_tuple_vararg(elements: Vec<ConcreteType>) -> ConcreteType {
        if elements.len() <= Self::TUPLE_VARARG_NORMALIZE_THRESHOLD {
            return ConcreteType::Tuple { elements };
        }
        // Keep a small fixed prefix and fold the remainder into a Vararg tail.
        let prefix_len = 1; // mirror Julia: "Tuple{T, Vararg{T}}" minimal form
        let prefix: Vec<ConcreteType> = elements.iter().take(prefix_len).cloned().collect();
        let rest = &elements[prefix_len..];
        let tail = join_concrete_types(rest);
        ConcreteType::tuple_with_vararg(prefix, tail)
    }

    // ── Smart constructors (Issue #6720, Phase 6, Slice 2 / Commit A) ──
    //
    // Behaviour-preserving wrappers over the *faithful* parametric variants —
    // the ones `docs/vm/CONCRETETYPE_RETIREMENT.md` §2.1 folds into
    // `Core(CoreType)` in Commit B. Construction sites call these so the
    // representation flip changes only the helper bodies, not every caller.
    // They build today's variants unchanged (a no-op refactor); the lattice-only
    // carriers (`Function`/`Closure`/`ComposedFunction`/`Enum`) keep their
    // struct-literal form because they survive Commit B intact.

    /// `Array{element}` of rank `ndims` (`None` = unknown rank; see #6817).
    pub fn array(element: ConcreteType, ndims: Option<usize>) -> Self {
        ConcreteType::Array {
            element: Box::new(element),
            ndims,
        }
    }

    /// A user struct referenced by `name`, with the "resolve later" `type_id: 0`
    /// sentinel (Commit B resolves the id from the name via the struct table).
    pub fn struct_named(name: impl Into<String>) -> Self {
        ConcreteType::Struct {
            name: name.into(),
            type_id: 0,
        }
    }

    /// A user struct with an already-resolved compiler struct-table `type_id`.
    pub fn struct_with_id(name: impl Into<String>, type_id: usize) -> Self {
        ConcreteType::Struct {
            name: name.into(),
            type_id,
        }
    }

    /// `Tuple{elements...}` (empty = unknown element types).
    pub fn tuple(elements: Vec<ConcreteType>) -> Self {
        ConcreteType::Tuple { elements }
    }

    /// `NamedTuple{(name, type)...}` (empty = unknown fields).
    pub fn named_tuple(fields: Vec<(String, ConcreteType)>) -> Self {
        ConcreteType::NamedTuple { fields }
    }

    /// A range whose element type is `element`.
    pub fn range(element: ConcreteType) -> Self {
        ConcreteType::Range {
            element: Box::new(element),
        }
    }

    /// `Dict{key, value}`.
    pub fn dict(key: ConcreteType, value: ConcreteType) -> Self {
        ConcreteType::Dict {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// `Set{element}`.
    pub fn set(element: ConcreteType) -> Self {
        ConcreteType::Set {
            element: Box::new(element),
        }
    }

    /// A lazy `Generator` whose element type is `element`.
    pub fn generator(element: ConcreteType) -> Self {
        ConcreteType::Generator {
            element: Box::new(element),
        }
    }

    /// A `DataType` type-object named `name` (the value of `typeof(x)`).
    pub fn data_type(name: impl Into<String>) -> Self {
        ConcreteType::DataType { name: name.into() }
    }

    /// A `Module` named `name`.
    pub fn module_named(name: impl Into<String>) -> Self {
        ConcreteType::Module { name: name.into() }
    }

    /// Returns true if this is a numeric type.
    /// In Julia, Bool is a subtype of Integer and participates in numeric operations.
    pub fn is_numeric(&self) -> bool {
        match self {
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_numeric()),
            _ => concrete_is_subtype_of_core_abstract(self, CoreAbstract::Number),
        }
    }

    /// Returns true if this is an integer type.
    pub fn is_integer(&self) -> bool {
        match self {
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_integer()),
            // The lattice keeps Bool out of integer-only analyses even though
            // Julia's type hierarchy has Bool <: Integer.
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)) => false,
            _ => concrete_is_subtype_of_core_abstract(self, CoreAbstract::Integer),
        }
    }

    /// Returns true if this is a floating-point type.
    pub fn is_float(&self) -> bool {
        match self {
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_float()),
            _ => concrete_is_subtype_of_core_abstract(self, CoreAbstract::AbstractFloat),
        }
    }

    /// Returns true if this is a type system type (DataType, Module).
    pub fn is_type_value(&self) -> bool {
        match self {
            ConcreteType::DataType { .. } | ConcreteType::Module { .. } => true,
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_type_value()),
            _ => false,
        }
    }

    /// Returns true if this is a metaprogramming type (Expr, Symbol, etc.).
    pub fn is_metaprogramming(&self) -> bool {
        match self {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol))
            | ConcreteType::Expr
            | ConcreteType::QuoteNode
            | ConcreteType::LineNumberNode
            | ConcreteType::GlobalRef => true,
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_metaprogramming()),
            _ => false,
        }
    }

    /// Convert this ConcreteType to its Julia type name string.
    /// Used for integration with the centralized promotion system.
    pub fn to_type_name(&self) -> Option<String> {
        match self {
            // Signed integers
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)) => {
                Some("Int8".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)) => {
                Some("Int16".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)) => {
                Some("Int32".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)) => {
                Some("Int64".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)) => {
                Some("Int128".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)) => {
                Some("BigInt".to_string())
            }
            // Unsigned integers
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)) => {
                Some("UInt8".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)) => {
                Some("UInt16".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)) => {
                Some("UInt32".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)) => {
                Some("UInt64".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)) => {
                Some("UInt128".to_string())
            }
            // Floats
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)) => {
                Some("Float16".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)) => {
                Some("Float32".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)) => {
                Some("Float64".to_string())
            }
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)) => {
                Some("BigFloat".to_string())
            }
            // Boolean
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)) => {
                Some("Bool".to_string())
            }
            // Any
            ConcreteType::Core(CoreType::Any) => Some("Any".to_string()),
            // Abstract numeric types
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)) => {
                Some("Number".to_string())
            }
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer)) => {
                Some("Integer".to_string())
            }
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat)) => {
                Some("AbstractFloat".to_string())
            }
            // Struct types (e.g., Complex{Float64})
            ConcreteType::Struct { name, .. } => Some(name.clone()),
            // Union types (e.g., Union{Int64, Float64})
            ConcreteType::UnionOf(types) => {
                let type_names: Vec<String> =
                    types.iter().filter_map(|t| t.to_type_name()).collect();
                if type_names.len() == types.len() {
                    Some(format!("Union{{{}}}", type_names.join(", ")))
                } else {
                    None
                }
            }
            // Tuple with Vararg tail (Issue #3511): "Tuple{T1, ..., Vararg{Tail}}"
            ConcreteType::TupleVararg { elements, tail } => {
                let mut parts: Vec<String> =
                    elements.iter().filter_map(|t| t.to_type_name()).collect();
                if parts.len() != elements.len() {
                    return None;
                }
                let tail_name = tail.to_type_name()?;
                parts.push(format!("Vararg{{{}}}", tail_name));
                Some(format!("Tuple{{{}}}", parts.join(", ")))
            }
            // Other types don't have simple type names
            _ => None,
        }
    }

    /// Create a ConcreteType from a Julia type name string.
    /// Used for integration with the centralized promotion system.
    pub fn from_type_name(name: &str) -> Option<Self> {
        match name {
            // Signed integers
            "Int8" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))),
            "Int16" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int16,
            ))),
            "Int32" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32,
            ))),
            "Int" if crate::types::native_int_type_name() == "Int32" => Some(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int32),
            )),
            "Int64" | "Int" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            "Int128" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128,
            ))),
            "BigInt" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
            // Unsigned integers
            "UInt8" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8,
            ))),
            "UInt16" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt16,
            ))),
            "UInt32" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt32,
            ))),
            "UInt" if crate::types::native_uint_type_name() == "UInt32" => Some(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
            ),
            "UInt64" | "UInt" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64,
            ))),
            "UInt128" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt128,
            ))),
            // Floats
            "Float16" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float16,
            ))),
            "Float32" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            ))),
            "Float64" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            "BigFloat" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            ))),
            // Boolean
            "Bool" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            // Any
            "Any" => Some(ConcreteType::Core(CoreType::Any)),
            // Abstract numeric types
            "Number" => Some(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))),
            "Integer" => Some(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer,
            ))),
            "AbstractFloat" => Some(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::AbstractFloat,
            ))),
            // String/Char
            "String" => Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            "Char" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
            // Parametric struct types (e.g., Complex{Float64})
            name if name.contains('{') => Some(ConcreteType::Struct {
                name: name.to_string(),
                type_id: 0, // Type ID resolved later
            }),
            // Unknown types
            _ => None,
        }
    }
}

fn concrete_is_subtype_of_core_abstract(ty: &ConcreteType, abstract_ty: CoreAbstract) -> bool {
    CoreSubtypeEngine::new().is_subtype(&CoreType::from(ty), &CoreType::Abstract(abstract_ty))
}

impl From<&LatticeType> for CoreType {
    fn from(ty: &LatticeType) -> Self {
        match ty {
            LatticeType::Bottom => CoreType::Bottom,
            LatticeType::Top => CoreType::Any,
            LatticeType::Const(value) => CoreType::from(&value.to_concrete_type()),
            LatticeType::Concrete(concrete) => CoreType::from(concrete),
            LatticeType::Union(types) => {
                CoreType::Union(types.iter().map(CoreType::from).collect())
            }
            LatticeType::Conditional {
                then_type,
                else_type,
                ..
            } => CoreType::from(then_type.as_ref()).typejoin(&CoreType::from(else_type.as_ref())),
        }
    }
}

impl From<&ConcreteType> for CoreType {
    fn from(ty: &ConcreteType) -> Self {
        match ty {
            // Faithful semantic types are carried directly by the core
            // (Issue #6720, Slice 2 / Commit B: all nullary folded).
            ConcreteType::Core(c) => c.clone(),
            ConcreteType::Array { element, .. } => CoreType::Struct {
                name: "Array".to_string(),
                params: vec![CoreType::from(element.as_ref())],
            },
            ConcreteType::Tuple { elements } => {
                CoreType::Tuple(elements.iter().map(CoreType::from).collect())
            }
            ConcreteType::TupleVararg { elements, tail } => {
                let mut params: Vec<CoreType> = elements.iter().map(CoreType::from).collect();
                params.push(CoreType::Vararg(Box::new(CoreType::from(tail.as_ref()))));
                CoreType::Tuple(params)
            }
            ConcreteType::NamedTuple { fields } => CoreType::NamedTuple(
                fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), CoreType::from(ty)))
                    .collect(),
            ),
            // Preserve the element type as `AbstractRange{T}` — the same
            // structured form `CoreType::from_julia_name("AbstractRange{T}")`
            // produces — so a `Range{Int64}` that visits `CoreType` is not
            // flattened to "some range" (Issue #5916). An unknown (`Any`)
            // element keeps the bare abstract family: `AbstractRange{Any}`
            // would be a different (invariant) claim than `AbstractRange`.
            ConcreteType::Range { element } => match element.as_ref() {
                ConcreteType::Core(CoreType::Any) => {
                    CoreType::Abstract(CoreAbstract::AbstractRange)
                }
                element => CoreType::Struct {
                    name: "AbstractRange".to_string(),
                    params: vec![CoreType::from(element)],
                },
            },
            ConcreteType::Dict { key, value } => CoreType::Struct {
                name: "Dict".to_string(),
                params: vec![CoreType::from(key.as_ref()), CoreType::from(value.as_ref())],
            },
            ConcreteType::Set { element } => CoreType::Struct {
                name: "Set".to_string(),
                params: vec![CoreType::from(element.as_ref())],
            },
            ConcreteType::Generator { element } => CoreType::Struct {
                name: "Generator".to_string(),
                params: vec![CoreType::from(element.as_ref())],
            },
            ConcreteType::Pairs => CoreType::Named("Pairs".to_string()),
            ConcreteType::Struct { name, .. } => CoreType::from_julia_name(name),
            ConcreteType::Function { .. }
            | ConcreteType::Closure { .. }
            | ConcreteType::ComposedFunction { .. } => CoreType::Abstract(CoreAbstract::Function),
            ConcreteType::DataType { name } => {
                CoreType::TypeOf(Box::new(CoreType::from_julia_name(name)))
            }
            ConcreteType::Module { name } => CoreType::Module(name.clone()),
            ConcreteType::Expr => CoreType::Named("Expr".to_string()),
            ConcreteType::QuoteNode => CoreType::Named("QuoteNode".to_string()),
            ConcreteType::LineNumberNode => CoreType::Named("LineNumberNode".to_string()),
            ConcreteType::GlobalRef => CoreType::Named("GlobalRef".to_string()),
            ConcreteType::Regex => CoreType::Named("Regex".to_string()),
            ConcreteType::RegexMatch => CoreType::Named("RegexMatch".to_string()),
            ConcreteType::UnionOf(types) => {
                CoreType::Union(types.iter().map(CoreType::from).collect())
            }
            ConcreteType::Enum { name } => CoreType::Named(name.clone()),
        }
    }
}

/// The downward edge of the `CoreType` hub (Issue #6599, Phase 3).
///
/// `CoreType` is the canonical, structurally-complete type; `ConcreteType`
/// is the lattice's concrete payload and strictly *less* expressive. This is
/// the inverse of [`From<&ConcreteType> for CoreType`] on the arms
/// `ConcreteType` can represent, and is intentionally **lossy** elsewhere:
/// `Bottom` over-approximates to `Any`; type variables, `UnionAll` binders,
/// `Vararg` lengths and value parameters are dropped/widened; abstract
/// families without a concrete image collapse to `Any`; and generic struct
/// parameters are re-embedded in the struct *name* string (with `type_id`
/// `0`) since `ConcreteType::Struct` carries no structured params. Container
/// structs (`Array`/`Vector`/`Matrix`/`Dict`/`Set`/range families) map to
/// the matching parametric `ConcreteType` so the two directions agree on the
/// non-lossy shapes. `julia_type_to_concrete_type_lossy` (bridge.rs, Issue
/// #6599 Phase 3 Slice B) routes `JuliaType → ConcreteType` through this edge.
impl From<&CoreType> for ConcreteType {
    fn from(ty: &CoreType) -> Self {
        match ty {
            // `ConcreteType` has no Bottom — over-approximate to the top.
            CoreType::Bottom | CoreType::Any => ConcreteType::Core(CoreType::Any),
            // Issue #6919 (epic #5916, ValueType demotion continuation): every
            // primitive projects to its own identity, so fold the former 21-arm
            // `CorePrimitive` match into a single binding arm. This is the
            // symmetric counterpart of the upward edge's
            // `ConcreteType::Core(c) => c.clone()` fold (#6720, Slice 2). Pinned
            // by `coretype_to_concretetype_maps_every_primitive_to_identity_issue_6919`.
            CoreType::Primitive(primitive) => {
                ConcreteType::Core(CoreType::Primitive(primitive.clone()))
            }
            CoreType::Abstract(abstract_ty) => match abstract_ty {
                CoreAbstract::Number => {
                    ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))
                }
                CoreAbstract::Integer => {
                    ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer))
                }
                CoreAbstract::AbstractFloat => {
                    ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat))
                }
                CoreAbstract::Function => ConcreteType::Function {
                    name: String::new(),
                },
                CoreAbstract::IO => ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)),
                CoreAbstract::DataType | CoreAbstract::Type => ConcreteType::DataType {
                    name: "DataType".to_string(),
                },
                // Real, Signed, Unsigned, AbstractString, AbstractChar,
                // AbstractArray, AbstractVector, AbstractMatrix, DenseArray,
                // AbstractDict, AbstractSet, AbstractRange, AbstractUnitRange,
                // Builtin — no concrete image, so over-approximate to `Any`.
                _ => ConcreteType::Core(CoreType::Any),
            },
            CoreType::AbstractUser { .. } => ConcreteType::Core(CoreType::Any),
            CoreType::Struct { name, params } => match name.as_str() {
                // Bare `Array`/`Vector`/`Matrix` (empty params) keep the
                // `Array{Any}` special case (#5916); a single element param is
                // carried through. The reverse impl maps every `Array{T}` back
                // to `Struct { "Array", [T] }`.
                "Array" | "Vector" | "Matrix" => ConcreteType::Array {
                    element: Box::new(
                        params
                            .first()
                            .map_or(ConcreteType::Core(CoreType::Any), ConcreteType::from),
                    ),
                    ndims: None,
                },
                "Dict" => ConcreteType::Dict {
                    key: Box::new(
                        params
                            .first()
                            .map_or(ConcreteType::Core(CoreType::Any), ConcreteType::from),
                    ),
                    value: Box::new(
                        params
                            .get(1)
                            .map_or(ConcreteType::Core(CoreType::Any), ConcreteType::from),
                    ),
                },
                "Set" => ConcreteType::Set {
                    element: Box::new(
                        params
                            .first()
                            .map_or(ConcreteType::Core(CoreType::Any), ConcreteType::from),
                    ),
                },
                "AbstractRange" | "UnitRange" | "StepRange" => ConcreteType::Range {
                    element: Box::new(
                        params
                            .first()
                            .map_or(ConcreteType::Core(CoreType::Any), ConcreteType::from),
                    ),
                },
                // Generic struct: `ConcreteType::Struct` has no structured
                // params, so re-embed them in the (rendered, braced) name and
                // use `type_id: 0` (the reverse impl ignores `type_id`).
                _ => ConcreteType::Struct {
                    name: ty.to_julia_name(),
                    type_id: 0,
                },
            },
            CoreType::Tuple(elements) => ConcreteType::Tuple {
                elements: elements.iter().map(ConcreteType::from).collect(),
            },
            CoreType::NamedTuple(fields) => ConcreteType::NamedTuple {
                fields: fields
                    .iter()
                    .map(|(name, field_ty)| (name.clone(), ConcreteType::from(field_ty)))
                    .collect(),
            },
            CoreType::Union(members) => {
                ConcreteType::UnionOf(members.iter().map(ConcreteType::from).collect())
            }
            // `ConcreteType` has no Vararg — drop the length/wrapper and keep
            // the element type.
            CoreType::Vararg(element) => ConcreteType::from(element.as_ref()),
            CoreType::VarargLen { element, .. } => ConcreteType::from(element.as_ref()),
            // Keep the upper bound when present (its best concrete witness),
            // else widen to `Any`.
            CoreType::TypeVar(var) => var
                .upper_bound
                .as_ref()
                .map_or(ConcreteType::Core(CoreType::Any), |bound| {
                    ConcreteType::from(bound.as_ref())
                }),
            // A value parameter is not a type.
            CoreType::Value(_) => ConcreteType::Core(CoreType::Any),
            // Drop the quantifier and project the body.
            CoreType::UnionAll { body, .. } => ConcreteType::from(body.as_ref()),
            CoreType::TypeOf(inner) => ConcreteType::DataType {
                name: inner.to_julia_name(),
            },
            CoreType::Module(name) => ConcreteType::Module { name: name.clone() },
            CoreType::Named(name) => ConcreteType::Struct {
                name: name.clone(),
                type_id: 0,
            },
        }
    }
}

impl Default for LatticeType {
    /// The default lattice type is Top (Any), representing maximum uncertainty.
    fn default() -> Self {
        LatticeType::Top
    }
}

/// Join a slice of concrete types into a single representative type.
///
/// Used by [`ConcreteType::normalize_tuple_vararg`] to fold the trailing
/// segment of a long flat tuple into the `Vararg{T}` element type. Mirrors
/// the behaviour of `most_general_argtypes` in Julia's
/// `julia/Compiler/src/inferenceresult.jl`: when the tail values are
/// homogeneous we collapse to the single element type, otherwise we widen
/// to a small `UnionOf` (or to `Any` if the union would be too large).
///
/// Issue #3511.
fn join_concrete_types(types: &[ConcreteType]) -> ConcreteType {
    if types.is_empty() {
        return ConcreteType::Core(CoreType::Any);
    }
    let first = types[0].clone();
    if types.iter().all(|t| t == &first) {
        return first;
    }
    // Heterogeneous tail — collect a small UnionOf so we don't lose all
    // precision. Cap at MAX_UNION_LENGTH to avoid unbounded growth, falling
    // back to `Any` for very heterogeneous tails.
    let mut deduped: Vec<ConcreteType> = Vec::new();
    for t in types {
        if !deduped.contains(t) {
            deduped.push(t.clone());
        }
    }
    if deduped.len() > MAX_UNION_LENGTH {
        return ConcreteType::Core(CoreType::Any);
    }
    if deduped.len() == 1 {
        return deduped
            .into_iter()
            .next()
            .unwrap_or(ConcreteType::Core(CoreType::Any));
    }
    ConcreteType::UnionOf(deduped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_core::{CoreTypeVar, CoreValueParam};

    /// Issue #6720 (Phase 6, Slice 1): characterization pin for the
    /// `ConcreteType ↔ CoreType` round-trip classification that
    /// `docs/vm/CONCRETETYPE_RETIREMENT.md` §2 depends on. Slice 2 flips
    /// `ConcreteType` to `Core(CoreType)` + lattice-only carriers; this test
    /// locks **which** variants are `CoreType`-faithful (must keep round-tripping
    /// identically) vs lattice-only (the documented loss). It is a golden
    /// snapshot of current behaviour — a diff here is a deliberate signal, not a
    /// silent regression.
    #[test]
    fn concretetype_coretype_roundtrip_classification_issue_6720() {
        let rt = |ct: &ConcreteType| ConcreteType::from(&CoreType::from(ct));

        // ── §2.1 faithful: round-trip is the identity ────────────────────────
        let faithful = vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing)),
            ConcreteType::Core(CoreType::Any),
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)),
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer)),
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat)),
            ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                ndims: None,
            },
            ConcreteType::Tuple {
                elements: vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ],
            },
            ConcreteType::UnionOf(vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            ConcreteType::NamedTuple {
                fields: vec![(
                    "a".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                )],
            },
            ConcreteType::Dict {
                key: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                value: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String,
                ))),
            },
            ConcreteType::Set {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            },
            ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            },
            ConcreteType::Module {
                name: "MyMod".to_string(),
            },
            ConcreteType::DataType {
                name: "Int64".to_string(),
            },
        ];
        for ct in &faithful {
            assert_eq!(
                &rt(ct),
                ct,
                "expected CoreType round-trip identity for faithful variant {ct:?}"
            );
        }

        // ── §3 faithful-modulo-type_id: name kept, type_id reset to 0 ─────────
        // (Slice 2 resolves the id from the name via the struct table.)
        assert_eq!(
            rt(&ConcreteType::Struct {
                name: "Foo".to_string(),
                type_id: 7,
            }),
            ConcreteType::Struct {
                name: "Foo".to_string(),
                type_id: 0,
            },
        );

        // ── §2.2 lattice-only carriers: round-trip is lossy ──────────────────
        // Function loses its name; Closure (name + captures) and ComposedFunction
        // (structure) collapse to a nameless Function; Enum loses its enum-ness
        // and becomes a plain Struct. These are exactly the variants Slice 2
        // keeps out of `Core(_)`.
        assert_eq!(
            rt(&ConcreteType::Function {
                name: "f".to_string(),
            }),
            ConcreteType::Function {
                name: String::new(),
            },
        );
        assert_eq!(
            rt(&ConcreteType::Closure {
                name: "c".to_string(),
                captures: vec![(
                    "x".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                )],
            }),
            ConcreteType::Function {
                name: String::new(),
            },
        );
        assert_eq!(
            rt(&ConcreteType::ComposedFunction {
                outer: Box::new(ConcreteType::Function {
                    name: "g".to_string(),
                }),
                inner: Box::new(ConcreteType::Function {
                    name: "h".to_string(),
                }),
            }),
            ConcreteType::Function {
                name: String::new(),
            },
        );
        assert_eq!(
            rt(&ConcreteType::Enum {
                name: "Color".to_string(),
            }),
            ConcreteType::Struct {
                name: "Color".to_string(),
                type_id: 0,
            },
        );
    }

    /// Issue #6720 (Phase 6, Slice 2 / Commit A): behaviour-preserving smart
    /// constructors for the faithful parametric variants that Commit B folds
    /// into `Core(CoreType)`. Today each returns the matching variant unchanged;
    /// Commit B swaps only the helper bodies, not the call sites. This pins the
    /// current (no-op) contract.
    #[test]
    fn concretetype_smart_constructors_issue_6720() {
        assert_eq!(
            ConcreteType::array(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                Some(2)
            ),
            ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: Some(2),
            }
        );
        assert_eq!(
            ConcreteType::struct_named("Foo"),
            ConcreteType::Struct {
                name: "Foo".to_string(),
                type_id: 0,
            }
        );
        assert_eq!(
            ConcreteType::struct_with_id("Bar", 7),
            ConcreteType::Struct {
                name: "Bar".to_string(),
                type_id: 7,
            }
        );
        assert_eq!(
            ConcreteType::tuple(vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))
            ]),
            ConcreteType::Tuple {
                elements: vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))
                ],
            }
        );
        assert_eq!(
            ConcreteType::named_tuple(vec![(
                "a".to_string(),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            )]),
            ConcreteType::NamedTuple {
                fields: vec![(
                    "a".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                )],
            }
        );
        assert_eq!(
            ConcreteType::range(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))),
            ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
            }
        );
        assert_eq!(
            ConcreteType::dict(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            ),
            ConcreteType::Dict {
                key: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                value: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String
                ))),
            }
        );
        assert_eq!(
            ConcreteType::set(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))),
            ConcreteType::Set {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            }
        );
        assert_eq!(
            ConcreteType::generator(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))),
            ConcreteType::Generator {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            }
        );
        assert_eq!(
            ConcreteType::data_type("DataType"),
            ConcreteType::DataType {
                name: "DataType".to_string(),
            }
        );
        assert_eq!(
            ConcreteType::module_named("Module"),
            ConcreteType::Module {
                name: "Module".to_string(),
            }
        );
    }

    #[test]
    fn test_concrete_is_numeric() {
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)).is_numeric());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)).is_numeric());
        // Bool is numeric in Julia (subtype of Integer)
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)).is_numeric());

        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)).is_numeric());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)).is_numeric());
        assert!(!ConcreteType::Core(CoreType::Any).is_numeric());
    }

    #[test]
    fn test_concrete_is_integer() {
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)).is_integer());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)).is_integer());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)).is_integer());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)).is_integer());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)).is_integer());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)).is_integer());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)).is_integer());

        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)).is_integer());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)).is_integer());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)).is_integer());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)).is_integer());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)).is_integer());
        assert!(!ConcreteType::Core(CoreType::Any).is_integer());
    }

    #[test]
    fn test_concrete_is_float() {
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)).is_float());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)).is_float());
        assert!(ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)).is_float());

        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)).is_float());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)).is_float());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)).is_float());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)).is_float());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)).is_float());
        assert!(!ConcreteType::Core(CoreType::Any).is_float());
    }

    #[test]
    fn test_concrete_is_type_value() {
        assert!(ConcreteType::DataType {
            name: "Int64".to_string()
        }
        .is_type_value());
        assert!(ConcreteType::Module {
            name: "Base".to_string()
        }
        .is_type_value());

        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)).is_type_value());
        assert!(!ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)).is_type_value());
    }

    #[test]
    fn test_concrete_is_metaprogramming() {
        assert!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)).is_metaprogramming()
        );
        assert!(ConcreteType::Expr.is_metaprogramming());
        assert!(ConcreteType::QuoteNode.is_metaprogramming());
        assert!(ConcreteType::LineNumberNode.is_metaprogramming());
        assert!(ConcreteType::GlobalRef.is_metaprogramming());

        assert!(
            !ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)).is_metaprogramming()
        );
        assert!(
            !ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)).is_metaprogramming()
        );
    }

    #[test]
    fn test_concrete_type_core_bridge_preserves_structured_shapes() {
        assert_eq!(
            CoreType::from(&ConcreteType::Dict {
                key: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String
                ))),
                value: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
            }),
            CoreType::Struct {
                name: "Dict".to_string(),
                params: vec![
                    CoreType::Primitive(CorePrimitive::String),
                    CoreType::Primitive(CorePrimitive::Int64),
                ],
            }
        );

        assert_eq!(
            CoreType::from(&ConcreteType::TupleVararg {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))],
                tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            }),
            CoreType::Tuple(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Vararg(Box::new(CoreType::Primitive(CorePrimitive::Float64))),
            ])
        );
    }

    /// Issue #5916: `Range{element} → CoreType` must keep the element type.
    #[test]
    fn test_range_element_preserved_in_core_bridge_issue_5916() {
        let int_range = CoreType::from(&ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert_eq!(
            int_range,
            CoreType::Struct {
                name: "AbstractRange".to_string(),
                params: vec![CoreType::Primitive(CorePrimitive::Int64)],
            }
        );
        // Same structured form the canonical string bridge produces.
        assert_eq!(int_range, CoreType::from_julia_name("AbstractRange{Int64}"));

        // Unknown element keeps the bare abstract family (`AbstractRange{Any}`
        // would be an invariant — and therefore different — claim).
        assert_eq!(
            CoreType::from(&ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
            }),
            CoreType::Abstract(CoreAbstract::AbstractRange)
        );

        // Downstream matchers that assumed the unparameterized form still
        // accept the parameterized one through the subtype engine.
        let engine = CoreSubtypeEngine::new();
        assert!(engine.is_subtype(&int_range, &CoreType::Abstract(CoreAbstract::AbstractRange)));
        assert!(engine.is_subtype(&int_range, &CoreType::Any));

        // Float ranges keep their element too.
        let float_range = CoreType::from(&ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        });
        assert_eq!(
            float_range,
            CoreType::from_julia_name("AbstractRange{Float64}")
        );
        assert!(!engine.is_subtype(&int_range, &float_range));
    }

    #[test]
    fn test_lattice_type_core_bridge_widens_conditional_shape() {
        let conditional = LatticeType::make_conditional(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );

        assert_eq!(
            CoreType::from(&conditional),
            CoreType::Union(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::Nothing),
            ])
        );
    }

    #[test]
    fn test_lattice_is_numeric() {
        assert!(
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .is_numeric()
        );
        assert!(
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
            .is_numeric()
        );

        // Union of numeric types
        let mut numeric_union = BTreeSet::new();
        numeric_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        numeric_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        assert!(LatticeType::Union(numeric_union).is_numeric());

        // Union with non-numeric type
        let mut mixed_union = BTreeSet::new();
        mixed_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        mixed_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        assert!(!LatticeType::Union(mixed_union).is_numeric());

        assert!(!LatticeType::Top.is_numeric());
        assert!(!LatticeType::Bottom.is_numeric());
    }

    #[test]
    fn test_lattice_is_integer() {
        assert!(
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .is_integer()
        );
        assert!(
            !LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
            .is_integer()
        );

        // Union of integer types
        let mut int_union = BTreeSet::new();
        int_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        int_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        )));
        assert!(LatticeType::Union(int_union).is_integer());

        // Union with float
        let mut mixed = BTreeSet::new();
        mixed.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        mixed.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        assert!(!LatticeType::Union(mixed).is_integer());
    }

    #[test]
    fn test_lattice_is_float() {
        assert!(
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
            .is_float()
        );
        assert!(
            !LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .is_float()
        );

        // Union of float types
        let mut float_union = BTreeSet::new();
        float_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        float_union.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        )));
        assert!(LatticeType::Union(float_union).is_float());

        // Union with int
        let mut mixed = BTreeSet::new();
        mixed.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        mixed.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert!(!LatticeType::Union(mixed).is_float());
    }

    #[test]
    fn test_default_lattice_type() {
        assert_eq!(LatticeType::default(), LatticeType::Top);
    }

    #[test]
    fn test_concrete_to_type_name() {
        // Signed integers
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)).to_type_name(),
            Some("Int8".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)).to_type_name(),
            Some("Int16".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)).to_type_name(),
            Some("Int32".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)).to_type_name(),
            Some("Int64".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)).to_type_name(),
            Some("Int128".to_string())
        );

        // Unsigned integers
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)).to_type_name(),
            Some("UInt8".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)).to_type_name(),
            Some("UInt64".to_string())
        );

        // Floats
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)).to_type_name(),
            Some("Float32".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)).to_type_name(),
            Some("Float64".to_string())
        );

        // Bool
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)).to_type_name(),
            Some("Bool".to_string())
        );
        assert_eq!(
            ConcreteType::Core(CoreType::Any).to_type_name(),
            Some("Any".to_string())
        );

        // Struct types
        let complex = ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        };
        assert_eq!(complex.to_type_name(), Some("Complex{Float64}".to_string()));

        // Non-convertible types
        assert_eq!(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)).to_type_name(),
            None
        );
    }

    #[test]
    fn test_concrete_from_type_name() {
        // Signed integers
        assert_eq!(
            ConcreteType::from_type_name("Int8"),
            Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        );
        assert_eq!(
            ConcreteType::from_type_name("Int64"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            ConcreteType::from_type_name("Int"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        ); // Alias

        // Unsigned integers
        assert_eq!(
            ConcreteType::from_type_name("UInt8"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8
            )))
        );
        assert_eq!(
            ConcreteType::from_type_name("UInt64"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64
            )))
        );
        assert_eq!(
            ConcreteType::from_type_name("UInt"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64
            )))
        ); // Alias

        // Floats
        assert_eq!(
            ConcreteType::from_type_name("Float32"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
        assert_eq!(
            ConcreteType::from_type_name("Float64"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        // Bool and String
        assert_eq!(
            ConcreteType::from_type_name("Bool"),
            Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
        assert_eq!(
            ConcreteType::from_type_name("String"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
        assert_eq!(
            ConcreteType::from_type_name("Any"),
            Some(ConcreteType::Core(CoreType::Any))
        );

        // Parametric struct types
        let result = ConcreteType::from_type_name("Complex{Float64}");
        assert!(
            matches!(result, Some(ConcreteType::Struct { name, .. }) if name == "Complex{Float64}")
        );

        // Unknown types
        assert_eq!(ConcreteType::from_type_name("Unknown"), None);
    }

    #[test]
    fn test_type_name_roundtrip() {
        // Test that to_type_name -> from_type_name roundtrips correctly
        let types = [
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Any),
        ];

        for ty in types {
            let name = ty.to_type_name().unwrap();
            let back = ConcreteType::from_type_name(&name).unwrap();
            assert_eq!(ty, back);
        }
    }

    // Issue #2863: Tests for Enum variant
    #[test]
    fn test_enum_is_not_numeric() {
        let enum_type = ConcreteType::Enum {
            name: "Color".to_string(),
        };
        assert!(!enum_type.is_numeric(), "Enum should not be numeric");
        assert!(!enum_type.is_integer(), "Enum should not be integer");
        assert!(!enum_type.is_float(), "Enum should not be float");
    }

    #[test]
    fn test_enum_is_not_type_value() {
        let enum_type = ConcreteType::Enum {
            name: "Color".to_string(),
        };
        assert!(!enum_type.is_type_value());
    }

    #[test]
    fn test_enum_is_not_metaprogramming() {
        let enum_type = ConcreteType::Enum {
            name: "Direction".to_string(),
        };
        assert!(!enum_type.is_metaprogramming());
    }

    #[test]
    fn test_enum_to_type_name_returns_none() {
        // Enum has no simple Julia type name string representation
        let enum_type = ConcreteType::Enum {
            name: "Color".to_string(),
        };
        assert_eq!(enum_type.to_type_name(), None);
    }

    #[test]
    fn test_lattice_concrete_enum_is_not_numeric() {
        let lattice = LatticeType::Concrete(ConcreteType::Enum {
            name: "Suit".to_string(),
        });
        assert!(!lattice.is_numeric());
        assert!(!lattice.is_integer());
        assert!(!lattice.is_float());
    }

    // Issue #1637: Tests for UnionOf variant
    #[test]
    fn test_union_of_is_numeric() {
        // UnionOf numeric types is numeric
        let union_numeric = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ]);
        assert!(union_numeric.is_numeric());

        // UnionOf with non-numeric type is not numeric
        let union_mixed = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
        ]);
        assert!(!union_mixed.is_numeric());
    }

    #[test]
    fn test_union_of_is_integer() {
        // UnionOf integer types is integer
        let union_int = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ]);
        assert!(union_int.is_integer());

        // UnionOf with float is not integer
        let union_float = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ]);
        assert!(!union_float.is_integer());
    }

    #[test]
    fn test_union_of_is_float() {
        // UnionOf float types is float
        let union_float = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ]);
        assert!(union_float.is_float());

        // UnionOf with int is not float
        let union_mixed = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ]);
        assert!(!union_mixed.is_float());
    }

    #[test]
    fn test_union_of_to_type_name() {
        // UnionOf should produce Union{...} string
        let union_type = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ]);
        let name = union_type.to_type_name();
        assert!(name.is_some());
        let name_str = name.unwrap();
        assert!(name_str.starts_with("Union{"));
        assert!(name_str.contains("Int64"));
        assert!(name_str.contains("Float64"));
    }

    #[test]
    fn test_union_of_nested() {
        // Nested UnionOf should work for is_numeric
        let nested = ConcreteType::UnionOf(vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::UnionOf(vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
        ]);
        assert!(nested.is_numeric());
    }

    // ====== Issue #3503: Conditional helpers ======

    #[test]
    fn test_make_conditional_collapses_identical_branches() {
        // Branches are identical → no narrowing info, return the branch directly.
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = LatticeType::make_conditional("x", int.clone(), int.clone());
        assert_eq!(result, int);
        assert!(!result.is_conditional());
    }

    #[test]
    fn test_make_conditional_keeps_distinct_branches() {
        let result = LatticeType::make_conditional(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert!(result.is_conditional());
    }

    #[test]
    fn test_make_conditional_top_top_collapses() {
        // Conditional(slot; Top, Top) carries no narrowing — collapses to Top.
        let result = LatticeType::make_conditional("x", LatticeType::Top, LatticeType::Top);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_widen_conditional_then_else_join() {
        let c = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))),
            else_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing),
            ))),
        };
        let widened = c.widen_conditional();
        // join(Int64, Nothing) → Union{Int64, Nothing}
        let mut expected = BTreeSet::new();
        expected.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        expected.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        assert_eq!(widened, LatticeType::Union(expected));
    }

    #[test]
    fn test_widen_conditional_non_conditional_is_identity() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(int.widen_conditional(), int);
        assert_eq!(LatticeType::Top.widen_conditional(), LatticeType::Top);
        assert_eq!(LatticeType::Bottom.widen_conditional(), LatticeType::Bottom);
    }

    #[test]
    fn test_is_conditional_predicate() {
        assert!(!LatticeType::Top.is_conditional());
        assert!(
            !LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .is_conditional()
        );
        let c = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))),
            else_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing),
            ))),
        };
        assert!(c.is_conditional());
    }

    /// Coverage test: all ConcreteType variants must be listed here (Issue #3187).
    ///
    /// When adding a new ConcreteType variant, update the list below AND review
    /// the checklist in docs/vm/LATTICE_TYPE.md.
    #[test]
    fn test_all_concrete_type_variants_constructible() {
        let variants: Vec<ConcreteType> = vec![
            // Signed integers
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)),
            // Unsigned integers
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)),
            // Floating point
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)),
            // Basic types
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)),
            ConcreteType::Core(CoreType::Any),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing)),
            // Abstract types
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)),
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer)),
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat)),
            // Symbolic
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
            // Composite
            ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None,
            },
            ConcreteType::Tuple { elements: vec![] },
            // Issue #3511: Tuple with Vararg tail.
            ConcreteType::TupleVararg {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))],
                tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            },
            ConcreteType::NamedTuple { fields: vec![] },
            ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            },
            ConcreteType::Dict {
                key: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String,
                ))),
                value: Box::new(ConcreteType::Core(CoreType::Any)),
            },
            ConcreteType::Set {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            },
            ConcreteType::Generator {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
            },
            ConcreteType::Pairs,
            // User-defined
            ConcreteType::Struct {
                name: "Test".to_string(),
                type_id: 0,
            },
            // Callable
            ConcreteType::Function {
                name: "f".to_string(),
            },
            ConcreteType::ComposedFunction {
                outer: Box::new(ConcreteType::Function {
                    name: "f".to_string(),
                }),
                inner: Box::new(ConcreteType::Function {
                    name: "g".to_string(),
                }),
            },
            // Type system
            ConcreteType::DataType {
                name: "Int64".to_string(),
            },
            ConcreteType::Module {
                name: "Main".to_string(),
            },
            // IO
            ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)),
            // Metaprogramming
            ConcreteType::Expr,
            ConcreteType::QuoteNode,
            ConcreteType::LineNumberNode,
            ConcreteType::GlobalRef,
            // Pattern matching
            ConcreteType::Regex,
            ConcreteType::RegexMatch,
            // Union
            ConcreteType::UnionOf(vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))]),
            // Enum
            ConcreteType::Enum {
                name: "Color".to_string(),
            },
        ];
        assert!(!variants.is_empty());
    }

    /// Issue #6599 (Phase 3, Slice A): the new downward edge
    /// `From<&CoreType> for ConcreteType` must be a genuine inverse of the
    /// existing `From<&ConcreteType> for CoreType` on the arms that
    /// `ConcreteType` can represent without loss. `CoreType::from(&ConcreteType::from(&core))`
    /// is the identity for every projectable `CoreType` below.
    #[test]
    fn coretype_concretetype_round_trips_on_projectable_arms_issue_6599() {
        let projectable = vec![
            // Primitives / singletons.
            CoreType::Primitive(CorePrimitive::Int64),
            CoreType::Primitive(CorePrimitive::Float64),
            CoreType::Primitive(CorePrimitive::Bool),
            CoreType::Primitive(CorePrimitive::String),
            CoreType::Primitive(CorePrimitive::Char),
            CoreType::Primitive(CorePrimitive::Nothing),
            CoreType::Primitive(CorePrimitive::Missing),
            CoreType::Primitive(CorePrimitive::Symbol),
            // Structured shapes.
            CoreType::Tuple(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::Float64),
            ]),
            CoreType::Union(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::Float64),
            ]),
            // Container struct: `Array{Int64}` survives both directions.
            CoreType::Struct {
                name: "Array".to_string(),
                params: vec![CoreType::Primitive(CorePrimitive::Int64)],
            },
            // Module spelling.
            CoreType::Module("Base".to_string()),
        ];

        for core in projectable {
            let projected = ConcreteType::from(&core);
            assert_eq!(
                CoreType::from(&projected),
                core,
                "round trip changed projectable CoreType: {core:?} -> {projected:?}"
            );
        }
    }

    /// Issue #6599 (Phase 3, Slice A): document the deliberately-lossy arms.
    /// `ConcreteType` is strictly less expressive than `CoreType`, so these
    /// arms over-approximate (usually to `Any`). This test pins the
    /// documented image so a future change that silently narrows/widens a
    /// lossy arm is caught.
    #[test]
    fn coretype_to_concretetype_lossy_arms_widen_issue_6599() {
        // `Bottom` has no ConcreteType image -> over-approximate to `Any`.
        assert_eq!(
            ConcreteType::from(&CoreType::Bottom),
            ConcreteType::Core(CoreType::Any)
        );

        // Abstract families without a concrete image collapse to `Any`.
        assert_eq!(
            ConcreteType::from(&CoreType::Abstract(CoreAbstract::Real)),
            ConcreteType::Core(CoreType::Any)
        );

        // Type variable: drop the quantifier, keep nothing concrete -> `Any`.
        assert_eq!(
            ConcreteType::from(&CoreType::TypeVar(CoreTypeVar {
                name: "T".to_string(),
                lower_bound: None,
                upper_bound: None,
            })),
            ConcreteType::Core(CoreType::Any)
        );

        // A value parameter is not a type -> `Any`.
        assert_eq!(
            ConcreteType::from(&CoreType::Value(CoreValueParam::Int(1))),
            ConcreteType::Core(CoreType::Any)
        );

        // `UnionAll` drops the binder and projects its body.
        assert_eq!(
            ConcreteType::from(&CoreType::UnionAll {
                var: CoreTypeVar {
                    name: "T".to_string(),
                    lower_bound: None,
                    upper_bound: None,
                },
                body: Box::new(CoreType::Primitive(CorePrimitive::Int64)),
            }),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
        );

        // Bare `Array` (no params) becomes `Array{Any}` — preserves the
        // bare-`Array` special case (#5916).
        assert_eq!(
            ConcreteType::from(&CoreType::Struct {
                name: "Array".to_string(),
                params: vec![],
            }),
            ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            }
        );
    }

    /// Issue #6919 (epic #5916, ValueType demotion continuation): the
    /// `CoreType::Primitive(_)` arm of the downward edge `From<&CoreType> for
    /// ConcreteType` is a pure identity — every primitive `p` projects to
    /// `ConcreteType::Core(CoreType::Primitive(p))`. This pins that identity
    /// over *every* `CorePrimitive` variant so the 21-arm match can be folded
    /// to a single binding arm without changing behaviour (the same fold the
    /// upward edge already uses via `ConcreteType::Core(c) => c.clone()`). The
    /// list mirrors the `CorePrimitive` enum; a new variant should be added
    /// here too.
    #[test]
    fn coretype_to_concretetype_maps_every_primitive_to_identity_issue_6919() {
        let primitives = [
            CorePrimitive::Bool,
            CorePrimitive::Int8,
            CorePrimitive::Int16,
            CorePrimitive::Int32,
            CorePrimitive::Int64,
            CorePrimitive::Int128,
            CorePrimitive::UInt8,
            CorePrimitive::UInt16,
            CorePrimitive::UInt32,
            CorePrimitive::UInt64,
            CorePrimitive::UInt128,
            CorePrimitive::Float16,
            CorePrimitive::Float32,
            CorePrimitive::Float64,
            CorePrimitive::BigInt,
            CorePrimitive::BigFloat,
            CorePrimitive::String,
            CorePrimitive::Char,
            CorePrimitive::Symbol,
            CorePrimitive::Nothing,
            CorePrimitive::Missing,
        ];
        for p in primitives {
            assert_eq!(
                ConcreteType::from(&CoreType::Primitive(p.clone())),
                ConcreteType::Core(CoreType::Primitive(p.clone())),
                "primitive {p:?} must project to its identity ConcreteType"
            );
        }
    }
}
