//! Lattice operations for type inference.
//!
//! This module implements the core operations on the type lattice:
//! - join (⊔): least upper bound (union of types)
//! - meet (⊓): greatest lower bound (intersection of types)
//! - is_subtype_of (⊑): subtype relation
//! - subtract: type subtraction for narrowing
//!
//! These operations follow Julia's type lattice semantics.
//!
//! # Where the algebra lives (Issue #6605)
//!
//! The meet/join/widen family is consolidated behind the
//! [`AbstractLattice`](super::abstract_lattice::AbstractLattice) trait,
//! mirroring upstream Julia's `AbstractLattice` abstraction
//! (`julia/Compiler/src/abstractlattice.jl`). The operation *bodies* are the
//! `lattice_*` associated functions in this module — the single source of
//! truth. Two thin layers forward to them:
//!
//! - the trait `impl` in `abstract_lattice.rs`, and
//! - the public inherent methods below (`join`, `meet`, …) kept for
//!   source-compatibility with existing call sites and tests.
//!
//! Because the bodies are shared, the inherent-method API and the trait API
//! always agree by construction — there is no second implementation to drift.

use super::types::{ConcreteType, LatticeType};
use super::widening::{limit_type_size, MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH};
use crate::compile::diagnostics::{emit_conditional_join, emit_union_widened, DiagnosticReason};
use crate::inference_core::CorePrimitive;
use crate::inference_core::{CoreAbstract, CoreSubtypeEngine, CoreType};
use std::collections::BTreeSet;

/// Public, source-compatible inherent forwarders.
///
/// These delegate to the canonical `lattice_*` bodies so that existing
/// `value.join(&other)` / `value.meet(&other)` call sites keep working
/// without importing the [`AbstractLattice`](super::abstract_lattice::AbstractLattice)
/// trait. They carry no logic of their own.
impl LatticeType {
    /// Join operation (⊔): see [`LatticeType::lattice_join`].
    #[inline]
    pub fn join(&self, other: &LatticeType) -> LatticeType {
        Self::lattice_join(self, other)
    }

    /// Comparison-aware join: see [`LatticeType::lattice_join_limited`].
    #[inline]
    pub fn join_limited(&self, other: &LatticeType, compare_to: &LatticeType) -> LatticeType {
        Self::lattice_join_limited(self, other, compare_to)
    }

    /// Meet operation (⊓): see [`LatticeType::lattice_meet`].
    #[inline]
    pub fn meet(&self, other: &LatticeType) -> LatticeType {
        Self::lattice_meet(self, other)
    }

    /// Subtype relation (⊑): see [`LatticeType::lattice_is_subtype`].
    #[inline]
    pub fn is_subtype_of(&self, other: &LatticeType) -> bool {
        Self::lattice_is_subtype(self, other)
    }

    /// Type subtraction (∖): see [`LatticeType::lattice_subtract`].
    #[inline]
    pub fn subtract(&self, other: &LatticeType) -> LatticeType {
        Self::lattice_subtract(self, other)
    }
}

/// Canonical lattice-operation bodies (Issue #6605).
///
/// These `lattice_*` functions are the single implementation consumed by
/// both the public inherent forwarders above and the
/// [`AbstractLattice`](super::abstract_lattice::AbstractLattice) trait impl.
impl LatticeType {
    /// Join operation (⊔): compute the least upper bound of two types.
    ///
    /// This creates a type that is a supertype of both inputs.
    /// In Julia, this corresponds to creating a Union type.
    ///
    /// # Examples
    /// ```text
    /// Int64.join(Float64) = Union{Int64, Float64}
    /// Int64.join(Int64) = Int64
    /// Const(42).join(Const(42)) = Const(42)
    /// Const(42).join(Const(43)) = Concrete(Int64)
    /// Const(42).join(Int64) = Concrete(Int64)
    /// Bottom.join(T) = T
    /// T.join(Top) = Top
    /// ```
    pub(crate) fn lattice_join(&self, other: &LatticeType) -> LatticeType {
        if self == other {
            return self.clone();
        }
        Self::bound_raw_join(self.join_raw(other))
    }

    pub(crate) fn join_raw(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Bottom is the identity element for join
            (LatticeType::Bottom, t) | (t, LatticeType::Bottom) => t.clone(),

            // Top is the absorbing element for join
            (LatticeType::Top, _) | (_, LatticeType::Top) => LatticeType::Top,

            // Same constant value → keep constant
            (LatticeType::Const(a), LatticeType::Const(b)) if a == b => {
                LatticeType::Const(a.clone())
            }

            // Different constant values → widen to concrete type
            (LatticeType::Const(a), LatticeType::Const(b)) => {
                LatticeType::Concrete(a.to_concrete_type())
                    .join_raw(&LatticeType::Concrete(b.to_concrete_type()))
            }

            // Const + Concrete → widen to concrete
            (LatticeType::Const(cv), LatticeType::Concrete(ct))
            | (LatticeType::Concrete(ct), LatticeType::Const(cv)) => {
                if &cv.to_concrete_type() == ct {
                    LatticeType::Concrete(ct.clone())
                } else {
                    // Different concrete types
                    LatticeType::Concrete(cv.to_concrete_type())
                        .join_raw(&LatticeType::Concrete(ct.clone()))
                }
            }

            // Const + Union → widen const to concrete and join with union
            (LatticeType::Const(cv), LatticeType::Union(us))
            | (LatticeType::Union(us), LatticeType::Const(cv)) => {
                let concrete = cv.to_concrete_type();
                let mut new_set = us.clone();
                new_set.insert(concrete);
                Self::raw_union(new_set)
            }

            // Same concrete type
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
                LatticeType::Concrete(a.clone())
            }

            // Different concrete types → the least upper bound is the more
            // general operand when one is a subtype of the other, otherwise a
            // Union. Use the SAME subtype notion as `is_subtype_of` — the core
            // hierarchy (`concrete_is_subtype`, e.g. `Int64 <: Integer`, so
            // `join(Int64, Integer) = Integer`) plus tuple/vararg awareness
            // (`concrete_tuple_subtype`, Issue #3511, keeps
            // `Tuple{Int,Int} ⊔ Tuple{Int, Vararg{Int}}` collapsed to the
            // latter). Previously this arm only consulted `concrete_tuple_subtype`,
            // leaving redundant union members like `Union{Int64, Integer}` that
            // are semantically just the supertype (Issue #5940).
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                if concrete_is_subtype(a, b) || concrete_tuple_subtype(a, b) {
                    LatticeType::Concrete(b.clone())
                } else if concrete_is_subtype(b, a) || concrete_tuple_subtype(b, a) {
                    LatticeType::Concrete(a.clone())
                } else {
                    let mut set = BTreeSet::new();
                    set.insert(a.clone());
                    set.insert(b.clone());
                    Self::raw_union(set)
                }
            }

            // Union + Concrete
            (LatticeType::Union(us), LatticeType::Concrete(c))
            | (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                let mut new_set = us.clone();
                new_set.insert(c.clone());
                Self::raw_union(new_set)
            }

            // Union + Union
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let combined: BTreeSet<_> = a.union(b).cloned().collect();
                Self::raw_union(combined)
            }

            // `Conditional ⊔ Conditional` (Issue #3503).
            //
            // When both Conditional values reference the SAME slot, we can
            // join branch-wise — the narrowing is preserved. This mirrors
            // the `tmerge` rule for Julia's `Conditional` in
            // `julia/Compiler/src/typelattice.jl`, which merges
            // `Conditional(slot, vt1, et1) ⊔ Conditional(slot, vt2, et2)` to
            // `Conditional(slot, vt1 ⊔ vt2, et1 ⊔ et2)`.
            //
            // When the slots differ, the two pieces of branch information
            // cannot be combined precisely — fall back to the widened
            // `then ⊔ else` form for both sides.
            (
                LatticeType::Conditional {
                    slot: s1,
                    then_type: t1,
                    else_type: e1,
                },
                LatticeType::Conditional {
                    slot: s2,
                    then_type: t2,
                    else_type: e2,
                },
            ) => {
                if s1 == s2 {
                    let then_joined = t1.join_raw(t2);
                    let else_joined = e1.join_raw(e2);
                    LatticeType::make_conditional(s1.clone(), then_joined, else_joined)
                } else {
                    emit_conditional_join();
                    self.widen_conditional()
                        .join_raw(&other.widen_conditional())
                }
            }

            // `Conditional ⊔ T` (or `T ⊔ Conditional`): drop the conditional
            // info and join the widened branches with the other operand.
            // This is `widenconditional` followed by ordinary join (Issue
            // #3503), and is strictly better than the previous "collapse to
            // Top" fallback — e.g., `Conditional(x; Int, Nothing) ⊔ String`
            // now becomes `Union{Int, Nothing, String}` instead of `Any`.
            (LatticeType::Conditional { .. }, _) | (_, LatticeType::Conditional { .. }) => {
                let lhs = self.widen_conditional();
                let rhs = other.widen_conditional();
                lhs.join_raw(&rhs)
            }
        }
    }

    /// Comparison-aware join: compute `self ⊔ other` and then bound the
    /// result's complexity using Julia-style `limit_type_size` against
    /// `compare_to`.
    ///
    /// This is the recommended entry point at inference call sites where a
    /// reference type is naturally available (e.g., the previously-known
    /// type for the same SSA value before a loop body re-execution). When
    /// no comparison context is available, prefer the plain [`Self::join`].
    ///
    /// Issue #3507: this is part of the staged migration away from the
    /// fixed-length union widening that lives inside `simplify_union`.
    pub(crate) fn lattice_join_limited(
        &self,
        other: &LatticeType,
        compare_to: &LatticeType,
    ) -> LatticeType {
        let joined = self.join_raw(other);
        limit_type_size(
            &joined,
            Some(compare_to),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        )
    }

    /// Meet operation (⊓): compute the greatest lower bound of two types.
    ///
    /// This creates a type that is a subtype of both inputs.
    /// In Julia, this corresponds to type intersection.
    ///
    /// # Examples
    /// ```text
    /// Int64.meet(Float64) = Bottom
    /// Int64.meet(Int64) = Int64
    /// Const(42).meet(Const(42)) = Const(42)
    /// Const(42).meet(Const(43)) = Bottom
    /// Const(42).meet(Int64) = Const(42)
    /// Union{Int, Float}.meet(Int) = Int
    /// Top.meet(T) = T
    /// ```
    pub(crate) fn lattice_meet(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Top is the identity element for meet
            (LatticeType::Top, t) | (t, LatticeType::Top) => t.clone(),

            // Bottom is the absorbing element for meet
            (LatticeType::Bottom, _) | (_, LatticeType::Bottom) => LatticeType::Bottom,

            // Same constant → keep constant
            (LatticeType::Const(a), LatticeType::Const(b)) if a == b => {
                LatticeType::Const(a.clone())
            }

            // Different constants → Bottom (empty intersection)
            (LatticeType::Const(_), LatticeType::Const(_)) => LatticeType::Bottom,

            // Const ⊓ Concrete → Const if types match, Bottom otherwise
            (LatticeType::Const(cv), LatticeType::Concrete(ct))
            | (LatticeType::Concrete(ct), LatticeType::Const(cv)) => {
                if &cv.to_concrete_type() == ct {
                    LatticeType::Const(cv.clone())
                } else {
                    LatticeType::Bottom
                }
            }

            // Same concrete type
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
                LatticeType::Concrete(a.clone())
            }

            // Different concrete types → the greatest lower bound is the more
            // specific operand when one is a subtype of the other, otherwise
            // Bottom (empty intersection). Use the SAME subtype notion as
            // `is_subtype_of` above — the core hierarchy (`concrete_is_subtype`,
            // e.g. `Int64 <: Integer`) plus tuple/vararg awareness
            // (`concrete_tuple_subtype`, Issue #3511). Previously this arm only
            // consulted `concrete_tuple_subtype`, so `meet(Int64, Integer)`
            // collapsed to Bottom instead of Int64, breaking conditional
            // narrowing and the `a ⊓ b ⊑ a` lattice invariant (Issue #5940).
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                if concrete_is_subtype(a, b) || concrete_tuple_subtype(a, b) {
                    LatticeType::Concrete(a.clone())
                } else if concrete_is_subtype(b, a) || concrete_tuple_subtype(b, a) {
                    LatticeType::Concrete(b.clone())
                } else {
                    LatticeType::Bottom
                }
            }

            // Union and Concrete intersection
            (LatticeType::Union(us), LatticeType::Concrete(c))
            | (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                if us.contains(c) {
                    LatticeType::Concrete(c.clone())
                } else {
                    LatticeType::Bottom
                }
            }

            // Union and Union intersection
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let intersection: BTreeSet<_> = a.intersection(b).cloned().collect();
                if intersection.is_empty() {
                    LatticeType::Bottom
                } else if intersection.len() == 1 {
                    if let Some(only) = intersection.into_iter().next() {
                        LatticeType::Concrete(only)
                    } else {
                        LatticeType::Bottom
                    }
                } else {
                    LatticeType::Union(intersection)
                }
            }

            // `Conditional ⊓ Conditional` (Issue #3503).
            //
            // When the slots match, we can intersect branch-wise: the
            // resulting Conditional describes a stricter narrowing where
            // BOTH operands' constraints hold. When the slots differ, fall
            // back to widening — the two pieces of branch info can't
            // combine.
            (
                LatticeType::Conditional {
                    slot: s1,
                    then_type: t1,
                    else_type: e1,
                },
                LatticeType::Conditional {
                    slot: s2,
                    then_type: t2,
                    else_type: e2,
                },
            ) => {
                if s1 == s2 {
                    let then_met = t1.meet(t2);
                    let else_met = e1.meet(e2);
                    if matches!(then_met, LatticeType::Bottom)
                        && matches!(else_met, LatticeType::Bottom)
                    {
                        LatticeType::Bottom
                    } else {
                        LatticeType::make_conditional(s1.clone(), then_met, else_met)
                    }
                } else {
                    self.widen_conditional().meet(&other.widen_conditional())
                }
            }

            // `Conditional ⊓ T` / `T ⊓ Conditional` — widen the Conditional
            // first, then continue the meet. This is the analogue of
            // `widenconditional` from Julia's `tmeet` (Issue #3503).
            // Strictly better than the previous `_ => Bottom` fallback,
            // which silently discarded compatible types like
            // `Conditional(x; Int, Nothing) ⊓ Union{Int, Nothing}`.
            (LatticeType::Conditional { .. }, _) | (_, LatticeType::Conditional { .. }) => {
                let lhs = self.widen_conditional();
                let rhs = other.widen_conditional();
                lhs.meet(&rhs)
            }

            // Const ⊓ Union — fall back to per-member intersection: the
            // Const's concrete type must appear in the union for the
            // intersection to be non-empty. Preserves the pre-Issue #3503
            // catch-all behaviour without quietly returning `Bottom` for
            // valid intersections.
            (LatticeType::Const(cv), LatticeType::Union(us))
            | (LatticeType::Union(us), LatticeType::Const(cv)) => {
                if us.contains(&cv.to_concrete_type()) {
                    LatticeType::Const(cv.clone())
                } else {
                    LatticeType::Bottom
                }
            }
        }
    }

    /// Subtype relation (⊑): check if self is a subtype of other.
    ///
    /// Returns true if every value of type `self` is also of type `other`.
    ///
    /// # Examples
    /// ```text
    /// Bottom ⊑ T (for all T)
    /// T ⊑ Top (for all T)
    /// Int64 ⊑ Union{Int64, Float64}
    /// Int64 ⊑ Int64
    /// ```
    pub(crate) fn lattice_is_subtype(&self, other: &LatticeType) -> bool {
        match (self, other) {
            // Bottom is a subtype of everything
            (LatticeType::Bottom, _) => true,

            // Everything is a subtype of Top
            (_, LatticeType::Top) => true,

            // Top is not a subtype of anything except itself
            (LatticeType::Top, _) => false,

            // Const is a subtype of identical Const
            (LatticeType::Const(a), LatticeType::Const(b)) => a == b,

            // Const is a subtype of Concrete if its type matches
            (LatticeType::Const(cv), LatticeType::Concrete(ct)) => &cv.to_concrete_type() == ct,

            // Const is a subtype of Union if its concrete type is in the union
            (LatticeType::Const(cv), LatticeType::Union(us)) => us.contains(&cv.to_concrete_type()),

            // Concrete is never a subtype of a more specific Const
            (LatticeType::Concrete(_), LatticeType::Const(_)) => false,
            // Union is never a subtype of a single Const
            (LatticeType::Union(_), LatticeType::Const(_)) => false,

            // Concrete types must be equal — except for tuple shapes, which
            // support element-wise / Vararg-aware subtyping (Issue #3511).
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                if a == b {
                    return true;
                }
                concrete_is_subtype(a, b) || concrete_tuple_subtype(a, b)
            }

            // Concrete is a subtype of Union if it's an element
            (LatticeType::Concrete(c), LatticeType::Union(us)) => us
                .iter()
                .any(|u| concrete_is_subtype(c, u) || concrete_tuple_subtype(c, u)),

            // Union is a subtype of Union if every left member is accepted by
            // at least one right member.
            (LatticeType::Union(a), LatticeType::Union(b)) => a.iter().all(|left| {
                b.iter().any(|right| {
                    concrete_is_subtype(left, right) || concrete_tuple_subtype(left, right)
                })
            }),

            // Union is a subtype of a single Concrete if all variants are.
            (LatticeType::Union(types), LatticeType::Concrete(concrete)) => {
                types.iter().all(|ty| {
                    concrete_is_subtype(ty, concrete) || concrete_tuple_subtype(ty, concrete)
                })
            }

            // `Conditional <: Conditional` (Issue #3503): a Conditional is
            // a subtype of another iff both branch constraints are
            // tightened. Slots must agree — otherwise the two values
            // describe unrelated narrowings and we widen.
            (
                LatticeType::Conditional {
                    slot: s1,
                    then_type: t1,
                    else_type: e1,
                },
                LatticeType::Conditional {
                    slot: s2,
                    then_type: t2,
                    else_type: e2,
                },
            ) => {
                if s1 == s2 {
                    t1.is_subtype_of(t2) && e1.is_subtype_of(e2)
                } else {
                    self.widen_conditional()
                        .is_subtype_of(&other.widen_conditional())
                }
            }

            // `Conditional <: T` / `T <: Conditional` (Issue #3503).
            // Widen the Conditional and compare. This means a Conditional
            // is a subtype of `T` whenever the join of its branches is.
            // It is reciprocally a supertype of `T` only when both branches
            // accept `T` — captured by `T <: widen(Cond)`.
            (LatticeType::Conditional { .. }, _) | (_, LatticeType::Conditional { .. }) => self
                .widen_conditional()
                .is_subtype_of(&other.widen_conditional()),

            // Anything ⊑ Bottom is false (Bottom only contains itself, and
            // the `(Bottom, _)` arm above already handled `Bottom <: T`).
            // Preserves the pre-Issue #3503 `_ => false` fallback for the
            // `(Const|Concrete|Union, Bottom)` triplet.
            (_, LatticeType::Bottom) => false,
        }
    }

    /// Type subtraction for narrowing: compute `self - other`.
    ///
    /// Used for control-flow sensitive type narrowing.
    /// For example, after checking `x isa Int` is false,
    /// we know x is not Int, so we subtract Int from its type.
    ///
    /// # Examples
    /// ```text
    /// Union{Int, Float, String}.subtract(Int) = Union{Float, String}
    /// Int64.subtract(Int64) = Bottom
    /// Int64.subtract(Float64) = Int64
    /// ```
    pub(crate) fn lattice_subtract(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Subtracting from Bottom or Top
            (LatticeType::Bottom, _) => LatticeType::Bottom,
            (LatticeType::Top, _) => LatticeType::Top, // Conservative

            // Subtracting Bottom or Top
            (t, LatticeType::Bottom) => t.clone(),
            (_, LatticeType::Top) => LatticeType::Bottom, // Everything is removed

            // Concrete - Concrete
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                if a == b {
                    LatticeType::Bottom
                } else {
                    LatticeType::Concrete(a.clone())
                }
            }

            // Concrete - Union
            (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                if us.contains(c) {
                    LatticeType::Bottom
                } else {
                    LatticeType::Concrete(c.clone())
                }
            }

            // Union - Concrete
            (LatticeType::Union(us), LatticeType::Concrete(c)) => {
                let remaining: BTreeSet<_> = us.iter().filter(|t| *t != c).cloned().collect();
                Self::simplify_union(remaining)
            }

            // Union - Union
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let remaining: BTreeSet<_> = a.difference(b).cloned().collect();
                Self::simplify_union(remaining)
            }

            // `Conditional - T` / `T - Conditional` (Issue #3503).
            // Subtraction is performed branch-wise on the LHS Conditional —
            // each branch loses `T` independently, and the resulting
            // Conditional may degenerate to a single type if both branches
            // collapse. When the RHS is also Conditional, widen it first
            // (we can't subtract a per-branch type from a flat type
            // soundly otherwise).
            (
                LatticeType::Conditional {
                    slot,
                    then_type,
                    else_type,
                },
                rhs,
            ) => {
                let rhs_widened = rhs.widen_conditional();
                let new_then = then_type.subtract(&rhs_widened);
                let new_else = else_type.subtract(&rhs_widened);
                LatticeType::make_conditional(slot.clone(), new_then, new_else)
            }
            (lhs, LatticeType::Conditional { .. }) => lhs.subtract(&other.widen_conditional()),

            // Conservative fallback for `Const` and other lattice variants
            // that don't have explicit subtraction rules — preserves the
            // pre-Issue #3503 behaviour where unhandled combinations leave
            // the LHS unchanged. Examples: `Const(42) - Concrete(Int64)`,
            // `Concrete(Int64) - Const(42)`.
            _ => self.clone(),
        }
    }

    /// Simplify a Union type, applying widening if necessary.
    ///
    /// Rules:
    /// - Empty set → Bottom
    /// - Single element → Concrete
    /// - Too many elements (> MAX_UNION_LENGTH) → widen
    /// - Too complex (> MAX_UNION_COMPLEXITY) → widen
    /// - Otherwise → Union
    fn simplify_union(types: BTreeSet<ConcreteType>) -> LatticeType {
        Self::bound_raw_join(Self::raw_union(types))
    }

    fn raw_union(types: BTreeSet<ConcreteType>) -> LatticeType {
        if types.is_empty() {
            return LatticeType::Bottom;
        }

        if types.len() == 1 {
            if let Some(only) = types.into_iter().next() {
                return LatticeType::Concrete(only);
            }
            return LatticeType::Bottom;
        }

        LatticeType::Union(types)
    }

    fn bound_raw_join(joined: LatticeType) -> LatticeType {
        let LatticeType::Union(types) = joined else {
            return joined;
        };

        // Check if widening is needed based on length
        if types.len() > MAX_UNION_LENGTH {
            emit_union_widened(DiagnosticReason::UnionTooLarge(types.len()));
            return Self::widen_union(&types);
        }

        // Check complexity (maximum depth of nested types)
        let complexity = Self::compute_complexity(&types);
        if complexity > MAX_UNION_COMPLEXITY {
            emit_union_widened(DiagnosticReason::UnionTooComplex(complexity));
            return Self::widen_union(&types);
        }

        LatticeType::Union(types)
    }

    /// Widen a Union type to prevent infinite growth.
    ///
    /// Strategy (Issue #3539): widen to a sound abstract numeric supertype
    /// rather than narrowing to `Union{Int64, Float64}`.
    ///
    /// - All members are integers (signed/unsigned/BigInt) → `Integer`
    /// - All members are floats (Float16/32/64/BigFloat) → `AbstractFloat`
    /// - All members are numeric (mixed int/float, including unsigned/big/Bool) → `Number`
    /// - Otherwise → `Top`
    ///
    /// The previous behavior narrowed `Union{UInt64, BigInt, Float32, Bool}`
    /// to just `Union{Int64, Float64}`, dropping unsigned/big numeric
    /// identities and causing wrong arithmetic/dispatch decisions
    /// downstream.
    pub(super) fn widen_union(types: &BTreeSet<ConcreteType>) -> LatticeType {
        if types.is_empty() {
            return LatticeType::Bottom;
        }
        if types.iter().all(|t| t.is_numeric()) {
            if types.iter().all(|t| t.is_integer()) {
                return LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                    CoreAbstract::Integer,
                )));
            }
            if types.iter().all(|t| t.is_float()) {
                return LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                    CoreAbstract::AbstractFloat,
                )));
            }
            return LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Number,
            )));
        }
        // Otherwise, widen to Top
        LatticeType::Top
    }

    /// Lattice-level widening (`widenlattice`-flavored) over a whole
    /// [`LatticeType`] (Issue #6605).
    ///
    /// This is the [`AbstractLattice::widen`](super::abstract_lattice::AbstractLattice::widen)
    /// entry point. It applies the union-widening strategy of
    /// [`Self::widen_union`] when the element is a `Union`, and is the
    /// identity for every other lattice element (there is nothing to widen).
    /// Behavior-preserving: `Union` widening is unchanged, and non-`Union`
    /// elements were never touched by the old free-function API.
    pub(crate) fn lattice_widen(&self) -> LatticeType {
        match self {
            LatticeType::Union(types) => Self::widen_union(types),
            other => other.clone(),
        }
    }

    /// Compute the complexity of a Union (maximum nesting depth).
    fn compute_complexity(types: &BTreeSet<ConcreteType>) -> usize {
        types.iter().map(Self::type_depth).max().unwrap_or(0)
    }

    /// Compute the nesting depth of a type.
    fn type_depth(ty: &ConcreteType) -> usize {
        match ty {
            // Simple types have depth 1
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol))
            | ConcreteType::Pairs
            | ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO))
            | ConcreteType::Expr
            | ConcreteType::QuoteNode
            | ConcreteType::LineNumberNode
            | ConcreteType::GlobalRef
            | ConcreteType::Regex
            | ConcreteType::RegexMatch
            // Abstract types have depth 1
            | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))
            | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer))
            | ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat)) => 1,

            // Composite types have depth 1 + max element depth
            ConcreteType::Array { element , .. } => 1 + Self::type_depth(element),
            ConcreteType::Tuple { elements } => {
                1 + elements.iter().map(Self::type_depth).max().unwrap_or(0)
            }
            // Issue #3511: TupleVararg counts the tail like any other element.
            ConcreteType::TupleVararg { elements, tail } => {
                let elem_depth = elements.iter().map(Self::type_depth).max().unwrap_or(0);
                let tail_depth = Self::type_depth(tail);
                1 + elem_depth.max(tail_depth)
            }
            ConcreteType::NamedTuple { fields } => {
                1 + fields
                    .iter()
                    .map(|(_, ty)| Self::type_depth(ty))
                    .max()
                    .unwrap_or(0)
            }

            // Range, Dict, Set, Generator types
            ConcreteType::Range { element } => 1 + Self::type_depth(element),
            ConcreteType::Dict { key, value } => {
                1 + Self::type_depth(key).max(Self::type_depth(value))
            }
            ConcreteType::Set { element } => 1 + Self::type_depth(element),
            ConcreteType::Generator { element } => 1 + Self::type_depth(element),

            // User-defined and type system types have depth 1
            ConcreteType::Struct { .. }
            | ConcreteType::Function { .. }
            | ConcreteType::Closure { .. }
            | ConcreteType::ComposedFunction { .. }
            | ConcreteType::DataType { .. }
            | ConcreteType::Module { .. }
            // Enum types have depth 1 (they are simple integer-backed types)
            | ConcreteType::Enum { .. } => 1,

            // Any is a top type with depth 1
            ConcreteType::Core(CoreType::Any) => 1,

            // Union types have depth 1 + max element depth
            ConcreteType::UnionOf(types) => {
                1 + types.iter().map(Self::type_depth).max().unwrap_or(0)
            }

            // Core-backed types not folded to dedicated arms are atomic
            // (depth 1) for complexity purposes (Issue #6720, Slice-2 step-1a).
            ConcreteType::Core(_) => 1,
        }
    }
}

/// Element-wise / Vararg-aware tuple subtyping (Issue #3511).
///
/// Returns true iff `a <: b` under Julia's tuple lattice rules:
/// - `Tuple{Ts...}` is a subtype of `Tuple{Us...}` when each `Ti <: Ui`.
/// - `Tuple{T1, ..., Tn}` is a subtype of `Tuple{P1, ..., Pk, Vararg{Q}}`
///   when `n >= k`, the prefix matches element-wise, and every remaining
///   `Ti <: Q`.
/// - `Tuple{P1, ..., Pk, Vararg{Q1}}` is a subtype of
///   `Tuple{P1', ..., Pk', Vararg{Q2}}` when prefixes match element-wise
///   and `Q1 <: Q2`.
///
/// All other pairings (e.g. tuple vs non-tuple, or non-matching prefix
/// arity) return false. Subtyping is delegated to the lattice via
/// `LatticeType::is_subtype_of` so abstract supertypes (`Integer`, etc.)
/// continue to apply.
fn concrete_tuple_subtype(a: &ConcreteType, b: &ConcreteType) -> bool {
    fn elem_sub(a: &ConcreteType, b: &ConcreteType) -> bool {
        if a == b {
            return true;
        }
        LatticeType::Concrete(a.clone()).is_subtype_of(&LatticeType::Concrete(b.clone()))
    }
    match (a, b) {
        (ConcreteType::Tuple { elements: ea }, ConcreteType::Tuple { elements: eb }) => {
            ea.len() == eb.len() && ea.iter().zip(eb.iter()).all(|(x, y)| elem_sub(x, y))
        }
        (
            ConcreteType::Tuple { elements: ea },
            ConcreteType::TupleVararg { elements: eb, tail },
        ) => {
            if ea.len() < eb.len() {
                return false;
            }
            let (prefix, rest) = ea.split_at(eb.len());
            prefix.iter().zip(eb.iter()).all(|(x, y)| elem_sub(x, y))
                && rest.iter().all(|x| elem_sub(x, tail))
        }
        (
            ConcreteType::TupleVararg {
                elements: ea,
                tail: ta,
            },
            ConcreteType::TupleVararg {
                elements: eb,
                tail: tb,
            },
        ) => {
            ea.len() == eb.len()
                && ea.iter().zip(eb.iter()).all(|(x, y)| elem_sub(x, y))
                && elem_sub(ta, tb)
        }
        // Note (Issue #3511): a `TupleVararg` is *not* a subtype of a flat
        // `Tuple{...}` because it has an unbounded number of elements.
        _ => false,
    }
}

fn concrete_is_subtype(a: &ConcreteType, b: &ConcreteType) -> bool {
    CoreSubtypeEngine::new().is_subtype(&CoreType::from(a), &CoreType::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_concrete_same() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = int.join(&int);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_join_concrete_different() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        let result = int.join(&float);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_join_with_bottom() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let bottom = LatticeType::Bottom;

        assert_eq!(int.join(&bottom), int);
        assert_eq!(bottom.join(&int), int);
    }

    #[test]
    fn test_join_with_top() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let top = LatticeType::Top;

        assert_eq!(int.join(&top), LatticeType::Top);
        assert_eq!(top.join(&int), LatticeType::Top);
    }

    #[test]
    fn test_join_concrete_subtype_returns_supertype() {
        // LUB of a concrete type and its abstract supertype is the supertype:
        // join(Int64, Integer) = Integer (not Union{Int64, Integer}). The
        // concrete×concrete join arm previously only consulted tuple-aware
        // subtyping, leaving redundant union members that are semantically the
        // supertype anyway (Issue #5940, symmetric to the meet fix).
        let int64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let integer = LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));

        assert_eq!(int64.join(&integer), integer);
        // join is commutative.
        assert_eq!(integer.join(&int64), integer);
    }

    #[test]
    fn test_meet_concrete_same() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = int.meet(&int);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_meet_concrete_different() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        let result = int.meet(&float);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_meet_concrete_subtype_returns_more_specific() {
        // GLB of a concrete type and its abstract supertype is the concrete
        // type: meet(Int64, Integer) = Int64 (not Bottom). The concrete×concrete
        // meet arm previously only consulted tuple-aware subtyping, leaving it
        // inconsistent with is_subtype_of (which uses the core hierarchy) and
        // collapsing valid narrowings to Bottom (Issue #5940).
        let int64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let integer = LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));

        assert_eq!(int64.meet(&integer), int64);
        // meet is commutative.
        assert_eq!(integer.meet(&int64), int64);

        // Sanity: the lattice already agrees Int64 <: Integer.
        assert!(int64.is_subtype_of(&integer));
    }

    #[test]
    fn test_meet_union_concrete() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));

        let result = union.meet(&int);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_is_subtype_of_bottom() {
        let bottom = LatticeType::Bottom;
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let top = LatticeType::Top;

        assert!(bottom.is_subtype_of(&int));
        assert!(bottom.is_subtype_of(&top));
        assert!(bottom.is_subtype_of(&bottom));
    }

    #[test]
    fn test_is_subtype_of_top() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let top = LatticeType::Top;

        assert!(int.is_subtype_of(&top));
        assert!(!top.is_subtype_of(&int));
    }

    #[test]
    fn test_is_subtype_of_concrete_union() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let string = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        assert!(int.is_subtype_of(&union));
        assert!(!string.is_subtype_of(&union));
    }

    #[test]
    fn test_is_subtype_of_uses_core_hierarchy_for_concrete_types() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let integer = LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));
        let number =
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)));
        let string = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        assert!(int.is_subtype_of(&integer));
        assert!(int.is_subtype_of(&number));
        assert!(!string.is_subtype_of(&number));
    }

    #[test]
    fn test_is_subtype_of_uses_core_hierarchy_for_union_members() {
        let mut numeric_variants = BTreeSet::new();
        numeric_variants.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        numeric_variants.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let numeric_union = LatticeType::Union(numeric_variants);

        let number =
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)));
        assert!(numeric_union.is_subtype_of(&number));

        let mut abstract_union_variants = BTreeSet::new();
        abstract_union_variants.insert(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));
        abstract_union_variants.insert(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::AbstractFloat,
        )));
        let abstract_union = LatticeType::Union(abstract_union_variants);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let string = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        assert!(int.is_subtype_of(&abstract_union));
        assert!(float.is_subtype_of(&abstract_union));
        assert!(!string.is_subtype_of(&abstract_union));
    }

    #[test]
    fn test_subtract_concrete() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        let result = int.subtract(&int);
        assert_eq!(result, LatticeType::Bottom);

        let result = int.subtract(&float);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_subtract_union_concrete() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = union.subtract(&int);

        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))));
            assert!(!types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
        }
    }

    #[test]
    fn test_union_widening_by_length() {
        // Create a union with more than MAX_UNION_LENGTH (8) elements
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Symbol,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Missing,
        )));
        types.insert(ConcreteType::Core(CoreType::Any)); // 9 elements, MAX_UNION_LENGTH = 8

        let result = LatticeType::simplify_union(types);
        // Should widen to Top (since they're not all numeric)
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_union_widening_all_integers() {
        // Issue #3539: a large all-integer union widens to the abstract
        // `Integer` supertype, not to `Union{Int64, Float64}`.
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int128,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt8,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt16,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        ))); // 9 elements, exceeds MAX_UNION_LENGTH

        let result = LatticeType::simplify_union(types);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn test_union_widening_mixed_numeric_includes_unsigned_and_big() {
        // Issue #3539: a mixed-numeric union (unsigned + big + float + bool)
        // must not be normalized to `Union{Int64, Float64}`. It widens to the
        // abstract `Number` supertype.
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt128,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        ))); // 9 elements, exceeds MAX_UNION_LENGTH
        let result = LatticeType::simplify_union(types);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)))
        );
    }

    #[test]
    fn test_union_widening_all_floats_to_abstract_float() {
        // Issue #3539: a wide union of only float types widens to AbstractFloat.
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float16,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat,
        )));
        let result = LatticeType::widen_union(&types);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::AbstractFloat
            )))
        );
    }

    #[test]
    fn test_is_subtype_of_const_and_concrete_3538() {
        // Issue #3538: Const must be a subtype of its concrete type and unions.
        use crate::compile::lattice::types::ConstValue;

        let c1 = LatticeType::Const(ConstValue::Int64(1));
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        // Const(1) <: Concrete(Int64)
        assert!(c1.is_subtype_of(&int));
        // Const(1) </: Concrete(Float64)
        assert!(!c1.is_subtype_of(&float));

        // Const(1) <: Union{Int64, Float64}
        let mut us = BTreeSet::new();
        us.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        us.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let u = LatticeType::Union(us);
        assert!(c1.is_subtype_of(&u));

        // Const(1) <: Top
        assert!(c1.is_subtype_of(&LatticeType::Top));

        // Const(1) <: Const(1) but not Const(1) <: Const(2)
        let c1b = LatticeType::Const(ConstValue::Int64(1));
        let c2 = LatticeType::Const(ConstValue::Int64(2));
        assert!(c1.is_subtype_of(&c1b));
        assert!(!c1.is_subtype_of(&c2));

        // Concrete(Int64) </: Const(1) (Const is more specific)
        assert!(!int.is_subtype_of(&c1));
    }

    // Issue #3511: tuple subtyping with Vararg tail.
    #[test]
    fn test_tuple_homogeneous_subtype_of_vararg() {
        // Tuple{Int,Int,Int} <: Tuple{Int, Vararg{Int}}
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(flat.is_subtype_of(&vararg));
        // The reverse should not hold.
        assert!(!vararg.is_subtype_of(&flat));
    }

    #[test]
    fn test_tuple_zero_tail_is_subtype_of_vararg() {
        // Tuple{Int} <: Tuple{Int, Vararg{Int}} (empty tail)
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(flat.is_subtype_of(&vararg));
    }

    #[test]
    fn test_tuple_heterogeneous_not_subtype_of_int_vararg() {
        // Tuple{Int, String} </: Tuple{Int, Vararg{Int}}
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(!flat.is_subtype_of(&vararg));
    }

    #[test]
    fn test_tuple_short_prefix_not_subtype_of_vararg_with_long_prefix() {
        // Tuple{Int} </: Tuple{Int, Int, Vararg{Int}} — needs at least 2 fixed.
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(!flat.is_subtype_of(&vararg));
    }

    #[test]
    fn test_normalize_tuple_vararg_short_unchanged() {
        // Short tuples are kept flat.
        let elements = vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)); 3];
        let normalized = ConcreteType::normalize_tuple_vararg(elements.clone());
        assert_eq!(normalized, ConcreteType::Tuple { elements });
    }

    #[test]
    fn test_normalize_tuple_vararg_long_homogeneous() {
        // 16 Int64 args -> Tuple{Int64, Vararg{Int64}}.
        let elements = vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)); 16];
        let normalized = ConcreteType::normalize_tuple_vararg(elements);
        match normalized {
            ConcreteType::TupleVararg { elements, tail } => {
                assert_eq!(
                    elements,
                    vec![ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64
                    ))]
                );
                assert_eq!(
                    *tail,
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                );
            }
            other => panic!("expected TupleVararg, got {:?}", other),
        }
    }

    #[test]
    fn test_normalize_tuple_vararg_long_heterogeneous() {
        // Mixed Int64/Float64 in a long tail -> Vararg with UnionOf tail.
        let mut elements = vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)); 6];
        elements.extend(vec![
            ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ));
            6
        ]);
        let normalized = ConcreteType::normalize_tuple_vararg(elements);
        match normalized {
            ConcreteType::TupleVararg { elements, tail } => {
                assert_eq!(
                    elements,
                    vec![ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64
                    ))]
                );
                match *tail {
                    ConcreteType::UnionOf(types) => {
                        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Int64
                        ))));
                        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Float64
                        ))));
                    }
                    other => panic!("expected UnionOf tail, got {:?}", other),
                }
            }
            other => panic!("expected TupleVararg, got {:?}", other),
        }
    }

    #[test]
    fn test_join_tuple_with_vararg_collapses() {
        // Tuple{Int,Int} ⊔ Tuple{Int, Vararg{Int}} = Tuple{Int, Vararg{Int}}.
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        let joined = flat.join(&vararg);
        assert_eq!(joined, vararg);
    }

    #[test]
    fn test_complexity_computation() {
        // Simple types have depth 1
        assert_eq!(
            LatticeType::type_depth(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))),
            1
        );
        assert_eq!(
            LatticeType::type_depth(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))),
            1
        );

        // Array has depth 1 + element depth
        let array_int = ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        };
        assert_eq!(LatticeType::type_depth(&array_int), 2);

        // Nested array has higher depth
        let nested_array = ConcreteType::Array {
            element: Box::new(array_int),
            ndims: None,
        };
        assert_eq!(LatticeType::type_depth(&nested_array), 3);
    }

    #[test]
    fn test_join_limited_preserves_nullable_pattern() {
        // Issue #3507: a `Union{Int64, Nothing}` joined with itself, given
        // itself as the comparison type, must come out unchanged. Pure
        // `join` would also return the same value, but we additionally
        // check that the limit step does not over-widen.
        let nullable = LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ]
            .iter()
            .cloned()
            .collect(),
        );
        let joined = nullable.join_limited(&nullable, &nullable);
        assert_eq!(joined, nullable);
    }

    // ====== Issue #3503: Conditional in lattice ops ======

    fn cond(slot: &str, then_t: LatticeType, else_t: LatticeType) -> LatticeType {
        LatticeType::make_conditional(slot, then_t, else_t)
    }

    fn ty(c: ConcreteType) -> LatticeType {
        LatticeType::Concrete(c)
    }

    fn union_of(items: &[ConcreteType]) -> LatticeType {
        let set: BTreeSet<ConcreteType> = items.iter().cloned().collect();
        if set.len() == 1 {
            LatticeType::Concrete(set.into_iter().next().unwrap())
        } else {
            LatticeType::Union(set)
        }
    }

    #[test]
    fn test_make_conditional_collapses_when_branches_equal() {
        // make_conditional drops the wrapper when then == else, since the
        // Conditional carries no narrowing information.
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = LatticeType::make_conditional("x", int.clone(), int.clone());
        assert_eq!(result, int);
        assert!(!result.is_conditional());
    }

    #[test]
    fn test_make_conditional_preserves_when_branches_differ() {
        let result = LatticeType::make_conditional(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert!(result.is_conditional());
        match result {
            LatticeType::Conditional { ref slot, .. } => assert_eq!(slot, "x"),
            other => panic!("expected Conditional, got {:?}", other),
        }
    }

    #[test]
    fn test_widen_conditional_yields_branch_join() {
        // widen_conditional ≡ then ⊔ else.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let widened = c.widen_conditional();
        assert_eq!(
            widened,
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
            ])
        );
    }

    #[test]
    fn test_widen_conditional_identity_for_non_conditional() {
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(int.widen_conditional(), int);
        assert_eq!(LatticeType::Top.widen_conditional(), LatticeType::Top);
    }

    #[test]
    fn test_join_two_conditionals_same_slot_branchwise() {
        // Conditional(x; Int, Nothing) ⊔ Conditional(x; String, Nothing) =
        //   Conditional(x; Union{Int, String}, Nothing)
        let a = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let b = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let joined = a.join(&b);
        match joined {
            LatticeType::Conditional {
                ref slot,
                ref then_type,
                ref else_type,
            } => {
                assert_eq!(slot, "x");
                assert_eq!(
                    **then_type,
                    union_of(&[
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
                    ])
                );
                assert_eq!(
                    **else_type,
                    ty(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Nothing
                    )))
                );
            }
            other => panic!("expected Conditional, got {:?}", other),
        }
    }

    #[test]
    fn test_join_two_conditionals_different_slot_widens_both() {
        // Different slots → widen both → join the widenings.
        let a = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let b = cond(
            "y",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let joined = a.join(&b);
        // widen(a) = Union{Int, Nothing}, widen(b) = Union{String, Nothing},
        // joined = Union{Int, String, Nothing}.
        let expected = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        assert_eq!(joined, expected);
    }

    #[test]
    fn test_join_conditional_with_concrete_no_longer_collapses_to_top() {
        // Pre-Issue #3503 this returned Top. Now: widen the conditional and
        // join with the concrete type, preserving the relevant Union.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let s = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        let joined = c.join(&s);
        let expected = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
        ]);
        assert_eq!(joined, expected);
        assert_ne!(joined, LatticeType::Top);
    }

    #[test]
    fn test_join_conditional_with_compatible_union_preserves_nullable() {
        // Acceptance criterion: nullable pattern preserved when joining
        // a Conditional with a compatible Union (Issue #3503).
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let nullable = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let joined = c.join(&nullable);
        assert_eq!(joined, nullable);
    }

    #[test]
    fn test_meet_two_conditionals_same_slot_branchwise() {
        // Conditional(x; Union{Int, Float}, Nothing) ⊓
        // Conditional(x; Union{Int, String}, Nothing) =
        //   Conditional(x; Int, Nothing)
        let a = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let b = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let met = a.meet(&b);
        // The Conditional(x; Int64, Nothing) collapses through
        // make_conditional rules.
        assert_eq!(
            met,
            cond(
                "x",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing
                )))
            )
        );
    }

    #[test]
    fn test_meet_conditional_with_concrete_no_longer_collapses_to_bottom() {
        // Pre-Issue #3503 this returned Bottom. Now: widen the Conditional
        // and meet with the concrete type — the intersection is not empty.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let met = c.meet(&int);
        assert_eq!(met, int);
        assert_ne!(met, LatticeType::Bottom);
    }

    #[test]
    fn test_meet_conditional_with_compatible_union() {
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let nullable = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let met = c.meet(&nullable);
        assert_eq!(met, nullable);
    }

    #[test]
    fn test_is_subtype_of_conditional_uses_widening() {
        // Conditional(x; Int, Nothing) <: Union{Int, Nothing} via widening.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let nullable = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        assert!(c.is_subtype_of(&nullable));

        // Conditional(x; Int, Nothing) </: Int (else branch is Nothing,
        // which is not a subtype of Int).
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert!(!c.is_subtype_of(&int));
    }

    #[test]
    fn test_is_subtype_of_two_conditionals_same_slot_branchwise() {
        // Conditional(x; Int, Nothing) <: Conditional(x; Union{Int, Float}, Nothing).
        let lhs = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let rhs = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert!(lhs.is_subtype_of(&rhs));
        // The reverse should not hold.
        assert!(!rhs.is_subtype_of(&lhs));
    }

    #[test]
    fn test_subtract_conditional_distributes_branchwise() {
        // (Conditional(x; Union{Int, String}, Nothing)) - String =
        //   Conditional(x; Int, Nothing).
        let c = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let s = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        let result = c.subtract(&s);
        assert_eq!(
            result,
            cond(
                "x",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing
                )))
            )
        );
    }

    #[test]
    fn test_subtract_conditional_collapses_when_branches_match() {
        // Build an explicit Conditional with diverging branches (the
        // public `make_conditional` would have collapsed it if branches
        // were already identical). After subtracting `Union{String,
        // Nothing}`, both branches become `Int64` and `make_conditional`
        // collapses the wrapper.
        let c = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ])),
            else_type: Box::new(union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ])),
        };
        let drop = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let result = c.subtract(&drop);
        assert_eq!(
            result,
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert!(!result.is_conditional());
    }

    #[test]
    fn test_join_conditional_top_passes_through_to_top() {
        // join with Top still gives Top (Top is the absorbing element,
        // checked before the Conditional arms). Issue #3503.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert_eq!(c.join(&LatticeType::Top), LatticeType::Top);
        assert_eq!(LatticeType::Top.join(&c), LatticeType::Top);
    }

    #[test]
    fn test_join_conditional_bottom_yields_conditional() {
        // join with Bottom is the identity (Bottom is the identity element,
        // checked before Conditional arms). Issue #3503.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert_eq!(c.join(&LatticeType::Bottom), c);
        assert_eq!(LatticeType::Bottom.join(&c), c);
    }

    #[test]
    fn test_meet_conditional_bottom_yields_bottom() {
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert_eq!(c.meet(&LatticeType::Bottom), LatticeType::Bottom);
        assert_eq!(LatticeType::Bottom.meet(&c), LatticeType::Bottom);
    }

    #[test]
    fn test_join_limited_widens_runaway_union_against_small_compare_to() {
        // A "growing" loop accumulator: previously known as `Int64`, the
        // body produces a wide unrelated mixed-numeric union. After the
        // join, `limit_type_size` must collapse the result rather than
        // letting the union grow without bound.
        let mut wide = BTreeSet::new();
        for c in [
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        ] {
            wide.insert(c);
        }
        let body = LatticeType::Union(wide);
        let prev = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = prev.join_limited(&body, &prev);
        // 9 distinct integer members exceed MAX_UNION_LENGTH=8 → widened
        // to the `Integer` supertype (Issue #3539 widening strategy).
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn test_join_limited_preserves_known_wide_union_against_self() {
        let wide = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ]);

        assert_eq!(wide.join_limited(&wide, &wide), wide);
    }

    /// Build a single-element tuple nested `depth` levels: depth 1 = `Tuple{T}`,
    /// depth 2 = `Tuple{Tuple{T}}`, etc.
    fn nested_tuple(leaf: ConcreteType, depth: usize) -> ConcreteType {
        let mut ty = leaf;
        for _ in 0..depth {
            ty = ConcreteType::Tuple { elements: vec![ty] };
        }
        ty
    }

    #[test]
    fn test_join_limited_preserves_deep_member_against_seeding_compare_to() {
        // Issue #4273: a structured return union whose deepest member exceeds
        // the absolute complexity cap is preserved when that member is already
        // present in the comparison type (it seeded the accumulator). This is
        // the branch/loop return-aggregation case: a deep tuple return seen
        // first, then a shallow `Int` return joined against it.
        //
        // depth 6 == 1 (outer) + 5 nesting levels → above MAX_UNION_COMPLEXITY.
        let deep = LatticeType::Concrete(nested_tuple(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            6,
        ));
        let shallow = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));

        // Plain join widens the deep-member union all the way to `Top` because
        // its complexity exceeds the unconditional bound.
        assert_eq!(deep.join(&shallow), LatticeType::Top);

        // Comparison-aware join against the deep seed preserves the union: the
        // deep member is derived from `compare_to`, so no new complexity is
        // introduced and only the shallow `Int` counts as a new member.
        let mut expected = BTreeSet::new();
        expected.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        expected.insert(nested_tuple(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            6,
        ));
        let expected = LatticeType::Union(expected);

        assert_eq!(deep.join_limited(&shallow, &deep), expected);
    }

    // ====== Issue #6605: AbstractLattice trait consolidation ======
    //
    // These tests are the authoritative verification layer for the
    // behavior-preserving consolidation. GLB/LUB precision bugs do not
    // surface in VM output (runtime fallback), so we PIN exact meet/join/
    // widen results AND assert the new `AbstractLattice` trait methods agree
    // with the public inherent methods bit-for-bit across concrete×concrete,
    // abstract-hierarchy, tuple, and union cases (symmetric-pair hazard,
    // #5940 lesson).

    use super::super::abstract_lattice::AbstractLattice;

    /// A representative spread of lattice values covering every variant and
    /// the cases the issue calls out (concrete×concrete, abstract hierarchy,
    /// tuple/vararg, union, const, conditional, top/bottom).
    fn sample_lattice_values() -> Vec<LatticeType> {
        vec![
            LatticeType::Bottom,
            LatticeType::Top,
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer,
            ))),
            ty(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))),
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::AbstractFloat,
            ))),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ]),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ]),
            LatticeType::Const(crate::compile::lattice::types::ConstValue::Int64(42)),
            LatticeType::Const(crate::compile::lattice::types::ConstValue::Int64(43)),
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ],
            }),
            LatticeType::Concrete(ConcreteType::TupleVararg {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))],
                tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            }),
            cond(
                "x",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                ))),
            ),
            cond(
                "y",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String,
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                ))),
            ),
        ]
    }

    #[test]
    fn test_trait_join_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::join(&a, &b),
                    a.join(&b),
                    "trait join != inherent join for {a:?} ⊔ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_meet_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::meet(&a, &b),
                    a.meet(&b),
                    "trait meet != inherent meet for {a:?} ⊓ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_is_subtype_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::is_subtype(&a, &b),
                    a.is_subtype_of(&b),
                    "trait is_subtype != inherent is_subtype_of for {a:?} ⊑ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_subtract_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::subtract(&a, &b),
                    a.subtract(&b),
                    "trait subtract != inherent subtract for {a:?} ∖ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_join_limited_matches_inherent_method() {
        let values = sample_lattice_values();
        for a in &values {
            for b in &values {
                for c in &values {
                    assert_eq!(
                        AbstractLattice::join_limited(a, b, c),
                        a.join_limited(b, c),
                        "trait join_limited != inherent join_limited"
                    );
                }
            }
        }
    }

    #[test]
    fn test_trait_widen_is_identity_for_non_union() {
        // `widen` only collapses Union elements; everything else is identity.
        for v in sample_lattice_values() {
            if !matches!(v, LatticeType::Union(_)) {
                assert_eq!(
                    AbstractLattice::widen(&v),
                    v,
                    "widen changed non-Union {v:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_widen_collapses_all_integer_union_to_integer() {
        // Pins the widen result for a homogeneous integer union: the abstract
        // `Integer` supertype (Issue #3539 strategy), reached via the trait.
        let mut ints = BTreeSet::new();
        for c in [
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ] {
            ints.insert(c);
        }
        let u = LatticeType::Union(ints);
        assert_eq!(
            AbstractLattice::widen(&u),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn test_trait_widen_matches_widen_union_helper_for_unions() {
        // The trait-level `widen` over a Union must equal the underlying
        // `widen_union(&BTreeSet)` building block — same widening semantics,
        // just lifted to a whole LatticeType.
        for v in sample_lattice_values() {
            if let LatticeType::Union(types) = &v {
                assert_eq!(AbstractLattice::widen(&v), LatticeType::widen_union(types));
            }
        }
    }

    #[test]
    fn test_pin_join_concrete_abstract_hierarchy() {
        // Abstract-hierarchy LUB: join(Int64, Integer) = Integer.
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .join(&ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))),
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
        // join(Int64, Float64) = Union{Int64, Float64} (no subtype relation).
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .join(&ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
            ])
        );
    }

    #[test]
    fn test_pin_meet_concrete_abstract_hierarchy() {
        // Abstract-hierarchy GLB: meet(Int64, Integer) = Int64.
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .meet(&ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        // Disjoint concrete types meet to Bottom.
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .meet(&ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))),
            LatticeType::Bottom
        );
    }

    #[test]
    fn test_pin_tuple_vararg_join_meet() {
        // Tuple ⊔ Vararg collapses to the Vararg supertype; ⊓ to the flat tuple.
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert_eq!(flat.join(&vararg), vararg);
        assert_eq!(flat.meet(&vararg), flat);
        // And the trait agrees.
        assert_eq!(AbstractLattice::join(&flat, &vararg), vararg);
        assert_eq!(AbstractLattice::meet(&flat, &vararg), flat);
    }
}
