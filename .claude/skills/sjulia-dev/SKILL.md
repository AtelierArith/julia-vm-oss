---
name: sjulia-dev
description: >-
  Use when adding base/stdlib Julia functions, writing or updating fixture
  tests, hitting a Julia compatibility gap, optimizing VM codegen/runtime,
  measuring VM performance, or preparing a commit/PR in the SubsetJuliaVM
  (sjulia) repo.
---

# SubsetJuliaVM Development

This skill encodes the project's mandatory workflows from `AGENTS.md`
(`CLAUDE.md` is a symlink to it). Follow the issue-driven, upstream-first
rules below.

**Companion files — read the one that matches your task, NOW, not later:**

| Your task involves… | Read |
|---------------------|------|
| Filing an Issue / found a julia-vs-sjulia gap / adding a workaround | [issue-workflow.md](issue-workflow.md) |
| Opening a PR / merge / post-PR doc updates / required builds | [pr-flow.md](pr-flow.md) |
| VM performance, `--dump-bytecode`, CLI timing, AoT gate | [vm-perf.md](vm-perf.md) |

## Core principles (always apply)

1. **Upstream first.** Before any design decision or implementation, consult
   `./julia` (official Julia). Reproduce upstream behavior at the matching path
   under `subset_julia_vm/src/julia/`. No ad hoc special cases, no
   package-name string matching.
2. **Pure Julia first.** Implement in `subset_julia_vm/src/julia/`; avoid new
   Rust intrinsics unless no upstream-shaped path exists.
3. **Issue-driven.** Create a GitHub Issue *before* any workaround or fix for a
   gap/bug (`sjulia-report-gap`). Reference the Issue number in workaround
   comments, tests, and PRs.
4. **No JIT, iOS-viable.** Prefer VM execution improvements over AoT unless the
   user explicitly asks for AoT. Keep the no-JIT iOS runtime viable.
5. **Don't reset others' work.** Never `git checkout`/`stash` files you didn't
   modify; never `git stash` at all (the stash is shared repo-globally).

## Adding a Julia function

1. Find the official implementation in `julia/base/` or `julia/stdlib/`.
2. Reproduce at the same relative path under
   `subset_julia_vm/src/julia/` (e.g. `julia/base/abc.jl` →
   `subset_julia_vm/src/julia/base/abc.jl`).
3. Use `::Type` (not `::DataType`) for type parameters — see
   `docs/vm/BUILTIN_REMOVAL.md`.
4. Watch the **promote-fallback recursion trap** (Issue #5966): numeric binary
   operators have a generic `promote`-based fallback that recurses forever if a
   mixed-type pair has no specific method. Mirror upstream's mixed-type methods
   (e.g. `==(z::Complex{T}, x::Real) where {T<:Real}`), prefer parametric
   `Complex{T} where {T<:Real}` over bare `::Complex`. After parallel merges
   always run the **full** `cargo nextest run --release`; never `| tail`.
5. Add fixture tests (next section).
6. Run the matching implementation checklist in `docs/vm/CHECKLISTS.md` (new
   type, new literal, new operator, internal `_foo` builtin, AoT op, …).
   In particular, a new `BuiltinId`/`Instr`/`Intrinsic` variant needs wire-ID
   entries in `compile/instr_wire_ids.rs` and a green
   `bash scripts/check_instr_wire_ids.sh` (Issue #8628).

## Fixture tests

- **Path:** `subset_julia_vm/tests/fixtures/<category>/` (NOT the outer
  `tests/fixtures/` — Issue #1768). Categories mirror `docs/vm/` topics
  (`arithmetic`, `array`, `closures`, `complex`, `dict`, …).
- **Verify with upstream Julia first:** `julia path/to/test.jl`.
- **Parity check (recommended):**
  `bash scripts/fixture_julia_parity.sh <fixture.jl>` — runs the fixture under
  both `target/release/sjulia` and upstream `julia`, exits non-zero on
  pass/fail-count mismatch (Issue #4712).
- **manifest.toml:** add a `[[tests]]` entry with `name`, `file`, `expected`,
  `description`.
- **The fixture file MUST end with `true`.** A fixture that passes standalone
  but lacks the trailing `true` fails under nextest with a confusing error.
- **Name uniqueness (Issue #3135):** prefix the test name with its category
  (e.g. `arithmetic_basic`). Run `bash scripts/check_fixture_test_names.sh`.
- **Don't assert iteration/call counts** of iterative solvers (Optim,
  root-finders, …) — those vary across upstream versions. Assert tolerances
  (`isapprox`) and structural facts instead.
- **Fast feedback:** `bash scripts/fixture_fast_feedback.sh <fixture.jl>…`
  prints the recommended sequential command set (upstream julia → rebuild
  sjulia → direct sjulia → category nextest → iOS gates). Run those
  sequentially; never run `cargo build` and `nextest` concurrently
  (artifact-lock contention).

Fixture file skeleton:

```julia
# <One-line description>

using Test

@testset "<Description>" begin
    @test <expression>
end

true  # Test passed — REQUIRED last expression
```

## Build & test commands

```bash
# Re-test pure-Julia base/ changes: the `repl` feature is REQUIRED to re-link
# target/release/sjulia. `cargo build --release` alone does NOT re-link.
cargo build --release -p subset_julia_vm --bin sjulia --features repl

# Direct fixture check (fast, before nextest)
timeout 180 target/release/sjulia <fixture.jl>

# Category gate during development
timeout 1800 cargo nextest run --release --test fixture_tests <category>::

# Full suite (run before PR; never `| tail` — it hides which test failed).
# This is the ONLY run that covers integration binaries; fixtures + `--lib`
# green does NOT imply full green.
timeout 1800 cargo nextest run --release

# Category list
cargo nextest list --test fixture_tests 2>/dev/null | awk '{print $2}' | awk -F'::' '{print $1}' | sort -u
```

ALWAYS wrap tests with `timeout 1800` (30-min max). For VM/codegen work, dump
bytecode before changing runtime fast paths and measure with a
`Vm::run()`-only Criterion harness — see [vm-perf.md](vm-perf.md) for
`--dump-bytecode` and precompiled-cache CLI timing.

## Issue workflow (unsupported-feature / bug)

Apply BEFORE adding any workaround or fix. Full decision tree, MWE template,
and workaround-comment format in [issue-workflow.md](issue-workflow.md).

Quick rules:

- `unsupported-feature` label → sjulia **cannot run** the construct (parse
  error, "unsupported"/"not implemented" runtime error, MethodError on
  otherwise-valid syntax) but upstream `julia` runs it.
- `bug` label → sjulia **runs but produces wrong output**, or you hit an
  existing sjulia error / crash / compatibility gap during work.
- Workaround comment format: `// Workaround: ... (Issue #NNNN)` (Rust) or
  `# Workaround: ... (Issue #NNNN)` (Julia). Register in
  `docs/vm/WORKAROUNDS.md` and run both:
  `bash scripts/check_workarounds_documented.sh` and
  `bash scripts/check_workarounds_sync.sh`.

## VM performance & bytecode

For VM/codegen performance work: dump the final compiled bytecode before
changing runtime fast paths, and do not report cold CLI timing as a VM-only
result. Details and precompiled-cache build procedure in
[vm-perf.md](vm-perf.md).

```bash
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode <file.jl>
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode -e 'f(x)=x+1; f(41)'
# Add --all when Base/prelude or generated helpers are relevant.
```

## Git / PR flow

Full branch → commit → push → PR → merge steps and post-PR doc updates in
[pr-flow.md](pr-flow.md). Key rules: regular merge (never squash), update
`DONE.md` / `UNIMPLEMENTED.md` / `STATUS.md` with dated headers (Issue #3760),
run `base_exports_do_not_exceed_upstream` after base/ changes, pre-PR
`cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`.

## Post-feature / post-bugfix checklist

1. Add tests (fixtures, integration, parser).
2. Update `docs/vm/`: `STATUS.md`, `DONE.md`, `UNIMPLEMENTED.md`
   (dated-header policy, Issue #3760; yearly archive, Issue #6341).
3. Performance impact → add benchmark to `benches/` (Issue #3210).
4. Pipeline/architecture change → update `ARCHITECTURE_OVERVIEW.md`
   (Issue #3244) and English docs (Issue #3246).
5. New Clippy patterns → update Code Audits (Issue #3292).
6. Run category tests; full `cargo nextest run --release` before PR.
7. After the merge, run the post-mortem (`sjulia-postmortem`): memory entry +
   prevention/follow-up Issues.

## Code audits

`cargo clippy --all-targets -- -D warnings` must pass (zero warnings). Each
`scripts/check_*.sh` is registered in CI. See `docs/vm/CODE_AUDITS.md`.
