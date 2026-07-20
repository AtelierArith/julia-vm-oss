//! Stable wire-ID tables for `BuiltinOp` (Issue #8627).
//!
//! Moved from `compile::instr_wire_ids` during the crate split (Issue #8656
//! Phase 1 completion). `BuiltinOp` now lives in `subset_julia_vm_types` so
//! its wire-ID mapping and serde implementation must live here too.
//!
//! Wire IDs are assigned once and never reused. Adding a variant: append to
//! the enum AND add a new wire ID at the next available number.
//! Retiring a variant: remove from the enum but keep a comment
//! `// Wire ID NN → RETIRED (VariantName removed in Issue #XXXX)`.

use super::core::BuiltinOp;

/// Map `BuiltinOp` variant to its stable wire ID (Issue #8627).
pub fn builtinop_to_wire_id(v: BuiltinOp) -> u32 {
    match v {
        BuiltinOp::Rand => 0,
        BuiltinOp::Sqrt => 1,
        BuiltinOp::IfElse => 2,
        BuiltinOp::TimeNs => 3,
        BuiltinOp::Zeros => 4,
        BuiltinOp::Ones => 5,
        BuiltinOp::Reshape => 6,
        BuiltinOp::Length => 7,
        BuiltinOp::Size => 8,
        BuiltinOp::Ndims => 9,
        BuiltinOp::Push => 10,
        BuiltinOp::Pop => 11,
        BuiltinOp::PushFirst => 12,
        BuiltinOp::PopFirst => 13,
        BuiltinOp::Insert => 14,
        BuiltinOp::DeleteAt => 15,
        BuiltinOp::Zero => 16,
        BuiltinOp::Lu => 17,
        BuiltinOp::Det => 18,
        BuiltinOp::StableRNG => 19,
        BuiltinOp::XoshiroRNG => 20,
        BuiltinOp::Randn => 21,
        BuiltinOp::TupleFirst => 22,
        BuiltinOp::TupleLast => 23,
        BuiltinOp::HasKey => 24,
        BuiltinOp::DictGet => 25,
        BuiltinOp::DictDelete => 26,
        BuiltinOp::DictKeys => 27,
        BuiltinOp::DictValues => 28,
        BuiltinOp::DictPairs => 29,
        BuiltinOp::DictMerge => 30,
        BuiltinOp::DictGetBang => 31,
        BuiltinOp::DictMergeBang => 32,
        BuiltinOp::DictEmpty => 33,
        BuiltinOp::DictGetkey => 34,
        BuiltinOp::Ref => 35,
        BuiltinOp::TypeOf => 36,
        BuiltinOp::Isa => 37,
        BuiltinOp::Eltype => 38,
        BuiltinOp::Keytype => 39,
        BuiltinOp::Valtype => 40,
        BuiltinOp::Sizeof => 41,
        BuiltinOp::Isbitstype => 42,
        BuiltinOp::Supertype => 43,
        BuiltinOp::Typename => 44,
        BuiltinOp::FunctionName => 45,
        BuiltinOp::Subtypes => 46,
        BuiltinOp::Objectid => 47,
        BuiltinOp::Isunordered => 48,
        BuiltinOp::Methods => 49,
        BuiltinOp::HasMethod => 50,
        BuiltinOp::In => 51,
        BuiltinOp::Seed => 52,
        BuiltinOp::Iterate => 53,
        BuiltinOp::Collect => 54,
        BuiltinOp::Generator => 55,
        BuiltinOp::SymbolNew => 56,
        BuiltinOp::ExprNew => 57,
        BuiltinOp::LineNumberNodeNew => 58,
        BuiltinOp::QuoteNodeNew => 59,
        BuiltinOp::GlobalRefNew => 60,
        BuiltinOp::Gensym => 61,
        BuiltinOp::Esc => 62,
        BuiltinOp::Eval => 63,
        BuiltinOp::MacroExpand => 64,
        BuiltinOp::MacroExpandBang => 65,
        BuiltinOp::IncludeString => 66,
        BuiltinOp::EvalFile => 67,
        BuiltinOp::SplatInterpolation => 68,
        BuiltinOp::TestRecord => 69,
        BuiltinOp::TestRecordBroken => 70,
        BuiltinOp::TestSetBegin => 71,
        BuiltinOp::TestSetEnd => 72,
        BuiltinOp::IsDefined => 73,
        BuiltinOp::GeneratedEval => 74,
        BuiltinOp::MersenneTwisterRNG => 75,
        BuiltinOp::RangeStep => 76,
        BuiltinOp::TestRecordError => 77,
    }
}

/// Map wire ID back to `BuiltinOp` variant (Issue #8627).
///
/// Returns `None` for unknown or retired IDs.
pub fn builtinop_from_wire_id(id: u32) -> Option<BuiltinOp> {
    Some(match id {
        0 => BuiltinOp::Rand,
        1 => BuiltinOp::Sqrt,
        2 => BuiltinOp::IfElse,
        3 => BuiltinOp::TimeNs,
        4 => BuiltinOp::Zeros,
        5 => BuiltinOp::Ones,
        6 => BuiltinOp::Reshape,
        7 => BuiltinOp::Length,
        8 => BuiltinOp::Size,
        9 => BuiltinOp::Ndims,
        10 => BuiltinOp::Push,
        11 => BuiltinOp::Pop,
        12 => BuiltinOp::PushFirst,
        13 => BuiltinOp::PopFirst,
        14 => BuiltinOp::Insert,
        15 => BuiltinOp::DeleteAt,
        16 => BuiltinOp::Zero,
        17 => BuiltinOp::Lu,
        18 => BuiltinOp::Det,
        19 => BuiltinOp::StableRNG,
        20 => BuiltinOp::XoshiroRNG,
        21 => BuiltinOp::Randn,
        22 => BuiltinOp::TupleFirst,
        23 => BuiltinOp::TupleLast,
        24 => BuiltinOp::HasKey,
        25 => BuiltinOp::DictGet,
        26 => BuiltinOp::DictDelete,
        27 => BuiltinOp::DictKeys,
        28 => BuiltinOp::DictValues,
        29 => BuiltinOp::DictPairs,
        30 => BuiltinOp::DictMerge,
        31 => BuiltinOp::DictGetBang,
        32 => BuiltinOp::DictMergeBang,
        33 => BuiltinOp::DictEmpty,
        34 => BuiltinOp::DictGetkey,
        35 => BuiltinOp::Ref,
        36 => BuiltinOp::TypeOf,
        37 => BuiltinOp::Isa,
        38 => BuiltinOp::Eltype,
        39 => BuiltinOp::Keytype,
        40 => BuiltinOp::Valtype,
        41 => BuiltinOp::Sizeof,
        42 => BuiltinOp::Isbitstype,
        43 => BuiltinOp::Supertype,
        44 => BuiltinOp::Typename,
        45 => BuiltinOp::FunctionName,
        46 => BuiltinOp::Subtypes,
        47 => BuiltinOp::Objectid,
        48 => BuiltinOp::Isunordered,
        49 => BuiltinOp::Methods,
        50 => BuiltinOp::HasMethod,
        51 => BuiltinOp::In,
        52 => BuiltinOp::Seed,
        53 => BuiltinOp::Iterate,
        54 => BuiltinOp::Collect,
        55 => BuiltinOp::Generator,
        56 => BuiltinOp::SymbolNew,
        57 => BuiltinOp::ExprNew,
        58 => BuiltinOp::LineNumberNodeNew,
        59 => BuiltinOp::QuoteNodeNew,
        60 => BuiltinOp::GlobalRefNew,
        61 => BuiltinOp::Gensym,
        62 => BuiltinOp::Esc,
        63 => BuiltinOp::Eval,
        64 => BuiltinOp::MacroExpand,
        65 => BuiltinOp::MacroExpandBang,
        66 => BuiltinOp::IncludeString,
        67 => BuiltinOp::EvalFile,
        68 => BuiltinOp::SplatInterpolation,
        69 => BuiltinOp::TestRecord,
        70 => BuiltinOp::TestRecordBroken,
        71 => BuiltinOp::TestSetBegin,
        72 => BuiltinOp::TestSetEnd,
        73 => BuiltinOp::IsDefined,
        74 => BuiltinOp::GeneratedEval,
        75 => BuiltinOp::MersenneTwisterRNG,
        76 => BuiltinOp::RangeStep,
        77 => BuiltinOp::TestRecordError,
        _ => return None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Custom Serde implementation for BuiltinOp (Issue #8627)
//
// These replace derived Serialize/Deserialize so that the encoded byte is the
// stable WIRE ID rather than the declaration-order discriminant.
// ─────────────────────────────────────────────────────────────────────────────

impl serde::Serialize for BuiltinOp {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u32(builtinop_to_wire_id(*self))
    }
}

impl<'de> serde::Deserialize<'de> for BuiltinOp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let id = u32::deserialize(d)?;
        builtinop_from_wire_id(id)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown BuiltinOp wire ID: {id}")))
    }
}
