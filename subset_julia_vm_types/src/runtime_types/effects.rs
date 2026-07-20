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

    /// `AlwaysFalse`-absorbing merge, matching upstream `merge_effectbits`
    /// (`julia/Compiler/src/effects.jl:288-291`) for the UInt8 tri-state bits:
    ///
    /// ```text
    /// function merge_effectbits(old::UInt8, new::UInt8)
    ///     if old === ALWAYS_FALSE || new === ALWAYS_FALSE
    ///         return ALWAYS_FALSE
    ///     end
    ///     return old | new
    /// end
    /// ```
    ///
    /// An `AlwaysFalse` operand wins unconditionally: a property proven false
    /// on one merge input can never be un-proven by merging in a branch that
    /// merely fails to disprove it. This is deliberately NOT the same join as
    /// [`EffectBit::merge`] (used today by `consistent`/`effect_free`, whose
    /// symmetric `(AlwaysTrue, AlwaysFalse) => Conditional` join predates any
    /// consumer that discharges `Conditional` back to a stronger guarantee —
    /// changing it retroactively risks unrelated regressions, so it is left
    /// alone here; Issue #9496 scopes only the new `noub` consumer).
    /// [`Effects::noub`] is the first sjulia bit whose `Conditional` state is
    /// actually discharged by a consumer ([`Effects::is_foldable`]'s
    /// `NOUB_IF_NOINBOUNDS`-equivalent branch), so joining the wrong direction
    /// here would let an `AlwaysFalse` (proven-UB-possible) branch get
    /// silently upgraded to foldable by merging with an unrelated proven-safe
    /// branch — see the `noub_merge_af_absorbs_alwaystrue_issue_9496`
    /// trip-wire.
    pub fn merge_af_absorbing(&self, other: &EffectBit) -> EffectBit {
        match (self, other) {
            (EffectBit::AlwaysFalse, _) | (_, EffectBit::AlwaysFalse) => EffectBit::AlwaysFalse,
            (EffectBit::AlwaysTrue, EffectBit::AlwaysTrue) => EffectBit::AlwaysTrue,
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

    /// No undefined behavior. Tri-stated (Issue #9496), mirroring upstream's
    /// `noub::UInt8` (`julia/Compiler/src/effects.jl:169-176`):
    /// `AlwaysTrue`/`AlwaysFalse` map to upstream `ALWAYS_TRUE`/`ALWAYS_FALSE`;
    /// `Conditional` maps to upstream `NOUB_IF_NOINBOUNDS` — "no UB as long as
    /// this method's `@boundscheck` code is not elided". [`Effects::is_foldable`]
    /// accepts `Conditional` here exactly as upstream's `is_foldable` accepts
    /// `is_noub_if_noinbounds` (`effects.jl:306-311`); merge it with
    /// [`EffectBit::merge_af_absorbing`], not the plain
    /// [`EffectBit::merge`] (see that method's doc for why).
    pub noub: EffectBit,

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
            noub: EffectBit::AlwaysTrue,
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
            noub: EffectBit::AlwaysFalse,
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
            noub: EffectBit::AlwaysTrue,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects for deterministic, effect-free operations that can throw
    /// for some input values (e.g. `div`, `rem`, `sqrt` domain errors).
    pub fn effect_free_may_throw() -> Self {
        Self {
            consistent: EffectBit::AlwaysTrue,
            effect_free: EffectBit::AlwaysTrue,
            nothrow: false,
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: true,
            noub: EffectBit::AlwaysTrue,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Effects for operations that allocate a fresh mutable object (`zeros`,
    /// `ones`, `fill`, `similar`, `copy`, `collect`, ...). Effect-free and
    /// nothrow, but deliberately NOT `:consistent`: each call returns an
    /// independent, non-egal, mutable result, so two textually-identical calls
    /// must never be merged by CSE or hoisted/const-folded into a shared value.
    /// Sharing such an allocation made `a = zeros(n); b = zeros(n)` alias the
    /// same array, which silently corrupted any code that mutates the two
    /// independently (Issue #7176).
    pub fn allocating() -> Self {
        Self {
            consistent: EffectBit::AlwaysFalse,
            effect_free: EffectBit::AlwaysTrue,
            nothrow: true,
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: true,
            noub: EffectBit::AlwaysTrue,
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
            noub: EffectBit::AlwaysTrue,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects for array indexing operations (may throw `BoundsError`,
    /// otherwise pure). `noub` is `Conditional` (upstream `NOUB_IF_NOINBOUNDS`,
    /// Issue #9496), not `AlwaysFalse`: sjulia's default `getindex` bytecode
    /// (`Instr::IndexLoad`) always performs the runtime bounds check — an
    /// out-of-range index raises `BoundsError`, not undefined behavior. UB is
    /// only reachable through the compiler's own statically-proven-in-bounds
    /// fast path (`Instr::IndexLoadInbounds`, gated by
    /// `CoreCompiler::is_proven_inbounds_index`), exactly the condition
    /// upstream's `NOUB_IF_NOINBOUNDS` models: this method call site is UB
    /// only if the (elsewhere-proven) `@inbounds`-equivalent context is wrong.
    pub fn array_getindex() -> Self {
        Self {
            consistent: EffectBit::AlwaysTrue,
            effect_free: EffectBit::AlwaysTrue,
            nothrow: false,
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: true,
            noub: EffectBit::Conditional,
            nonoverlayed: true,
            nortcall: true,
        }
    }

    /// Create effects for array mutation operations (mutates array, may throw
    /// `BoundsError`). `noub` is `Conditional`, mirroring [`array_getindex`]'s
    /// reasoning: `Instr::IndexStore` always bounds-checks; only the
    /// proven-in-bounds `Instr::IndexStoreInbounds` fast path can be UB if its
    /// static proof were ever wrong (Issue #9496).
    ///
    /// [`array_getindex`]: Self::array_getindex
    pub fn array_setindex() -> Self {
        Self {
            consistent: EffectBit::AlwaysFalse,
            effect_free: EffectBit::AlwaysFalse,
            nothrow: false,
            terminates: true,
            notaskstate: true,
            inaccessiblememonly: false,
            noub: EffectBit::Conditional,
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
            // AF-absorbing (Issue #9496): see `EffectBit::merge_af_absorbing`.
            noub: self.noub.merge_af_absorbing(&other.noub),
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

    /// Check if the operation can be constant-folded.
    ///
    /// The `noub` clause mirrors upstream `is_foldable`
    /// (`julia/Compiler/src/effects.jl:306-311`):
    /// `is_consistent(effects) && (is_noub(effects) || is_noub_if_noinbounds(effects))
    /// && is_effect_free(effects) && is_terminates(effects)`. Accepting
    /// `Conditional` (upstream's `NOUB_IF_NOINBOUNDS`) here — not just
    /// `AlwaysTrue` — is exactly the tri-state refinement Issue #9496 adds:
    /// folding elides the call entirely, so a bounds check that would merely
    /// have been *skipped* under a proven-in-bounds fast path never runs
    /// either way, and the call's own semantics (throw a catchable
    /// `BoundsError` on the general, non-elided path) are preserved by
    /// `nothrow` — which `is_foldable` deliberately does not gate on, matching
    /// upstream.
    ///
    /// `inaccessiblememonly` is additionally required here; upstream's own
    /// `is_foldable` does not check it explicitly (relying on it being implied
    /// by `is_consistent` in practice — see the comment on
    /// `is_foldable` in effects.jl). Keeping this extra, strictly-narrowing
    /// check is a pre-existing sjulia divergence, out of scope for #9496.
    pub fn is_foldable(&self) -> bool {
        self.consistent.is_always_true()
            && (self.noub.is_always_true() || self.noub.is_conditional())
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
mod noub_tristate_tests {
    //! Soundness trip-wires for the `noub` tri-state (Issue #9496), mirroring
    //! the #8441 trip-wire / #9439 pattern: every precision increase gets a
    //! test proving it does not over-claim, plus a test proving the intended
    //! discharge actually fires.

    use super::{EffectBit, Effects};

    /// MUST-NOT-discharge trip-wire: an `AlwaysFalse` (proven-UB-possible)
    /// noub must absorb through merge, never get diluted to `Conditional` by
    /// an unrelated proven-safe branch. This is the exact join direction
    /// `EffectBit::merge` (used by `consistent`/`effect_free`) gets wrong for
    /// this purpose — `merge_af_absorbing` exists specifically to avoid it.
    #[test]
    fn noub_merge_af_absorbs_alwaystrue_issue_9496() {
        assert_eq!(
            EffectBit::AlwaysFalse.merge_af_absorbing(&EffectBit::AlwaysTrue),
            EffectBit::AlwaysFalse,
            "an AlwaysFalse noub must not be upgraded by merging with AlwaysTrue"
        );
        assert_eq!(
            EffectBit::AlwaysTrue.merge_af_absorbing(&EffectBit::AlwaysFalse),
            EffectBit::AlwaysFalse,
            "merge_af_absorbing must be symmetric in which operand is AlwaysFalse"
        );
        assert_eq!(
            EffectBit::AlwaysFalse.merge_af_absorbing(&EffectBit::Conditional),
            EffectBit::AlwaysFalse,
            "an AlwaysFalse noub must not be upgraded by merging with Conditional"
        );
    }

    /// MUST-NOT-discharge trip-wire, at the `Effects`/`is_foldable` level: a
    /// function whose body merges a proven-UB-possible branch (noub =
    /// AlwaysFalse) with an otherwise-total branch must NOT become foldable.
    /// If `Effects::merge` ever regressed to using the plain (non-absorbing)
    /// `EffectBit::merge` for `noub`, this over-claims: AlwaysFalse ⊔
    /// AlwaysTrue would land on `Conditional`, which `is_foldable` treats as
    /// discharged.
    #[test]
    fn noub_alwaysfalse_branch_blocks_foldable_after_merge_issue_9496() {
        let proven_ub_possible = Effects {
            noub: EffectBit::AlwaysFalse,
            ..Effects::total()
        };
        let proven_safe = Effects::total();
        let merged = proven_ub_possible.merge(&proven_safe);
        assert_eq!(
            merged.noub,
            EffectBit::AlwaysFalse,
            "merge must keep the AlwaysFalse noub, not upgrade it"
        );
        assert!(
            !merged.is_foldable(),
            "a method that may execute UB in one merged branch must never be foldable: {merged:?}"
        );
    }

    /// MUST-discharge trip-wire: `Conditional` (`NOUB_IF_NOINBOUNDS`-equivalent)
    /// noub, combined with the rest of the properties `is_foldable` requires,
    /// must actually be accepted — otherwise the tri-state refinement is dead
    /// weight (Issue #9496 §"gate on a measured DCE/CSE hit-rate improvement").
    #[test]
    fn noub_conditional_is_discharged_by_is_foldable_issue_9496() {
        let conditional_noub = Effects {
            noub: EffectBit::Conditional,
            ..Effects::total()
        };
        assert!(
            conditional_noub.is_foldable(),
            "Conditional noub (NOUB_IF_NOINBOUNDS-equivalent) must be accepted by is_foldable \
             when every other property is proven: {conditional_noub:?}"
        );
    }

    /// `array_getindex`/`array_setindex` are the flagship upstream
    /// `NOUB_IF_NOINBOUNDS` examples (their real Base bodies use
    /// `@boundscheck`) — verify sjulia's presets carry the `Conditional`
    /// classification, not `AlwaysFalse` or `AlwaysTrue`.
    #[test]
    fn array_index_presets_are_conditional_noub_not_alwaysfalse_issue_9496() {
        assert_eq!(Effects::array_getindex().noub, EffectBit::Conditional);
        assert_eq!(Effects::array_setindex().noub, EffectBit::Conditional);
    }

    /// Regression lock: the `noub` gate added to `is_foldable` must not change
    /// the foldability verdict for any existing preset (the "measurement gate"
    /// for this PR — see the PR description for the corresponding whole-Base
    /// corpus count). `array_getindex` was accidentally foldable before this
    /// change (the old `is_foldable` never consulted `noub` at all); it must
    /// stay foldable now that `noub` is consulted, because the classification
    /// was reclassified from `AlwaysFalse` to `Conditional` in lock-step.
    #[test]
    fn is_foldable_verdicts_unchanged_across_noub_gate_issue_9496() {
        let cases: &[(&str, Effects, bool)] = &[
            ("total", Effects::total(), true),
            ("arbitrary", Effects::arbitrary(), false),
            ("pure_arithmetic", Effects::pure_arithmetic(), true),
            (
                "effect_free_may_throw",
                Effects::effect_free_may_throw(),
                true,
            ),
            ("allocating", Effects::allocating(), false),
            ("with_side_effects", Effects::with_side_effects(), false),
            ("array_getindex", Effects::array_getindex(), true),
            ("array_setindex", Effects::array_setindex(), false),
        ];
        for (name, effects, expected_foldable) in cases {
            assert_eq!(
                effects.is_foldable(),
                *expected_foldable,
                "{name}: is_foldable() verdict changed unexpectedly: {effects:?}"
            );
        }
    }
}
