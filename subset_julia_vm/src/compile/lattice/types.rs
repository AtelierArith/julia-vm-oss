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
            ConstValue::Int64(_) => ConcreteType::Int64,
            ConstValue::Float64(_) => ConcreteType::Float64,
            ConstValue::Bool(_) => ConcreteType::Bool,
            ConstValue::String(_) => ConcreteType::String,
            ConstValue::Symbol(_) => ConcreteType::Symbol,
            ConstValue::Nothing => ConcreteType::Nothing,
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
    // Numeric types - signed integers
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    BigInt,

    // Numeric types - unsigned integers
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,

    // Numeric types - floating point
    Float16,
    Float32,
    Float64,
    BigFloat,

    // Boolean
    Bool,

    // Text types
    String,
    Char,

    // Special values
    /// Any (used for Array{Any} and other element-unknown cases)
    Any,
    Nothing,
    Missing,

    // Abstract types
    /// Number - abstract supertype of all numeric types (Int*, UInt*, Float*, etc.)
    /// Used for usage-based type inference when a parameter is used in arithmetic.
    Number,
    /// Integer - abstract supertype of all integer types (Int*, UInt*)
    Integer,
    /// AbstractFloat - abstract supertype of all floating-point types (Float*)
    AbstractFloat,

    // Symbolic types
    Symbol,

    // Composite types
    Array {
        element: Box<ConcreteType>,
    },
    Tuple {
        elements: Vec<ConcreteType>,
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

    // Type system types
    /// DataType - the type of types (returned by typeof)
    DataType {
        name: String,
    },
    /// Module type (e.g., Statistics, Base)
    Module {
        name: String,
    },

    // IO types
    IO,

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
    /// Returns true if this is a numeric type.
    /// In Julia, Bool is a subtype of Integer and participates in numeric operations.
    pub fn is_numeric(&self) -> bool {
        match self {
            // Abstract numeric types
            ConcreteType::Number
            | ConcreteType::Integer
            | ConcreteType::AbstractFloat
            // Bool is numeric in Julia (subtype of Integer)
            | ConcreteType::Bool
            // Concrete signed integers
            | ConcreteType::Int8
            | ConcreteType::Int16
            | ConcreteType::Int32
            | ConcreteType::Int64
            | ConcreteType::Int128
            | ConcreteType::BigInt
            // Concrete unsigned integers
            | ConcreteType::UInt8
            | ConcreteType::UInt16
            | ConcreteType::UInt32
            | ConcreteType::UInt64
            | ConcreteType::UInt128
            // Concrete floating-point
            | ConcreteType::Float16
            | ConcreteType::Float32
            | ConcreteType::Float64
            | ConcreteType::BigFloat => true,
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_numeric()),
            _ => false,
        }
    }

    /// Returns true if this is an integer type.
    pub fn is_integer(&self) -> bool {
        match self {
            // Abstract integer type
            ConcreteType::Integer
            // Concrete signed integers
            | ConcreteType::Int8
            | ConcreteType::Int16
            | ConcreteType::Int32
            | ConcreteType::Int64
            | ConcreteType::Int128
            | ConcreteType::BigInt
            // Concrete unsigned integers
            | ConcreteType::UInt8
            | ConcreteType::UInt16
            | ConcreteType::UInt32
            | ConcreteType::UInt64
            | ConcreteType::UInt128 => true,
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_integer()),
            _ => false,
        }
    }

    /// Returns true if this is a floating-point type.
    pub fn is_float(&self) -> bool {
        match self {
            // Abstract floating-point type
            ConcreteType::AbstractFloat
            // Concrete floating-point types
            | ConcreteType::Float16
            | ConcreteType::Float32
            | ConcreteType::Float64
            | ConcreteType::BigFloat => true,
            ConcreteType::UnionOf(types) => types.iter().all(|t| t.is_float()),
            _ => false,
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
            ConcreteType::Symbol
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
            ConcreteType::Int8 => Some("Int8".to_string()),
            ConcreteType::Int16 => Some("Int16".to_string()),
            ConcreteType::Int32 => Some("Int32".to_string()),
            ConcreteType::Int64 => Some("Int64".to_string()),
            ConcreteType::Int128 => Some("Int128".to_string()),
            ConcreteType::BigInt => Some("BigInt".to_string()),
            // Unsigned integers
            ConcreteType::UInt8 => Some("UInt8".to_string()),
            ConcreteType::UInt16 => Some("UInt16".to_string()),
            ConcreteType::UInt32 => Some("UInt32".to_string()),
            ConcreteType::UInt64 => Some("UInt64".to_string()),
            ConcreteType::UInt128 => Some("UInt128".to_string()),
            // Floats
            ConcreteType::Float16 => Some("Float16".to_string()),
            ConcreteType::Float32 => Some("Float32".to_string()),
            ConcreteType::Float64 => Some("Float64".to_string()),
            ConcreteType::BigFloat => Some("BigFloat".to_string()),
            // Boolean
            ConcreteType::Bool => Some("Bool".to_string()),
            // Any
            ConcreteType::Any => Some("Any".to_string()),
            // Abstract numeric types
            ConcreteType::Number => Some("Number".to_string()),
            ConcreteType::Integer => Some("Integer".to_string()),
            ConcreteType::AbstractFloat => Some("AbstractFloat".to_string()),
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
            // Other types don't have simple type names
            _ => None,
        }
    }

    /// Create a ConcreteType from a Julia type name string.
    /// Used for integration with the centralized promotion system.
    pub fn from_type_name(name: &str) -> Option<Self> {
        match name {
            // Signed integers
            "Int8" => Some(ConcreteType::Int8),
            "Int16" => Some(ConcreteType::Int16),
            "Int32" => Some(ConcreteType::Int32),
            "Int64" | "Int" => Some(ConcreteType::Int64),
            "Int128" => Some(ConcreteType::Int128),
            "BigInt" => Some(ConcreteType::BigInt),
            // Unsigned integers
            "UInt8" => Some(ConcreteType::UInt8),
            "UInt16" => Some(ConcreteType::UInt16),
            "UInt32" => Some(ConcreteType::UInt32),
            "UInt64" | "UInt" => Some(ConcreteType::UInt64),
            "UInt128" => Some(ConcreteType::UInt128),
            // Floats
            "Float16" => Some(ConcreteType::Float16),
            "Float32" => Some(ConcreteType::Float32),
            "Float64" => Some(ConcreteType::Float64),
            "BigFloat" => Some(ConcreteType::BigFloat),
            // Boolean
            "Bool" => Some(ConcreteType::Bool),
            // Any
            "Any" => Some(ConcreteType::Any),
            // Abstract numeric types
            "Number" => Some(ConcreteType::Number),
            "Integer" => Some(ConcreteType::Integer),
            "AbstractFloat" => Some(ConcreteType::AbstractFloat),
            // String/Char
            "String" => Some(ConcreteType::String),
            "Char" => Some(ConcreteType::Char),
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

impl Default for LatticeType {
    /// The default lattice type is Top (Any), representing maximum uncertainty.
    fn default() -> Self {
        LatticeType::Top
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concrete_is_numeric() {
        assert!(ConcreteType::Int64.is_numeric());
        assert!(ConcreteType::Float64.is_numeric());
        assert!(ConcreteType::Int8.is_numeric());
        assert!(ConcreteType::UInt32.is_numeric());
        assert!(ConcreteType::Float32.is_numeric());
        assert!(ConcreteType::Int128.is_numeric());
        assert!(ConcreteType::UInt128.is_numeric());
        assert!(ConcreteType::BigInt.is_numeric());
        assert!(ConcreteType::BigFloat.is_numeric());
        // Bool is numeric in Julia (subtype of Integer)
        assert!(ConcreteType::Bool.is_numeric());

        assert!(!ConcreteType::String.is_numeric());
        assert!(!ConcreteType::Nothing.is_numeric());
        assert!(!ConcreteType::Any.is_numeric());
    }

    #[test]
    fn test_concrete_is_integer() {
        assert!(ConcreteType::Int64.is_integer());
        assert!(ConcreteType::Int8.is_integer());
        assert!(ConcreteType::UInt32.is_integer());
        assert!(ConcreteType::UInt64.is_integer());
        assert!(ConcreteType::Int128.is_integer());
        assert!(ConcreteType::UInt128.is_integer());
        assert!(ConcreteType::BigInt.is_integer());

        assert!(!ConcreteType::Float64.is_integer());
        assert!(!ConcreteType::Float32.is_integer());
        assert!(!ConcreteType::BigFloat.is_integer());
        assert!(!ConcreteType::String.is_integer());
        assert!(!ConcreteType::Bool.is_integer());
        assert!(!ConcreteType::Any.is_integer());
    }

    #[test]
    fn test_concrete_is_float() {
        assert!(ConcreteType::Float64.is_float());
        assert!(ConcreteType::Float32.is_float());
        assert!(ConcreteType::BigFloat.is_float());

        assert!(!ConcreteType::Int64.is_float());
        assert!(!ConcreteType::Int8.is_float());
        assert!(!ConcreteType::BigInt.is_float());
        assert!(!ConcreteType::String.is_float());
        assert!(!ConcreteType::Bool.is_float());
        assert!(!ConcreteType::Any.is_float());
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

        assert!(!ConcreteType::Int64.is_type_value());
        assert!(!ConcreteType::String.is_type_value());
    }

    #[test]
    fn test_concrete_is_metaprogramming() {
        assert!(ConcreteType::Symbol.is_metaprogramming());
        assert!(ConcreteType::Expr.is_metaprogramming());
        assert!(ConcreteType::QuoteNode.is_metaprogramming());
        assert!(ConcreteType::LineNumberNode.is_metaprogramming());
        assert!(ConcreteType::GlobalRef.is_metaprogramming());

        assert!(!ConcreteType::Int64.is_metaprogramming());
        assert!(!ConcreteType::String.is_metaprogramming());
    }

    #[test]
    fn test_lattice_is_numeric() {
        assert!(LatticeType::Concrete(ConcreteType::Int64).is_numeric());
        assert!(LatticeType::Concrete(ConcreteType::Float64).is_numeric());

        // Union of numeric types
        let mut numeric_union = BTreeSet::new();
        numeric_union.insert(ConcreteType::Int64);
        numeric_union.insert(ConcreteType::Float64);
        assert!(LatticeType::Union(numeric_union).is_numeric());

        // Union with non-numeric type
        let mut mixed_union = BTreeSet::new();
        mixed_union.insert(ConcreteType::Int64);
        mixed_union.insert(ConcreteType::String);
        assert!(!LatticeType::Union(mixed_union).is_numeric());

        assert!(!LatticeType::Top.is_numeric());
        assert!(!LatticeType::Bottom.is_numeric());
    }

    #[test]
    fn test_lattice_is_integer() {
        assert!(LatticeType::Concrete(ConcreteType::Int64).is_integer());
        assert!(!LatticeType::Concrete(ConcreteType::Float64).is_integer());

        // Union of integer types
        let mut int_union = BTreeSet::new();
        int_union.insert(ConcreteType::Int64);
        int_union.insert(ConcreteType::Int32);
        assert!(LatticeType::Union(int_union).is_integer());

        // Union with float
        let mut mixed = BTreeSet::new();
        mixed.insert(ConcreteType::Int64);
        mixed.insert(ConcreteType::Float64);
        assert!(!LatticeType::Union(mixed).is_integer());
    }

    #[test]
    fn test_lattice_is_float() {
        assert!(LatticeType::Concrete(ConcreteType::Float64).is_float());
        assert!(!LatticeType::Concrete(ConcreteType::Int64).is_float());

        // Union of float types
        let mut float_union = BTreeSet::new();
        float_union.insert(ConcreteType::Float64);
        float_union.insert(ConcreteType::Float32);
        assert!(LatticeType::Union(float_union).is_float());

        // Union with int
        let mut mixed = BTreeSet::new();
        mixed.insert(ConcreteType::Float64);
        mixed.insert(ConcreteType::Int64);
        assert!(!LatticeType::Union(mixed).is_float());
    }

    #[test]
    fn test_default_lattice_type() {
        assert_eq!(LatticeType::default(), LatticeType::Top);
    }

    #[test]
    fn test_concrete_to_type_name() {
        // Signed integers
        assert_eq!(ConcreteType::Int8.to_type_name(), Some("Int8".to_string()));
        assert_eq!(
            ConcreteType::Int16.to_type_name(),
            Some("Int16".to_string())
        );
        assert_eq!(
            ConcreteType::Int32.to_type_name(),
            Some("Int32".to_string())
        );
        assert_eq!(
            ConcreteType::Int64.to_type_name(),
            Some("Int64".to_string())
        );
        assert_eq!(
            ConcreteType::Int128.to_type_name(),
            Some("Int128".to_string())
        );

        // Unsigned integers
        assert_eq!(
            ConcreteType::UInt8.to_type_name(),
            Some("UInt8".to_string())
        );
        assert_eq!(
            ConcreteType::UInt64.to_type_name(),
            Some("UInt64".to_string())
        );

        // Floats
        assert_eq!(
            ConcreteType::Float32.to_type_name(),
            Some("Float32".to_string())
        );
        assert_eq!(
            ConcreteType::Float64.to_type_name(),
            Some("Float64".to_string())
        );

        // Bool
        assert_eq!(ConcreteType::Bool.to_type_name(), Some("Bool".to_string()));
        assert_eq!(ConcreteType::Any.to_type_name(), Some("Any".to_string()));

        // Struct types
        let complex = ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        };
        assert_eq!(complex.to_type_name(), Some("Complex{Float64}".to_string()));

        // Non-convertible types
        assert_eq!(ConcreteType::Nothing.to_type_name(), None);
    }

    #[test]
    fn test_concrete_from_type_name() {
        // Signed integers
        assert_eq!(
            ConcreteType::from_type_name("Int8"),
            Some(ConcreteType::Int8)
        );
        assert_eq!(
            ConcreteType::from_type_name("Int64"),
            Some(ConcreteType::Int64)
        );
        assert_eq!(
            ConcreteType::from_type_name("Int"),
            Some(ConcreteType::Int64)
        ); // Alias

        // Unsigned integers
        assert_eq!(
            ConcreteType::from_type_name("UInt8"),
            Some(ConcreteType::UInt8)
        );
        assert_eq!(
            ConcreteType::from_type_name("UInt64"),
            Some(ConcreteType::UInt64)
        );
        assert_eq!(
            ConcreteType::from_type_name("UInt"),
            Some(ConcreteType::UInt64)
        ); // Alias

        // Floats
        assert_eq!(
            ConcreteType::from_type_name("Float32"),
            Some(ConcreteType::Float32)
        );
        assert_eq!(
            ConcreteType::from_type_name("Float64"),
            Some(ConcreteType::Float64)
        );

        // Bool and String
        assert_eq!(
            ConcreteType::from_type_name("Bool"),
            Some(ConcreteType::Bool)
        );
        assert_eq!(
            ConcreteType::from_type_name("String"),
            Some(ConcreteType::String)
        );
        assert_eq!(ConcreteType::from_type_name("Any"), Some(ConcreteType::Any));

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
            ConcreteType::Int8,
            ConcreteType::Int64,
            ConcreteType::UInt32,
            ConcreteType::Float32,
            ConcreteType::Float64,
            ConcreteType::Bool,
            ConcreteType::Any,
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
        let union_numeric = ConcreteType::UnionOf(vec![ConcreteType::Int64, ConcreteType::Float64]);
        assert!(union_numeric.is_numeric());

        // UnionOf with non-numeric type is not numeric
        let union_mixed = ConcreteType::UnionOf(vec![ConcreteType::Int64, ConcreteType::String]);
        assert!(!union_mixed.is_numeric());
    }

    #[test]
    fn test_union_of_is_integer() {
        // UnionOf integer types is integer
        let union_int = ConcreteType::UnionOf(vec![ConcreteType::Int64, ConcreteType::Int32]);
        assert!(union_int.is_integer());

        // UnionOf with float is not integer
        let union_float = ConcreteType::UnionOf(vec![ConcreteType::Int64, ConcreteType::Float64]);
        assert!(!union_float.is_integer());
    }

    #[test]
    fn test_union_of_is_float() {
        // UnionOf float types is float
        let union_float = ConcreteType::UnionOf(vec![ConcreteType::Float32, ConcreteType::Float64]);
        assert!(union_float.is_float());

        // UnionOf with int is not float
        let union_mixed = ConcreteType::UnionOf(vec![ConcreteType::Float64, ConcreteType::Int64]);
        assert!(!union_mixed.is_float());
    }

    #[test]
    fn test_union_of_to_type_name() {
        // UnionOf should produce Union{...} string
        let union_type = ConcreteType::UnionOf(vec![ConcreteType::Int64, ConcreteType::Float64]);
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
            ConcreteType::Int64,
            ConcreteType::UnionOf(vec![ConcreteType::Float32, ConcreteType::Float64]),
        ]);
        assert!(nested.is_numeric());
    }

    /// Coverage test: all ConcreteType variants must be listed here (Issue #3187).
    ///
    /// When adding a new ConcreteType variant, update the list below AND review
    /// the checklist in docs/vm/LATTICE_TYPE.md.
    #[test]
    fn test_all_concrete_type_variants_constructible() {
        let variants: Vec<ConcreteType> = vec![
            // Signed integers
            ConcreteType::Int8,
            ConcreteType::Int16,
            ConcreteType::Int32,
            ConcreteType::Int64,
            ConcreteType::Int128,
            ConcreteType::BigInt,
            // Unsigned integers
            ConcreteType::UInt8,
            ConcreteType::UInt16,
            ConcreteType::UInt32,
            ConcreteType::UInt64,
            ConcreteType::UInt128,
            // Floating point
            ConcreteType::Float16,
            ConcreteType::Float32,
            ConcreteType::Float64,
            ConcreteType::BigFloat,
            // Basic types
            ConcreteType::Bool,
            ConcreteType::String,
            ConcreteType::Char,
            ConcreteType::Any,
            ConcreteType::Nothing,
            ConcreteType::Missing,
            // Abstract types
            ConcreteType::Number,
            ConcreteType::Integer,
            ConcreteType::AbstractFloat,
            // Symbolic
            ConcreteType::Symbol,
            // Composite
            ConcreteType::Array {
                element: Box::new(ConcreteType::Any),
            },
            ConcreteType::Tuple {
                elements: vec![],
            },
            ConcreteType::NamedTuple { fields: vec![] },
            ConcreteType::Range {
                element: Box::new(ConcreteType::Int64),
            },
            ConcreteType::Dict {
                key: Box::new(ConcreteType::String),
                value: Box::new(ConcreteType::Any),
            },
            ConcreteType::Set {
                element: Box::new(ConcreteType::Int64),
            },
            ConcreteType::Generator {
                element: Box::new(ConcreteType::Any),
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
            // Type system
            ConcreteType::DataType {
                name: "Int64".to_string(),
            },
            ConcreteType::Module {
                name: "Main".to_string(),
            },
            // IO
            ConcreteType::IO,
            // Metaprogramming
            ConcreteType::Expr,
            ConcreteType::QuoteNode,
            ConcreteType::LineNumberNode,
            ConcreteType::GlobalRef,
            // Pattern matching
            ConcreteType::Regex,
            ConcreteType::RegexMatch,
            // Union
            ConcreteType::UnionOf(vec![ConcreteType::Int64]),
            // Enum
            ConcreteType::Enum {
                name: "Color".to_string(),
            },
        ];
        assert!(!variants.is_empty());
    }
}
