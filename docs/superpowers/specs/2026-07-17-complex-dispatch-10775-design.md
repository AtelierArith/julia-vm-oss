# Issue #10775 Complex dispatch regression design

## Context

Issue #10775 reported that fresh sjulia processes sometimes admitted concrete
`Complex{Float64}` or `Complex{Float32}` methods for a `Complex{Int64}` actual
argument. The original reproducer selected the wrong `abs2` method in roughly
one third of processes. PR #10784 removed two additional concrete binary
overloads as a containment measure, but deliberately left the resolver bug
open.

Current `origin/main` no longer reproduces the defect: the original `abs2`
program was correct in 60/60 fresh processes, and an independent three-method
generic/Float32/Float64 reproducer selected the generic Int64-compatible method
in 100/100 fresh processes. Several shared-dispatch changes landed after the
report, so this issue is now a missing durable regression and stale-status
cleanup rather than an observed production failure.

## Considered approaches

1. Change the matcher again. Rejected: there is no current failing behavior or
   demonstrated unsound branch to justify a production resolver change.
2. Restore the concrete ComplexF64 `+` and `*` overloads removed by #10784.
   Rejected for this issue: #10784 documented that the Rust fast path preserves
   performance without them, and restoring a separate optimization is broader
   than closing the dispatch correctness gap.
3. Characterize and pin the repaired resolver behavior, then remove stale
   statements that claim #10775 still affects current code. Selected: it proves
   the reported invariant without speculative production changes.

## Design

Add an end-to-end `MethodTable` regression containing the same three method
shapes as the report: a bounded `Complex{T} where T<:Real` method and concrete
`Complex{Float32}` / `Complex{Float64}` methods. Construct the table in every
permutation of insertion order and require `Complex{Int64}` to select the
generic method each time. Also require exact Float32 and Float64 actuals to
select their concrete methods, preventing an over-corrected matcher that simply
rejects all concrete parametric signatures.

The test exercises the canonical MethodSig/CoreType projection and the
production `MethodTable::dispatch` entry point. Fresh-process CLI repetition
remains a verification step because Rust's `HashMap` random seed is per process
and cannot be represented by repeated calls within one test process.

Remove the obsolete comments in `base/complex.jl` that state the bug is still
latent and prohibit future concrete overloads. Keep the generic `+` and `*`
implementation unchanged; this PR does not reintroduce the removed optimization.
Record the verified closure in STATUS/DONE.

## Verification

- Focused MethodTable regression.
- `subset_julia_vm_bytecode` dispatch/method-table test group.
- Upstream Julia and sjulia synthetic overload reproducer.
- At least 100 fresh sjulia processes for both the original `abs2` MWE and the
  synthetic three-method MWE, requiring one unique correct output.
- Complex fixture category, formatting, default clippy lane, and the repository
  full nextest gate before merge.

## Scope and failure policy

No production resolver logic changes without a newly demonstrated RED case. If
the permutation test or fresh-process verification finds a mismatch, stop the
documentation cleanup and return to root-cause diagnosis before editing the
matcher.
