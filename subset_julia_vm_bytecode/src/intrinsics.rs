//! Core intrinsics - CPU instructions that map directly to low-level operations.
//!
//! These correspond to Julia's `src/intrinsics.h` and represent the minimal set
//! of operations that cannot be decomposed further.
//!
//! Design principle: Intrinsics are the atoms of computation. Higher-level
//! operations (like `sin`, `map`) are built on top of these through Builtin
//! functions or Julia code.

/// Core intrinsics - CPU instruction-level operations.
///
/// Naming follows Julia's convention from `src/intrinsics.h`:
/// - `_int` suffix for integer operations
/// - `_float` suffix for floating-point operations
/// - `s` prefix for signed operations (e.g., `slt` = signed less than)
/// - `_llvm` suffix for LLVM intrinsic-backed operations
// `strum::VariantNames` feeds `compile::precompile::enum_variant_fingerprint()`
// (Issue #8626): `Intrinsic` is part of the serialized bytecode wire format,
// so variant insert/remove/reorder must be detected at cache load time.
//
// Serialize/Deserialize are implemented in `compile::instr_wire_ids` via stable
// wire IDs (Issue #8627) — not derived, to decouple declaration order from
// the wire representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::VariantNames)]
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
    //
    // These variants back the runtime-dispatched `+ - * / ^` operators (emitted
    // as the operand of `CallDynamicBinaryBoth` and `CallIntrinsic`). Despite
    // the historical `add_float`/… wire names — kept so `Core.Intrinsics.*_float`
    // still resolves via `from_name` — they are NOT float-only: at runtime they
    // dispatch over every argument type (Int64, Float64, Complex, BigInt, …).
    // They were renamed away from the `…Float` spelling because the old
    // `AddFloat`/`MulFloat` Debug names in `--dump-bytecode` read as if the op
    // were floating-point-only (Issue #10754).
    /// neg_any(x) -> -x (runtime type dispatch: returns I64 for I64 input, F64 for F64)
    NegAny,
    /// Runtime-dispatched `+` (wire name `add_float`; dispatches over all types).
    DynamicAdd,
    /// Runtime-dispatched `-` (wire name `sub_float`; dispatches over all types).
    DynamicSub,
    /// Runtime-dispatched `*` (wire name `mul_float`; dispatches over all types).
    DynamicMul,
    /// Runtime-dispatched `/` (wire name `div_float`; dispatches over all types).
    DynamicDiv,
    /// Runtime-dispatched `^` (wire name `pow_float`; dispatches over all types).
    DynamicPow,

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
    /// rint_llvm(x) -> round(x) to nearest, ties to even (banker's rounding)
    RintLlvm,
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
    /// rem_bigfloat(a, b) -> a % b (remainder, sign of `a`; a % 0 -> NaN)
    RemBigFloat,
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

/// Generates `Intrinsic::name` and `Intrinsic::from_name` from a single table so
/// the two directions cannot drift out of sync (Issue #6831). Each row is
/// `Variant: "name" => ["from_name", ...]`. Discriminant order is fixed by the
/// hand-written `enum Intrinsic` above (bincode cache compatibility), not by this
/// table.
macro_rules! define_intrinsic_table {
    ( $( $variant:ident : $canon:literal => [ $( $alias:literal ),* $(,)? ] ),* $(,)? ) => {
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
                    $( $( $alias => Some(Self::$variant), )* )*
                    _ => None,
                }
            }

            /// Get the function name for this intrinsic.
            pub fn name(&self) -> &'static str {
                match self {
                    $( Self::$variant => $canon, )*
                }
            }
        }
    };
}

define_intrinsic_table! {
    NegInt: "neg_int" => ["neg_int"],
    AddInt: "add_int" => ["add_int"],
    SubInt: "sub_int" => ["sub_int"],
    MulInt: "mul_int" => ["mul_int"],
    SdivInt: "sdiv_int" => ["sdiv_int"],
    SremInt: "srem_int" => ["srem_int"],
    NegFloat: "neg_float" => ["neg_float"],
    NegAny: "neg_any" => ["neg_any"],
    DynamicAdd: "add_float" => ["add_float"],
    DynamicSub: "sub_float" => ["sub_float"],
    DynamicMul: "mul_float" => ["mul_float"],
    DynamicDiv: "div_float" => ["div_float"],
    DynamicPow: "pow_float" => ["pow_float"],
    EqInt: "eq_int" => ["eq_int"],
    NeInt: "ne_int" => ["ne_int"],
    SltInt: "slt_int" => ["slt_int"],
    SleInt: "sle_int" => ["sle_int"],
    SgtInt: "sgt_int" => ["sgt_int"],
    SgeInt: "sge_int" => ["sge_int"],
    EqFloat: "eq_float" => ["eq_float"],
    NeFloat: "ne_float" => ["ne_float"],
    LtFloat: "lt_float" => ["lt_float"],
    LeFloat: "le_float" => ["le_float"],
    GtFloat: "gt_float" => ["gt_float"],
    GeFloat: "ge_float" => ["ge_float"],
    AndInt: "and_int" => ["and_int"],
    OrInt: "or_int" => ["or_int"],
    XorInt: "xor_int" => ["xor_int"],
    NotInt: "not_int" => ["not_int"],
    ShlInt: "shl_int" => ["shl_int"],
    LshrInt: "lshr_int" => ["lshr_int"],
    AshrInt: "ashr_int" => ["ashr_int"],
    Sitofp: "sitofp" => ["sitofp"],
    Fptosi: "fptosi" => ["fptosi"],
    SqrtLlvm: "sqrt_llvm" => ["sqrt_llvm"],
    FloorLlvm: "floor_llvm" => ["floor_llvm"],
    CeilLlvm: "ceil_llvm" => ["ceil_llvm"],
    TruncLlvm: "trunc_llvm" => ["trunc_llvm"],
    RintLlvm: "rint_llvm" => ["rint_llvm"],
    AbsFloat: "abs_float" => ["abs_float"],
    CopysignFloat: "copysign_float" => ["copysign_float"],
    NegBigInt: "neg_bigint" => ["neg_bigint"],
    AddBigInt: "add_bigint" => ["add_bigint"],
    SubBigInt: "sub_bigint" => ["sub_bigint"],
    MulBigInt: "mul_bigint" => ["mul_bigint"],
    DivBigInt: "div_bigint" => ["div_bigint"],
    RemBigInt: "rem_bigint" => ["rem_bigint"],
    AbsBigInt: "abs_bigint" => ["abs_bigint"],
    PowBigInt: "pow_bigint" => ["pow_bigint"],
    EqBigInt: "eq_bigint" => ["eq_bigint"],
    NeBigInt: "ne_bigint" => ["ne_bigint"],
    LtBigInt: "lt_bigint" => ["lt_bigint"],
    LeBigInt: "le_bigint" => ["le_bigint"],
    GtBigInt: "gt_bigint" => ["gt_bigint"],
    GeBigInt: "ge_bigint" => ["ge_bigint"],
    I64ToBigInt: "i64_to_bigint" => ["i64_to_bigint"],
    BigIntToI64: "bigint_to_i64" => ["bigint_to_i64"],
    StringToBigInt: "string_to_bigint" => ["string_to_bigint"],
    BigIntToString: "bigint_to_string" => ["bigint_to_string"],
    NegBigFloat: "neg_bigfloat" => ["neg_bigfloat"],
    AddBigFloat: "add_bigfloat" => ["add_bigfloat"],
    SubBigFloat: "sub_bigfloat" => ["sub_bigfloat"],
    MulBigFloat: "mul_bigfloat" => ["mul_bigfloat"],
    DivBigFloat: "div_bigfloat" => ["div_bigfloat"],
    RemBigFloat: "rem_bigfloat" => ["rem_bigfloat"],
    AbsBigFloat: "abs_bigfloat" => ["abs_bigfloat"],
    EqBigFloat: "eq_bigfloat" => ["eq_bigfloat"],
    NeBigFloat: "ne_bigfloat" => ["ne_bigfloat"],
    LtBigFloat: "lt_bigfloat" => ["lt_bigfloat"],
    LeBigFloat: "le_bigfloat" => ["le_bigfloat"],
    GtBigFloat: "gt_bigfloat" => ["gt_bigfloat"],
    GeBigFloat: "ge_bigfloat" => ["ge_bigfloat"],
    F64ToBigFloat: "f64_to_bigfloat" => ["f64_to_bigfloat"],
    BigFloatToF64: "bigfloat_to_f64" => ["bigfloat_to_f64"],
    StringToBigFloat: "string_to_bigfloat" => ["string_to_bigfloat"],
    BigFloatToString: "bigfloat_to_string" => ["bigfloat_to_string"],
}

impl Intrinsic {
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

    /// Regression for Issue #10754: the runtime-dispatched `+ - * / ^` operator
    /// tags must NOT have a `Debug` name that reads as floating-point-only
    /// (they appear verbatim in `--dump-bytecode` as
    /// `CallDynamicBinaryBoth(<Debug>, …)` yet dispatch over all argument
    /// types). Their `from_name` wire strings stay `*_float` so
    /// `Core.Intrinsics.*_float` keeps resolving.
    #[test]
    fn test_runtime_dispatched_ops_not_named_float_issue_10754() {
        let dynamic = [
            (Intrinsic::DynamicAdd, "add_float"),
            (Intrinsic::DynamicSub, "sub_float"),
            (Intrinsic::DynamicMul, "mul_float"),
            (Intrinsic::DynamicDiv, "div_float"),
            (Intrinsic::DynamicPow, "pow_float"),
        ];
        for (intr, wire_name) in dynamic {
            let debug = format!("{intr:?}");
            assert!(
                !debug.contains("Float"),
                "{debug} still reads as float-only in bytecode dumps"
            );
            assert!(
                debug.starts_with("Dynamic"),
                "{debug} lost the Dynamic prefix"
            );
            // The wire/from_name string is intentionally kept for
            // `Core.Intrinsics.*_float` resolution.
            assert_eq!(intr.name(), wire_name);
            assert_eq!(Intrinsic::from_name(wire_name), Some(intr));
        }
    }

    #[test]
    fn test_name_roundtrip() {
        let intrinsics = [
            Intrinsic::AddInt,
            Intrinsic::DynamicSub,
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
