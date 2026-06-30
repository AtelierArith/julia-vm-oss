//! World-age modeling for inference cache validity (Issue #4271).
//!
//! Mirrors upstream Julia's `WorldRange` / `CodeInstance.min_world` /
//! `CodeInstance.max_world` model from `julia/Compiler/src/cicache.jl` and the
//! validity check in `julia/src/gf.c` (`jl_rettype_inferred`, the
//! `min_world <= world <= max_world` guard).
//!
//! In upstream Julia, every inferred `CodeInstance` is stamped with the world
//! range over which the inference result is valid. A cache lookup at world `w`
//! is a hit only when `w` falls inside that range. When a method is added or
//! replaced, the global `jl_world_counter` is bumped and the affected
//! `CodeInstance`s have their `max_world` capped (via backedge walking) so that
//! they are no longer reused at the new world, while *unaffected* results
//! remain valid.
//!
//! sjulia does not yet model full `MethodInstance` / `CodeInstance` identity or
//! precise backedge graphs. This module provides the *world-range* half of that
//! model so cached inference results carry an explicit validity window instead
//! of being unconditionally wiped on every method-table mutation. The engine
//! pairs this with a conservative callee-dependency approximation (see
//! `InferenceEngine`) to decide which entries to cap on a mutation, mirroring
//! upstream backedge invalidation closely enough to be sound while remaining
//! bounded.

use serde::{Deserialize, Serialize};

/// The monotonic inference world counter type.
///
/// Analogue of upstream `jl_world_counter` (`size_t`). sjulia starts at world
/// `1` (matching upstream's initial value) and advances on every inference-only
/// method-table mutation.
pub type World = u64;

/// A closed interval of world ages `[min_world, max_world]` over which a cached
/// inference result is valid.
///
/// Mirrors `struct WorldRange` in `julia/Compiler/src/cicache.jl`:
///
/// ```julia
/// struct WorldRange
///     min_world::UInt
///     max_world::UInt
/// end
/// in(world::UInt, wr::WorldRange) = wr.min_world <= world <= wr.max_world
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldRange {
    /// First world (inclusive) at which the result is valid.
    pub min_world: World,
    /// Last world (inclusive) at which the result is valid. A freshly inferred
    /// result is valid "into the future" and uses [`World::MAX`], matching
    /// upstream's `~(size_t)0` (`typemax(UInt)`) sentinel for an open-ended
    /// `max_world`.
    pub max_world: World,
}

impl WorldRange {
    /// A range that is valid from `min` onward with no upper bound, mirroring a
    /// freshly inferred upstream `CodeInstance` whose `max_world` is
    /// `typemax(UInt)`.
    pub fn from_world(min: World) -> Self {
        Self {
            min_world: min,
            max_world: World::MAX,
        }
    }

    /// Returns whether `world` falls within `[min_world, max_world]`.
    ///
    /// Mirrors `in(world::UInt, wr::WorldRange)`.
    pub fn contains(&self, world: World) -> bool {
        self.min_world <= world && world <= self.max_world
    }

    /// Returns whether this range has been invalidated relative to `world`,
    /// i.e. `world` is past `max_world` (the entry was capped by a later
    /// method mutation).
    pub fn is_expired_at(&self, world: World) -> bool {
        world > self.max_world
    }

    /// Caps `max_world` so this range no longer includes any world `>= bound`.
    ///
    /// Used on method mutation to retire an affected entry: upstream walks
    /// backedges and sets `max_world` of invalidated `CodeInstance`s to the
    /// world just before the change. `bound` is the *new* world; after capping,
    /// `max_world == bound - 1` (or unchanged if it was already lower), so the
    /// entry is no longer a hit at the new world but its history is preserved.
    ///
    /// `bound == 0` is a no-op guard (there is no world before world 0).
    pub fn cap_before(&mut self, bound: World) {
        if bound == 0 {
            return;
        }
        let capped = bound - 1;
        if capped < self.max_world {
            self.max_world = capped;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_world_is_open_ended() {
        let wr = WorldRange::from_world(3);
        assert_eq!(wr.min_world, 3);
        assert_eq!(wr.max_world, World::MAX);
        assert!(wr.contains(3));
        assert!(wr.contains(World::MAX));
        assert!(!wr.contains(2));
    }

    #[test]
    fn world_range_serializes_roundtrip_issue_5093() {
        let wr = WorldRange {
            min_world: 3,
            max_world: 8,
        };

        let encoded = bincode::serialize(&wr).expect("serialize world range");
        let decoded: WorldRange = bincode::deserialize(&encoded).expect("deserialize world range");

        assert_eq!(decoded, wr);
        assert!(decoded.contains(5));
        assert!(!decoded.contains(9));
    }

    #[test]
    fn contains_matches_closed_interval() {
        let wr = WorldRange {
            min_world: 2,
            max_world: 5,
        };
        assert!(!wr.contains(1));
        assert!(wr.contains(2));
        assert!(wr.contains(4));
        assert!(wr.contains(5));
        assert!(!wr.contains(6));
    }

    #[test]
    fn cap_before_retires_entry_at_new_world() {
        // Entry inferred at world 3, open-ended.
        let mut wr = WorldRange::from_world(3);
        // A method mutation advances the world to 4 and caps this entry.
        wr.cap_before(4);
        assert_eq!(wr.max_world, 3);
        // Still a hit at its own world (history preserved)...
        assert!(wr.contains(3));
        assert!(wr.is_expired_at(4));
        // ...but not at the new world.
        assert!(!wr.contains(4));
    }

    #[test]
    fn cap_before_is_monotonic_and_never_widens() {
        let mut wr = WorldRange {
            min_world: 1,
            max_world: 2,
        };
        // A later, larger bound must not widen an already-capped entry.
        wr.cap_before(10);
        assert_eq!(wr.max_world, 2);
    }

    #[test]
    fn cap_before_zero_is_noop() {
        let mut wr = WorldRange::from_world(1);
        wr.cap_before(0);
        assert_eq!(wr.max_world, World::MAX);
    }
}
