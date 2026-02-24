//! Core intrinsics - CPU instructions that map directly to low-level operations.
//!
//! These correspond to Julia's `src/intrinsics.h` and represent the minimal set
//! of operations that cannot be decomposed further.
//!
//! Design principle: Intrinsics are the atoms of computation. Higher-level
//! operations (like `sin`, `map`) are built on top of these through Builtin
//! functions or Julia code.

use serde::{Deserialize, Serialize};

/// Core intrinsics - CPU instruction-level operations.
///
/// Naming follows Julia's convention from `src/intrinsics.h`:
/// - `_int` suffix for integer operations
/// - `_float` suffix for floating-point operations
/// - `s` prefix for signed operations (e.g., `slt` = signed less than)
/// - `_llvm` suffix for LLVM intrinsic-backed operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Intrinsic {
    // === Integer Arithmetic ===
    /// neg_int(x) -> -x
    NegInt,
    /// add_int(a, b) -> a + b
    AddInt,
    /// sub_int(a, b) -> a - b
    SubInt,
    /// mul_int(a, b) -> a * b
    MulInt,
    /// sdiv_int(a, b) -> a / b (signed division)
    SdivInt,
    /// srem_int(a, b) -> a % b (signed remainder)
    SremInt,

    // === Floating-Point Arithmetic ===
    /// neg_float(x) -> -x
    NegFloat,

    // === Runtime-Dispatched Operations ===
    /// neg_any(x) -> -x (runtime type dispatch: returns I64 for I64 input, F64 for F64)
    NegAny,
    /// add_float(a, b) -> a + b
    AddFloat,
    /// sub_float(a, b) -> a - b
    SubFloat,
    /// mul_float(a, b) -> a * b
    MulFloat,
    /// div_float(a, b) -> a / b
    DivFloat,
    /// pow_float(a, b) -> a ^ b
    PowFloat,

    // === Integer Comparisons ===
    /// eq_int(a, b) -> a == b
    EqInt,
    /// ne_int(a, b) -> a != b
    NeInt,
    /// slt_int(a, b) -> a < b (signed less than)
    SltInt,
    /// sle_int(a, b) -> a <= b (signed less or equal)
    SleInt,
    /// sgt_int(a, b) -> a > b (signed greater than)
    SgtInt,
    /// sge_int(a, b) -> a >= b (signed greater or equal)
    SgeInt,

    // === Floating-Point Comparisons ===
    /// eq_float(a, b) -> a == b
    EqFloat,
    /// ne_float(a, b) -> a != b
    NeFloat,
    /// lt_float(a, b) -> a < b
    LtFloat,
    /// le_float(a, b) -> a <= b
    LeFloat,
    /// gt_float(a, b) -> a > b
    GtFloat,
    /// ge_float(a, b) -> a >= b
    GeFloat,

    // === Bitwise Operations ===
    /// and_int(a, b) -> a & b
    AndInt,
    /// or_int(a, b) -> a | b
    OrInt,
    /// xor_int(a, b) -> a ^ b (xor)
    XorInt,
    /// not_int(x) -> ~x
    NotInt,
    /// shl_int(a, b) -> a << b (shift left)
    ShlInt,
    /// lshr_int(a, b) -> a >>> b (logical shift right)
    LshrInt,
    /// ashr_int(a, b) -> a >> b (arithmetic shift right)
    AshrInt,

    // === Type Conversions ===
    /// sitofp(x) -> convert signed int to float
    Sitofp,
    /// fptosi(x) -> convert float to signed int (truncate)
    Fptosi,

    // === Low-Level Math (CPU/FPU instructions) ===
    /// sqrt_llvm(x) -> sqrt(x) - maps to CPU sqrt instruction
    SqrtLlvm,
    /// floor_llvm(x) -> floor(x) - maps to CPU floor instruction
    FloorLlvm,
    /// ceil_llvm(x) -> ceil(x) - maps to CPU ceil instruction
    CeilLlvm,
    /// trunc_llvm(x) -> trunc(x) - round toward zero
    TruncLlvm,
    /// abs_float(x) -> |x|
    AbsFloat,
    /// copysign_float(a, b) -> copy sign of b to a
    CopysignFloat,

    // === BigInt Arithmetic ===
    /// neg_bigint(x) -> -x
    NegBigInt,
    /// add_bigint(a, b) -> a + b
    AddBigInt,
    /// sub_bigint(a, b) -> a - b
    SubBigInt,
    /// mul_bigint(a, b) -> a * b
    MulBigInt,
    /// div_bigint(a, b) -> a ÷ b (truncated division)
    DivBigInt,
    /// rem_bigint(a, b) -> a % b (remainder)
    RemBigInt,
    /// abs_bigint(x) -> |x|
    AbsBigInt,
    /// pow_bigint(base, exp) -> base^exp (BigInt exponentiation with Int64 exponent)
    PowBigInt,

    // === BigInt Comparisons ===
    /// eq_bigint(a, b) -> a == b
    EqBigInt,
    /// ne_bigint(a, b) -> a != b
    NeBigInt,
    /// lt_bigint(a, b) -> a < b
    LtBigInt,
    /// le_bigint(a, b) -> a <= b
    LeBigInt,
    /// gt_bigint(a, b) -> a > b
    GtBigInt,
    /// ge_bigint(a, b) -> a >= b
    GeBigInt,

    // === BigInt Conversions ===
    /// i64_to_bigint(x) -> BigInt(x)
    I64ToBigInt,
    /// bigint_to_i64(x) -> Int64(x) (may overflow)
    BigIntToI64,
    /// string_to_bigint(s) -> parse(BigInt, s)
    StringToBigInt,
    /// bigint_to_string(x) -> string(x)
    BigIntToString,

    // === BigFloat Arithmetic ===
    /// neg_bigfloat(x) -> -x
    NegBigFloat,
    /// add_bigfloat(a, b) -> a + b
    AddBigFloat,
    /// sub_bigfloat(a, b) -> a - b
    SubBigFloat,
    /// mul_bigfloat(a, b) -> a * b
    MulBigFloat,
    /// div_bigfloat(a, b) -> a / b
    DivBigFloat,
    /// abs_bigfloat(x) -> |x|
    AbsBigFloat,

    // === BigFloat Comparisons ===
    /// eq_bigfloat(a, b) -> a == b
    EqBigFloat,
    /// ne_bigfloat(a, b) -> a != b
    NeBigFloat,
    /// lt_bigfloat(a, b) -> a < b
    LtBigFloat,
    /// le_bigfloat(a, b) -> a <= b
    LeBigFloat,
    /// gt_bigfloat(a, b) -> a > b
    GtBigFloat,
    /// ge_bigfloat(a, b) -> a >= b
    GeBigFloat,

    // === BigFloat Conversions ===
    /// f64_to_bigfloat(x) -> BigFloat(x)
    F64ToBigFloat,
    /// bigfloat_to_f64(x) -> Float64(x)
    BigFloatToF64,
    /// string_to_bigfloat(s) -> parse(BigFloat, s)
    StringToBigFloat,
    /// bigfloat_to_string(x) -> string(x)
    BigFloatToString,
}

impl Intrinsic {
    /// Get the intrinsic from a function name.
    ///
    /// # Examples
    /// ```
    /// use subset_julia_vm::intrinsics::Intrinsic;
    /// assert_eq!(Intrinsic::from_name("add_int"), Some(Intrinsic::AddInt));
    /// assert_eq!(Intrinsic::from_name("sqrt_llvm"), Some(Intrinsic::SqrtLlvm));
    /// ```
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            // Integer arithmetic
            "neg_int" => Some(Self::NegInt),
            "add_int" => Some(Self::AddInt),
            "sub_int" => Some(Self::SubInt),
            "mul_int" => Some(Self::MulInt),
            "sdiv_int" => Some(Self::SdivInt),
            "srem_int" => Some(Self::SremInt),

            // Float arithmetic
            "neg_float" => Some(Self::NegFloat),
            // Runtime-dispatched
            "neg_any" => Some(Self::NegAny),
            "add_float" => Some(Self::AddFloat),
            "sub_float" => Some(Self::SubFloat),
            "mul_float" => Some(Self::MulFloat),
            "div_float" => Some(Self::DivFloat),
            "pow_float" => Some(Self::PowFloat),

            // Integer comparisons
            "eq_int" => Some(Self::EqInt),
            "ne_int" => Some(Self::NeInt),
            "slt_int" => Some(Self::SltInt),
            "sle_int" => Some(Self::SleInt),
            "sgt_int" => Some(Self::SgtInt),
            "sge_int" => Some(Self::SgeInt),

            // Float comparisons
            "eq_float" => Some(Self::EqFloat),
            "ne_float" => Some(Self::NeFloat),
            "lt_float" => Some(Self::LtFloat),
            "le_float" => Some(Self::LeFloat),
            "gt_float" => Some(Self::GtFloat),
            "ge_float" => Some(Self::GeFloat),

            // Bitwise
            "and_int" => Some(Self::AndInt),
            "or_int" => Some(Self::OrInt),
            "xor_int" => Some(Self::XorInt),
            "not_int" => Some(Self::NotInt),
            "shl_int" => Some(Self::ShlInt),
            "lshr_int" => Some(Self::LshrInt),
            "ashr_int" => Some(Self::AshrInt),

            // Type conversions
            "sitofp" => Some(Self::Sitofp),
            "fptosi" => Some(Self::Fptosi),

            // Low-level math
            "sqrt_llvm" => Some(Self::SqrtLlvm),
            "floor_llvm" => Some(Self::FloorLlvm),
            "ceil_llvm" => Some(Self::CeilLlvm),
            "trunc_llvm" => Some(Self::TruncLlvm),
            "abs_float" => Some(Self::AbsFloat),
            "copysign_float" => Some(Self::CopysignFloat),

            // BigInt arithmetic
            "neg_bigint" => Some(Self::NegBigInt),
            "add_bigint" => Some(Self::AddBigInt),
            "sub_bigint" => Some(Self::SubBigInt),
            "mul_bigint" => Some(Self::MulBigInt),
            "div_bigint" => Some(Self::DivBigInt),
            "rem_bigint" => Some(Self::RemBigInt),
            "abs_bigint" => Some(Self::AbsBigInt),
            "pow_bigint" => Some(Self::PowBigInt),

            // BigInt comparisons
            "eq_bigint" => Some(Self::EqBigInt),
            "ne_bigint" => Some(Self::NeBigInt),
            "lt_bigint" => Some(Self::LtBigInt),
            "le_bigint" => Some(Self::LeBigInt),
            "gt_bigint" => Some(Self::GtBigInt),
            "ge_bigint" => Some(Self::GeBigInt),

            // BigInt conversions
            "i64_to_bigint" => Some(Self::I64ToBigInt),
            "bigint_to_i64" => Some(Self::BigIntToI64),
            "string_to_bigint" => Some(Self::StringToBigInt),
            "bigint_to_string" => Some(Self::BigIntToString),

            // BigFloat arithmetic
            "neg_bigfloat" => Some(Self::NegBigFloat),
            "add_bigfloat" => Some(Self::AddBigFloat),
            "sub_bigfloat" => Some(Self::SubBigFloat),
            "mul_bigfloat" => Some(Self::MulBigFloat),
            "div_bigfloat" => Some(Self::DivBigFloat),
            "abs_bigfloat" => Some(Self::AbsBigFloat),

            // BigFloat comparisons
            "eq_bigfloat" => Some(Self::EqBigFloat),
            "ne_bigfloat" => Some(Self::NeBigFloat),
            "lt_bigfloat" => Some(Self::LtBigFloat),
            "le_bigfloat" => Some(Self::LeBigFloat),
            "gt_bigfloat" => Some(Self::GtBigFloat),
            "ge_bigfloat" => Some(Self::GeBigFloat),

            // BigFloat conversions
            "f64_to_bigfloat" => Some(Self::F64ToBigFloat),
            "bigfloat_to_f64" => Some(Self::BigFloatToF64),
            "string_to_bigfloat" => Some(Self::StringToBigFloat),
            "bigfloat_to_string" => Some(Self::BigFloatToString),

            _ => None,
        }
    }

    /// Get the function name for this intrinsic.
    pub fn name(&self) -> &'static str {
        match self {
            // Integer arithmetic
            Self::NegInt => "neg_int",
            Self::AddInt => "add_int",
            Self::SubInt => "sub_int",
            Self::MulInt => "mul_int",
            Self::SdivInt => "sdiv_int",
            Self::SremInt => "srem_int",

            // Float arithmetic
            Self::NegFloat => "neg_float",
            // Runtime-dispatched
            Self::NegAny => "neg_any",
            Self::AddFloat => "add_float",
            Self::SubFloat => "sub_float",
            Self::MulFloat => "mul_float",
            Self::DivFloat => "div_float",
            Self::PowFloat => "pow_float",

            // Integer comparisons
            Self::EqInt => "eq_int",
            Self::NeInt => "ne_int",
            Self::SltInt => "slt_int",
            Self::SleInt => "sle_int",
            Self::SgtInt => "sgt_int",
            Self::SgeInt => "sge_int",

            // Float comparisons
            Self::EqFloat => "eq_float",
            Self::NeFloat => "ne_float",
            Self::LtFloat => "lt_float",
            Self::LeFloat => "le_float",
            Self::GtFloat => "gt_float",
            Self::GeFloat => "ge_float",

            // Bitwise
            Self::AndInt => "and_int",
            Self::OrInt => "or_int",
            Self::XorInt => "xor_int",
            Self::NotInt => "not_int",
            Self::ShlInt => "shl_int",
            Self::LshrInt => "lshr_int",
            Self::AshrInt => "ashr_int",

            // Type conversions
            Self::Sitofp => "sitofp",
            Self::Fptosi => "fptosi",

            // Low-level math
            Self::SqrtLlvm => "sqrt_llvm",
            Self::FloorLlvm => "floor_llvm",
            Self::CeilLlvm => "ceil_llvm",
            Self::TruncLlvm => "trunc_llvm",
            Self::AbsFloat => "abs_float",
            Self::CopysignFloat => "copysign_float",

            // BigInt arithmetic
            Self::NegBigInt => "neg_bigint",
            Self::AddBigInt => "add_bigint",
            Self::SubBigInt => "sub_bigint",
            Self::MulBigInt => "mul_bigint",
            Self::DivBigInt => "div_bigint",
            Self::RemBigInt => "rem_bigint",
            Self::AbsBigInt => "abs_bigint",
            Self::PowBigInt => "pow_bigint",

            // BigInt comparisons
            Self::EqBigInt => "eq_bigint",
            Self::NeBigInt => "ne_bigint",
            Self::LtBigInt => "lt_bigint",
            Self::LeBigInt => "le_bigint",
            Self::GtBigInt => "gt_bigint",
            Self::GeBigInt => "ge_bigint",

            // BigInt conversions
            Self::I64ToBigInt => "i64_to_bigint",
            Self::BigIntToI64 => "bigint_to_i64",
            Self::StringToBigInt => "string_to_bigint",
            Self::BigIntToString => "bigint_to_string",

            // BigFloat arithmetic
            Self::NegBigFloat => "neg_bigfloat",
            Self::AddBigFloat => "add_bigfloat",
            Self::SubBigFloat => "sub_bigfloat",
            Self::MulBigFloat => "mul_bigfloat",
            Self::DivBigFloat => "div_bigfloat",
            Self::AbsBigFloat => "abs_bigfloat",

            // BigFloat comparisons
            Self::EqBigFloat => "eq_bigfloat",
            Self::NeBigFloat => "ne_bigfloat",
            Self::LtBigFloat => "lt_bigfloat",
            Self::LeBigFloat => "le_bigfloat",
            Self::GtBigFloat => "gt_bigfloat",
            Self::GeBigFloat => "ge_bigfloat",

            // BigFloat conversions
            Self::F64ToBigFloat => "f64_to_bigfloat",
            Self::BigFloatToF64 => "bigfloat_to_f64",
            Self::StringToBigFloat => "string_to_bigfloat",
            Self::BigFloatToString => "bigfloat_to_string",
        }
    }

    /// Get the number of arguments for this intrinsic.
    pub fn arity(&self) -> usize {
        match self {
            // Unary operations
            Self::NegInt
            | Self::NegFloat
            | Self::NegAny
            | Self::NegBigInt
            | Self::NotInt
            | Self::Sitofp
            | Self::Fptosi
            | Self::SqrtLlvm
            | Self::FloorLlvm
            | Self::CeilLlvm
            | Self::TruncLlvm
            | Self::AbsFloat
            | Self::AbsBigInt
            | Self::I64ToBigInt
            | Self::BigIntToI64
            | Self::StringToBigInt
            | Self::BigIntToString
            | Self::NegBigFloat
            | Self::AbsBigFloat
            | Self::F64ToBigFloat
            | Self::BigFloatToF64
            | Self::StringToBigFloat
            | Self::BigFloatToString => 1,

            // Binary operations
            _ => 2,
        }
    }

    /// Check if this intrinsic returns a boolean (0 or 1).
    pub fn returns_bool(&self) -> bool {
        matches!(
            self,
            Self::EqInt
                | Self::NeInt
                | Self::SltInt
                | Self::SleInt
                | Self::SgtInt
                | Self::SgeInt
                | Self::EqFloat
                | Self::NeFloat
                | Self::LtFloat
                | Self::LeFloat
                | Self::GtFloat
                | Self::GeFloat
                | Self::EqBigInt
                | Self::NeBigInt
                | Self::LtBigInt
                | Self::LeBigInt
                | Self::GtBigInt
                | Self::GeBigInt
                | Self::EqBigFloat
                | Self::NeBigFloat
                | Self::LtBigFloat
                | Self::LeBigFloat
                | Self::GtBigFloat
                | Self::GeBigFloat
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_name() {
        assert_eq!(Intrinsic::from_name("add_int"), Some(Intrinsic::AddInt));
        assert_eq!(Intrinsic::from_name("sqrt_llvm"), Some(Intrinsic::SqrtLlvm));
        assert_eq!(Intrinsic::from_name("unknown"), None);
    }

    #[test]
    fn test_name_roundtrip() {
        let intrinsics = [
            Intrinsic::AddInt,
            Intrinsic::SubFloat,
            Intrinsic::SqrtLlvm,
            Intrinsic::AddBigInt,
        ];
        for intr in intrinsics {
            assert_eq!(Intrinsic::from_name(intr.name()), Some(intr));
        }
    }

    #[test]
    fn test_arity() {
        assert_eq!(Intrinsic::NegInt.arity(), 1);
        assert_eq!(Intrinsic::AddInt.arity(), 2);
        assert_eq!(Intrinsic::SqrtLlvm.arity(), 1);
        assert_eq!(Intrinsic::MulBigInt.arity(), 2);
    }

    #[test]
    fn test_returns_bool() {
        assert!(Intrinsic::EqInt.returns_bool());
        assert!(Intrinsic::LtFloat.returns_bool());
        assert!(!Intrinsic::AddInt.returns_bool());
        assert!(!Intrinsic::SqrtLlvm.returns_bool());
    }
}
