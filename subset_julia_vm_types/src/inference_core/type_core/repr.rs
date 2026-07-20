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

    /// Return the canonical Julia type name for this primitive (e.g. `"Int64"`).
    ///
    /// Used by the first-arg dispatch bucket index (Issue #9112) to derive a
    /// stable string key from a `CoreType::Primitive` without allocating.
    pub fn julia_name(&self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::Int8 => "Int8",
            Self::Int16 => "Int16",
            Self::Int32 => "Int32",
            Self::Int64 => "Int64",
            Self::Int128 => "Int128",
            Self::UInt8 => "UInt8",
            Self::UInt16 => "UInt16",
            Self::UInt32 => "UInt32",
            Self::UInt64 => "UInt64",
            Self::UInt128 => "UInt128",
            Self::Float16 => "Float16",
            Self::Float32 => "Float32",
            Self::Float64 => "Float64",
            Self::BigInt => "BigInt",
            Self::BigFloat => "BigFloat",
            Self::String => "String",
            Self::Char => "Char",
            Self::Symbol => "Symbol",
            Self::Nothing => "Nothing",
            Self::Missing => "Missing",
        }
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
    OrdinalRange,
}

impl CoreAbstract {
    /// Return the canonical Julia type name for this abstract type (e.g.
    /// `"Real"`). Mirrors [`CorePrimitive::julia_name`]; used by
    /// `rebind_where_binders` to recognize a `where`-binder shadowing a
    /// builtin abstract type name (Issue #10100 / epic #10049).
    pub fn julia_name(&self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Real => "Real",
            Self::Integer => "Integer",
            Self::Signed => "Signed",
            Self::Unsigned => "Unsigned",
            Self::AbstractFloat => "AbstractFloat",
            Self::AbstractString => "AbstractString",
            Self::AbstractChar => "AbstractChar",
            Self::AbstractArray => "AbstractArray",
            Self::AbstractVector => "AbstractVector",
            Self::AbstractMatrix => "AbstractMatrix",
            Self::DenseArray => "DenseArray",
            Self::AbstractDict => "AbstractDict",
            Self::AbstractSet => "AbstractSet",
            Self::AbstractRange => "AbstractRange",
            Self::AbstractUnitRange => "AbstractUnitRange",
            Self::Function => "Function",
            Self::Builtin => "Builtin",
            Self::IO => "IO",
            Self::Type => "Type",
            Self::DataType => "DataType",
            Self::OrdinalRange => "OrdinalRange",
        }
    }
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

/// Core-owned function bindings that are not visible as the unqualified Core
/// export in a source module. Ordinary modules may instead receive a Base
/// wrapper/export of the same spelling; `baremodule` must not (Issue #11410).
const CORE_HIDDEN_FUNCTION_BINDING_NAMES: &[&str] = &[
    "ifelse",
    "sizeof",
    "getproperty",
    "setproperty!",
    "finalizer",
    "memoryref",
    "memoryrefnew",
    "memoryrefget",
    "memoryrefset!",
    "memoryrefoffset",
    "print",
    "println",
    "write",
    ">:",
    "convert",
    "iterate",
    "eval",
    "include",
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

/// True iff `Core` itself owns a function binding with this spelling.
/// Unlike [`is_core_builtin_function_name`], this includes qualified-only
/// Core bindings that are shadowed or re-exported through Base.
pub fn is_core_function_binding_name(name: &str) -> bool {
    is_core_builtin_function_name(name) || CORE_HIDDEN_FUNCTION_BINDING_NAMES.contains(&name)
}

/// Owner-scoped identity projection for a [`CoreTypeVar`].
///
/// This is the first typed-ID wrapper for the #10459 migration. `CoreTypeVar`
/// still serializes the legacy `scope_id` / `rigid_identity` fields for cache
/// compatibility, but semantic lookup code should ask for this projection
/// instead of rebuilding ad-hoc `(scope_id, rigid_identity)` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CoreTypeVarId {
    Unresolved,
    Scoped(u32),
    Rigid(u64),
}

/// Type variable with optional lower/upper bounds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CoreTypeVar {
    #[serde(default)]
    pub scope_id: u32,
    /// Identity of a free runtime TypeVar. `None` denotes a bindable pattern or
    /// an enclosing `UnionAll` binder; `Some` is rigid in semantic comparisons.
    #[serde(default)]
    pub rigid_identity: Option<u64>,
    pub name: String,
    pub lower_bound: Option<Box<CoreType>>,
    pub upper_bound: Option<Box<CoreType>>,
}

impl CoreTypeVar {
    pub const UNRESOLVED_SCOPE_ID: u32 = 0;

    pub fn unscoped(name: impl Into<String>) -> Self {
        Self {
            scope_id: Self::UNRESOLVED_SCOPE_ID,
            rigid_identity: None,
            name: name.into(),
            lower_bound: None,
            upper_bound: None,
        }
    }

    pub fn with_bounds(
        name: impl Into<String>,
        lower_bound: Option<Box<CoreType>>,
        upper_bound: Option<Box<CoreType>>,
    ) -> Self {
        Self {
            scope_id: Self::UNRESOLVED_SCOPE_ID,
            rigid_identity: None,
            name: name.into(),
            lower_bound,
            upper_bound,
        }
    }

    pub fn with_scope_id(mut self, scope_id: u32) -> Self {
        self.scope_id = scope_id;
        self
    }

    pub fn with_rigid_identity(mut self, identity: u64) -> Self {
        self.rigid_identity = Some(identity);
        self
    }

    pub fn is_rigid(&self) -> bool {
        self.rigid_identity.is_some()
    }

    pub fn typevar_id(&self) -> CoreTypeVarId {
        match (self.rigid_identity, self.scope_id) {
            (Some(id), _) => CoreTypeVarId::Rigid(id),
            (None, Self::UNRESOLVED_SCOPE_ID) => CoreTypeVarId::Unresolved,
            (None, scope_id) => CoreTypeVarId::Scoped(scope_id),
        }
    }
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
