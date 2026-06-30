use super::super::PrimitiveNumeric;
use serde::{Deserialize, Serialize};

/// Concrete primitive / singleton Julia types that do not carry parameters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CorePrimitive {
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    Float16,
    Float32,
    Float64,
    BigInt,
    BigFloat,
    String,
    Char,
    Symbol,
    Nothing,
    Missing,
}

impl CorePrimitive {
    pub fn primitive_numeric(&self) -> Option<PrimitiveNumeric> {
        Some(match self {
            Self::Bool => PrimitiveNumeric::Bool,
            Self::Int8 => PrimitiveNumeric::Int8,
            Self::Int16 => PrimitiveNumeric::Int16,
            Self::Int32 => PrimitiveNumeric::Int32,
            Self::Int64 => PrimitiveNumeric::Int64,
            Self::Int128 => PrimitiveNumeric::Int128,
            Self::UInt8 => PrimitiveNumeric::UInt8,
            Self::UInt16 => PrimitiveNumeric::UInt16,
            Self::UInt32 => PrimitiveNumeric::UInt32,
            Self::UInt64 => PrimitiveNumeric::UInt64,
            Self::UInt128 => PrimitiveNumeric::UInt128,
            Self::Float16 => PrimitiveNumeric::Float16,
            Self::Float32 => PrimitiveNumeric::Float32,
            Self::Float64 => PrimitiveNumeric::Float64,
            Self::BigInt
            | Self::BigFloat
            | Self::String
            | Self::Char
            | Self::Symbol
            | Self::Nothing
            | Self::Missing => return None,
        })
    }

    pub fn builtin_sizeof_bytes(&self) -> Option<usize> {
        Some(match self {
            Self::Bool | Self::Int8 | Self::UInt8 => 1,
            Self::Int16 | Self::UInt16 | Self::Float16 => 2,
            Self::Int32 | Self::UInt32 | Self::Float32 | Self::Char => 4,
            Self::Int64 | Self::UInt64 | Self::Float64 => 8,
            Self::Int128 | Self::UInt128 => 16,
            Self::Nothing | Self::Missing => 0,
            Self::BigInt | Self::BigFloat | Self::String | Self::Symbol => return None,
        })
    }
}

/// Built-in abstract Julia types currently represented by the shared core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CoreAbstract {
    Number,
    Real,
    Integer,
    Signed,
    Unsigned,
    AbstractFloat,
    AbstractString,
    AbstractChar,
    AbstractArray,
    AbstractVector,
    AbstractMatrix,
    DenseArray,
    AbstractDict,
    AbstractSet,
    AbstractRange,
    AbstractUnitRange,
    Function,
    /// `Core.Builtin`: the abstract supertype of genuine built-in functions
    /// (`===`, `getfield`, `typeof`, ...). `Core.Builtin <: Function`, but
    /// generic / user functions are `<: Function` only, never `<: Core.Builtin`
    /// (Issue #5129).
    Builtin,
    IO,
    Type,
    DataType,
}

/// Names that, when referenced *unqualified* in ordinary user code, resolve to a
/// genuine `Core.Builtin` function in upstream Julia 1.12. Verified by evaluating
/// each name and testing `isa(v, Core.Builtin)` against the real `julia` binary
/// (Issue #5129). Note that some `Core.Builtin` names (`ifelse`, `sizeof`,
/// `getproperty`, `setproperty!`, `finalizer`) are shadowed by generic `Base`
/// wrappers when referenced unqualified, so they are deliberately excluded here.
pub const CORE_BUILTIN_FUNCTION_NAMES: &[&str] = &[
    "===",
    "<:",
    "applicable",
    "fieldtype",
    "getfield",
    "getglobal",
    "invoke",
    "isa",
    "isdefined",
    "modifyfield!",
    "nfields",
    "replacefield!",
    "setfield!",
    "swapfield!",
    "throw",
    "tuple",
    "typeassert",
    "typeof",
];

/// True iff `name` is a function-singleton type name `typeof(<fn>)` whose
/// underlying function is a genuine `Core.Builtin` (Issue #5129).
pub fn is_core_builtin_singleton_type_name(name: &str) -> bool {
    name.strip_prefix("typeof(")
        .and_then(|inner| inner.strip_suffix(')'))
        .is_some_and(is_core_builtin_function_name)
}

/// True iff a function referenced by `name` (unqualified) is a genuine
/// `Core.Builtin` in upstream Julia (Issue #5129).
pub fn is_core_builtin_function_name(name: &str) -> bool {
    CORE_BUILTIN_FUNCTION_NAMES.contains(&name)
}

/// Type variable with optional lower/upper bounds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CoreTypeVar {
    pub name: String,
    pub lower_bound: Option<Box<CoreType>>,
    pub upper_bound: Option<Box<CoreType>>,
}

/// Julia value parameter represented inside a type expression.
///
/// Upstream Julia allows bits values as type parameters.  The current bridge
/// starts with the value shapes needed by Base aliases such as `Val{1}`,
/// `Array{T,2}`, and `NTuple{N,T}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CoreValueParam {
    Int(i64),
    SignedInt { bits: u16, value: i128 },
    UnsignedInt { bits: u16, value: u128 },
    Bool(bool),
    Symbol(String),
    String(String),
}

impl CoreValueParam {
    pub(super) fn to_julia_name(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::SignedInt { bits, value } => format!("Int{bits}({value})"),
            Self::UnsignedInt { bits, value } => {
                let width = usize::from(*bits) / 4;
                format!("0x{value:0width$x}")
            }
            Self::Bool(value) => value.to_string(),
            Self::Symbol(value) => format!(":{value}"),
            Self::String(value) => format!("\"{value}\""),
        }
    }
}

/// Shared structured type shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CoreType {
    Bottom,
    Any,
    Primitive(CorePrimitive),
    Abstract(CoreAbstract),
    AbstractUser {
        name: String,
        parent: Option<Box<CoreType>>,
    },
    Struct {
        name: String,
        params: Vec<CoreType>,
    },
    Tuple(Vec<CoreType>),
    Vararg(Box<CoreType>),
    VarargLen {
        element: Box<CoreType>,
        len: Box<CoreType>,
    },
    NamedTuple(Vec<(String, CoreType)>),
    Union(Vec<CoreType>),
    TypeOf(Box<CoreType>),
    TypeVar(CoreTypeVar),
    Value(CoreValueParam),
    UnionAll {
        var: CoreTypeVar,
        body: Box<CoreType>,
    },
    Module(String),
    /// Fallback for type names the current bridge cannot structure yet.
    Named(String),
}
