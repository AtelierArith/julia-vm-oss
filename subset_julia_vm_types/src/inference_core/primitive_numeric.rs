//! Canonical primitive numeric taxonomy shared between VM and AoT inference.
//!
//! Issue #3508 — both the VM-side abstract-interpretation lattice and the
//! AoT static-inference engine need to answer questions like *"is this type a
//! float?"*, *"is this an integer?"*, *"is this a numeric primitive at all?"*.
//! Historically each side answered them with its own `match`-based predicate
//! over its own type representation (`StaticType`, `ConcreteType`, name
//! strings, …), which let the two pipelines drift out of sync as one was
//! taught about a new primitive and the other forgotten.
//!
//! This module introduces [`PrimitiveNumeric`], a single canonical enum
//! covering every concrete primitive numeric type that either pipeline
//! recognises today. Both sides convert their type representation into a
//! `PrimitiveNumeric` (via a thin bridge) and then ask the same predicates
//! defined here. The conversion is lossy by design — non-primitive variants
//! (struct, array, union, …) round-trip through `None` — but the predicates
//! never disagree once the two sides reach the canonical form.
//!
//! Behaviour-preserving by construction: every variant matches an existing
//! primitive recognised by the shared `promotion::is_*_type_name`
//! predicates, and the AoT-side bridge is exhaustive over the primitive
//! variants of `StaticType`. Pure refactor — see the unit tests at the end of
//! this file for the property-style coverage.

/// Canonical primitive numeric type used by inference.
///
/// The variants mirror Julia's primitive numeric taxonomy: `Bool`, the eight
/// signed/unsigned bounded integer widths, plus `Float16`/`Float32`/`Float64`.
/// Arbitrary-precision types (`BigInt`, `BigFloat`) are deliberately *not*
/// included — neither the shared `promotion::is_*_type_name`
/// predicates nor the AoT-side `StaticType` consider them primitive numerics
/// today, and adding them here would silently change classification at every
/// call site. They can be added in a follow-up alongside coordinated changes
/// at the call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrimitiveNumeric {
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
}

impl PrimitiveNumeric {
    /// Map a Julia type-name string to its canonical primitive variant.
    ///
    /// Returns `None` for non-primitive or unrecognised names. Aliases such
    /// as `"Int"` / `"UInt"` are deliberately *not* recognised — the previous
    /// VM-side `is_*_type_name` predicates rejected them, and a pure-refactor
    /// extraction must preserve that exactly. Alias resolution should happen
    /// at the caller before invoking this conversion.
    pub fn from_julia_name(name: &str) -> Option<Self> {
        Some(match name {
            "Bool" => Self::Bool,
            "Int8" => Self::Int8,
            "Int16" => Self::Int16,
            "Int32" => Self::Int32,
            "Int64" => Self::Int64,
            "Int128" => Self::Int128,
            "UInt8" => Self::UInt8,
            "UInt16" => Self::UInt16,
            "UInt32" => Self::UInt32,
            "UInt64" => Self::UInt64,
            "UInt128" => Self::UInt128,
            "Float16" => Self::Float16,
            "Float32" => Self::Float32,
            "Float64" => Self::Float64,
            _ => return None,
        })
    }

    /// Canonical Julia type-name string for this primitive.
    pub fn julia_name(self) -> &'static str {
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
        }
    }

    /// True iff this is a floating-point primitive (`Float16/32/64`).
    pub fn is_float(self) -> bool {
        matches!(self, Self::Float16 | Self::Float32 | Self::Float64)
    }

    /// True iff this is a *signed* bounded-width integer (`Int8..Int128`).
    /// `Bool` is **not** signed.
    pub fn is_signed_integer(self) -> bool {
        matches!(
            self,
            Self::Int8 | Self::Int16 | Self::Int32 | Self::Int64 | Self::Int128
        )
    }

    /// True iff this is an *unsigned* bounded-width integer (`UInt8..UInt128`).
    /// `Bool` is **not** classified as unsigned by this predicate (it is its
    /// own thing in Julia's lattice — `Bool <: Integer` but
    /// `Bool ⊄ Unsigned`).
    pub fn is_unsigned_integer(self) -> bool {
        matches!(
            self,
            Self::UInt8 | Self::UInt16 | Self::UInt32 | Self::UInt64 | Self::UInt128
        )
    }

    /// True iff this is an integer primitive — i.e. signed, unsigned, or
    /// `Bool`. Mirrors Julia's `T <: Integer` test for primitives.
    pub fn is_integer(self) -> bool {
        self.is_signed_integer() || self.is_unsigned_integer() || matches!(self, Self::Bool)
    }

    /// True iff this is *any* numeric primitive. Trivially `true` for every
    /// variant since the enum only contains numeric primitives, but exposing
    /// it as a method keeps call sites symmetric with `is_integer` /
    /// `is_float` and reads better than `_ = primitive;`.
    pub fn is_numeric(self) -> bool {
        // All variants are numeric by construction; method exists so callers
        // can treat the trio (is_numeric, is_integer, is_float) uniformly.
        let _ = self;
        true
    }

    /// Numeric promotion shared by VM/AoT inference.
    ///
    /// This intentionally preserves the historical AoT transfer-function
    /// shape: small integers and `Bool` widen to `Int64`, existing 64-bit
    /// or wider integer operands keep the wider rank, and floats dominate
    /// integers while wider floats dominate narrower floats.
    pub fn promote(self, other: Self) -> Self {
        if self.is_float() && other.is_float() {
            return if self.rank() >= other.rank() {
                self
            } else {
                other
            };
        }
        if self.is_float() {
            return self;
        }
        if other.is_float() {
            return other;
        }

        let max_rank = self.rank().max(other.rank());
        if max_rank <= Self::UInt32.rank() {
            Self::Int64
        } else if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    fn rank(self) -> i32 {
        match self {
            Self::Bool => 0,
            Self::Int8 => 1,
            Self::UInt8 => 2,
            Self::Int16 => 3,
            Self::UInt16 => 4,
            Self::Int32 => 5,
            Self::UInt32 => 6,
            Self::Int64 => 7,
            Self::UInt64 => 8,
            Self::Int128 => 9,
            Self::UInt128 => 10,
            Self::Float16 => 99,
            Self::Float32 => 100,
            Self::Float64 => 101,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant must round-trip through `from_julia_name` /
    /// `julia_name`. Guards against mismatched name tables.
    #[test]
    fn name_roundtrip() {
        let all = [
            PrimitiveNumeric::Bool,
            PrimitiveNumeric::Int8,
            PrimitiveNumeric::Int16,
            PrimitiveNumeric::Int32,
            PrimitiveNumeric::Int64,
            PrimitiveNumeric::Int128,
            PrimitiveNumeric::UInt8,
            PrimitiveNumeric::UInt16,
            PrimitiveNumeric::UInt32,
            PrimitiveNumeric::UInt64,
            PrimitiveNumeric::UInt128,
            PrimitiveNumeric::Float16,
            PrimitiveNumeric::Float32,
            PrimitiveNumeric::Float64,
        ];
        for p in all {
            assert_eq!(
                PrimitiveNumeric::from_julia_name(p.julia_name()),
                Some(p),
                "round-trip failed for {:?}",
                p
            );
        }
    }

    /// Aliases (`Int`, `UInt`) and abstract names must be rejected so the
    /// new module preserves the historical VM-side `is_*_type_name`
    /// behaviour exactly.
    #[test]
    fn unrecognised_names() {
        for name in ["Int", "UInt", "Number", "Integer", "AbstractFloat", ""] {
            assert!(
                PrimitiveNumeric::from_julia_name(name).is_none(),
                "{name:?} should not be recognised as a primitive"
            );
        }
    }

    /// Float predicates partition the enum exactly into floats vs
    /// non-floats.
    #[test]
    fn float_partition() {
        for name in ["Float16", "Float32", "Float64"] {
            let p = PrimitiveNumeric::from_julia_name(name).unwrap();
            assert!(p.is_float(), "{name} should be float");
            assert!(!p.is_integer(), "{name} should not be integer");
            assert!(p.is_numeric(), "{name} should be numeric");
        }
        for name in [
            "Bool", "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32",
            "UInt64", "UInt128",
        ] {
            let p = PrimitiveNumeric::from_julia_name(name).unwrap();
            assert!(!p.is_float(), "{name} should not be float");
            assert!(p.is_integer(), "{name} should be integer");
            assert!(p.is_numeric(), "{name} should be numeric");
        }
    }

    /// Signed / unsigned partition over integer primitives. Bool is
    /// integer but neither signed nor unsigned (matches Julia's
    /// `Bool <: Integer`, `Bool ⊄ Signed`, `Bool ⊄ Unsigned`).
    #[test]
    fn signed_unsigned_partition() {
        let signed = [
            PrimitiveNumeric::Int8,
            PrimitiveNumeric::Int16,
            PrimitiveNumeric::Int32,
            PrimitiveNumeric::Int64,
            PrimitiveNumeric::Int128,
        ];
        for p in signed {
            assert!(p.is_signed_integer());
            assert!(!p.is_unsigned_integer());
            assert!(p.is_integer());
        }
        let unsigned = [
            PrimitiveNumeric::UInt8,
            PrimitiveNumeric::UInt16,
            PrimitiveNumeric::UInt32,
            PrimitiveNumeric::UInt64,
            PrimitiveNumeric::UInt128,
        ];
        for p in unsigned {
            assert!(!p.is_signed_integer());
            assert!(p.is_unsigned_integer());
            assert!(p.is_integer());
        }

        // Bool: integer but neither signed nor unsigned by our taxonomy.
        let b = PrimitiveNumeric::Bool;
        assert!(b.is_integer());
        assert!(!b.is_signed_integer());
        assert!(!b.is_unsigned_integer());

        // Floats are neither signed nor unsigned integer.
        for f in [
            PrimitiveNumeric::Float16,
            PrimitiveNumeric::Float32,
            PrimitiveNumeric::Float64,
        ] {
            assert!(!f.is_signed_integer());
            assert!(!f.is_unsigned_integer());
            assert!(!f.is_integer());
        }
    }

    #[test]
    fn promote_matches_existing_aot_numeric_widening() {
        assert_eq!(
            PrimitiveNumeric::Bool.promote(PrimitiveNumeric::Bool),
            PrimitiveNumeric::Int64
        );
        assert_eq!(
            PrimitiveNumeric::UInt32.promote(PrimitiveNumeric::Int16),
            PrimitiveNumeric::Int64
        );
        assert_eq!(
            PrimitiveNumeric::UInt64.promote(PrimitiveNumeric::Int64),
            PrimitiveNumeric::UInt64
        );
        assert_eq!(
            PrimitiveNumeric::Float32.promote(PrimitiveNumeric::Int64),
            PrimitiveNumeric::Float32
        );
        assert_eq!(
            PrimitiveNumeric::Float32.promote(PrimitiveNumeric::Float64),
            PrimitiveNumeric::Float64
        );
    }
}
