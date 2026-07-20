use super::LatticeType;
use std::collections::BTreeSet;

/// Abstract exception result for expression inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExceptionType {
    /// Expression is known not to throw.
    Bottom,
    /// Expression may throw a known exception type.
    Known(&'static str),
    /// Expression may throw one of several known exception types.
    /// Mirrors upstream Julia's `Union{ExcA, ExcB, ...}` exception
    /// inference (e.g. `Base.infer_exception_type(() -> log(-1))`
    /// -> `Union{DomainError, InexactError}`). Sorted set ensures
    /// canonical equality independent of insertion order
    /// (Issue #4700). Always has length >= 2 -- single-element
    /// unions canonicalize to `Known(...)` via the `merge` helper,
    /// and empty unions canonicalize to `Bottom`.
    Union(BTreeSet<&'static str>),
    /// Expression may throw an unknown exception type.
    Any,
}

impl ExceptionType {
    /// Lattice-merge two `ExceptionType` values (set union),
    /// canonicalizing the result. `Any` absorbs; `Bottom` is the
    /// identity; matching `Known` stays `Known`; differing `Known`
    /// promote to `Union`. Used by inference paths that join the
    /// exception types of multiple branches (Issue #4700).
    pub fn merge(&self, other: &ExceptionType) -> ExceptionType {
        match (self, other) {
            (ExceptionType::Any, _) | (_, ExceptionType::Any) => ExceptionType::Any,
            (ExceptionType::Bottom, x) | (x, ExceptionType::Bottom) => x.clone(),
            (ExceptionType::Known(a), ExceptionType::Known(b)) => {
                if a == b {
                    ExceptionType::Known(a)
                } else {
                    let mut set = BTreeSet::new();
                    set.insert(*a);
                    set.insert(*b);
                    ExceptionType::Union(set)
                }
            }
            (ExceptionType::Known(a), ExceptionType::Union(set))
            | (ExceptionType::Union(set), ExceptionType::Known(a)) => {
                let mut merged = set.clone();
                merged.insert(*a);
                ExceptionType::canonical_union(merged)
            }
            (ExceptionType::Union(a), ExceptionType::Union(b)) => {
                let mut merged = a.clone();
                merged.extend(b.iter());
                ExceptionType::canonical_union(merged)
            }
        }
    }

    /// Construct a `Union` from a set, canonicalizing 0-element to
    /// `Bottom` and 1-element to `Known`. Internal helper for
    /// `merge`.
    fn canonical_union(set: BTreeSet<&'static str>) -> ExceptionType {
        match set.len() {
            0 => ExceptionType::Bottom,
            1 => ExceptionType::Known(set.iter().next().unwrap()),
            _ => ExceptionType::Union(set),
        }
    }
}

/// Consulted during reflection-time interprocedural exception inference to
/// obtain a Base callee's exception type from the pure-Julia reflection
/// classification (`Base._classified_exception_type`) instead of recursively
/// walking the callee's pure-Julia body (Issue #6272).
///
/// This mirrors upstream Julia's abstract interpreter, which composes a
/// caller's exception type by joining each callee's already-inferred cached
/// exception type rather than re-deriving it from the callee body on every
/// reference. The pure-Julia classification table plays the role of that
/// cached per-callee result here, keeping those semantics owned by pure Julia
/// rather than encoded as Rust name special-cases.
///
/// Returns `Some(exct)` when the pure-Julia layer classifies the callee, or
/// `None` when it has no classification -- in which case the composer treats the
/// call conservatively (no proven throw) without descending into the body.
pub trait BaseCalleeExceptionClassifier {
    fn classify_base_callee(
        &mut self,
        name: &str,
        arg_types: &[LatticeType],
    ) -> Option<ExceptionType>;

    fn compose_base_extension_callee(
        &mut self,
        _name: &str,
        _arg_types: &[LatticeType],
    ) -> Option<ExceptionType> {
        None
    }
}
