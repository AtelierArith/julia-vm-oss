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
            // Workaround: Subtype (<:) has no dedicated AotBinOp variant; mapped to Lt as a structural placeholder (Issue #3342).
            // Subtype semantics differ significantly from Lt — AoT codegen emits incorrect comparisons.
            BinaryOp::Subtype => AotBinOp::Lt,
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
    // Integer math
    Div, // Integer division
    Mod, // Modulo (Euclidean)
    Rem, // Remainder
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

    // I/O operations
    Println,
    Print,

    // Type operations
    TypeOf,
    Isa,

    // Random
    Rand,
    Randn,

    // Type conversion intrinsics
    Sitofp, // Signed int to floating point
    Fptosi, // Floating point to signed int
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
            | AotBuiltinOp::Log
            | AotBuiltinOp::Rand
            | AotBuiltinOp::Randn => StaticType::F64,

            // Integer-returning functions
            AotBuiltinOp::Length
            | AotBuiltinOp::Ndims
            | AotBuiltinOp::StringLength
            // Note: Gcd, Lcm removed - now Pure Julia (base/intfuncs.jl)
            => StaticType::I64,

            // Type-preserving functions (return same type as input)
            AotBuiltinOp::Abs
            | AotBuiltinOp::Floor
            | AotBuiltinOp::Ceil
            | AotBuiltinOp::Round
            | AotBuiltinOp::Trunc
            | AotBuiltinOp::Min
            | AotBuiltinOp::Max
            | AotBuiltinOp::Clamp
            | AotBuiltinOp::Sign
            | AotBuiltinOp::Copysign
            | AotBuiltinOp::Div
            | AotBuiltinOp::Mod
            | AotBuiltinOp::Rem => {
                // Return type depends on input type; default to F64
                StaticType::F64
            }

            // Boolean-returning special value checks
            AotBuiltinOp::Isnan
            | AotBuiltinOp::Isinf
            | AotBuiltinOp::Isfinite
            | AotBuiltinOp::Signbit => StaticType::Bool,

            // Array-returning functions
            AotBuiltinOp::Zeros | AotBuiltinOp::Ones => StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(if arg_types.is_empty() { 1 } else { arg_types.len() }),
            },
            AotBuiltinOp::Reshape => StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: None,
            },

            // Sum returns same as element type
            AotBuiltinOp::Sum => StaticType::F64,

            // Size returns tuple
            AotBuiltinOp::Size => StaticType::Tuple(vec![StaticType::I64]),

            // Push/Pop return array or element
            AotBuiltinOp::Push | AotBuiltinOp::PushFirst | AotBuiltinOp::Insert => {
                StaticType::Array {
                    element: Box::new(StaticType::Any),
                    ndims: Some(1),
                }
            }
            AotBuiltinOp::Pop | AotBuiltinOp::PopFirst | AotBuiltinOp::DeleteAt => StaticType::Any,

            // Append returns the mutated array
            AotBuiltinOp::Append | AotBuiltinOp::Collect => StaticType::Array {
                element: Box::new(StaticType::Any),
                ndims: Some(1),
            },

            // Element access
            AotBuiltinOp::First
            | AotBuiltinOp::Last
            | AotBuiltinOp::TupleFirst
            | AotBuiltinOp::TupleLast => StaticType::Any, // Element type

            // Boolean predicates
            AotBuiltinOp::IsEmpty => StaticType::Bool,

            // Higher-order functions
            AotBuiltinOp::Map | AotBuiltinOp::Filter => StaticType::Array {
                element: Box::new(StaticType::Any),
                ndims: Some(1),
            },
            AotBuiltinOp::Reduce => StaticType::Any, // Depends on function and array type
            AotBuiltinOp::ForEach => StaticType::Nothing,
            AotBuiltinOp::Any | AotBuiltinOp::All => StaticType::Bool,

            // String operations
            AotBuiltinOp::Uppercase | AotBuiltinOp::Lowercase => StaticType::Str,

            // I/O operations return nothing
            AotBuiltinOp::Println | AotBuiltinOp::Print => StaticType::Nothing,

            // Type operations
            AotBuiltinOp::TypeOf => StaticType::Str, // Returns type name as string
            AotBuiltinOp::Isa => StaticType::Bool,

            // Type conversion intrinsics
            AotBuiltinOp::Sitofp => StaticType::F64,
            AotBuiltinOp::Fptosi => StaticType::I64,
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
            "collect" => Some(AotBuiltinOp::Collect),
            "zeros" => Some(AotBuiltinOp::Zeros),
            "ones" => Some(AotBuiltinOp::Ones),
            "reshape" => Some(AotBuiltinOp::Reshape),
            "sum" => Some(AotBuiltinOp::Sum),

            // Higher-order functions
            "map" => Some(AotBuiltinOp::Map),
            "filter" => Some(AotBuiltinOp::Filter),
            "reduce" | "foldl" => Some(AotBuiltinOp::Reduce),
            "foreach" => Some(AotBuiltinOp::ForEach),
            "any" => Some(AotBuiltinOp::Any),
            "all" => Some(AotBuiltinOp::All),

            // I/O and misc
            "println" => Some(AotBuiltinOp::Println),
            "print" => Some(AotBuiltinOp::Print),
            "typeof" => Some(AotBuiltinOp::TypeOf),
            "isa" => Some(AotBuiltinOp::Isa),
            "rand" => Some(AotBuiltinOp::Rand),
            "randn" => Some(AotBuiltinOp::Randn),
            "uppercase" => Some(AotBuiltinOp::Uppercase),
            "lowercase" => Some(AotBuiltinOp::Lowercase),

            // Type conversion intrinsics
            "sitofp" => Some(AotBuiltinOp::Sitofp),
            "fptosi" => Some(AotBuiltinOp::Fptosi),
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
            AotBuiltinOp::Collect => "collect",
            AotBuiltinOp::Zeros => "zeros",
            AotBuiltinOp::Ones => "ones",
            AotBuiltinOp::Reshape => "reshape",
            AotBuiltinOp::Sum => "sum",

            // Higher-order functions
            AotBuiltinOp::Map => "map",
            AotBuiltinOp::Filter => "filter",
            AotBuiltinOp::Reduce => "reduce",
            AotBuiltinOp::ForEach => "foreach",
            AotBuiltinOp::Any => "any",
            AotBuiltinOp::All => "all",

            // I/O and misc
            AotBuiltinOp::Println => "println",
            AotBuiltinOp::Print => "print",
            AotBuiltinOp::TypeOf => "typeof",
            AotBuiltinOp::Isa => "isa",
            AotBuiltinOp::Rand => "rand",
            AotBuiltinOp::Randn => "randn",
            AotBuiltinOp::Uppercase => "uppercase",
            AotBuiltinOp::Lowercase => "lowercase",
            AotBuiltinOp::StringLength => "length",

            // Type conversion intrinsics
            AotBuiltinOp::Sitofp => "sitofp",
            AotBuiltinOp::Fptosi => "fptosi",
        };
        write!(f, "{}", name)
    }
}
