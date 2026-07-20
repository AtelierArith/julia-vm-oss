//! Operator types and conversions for AoT compilation.
//!
//! Contains binary, unary, compound assignment, and builtin operation types
//! along with their Display and From trait implementations.

use super::super::types::StaticType;
use std::fmt;

/// AoT binary operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AotBinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    // Comparison
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    Egal,    // ===
    NotEgal, // !==
    Subtype, // <:
    // Logical
    And,
    Or,
    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

/// Compound assignment operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundAssignOp {
    /// += (addition assignment)
    AddAssign,
    /// -= (subtraction assignment)
    SubAssign,
    /// *= (multiplication assignment)
    MulAssign,
    /// /= (division assignment)
    DivAssign,
    /// ÷= (integer division assignment)
    IntDivAssign,
    /// %= (modulo assignment)
    ModAssign,
    /// ^= (power assignment)
    PowAssign,
    /// &= (bitwise AND assignment)
    BitAndAssign,
    /// |= (bitwise OR assignment)
    BitOrAssign,
    /// ⊻= (bitwise XOR assignment)
    BitXorAssign,
    /// <<= (left shift assignment)
    ShlAssign,
    /// >>= (right shift assignment)
    ShrAssign,
}

impl CompoundAssignOp {
    /// Convert to Rust compound assignment operator string
    pub fn to_rust_op(&self) -> &'static str {
        match self {
            CompoundAssignOp::AddAssign => "+=",
            CompoundAssignOp::SubAssign => "-=",
            CompoundAssignOp::MulAssign => "*=",
            CompoundAssignOp::DivAssign => "/=",
            CompoundAssignOp::IntDivAssign => "/=", // Rust uses same operator
            CompoundAssignOp::ModAssign => "%=",
            CompoundAssignOp::PowAssign => "pow", // Needs special handling
            CompoundAssignOp::BitAndAssign => "&=",
            CompoundAssignOp::BitOrAssign => "|=",
            CompoundAssignOp::BitXorAssign => "^=",
            CompoundAssignOp::ShlAssign => "<<=",
            CompoundAssignOp::ShrAssign => ">>=",
        }
    }

    /// Check if this operator needs special handling (e.g., power)
    pub fn needs_special_handling(&self) -> bool {
        matches!(self, CompoundAssignOp::PowAssign)
    }

    /// Convert to corresponding binary operator
    pub fn to_binop(&self) -> AotBinOp {
        match self {
            CompoundAssignOp::AddAssign => AotBinOp::Add,
            CompoundAssignOp::SubAssign => AotBinOp::Sub,
            CompoundAssignOp::MulAssign => AotBinOp::Mul,
            CompoundAssignOp::DivAssign => AotBinOp::Div,
            CompoundAssignOp::IntDivAssign => AotBinOp::IntDiv,
            CompoundAssignOp::ModAssign => AotBinOp::Mod,
            CompoundAssignOp::PowAssign => AotBinOp::Pow,
            CompoundAssignOp::BitAndAssign => AotBinOp::BitAnd,
            CompoundAssignOp::BitOrAssign => AotBinOp::BitOr,
            CompoundAssignOp::BitXorAssign => AotBinOp::BitXor,
            CompoundAssignOp::ShlAssign => AotBinOp::Shl,
            CompoundAssignOp::ShrAssign => AotBinOp::Shr,
        }
    }
}

impl AotBinOp {
    /// Convert to Rust operator string
    pub fn to_rust_op(&self) -> &'static str {
        match self {
            AotBinOp::Add => "+",
            AotBinOp::Sub => "-",
            AotBinOp::Mul => "*",
            AotBinOp::Div => "/",
            AotBinOp::IntDiv => "/", // Integer division in Rust uses /
            AotBinOp::Mod => "%",
            AotBinOp::Pow => ".pow", // Needs special handling
            AotBinOp::Lt => "<",
            AotBinOp::Gt => ">",
            AotBinOp::Le => "<=",
            AotBinOp::Ge => ">=",
            AotBinOp::Eq => "==",
            AotBinOp::Ne => "!=",
            AotBinOp::Egal => "==",    // Object identity
            AotBinOp::NotEgal => "!=", // Not object identity
            AotBinOp::Subtype => "<:",
            AotBinOp::And => "&&",
            AotBinOp::Or => "||",
            AotBinOp::BitAnd => "&",
            AotBinOp::BitOr => "|",
            AotBinOp::BitXor => "^",
            AotBinOp::Shl => "<<",
            AotBinOp::Shr => ">>",
        }
    }

    /// Check if this is a comparison operator (returns bool)
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            AotBinOp::Lt
                | AotBinOp::Gt
                | AotBinOp::Le
                | AotBinOp::Ge
                | AotBinOp::Eq
                | AotBinOp::Ne
                | AotBinOp::Egal
                | AotBinOp::NotEgal
                | AotBinOp::Subtype
        )
    }

    /// Check if this is a logical operator
    pub fn is_logical(&self) -> bool {
        matches!(self, AotBinOp::And | AotBinOp::Or)
    }

    /// Check if this operator needs special handling (e.g., power)
    pub fn needs_special_handling(&self) -> bool {
        matches!(self, AotBinOp::Pow | AotBinOp::IntDiv)
    }
}

impl fmt::Display for AotBinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_rust_op())
    }
}

/// Convert from Core IR BinaryOp to AotBinOp
impl From<&crate::ir::core::BinaryOp> for AotBinOp {
    fn from(op: &crate::ir::core::BinaryOp) -> Self {
        use crate::ir::core::BinaryOp;
        match op {
            BinaryOp::Add => AotBinOp::Add,
            BinaryOp::Sub => AotBinOp::Sub,
            BinaryOp::Mul => AotBinOp::Mul,
            BinaryOp::Div => AotBinOp::Div,
            BinaryOp::IntDiv => AotBinOp::IntDiv,
            BinaryOp::Mod => AotBinOp::Mod,
            BinaryOp::Pow => AotBinOp::Pow,
            BinaryOp::Lt => AotBinOp::Lt,
            BinaryOp::Gt => AotBinOp::Gt,
            BinaryOp::Le => AotBinOp::Le,
            BinaryOp::Ge => AotBinOp::Ge,
            BinaryOp::Eq => AotBinOp::Eq,
            BinaryOp::Ne => AotBinOp::Ne,
            BinaryOp::Egal => AotBinOp::Egal,
            BinaryOp::NotEgal => AotBinOp::NotEgal,
            BinaryOp::Subtype => AotBinOp::Subtype,
            BinaryOp::And => AotBinOp::And,
            BinaryOp::Or => AotBinOp::Or,
        }
    }
}

/// AoT unary operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AotUnaryOp {
    /// Negation: -x
    Neg,
    /// Logical not: !x
    Not,
    /// Unary plus: +x (identity)
    Pos,
    /// Bitwise not: ~x
    BitNot,
}

impl AotUnaryOp {
    /// Convert to Rust operator string
    pub fn to_rust_op(&self) -> &'static str {
        match self {
            AotUnaryOp::Neg => "-",
            AotUnaryOp::Not => "!",
            AotUnaryOp::Pos => "+", // Usually a no-op
            AotUnaryOp::BitNot => "!",
        }
    }
}

impl fmt::Display for AotUnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_rust_op())
    }
}

/// Convert from Core IR UnaryOp to AotUnaryOp
impl From<&crate::ir::core::UnaryOp> for AotUnaryOp {
    fn from(op: &crate::ir::core::UnaryOp) -> Self {
        use crate::ir::core::UnaryOp;
        match op {
            UnaryOp::Neg => AotUnaryOp::Neg,
            UnaryOp::Not => AotUnaryOp::Not,
            UnaryOp::Pos => AotUnaryOp::Pos,
        }
    }
}

/// AoT builtin operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AotBuiltinOp {
    // Basic math functions
    Sqrt,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2, // Two-argument arctangent
    Exp,
    Log,
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    Min,
    Max,
    Clamp,
    Sign,
    Signbit,
    Copysign,
    /// isless(a, b) — Julia's canonical total order over real numbers:
    /// `<` for integers/Bool; for floats NaN sorts after everything and
    /// -0.0 before 0.0 (upstream julia/base/float.jl `_fpint`), Issue #10131.
    IsLess,
    // Integer math
    Div, // Integer division
    Mod, // Modulo (Euclidean)
    Rem, // Remainder
    Fld, // Floored division
    Cld, // Ceiling division
    // Note: Gcd, Lcm removed - now Pure Julia (base/intfuncs.jl)

    // Special value checks
    Isnan,
    Isinf,
    Isfinite,

    // Array operations
    Length,
    Size,
    Ndims,
    Push,
    Pop,
    PushFirst,
    PopFirst,
    /// insert!(arr, i, x) - Insert element at position
    Insert,
    /// deleteat!(arr, i) - Delete element at position
    DeleteAt,
    /// append!(arr, other) - Append another array
    Append,
    /// first(arr) - Get first element of array
    First,
    /// last(arr) - Get last element of array
    Last,
    /// first(tuple) - Get first element of tuple
    TupleFirst,
    /// last(tuple) - Get last element of tuple
    TupleLast,
    /// isempty(arr) - Check if array is empty
    IsEmpty,
    /// in(x, collection) - Membership test
    In,
    /// Dict(args...) - construct a Dict from Pair arguments
    Dict,
    /// haskey(dict, key) - Check whether a key exists
    HasKey,
    /// get(dict, key, default) - Lookup with a default
    DictGet,
    /// collect(iter) - Collect iterator into array
    Collect,
    Zeros,
    Ones,
    // Note: Fill removed — now Pure Julia (Issue #2640)
    Reshape,
    Sum,

    // Higher-order functions
    /// map(f, arr) - Apply function to each element
    Map,
    /// filter(f, arr) - Filter elements by predicate
    Filter,
    /// reduce(f, arr) - Left fold over array
    Reduce,
    /// mapreduce(f, op, arr) - Map each element, then reduce with op
    MapReduce,
    /// foreach(f, arr) - Apply function for side effects
    ForEach,
    /// any(f, arr) - Check if any element satisfies predicate
    Any,
    /// all(f, arr) - Check if all elements satisfy predicate
    All,

    // String operations
    StringLength,
    Uppercase,
    Lowercase,
    Occursin,
    StartsWith,
    EndsWith,

    // I/O operations
    Println,
    Print,
    TimeNs,

    // Type operations
    TypeOf,
    Isa,

    // Random
    Rand,
    Randn,

    // Type conversion intrinsics
    Sitofp, // Signed int to floating point
    Fptosi, // Floating point to signed int

    // Error handling (Issue #3406)
    /// throw(x) — emits panic! in generated Rust code
    Throw,

    // String operations (Issue #3405)
    /// string(args...) — concatenates arguments into a String
    StringConcat,

    // Complex number operations (Issue #3410)
    /// abs2(z) — squared absolute value: re^2 + im^2
    Abs2,
    /// real(z) — real part of complex number
    Real,
    /// imag(z) — imaginary part of complex number
    Imag,

    // Transpose (Issue #3410)
    /// adjoint(x) — transpose/conjugate transpose (identity for 1D)
    Adjoint,

    // Range construction (Issue #3413)
    /// linspace(start, stop, n) — linearly spaced vector
    Linspace,
}

impl AotBuiltinOp {
    /// Get the return type for this builtin given argument types
    pub fn return_type(&self, arg_types: &[StaticType]) -> StaticType {
        match self {
            // Float-returning math functions
            AotBuiltinOp::Sqrt
            | AotBuiltinOp::Sin
            | AotBuiltinOp::Cos
            | AotBuiltinOp::Tan
            | AotBuiltinOp::Asin
            | AotBuiltinOp::Acos
            | AotBuiltinOp::Atan
            | AotBuiltinOp::Atan2
            | AotBuiltinOp::Exp
            | AotBuiltinOp::Log => StaticType::F64,

            AotBuiltinOp::Rand | AotBuiltinOp::Randn => {
                if arg_types.is_empty() {
                    StaticType::F64
                } else {
                    StaticType::Array {
                        element: Box::new(StaticType::F64),
                        ndims: Some(arg_types.len()),
                    }
                }
            }

            // Integer-returning functions
            AotBuiltinOp::Length
            | AotBuiltinOp::Ndims
            | AotBuiltinOp::StringLength
            | AotBuiltinOp::TimeNs
            // Note: Gcd, Lcm removed - now Pure Julia (base/intfuncs.jl)
            => StaticType::I64,

            // Type-preserving functions (return same type as input)
            AotBuiltinOp::Abs => arg_types.first().cloned().unwrap_or(StaticType::F64),
            // min/max/clamp return the PROMOTED common numeric type
            // (`min(Int64, Float64)` is `Float64`, Issue #10131); same-type
            // calls keep their type, and Bool mixes follow the shared
            // Bool-promotion rule below.
            AotBuiltinOp::Min | AotBuiltinOp::Max | AotBuiltinOp::Clamp => {
                if let Some(promoted) = StaticType::promote_numeric_args(arg_types) {
                    promoted
                } else if matches!(arg_types, [StaticType::Bool, StaticType::Bool, ..]) {
                    StaticType::Bool
                } else if arg_types.iter().any(|ty| matches!(ty, StaticType::Bool)) {
                    arg_types
                        .iter()
                        .find(|ty| ty.is_integer() && !matches!(ty, StaticType::Bool))
                        .cloned()
                        .unwrap_or(StaticType::I64)
                } else {
                    arg_types.first().cloned().unwrap_or(StaticType::F64)
                }
            }
            AotBuiltinOp::Floor
            | AotBuiltinOp::Ceil
            | AotBuiltinOp::Round
            | AotBuiltinOp::Trunc
            | AotBuiltinOp::Sign
            | AotBuiltinOp::Copysign
            | AotBuiltinOp::Div
            | AotBuiltinOp::Mod
            | AotBuiltinOp::Rem
            | AotBuiltinOp::Fld
            | AotBuiltinOp::Cld => {
                if matches!(arg_types, [StaticType::Bool, StaticType::Bool, ..]) {
                    StaticType::Bool
                } else if arg_types.iter().any(|ty| matches!(ty, StaticType::Bool)) {
                    arg_types
                        .iter()
                        .find(|ty| ty.is_integer() && !matches!(ty, StaticType::Bool))
                        .cloned()
                        .unwrap_or(StaticType::I64)
                } else {
                    arg_types.first().cloned().unwrap_or(StaticType::F64)
                }
            }

            // Boolean-returning special value checks
            AotBuiltinOp::Isnan
            | AotBuiltinOp::Isinf
            | AotBuiltinOp::Isfinite
            | AotBuiltinOp::Signbit
            | AotBuiltinOp::IsLess => StaticType::Bool,

            // Array-returning functions
            AotBuiltinOp::Zeros | AotBuiltinOp::Ones => StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(if arg_types.is_empty() { 1 } else { arg_types.len() }),
            },
            AotBuiltinOp::Reshape => StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: None,
            },

            // Sum returns the array element type, or sum(f, arr)'s mapped element type.
            AotBuiltinOp::Sum => match arg_types {
                [StaticType::Array { element, .. }]
                | [StaticType::Range { element }]
                | [StaticType::Generator { element }] => element.as_ref().clone(),
                [StaticType::Function { ret, .. }, StaticType::Array { .. }]
                | [StaticType::Function { ret, .. }, StaticType::Range { .. }]
                | [StaticType::Function { ret, .. }, StaticType::Generator { .. }] => {
                    ret.as_ref().clone()
                }
                _ => StaticType::F64,
            },

            // Size returns tuple
            AotBuiltinOp::Size => StaticType::Tuple(vec![StaticType::I64]),

            // Push/Pop return array or element
            AotBuiltinOp::Push | AotBuiltinOp::PushFirst | AotBuiltinOp::Insert => {
                arg_types.first().cloned().unwrap_or(StaticType::Array {
                    element: Box::new(StaticType::Any),
                    ndims: Some(1),
                })
            }
            AotBuiltinOp::Pop | AotBuiltinOp::PopFirst | AotBuiltinOp::DeleteAt => StaticType::Any,

            // Append returns the mutated array
            AotBuiltinOp::Append => StaticType::Array {
                element: Box::new(StaticType::Any),
                ndims: Some(1),
            },
            AotBuiltinOp::Dict => arg_types.first().cloned().unwrap_or(StaticType::Dict {
                key: Box::new(StaticType::Any),
                value: Box::new(StaticType::Any),
            }),
            AotBuiltinOp::DictGet => match arg_types {
                [StaticType::Dict { value, .. }, _, default, ..] => {
                    if default == value.as_ref() {
                        value.as_ref().clone()
                    } else {
                        StaticType::Union {
                            variants: vec![value.as_ref().clone(), default.clone()],
                        }
                    }
                }
                [StaticType::Dict { value, .. }, ..] => value.as_ref().clone(),
                [_, _, default, ..] => default.clone(),
                _ => StaticType::Any,
            },
            AotBuiltinOp::Collect => match arg_types.first() {
                Some(StaticType::Array { element, .. })
                | Some(StaticType::Range { element })
                | Some(StaticType::Generator { element })
                | Some(StaticType::Set { element }) => StaticType::Array {
                    element: element.clone(),
                    ndims: Some(1),
                },
                Some(StaticType::Dict { key, value }) => StaticType::Array {
                    element: Box::new(StaticType::Tuple(vec![
                        key.as_ref().clone(),
                        value.as_ref().clone(),
                    ])),
                    ndims: Some(1),
                },
                _ => StaticType::Array {
                    element: Box::new(StaticType::Any),
                    ndims: Some(1),
                },
            },

            // Element access
            AotBuiltinOp::First
            | AotBuiltinOp::Last
            | AotBuiltinOp::TupleFirst
            | AotBuiltinOp::TupleLast => StaticType::Any, // Element type

            // Boolean predicates
            AotBuiltinOp::IsEmpty | AotBuiltinOp::In | AotBuiltinOp::HasKey => StaticType::Bool,

            // Higher-order functions
            AotBuiltinOp::Map => match (arg_types.first(), arg_types.get(1)) {
                (Some(StaticType::Function { ret, .. }), Some(StaticType::Array { ndims, .. })) => {
                    StaticType::Array {
                        element: ret.clone(),
                        ndims: Some(ndims.unwrap_or(1)),
                    }
                }
                (
                    Some(StaticType::Function { ret, .. }),
                    Some(StaticType::Range { .. } | StaticType::Generator { .. }),
                ) => StaticType::Array {
                    element: ret.clone(),
                    ndims: Some(1),
                },
                _ => StaticType::Array {
                    element: Box::new(StaticType::Any),
                    ndims: Some(1),
                },
            },
            AotBuiltinOp::Filter => match arg_types.get(1) {
                Some(StaticType::Array { element, ndims }) => StaticType::Array {
                    element: element.clone(),
                    ndims: *ndims,
                },
                Some(StaticType::Range { element } | StaticType::Generator { element }) => {
                    StaticType::Array {
                        element: element.clone(),
                        ndims: Some(1),
                    }
                }
                _ => StaticType::Array {
                    element: Box::new(StaticType::Any),
                    ndims: Some(1),
                },
            },
            AotBuiltinOp::Reduce => match (arg_types.first(), arg_types.get(1)) {
                (Some(StaticType::Function { ret, .. }), _) if !matches!(ret.as_ref(), StaticType::Any) => {
                    ret.as_ref().clone()
                }
                (_, Some(StaticType::Array { element, .. } | StaticType::Range { element } | StaticType::Generator { element })) => {
                    element.as_ref().clone()
                }
                _ => StaticType::Any,
            },
            AotBuiltinOp::MapReduce => match arg_types.first() {
                Some(StaticType::Function { ret, .. }) => match arg_types.get(1) {
                    Some(StaticType::Function { ret: op_ret, .. }) => op_ret.as_ref().clone(),
                    _ => ret.as_ref().clone(),
                },
                _ => StaticType::Any,
            },
            AotBuiltinOp::ForEach => StaticType::Nothing,
            AotBuiltinOp::Any | AotBuiltinOp::All => StaticType::Bool,

            // String operations
            AotBuiltinOp::Uppercase | AotBuiltinOp::Lowercase => StaticType::Str,
            AotBuiltinOp::Occursin | AotBuiltinOp::StartsWith | AotBuiltinOp::EndsWith => {
                StaticType::Bool
            }

            // I/O operations return nothing
            AotBuiltinOp::Println | AotBuiltinOp::Print => StaticType::Nothing,

            // Type operations
            AotBuiltinOp::TypeOf => StaticType::DataType,
            AotBuiltinOp::Isa => StaticType::Bool,

            // Type conversion intrinsics
            AotBuiltinOp::Sitofp => StaticType::F64,
            AotBuiltinOp::Fptosi => StaticType::I64,

            // Error handling — throw never returns (but we model as Nothing)
            AotBuiltinOp::Throw => StaticType::Nothing,

            // String concatenation
            AotBuiltinOp::StringConcat => StaticType::Str,

            // Complex number operations (Issue #3410, #7041)
            AotBuiltinOp::Abs2 | AotBuiltinOp::Real | AotBuiltinOp::Imag => arg_types
                .first()
                .and_then(|ty| match ty {
                    StaticType::Struct { name, .. } => {
                        StaticType::complex_param_type_from_name(name)
                    }
                    _ => None,
                })
                .unwrap_or(StaticType::F64),

            // Transpose — returns same type as input
            AotBuiltinOp::Adjoint => {
                if arg_types.is_empty() {
                    StaticType::Any
                } else {
                    arg_types[0].clone()
                }
            }

            // Linspace returns Vec<f64>
            AotBuiltinOp::Linspace => StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(1),
            },
        }
    }

    /// Convert builtin name to AotBuiltinOp
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            // Basic math functions
            "sqrt" => Some(AotBuiltinOp::Sqrt),
            "sin" => Some(AotBuiltinOp::Sin),
            "cos" => Some(AotBuiltinOp::Cos),
            "tan" => Some(AotBuiltinOp::Tan),
            "asin" => Some(AotBuiltinOp::Asin),
            "acos" => Some(AotBuiltinOp::Acos),
            "atan" => Some(AotBuiltinOp::Atan),
            "atan2" => Some(AotBuiltinOp::Atan2),
            "exp" => Some(AotBuiltinOp::Exp),
            "log" => Some(AotBuiltinOp::Log),
            "abs" => Some(AotBuiltinOp::Abs),
            "floor" => Some(AotBuiltinOp::Floor),
            "ceil" => Some(AotBuiltinOp::Ceil),
            "round" => Some(AotBuiltinOp::Round),
            "trunc" => Some(AotBuiltinOp::Trunc),
            "isless" => Some(AotBuiltinOp::IsLess),
            "min" => Some(AotBuiltinOp::Min),
            "max" => Some(AotBuiltinOp::Max),
            "clamp" => Some(AotBuiltinOp::Clamp),
            "sign" => Some(AotBuiltinOp::Sign),
            "signbit" => Some(AotBuiltinOp::Signbit),
            "copysign" => Some(AotBuiltinOp::Copysign),

            // Integer math
            "div" => Some(AotBuiltinOp::Div),
            "mod" => Some(AotBuiltinOp::Mod),
            "rem" => Some(AotBuiltinOp::Rem),
            "fld" => Some(AotBuiltinOp::Fld),
            "cld" => Some(AotBuiltinOp::Cld),
            // Note: gcd, lcm removed - now Pure Julia (base/intfuncs.jl)

            // Special value checks
            "isnan" => Some(AotBuiltinOp::Isnan),
            "isinf" => Some(AotBuiltinOp::Isinf),
            "isfinite" => Some(AotBuiltinOp::Isfinite),

            // Array operations
            "length" => Some(AotBuiltinOp::Length),
            "size" => Some(AotBuiltinOp::Size),
            "ndims" => Some(AotBuiltinOp::Ndims),
            "push!" => Some(AotBuiltinOp::Push),
            "pop!" => Some(AotBuiltinOp::Pop),
            "pushfirst!" => Some(AotBuiltinOp::PushFirst),
            "popfirst!" => Some(AotBuiltinOp::PopFirst),
            "insert!" => Some(AotBuiltinOp::Insert),
            "deleteat!" => Some(AotBuiltinOp::DeleteAt),
            "append!" => Some(AotBuiltinOp::Append),
            "first" => Some(AotBuiltinOp::First),
            "last" => Some(AotBuiltinOp::Last),
            "isempty" => Some(AotBuiltinOp::IsEmpty),
            "in" | "∈" => Some(AotBuiltinOp::In),
            "Dict" => Some(AotBuiltinOp::Dict),
            "haskey" => Some(AotBuiltinOp::HasKey),
            "get" => Some(AotBuiltinOp::DictGet),
            "collect" => Some(AotBuiltinOp::Collect),
            "zeros" => Some(AotBuiltinOp::Zeros),
            "ones" => Some(AotBuiltinOp::Ones),
            "reshape" => Some(AotBuiltinOp::Reshape),
            "sum" => Some(AotBuiltinOp::Sum),

            // Higher-order functions
            "map" => Some(AotBuiltinOp::Map),
            "filter" => Some(AotBuiltinOp::Filter),
            "reduce" | "foldl" => Some(AotBuiltinOp::Reduce),
            "mapreduce" => Some(AotBuiltinOp::MapReduce),
            "foreach" => Some(AotBuiltinOp::ForEach),
            "any" => Some(AotBuiltinOp::Any),
            "all" => Some(AotBuiltinOp::All),

            // I/O and misc
            "println" => Some(AotBuiltinOp::Println),
            "print" => Some(AotBuiltinOp::Print),
            "time_ns" => Some(AotBuiltinOp::TimeNs),
            "typeof" => Some(AotBuiltinOp::TypeOf),
            "isa" => Some(AotBuiltinOp::Isa),
            "rand" => Some(AotBuiltinOp::Rand),
            "randn" => Some(AotBuiltinOp::Randn),
            "uppercase" => Some(AotBuiltinOp::Uppercase),
            "lowercase" => Some(AotBuiltinOp::Lowercase),
            "occursin" => Some(AotBuiltinOp::Occursin),
            "startswith" => Some(AotBuiltinOp::StartsWith),
            "endswith" => Some(AotBuiltinOp::EndsWith),

            // Type conversion intrinsics
            "sitofp" => Some(AotBuiltinOp::Sitofp),
            "fptosi" => Some(AotBuiltinOp::Fptosi),

            // String concatenation (Issue #3405)
            "string" => Some(AotBuiltinOp::StringConcat),

            // Complex number operations (Issue #3410)
            "abs2" => Some(AotBuiltinOp::Abs2),
            "real" => Some(AotBuiltinOp::Real),
            "imag" => Some(AotBuiltinOp::Imag),

            // Transpose (Issue #3410)
            "adjoint" => Some(AotBuiltinOp::Adjoint),

            _ => None,
        }
    }
}

impl fmt::Display for AotBuiltinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            // Basic math
            AotBuiltinOp::Sqrt => "sqrt",
            AotBuiltinOp::Sin => "sin",
            AotBuiltinOp::Cos => "cos",
            AotBuiltinOp::Tan => "tan",
            AotBuiltinOp::Asin => "asin",
            AotBuiltinOp::Acos => "acos",
            AotBuiltinOp::Atan => "atan",
            AotBuiltinOp::Atan2 => "atan2",
            AotBuiltinOp::Exp => "exp",
            AotBuiltinOp::Log => "log",
            AotBuiltinOp::Abs => "abs",
            AotBuiltinOp::Floor => "floor",
            AotBuiltinOp::Ceil => "ceil",
            AotBuiltinOp::Round => "round",
            AotBuiltinOp::Trunc => "trunc",
            AotBuiltinOp::IsLess => "isless",
            AotBuiltinOp::Min => "min",
            AotBuiltinOp::Max => "max",
            AotBuiltinOp::Clamp => "clamp",
            AotBuiltinOp::Sign => "sign",
            AotBuiltinOp::Signbit => "signbit",
            AotBuiltinOp::Copysign => "copysign",

            // Integer math
            AotBuiltinOp::Div => "div",
            AotBuiltinOp::Mod => "mod",
            AotBuiltinOp::Rem => "rem",
            AotBuiltinOp::Fld => "fld",
            AotBuiltinOp::Cld => "cld",
            // Note: gcd, lcm removed - now Pure Julia (base/intfuncs.jl)

            // Special value checks
            AotBuiltinOp::Isnan => "isnan",
            AotBuiltinOp::Isinf => "isinf",
            AotBuiltinOp::Isfinite => "isfinite",

            // Array operations
            AotBuiltinOp::Length => "length",
            AotBuiltinOp::Size => "size",
            AotBuiltinOp::Ndims => "ndims",
            AotBuiltinOp::Push => "push!",
            AotBuiltinOp::Pop => "pop!",
            AotBuiltinOp::PushFirst => "pushfirst!",
            AotBuiltinOp::PopFirst => "popfirst!",
            AotBuiltinOp::Insert => "insert!",
            AotBuiltinOp::DeleteAt => "deleteat!",
            AotBuiltinOp::Append => "append!",
            AotBuiltinOp::First => "first",
            AotBuiltinOp::Last => "last",
            AotBuiltinOp::TupleFirst => "first",
            AotBuiltinOp::TupleLast => "last",
            AotBuiltinOp::IsEmpty => "isempty",
            AotBuiltinOp::In => "in",
            AotBuiltinOp::Dict => "Dict",
            AotBuiltinOp::HasKey => "haskey",
            AotBuiltinOp::DictGet => "get",
            AotBuiltinOp::Collect => "collect",
            AotBuiltinOp::Zeros => "zeros",
            AotBuiltinOp::Ones => "ones",
            AotBuiltinOp::Reshape => "reshape",
            AotBuiltinOp::Sum => "sum",

            // Higher-order functions
            AotBuiltinOp::Map => "map",
            AotBuiltinOp::Filter => "filter",
            AotBuiltinOp::Reduce => "reduce",
            AotBuiltinOp::MapReduce => "mapreduce",
            AotBuiltinOp::ForEach => "foreach",
            AotBuiltinOp::Any => "any",
            AotBuiltinOp::All => "all",

            // I/O and misc
            AotBuiltinOp::Println => "println",
            AotBuiltinOp::Print => "print",
            AotBuiltinOp::TimeNs => "time_ns",
            AotBuiltinOp::TypeOf => "typeof",
            AotBuiltinOp::Isa => "isa",
            AotBuiltinOp::Rand => "rand",
            AotBuiltinOp::Randn => "randn",
            AotBuiltinOp::Uppercase => "uppercase",
            AotBuiltinOp::Lowercase => "lowercase",
            AotBuiltinOp::Occursin => "occursin",
            AotBuiltinOp::StartsWith => "startswith",
            AotBuiltinOp::EndsWith => "endswith",
            AotBuiltinOp::StringLength => "length",

            // Type conversion intrinsics
            AotBuiltinOp::Sitofp => "sitofp",
            AotBuiltinOp::Fptosi => "fptosi",

            // Error handling + string concat
            AotBuiltinOp::Throw => "throw",
            AotBuiltinOp::StringConcat => "string",

            // Complex number operations
            AotBuiltinOp::Abs2 => "abs2",
            AotBuiltinOp::Real => "real",
            AotBuiltinOp::Imag => "imag",

            // Transpose
            AotBuiltinOp::Adjoint => "adjoint",

            // Linspace
            AotBuiltinOp::Linspace => "linspace",
        };
        write!(f, "{}", name)
    }
}
