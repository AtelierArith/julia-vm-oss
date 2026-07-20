# Inference Work Budgets (Issue #8546)

Measurement-driven record of the abstract-interpretation work budgets after
the abstract-domain enrichment slices (PartialStruct lattice, Issue #8544 /
PR #8689; InterConditional early-return narrowing, Issue #8545 / PR #8601).

- **Date**: 2026-07-02
- **Post-enrichment**: `origin/main` @ `c8763fda7` + the #8546 instrumentation
  commit `8309655f0`.
- **Pre-enrichment baseline**: `45b3072c0` (the commit before both enrichment
  merges) with the *identical* instrumentation patch applied in a temporary
  worktree — a true before/after, not a characterization-only run.

## The budgets

Defined in `subset_julia_vm_compile/src/compile/abstract_interp/engine/mod.rs` unless
noted:

| Constant | Value | Bounds |
|----------|-------|--------|
| `MAX_LOOP_FIXPOINT_ITERATIONS` | 10 | loop-body env fixpoint iterations (`for`/`foreach`/`while`) |
| `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH` | 10 | interprocedural recursion depth |
| `MAX_INTERPROCEDURAL_ANALYSIS_WORK` | 2,000,000 | per-root callee-body re-inferences (#8185 catastrophe backstop) |
| `MAX_RECURSIVE_FIXPOINT_ITERATIONS` | 4 | outer fixpoint refining a recursive call's return type |
| `MAX_METHOD_UNION_SPLIT_VARIANTS` | 4 | method-match union-split variant product (upstream `max_union_splitting` parity) |
| `MAX_INFERENCE_ITERATIONS` (`lattice/widening.rs`) | 100 | block CFG return-type fixpoint |

## Attributable widening counters

`compile::budget_metrics` (opt-in: `SJULIA_INFER_BUDGET_METRICS=1`, or
`set_infer_budget_metrics_forced(true)`; when off, each record site costs one
relaxed atomic load — no thread-local traffic) splits every widening/cutoff
event by trigger:

- **Budget exhaustion**: `work_budget_widenings`, `depth_limit_cutoffs`,
  `loop_fixpoint_exhausted`, `recursive_fixpoint_exhausted`,
  `block_fixpoint_limit_hits`, `union_split_bailouts`.
- **Genuine lattice imprecision**: `lattice_join_top_widenings`
  (`join`/`join_limited` reached `Top` from two non-`Top` inputs).

Harness (each workload compiles Base + prelude + package from source with the
persistent Base cache disabled, so numbers are independent of ambient cache
state; a package's incremental cost is its column minus the Base column):

```bash
cargo nextest run --cargo-profile release-fast --lib \
  -E 'test(/budget_metrics_8546/)' --run-ignored all --no-capture
# ad hoc: SJULIA_INFER_BUDGET_METRICS=1 target/release/sjulia file.jl
```

The counters are deterministic event counts (verified identical across
repeated runs), so they are the machine-quiet primary evidence; wall times
are secondary.

## Measurements (2026-07-02, pre → post enrichment)

| Counter | Base prelude | using Optim | using Plots | using Symbolics |
|---------|--------------|-------------|-------------|-----------------|
| roots | 4852 → 4853 | 4904 → 4905 | 4926 → 4927 | 5027 → 5028 |
| total_work | 9525 → 9536 | 9583 → 9594 | 9599 → 9610 | 9706 → 9717 |
| peak_root_work (cap 2,000,000) | 693 → 693 | 693 → 693 | 693 → 693 | 693 → 693 |
| work_budget_widenings | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| depth_limit_cutoffs | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| loop_fixpoint_exhausted | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| recursive_fixpoint_exhausted | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| block_fixpoint_limit_hits | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| union_split_bailouts | 261 → 287 | 261 → 287 | 261 → 287 | 261 → 287 |
| lattice_join_top_widenings | 70 → 72 | 70 → 72 | 70 → 72 | 70 → 72 |
| loop_fixpoint runs / iters / max | 7669/10274/3 → 7744/10171/3 | 7680/10288/3 → 7755/10185/3 | 7677/10282/3 → 7752/10179/3 | 7747/10375/3 → 7822/10268/3 |
| recursive_fixpoint runs / iters / max | 2319/4629/2 → 2324/4639/2 | 2322/4635/2 → 2327/4645/2 | 2319/4629/2 → 2324/4639/2 | 2322/4635/2 → 2327/4645/2 |
| block_fixpoint runs / iters | 4389/8754 → 4413/8806 | 4415/8806 → 4439/8858 | 4407/8790 → 4431/8842 | 4425/8826 → 4449/8878 |

Reading of the data:

- **Zero budget-exhaustion widenings anywhere, before and after.** Every
  hard budget (`WORK`, `DEPTH`, loop fixpoint, recursive fixpoint, block
  fixpoint) sits far from its cap: peak per-root work 693 vs 2,000,000; loop
  fixpoints converge in ≤ 3 of 10 iterations; recursive fixpoints in ≤ 2 of
  4; block fixpoints average 2.0 iterations vs a cap of 100. All widening to
  `Top` observed today is genuine lattice imprecision (70 → 72 joins), not
  budget pressure.
- **Enrichment made loop fixpoints converge slightly faster**: +75 loop
  fixpoint runs but −103 iterations on the Base workload (10274 → 10171) —
  richer per-statement facts reach the loop-stable env sooner.
- **The only budget that binds is `MAX_METHOD_UNION_SPLIT_VARIANTS`**
  (261 → 287 bailouts, all in Base/prelude inference). The +26 shift means
  the richer domain carries more multi-member unions to call sites. This
  budget is deliberate upstream parity (Julia's
  `InferenceParams.max_union_splitting = 4`), and a bailout falls back to
  inferring the joined type (precision, not soundness); raising it would
  multiply method-match work superlinearly.
- Package load-time inference stays tiny thanks to the #8185/#8213 return
  annotations: Optim +52 roots / +58 work, Plots +74/+74, Symbolics
  +175/+181 over the Base prelude.

## Decision: keep all budget values unchanged

- Raising is unjustified: no workload exhausts any budget, before or after
  enrichment, and the enrichment slices added only ~0.1 % total work.
- Tightening (e.g. loop 10 → 4, recursive 4 → 3, which the max-usage data
  would nominally permit) buys nothing on these workloads — an unhit cap has
  zero cost — while it can only change behavior for *unmeasured* programs
  (the fixture corpus and user code with deeper union-growing loops), where
  it would convert convergence into exhaustion widenings. The caps are
  catastrophe backstops (#8185), not tuning knobs.
- `MAX_METHOD_UNION_SPLIT_VARIANTS = 4` stays for upstream parity; the 287
  bailouts are the designed behavior of that parity choice.

Re-run the harness above after future domain or budget changes; quote the
counter deltas (not wall times) as evidence.
