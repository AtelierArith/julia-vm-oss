//! `AbstractLattice` trait — the unified home for the lattice algebra.
//!
//! Upstream Julia organizes its type-inference lattice through an
//! `AbstractLattice` abstraction (`julia/Compiler/src/abstractlattice.jl`):
//! the lattice operations (`⊑`, `tmeet`, `tmerge`, `widenlattice`) are
//! grouped behind a single dispatchable interface instead of living as a
//! loose collection of free functions. This module mirrors that structure
//! for SubsetJuliaVM by gathering the core operations that used to be
//! standalone inherent methods (`join`, `join_limited`, `meet`,
//! `is_subtype_of`, `subtract`, `widen_union`) onto one trait.
//!
//! The trait is intentionally **lightweight** (Issue #6605 / parent epic
//! #5922, design note D20): the no-JIT iOS runtime does not need upstream's
//! full lattice tower (IPO / constant-propagation / `Conditional` /
//! `LimitedAccuracy` sub-lattices stacked via `widenlattice`). A single
//! lattice over [`LatticeType`] suffices, so the trait exposes only the
//! operations the inference engine actually consumes.
//!
//! This is a **behavior-preserving** consolidation: the operation bodies
//! moved here verbatim from `ops.rs`, and the public inherent methods on
//! [`LatticeType`] remain as thin forwarders so existing call sites and
//! tests are untouched.
//!
//! # Operation map (sjulia ⇆ upstream)
//!
//! | sjulia trait method | lattice symbol | upstream analogue        |
//! |---------------------|----------------|--------------------------|
//! | [`AbstractLattice::join`]         | `⊔` | `tmerge(𝕃, a, b)`        |
//! | [`AbstractLattice::join_limited`] | `⊔` | `tmerge` + `limit_type_size` |
//! | [`AbstractLattice::meet`]         | `⊓` | `tmeet(𝕃, a, b)`         |
//! | [`AbstractLattice::is_subtype`]   | `⊑` | `⊑(𝕃, a, b)`             |
//! | [`AbstractLattice::subtract`]     | `∖` | `typesubtract`           |
//! | [`AbstractLattice::widen`]        | —   | `widenlattice` (union widening) |

use super::types::LatticeType;

/// The lattice algebra over abstract types.
///
/// This trait is the single, dispatchable home for the meet/join/widen
/// family of operations, mirroring upstream Julia's `AbstractLattice`
/// abstraction while staying lightweight for the no-JIT VM (no IPO /
/// const-prop / `Conditional` / `LimitedAccuracy` sub-lattices).
///
/// All methods are total and operate on values of the implementing lattice
/// element type (`Self`). The semantics are documented per-method on the
/// [`LatticeType`] implementation in `ops.rs`.
pub trait AbstractLattice {
    /// Join (`⊔`, least upper bound): the most specific common supertype.
    ///
    /// Equivalent to upstream `tmerge`. See the implementation for the
    /// per-variant rules and examples.
    fn join(&self, other: &Self) -> Self;

    /// Comparison-aware join: `self ⊔ other`, then bound the result's
    /// complexity against `compare_to` via Julia-style `limit_type_size`.
    ///
    /// Equivalent to upstream `tmerge` followed by `limit_type_size`.
    fn join_limited(&self, other: &Self, compare_to: &Self) -> Self;

    /// Meet (`⊓`, greatest lower bound): the type intersection.
    ///
    /// Equivalent to upstream `tmeet`.
    fn meet(&self, other: &Self) -> Self;

    /// Subtype relation (`⊑`): is every value of `self` also of `other`?
    ///
    /// Equivalent to upstream `⊑(𝕃, a, b)`.
    fn is_subtype(&self, other: &Self) -> bool;

    /// Type subtraction (`∖`): `self` with everything in `other` removed.
    ///
    /// Used for control-flow sensitive narrowing.
    fn subtract(&self, other: &Self) -> Self;

    /// Widen this lattice element toward a sound, bounded supertype.
    ///
    /// This is the union-widening operation (`widenlattice`-flavored): when a
    /// value's complexity would grow without bound it is collapsed to an
    /// abstract numeric supertype (`Integer` / `AbstractFloat` / `Number`)
    /// or `Top`. Non-`Union` elements are returned unchanged.
    fn widen(&self) -> Self;
}

impl AbstractLattice for LatticeType {
    #[inline]
    fn join(&self, other: &Self) -> Self {
        LatticeType::lattice_join(self, other)
    }

    #[inline]
    fn join_limited(&self, other: &Self, compare_to: &Self) -> Self {
        LatticeType::lattice_join_limited(self, other, compare_to)
    }

    #[inline]
    fn meet(&self, other: &Self) -> Self {
        LatticeType::lattice_meet(self, other)
    }

    #[inline]
    fn is_subtype(&self, other: &Self) -> bool {
        LatticeType::lattice_is_subtype(self, other)
    }

    #[inline]
    fn subtract(&self, other: &Self) -> Self {
        LatticeType::lattice_subtract(self, other)
    }

    #[inline]
    fn widen(&self) -> Self {
        LatticeType::lattice_widen(self)
    }
}
