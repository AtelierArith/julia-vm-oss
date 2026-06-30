//! Type widening constants and Julia-style `limit_type_size` for lattice operations.
//!
//! These constants control when and how Union types are widened to prevent
//! infinite recursion during type inference, and the `limit_type_size` API
//! provides comparison-aware widening that mirrors Julia's
//! `Compiler/src/typelimits.jl::limit_type_size`.
//!
//! Issue #3507 — replace fixed union widening with Julia-inspired
//! comparison-aware type-size limiting.

use crate::inference_core::CoreType;
use std::collections::BTreeSet;

use crate::compile::infer_metrics;

use super::types::{ConcreteType, LatticeType};

/// Maximum number of elements in a Union type before widening to a supertype.
/// Mirrors Julia's MAX_TYPEUNION_LENGTH.
/// Increased from 4 to 8 to allow more precise type inference for heterogeneous collections.
pub const MAX_UNION_LENGTH: usize = 8;

/// Maximum nesting depth of Union types before widening.
/// Mirrors Julia's MAX_TYPEUNION_COMPLEXITY.
/// Increased from 3 to 5 to allow deeper nested union types.
pub const MAX_UNION_COMPLEXITY: usize = 5;

/// Maximum iterations for fixed-point computation in abstract interpretation.
pub const MAX_INFERENCE_ITERATIONS: usize = 100;

/// Comparison-aware Julia-style limit on the size of a `LatticeType`.
///
/// This is the sjulia counterpart of Julia's `limit_type_size` in
/// `julia/Compiler/src/typelimits.jl`. The fundamental idea, ported from
/// Julia, is:
///
/// > Limit the complexity of `t` so that it is no more complex than the
/// > comparison type `compare_to`. Components of `t` that are "already
/// > present" in `compare_to` (or its sources) are not counted as new
/// > complexity and are kept verbatim. Only the genuinely-new complexity is
/// > subject to widening.
///
/// This avoids the failure mode of pure length-based widening, where common
/// patterns like `Union{T, Nothing}` collapse to `Top` simply because they
/// live alongside other unions. With a comparison type, those patterns are
/// preserved as long as the *new* growth stays bounded.
///
/// # Behaviour summary
///
/// - If `t` is "derived from" `compare_to` (each of its members already
///   appears in `compare_to`, after recursion through `Union`s), `t` is
///   returned unchanged. This is the equivalent of the
///   `is_derived_type_from_any` fast path in Julia's implementation.
/// - Otherwise, when `t` is a `Union`, only the members that are *new*
///   relative to `compare_to` count against the length budget. If the union
///   length still exceeds `max_length`, the union is widened with the
///   existing `widen_union` strategy (preserving abstract numeric supertypes
///   when possible, see Issue #3539).
/// - For composite `Concrete` types, nested complexity beyond `max_complexity`
///   is collapsed to a less-precise structural form. Tail-vararg-shaped
///   tuples (`Tuple{T, T, T, ...}`) collapse their tail into a single
///   `T`-typed element, mirroring Julia's vararg handling.
/// - `Top`, `Bottom`, `Const`, and `Conditional` are passed through
///   unchanged: they are already minimal/maximal points of the lattice.
///
/// # Parameters
///
/// - `t`: the type to (potentially) limit.
/// - `compare_to`: optional reference type. When `None`, this falls back to
///   the existing fixed-length widening (length/complexity bounds applied
///   without any comparison context). When `Some`, the comparison type is
///   used to absorb already-known complexity.
/// - `max_length`: maximum permitted union length. Defaults to
///   [`MAX_UNION_LENGTH`] when callers want the standard policy.
/// - `max_complexity`: maximum permitted nesting depth. Defaults to
///   [`MAX_UNION_COMPLEXITY`].
///
/// # Notes
///
/// This is the *first step* of porting Julia's `limit_type_size` (Issue
/// #3507). Several call sites in the inference engine still call the
/// length-based `simplify_union` path; converting them is left as
/// follow-up work.
pub fn limit_type_size(
    t: &LatticeType,
    compare_to: Option<&LatticeType>,
    max_length: usize,
    max_complexity: usize,
) -> LatticeType {
    infer_metrics::record_limit_type_size_call();

    // Fast-path: identical types, top/bottom/const/conditional are minimal.
    if let Some(c) = compare_to {
        if t == c {
            return t.clone();
        }
    }

    match t {
        // Top / Bottom / Const / Conditional — pass through.
        LatticeType::Top
        | LatticeType::Bottom
        | LatticeType::Const(_)
        | LatticeType::Conditional { .. } => t.clone(),

        // A single concrete type may still be too deeply nested, or it may be
        // structurally *more complex* than the comparison type (recursive
        // same-shape growth such as `Array{Int}` → `Array{Array{Int}}` → ...).
        LatticeType::Concrete(ct) => {
            // Comparison-aware step first (Issue #4273): if `ct` grew a new
            // structural level relative to `compare_to`, widen it to its
            // wrapper so runaway recursive nesting is bounded one level above
            // the comparison rather than only at the absolute `max_complexity`
            // cap. Mirrors Julia's `_limit_type_size` wrapper-widening for
            // same-named DataTypes.
            let limited = match compare_to {
                Some(c) => limit_concrete_against(ct, c),
                None => ct.clone(),
            };
            if depth(&limited) <= max_complexity {
                LatticeType::Concrete(limited)
            } else {
                infer_metrics::record_union_complexity_widening();
                LatticeType::Concrete(limit_concrete(&limited, max_complexity))
            }
        }

        LatticeType::Union(types) => limit_union(types, compare_to, max_length, max_complexity),
    }
}

/// Apply `limit_type_size` to a union, comparison-aware.
fn limit_union(
    types: &BTreeSet<ConcreteType>,
    compare_to: Option<&LatticeType>,
    max_length: usize,
    max_complexity: usize,
) -> LatticeType {
    if types.is_empty() {
        return LatticeType::Bottom;
    }

    // First apply the comparison-aware step to each member (Issue #4273):
    // a member that is structurally more complex than `compare_to` (e.g. a
    // new recursive nesting level) is widened to its wrapper so the union
    // stays bounded relative to the comparison, not just relative to the
    // absolute depth cap. Then trim each member down to `max_complexity`
    // depth as a final safety net.
    let trimmed: BTreeSet<ConcreteType> = types
        .iter()
        .map(|ct| {
            let stepped = match compare_to {
                Some(c) => limit_concrete_against(ct, c),
                None => ct.clone(),
            };
            if depth(&stepped) <= max_complexity {
                stepped
            } else {
                infer_metrics::record_union_complexity_widening();
                limit_concrete(&stepped, max_complexity)
            }
        })
        .collect();

    // If `compare_to` already covers every member of the union, there is no
    // *new* growth and we may keep the union as-is regardless of length.
    if let Some(c) = compare_to {
        if union_is_derived_from(&trimmed, c) {
            return if trimmed.len() == 1 {
                LatticeType::Concrete(trimmed.into_iter().next().expect("non-empty trimmed union"))
            } else {
                LatticeType::Union(trimmed)
            };
        }
    }

    // Otherwise count only the genuinely-new members against `max_length`.
    let new_count = match compare_to {
        Some(c) => trimmed
            .iter()
            .filter(|ct| !concrete_is_derived_from(ct, c))
            .count(),
        None => trimmed.len(),
    };

    if new_count <= max_length && trimmed.len() <= MAX_UNION_LENGTH {
        if trimmed.len() == 1 {
            return LatticeType::Concrete(
                trimmed.into_iter().next().expect("non-empty trimmed union"),
            );
        }
        return LatticeType::Union(trimmed);
    }

    // Length budget exceeded — fall back to the existing widening strategy
    // (Issue #3539: prefer abstract numeric supertypes over `Top` when
    // possible).
    infer_metrics::record_union_length_widening();
    LatticeType::widen_union(&trimmed)
}

/// Return `true` when every element of `types` already appears (directly or
/// inside a union) in `compare_to`.
fn union_is_derived_from(types: &BTreeSet<ConcreteType>, compare_to: &LatticeType) -> bool {
    types
        .iter()
        .all(|ct| concrete_is_derived_from(ct, compare_to))
}

/// Julia's `is_derived_type_from_any`, simplified for the sjulia lattice:
/// a `ConcreteType` is "derived from" a comparison `LatticeType` if it
/// equals, or is a structural component of, that type.
pub fn concrete_is_derived_from(ct: &ConcreteType, compare_to: &LatticeType) -> bool {
    match compare_to {
        LatticeType::Top => true,
        LatticeType::Bottom => false,
        LatticeType::Const(cv) => &cv.to_concrete_type() == ct,
        LatticeType::Concrete(other) => concrete_contains(other, ct),
        LatticeType::Union(others) => others.iter().any(|o| concrete_contains(o, ct)),
        // Conservative: a Conditional is structurally complex; treat its two
        // branches.
        LatticeType::Conditional {
            then_type,
            else_type,
            ..
        } => concrete_is_derived_from(ct, then_type) || concrete_is_derived_from(ct, else_type),
    }
}

/// `true` if `needle` equals `haystack` or is one of its structural
/// components (recursively).
fn concrete_contains(haystack: &ConcreteType, needle: &ConcreteType) -> bool {
    if haystack == needle {
        return true;
    }
    match haystack {
        ConcreteType::Array { element, .. }
        | ConcreteType::Range { element }
        | ConcreteType::Set { element }
        | ConcreteType::Generator { element } => concrete_contains(element, needle),

        ConcreteType::Tuple { elements } => elements.iter().any(|e| concrete_contains(e, needle)),

        ConcreteType::NamedTuple { fields } => {
            fields.iter().any(|(_, ty)| concrete_contains(ty, needle))
        }

        ConcreteType::Dict { key, value } => {
            concrete_contains(key, needle) || concrete_contains(value, needle)
        }

        ConcreteType::UnionOf(members) => members.iter().any(|m| concrete_contains(m, needle)),

        // Atomic / leaf types: only equal-to-`needle` matches, handled above.
        _ => false,
    }
}

/// Comparison-aware limiting of a single `ConcreteType` against a reference
/// `compare_to` lattice type (Issue #4273).
///
/// This is the sjulia counterpart of the same-named-DataType branch of
/// Julia's `_limit_type_size`/`type_more_complex` in
/// `julia/Compiler/src/typelimits.jl`. The motivating failure mode is
/// recursive type growth in a loop accumulator, e.g.
///
/// ```text
/// x = (x,)   # Tuple{Int} → Tuple{Tuple{Int}} → Tuple{Tuple{Tuple{Int}}} → ...
/// ```
///
/// At each fixpoint iteration, the new value is joined against the
/// previous-iteration type (the `compare_to`). Without comparison-aware
/// limiting, every iteration adds a genuinely-new, deeper-nested member and
/// the union/type only widens once the *absolute* depth or length caps are
/// hit — much later than upstream Julia, which widens as soon as the new type
/// is structurally more complex than the comparison.
///
/// Behaviour:
///
/// - If `ct` is *derived from* `compare_to` (already a known component), it is
///   returned unchanged — no new complexity.
/// - If `ct` is structurally *more complex* than the corresponding component
///   of `compare_to` (same wrapper, but a strictly-deeper nesting that is not
///   present in the comparison), `ct` is widened to its wrapper: the
///   offending element type collapses to `Any`. This bounds the growth one
///   structural level above the comparison, mirroring `widert = t.name.wrapper`.
/// - Otherwise `ct` is returned unchanged.
///
/// This only ever *widens*, so it is always sound (the result is a supertype
/// of `ct`). It is also conservative: it fires only when a comparison type is
/// available and `ct` is genuinely more complex than it, so ordinary
/// inference (where each branch contributes a sibling, not a deeper nesting)
/// is unaffected.
fn limit_concrete_against(ct: &ConcreteType, compare_to: &LatticeType) -> ConcreteType {
    // Already known structure → nothing new, keep verbatim.
    if concrete_is_derived_from(ct, compare_to) {
        return ct.clone();
    }
    // Find a same-wrapper component of `compare_to` to compare against. If the
    // comparison has no component with the same wrapper, there is no recursive
    // same-shape growth to bound here; leave `ct` for the depth/length caps.
    match find_same_wrapper(ct, compare_to) {
        Some(c) if concrete_more_complex(ct, &c) => {
            infer_metrics::record_comparison_wrapper_widening();
            widen_concrete_to_wrapper(ct)
        }
        _ => ct.clone(),
    }
}

/// Find a structural component of `compare_to` that shares `ct`'s wrapper
/// (same composite constructor: `Array`, `Tuple`, `Dict`, ...). Returns the
/// first such component found by a structural walk, or `None`.
fn find_same_wrapper(ct: &ConcreteType, compare_to: &LatticeType) -> Option<ConcreteType> {
    match compare_to {
        LatticeType::Concrete(other) => find_same_wrapper_in_concrete(ct, other),
        LatticeType::Union(members) => members
            .iter()
            .find_map(|m| find_same_wrapper_in_concrete(ct, m)),
        LatticeType::Const(cv) => find_same_wrapper_in_concrete(ct, &cv.to_concrete_type()),
        LatticeType::Conditional {
            then_type,
            else_type,
            ..
        } => find_same_wrapper(ct, then_type).or_else(|| find_same_wrapper(ct, else_type)),
        LatticeType::Top | LatticeType::Bottom => None,
    }
}

fn find_same_wrapper_in_concrete(
    ct: &ConcreteType,
    haystack: &ConcreteType,
) -> Option<ConcreteType> {
    if same_wrapper(ct, haystack) {
        return Some(haystack.clone());
    }
    // Recurse into the haystack's components.
    match haystack {
        ConcreteType::Array { element, .. }
        | ConcreteType::Range { element }
        | ConcreteType::Set { element }
        | ConcreteType::Generator { element } => find_same_wrapper_in_concrete(ct, element),
        ConcreteType::Tuple { elements } => elements
            .iter()
            .find_map(|e| find_same_wrapper_in_concrete(ct, e)),
        ConcreteType::NamedTuple { fields } => fields
            .iter()
            .find_map(|(_, t)| find_same_wrapper_in_concrete(ct, t)),
        ConcreteType::Dict { key, value } => find_same_wrapper_in_concrete(ct, key)
            .or_else(|| find_same_wrapper_in_concrete(ct, value)),
        ConcreteType::UnionOf(members) => members
            .iter()
            .find_map(|m| find_same_wrapper_in_concrete(ct, m)),
        _ => None,
    }
}

/// `true` when `a` and `b` are built from the same composite wrapper
/// constructor (mirrors `t.name === c.name` for the parametric wrappers sjulia
/// models structurally). Leaf/atomic types are never considered "same
/// wrapper" here — they carry no parameters to grow, so there is nothing to
/// limit.
fn same_wrapper(a: &ConcreteType, b: &ConcreteType) -> bool {
    use ConcreteType::*;
    matches!(
        (a, b),
        (Array { .. }, Array { .. })
            | (Range { .. }, Range { .. })
            | (Set { .. }, Set { .. })
            | (Generator { .. }, Generator { .. })
            | (Tuple { .. }, Tuple { .. })
            | (NamedTuple { .. }, NamedTuple { .. })
            | (Dict { .. }, Dict { .. })
    )
}

/// `true` when `t` is structurally more complex than `c`, for two types that
/// share the same wrapper. Mirrors the same-name DataType branch of Julia's
/// `type_more_complex`: a parameter is more complex when it nests strictly
/// deeper than the corresponding parameter of `c`.
///
/// For `Tuple` / `NamedTuple` the per-element comparison pads `c` with `Any`
/// when it has fewer elements; growing the element *count* is handled by the
/// existing length/`max_complexity` machinery, so here we focus on per-slot
/// nesting depth.
fn concrete_more_complex(t: &ConcreteType, c: &ConcreteType) -> bool {
    use ConcreteType::*;
    match (t, c) {
        (Array { element: te, .. }, Array { element: ce, .. })
        | (Range { element: te }, Range { element: ce })
        | (Set { element: te }, Set { element: ce })
        | (Generator { element: te }, Generator { element: ce }) => element_more_complex(te, ce),
        (Dict { key: tk, value: tv }, Dict { key: ck, value: cv }) => {
            element_more_complex(tk, ck) || element_more_complex(tv, cv)
        }
        (Tuple { elements: tes }, Tuple { elements: ces }) => {
            tes.iter().enumerate().any(|(i, te)| {
                element_more_complex(te, ces.get(i).unwrap_or(&ConcreteType::Core(CoreType::Any)))
            })
        }
        (NamedTuple { fields: tf }, NamedTuple { fields: cf }) => tf.iter().any(|(name, te)| {
            let ce = cf
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, ty)| ty)
                .unwrap_or(&ConcreteType::Core(CoreType::Any));
            element_more_complex(te, ce)
        }),
        // Different wrappers: caller only invokes this for same-wrapper pairs,
        // so be conservative and report "not more complex".
        _ => false,
    }
}

/// A single element/parameter slot `te` is "more complex" than `ce` when it
/// nests strictly deeper. `Any` (and other leaves) are the simplest, so any
/// composite `te` over a leaf `ce` counts as more complex.
fn element_more_complex(te: &ConcreteType, ce: &ConcreteType) -> bool {
    depth(te) > depth(ce)
}

/// Widen a composite `ConcreteType` to its wrapper by collapsing its immediate
/// parameter(s) to `Any` (mirrors `widert = t.name.wrapper`, e.g.
/// `Array{Array{Int}}` → `Array{Any}`, `Dict{K,V}` → `Dict{Any,Any}`). Leaf
/// types are returned unchanged.
fn widen_concrete_to_wrapper(ct: &ConcreteType) -> ConcreteType {
    match ct {
        ConcreteType::Array { .. } => ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
            ndims: None,
        },
        ConcreteType::Range { .. } => ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
        },
        ConcreteType::Set { .. } => ConcreteType::Set {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
        },
        ConcreteType::Generator { .. } => ConcreteType::Generator {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
        },
        ConcreteType::Dict { .. } => ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Any)),
            value: Box::new(ConcreteType::Core(CoreType::Any)),
        },
        ConcreteType::Tuple { elements } => ConcreteType::Tuple {
            elements: elements
                .iter()
                .map(|_| ConcreteType::Core(CoreType::Any))
                .collect(),
        },
        ConcreteType::NamedTuple { fields } => ConcreteType::NamedTuple {
            fields: fields
                .iter()
                .map(|(k, _)| (k.clone(), ConcreteType::Core(CoreType::Any)))
                .collect(),
        },
        other => other.clone(),
    }
}

/// Compute the structural depth of a concrete type (mirrors
/// `LatticeType::type_depth` for use from this module without breaking
/// privacy).
fn depth(ct: &ConcreteType) -> usize {
    match ct {
        ConcreteType::Array { element, .. }
        | ConcreteType::Range { element }
        | ConcreteType::Set { element }
        | ConcreteType::Generator { element } => 1 + depth(element),

        ConcreteType::Tuple { elements } => 1 + elements.iter().map(depth).max().unwrap_or(0),
        ConcreteType::NamedTuple { fields } => {
            1 + fields.iter().map(|(_, t)| depth(t)).max().unwrap_or(0)
        }
        ConcreteType::Dict { key, value } => 1 + depth(key).max(depth(value)),
        ConcreteType::UnionOf(members) => 1 + members.iter().map(depth).max().unwrap_or(0),
        _ => 1,
    }
}

/// Limit a `ConcreteType` so that its structural depth is at most
/// `max_complexity`. Beyond that bound, structural information is dropped:
/// nested element types collapse to `Any`, and tail-vararg-shaped tuples
/// (a run of identical trailing element types) collapse to the unique tail
/// element type — mirroring Julia's `Vararg` handling.
fn limit_concrete(ct: &ConcreteType, max_complexity: usize) -> ConcreteType {
    limit_concrete_at(ct, max_complexity, 1)
}

fn limit_concrete_at(ct: &ConcreteType, max_complexity: usize, cur: usize) -> ConcreteType {
    if cur >= max_complexity {
        // Past the budget — drop the inner structure.
        return match ct {
            ConcreteType::Array { .. } => ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None,
            },
            ConcreteType::Range { .. } => ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
            },
            ConcreteType::Set { .. } => ConcreteType::Set {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
            },
            ConcreteType::Generator { .. } => ConcreteType::Generator {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
            },
            ConcreteType::Dict { .. } => ConcreteType::Dict {
                key: Box::new(ConcreteType::Core(CoreType::Any)),
                value: Box::new(ConcreteType::Core(CoreType::Any)),
            },
            ConcreteType::Tuple { elements } => {
                // Vararg-style collapse: if all trailing elements are equal,
                // keep one. Otherwise, an all-Any tuple of the same length.
                if !elements.is_empty() && elements.iter().all(|e| e == &elements[0]) {
                    ConcreteType::Tuple {
                        elements: vec![elements[0].clone()],
                    }
                } else {
                    ConcreteType::Tuple {
                        elements: elements
                            .iter()
                            .map(|_| ConcreteType::Core(CoreType::Any))
                            .collect(),
                    }
                }
            }
            ConcreteType::NamedTuple { fields } => ConcreteType::NamedTuple {
                fields: fields
                    .iter()
                    .map(|(k, _)| (k.clone(), ConcreteType::Core(CoreType::Any)))
                    .collect(),
            },
            other => other.clone(),
        };
    }

    match ct {
        ConcreteType::Array { element, .. } => ConcreteType::Array {
            element: Box::new(limit_concrete_at(element, max_complexity, cur + 1)),
            ndims: None,
        },
        ConcreteType::Range { element } => ConcreteType::Range {
            element: Box::new(limit_concrete_at(element, max_complexity, cur + 1)),
        },
        ConcreteType::Set { element } => ConcreteType::Set {
            element: Box::new(limit_concrete_at(element, max_complexity, cur + 1)),
        },
        ConcreteType::Generator { element } => ConcreteType::Generator {
            element: Box::new(limit_concrete_at(element, max_complexity, cur + 1)),
        },
        ConcreteType::Dict { key, value } => ConcreteType::Dict {
            key: Box::new(limit_concrete_at(key, max_complexity, cur + 1)),
            value: Box::new(limit_concrete_at(value, max_complexity, cur + 1)),
        },
        ConcreteType::Tuple { elements } => ConcreteType::Tuple {
            elements: elements
                .iter()
                .map(|e| limit_concrete_at(e, max_complexity, cur + 1))
                .collect(),
        },
        ConcreteType::NamedTuple { fields } => ConcreteType::NamedTuple {
            fields: fields
                .iter()
                .map(|(k, t)| (k.clone(), limit_concrete_at(t, max_complexity, cur + 1)))
                .collect(),
        },
        ConcreteType::UnionOf(members) => ConcreteType::UnionOf(
            members
                .iter()
                .map(|m| limit_concrete_at(m, max_complexity, cur + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_core::{CoreAbstract, CorePrimitive, CoreType};

    fn ty(c: ConcreteType) -> LatticeType {
        LatticeType::Concrete(c)
    }

    fn make_union(items: &[ConcreteType]) -> LatticeType {
        let set: BTreeSet<_> = items.iter().cloned().collect();
        if set.len() == 1 {
            LatticeType::Concrete(set.into_iter().next().unwrap())
        } else {
            LatticeType::Union(set)
        }
    }

    #[test]
    fn short_unions_stay_intact_when_source_covers_them() {
        // Union{Int64, Nothing} — a tiny nullable pattern. With a similar
        // comparison type, it must not be widened.
        let nullable = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let result = limit_type_size(&nullable, Some(&nullable), 3, 5);
        assert_eq!(result, nullable);
    }

    #[test]
    fn nullable_kept_when_compared_to_int_alone() {
        // The original nullable pattern is preserved even when comparing to
        // a strictly-narrower source type, as long as the union length is
        // within budget.
        let nullable = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let int_only = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = limit_type_size(&nullable, Some(&int_only), 3, 5);
        // length 2 <= max_length 3 → kept as-is.
        assert_eq!(result, nullable);
    }

    #[test]
    fn over_long_union_widens_when_no_comparison() {
        // 4 distinct numeric types, max_length = 2, no comparison: must
        // widen via widen_union (which collapses an all-integer union to
        // `Integer`).
        let huge = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ]);
        let result = limit_type_size(&huge, None, 2, 5);
        assert_eq!(
            result,
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn over_long_union_collapses_when_growth_is_new() {
        // Comparison covers Int64 only. A new union of 4 *new* numeric
        // types exceeds max_length=2 → widen to `Integer`.
        let huge = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ]);
        let known = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = limit_type_size(&huge, Some(&known), 2, 5);
        assert_eq!(
            result,
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn long_union_kept_when_all_members_already_in_source() {
        // The union is long, but every member already appears in the
        // source → kept verbatim, no widening.
        let huge = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ]);
        let result = limit_type_size(&huge, Some(&huge), 2, 5);
        assert_eq!(result, huge);
    }

    #[test]
    fn tail_vararg_shaped_tuple_widens_to_single_element() {
        // Tuple{Int, Int, Int, Int} at depth 5 should collapse its tail to
        // a single Int-typed slot when max_complexity is small.
        let tup = ty(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let result = limit_type_size(&tup, None, 8, 1);
        assert_eq!(
            result,
            ty(ConcreteType::Tuple {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))],
            })
        );
    }

    #[test]
    fn deeply_nested_array_collapses_to_array_of_any() {
        // Array{Array{Array{Int64}}} (depth 4) → with max_complexity = 2
        // we keep only the outer Array and the immediate inner element
        // becomes Any.
        let inner = ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        };
        let mid = ConcreteType::Array {
            element: Box::new(inner),
            ndims: None,
        };
        let outer = ConcreteType::Array {
            element: Box::new(mid),
            ndims: None,
        };
        let limited = limit_type_size(&ty(outer.clone()), None, 8, 2);

        // Expected: Array{Array{Any}}
        let expected = ty(ConcreteType::Array {
            element: Box::new(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None,
            }),
            ndims: None,
        });
        assert_eq!(limited, expected);
    }

    #[test]
    fn top_bottom_and_const_pass_through() {
        let cmp = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            limit_type_size(&LatticeType::Top, Some(&cmp), 3, 5),
            LatticeType::Top
        );
        assert_eq!(
            limit_type_size(&LatticeType::Bottom, Some(&cmp), 3, 5),
            LatticeType::Bottom
        );
        let c = LatticeType::Const(crate::compile::lattice::types::ConstValue::Int64(42));
        assert_eq!(limit_type_size(&c, Some(&cmp), 3, 5), c);
    }

    #[test]
    fn small_union_preserved_at_default_budget() {
        // `Union{T, Nothing, Missing}` (length 3) is the canonical
        // "small enough" union. With the recommended default
        // `max_length = MAX_UNION_LENGTH`, it is preserved verbatim.
        let small = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing)),
        ]);
        let result = limit_type_size(&small, None, MAX_UNION_LENGTH, MAX_UNION_COMPLEXITY);
        assert_eq!(result, small);
    }

    #[test]
    fn concrete_is_derived_from_finds_nested_int() {
        // Int64 appears inside Array{Int64} → derived.
        let array_int = ty(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        assert!(concrete_is_derived_from(
            &ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            &array_int
        ));
        assert!(!concrete_is_derived_from(
            &ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            &array_int
        ));
    }

    // ---------------------------------------------------------------------
    // Issue #4273: comparison-aware concrete (recursive-growth) limiting.
    // ---------------------------------------------------------------------

    fn array_of(inner: ConcreteType) -> ConcreteType {
        ConcreteType::Array {
            element: Box::new(inner),
            ndims: None,
        }
    }

    fn tuple_of(elems: &[ConcreteType]) -> ConcreteType {
        ConcreteType::Tuple {
            elements: elems.to_vec(),
        }
    }

    #[test]
    fn recursive_array_growth_widens_to_wrapper_against_prev() {
        // Loop iteration grew `Array{Int}` (prev) into `Array{Array{Int}}`
        // (new). limit_type_size with the previous type as comparison must
        // bound the new level: widen to `Array{Any}` rather than keep nesting.
        let prev = array_of(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let grown = array_of(array_of(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))));
        let result = limit_type_size(
            &ty(grown),
            Some(&ty(prev)),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        assert_eq!(result, ty(array_of(ConcreteType::Core(CoreType::Any))));
    }

    #[test]
    fn recursive_growth_reaches_fixpoint_after_one_widen() {
        // Demonstrate the bounding is a *fixpoint*: once widened to
        // `Array{Any}`, re-applying the limit (now `Array{Any}` is the
        // comparison and a yet-deeper value arrives) does not grow further.
        let widened = array_of(ConcreteType::Core(CoreType::Any));
        // Next iteration's raw new value would be Array{Array{Any}} — but
        // comparison against Array{Any} bounds it right back to Array{Any}.
        let next = array_of(array_of(ConcreteType::Core(CoreType::Any)));
        let result = limit_type_size(
            &ty(next),
            Some(&ty(widened.clone())),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        assert_eq!(result, ty(widened));
    }

    #[test]
    fn recursive_tuple_growth_widens_to_wrapper() {
        // `x = (x,)` style: Tuple{Int} → Tuple{Tuple{Int}}. Against the
        // previous Tuple{Int}, the new nesting is bounded to Tuple{Any}.
        let prev = tuple_of(&[ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))]);
        let grown = tuple_of(&[tuple_of(&[ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))])]);
        let result = limit_type_size(
            &ty(grown),
            Some(&ty(prev)),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        assert_eq!(result, ty(tuple_of(&[ConcreteType::Core(CoreType::Any)])));
    }

    #[test]
    fn same_depth_sibling_is_not_widened() {
        // A sibling element of equal depth is NOT "more complex"; ordinary
        // inference must be unaffected. Array{Float64} compared to
        // Array{Int64} keeps its precise element type.
        let prev = array_of(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let sibling = array_of(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let result = limit_type_size(
            &ty(sibling.clone()),
            Some(&ty(prev)),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        // Not derived, but not deeper either → kept verbatim.
        assert_eq!(result, ty(sibling));
    }

    #[test]
    fn growth_against_unrelated_wrapper_is_left_to_depth_cap() {
        // If the comparison has no same-wrapper component, the comparison-aware
        // step is a no-op (the depth/length caps still apply). Array{Array{Int}}
        // compared to a plain Int64 keeps its structure (depth 3 <= cap 5).
        let grown = array_of(array_of(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))));
        let result = limit_type_size(
            &ty(grown.clone()),
            Some(&ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        assert_eq!(result, ty(grown));
    }

    #[test]
    fn derived_member_is_kept_even_if_nested() {
        // When the grown type is already a known component of the comparison
        // (the comparison itself is the deep type), it is not "new" growth and
        // must be preserved verbatim.
        let deep = array_of(array_of(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))));
        let result = limit_type_size(
            &ty(deep.clone()),
            Some(&ty(deep.clone())),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        assert_eq!(result, ty(deep));
    }

    #[test]
    fn union_member_recursive_growth_is_bounded() {
        // A union accumulator where one branch keeps nesting deeper: the deep
        // member is widened to its wrapper while the shallow member is kept.
        let prev = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            array_of(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ]);
        let grown = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            array_of(array_of(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))),
        ]);
        let result = limit_type_size(&grown, Some(&prev), MAX_UNION_LENGTH, MAX_UNION_COMPLEXITY);
        let expected = make_union(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            array_of(ConcreteType::Core(CoreType::Any)),
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn dict_value_recursive_growth_widens() {
        // Dict{String, Array{Int}} → Dict{String, Array{Array{Int}}}: the
        // deepening value bounds the whole Dict to its wrapper.
        let prev = ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            value: Box::new(array_of(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))),
        };
        let grown = ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            value: Box::new(array_of(array_of(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))))),
        };
        let result = limit_type_size(
            &ty(grown),
            Some(&ty(prev)),
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        assert_eq!(
            result,
            ty(ConcreteType::Dict {
                key: Box::new(ConcreteType::Core(CoreType::Any)),
                value: Box::new(ConcreteType::Core(CoreType::Any)),
            })
        );
    }
}
