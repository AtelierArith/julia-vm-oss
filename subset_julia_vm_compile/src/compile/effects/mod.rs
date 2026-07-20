//! Effects system for tracking computational properties of method calls.
//!
//! This module implements an effects system similar to Julia's `Core.Compiler.Effects`
//! to track properties like purity, side effects, termination, and exception behavior.
//! These properties enable optimization opportunities and safety guarantees.

pub mod inference;
pub mod propagation;
pub mod static_dispatch;
pub use subset_julia_vm_types::runtime_types::{EffectBit, Effects};
// The VM-instruction effect table (`effects/instr.rs`, Issue #5185) was removed:
// it was unwired scaffolding for a bytecode-level LICM/CSE pass that never
// landed (the production optimizer `ssa_ir::opt` works on the SSA IR, not the VM
// `Instr` model). Re-introduce it alongside its consumer — tracked by #9494
// (Issue #9205 acceptance criterion 2).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_bit_merge() {
        assert_eq!(
            EffectBit::AlwaysTrue.merge(&EffectBit::AlwaysTrue),
            EffectBit::AlwaysTrue
        );
        assert_eq!(
            EffectBit::AlwaysFalse.merge(&EffectBit::AlwaysFalse),
            EffectBit::AlwaysFalse
        );
        assert_eq!(
            EffectBit::AlwaysTrue.merge(&EffectBit::AlwaysFalse),
            EffectBit::Conditional
        );
        assert_eq!(
            EffectBit::Conditional.merge(&EffectBit::AlwaysTrue),
            EffectBit::Conditional
        );
    }

    #[test]
    fn test_total_effects() {
        let effects = Effects::total();
        assert!(effects.is_pure());
        assert!(effects.is_total());
        assert!(effects.is_removable());
        assert!(effects.is_foldable());
    }

    #[test]
    fn test_arbitrary_effects() {
        let effects = Effects::arbitrary();
        assert!(!effects.is_pure());
        assert!(!effects.is_total());
        assert!(!effects.is_removable());
        assert!(!effects.is_foldable());
    }

    #[test]
    fn test_pure_arithmetic_effects() {
        let effects = Effects::pure_arithmetic();
        assert!(effects.is_pure());
        assert!(effects.is_total());
        assert!(effects.is_removable());
        assert!(effects.is_foldable());
    }

    #[test]
    fn test_side_effects() {
        let effects = Effects::with_side_effects();
        assert!(!effects.is_pure());
        assert!(!effects.is_removable());
        assert!(!effects.is_foldable());
    }

    #[test]
    fn test_array_getindex_effects() {
        let effects = Effects::array_getindex();
        assert!(!effects.is_pure()); // Not pure because nothrow = false
        assert!(!effects.is_total()); // Not total because nothrow = false
        assert!(!effects.is_removable()); // Not removable because nothrow = false
        assert!(effects.consistent.is_always_true());
        assert!(effects.effect_free.is_always_true());
    }

    #[test]
    fn test_array_setindex_effects() {
        let effects = Effects::array_setindex();
        assert!(!effects.is_pure());
        assert!(!effects.is_removable());
        assert!(!effects.is_foldable());
        assert!(effects.consistent.is_always_false());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_effects_merge() {
        let pure = Effects::pure_arithmetic();
        let side_effect = Effects::with_side_effects();
        let merged = pure.merge(&side_effect);

        // Merged effects are conservative (pessimistic)
        assert!(!merged.is_pure());
        assert!(!merged.nothrow);
        assert!(merged.consistent.is_conditional());
        assert!(merged.effect_free.is_conditional());
    }

    #[test]
    fn test_effects_merge_two_pure() {
        let pure1 = Effects::pure_arithmetic();
        let pure2 = Effects::pure_arithmetic();
        let merged = pure1.merge(&pure2);

        // Two pure operations remain pure
        assert!(merged.is_pure());
        assert!(merged.is_total());
        assert!(merged.is_foldable());
    }
}
