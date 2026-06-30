//! Shared method-selection control flow — the typemap-equivalent core
//! (Issue #6502).
//!
//! Semantic judgments (subtype, specificity, parent hierarchy) were already
//! unified in `inference_core::specificity` / `types::StructHierarchy`
//! (Issues #6331 / #6336), but the SELECTION control flow — candidate
//! enumeration → match → dominance → pick — was still re-implemented at each
//! dispatch entry point. Upstream Julia keeps this flow in one place
//! (`jl_lookup_generic` + the typemap tree in `julia/src/gf.c` /
//! `julia/src/typemap.c`); this module is the SubsetJuliaVM equivalent.
//!
//! The module is deliberately generic: it owns only the *control flow* and
//! delegates every semantic decision to caller-provided closures, so it can
//! be adopted by the compile-time `MethodTable::dispatch_inner` first and by
//! the runtime `call_dynamic*` paths as a follow-up without depending on
//! `compile::MethodSig` (this module sits below the `compile` layer).
//!
//! Hot-path note: both functions are monomorphized over their closures and
//! allocate exactly what the previous inline copies allocated (one `Vec` of
//! max-score indices, plus the tie-breaker scratch vectors that only exist
//! when a score tie occurs). The common leaf×leaf single-match dispatch
//! takes the `ScoredPick::Single` early path.

/// Outcome of the full method-selection pipeline ([`select_method`]).
///
/// `T` is the caller's winner payload (an index into its matched-candidate
/// list or a function index); `A` is the ambiguity payload the caller turns
/// into its `AmbiguousMethod` / `MethodError` diagnostic.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Selection<T, A> {
    /// No candidate matched the arguments (or the caller's final pick
    /// rejected the only winner).
    NoMatch,
    /// A single best method was selected.
    Selected(T),
    /// The matched set is irreducibly ambiguous.
    Ambiguous(A),
}

/// The typemap-equivalent method-selection pipeline (Issue #6502): given the
/// caller's already-matched candidate set, run
///
/// 1. the empty gate (`match_count == 0` → [`Selection::NoMatch`]);
/// 2. when more than one candidate matched, the caller's ordered
///    morespecific dominance pre-check rules (Issue #5926 family) — a unique
///    dominant winner short-circuits selection;
/// 3. when more than one candidate matched, the caller's conflict gate
///    (e.g. mutually-incomparable `Tuple` vararg patterns, Issue #6220) —
///    a conflict is an irreducible ambiguity;
/// 4. otherwise the caller's final scored pick (score winnowing +
///    tie-breakers), which reports its own outcome.
///
/// Upstream Julia keeps this flow in one place (`jl_lookup_generic` /
/// `ml_matches` over the typemap); compile-time `MethodTable::dispatch_inner`
/// and the runtime `Vm::find_best_method_index_from_candidates` are both thin
/// adapters over this driver, injecting their own semantics as closures.
///
/// Hot-path note: all closures are monomorphized and only invoked exactly as
/// often as the previous inline copies invoked the same logic; the
/// single-match fast path runs `final_pick` directly (no dominance work).
pub(crate) fn select_method<T, A>(
    match_count: usize,
    dominance_precheck: impl FnOnce() -> Option<T>,
    conflict: impl FnOnce() -> Option<A>,
    final_pick: impl FnOnce() -> Selection<T, A>,
) -> Selection<T, A> {
    if match_count == 0 {
        return Selection::NoMatch;
    }
    if match_count > 1 {
        if let Some(winner) = dominance_precheck() {
            return Selection::Selected(winner);
        }
        if let Some(ambiguity) = conflict() {
            return Selection::Ambiguous(ambiguity);
        }
    }
    final_pick()
}

/// Index of the unique eligible candidate that dominates every other
/// candidate, if exactly one such candidate exists.
///
/// This is the "unambiguous-most-specific" skeleton shared by all
/// `*dominant_match_index` dominance pre-checks (Issue #5926 family) and the
/// pairwise-subtype tie-breaker (Issue #5068):
///
/// - `eligible(i)` gates which candidates may *win* (ineligible candidates
///   still participate as opponents in the dominance check);
/// - `dominates(i, j)` is the asymmetric "strictly more specific" relation;
/// - if two candidates each dominate all others the relation is inconsistent
///   (strict dominance is asymmetric), so `None` is returned and the caller
///   falls back to its score path.
pub(crate) fn unique_dominant_index(
    candidate_count: usize,
    mut eligible: impl FnMut(usize) -> bool,
    mut dominates: impl FnMut(usize, usize) -> bool,
) -> Option<usize> {
    let mut dominant: Option<usize> = None;
    for i in 0..candidate_count {
        if !eligible(i) {
            continue;
        }
        let dominates_all = (0..candidate_count).all(|j| i == j || dominates(i, j));
        if dominates_all {
            if dominant.is_some() {
                return None;
            }
            dominant = Some(i);
        }
    }
    dominant
}

/// Outcome of [`pick_scored_match`] over a non-empty scored candidate list.
///
/// `Single` is kept distinct from `TieBroken` because the compile-time caller
/// applies its imprecise-argument (`Any`) subtype guard only to a unique
/// max-score winner — tie-breaker selections historically bypass that guard,
/// and this refactor preserves that behavior exactly.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScoredPick {
    /// Exactly one candidate carries the maximum score; its index.
    Single(usize),
    /// A score tie was resolved by the tie-breaker ladder; the winner's index.
    TieBroken(usize),
    /// The tie could not be broken; indices of the tied max-score candidates
    /// in their original order (for the caller's ambiguity error).
    Ambiguous(Vec<usize>),
}

/// Select the best candidate from scored matches: max-score winnowing
/// followed by the dispatch tie-breaker ladder.
///
/// The ladder replicates `MethodTable::dispatch_inner`'s historical order:
///
/// 0. unique exact signature match (same arity, slot-for-slot equal);
/// 1. most `Any` parameters when some argument is statically `Any`;
/// 2. unique non-vararg method;
/// 3. struct-ancestry filter (Issue #3144), only when `use_ancestry_filter`;
/// 4. fewest `where` type parameters (prefer the non-`UnionAll` projection);
/// 5. unique pairwise strictly-more-specific candidate (Issue #5068).
///
/// All semantic predicates are caller-provided; indices in the result refer
/// to positions in `matches`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pick_scored_match<M>(
    matches: &[(M, u32)],
    has_any_arg: bool,
    use_ancestry_filter: bool,
    is_exact: impl Fn(&M) -> bool,
    is_vararg: impl Fn(&M) -> bool,
    any_param_count: impl Fn(&M) -> usize,
    type_param_count: impl Fn(&M) -> usize,
    ancestry_passes: impl Fn(&M) -> bool,
    strictly_more_specific: impl Fn(&M, &M) -> bool,
) -> ScoredPick {
    debug_assert!(!matches.is_empty(), "pick_scored_match needs candidates");

    // Find max specificity score and winnow.
    let max_score = matches.iter().map(|(_, s)| *s).max().unwrap_or(0);
    let best: Vec<usize> = matches
        .iter()
        .enumerate()
        .filter(|(_, (_, s))| *s == max_score)
        .map(|(i, _)| i)
        .collect();

    if best.len() == 1 {
        return ScoredPick::Single(best[0]);
    }

    // Tie-breaker 0: unique exact signature match.
    let exact_best: Vec<usize> = best
        .iter()
        .copied()
        .filter(|&i| is_exact(&matches[i].0))
        .collect();
    if exact_best.len() == 1 {
        return ScoredPick::TieBroken(exact_best[0]);
    }

    // Combined tie-breaker pass: compute all criteria in one loop (Issue #3361).
    let mut max_any_count = 0usize;
    let mut non_varargs: Vec<usize> = Vec::new();
    let mut ancestry_passed: Vec<usize> = Vec::new();
    let mut any_counts: Vec<(usize, usize)> = Vec::new();
    let mut min_type_param_count = usize::MAX;
    let mut type_param_counts: Vec<(usize, usize)> = Vec::new();

    for &i in &best {
        let m = &matches[i].0;

        // Tie-breaker 1 data: count Any params.
        if has_any_arg {
            let any_count = any_param_count(m);
            if any_count > max_any_count {
                max_any_count = any_count;
            }
            any_counts.push((i, any_count));
        }

        // Tie-breaker 2 data: non-varargs.
        if !is_vararg(m) {
            non_varargs.push(i);
        }

        // Tie-breaker 4 data: where-clause type parameter count.
        let tp_count = type_param_count(m);
        if tp_count < min_type_param_count {
            min_type_param_count = tp_count;
        }
        type_param_counts.push((i, tp_count));

        // Tie-breaker 3 data: struct ancestry filter.
        if use_ancestry_filter && ancestry_passes(m) {
            ancestry_passed.push(i);
        }
    }

    // Tie-breaker 1: prefer methods with most Any params when an arg is Any.
    if has_any_arg {
        let most_any: Vec<usize> = any_counts
            .iter()
            .filter(|(_, c)| *c == max_any_count)
            .map(|(i, _)| *i)
            .collect();
        if most_any.len() == 1 {
            return ScoredPick::TieBroken(most_any[0]);
        }
    }

    // Tie-breaker 2: prefer the unique non-vararg method.
    if non_varargs.len() == 1 {
        return ScoredPick::TieBroken(non_varargs[0]);
    }

    // Tie-breaker 3: struct ancestry filter (Issue #3144).
    if use_ancestry_filter && ancestry_passed.len() == 1 {
        return ScoredPick::TieBroken(ancestry_passed[0]);
    }

    // Tie-breaker 4: prefer the non-UnionAll projection when a generic
    // constructor/helper and its concrete wrapper have the same score.
    let fewest_type_params: Vec<usize> = type_param_counts
        .iter()
        .filter(|(_, c)| *c == min_type_param_count)
        .map(|(i, _)| *i)
        .collect();
    if fewest_type_params.len() == 1 {
        return ScoredPick::TieBroken(fewest_type_params[0]);
    }

    // Tie-breaker 5: most specific by pairwise subtype (Issue #5068) — the
    // unique-dominant skeleton over the tied set.
    if let Some(pos) = unique_dominant_index(
        best.len(),
        |_| true,
        |a, b| strictly_more_specific(&matches[best[a]].0, &matches[best[b]].0),
    ) {
        return ScoredPick::TieBroken(best[pos]);
    }

    ScoredPick::Ambiguous(best)
}

/// First candidate attaining the strictly maximal score.
///
/// This is the runtime winnowing skeleton (Issue #6502 wave 2): every
/// runtime `call_dynamic*` selection loop keeps the *first* candidate whose
/// score is strictly greater than the running best, so earlier candidates win
/// ties. Candidates are pre-filtered by the caller (value-dependent VM
/// representation checks such as Dict/Range mismatches stay at the call
/// site); this helper owns only the max-score control flow.
///
/// Generic over the candidate id `T` and the score `S` so the same skeleton
/// serves the `u32` shared-resolver scores and the `i32` typed-dispatch
/// specificity scores. Monomorphized; allocates nothing.
pub(crate) fn pick_max_score<T, S: PartialOrd + Copy>(
    scored_candidates: impl IntoIterator<Item = (T, S)>,
) -> Option<(T, S)> {
    pick_best(scored_candidates, |(_, score), (_, best_score)| {
        *score > *best_score
    })
}

/// First candidate that is strictly `better` than the running best.
///
/// The generalized first-wins winnowing skeleton behind [`pick_max_score`]:
/// `better(candidate, best)` is the caller's strict preference order, so
/// earlier candidates win whenever `better` reports neither direction (e.g.
/// the runtime score fold's "higher score, or equal score and the running
/// best is a vararg while the candidate is not" order, Issue #3910).
/// Monomorphized; allocates nothing.
pub(crate) fn pick_best<T>(
    candidates: impl IntoIterator<Item = T>,
    mut better: impl FnMut(&T, &T) -> bool,
) -> Option<T> {
    let mut best: Option<T> = None;
    for candidate in candidates {
        if best
            .as_ref()
            .is_none_or(|current| better(&candidate, current))
        {
            best = Some(candidate);
        }
    }
    best
}

/// Run ordered selection tiers until one produces a winner.
///
/// The runtime `Instr::CallDynamic` handler narrows its metadata candidate
/// index list through successive tiers (all candidates → user-defined only →
/// a Base-only allowlist) before falling back to string-pattern resolution;
/// each tier is only *constructed* if the previous one found nothing. This
/// helper owns that fallback control flow: `resolve_tier(t)` is called for
/// `t in 0..tier_count` in order, the first `Ok(Some(_))` wins, and errors
/// propagate immediately (the caller raises them once instead of per tier).
///
/// Monomorphized over the closure; tier candidate lists remain lazily built
/// inside `resolve_tier`, so the hot path allocates exactly what the previous
/// hand-rolled nested matches allocated.
pub(crate) fn pick_first_tier<T, E>(
    tier_count: usize,
    mut resolve_tier: impl FnMut(usize) -> Result<Option<T>, E>,
) -> Result<Option<T>, E> {
    for tier in 0..tier_count {
        if let Some(pick) = resolve_tier(tier)? {
            return Ok(Some(pick));
        }
    }
    Ok(None)
}

/// The native-array wrapper fence as a selection-core policy (Issue #6595).
///
/// A signature is an over-broad catch-all when every slot is `Any` or
/// `Function`. Base's typed specializations that carry an array-wrapper
/// parameter (e.g. `reduce(::typeof(+), A::Vector{T})`) are excluded from the
/// **value channel** by the native-array wrapper dispatch fence (#3908/#4189),
/// so a broad catch-all sibling such as `reduce(op::Function, itr)` can become
/// the unique value-channel match and override the **name channel**, which
/// ranks the typed specialization higher.
///
/// `Function`-typed slots count as broad since Issue #6512 made function
/// singletons (`typeof(+)`) engine-true subtypes of `Function`: without this,
/// `reduce(op::Function, itr)` swallowed `reduce(+, Int64[])` and threw on
/// empty collections (Issues #6529 / #6528).
///
/// This predicate is the policy authority shared by both channels' candidate
/// pipelines; see [`wrapper_fence_name_channel_repair`] for the repair flow it
/// gates.
pub(crate) fn signature_is_broad_wrapper_fence(slots: &[String]) -> bool {
    slots.iter().all(|ty| ty == "Any" || ty == "Function")
}

/// Compute the name-channel repair for a value-channel winner that the
/// native-array wrapper fence let through as an over-broad catch-all
/// (Issue #6595, hazard #6528).
///
/// This absorbs the value/name channel asymmetry into the structured selection
/// core: the value channel's `metadata_best` winner is checked against the
/// wrapper-fence policy ([`signature_is_broad_wrapper_fence`]) via
/// `metadata_best_is_broad`; only when it *is* broad does the repair re-resolve
/// the name channel over the non-broad candidate subset
/// (`resolve_non_broad_best`). The resulting `non_broad_best` then takes
/// priority in [`select_typed_dispatch_candidate`], so a broad `::Function`
/// method can never overwrite a typed specialization the fence excludes from
/// the value channel.
///
/// `resolve_non_broad_best` is lazy: the common (non-broad value-channel
/// winner) path never re-runs name-channel resolution.
pub(crate) fn wrapper_fence_name_channel_repair<T>(
    metadata_best: Option<usize>,
    metadata_best_is_broad: impl FnOnce(usize) -> bool,
    resolve_non_broad_best: impl FnOnce() -> Option<T>,
) -> Option<T> {
    let metadata_is_broad = metadata_best.is_some_and(metadata_best_is_broad);
    if metadata_is_broad {
        resolve_non_broad_best()
    } else {
        None
    }
}

/// Select the final `CallTypedDispatch` function index from its ordered runtime
/// channels (Issue #6502).
///
/// This keeps the VM handler as a thin adapter over the established typed
/// dispatch policy:
///
/// 1. a non-broad name-channel match repairs broad value-channel overrides;
/// 2. the metadata/value channel wins when its signature still matches;
/// 3. a positive compiled name-channel match wins;
/// 4. otherwise runtime name-search may replace the compiled match only when it
///    has strictly higher specificity, or when no compiled match exists;
/// 5. the instruction fallback index is the last resort.
///
/// `runtime_best` is lazy so the common compiled/metadata paths do not scan the
/// function name index.
pub(crate) fn select_typed_dispatch_candidate(
    fallback_index: usize,
    compiled_best: Option<(usize, i32)>,
    metadata_best: Option<usize>,
    non_broad_best: Option<(usize, i32)>,
    runtime_best: impl FnOnce() -> Option<(usize, i32)>,
) -> usize {
    if let Some((idx, score)) = non_broad_best {
        if score > 0 {
            return idx;
        }
    }
    if let Some(idx) = metadata_best {
        return idx;
    }
    if let Some((idx, score)) = compiled_best {
        if score > 0 {
            return idx;
        }
    }

    match (runtime_best(), compiled_best) {
        (Some((runtime_idx, runtime_score)), Some((_, compiled_score)))
            if runtime_score > compiled_score =>
        {
            runtime_idx
        }
        (Some((runtime_idx, _)), None) => runtime_idx,
        _ => compiled_best.map(|(idx, _)| idx).unwrap_or(fallback_index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // unique_dominant_index
    // ------------------------------------------------------------------

    #[test]
    fn unique_dominant_index_selects_single_dominator() {
        // Candidate 1 dominates 0 and 2.
        let result = unique_dominant_index(3, |_| true, |i, _| i == 1);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn unique_dominant_index_returns_none_without_dominator() {
        let result = unique_dominant_index(3, |_| true, |_, _| false);
        assert_eq!(result, None);
    }

    #[test]
    fn unique_dominant_index_returns_none_on_two_dominators() {
        // An inconsistent (non-asymmetric) relation where everyone dominates:
        // two all-dominating candidates cannot coexist → None.
        let result = unique_dominant_index(2, |_| true, |_, _| true);
        assert_eq!(result, None);
    }

    #[test]
    fn unique_dominant_index_ineligible_candidate_cannot_win() {
        // Candidate 0 would dominate, but is ineligible; candidate 1 does not
        // dominate candidate 0.
        let result = unique_dominant_index(2, |i| i == 1, |i, _| i == 0);
        assert_eq!(result, None);
    }

    #[test]
    fn unique_dominant_index_ineligible_candidate_still_blocks() {
        // Candidate 1 is eligible and must still dominate the ineligible
        // candidate 0 to win.
        let dominated = unique_dominant_index(2, |i| i == 1, |_, j| j == 0);
        assert_eq!(dominated, Some(1));
        let not_dominated = unique_dominant_index(2, |i| i == 1, |_, _| false);
        assert_eq!(not_dominated, None);
    }

    #[test]
    fn unique_dominant_index_empty_candidates() {
        assert_eq!(unique_dominant_index(0, |_| true, |_, _| true), None);
    }

    // ------------------------------------------------------------------
    // pick_scored_match
    // ------------------------------------------------------------------

    /// Minimal candidate descriptor for ladder tests.
    #[derive(Clone, Copy)]
    struct Cand {
        exact: bool,
        vararg: bool,
        any_params: usize,
        type_params: usize,
        ancestry: bool,
        /// Specificity rank used by the pairwise tie-breaker (lower is more
        /// specific; equal ranks are incomparable).
        rank: usize,
    }

    const NEUTRAL: Cand = Cand {
        exact: false,
        vararg: false,
        any_params: 0,
        type_params: 0,
        ancestry: true,
        rank: 0,
    };

    fn pick(matches: &[(Cand, u32)], has_any_arg: bool, use_ancestry: bool) -> ScoredPick {
        pick_scored_match(
            matches,
            has_any_arg,
            use_ancestry,
            |m| m.exact,
            |m| m.vararg,
            |m| m.any_params,
            |m| m.type_params,
            |m| m.ancestry,
            |a, b| a.rank < b.rank,
        )
    }

    #[test]
    fn pick_single_max_score_wins() {
        let matches = [(NEUTRAL, 3), (NEUTRAL, 7), (NEUTRAL, 5)];
        assert_eq!(pick(&matches, false, false), ScoredPick::Single(1));
    }

    #[test]
    fn pick_exact_breaks_tie() {
        let exact = Cand {
            exact: true,
            ..NEUTRAL
        };
        let matches = [(NEUTRAL, 7), (exact, 7)];
        assert_eq!(pick(&matches, false, false), ScoredPick::TieBroken(1));
    }

    #[test]
    fn pick_most_any_breaks_tie_only_with_any_arg() {
        let any_heavy = Cand {
            any_params: 2,
            ..NEUTRAL
        };
        let matches = [(any_heavy, 7), (NEUTRAL, 7)];
        assert_eq!(pick(&matches, true, false), ScoredPick::TieBroken(0));
        // Without an Any argument the ladder falls through to ambiguity
        // (all other criteria tie and equal ranks are incomparable).
        assert_eq!(
            pick(&matches, false, false),
            ScoredPick::Ambiguous(vec![0, 1])
        );
    }

    #[test]
    fn pick_non_vararg_breaks_tie() {
        let vararg = Cand {
            vararg: true,
            ..NEUTRAL
        };
        let matches = [(vararg, 7), (NEUTRAL, 7)];
        assert_eq!(pick(&matches, false, false), ScoredPick::TieBroken(1));
    }

    #[test]
    fn pick_ancestry_filter_breaks_tie_only_when_enabled() {
        let no_ancestry = Cand {
            ancestry: false,
            ..NEUTRAL
        };
        let matches = [(no_ancestry, 7), (NEUTRAL, 7)];
        assert_eq!(pick(&matches, false, true), ScoredPick::TieBroken(1));
        assert_eq!(
            pick(&matches, false, false),
            ScoredPick::Ambiguous(vec![0, 1])
        );
    }

    #[test]
    fn pick_fewest_type_params_breaks_tie() {
        let generic = Cand {
            type_params: 1,
            ..NEUTRAL
        };
        let matches = [(generic, 7), (NEUTRAL, 7)];
        assert_eq!(pick(&matches, false, false), ScoredPick::TieBroken(1));
    }

    #[test]
    fn pick_pairwise_specificity_breaks_tie() {
        let specific = Cand { rank: 0, ..NEUTRAL };
        let general = Cand { rank: 1, ..NEUTRAL };
        let matches = [(general, 7), (specific, 7)];
        assert_eq!(pick(&matches, false, false), ScoredPick::TieBroken(1));
    }

    #[test]
    fn pick_unresolved_tie_is_ambiguous_in_order() {
        let matches = [(NEUTRAL, 7), (NEUTRAL, 3), (NEUTRAL, 7)];
        assert_eq!(
            pick(&matches, false, false),
            ScoredPick::Ambiguous(vec![0, 2])
        );
    }

    // ------------------------------------------------------------------
    // pick_max_score
    // ------------------------------------------------------------------

    #[test]
    fn pick_max_score_selects_strict_maximum() {
        let picked = pick_max_score([(10usize, 1u32), (11, 7), (12, 3)]);
        assert_eq!(picked, Some((11, 7)));
    }

    #[test]
    fn pick_max_score_first_wins_on_tie() {
        // Strict `>` replacement: the earliest max-score candidate is kept,
        // pinning the runtime resolvers' first-best tie behavior.
        let picked = pick_max_score([(10usize, 7u32), (11, 7), (12, 3)]);
        assert_eq!(picked, Some((10, 7)));
    }

    #[test]
    fn pick_max_score_supports_signed_scores() {
        // The typed-dispatch runtime name search uses i32 specificity that can
        // be negative (TypeVar fallback); the max must still be found.
        let picked = pick_max_score([(0usize, -5i32), (1, -2), (2, -9)]);
        assert_eq!(picked, Some((1, -2)));
    }

    #[test]
    fn pick_max_score_empty_is_none() {
        assert_eq!(pick_max_score(std::iter::empty::<(usize, u32)>()), None);
    }

    // ------------------------------------------------------------------
    // select_typed_dispatch_candidate
    // ------------------------------------------------------------------

    #[test]
    fn typed_dispatch_selection_prefers_non_broad_repair_over_metadata() {
        let picked =
            select_typed_dispatch_candidate(99, Some((10, 5)), Some(20), Some((30, 7)), || {
                Some((40, 9))
            });
        assert_eq!(picked, 30);
    }

    #[test]
    fn typed_dispatch_selection_metadata_wins_before_compiled_best() {
        let picked =
            select_typed_dispatch_candidate(99, Some((10, 5)), Some(20), None, || Some((40, 9)));
        assert_eq!(picked, 20);
    }

    #[test]
    fn typed_dispatch_selection_skips_runtime_when_compiled_is_positive() {
        let mut runtime_called = false;
        let picked = select_typed_dispatch_candidate(99, Some((10, 5)), None, None, || {
            runtime_called = true;
            Some((40, 9))
        });
        assert_eq!(picked, 10);
        assert!(!runtime_called);
    }

    #[test]
    fn typed_dispatch_selection_runtime_can_beat_non_positive_compiled() {
        let picked =
            select_typed_dispatch_candidate(99, Some((10, 0)), None, None, || Some((40, 1)));
        assert_eq!(picked, 40);
    }

    #[test]
    fn typed_dispatch_selection_keeps_non_positive_compiled_when_runtime_is_weaker() {
        let picked =
            select_typed_dispatch_candidate(99, Some((10, 0)), None, None, || Some((40, -1)));
        assert_eq!(picked, 10);
    }

    #[test]
    fn typed_dispatch_selection_uses_fallback_without_any_match() {
        let picked = select_typed_dispatch_candidate(99, None, None, None, || None);
        assert_eq!(picked, 99);
    }

    // ------------------------------------------------------------------
    // signature_is_broad_wrapper_fence (Issue #6595)
    // ------------------------------------------------------------------

    #[test]
    fn wrapper_fence_treats_all_any_signature_as_broad() {
        assert!(signature_is_broad_wrapper_fence(&[
            "Any".to_string(),
            "Any".to_string()
        ]));
    }

    #[test]
    fn wrapper_fence_treats_function_slots_as_broad() {
        // `reduce(op::Function, itr::Any)`: both slots are broad once function
        // singletons subtype Function (Issue #6512), so the value channel must
        // not let it override the typed name-channel specialization (#6528).
        assert!(signature_is_broad_wrapper_fence(&[
            "Function".to_string(),
            "Any".to_string()
        ]));
    }

    #[test]
    fn wrapper_fence_rejects_typed_specialization() {
        // `reduce(::typeof(+), A::Vector{T})`: a concrete/array-wrapper slot is
        // present, so this is NOT a broad catch-all.
        assert!(!signature_is_broad_wrapper_fence(&[
            "typeof(+)".to_string(),
            "Vector{Int64}".to_string()
        ]));
        assert!(!signature_is_broad_wrapper_fence(&["Int64".to_string()]));
    }

    #[test]
    fn wrapper_fence_empty_signature_is_broad() {
        // Vacuously broad; matches the historical `.all(..)` semantics.
        assert!(signature_is_broad_wrapper_fence(&[]));
    }

    // ------------------------------------------------------------------
    // wrapper_fence_name_channel_repair (Issue #6595)
    // ------------------------------------------------------------------

    #[test]
    fn wrapper_fence_repair_skips_when_no_metadata_winner() {
        let repair = wrapper_fence_name_channel_repair(
            None,
            |_| unreachable!("broad check must not run without a metadata winner"),
            || unreachable!("non-broad resolution must not run without a metadata winner"),
        );
        assert_eq!(repair, None::<usize>);
    }

    #[test]
    fn wrapper_fence_repair_skips_when_metadata_winner_is_not_broad() {
        let repair = wrapper_fence_name_channel_repair(
            Some(7),
            |idx| {
                assert_eq!(idx, 7);
                false
            },
            || unreachable!("non-broad resolution must not run for a non-broad winner"),
        );
        assert_eq!(repair, None::<usize>);
    }

    #[test]
    fn wrapper_fence_repair_resolves_non_broad_for_broad_winner() {
        // The value-channel winner is broad (`::Function`/`Any`), so the repair
        // re-resolves the name channel over the non-broad subset (#6528 guard).
        let repair = wrapper_fence_name_channel_repair(
            Some(7),
            |idx| {
                assert_eq!(idx, 7);
                true
            },
            || Some((42usize, 5i32)),
        );
        assert_eq!(repair, Some((42, 5)));
    }

    #[test]
    fn wrapper_fence_repair_broad_winner_with_no_non_broad_candidate() {
        // Broad winner but no non-broad sibling: repair yields None and the
        // value channel keeps the broad winner downstream.
        let repair: Option<(usize, i32)> =
            wrapper_fence_name_channel_repair(Some(7), |_| true, || None);
        assert_eq!(repair, None);
    }

    // ------------------------------------------------------------------
    // pick_first_tier
    // ------------------------------------------------------------------

    #[test]
    fn pick_first_tier_returns_first_some() {
        let mut visited = Vec::new();
        let result: Result<Option<usize>, ()> = pick_first_tier(3, |tier| {
            visited.push(tier);
            Ok((tier == 1).then_some(42))
        });
        assert_eq!(result, Ok(Some(42)));
        // Tier 2 must never run once tier 1 produced a winner.
        assert_eq!(visited, vec![0, 1]);
    }

    #[test]
    fn pick_first_tier_exhausts_to_none() {
        let result: Result<Option<usize>, ()> = pick_first_tier(3, |_| Ok(None));
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn pick_first_tier_propagates_error_immediately() {
        let mut visited = Vec::new();
        let result: Result<Option<usize>, &str> = pick_first_tier(3, |tier| {
            visited.push(tier);
            if tier == 1 {
                Err("boom")
            } else {
                Ok(None)
            }
        });
        assert_eq!(result, Err("boom"));
        assert_eq!(visited, vec![0, 1]);
    }

    #[test]
    fn pick_first_tier_zero_tiers_is_none() {
        let result: Result<Option<usize>, ()> = pick_first_tier(0, |_| Ok(Some(1)));
        assert_eq!(result, Ok(None));
    }

    // ------------------------------------------------------------------
    // select_method
    // ------------------------------------------------------------------

    #[test]
    fn select_method_empty_is_no_match() {
        let result: Selection<usize, ()> = select_method(
            0,
            || unreachable!("dominance must not run on empty matches"),
            || unreachable!("conflict must not run on empty matches"),
            || unreachable!("final pick must not run on empty matches"),
        );
        assert_eq!(result, Selection::NoMatch);
    }

    #[test]
    fn select_method_single_match_skips_dominance_and_conflict() {
        let result: Selection<usize, ()> = select_method(
            1,
            || unreachable!("dominance must not run on a single match"),
            || unreachable!("conflict must not run on a single match"),
            || Selection::Selected(7),
        );
        assert_eq!(result, Selection::Selected(7));
    }

    #[test]
    fn select_method_dominant_winner_short_circuits() {
        let result: Selection<usize, ()> = select_method(
            2,
            || Some(1),
            || unreachable!("conflict must not run after a dominant winner"),
            || unreachable!("final pick must not run after a dominant winner"),
        );
        assert_eq!(result, Selection::Selected(1));
    }

    #[test]
    fn select_method_conflict_is_ambiguous() {
        let result: Selection<usize, &str> = select_method(
            2,
            || None,
            || Some("tuple vararg conflict"),
            || unreachable!("final pick must not run after a conflict"),
        );
        assert_eq!(result, Selection::Ambiguous("tuple vararg conflict"));
    }

    #[test]
    fn select_method_falls_through_to_final_pick() {
        let result: Selection<usize, ()> =
            select_method(3, || None, || None, || Selection::NoMatch);
        assert_eq!(result, Selection::NoMatch);
    }

    // ------------------------------------------------------------------
    // pick_best
    // ------------------------------------------------------------------

    #[test]
    fn pick_best_first_wins_when_never_better() {
        // `better` never fires → the first candidate is kept (the runtime
        // resolvers' first-best tie behavior).
        let picked = pick_best([3usize, 1, 2], |_, _| false);
        assert_eq!(picked, Some(3));
    }

    #[test]
    fn pick_best_applies_custom_tie_preference() {
        // (idx, score, is_vararg): equal scores prefer the non-vararg
        // candidate over a running vararg best, pinning the runtime fold.
        let candidates = [(0usize, 7u32, true), (1, 7, false), (2, 7, true)];
        let picked = pick_best(candidates, |new, best| {
            new.1 > best.1 || (new.1 == best.1 && best.2 && !new.2)
        });
        assert_eq!(picked, Some((1, 7, false)));
    }

    #[test]
    fn pick_best_empty_is_none() {
        assert_eq!(pick_best(std::iter::empty::<usize>(), |_, _| true), None);
    }

    #[test]
    fn pick_ladder_order_exact_beats_most_any() {
        // The exact candidate wins even though the other has more Any params,
        // pinning the ladder order (exact runs before the Any-count rule).
        let exact = Cand {
            exact: true,
            ..NEUTRAL
        };
        let any_heavy = Cand {
            any_params: 3,
            ..NEUTRAL
        };
        let matches = [(any_heavy, 7), (exact, 7)];
        assert_eq!(pick(&matches, true, false), ScoredPick::TieBroken(1));
    }
}
