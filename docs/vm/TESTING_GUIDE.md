# Testing Guide

This document describes the test suites in SubsetJuliaVM, when to run each, and how to write new tests.

## Test Suites

### Fixture Tests (`fixture_tests.rs`)

The primary test suite. Auto-generated from `manifest.toml` files in `tests/fixtures/<category>/`. Each test compiles and runs a `.jl` file, comparing the result against an expected value.

- **2,377 manifest-defined tests** across 104 categories
- Float comparison with `epsilon = 1e-10`
- 16 MB thread stack per test

**When to run:** After any change to parser, lowering, compiler, or VM.

```bash
timeout 1800 cargo nextest run --release --test fixture_tests
timeout 1800 cargo nextest run --release --test fixture_tests <category>::   # specific category
```

### Integration Tests

End-to-end scenarios live in a single `integration_tests.rs` binary, one inline
`mod` per subsystem (Issue #9671 Phase 1 consolidated the former six binaries):

| Module (inside `integration_tests.rs`) | Purpose |
|------|---------|
| `integration_array_tests` | Array, matrix, broadcast, complex numbers |
| `integration_string_type_tests` | Char, strings, math constants, BigInt |
| `integration_dict_broadcast_tests` | Dictionary and broadcast operations |
| `integration_struct_hof_tests` | Structs and higher-order functions |
| `integration_module_base_tests` | Module system and Base functions |
| `integration_compile_sample_tests` | Compilation validation for code samples |

**When to run:** After changes to specific subsystems (arrays, strings, etc.).
Filter a single module with, e.g., `--test integration_tests -E 'test(integration_array_tests)'`.

### Test-binary consolidation (Issue #9671 Phase 1)

Each `tests/*.rs` is a separate binary that links the full ~370k-line VM rlib, so
binary count dominates full-suite BUILD time. Phase 1 folded per-issue one-off
binaries into a small set of consolidated binaries. Old → new nextest filter map:

| Former binary (removed) | Now a `mod` inside |
|-------------------------|--------------------|
| `hof_*_specialization_5094_tests` (14) | `hof_specialization_5094_tests.rs` |
| `mandelbrot_*` / `test_mandelbrot_grid_comparison` (5) | `mandelbrot_tests.rs` |
| `sjulia_cli_*` + `sjulia_cli_soft_scope_9283_tests` (6) | `sjulia_cli_tests.rs` |
| `register_vm_*` (3) | `register_vm_tests.rs` |
| `integration_*_tests` (6) | `integration_tests.rs` |
| field/index/destructuring/slot/bounds/inbounds/hot-loop specialization (12) | `regression_specialization_tests.rs` |
| annotation/dispatch-cache/inline-cache/lattice/predicate inference (9) | `regression_dispatch_inference_tests.rs` |
| soft-scope/hardscope/session/memory-budget/cache-eviction (8) | `regression_scope_session_tests.rs` |
| `*_cached_base_*` parity (3) | `regression_base_cache_tests.rs` |
| array-construction/structref/dict-demotion/complex-loop/slot-soundness (5) | `regression_runtime_tests.rs` |
| `ssa_ir_8440` / `ssa_pipeline_parity_8552` (2) | `ssa_pipeline_tests.rs` |
| `display_plot_artifact_9262` / `plot_artifact_mime_tests` (2) | `plot_artifact_tests.rs` |
| `test_try_debug` / `test_randn` / `test_if_elseif_else` (3) | `regression_misc_tests.rs` |

The per-test module path is preserved, so `-E 'test(<old_module>)'` still selects
the same tests inside the new binary.

**Where to put a new regression test (decision flow):**

1. **Prefer a fixture** (`.jl` under `tests/fixtures/<category>/`) with a
   `manifest.toml` entry — no new binary, no relink.
2. If it needs Rust (bytecode/inference/dispatch assertions), **add a `mod` to an
   existing consolidated binary** (`regression_*_tests.rs`, `integration_tests.rs`).
3. Only create a **new** `tests/*.rs` binary when process isolation or a distinct
   `required-features` genuinely forces it — and add its name to
   `docs/vm/TEST_BINARY_ALLOWLIST.tsv` in the same PR (enforced by
   `scripts/check_test_binary_budget.sh`).

## Test Organization

Keep implementation files readable by putting public/API-level behavior tests in
`subset_julia_vm/tests/` whenever they can exercise the same surface as an
external crate. Use `src/**/tests.rs` or inline `#[cfg(test)]` modules only for
tests that need private functions, private types, module-local helpers, or
white-box state that should not become part of the public API.

When moving tests out of `src/`, do not promote private APIs just to satisfy an
integration test. Prefer one of these outcomes:

1. Move the test unchanged if it uses only public crate APIs.
2. Split the module so public behavior moves to `tests/` and private helper
   coverage stays in `src/`.
3. Keep the test in `src/` if it is genuinely white-box coverage.

Issue #8460 audit snapshot:

| Source file | Classification and action |
|-------------|---------------------------|
| `compile/abstract_interp/engine/mod.rs` | Internal inference-engine helpers and `mod tests;`; keep source-side. |
| `compile/method_table.rs` | Private `MethodTable` construction/matching coverage; keep source-side. |
| `inference_core/type_core.rs` | Public standalone subtype modules moved to `tests/core_type_public_tests.rs`; private parser/helper coverage stays source-side. |
| `inference_core/dispatch_resolver.rs` | Resolver tests use test-only helper entry points and internal candidate structs; keep source-side until a public dispatch harness exists. |
| `compile/expr/binary/mod.rs` | Private compiler predicate coverage for a private module; keep source-side. |
| `types/julia_type/comparison.rs` | Public `JuliaType` behavior moved to `tests/julia_type_comparison_tests.rs`. |

The structural-debt audit also ratchets large `#[cfg(test)]` blocks in
`subset_julia_vm/src/`: `bash scripts/check_structural_debt_inventory.sh` fails
if the number or total line budget of source-side test blocks over 200 lines
increases. Lower those baselines when migrating more tests to `tests/`.

### Panic-Free VM Tests (`panic_free_vm_tests.rs`)

Counts `.unwrap()`, `.expect()`, and `panic!()` in VM runtime code. Fails if counts increase.

- Baselines: `.unwrap()` = 0, `.expect()` = 1 (SystemTime), `panic!()` = 0
- Excludes test code, doc comments, `unwrap_or`/`unwrap_or_else`/`unwrap_or_default`

**When to run:** After adding code to `src/vm/exec/` or `src/vm/builtins_*.rs`.

```bash
timeout 1800 cargo nextest run --release --test panic_free_vm_tests
```

### Dispatch Tests (`dispatch_tests.rs`)

Tests multiple dispatch functionality and the type system.

**When to run:** After changes to dispatch logic, type matching, or method resolution.

### AoT E2E Tests (`aot_e2e_tests.rs`)

End-to-end Ahead-of-Time compilation tests. Verifies Julia-to-Rust codegen and type inference.

**When to run:** After changes to the AoT compiler (`src/aot/`).

```bash
timeout 1800 cargo nextest run --release --test aot_e2e_tests
```

### Code Samples Tests (`code_samples_tests.rs`, `ios_samples_tests.rs`)

Tests that all code samples (Hello World, arrays, matrices, etc.) compile and run correctly. `ios_samples_tests.rs` covers iOS app `CodeSample.swift` samples.

**When to run:** After adding or modifying code samples, or after changes that could affect sample output.

### Parser Tests (`parser_pure_rust.rs`)

400+ pure Rust parser tests covering Julia syntax edge cases.

**When to run:** After changes to the parser or tree-sitter grammar.

### Other Test Files

| File | Purpose |
|------|---------|
| `unicode_tests.rs` | Unicode handling |
| `regression_dispatch_inference_tests.rs` | Broadcast/dispatch analysis + type propagation in calls (Issue #9671: absorbed `broadcast_dispatch_analysis_tests`, `type_propagation_call_tests`) |
| `core_ir_aot_tests.rs` | AoT Core IR file roundtrip |
| `include_tests.rs` | `include()` directive |
| `base_exports_consistency_tests.rs` | Base exports don't exceed upstream Julia |

## Which Tests to Run

| Change | Tests to Run |
|--------|-------------|
| Parser | `parser_pure_rust`, `fixture_tests` |
| Lowering | `fixture_tests`, unit tests (`--lib`) |
| Compiler | `fixture_tests`, `dispatch_tests` |
| VM execution | `fixture_tests`, `panic_free_vm_tests` |
| VM builtins | `fixture_tests`, `panic_free_vm_tests` |
| AoT compiler | `aot_e2e_tests` |
| Code samples | `code_samples_tests`, `ios_samples_tests` |
| Base/stdlib Julia | `fixture_tests`, `base_exports_consistency_tests` |
| Fixtures (inner loop) | Smoke tier: `scripts/fixture_fast_feedback.sh <changed.jl>...` emits a combined nextest over the changed categories + representative cross-cutting ones (dispatch / type_inference / types / promotion / iteration / numeric / strings) — the #5966-prone set (Issue #9671 Phase 4) |
| Any PR | Full: `timeout 1800 cargo nextest run --release` (the merge gate) |

## Writing Fixture Tests

### Directory Structure

```
subset_julia_vm/tests/fixtures/
  manifest.toml              # Root config (epsilon)
  <category>/
    manifest.toml            # Test definitions for this category
    test_file.jl             # Julia test file
```

**Canonical categories (Issue #9671 Phase 2).** The set of category directories
is an allowlist in `docs/vm/FIXTURE_CATEGORIES.tsv`, enforced by
`scripts/check_fixture_categories.sh`. Prefer an EXISTING category over inventing
a near-synonym — Phase 2 merged the historical duplicates (`arrays`/`array_utils`/
`global_arrays` → `array`, `macro` → `macros`, `function` → `functions`,
`module` → `modules`, `int_ops` → `intfuncs`, `float_ops` → `floatfuncs`,
`meta` → `metaprogramming`, `number` → `numeric`). Adding a genuinely new
category means adding its name to that TSV in the same PR (align with upstream
`julia/test/` filenames where one exists).

### manifest.toml Format

```toml
[[tests]]
name = "category_test_name"
file = "test_file.jl"
expected = true
description = "What this test verifies (Issue #XXXX)"
```

**Fields:**
- `name` — Unique across ALL categories. Prefix with category name (e.g., `array_basic_indexing`).
- `file` — Relative path to `.jl` file within the category directory.
- `expected` — Expected result: `true`/`false` (bool), `42` (integer), `3.14` (float), `"hello"` (string).
- `description` — What the test verifies. Include issue number if applicable.
- `skip` — Optional. Set to `true` to skip the test.
- `skip_julia_test` — Optional. Marks an intentional SubsetJuliaVM extension
  that must NOT be run under upstream `julia` for parity (e.g. callable
  GlobalRef, Issue #302). Honored by `scripts/check_fixture_parity_sweep.sh`.
- `cache_sensitive` — Optional. Marks a fixture whose semantics depend on the
  compile/persistent cache mode (GC/WeakRef/finalizer, struct-table identity
  across cache restore — the #10092 bug class). Any category containing a
  tagged entry is run under BOTH cache modes by
  `scripts/check_cache_sensitive_fixture_lane.sh` (Issue #10223).
- `env` — Optional inline table of per-test environment variables applied by
  the harness for that fixture only (Issue #9486).

### Julia Test File Rules

1. Types, functions, and modules must be defined OUTSIDE `@testset`.
2. The file must end with an expression that produces the expected value.
3. Typically end with `true` for tests that verify behavior via assertions.
4. Verify with Julia first: `julia path/to/test.jl`

### @testset-failure gate — what the harness catches vs. masks (Issue #9360 / #9472 / #10045)

Before Issue #9360, `fixture_tests.rs::run_test_case` compared only the VM's
**final returned value** against the manifest `expected`. A fixture whose
`@testset` recorded a failure but still ended with a matching value (e.g. a
trailing `true`) stayed **green in the harness while red on the CLI**
(`sjulia <fixture>` prints `Test Failed` and exits 1). #9360 closed that hole
with a gate; #9472 found and grandfathered the pre-existing backlog behind a
two-sided ratchet (`docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv`, now **empty** — the
entire backlog has since been fixed, so today every registered fixture must be
genuinely green). Epic #10045 task D re-verified this empirically and added
regression coverage that pins the gate's *decision logic* directly (previously
only exercised indirectly by the ~4,000 real fixtures in the tree).

**Mechanism.** `Vm::any_test_failed()` is a sticky per-run flag set whenever a
`@test`/`@test_throws` records a failure (`@test_broken` failing as expected
does NOT set it). `run_test_case_source` in `fixture_tests.rs` reads that flag
after `vm.run()` returns and calls the pure `testset_gate_verdict(name, file,
description, testset_failed, allowlisted)` function, which rejects exactly two
of its four `(testset_failed, allowlisted)` quadrants: a failure that is NOT
allowlisted (new masking), and an allowlisted fixture that no longer fails
(stale ratchet entry).

**Empirically verified truth table** (probed 2026-07-10 by running each shape
under `target/dev-fast/sjulia` directly and cross-checked against upstream
`julia` 1.12.6; see `testset_gate_regression_tests_10045` in
`fixture_tests.rs` for the machine-checked version of row 1). The "Harness
before #9360" column is what the ORIGINAL value-only comparison would have
done (git history / the #9360 Issue body); "Harness today" is what
`run_test_case_source` actually does right now — Issue #10045 task D did not
change this column, it added the regression test that pins it:

| # | Fixture shape | sjulia CLI | Harness before #9360 | Harness today | Mechanism (today) |
|---|---|---|---|---|---|
| 1 | `@testset` runs a failing `@test`, file still ends `true` | `Test Failed`, exit 1 | **Masked** (green — only compared the final value) | **Caught** — `run_test_case` panics via the #9360 gate unless the file is in `TESTSET_FAILURE_ALLOWLIST.tsv` (currently empty) | `any_test_failed()` → `testset_gate_verdict`; pinned by `testset_gate_regression_tests_10045` |
| 2 | Code after a passing `@test` throws before the trailing `true` | `Runtime error: ...`, exit 1 | **Caught** — `vm.run()` already returned `Err` | **Caught** (unchanged) | Generic VM-error propagation (predates #9360; unrelated to the testset gate) |
| 3 | The expression inside `@test(...)` itself throws | `Error During Test: ...` then the testset summary (`N errored`), exit 1 — since Issue #10093 sjulia's `@test` wraps its expression in `try`/`catch` and records an "errored" outcome like upstream (before #10093: `Runtime error: ...`, no summary) | **Caught** — was generic VM-error propagation | **Caught** — the errored outcome sets `any_test_failed()`, so the #9360 gate rejects it like row 1 | `_test_record_error!` → `any_test_failed()` → `testset_gate_verdict`; pinned by `testset_exit_code_8191_tests.rs` (#10093 mods) |
| 4 | `@testset` body executes **zero** `@test`/`@test_throws`/`@test_broken` (e.g. a vacuous `for i in 1:0`) | `0 passed, 0 failed (0 total)`, exit **0** | Not gated | **Not gated — intentionally** (unchanged) | Matches upstream `julia`, which also exits 0 for a zero-test `@testset` (verified: `Test Summary: | Total 0`). Per-testset counts are not currently exposed as a public `Vm` API (`test_pass_count`/`test_fail_count`/`test_broken_count` are private and reset per-testset), so flagging this would require new state — not pursued because it would diverge from upstream and risk false positives on legitimate `for i in 1:0` guard patterns. |

Row 1 is the only row where "before" and "today" differ — that gap is what
#9360/#9472 already closed (merged prior to epic #10045). Rows 2–3 were never
masked: row 2 goes through the pipeline's ordinary `Result`-to-`panic!`
unwrap, and row 3 — which used that same mechanism when this table was first
probed — now records an errored outcome instead (Issue #10093) and is caught
by the same `any_test_failed()` gate as row 1. Row 4 is a
**blind spot for silent-skip fixtures**, not a masking bug in the #9360/#9472
sense: masking means "harness green, CLI red." A vacuous `@testset` is green
on both sides, matching upstream, so there is nothing for the harness to
catch without inventing behavior upstream itself does not have.

**Regression coverage.** `testset_gate_regression_tests_10045` in
`fixture_tests.rs` (`#[cfg(test)] mod`, run via `cargo nextest run --test
fixture_tests testset_gate_regression_tests_10045`) pins this two ways without
touching the fixtures tree or the allowlist TSV:
- Unit tests all four `(testset_failed, allowlisted)` quadrants of the pure
  `testset_gate_verdict` decision function directly (no VM run).
- Feeds a deliberately-failing `@testset`-with-trailing-`true` Julia source
  string straight into `run_test_case_source` (the same function
  `run_test_case` calls after reading a fixture file) inside
  `std::panic::catch_unwind`, and asserts it panics — plus a genuinely-passing
  counterpart asserting it does NOT panic, to guard against false positives.

If a future refactor of `run_test_case`/`run_test_case_source` accidentally
drops or inverts the gate, these tests fail immediately instead of waiting for
someone to notice a broken fixture is green. Verified directly during
development: temporarily changing `testset_gate_verdict`'s `(true, false)`
arm to `Ok(())` (silently swallowing the masked-failure case) made both
`testset_gate_verdict_failed_and_unallowlisted_is_rejected_10045` and
`run_test_case_source_rejects_broken_but_green_fixture_10045` fail — the
sabotage was caught, and the change was reverted before landing.

### Upstream-parity sweep (Issue #10246; drift backlog #10237)

Issue #10237 found 13 fixtures that were green in the sjulia harness but red
under upstream julia 1.12.6 — they asserted sjulia's wrong behavior, and
nothing compared registered fixtures against upstream by default.
`scripts/check_fixture_parity_sweep.sh` closes that hole:

```bash
# scoped, day-to-day (needs upstream julia + a built sjulia binary):
SJULIA_BIN=target/release-fast/sjulia \
  bash scripts/check_fixture_parity_sweep.sh --jobs 8 strings macros

# full sweep (nightly-scale):
bash scripts/check_fixture_parity_sweep.sh --jobs 4 --all
```

Each registered fixture of the selected categories runs through
`scripts/fixture_julia_parity.sh --red-green`: divergence = red under one
interpreter and green under the other, or a differing wrapped final value for
legacy fixtures without a Test.jl summary. (Exact per-testset pass-count
comparison — the script's default single-fixture mode — is not sweep-safe
until sjulia's outer `@testset` summary aggregates nested counts, Issue
#10338.) Entries with `skip`, `skip_julia_test`, a per-test `env` table, a
`TESTSET_FAILURE_ALLOWLIST.tsv` row, or a bundled non-stdlib package import
are skipped and reported.

Known drift is ratcheted through `docs/vm/FIXTURE_PARITY_SWEEP_ALLOWLIST.tsv`
(TWO-SIDED): an unallowlisted divergence fails the gate, and an allowlisted
fixture in a swept category that no longer diverges fails as a stale entry —
the list must monotonically shrink as the #10237 backlog is triaged
(sjulia-bug → fix the VM; bad-fixture → fix the assertion). The nightly
`fixture-parity-sweep` job runs the audited category set.

### Cache-mode lane for cache-sensitive fixtures (Issue #10223)

Issue #10092 (a standalone `WeakRef` target surviving `GC.gc()`) manifested
ONLY with the persistent Base cache present: both cache-restore paths rebuilt
`struct_table` with `has_inner_constructor: false`, so `WeakRef(x)` skipped
the outer constructor and the weak cell was never GC-registered. The fixture
harness runs each fixture under exactly one cache configuration, so no
single-mode run can catch a cache-mode-dependent bug.

Tag at least one `[[tests]]` entry of an affected category with
`cache_sensitive = true` (the WeakRef/GC fixtures in `tests/fixtures/ref/` are
the canonical set). `scripts/check_cache_sensitive_fixture_lane.sh` then runs
every tagged category three times — cold (persistent caches removed +
`SUBSET_JULIA_VM_DISABLE_*` env), prime (regenerates the caches), cached
(every test process restores Base from the persistent cache) — and fails on a
cold-vs-cached pass/fail divergence:

```bash
SJULIA_CACHE_LANE_CARGO_PROFILE=release-fast \
  bash scripts/check_cache_sensitive_fixture_lane.sh        # tagged categories
bash scripts/check_cache_sensitive_fixture_lane.sh ref      # explicit category
```

Only tagged categories run three times, keeping suite wall-clock bounded; the
whole-suite cache-transparency counterpart is the nightly
`check_cold_cached_nextest.sh` job (Issue #8719). Registered in the nightly
`cold-cached-parity` job. When adding a GC/WeakRef/finalizer or
cache-restore-identity fixture, tag its entry `cache_sensitive = true`.
### The unified `@test`-family recording harness (Issue #10273 / #10093)

Every `@test`-family construct records its outcome through a **single set of
recording builtins** on the VM, and the harness-level invariant is:

> **No `@test`-family entry point may propagate an evaluation exception past
> the enclosing `@testset` without first recording an outcome.**

An exception raised while evaluating a test expression must become a recorded
*errored* (or, for `@test_throws`/`@test_broken`, *pass*/*broken*) outcome —
never a bare VM error that unwinds out of the testset and drops the summary.
This mirrors upstream `Test`, where `do_test`/`get_test_result` catch inside
`try`/`catch` and only throw `TestSetException` at `@testset` end.

**The recording builtins** (`subset_julia_vm_vm/src/vm/builtins_macro/mod.rs`), all
of which feed the per-testset counters and the sticky
`any_test_failed()` exit-code flag (Issue #8191):

| Builtin | `BuiltinId` | Outcome | Sets `any_test_failed`? |
|---|---|---|---|
| `_test_record!(passed, msg)` | `TestRecord` | Pass / Fail | on Fail |
| `_test_record_error!(msg, detail)` | `TestRecordError` (wire 306) | Errored | yes |
| `_test_record_broken!(passed, msg)` | `TestRecordBroken` | Broken / (unexpected-pass ⇒ Error) | on unexpected pass |
| `_testset_begin!(name)` / `_testset_end!()` | `TestSetBegin`/`TestSetEnd` | scope + summary | — |

**Entry points and how each reaches the recorders:**

| Entry point | Path to the recorders |
|---|---|
| `@test ex` (macro) | `macro test` in `stdlib/Test/src/Test.jl` wraps `ex` in `try`/`catch`: `_test_record!` on the Bool result, `_test_record_error!` in the catch (or on a non-Bool value). |
| `@test x isa T` / `@test isa(x, T)` | Lowered by `try_lower_test_isa_macro_with_ctx` (`lowering/stmt/mod.rs`). **Since Issue #10273** it emits the SAME try/catch recording IR via `build_test_record_try_stmt` instead of the old `Stmt::Test`→`Instr::Test` fast path, which evaluated the condition **outside** any handler and let a throwing `isa`-test escape the testset. |
| `@test_throws T ex` | `macro test_throws`: `_test_record!(true, …)` in the catch, `_test_record!(false, …)` if no throw. (Type matching not yet implemented — Issue #10354.) |
| `@test_broken ex` | `macro test_broken`: `try`/`catch` → `_test_record_broken!`. |
| `@test_skip ex` | `macro test_skip` (Issue #10350): `_test_record_broken!(false, …)` WITHOUT evaluating `ex`. |
| `@testset …` (incl. nesting) | `macro testset` / `lower_testset_for_macro`: `_testset_begin!`/`_testset_end!` around a hard `let` scope; nested testsets nest these calls. |

**Legacy `Instr::Test` / `Instr::TestSetBegin` path.** The bytecode
instructions in `vm/exec/error_handling.rs` still exist and are emitted by
`compile/stmt.rs` for any residual `Stmt::Test`/`Stmt::TestSet`/`Stmt::TestThrows`
producers (e.g. the REPL Expr→IR round-trip in
`vm/builtins_macro/ir_conversion.rs`). `Instr::Test` evaluates its condition
**before** the instruction runs, so a throwing condition on that path still
escapes — which is exactly why the `isa` fast path was rerouted through the
macro-shaped try/catch IR rather than left on `Instr::Test`. New `@test`-family
lowering MUST go through the recording builtins, not `Instr::Test`.

**Prevention.** `test_harness_entry_point_coverage_10273` in
`tests/testset_exit_code_8191_tests.rs` enumerates every entry point above as a
source string and asserts, through a full `vm.run()`, that a *throwing* test
expression (a) does not propagate out of `vm.run()`, (b) still prints the
testset summary, and (c) sets `any_test_failed()` for the errored forms. Adding
a new entry point that bypasses the recorders (e.g. re-introducing a bare
`Instr::Test` fast path for a throwing condition) fails this test.

### Fixture aggregation (Issue #9671 Phase 3)

Each fixture runs the full parse → lower → compile → VM pipeline, so a category
of many tiny MWEs pays that per-fixture overhead N times. **Concat-safe**
fixtures — those whose only top-level content is `using Test`, `@testset` blocks,
and a trailing `true` (no top-level `struct`/`function`/`const`/global
assignment; all state lives inside `@testset` scopes) — can be concatenated into
one aggregate `.jl` with zero name-collision or order-dependence risk. The
harness `@testset` gate (#9360) still fails the aggregate on any per-`@testset`
failure, keeping test-level granularity in the failure message.

Rules:
- Only aggregate **concat-safe** fixtures; keep the per-source banner comment and
  the original `@testset` names / Issue numbers (e.g. `@testset "resize! (#6621)"`).
- One manifest `[[tests]]` entry per aggregate (`expected = true`).
- Verify the aggregate's sjulia pass count equals the sum of its members before
  landing. Leave fixtures with top-level defs/globals or any order-dependence as
  standalone files (the #5966 one-process-interaction risk).
- Also verify the aggregate is green under upstream `julia` before landing.
  A member fixture that is already red under upstream julia (fixture drift)
  must stay standalone — folding it in would paint the whole aggregate red for
  `fixture_julia_parity.sh` and hide which member is at fault.
- Exclude path-sensitive fixtures: anything asserting on `@__FILE__` /
  `@__DIR__` or its own on-disk filename (e.g. `isfile(joinpath(@__DIR__,
  "<self>.jl"))`) breaks when its content moves into an aggregate file.
**Module-wrap aggregation** (Issue #10238, unblocked by the #9942 fix): fixtures
with top-level `struct`/`function`/`const`/global definitions cannot be
concatenated directly (name collisions), but CAN be aggregated by wrapping each
former fixture body verbatim in its own top-level `module Agg_<stem> ... end`
inside the aggregate, so definitions stay namespaced. Recipe and rules:

- One `module Agg_<former-fixture-stem>` per source block (`Agg_` prefix so the
  module name can never collide with a type the fixture defines); banner
  comment with the source path and the original `@testset` names / Issue
  numbers preserved. Strip each member's trailing protocol `true`; emit one
  file-level `true` at the aggregate end. Keep `using Test` (and any other
  `using`/`import`) INSIDE each module — modules do not inherit imports.
  Aggregate files are named `<category>_agg_<theme>_NNNN.jl` (NNNN = the
  aggregation Issue).
- **Verify each member wrapped ALONE first** (julia + sjulia): several sjulia
  construct classes are green at top level but diverge inside a `module` —
  inference/reflection APIs on module-local functions (Issue #10343), VM
  crashes / compile errors / silent test loss (Issue #10344). A member whose
  single-module wrap is not pass-count-identical under sjulia AND green under
  julia stays standalone with the Issue reference. A member that turns
  julia-red when wrapped relies on `Main`-scope semantics and also stays
  standalone.
- **Top-level AND `@testset`-local defined names must be pairwise disjoint
  across the members of one aggregate**: sjulia's name-keyed lookups let a
  later sibling module's same-named struct retroactively clobber an earlier
  module's type identity (Issue #10342) and let same-named `@testset`-local
  functions leak across sibling modules (Issue #10345). Do not rely on module
  isolation for same-named definitions until those are fixed.
- Fixtures that override Base methods on Base argument types (method piracy,
  e.g. the `dispatch/*_user_method_4276.jl` family) interact process-globally
  — the combined file is order-dependent even under upstream julia (#5966
  class). They stay standalone with a comment.
- Exclusions carried over from the concat-safe pass still apply: upstream-red
  members, path-sensitive members (`@__FILE__`/`@__DIR__`), fixtures
  referenced by machine-read lists (`docs/vm/WASM_FIXTURE_SMOKE.tsv`,
  `docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv`), and generator-managed fixtures
  (e.g. `types/subtype_matrix_oracle_10049.jl`). Additionally exclude fixtures
  that use `@__MODULE__`, reference `Main`, call `eval`/`@eval`, or define
  modules themselves.
- Landing verification is the same as concat-safe: per aggregate, sjulia pass
  count == sum of the members' standalone pass counts (re-measure the members
  from `git show HEAD:` copies), 0 failures, AND the aggregate green under
  upstream `julia`; then the touched category's `fixture_tests` nextest run.

Pilot: `array` 268 → 213 fixtures (60 concat-safe → 5 themed `array_agg_*_9671.jl`).
Expansion (2026-07-10): `types` / `dispatch` / `type_inference` / `macros` /
`strings` / `reflection` — 258 concat-safe fixtures → 18 themed
`<category>_agg_<theme>_9671.jl` aggregates (suite 3,327 → 3,087 files), each
verified pass-count-exact under sjulia and green under upstream julia.
Module-wrap expansion (2026-07-11, Issue #10238): 305 definition-heavy fixtures
across `array` / `dispatch` / `macros` / `reflection` / `strings` /
`type_inference` / `types` → 25 themed `<category>_agg_<theme>_10238.jl`
aggregates (suite 3,103 → 2,823 files), same pass-count-exact + julia-green
verification on every member (wrapped alone) and every aggregate.

**Example (`tests/fixtures/arithmetic/basic.jl`):**

```julia
function test_basic_arithmetic()
    a = 1 + 2
    b = 10 - 3
    c = 4 * 5
    a == 3 && b == 7 && c == 20
end
test_basic_arithmetic()
```

### Name Uniqueness (Issue #3135)

Test names must be unique across ALL categories. The runtime uses `find()` on merged tests — duplicates silently load the wrong file. Always prefix with the category name.

Run before opening a PR:
```bash
bash scripts/check_fixture_test_names.sh
```

## Test Execution Commands

```bash
# Full test suite (always use timeout)
timeout 1800 cargo nextest run --release

# Fixture tests only
timeout 1800 cargo nextest run --release --test fixture_tests

# Specific fixture category
timeout 1800 cargo nextest run --release --test fixture_tests array::

# Library unit tests only
timeout 1800 cargo nextest run --release --lib

# List all fixture categories
cargo nextest list --test fixture_tests 2>/dev/null | sed 's/::.*/::/;s/ .*//' | sort -u

# Specific test file
timeout 1800 cargo nextest run --release --test dispatch_tests

# Clippy (lint checks)
cargo clippy
```

## Helpers (`tests/common/mod.rs`)

Shared utilities for integration tests:

- `run_core_pipeline(src, seed)` — Parse, lower, compile, run.
- `compile_and_run_str_with_output(src, seed)` — Returns output string.
- `compile_and_run_program_direct(src, seed)` — Returns `(Value, String)`.
- `assert_i64()`, `assert_f64()`, `assert_f32()` — Type-specific assertions.
- `assert_ok_numeric()` — Accepts either I64 or F64 result.

## Adding Rust Unit Tests

For `#[cfg(test)]` modules in library code:

1. Add `#[cfg(test)] mod tests;` to the module's `mod.rs`
2. Create a `tests.rs` file in the same directory
3. Follow the pattern from `lowering/function/tests.rs`:

```rust
use crate::lowering::Lowering;
use crate::parser::Parser;

fn lower_source(source: &str) -> crate::ir::core::Program {
    let mut parser = Parser::new().expect("Failed to init parser");
    let parse_outcome = parser.parse(source).expect("Failed to parse");
    let mut lowering = Lowering::new(source);
    lowering.lower(parse_outcome).expect("Failed to lower")
}

#[test]
fn test_something() {
    let program = lower_source("...");
    // assertions
}
```

4. Run: `timeout 1800 cargo nextest run --release --lib`

## Known SubsetJuliaVM Limitations in Tests

Before writing fixture tests, avoid these tracked patterns that fail in SubsetJuliaVM even though they work in Julia:

- **Avoid property-bearing direct `IOContext(...)` fixtures**: `IOContext(io, :key => value)` fails in sjulia (Issue #6409), while the `iocontext(...)` workaround is sjulia-only and fails upstream Julia fixture validation (Issue #6408). `get(ctx, key, default)` itself works once a context exists.
- **Avoid the `for outer i in itr` modifier form**: `for outer in itr` works as a normal loop variable, but the upstream `outer` modifier form is rejected during lowering rather than mis-executed (Issue #6465).

**Keeping this list updated** (Issue #3173): When a bug fix or issue reveals a new SubsetJuliaVM limitation that affects fixture test authoring, add a bullet here in the same PR. When a limitation is resolved (feature implemented), remove the corresponding bullet.

## Behavioral Changes in Fixture Tests (Issue #2261)

When making behavioral changes, search affected tests; verify hardcoded expectations; document the computation chain.

## Unit Test Conventions for compile/ and vm/ Modules

### IR Literal Pitfall (Issue #3194)

`ir::core::Literal` uses `Literal::Int(i64)` for integer literals — there is NO `Literal::Int64` variant. Quick reference:
- `Literal::Int(v)` — i64 integer
- `Literal::Float(v)` — f64 float
- `Literal::Float32(v)` — f32 float
- `Literal::Bool(v)` — boolean
- `Literal::Str(s)` — string

### Test Helpers for compile/ (Issue #3183)

Use `compile::test_helpers` (only available in `#[cfg(test)]`) for constructing IR nodes:
- `zero_span()` — creates `Span::new(0,0,0,0,0,0)` (Span has no `Default` impl)
- `int_lit(v)` — creates `Expr::Literal(Literal::Int(v), zero_span())`
- `var_expr(name)` — creates `Expr::Var(name, zero_span())`
- `call_expr(fn_name, args)` — creates `Expr::Call` with empty splat/kwargs masks

### Pure Function Test Policy (Issue #3185, #3189, #3191, #3207, #3214, #3224)

Every new standalone `fn` (no `&self`) in `compile/` or `vm/` that takes only primitive/standard types MUST have at least:
- One happy-path test
- One edge-case test (empty input, `None`-returning, boundary condition)

When adding a new pure function, add tests in the same file's `#[cfg(test)] mod tests { ... }` block.

### Lowering Module Test Patterns (Issue #3198, #3200)

Lowering modules fall into two categories:
1. **Pure functions** (testable): `helpers.rs`, `literal.rs`, `collection.rs`, `views.rs` — test with unit tests
2. **CST-walker functions** (hard to isolate): `expr/mod.rs`, `binary.rs`, `call.rs` — test via fixture tests

When adding a pure function to `lowering/`, always add unit tests. For `replace_end_with_lastindex` / `replace_begin_with_firstindex` patterns, test: identity case, dimension-aware case, recursive BinaryOp application, and pass-through for non-keyword vars.

### Self-Free CoreCompiler Method Extraction (Issue #3238)

When adding a method to `impl CoreCompiler` whose body makes no `self.field` access:
1. Extract it as a standalone `pub(super) fn` outside the `impl` block
2. Add unit tests for the standalone function
3. If the method mixes static + `self`-dependent logic, extract the static sub-predicate returning `Option<bool>`

Signals: name starts with `can_`, `is_`, `should_`, `static_`.

### vm/matmul/ and vm/builtins_macro/ Helper Tests (Issue #3227)

Leaf helper modules (`vm/matmul/complex.rs`, `vm/builtins_macro/helpers.rs`) with pure math or string predicate logic MUST have `#[cfg(test)] mod tests` blocks. The parent handler is not a substitute for unit tests.

### Global Atomic State Tests (Issue #3251)

Tests for `get/set_bigfloat_precision` and `get/set_bigfloat_rounding_mode` mutate global atomics.
Each test MUST save and restore state. Use `cargo nextest` (process isolation) for reliable execution.

## Rust Test Assertion Style

When writing `#[cfg(test)]` or `tests.rs` Rust tests (Issue #3053, #3090, #3098):

- **DO NOT** use `match result { pat => {} other => panic!("Expected...", other) }` — fragile anti-pattern
- **DO** use `assert!(matches!(result, ExpectedVariant(..)), "Expected ..., got {:?}", result)`
- **DO** use `assert_eq!` for types implementing `PartialEq`
- **DO** ensure all types used in `assert!(matches!())` with `{:?}` derive `Debug` — if not, add `#[derive(Debug)]`
- New enums and structs should derive `Debug` by default: `#[derive(Debug, Clone, PartialEq)]` for data types
- `std::assert_matches::assert_matches!` is still nightly-only (rust-lang/rust#82775), so use `assert!(matches!())`
- If `panic!` is legitimately needed, annotate with `// OK: panic! — <reason>` on the same line
- This applies to ALL `#[cfg(test)]` blocks — both `tests.rs` files and inline modules in `mod.rs`, `lib.rs`, etc.
- Run `bash scripts/check_no_panic_in_tests.sh` to check for violations (baseline: 0, zero tolerance, scans ALL `.rs` files)
- **Float test values** (Issue #3288, #3290): Avoid `3.14`, `2.71`, `1.41`, `1.73` in test float literals — these trigger the `approx_constant` Clippy lint (π ≈ 3.14159, e ≈ 2.71828, √2 ≈ 1.41421, √3 ≈ 1.73205). Use unambiguous values like `1.25`, `6.78`, `0.75` instead.
- **Duplicate edit pattern workaround** (Issue #3204): When a manual edit has multiple possible matches, make the patch context unique or anchor the insertion at the final module boundary with `apply_patch`. Do not use shell append/redirection for repository edits.

## Related Documentation

- `PANIC_FREE.md` — VM panic-prevention policies
- `ERROR_DESIGN.md` — Error type design guidelines
- `LOWERING.md` — Parser/lowering details
- `CODE_AUDITS.md` — Code audit policies and scripts
- `CHECKLISTS.md` — Implementation checklists for new types/variants
