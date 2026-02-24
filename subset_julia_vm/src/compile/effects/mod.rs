//! Effects system for tracking computational properties of method calls.
//!
//! This module implements an effects system similar to Julia's `Core.Compiler.Effects`
//! to track properties like purity, side effects, termination, and exception behavior.
//! These properties enable optimization opportunities and safety guarantees.

pub mod inference;
pub mod propagation;

/// Effect bit representing a tri-state boolean property.
///
/// Effects can be:
/// - AlwaysTrue: Property is guaranteed to hold
/// - AlwaysFalse: Property is guaranteed not to hold
/// - Conditional: Property may or may not hold (conservative approximation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectBit {
    /// Property is guaranteed to hold
    AlwaysTrue,
    /// Property is guaranteed not to hold
    AlwaysFalse,
    /// Property may or may not hold (conservative)
    Conditional,
}

impl EffectBit {
    /// Returns true if the effect bit is AlwaysTrue
    pub fn is_always_true(&self) -> bool {
        matches!(self, EffectBit::AlwaysTrue)
    }

    /// Returns true if the effect bit is AlwaysFalse
    pub fn is_always_false(&self) -> bool {
        matches!(self, EffectBit::AlwaysFalse)
    }

    /// Returns true if the effect bit is Conditional
    pub fn is_conditional(&self) -> bool {
        matches!(self, EffectBit::Conditional)
    }

    /// Combine two effect bits conservatively
    /// - AlwaysTrue & AlwaysTrue => AlwaysTrue
    /// - AlwaysFalse & AlwaysFalse => AlwaysFalse
    /// - Otherwise => Conditional
    pub fn merge(&self, other: &EffectBit) -> EffectBit {
        match (self, other) {
            (EffectBit::AlwaysTrue, EffectBit::AlwaysTrue) => EffectBit::AlwaysTrue,
            (EffectBit::AlwaysFalse, EffectBit::AlwaysFalse) => EffectBit::AlwaysFalse,
            _ => EffectBit::Conditional,
        }
    }
}

/// Computational effects tracking for method calls.
///
/// Based on Julia's `Core.Compiler.Effects` (julia/Compiler/src/effects.jl)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Effects {
    /// Returns same result for same inputs (referentially transparent)
    pub consistent: EffectBit,

    /// No externally visible side effects (no IO, no global mutation)
    pub effect_free: EffectBit,

    /// Guaranteed not to throw exceptions
    pub nothrow: bool,

    /// Guaranteed to terminate (no infinite loops)
    pub terminates: bool,

    /// Doesn't access task-local state
    pub notaskstate: bool,

    /// Doesn't access external mutable memory
    pub inaccessiblememonly: bool,

    /// No undefined behavior
    pub noub: bool,

    /// Doesn't use overlay methods
    pub nonoverlayed: bool,

    /// No runtime calls
    pub nortcall: bool,
}

impl Effects {
    /// Create effects with all properties guaranteed
    pub fn total() -> Self {
        Self {
            consistent: EffectBit::AlwaysTrue,
            effect_free: EffectBit::AlwaysTrue,
            nothrow: true,
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: true,
            noub: true,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects with no guarantees (most conservative)
    pub fn arbitrary() -> Self {
        Self {
            consistent: EffectBit::AlwaysFalse,
            effect_free: EffectBit::AlwaysFalse,
            nothrow: false,
            terminates: false,
            notaskstate: false,
            inaccessiblememonly: false,
            noub: false,
            nonoverlayed: false,
            nortcall: false,
        }
    }

    /// Create effects for pure arithmetic operations
    /// (consistent, effect-free, no throw, terminates)
    pub fn pure_arithmetic() -> Self {
        Self {
            consistent: EffectBit::AlwaysTrue,
            effect_free: EffectBit::AlwaysTrue,
            nothrow: true,
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: true,
            noub: true,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects for operations with side effects (IO, global mutation)
    pub fn with_side_effects() -> Self {
        Self {
            consistent: EffectBit::AlwaysFalse,
            effect_free: EffectBit::AlwaysFalse,
            nothrow: false,
            terminates: true,
            notaskstate: false,
            inaccessiblememonly: false,
            noub: true,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects for array indexing operations
    /// (may throw bounds error, otherwise pure)
    pub fn array_getindex() -> Self {
        Self {
            consistent: EffectBit::AlwaysTrue,
            effect_free: EffectBit::AlwaysTrue,
            nothrow: false, // Can throw BoundsError
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: true,
            noub: false, // Out-of-bounds is undefined
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects for array mutation operations
    /// (mutates array, may throw bounds error)
    pub fn array_setindex() -> Self {
        Self {
            consistent: EffectBit::AlwaysFalse,  // Mutates state
            effect_free: EffectBit::AlwaysFalse, // Side effect
            nothrow: false,                      // Can throw BoundsError
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: false, // Mutates memory
            noub: false,                // Out-of-bounds is undefined
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Merge two effects conservatively (pessimistic combination)
    pub fn merge(&self, other: &Effects) -> Effects {
        Effects {
            consistent: self.consistent.merge(&other.consistent),
            effect_free: self.effect_free.merge(&other.effect_free),
            nothrow: self.nothrow && other.nothrow,
            terminates: self.terminates && other.terminates,
            notaskstate: self.notaskstate && other.notaskstate,
            inaccessiblememonly: self.inaccessiblememonly && other.inaccessiblememonly,
            noub: self.noub && other.noub,
            nonoverlayed: self.nonoverlayed && other.nonoverlayed,
            nortcall: self.nortcall && other.nortcall,
        }
    }

    /// Check if the operation is pure (consistent, effect-free, no throw)
    pub fn is_pure(&self) -> bool {
        self.consistent.is_always_true() && self.effect_free.is_always_true() && self.nothrow
    }

    /// Check if the operation is guaranteed to terminate
    pub fn is_total(&self) -> bool {
        self.nothrow && self.terminates
    }

    /// Check if the operation can be eliminated (dead code elimination)
    pub fn is_removable(&self) -> bool {
        self.effect_free.is_always_true() && self.nothrow && self.terminates
    }

    /// Check if the operation can be constant-folded
    pub fn is_foldable(&self) -> bool {
        self.consistent.is_always_true()
            && self.effect_free.is_always_true()
            && self.terminates
            && self.inaccessiblememonly
    }
}

impl Default for Effects {
    /// Default effects are conservative (arbitrary)
    fn default() -> Self {
        Self::arbitrary()
    }
}

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
