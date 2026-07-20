//! Stable wire-ID tables for serialized enum types (Issue #8627).
//!
//! bincode encodes enums by variant *declaration index* — the position of a
//! variant in the enum definition file. This is fragile: inserting, removing, or
//! reordering variants silently re-tags every later instruction in a cached
//! bytecode file (historically the root of many CACHE_VERSION bumps).
//!
//! These tables decouple the *wire representation* (a stable u32 "wire ID")
//! from the *declaration order* so that reordering variants only requires
//! updating the match arms here, not bumping CACHE_VERSION or regenerating
//! caches. Wire IDs are assigned once and never reused:
//!
//! - **Initial assignment**: current declaration order index (backward-compatible
//!   with caches produced before Issue #8627).
//! - **Adding a variant**: append to the enum AND add a new wire ID at the next
//!   available number (conventionally the next unused integer).
//! - **Retiring a variant**: remove from the enum but keep a comment:
//!   `// Wire ID NN → RETIRED (VariantName removed in Issue #8628)`
//! - **Reordering**: change declaration order freely; update the match arms here
//!   so the wire IDs stay at their original numbers.
//!
//! For `Instr` (415 payload-bearing variants) the full wire-ID-based Serde
//! is deferred to the Register VM migration (Issue #8448); the
//! `enum_variant_fingerprint` from Issue #8626 provides the safety net in the
//! interim.  The audit script `scripts/check_instr_wire_ids.sh` (Issue #8628)
//! verifies coverage and the no-reuse rule for all four enums.

// `builtinop_to_wire_id` and `builtinop_from_wire_id` are now owned by
// `subset_julia_vm_types::ir::wire_ids` (Issue #8656 Phase 1 completion).
// The serde Serialize/Deserialize impls for BuiltinOp also live there.
// Re-export into the test namespace so the existing round-trip tests still compile.
#[cfg(test)]
pub(crate) use subset_julia_vm_types::ir::wire_ids::{
    builtinop_from_wire_id, builtinop_to_wire_id,
};

#[cfg(test)]
pub(crate) use subset_julia_vm_bytecode::wire_ids::{
    builtinid_from_wire_id, builtinid_to_wire_id, intrinsic_from_wire_id, intrinsic_to_wire_id,
};

/// Wire IDs for `Instr` variants (Issue #8627).
///
/// These constants document the stable wire representation for each `Instr`
/// variant. `Instr` contains 415 payload-bearing variants; implementing custom
/// Serialize/Deserialize through this table requires generating match arms for
/// each variant's payload, which is deferred to the Register VM migration
/// (Issue #8448) when the instruction set is reorganized.
///
/// Until that migration, `Instr` is serialized by declaration order (the
/// existing behavior); the `enum_variant_fingerprint` in the cache header
/// (Issue #8626) ensures any declaration-order change is detected and triggers
/// cache regeneration rather than silent misdecoding.
///
/// Wire IDs here are informational / used by the audit script; they must match
/// the current declaration order to accurately document the wire format.
pub(crate) mod instr_wire_id_docs {
    // Wire IDs are assigned from 0 in declaration order.
    // This module exists so `scripts/check_instr_wire_ids.sh` can verify
    // that every Instr variant has a documented wire ID, and that no wire
    // ID is duplicated or retired IDs reused.
    //
    // DO NOT reorder these constants: their position = their wire ID
    // (= current declaration order index). When Instr is reordered in
    // Issue #8448, this module will be updated to carry explicit assignments.
}

// ─────────────────────────────────────────────────────────────────────────────
// Note: Custom Serde implementation for BuiltinOp (Issue #8627) has been moved
// to `subset_julia_vm_types::ir::wire_ids` (Issue #8656). The impls are no
// longer here because BuiltinOp is now a foreign type (defined in _types).
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Tests (Issue #8627)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builtins::BuiltinId;
    use crate::intrinsics::Intrinsic;
    use crate::ir::core::BuiltinOp;
    use bincode::Options;

    /// Wire IDs initially equal declaration order; verify round-trips produce
    /// the same bytes as old derive(Serialize) would have.
    #[test]
    fn builtin_op_wire_id_round_trips_8627() {
        for wire_id in 0..76_u32 {
            let v = builtinop_from_wire_id(wire_id)
                .unwrap_or_else(|| panic!("no BuiltinOp for wire_id {wire_id}"));
            assert_eq!(builtinop_to_wire_id(v), wire_id);
        }
        // Unknown wire IDs must fail cleanly.
        assert!(builtinop_from_wire_id(9999).is_none());
    }

    #[test]
    fn builtin_id_wire_id_round_trips_8627() {
        // Retired wire IDs stay reserved forever and must map to None:
        // 272 = BuiltinId::Bool, removed when the Bool constructor moved to
        // Pure Julia (Issues #8768 / #8820).
        const RETIRED_BUILTIN_ID_WIRE_IDS: &[u32] = &[272];
        for wire_id in 0..308_u32 {
            if RETIRED_BUILTIN_ID_WIRE_IDS.contains(&wire_id) {
                assert!(
                    builtinid_from_wire_id(wire_id).is_none(),
                    "retired wire_id {wire_id} must not resolve"
                );
                continue;
            }
            let v = builtinid_from_wire_id(wire_id)
                .unwrap_or_else(|| panic!("no BuiltinId for wire_id {wire_id}"));
            assert_eq!(builtinid_to_wire_id(v), wire_id);
        }
        assert!(builtinid_from_wire_id(9999).is_none());
    }

    #[test]
    fn intrinsic_wire_id_round_trips_8627() {
        for wire_id in 0..76_u32 {
            let v = intrinsic_from_wire_id(wire_id)
                .unwrap_or_else(|| panic!("no Intrinsic for wire_id {wire_id}"));
            assert_eq!(intrinsic_to_wire_id(v), wire_id);
        }
        assert!(intrinsic_from_wire_id(9999).is_none());
    }

    /// Verify that the serde round-trip through wire IDs produces identical
    /// bytes to what the old `#[derive(Serialize, Deserialize)]` would have —
    /// i.e., this change does NOT break existing caches (backward-compatible).
    #[test]
    fn builtin_op_serde_bytes_match_declaration_order_8627() {
        let codec = bincode::DefaultOptions::new()
            .with_varint_encoding()
            .allow_trailing_bytes();

        // With the old derive, BuiltinOp::Rand (declaration index 0) serialized
        // as the u32 value 0 via varint.
        let v = BuiltinOp::Rand;
        let bytes = codec.serialize(&v).expect("serialize");
        let decoded: BuiltinOp = codec.deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded, BuiltinOp::Rand);

        // Spot-check a later variant: MersenneTwisterRNG should be last (75).
        let last = BuiltinOp::MersenneTwisterRNG;
        let bytes_last = codec.serialize(&last).expect("serialize last");
        let decoded_last: BuiltinOp = codec.deserialize(&bytes_last).expect("deserialize last");
        assert_eq!(decoded_last, BuiltinOp::MersenneTwisterRNG);
    }

    #[test]
    fn builtin_id_serde_bytes_match_declaration_order_8627() {
        let codec = bincode::DefaultOptions::new()
            .with_varint_encoding()
            .allow_trailing_bytes();
        let v = BuiltinId::Sqrt;
        let bytes = codec.serialize(&v).expect("serialize");
        let decoded: BuiltinId = codec.deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded, BuiltinId::Sqrt);
    }

    #[test]
    fn intrinsic_serde_bytes_match_declaration_order_8627() {
        let codec = bincode::DefaultOptions::new()
            .with_varint_encoding()
            .allow_trailing_bytes();
        let v = Intrinsic::NegInt;
        let bytes = codec.serialize(&v).expect("serialize");
        let decoded: Intrinsic = codec.deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded, Intrinsic::NegInt);
    }
}
