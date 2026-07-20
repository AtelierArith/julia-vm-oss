# Upstream Julia Test Baseline

This document records first-pass execution attempts for upstream `julia/test`
files under sjulia. It is the working baseline for Issue #8684.

## 2026-07-02: seven-file sweep baseline (Issue #8700)

Added `scripts/upstream_test_sweep.sh` as the first reusable upstream
`julia/test` sweep driver. It runs each target through upstream Julia and
`sjulia`, writes per-run logs under `target/upstream-test-sweep/`, and emits a
TSV with upstream status, sjulia status, observed testset counts,
classification, linked tracker issues, and the first sjulia error line.

Committed artifacts:

- `docs/vm/UPSTREAM_TEST_SWEEP_BASELINE.tsv`: initial seven-target baseline.
- `docs/vm/UPSTREAM_TEST_SWEEP_ALLOWLIST.tsv`: row-level known-blocker
  allowlist used by the script for `classification` and `issue` columns.

Run shape:

```bash
JULIA_TEST_ROOT=/path/to/julia/test \
  TIMEOUT_SECONDS=180 \
  OUT_DIR=target/upstream-test-sweep \
  scripts/upstream_test_sweep.sh
```

Initial summary:

| file | upstream | sjulia | upstream testsets | sjulia testsets | classification | issues |
| --- | --- | --- | ---: | ---: | --- | --- |
| `int.jl` | pass | error | 145 | 0 | unsupported-feature | #8866, #8867 |
| `operators.jl` | error | error | 18 | 0 | unsupported-feature | #8759, #8866 |
| `bool.jl` | missing | missing | 0 | 0 | scope-out | file absent in this upstream checkout |
| `char.jl` | error | error | 7 | 0 | unsupported-feature | #8870 |
| `rational.jl` | error | error | 1 | 0 | unsupported-feature | #8871 |
| `dict.jl` | error | error | 12 | 0 | unsupported-feature | #8872, #8870, #8874, #8759, #8756 |
| `sets.jl` | error | error | 27 | 0 | unsupported-feature | #8759, #8873, #8866 |

New blockers filed while producing the sweep:

- #8870: extended Unicode `Char` escape literals.
- #8871: coefficient syntax after typed calls before `im`.
- #8872: multi-target indexed assignment.
- #8873: pair arrow expressions inside ternary branches.
- #8874: destructuring `do` block arguments.

The non-`int.jl` upstream `error` rows are preserved as baseline observations:
these files are not all standalone-clean in this checkout when invoked directly
with `using Test; include(...)`, but upstream still reaches test execution. sjulia
currently stops during include-time parsing for each non-missing row, so all
sjulia failures in this first baseline are classified as parser unsupported
features rather than runtime wrong results.

## 2026-07-02: sweep ratchet and expansion policy (Issue #8701)

`scripts/check_upstream_test_sweep_allowlist.sh` enforces the execution-sweep
ratchet for `docs/vm/UPSTREAM_TEST_SWEEP_BASELINE.tsv` and
`docs/vm/UPSTREAM_TEST_SWEEP_ALLOWLIST.tsv`.

The check fails when:

- a current non-passing row is absent from the allowlist;
- a non-`scope-out` failure has no linked Issue;
- an allowlist file now passes and the allowlist was not shrunk;
- the current sjulia testset count drops below the committed baseline for a file.

Nightly integration:

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
JULIA_TEST_ROOT=julia/test \
  TIMEOUT_SECONDS=300 \
  OUT_DIR=target/upstream-test-sweep \
  scripts/upstream_test_sweep.sh > target/upstream-test-sweep/sweep.tsv
SWEEP_TSV=target/upstream-test-sweep/sweep.tsv \
  bash scripts/check_upstream_test_sweep_allowlist.sh
```

Target expansion policy for parent #8684:

- Expand quarterly by 2-3 upstream `julia/test/*.jl` files.
- Prioritize files that cover package-facing behavior sjulia already tries to
  support: numeric tower and promotion, collections, arrays/broadcast,
  strings/chars, modules/imports, Test/REPL-visible behavior, and parser forms
  that block bundled packages.
- Add a file only when the sweep row is reproducible and every current sjulia
  failure is either a passing row, `scope-out`, or linked to a concrete
  `unsupported-feature`/`bug` Issue.
- Update the baseline and allowlist in the same PR as each expansion. If a file
  starts passing, remove its allowlist rows rather than carrying stale debt.

## 2026-07-02: `julia/test/int.jl` (Issue #8699)

Source file: `/Users/atelierarith/work/atelierarith/ailujsoi/julia/test/int.jl`
(upstream Julia checkout submodule in the root worktree).

Run shape:

```bash
julia --startup-file=no -e 'using Test; include("/Users/atelierarith/work/atelierarith/ailujsoi/julia/test/int.jl")'
./target/release/sjulia -e 'using Test; include("/Users/atelierarith/work/atelierarith/ailujsoi/julia/test/int.jl")'
```

Upstream Julia result:

- The file completes successfully when `Test` is loaded by the caller.
- The file uses `using Random` internally and runs through the full integer test
  body, including nested and loop-form `@testset`s.

sjulia result:

- sjulia reaches the parser before executing the file and fails with two parse
  diagnostics reported from the full-file parse:
  - `unexpected token 'U' ... expected 'in' or '='` at the newline-separated
    second iterator in the loop-form `@testset` on lines 291-294.
  - `unexpected token '<' ... expected expression` at `.<<` on line 340.

Filed blockers:

- #8866: newline-separated multi-iterator `@testset ... for` parser gap.
- #8867: broadcast shift operator `.<<` parser gap.

Test stdlib API spot check:

| API | sjulia status | Probe result |
| --- | --- | --- |
| `@test` | supported | `using Test; @test true` passes |
| `@testset` | supported | nested `@testset` with passing `@test`s reports success |
| loop-form `@testset` | partial | one-line multi-iterator form parses; newline-separated second iterator blocks upstream `int.jl` (#8866) |
| `@test_throws` | supported, broad type check | `@test_throws ErrorException error("x")` passes; shim currently checks that an exception was thrown, not exact subtype |
| `@test_broken` | supported | `@test_broken 1 == 2` reports broken |
| `@inferred` | unsupported | `using Test; @inferred(1 + 1)` fails as unknown macro |
| `using Random` | loads | `using Random` succeeds; `rand(1:3)` fails with `Cannot convert Range to I64` and remains outside the first parse blocker |

Minimal upstream-compatible parser MWEs captured for blockers:

```julia
using Test
@testset "x" for T in [Int8],
    U in [Int8]
    @test true
end
```

```julia
x = BigInt(1) .<< [1:3;]
println(x)
println(eltype(x))
```
