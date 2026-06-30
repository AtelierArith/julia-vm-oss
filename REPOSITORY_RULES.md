# Repository Rules

Status: normative. This is the root durable rule book for the SubsetJuliaVM
(sjulia) repository. It applies to first-party Rust VM code, Pure Julia
`src/julia/` code, the parser crate, the web/iOS bindings, fixtures, docs, and
QA/audit automation. The vendored upstream `julia/` tree is **reference only**
— never modify it; treat it as the compatibility gold standard.

Last amended: 2026-06-30.

## Read Order And Authority

Read these files before changing behavior:

1. `REPOSITORY_RULES.md` — durable repository rules and prohibitions (this file).
2. `CLAUDE.md` / `AGENTS.md` — operational entry: build/test commands, git
   workflow, fixture conventions, and the Agent Skills table.
3. `docs/vm/ARCHITECTURE_OVERVIEW.md` — long-term pipeline architecture, layer
   ownership, and runtime model.
4. `docs/vm/CHECKLISTS.md` — implementation checklists for new types, literals,
   operators, AoT ops, cache format, etc.
5. The relevant topic doc under `docs/vm/` (`LOWERING.md`, `TYPE_SYSTEM.md`,
   `BINARY_DISPATCH.md`, `PROMOTION.md`, `NUMERIC_TYPES.md`, `PANIC_FREE.md`,
   … — see the table in `CLAUDE.md`).
6. The corresponding upstream Julia implementation under `./julia`.

`CLAUDE.md`/`AGENTS.md` are the operational entry files. They may contain
local setup, fast-feedback recipes, and known-limitation notes, but durable
rules discovered there must be promoted into this file or
`docs/vm/CHECKLISTS.md` with `Last amended` bumped.

When two normative documents appear to disagree, stop and reconcile the canon
before changing code. **Upstream Julia is the compatibility gold standard.**
While a conflict is being resolved, the stricter upstream-compatibility /
no-JIT-iOS-viability rule applies.

## Canon-First Change Protocol

- Do not improvise undocumented Julia behavior when code contradicts the canon
  or the canon is silent. Record the assumed behavior and the observed
  behavior, then amend the relevant canon (`docs/vm/<topic>.md` or this file)
  first.
- **Architecture / pipeline changes** amend `docs/vm/ARCHITECTURE_OVERVIEW.md`
  before code (Issue #3244), and English docs when user-facing (Issue #3246).
- **New type / literal / operator / AoT op** amendments land in
  `docs/vm/CHECKLISTS.md` (and `TYPE_SYSTEM.md` / `NUMERIC_TYPES.md` /
  `PROMOTION.md` as appropriate) in the same change as the implementation.
- **Cache format** changes bump `CACHE_VERSION` and amend the SerializedBaseCache
  checklist in `docs/vm/CHECKLISTS.md` (Issue #3240).
- **QA / audit contracts** amend `docs/vm/CODE_AUDITS.md` and the enforcing
  `scripts/check_*.sh` in the same change.
- Canon amendments that change durable prohibitions bump `Last amended` above.
- When sjulia diverges from upstream Julia, the divergence must be recorded as
  an Issue (`unsupported-feature` or `bug`) before any workaround lands. See
  the `sjulia-report-gap` Agent Skill.

## Root-Cause Fix Discipline

- Permanent fixes must address the authoritative cause, not only the visible
  symptom. When a bug exposes a mismatch across parser / lowering / compiler /
  VM / Pure-Julia base / fixture / docs, fix the owning layer first and align
  adapters, fixtures, and tests to that layer.
- Do not add compile/lowering/runtime branches that special-case package,
  module, or type names by string (for example
  `base_name == "AbstractAlgebra.Integers"`). Fix the structural dispatch,
  import, lowering, type, or runtime capability instead.
- Ad hoc patches, Pure-Julia-only workarounds, fixture-only behavior, or
  expectation relaxations are allowed only as temporary diagnostic or
  containment steps. They must:
  - carry a `// Workaround: ... (Issue #NNNN)` (Rust) or
    `# Workaround: ... (Issue #NNNN)` (Julia) comment,
  - be registered in `docs/vm/WORKAROUNDS.md` (Issue #2843),
  - pass `bash scripts/check_workarounds_documented.sh` and
    `bash scripts/check_workarounds_sync.sh`,
  - be tracked in their Issue, and
  - be removed or replaced by the root-cause fix before the PR lands on
    `main`.
- A review finding that can be fixed either by masking the failure or by
  correcting the underlying lowering / dispatch / intrinsic / fixture must
  choose the underlying correction. If that correction requires a canon change,
  follow the Canon-First Change Protocol before code continues.
- After fixing a bug, convert the knowledge into prevention (root cause, why
  tests missed it, regression test, blast radius, prevention mechanism) and
  file it as a follow-up Issue. See the `sjulia-bug-prevention` Agent Skill.

## Architecture And Ownership

- **SubsetJuliaVM** is a static pipeline for running a strict subset of Julia
  on iOS with no JIT:
  `Julia source → Parser → Lowering → Compiler → VM → Swift/iOS via C ABI`.
- **Pure Julia first.** Implement Julia semantics in
  `subset_julia_vm/src/julia/` at the matching upstream path
  (`julia/base/abc.jl` → `subset_julia_vm/src/julia/base/abc.jl`). Avoid new
  Rust intrinsics unless no upstream-shaped Pure-Julia path exists
  (`docs/vm/PURE_JULIA_DESIGN.md`).
- **Multiple dispatch & lowering.** Prefer dispatch over type-checking;
  centralize feature checks in lowering (`docs/vm/LOWERING.md`).
- The Rust VM (`subset_julia_vm/src/`) owns execution, intrinsics, and the
  type lattice. Pure Julia owns method tables and base/stdlib behavior. Do not
  duplicate a base method's semantics in Rust when a Pure-Julia method can
  carry it.
- `subset_julia_vm_parser/` is the pure-Rust Julia parser/lexer/CST. Parsing
  semantics must match upstream Julia CST; lowering consumes its output.
- `subset_julia_vm_web/` (`wasm-bindgen`) and the iOS C ABI / Swift app
  (`SubsetJuliaVMApp/`) are **transport adapters** over the same VM. They must
  not introduce semantics that the VM does not already support. iOS samples
  (`.jl` under `SubsetJuliaVMApp/.../Samples/`) must have a matching Swift
  fallback (`CodeSamples+*.swift`) and an entry in `samples.json`.
- `subset_julia_vm_runtime/` is the AoT bytecode runtime. AoT-only assumptions
  must not leak into the no-JIT VM path (VM performance priority — see below).
- `julia/` is reference only. Never modify, branch, or vendored-edit it.

## Type System And Dispatch Discipline

- Use `::Type` (not `::DataType`) for type parameters — see
  `docs/vm/BUILTIN_REMOVAL.md`.
- Numeric binary operators have a generic `promote`-based fallback that
  **only terminates when `promote` widens both operands to a type with a more
  specific method**. A mixed-type pair with no specific method re-dispatches
  itself forever → unbounded VM call stack / host OOM (Issue #5966). Mirror
  upstream's mixed-type methods (e.g.
  `==(z::Complex{T}, x::Real) where {T<:Real}`). Prefer the parametric
  `Complex{T} where {T<:Real}` form over a bare `::Complex` annotation, which
  the runtime dispatcher can mis-apply under specialization.
- This recursion is dispatch-order / cache / HashMap-seed dependent: it can
  pass every targeted test and only OOM in the **full** suite. After parallel
  merges ALWAYS run a full `cargo nextest run --release`; never `| tail` (it
  hides which test failed).
- Type preservation is a four-layer model (compile-time inference, compile-time
  early-routes, Pure-Julia method table, runtime fallback). Changes touching
  one primitive must verify all four — see `docs/vm/TYPE_PRESERVATION.md` and
  the "New Primitive Type / New Operator Method" checklist in
  `docs/vm/CHECKLISTS.md`. Test both inline-constructor and variable-bound
  `typeof()` forms; the variable-bound form often passes when the inline form
  fails.
- Exhaustive `match` sites on `ConcreteType` / `ArrayElementType` / `IOKind` /
  `Literal` must be updated **together** when a variant is added; add a
  `test_all_<Enum>_variants_*` coverage test so a missing arm fails the build.

## VM Performance Priority

- While preserving upstream Julia compatibility as the gold standard,
  prioritize VM execution improvements over AoT work unless the user
  explicitly asks for AoT. Prefer optimizations that keep the no-JIT iOS
  runtime viable and avoid AoT-only assumptions.
- For VM/codegen work, dump the final compiled bytecode
  (`--dump-bytecode`, `--all` when Base/prelude/generated helpers matter)
  **before** changing runtime fast paths.
- Do not report cold CLI timing as a VM-only result. For CLI comparisons,
  build baseline and current with the same precompiled prelude/Base caches
  (`--precompile-prelude`, `--precompile-base`, then rebuild with
  `SJULIA_PRELUDE_PROGRAM_CACHE` / `SJULIA_BASE_CACHE`). Prefer a
  `Vm::run()`-only Criterion harness that reuses a precompiled
  `CompiledProgram`, and report CLI and VM-only numbers separately.
- Performance impact → add a benchmark to `benches/` (Issue #3210).

## Error Spans And Output Compatibility

- All errors carry precise spans. Parser/lowering/runtime error output must
  match official Julia where the construct is supported.
- `docs/vm/PANIC_FREE.md` — the VM must not panic on user input; surface
  recoverable Julia-level errors instead.
- `docs/vm/ERROR_DESIGN.md` governs error kinds and classification. Adding or
  changing a `VmError` kind updates `scripts/check_vmerror_classification.sh`.

## Build, Test, And Audit Gates

- **ALWAYS** wrap tests with `timeout 1800 cargo nextest run --release`
  (30-min max). Fast feedback first: validate with upstream `julia` and direct
  `target/release/sjulia <fixture>` before category/full nextest. Do not run
  `cargo build` and `nextest` concurrently (artifact-lock contention).
- `cargo clippy --all-targets -- -D warnings` must pass with zero warnings.
  Each `scripts/check_*.sh` is registered in CI (`docs/vm/CODE_AUDITS.md`).
- New `check_*.sh` scripts follow the "Adding a New Audit Script" checklist in
  `docs/vm/CODE_AUDITS.md` and must pass
  `bash scripts/check_audit_scripts_bash3_compat.sh`.
- AoT changes run the AoT gate (the default test feature set does not build
  `#[cfg(feature = "aot")]` code; there is no PR CI — regressions slip
  through, cf. #6629/#5658):
  ```bash
  bash scripts/test_aot.sh
  # nextest filters match on `binary test` (space), not `binary::test`
  ```
- Precompiled Base cache build procedure: see `docs/vm/CHECKLISTS.md`
  (Issue #2929). Bump `CACHE_VERSION` when the serialized cache shape changes.

## Tests And Fixtures

- Fixtures live under `subset_julia_vm/tests/fixtures/<category>/` (NOT the
  outer `tests/fixtures/` — Issue #1768). Categories mirror `docs/vm/` topics.
- **Verify with upstream Julia first:** `julia path/to/test.jl`. Recommended
  parity check: `bash scripts/fixture_julia_parity.sh <fixture.jl>` (exits
  non-zero on pass/fail-count mismatch, Issue #4712).
- `manifest.toml`: `[[tests]]` with `name`, `file`, `expected`, `description`.
  End the fixture file with `true`.
- **Name uniqueness (Issue #3135):** prefix the test name with its category
  (e.g. `arithmetic_basic`). Run
  `bash scripts/check_fixture_test_names.sh`.
- Fast feedback: `bash scripts/fixture_fast_feedback.sh <fixture.jl>…` prints
  the recommended sequential command set. Run those sequentially.
- Generated fixture tests are batched as `chunk_NNN` (default 32) because
  nextest runs each Rust test in a separate process. Keep category-level
  targeting intact. Run `bash scripts/check_fixture_chunk_size.sh` when adding
  fixtures.
- Do not commit `target/` cache binaries (`.bin`), `.env`, or credentials.
  Use synthetic fixture data; do not transcribe real user programs that
  contain private data into fixtures.

## Concurrent Work And Merge-Conflict Avoidance

Parallel work collides on shared surfaces when agents treat them as free-form
append targets. The following rules reduce conflicts without weakening the
serialization points that keep the stack consistent.

### Test Placement

- **Integration-style or projection tests belong under `tests/`.** Tests that
  exercise parser/lowering/compiler/VM projection, IR literals, or
  cross-layer behavior live in `subset_julia_vm/tests/` (Rust) or
  `subset_julia_vm/tests/fixtures/<category>/` (Julia), not inside a
  monolithic `#[cfg(test)] mod tests` block in a source file.
- **Pure unit tests may stay inline.** Small tests for a single pure helper,
  parser, or private algorithm may remain in the source file under
  `#[cfg(test)] mod tests`. When a unit test file grows beyond one screen or
  begins to assert cross-module projection, move it to `tests/`.
- **Do not add new tests to existing monolithic test files.** Add a new
  `tests/<feature>.rs` / fixture file instead. Existing monolithic files may
  be split opportunistically when touched for a new feature.
- **Test fixtures and fakes belong near their consumer.** A fixture used by a
  single feature's tests lives in that feature's `tests/fixtures/<category>/`.
  Shared fixtures live in an append-friendly shared location.

### Shared Hot Files

The main agent owns integration of the following shared surfaces. Subagents
may read them but must not append to them without main-agent coordination:

- `docs/vm/STATUS.md`, `docs/vm/DONE.md`, `docs/vm/UNIMPLEMENTED.md`
- `docs/vm/CHECKLISTS.md`, `docs/vm/CODE_AUDITS.md`, `docs/vm/WORKAROUNDS.md`
- `docs/vm/ARCHITECTURE_OVERVIEW.md`
- `subset_julia_vm/tests/fixtures/<category>/manifest.toml` (all categories)
- `subset_julia_vm/src/julia/base/version.jl` and `Cargo.toml` version fields
- `scripts/check_*.sh` and `.github/workflows/ci.yml`
- `CLAUDE.md`, `AGENTS.md`, `REPOSITORY_RULES.md`

To reduce conflicts on these files:

- **STATUS.md / DONE.md dated-header policy (Issue #3760):** group new entries
  under a shared date-bearing daily `## ...YYYY-MM-DD...` header, with each
  issue as its own `### ... (Issue #NNNN)` subsection. If today's header
  already exists, add a subsection under it instead of prepending a new
  top-level "latest" block or rewriting older entries.
- **Yearly archive policy (Issue #6341):** keep only the recent (~3 months,
  ≤3,000 lines) dated sections. When the year changes or a file exceeds 3,000
  lines, move older dated sections verbatim to
  `docs/vm/archive/STATUS-<YYYY>.md` / `DONE-<YYYY>.md` (mechanical cut &
  paste, no rewriting), upstream Julia NEWS/HISTORY style.
- **`manifest.toml` entries go at the end of the relevant category's file**
  without renumbering or reformatting unrelated entries.
- **Generated/registry artifacts** (e.g. `samples.json`,
  `web/samples_ir.js`, `coreEvents`-style generated files) are edited
  append-friendly: add at the end of the relevant array/object without
  reformatting unrelated entries.

### Parallel Implementation Protocol

- **Serialize shared surface design before parallelizing implementation.** The
  main agent decides module boundaries, enum variants, fixture category, and
  test file names before subagents begin coding. Subagents receive a bounded
  file allow-list and a shared-file deny-list in their prompt.
- **Do not parallelize two agents on the same hot file.** Cap concurrent
  subagents to disjoint territories (typically 2-3). If two features both
  need to change the same hot file, split sequentially or have the main agent
  pre-apply the shared scaffold and let subagents fill module-local bodies.
- **Subagent output is a draft to integrate, not merged evidence.**
  Implementation subagents (`rust-build-validator`,
  `test-runner-analyzer`, etc.) may write module-local code, tests, and docs.
  The main agent still integrates shared canon updates, STATUS/DONE entries,
  manifest additions, audit scripts, and CI wiring.
- **Merge integration branches before landing on `main`.** When multiple
  feature branches run in parallel, create a short-lived integration worktree,
  resolve conflicts and run the full gate there, then fast-forward `main`. Do
  not push a feature branch directly to `main` while another parallel feature
  is still open.

### Worktree And Build Artifact Cleanup

- **Remove temporary worktrees as soon as they are no longer needed.** A
  merged feature branch should not keep a worktree alive. Use
  `git worktree remove --force <path>` when needed.
- **Do not delete the shared `target/`.** The workspace `target/` is shared
  across worktrees and speeds up rebuilds; preserve it. Only delete
  per-worktree `target/` overrides when `CARGO_TARGET_DIR` is isolated.
- **Verify cleanup.** After removing a worktree, confirm
  `git worktree list` and disk usage look reasonable.

## Review And Audit

- **Non-frontier models must receive frontier-model review for substantial
  work.** When a cheaper or non-frontier agent (e.g. an implementation
  subagent) completes a significant change — especially after parallel
  implementation or changes to shared hot files — a frontier model must
  review the diff against the canon (`REPOSITORY_RULES.md`,
  `docs/vm/ARCHITECTURE_OVERVIEW.md`, `docs/vm/CHECKLISTS.md`,
  `docs/vm/CODE_AUDITS.md`, `CLAUDE.md`/`AGENTS.md`). The review must include
  the verification output (nextest, clippy, parity, iOS/WASM builds) and any
  type-preservation / dispatch-sensitive surfaces.
- **Review focus areas.** The auditor must prioritize, in order:
  1. Consistency with repository rules and canon documents.
  2. Upstream Julia compatibility (parity, error spans, output match).
  3. Correctness of lowering, dispatch, type preservation, and VM/codegen.
  4. No-JIT iOS runtime viability and AoT/VM boundary cleanliness.
  5. Fixture/test adequacy and audit-gate coverage.
- **Rule gaps found during audit must be reported as rule-update proposals.**
  If the auditor discovers a problem caused or enabled by a gap, ambiguity, or
  missing rule in the canon, the review must propose an amendment to
  `REPOSITORY_RULES.md`, `docs/vm/CHECKLISTS.md`, or
  `docs/vm/CODE_AUDITS.md` rather than only patching the immediate code. The
  main agent decides whether to adopt, escalate to the user, or defer.
- **Review findings are implementation tasks, not optional suggestions.**
  Blocking issues must be addressed and the relevant gates re-run before
  landing on `main`.
- **Frontier-model-authored implementation is exempt from mandatory external
  review**, but the author should still run the full gate set and self-audit
  before claiming completion. Escalate to the user when uncertain about a
  cross-layer decision.
- **Audit scope is proportional to risk.** A narrow module-local patch may
  need only a quick diff check; a parallel integration that touches shared
  canon, dispatch, type preservation, cache format, or audit scripts needs a
  thorough cross-layer audit.

## Git Workflow And Logical Commits

- Branch from `main`, keep branches short-lived, and use **regular merge**
  (never squash): `gh pr merge --auto --merge`.
- **Issue-driven:** create the `unsupported-feature` / `bug` Issue before any
  workaround or fix; link it in the PR.
- **Logical commits:** commit as a sequence of buildable, self-contained
  logical units — one coherent change per commit (fix + its regression fixture
  + matching `docs/vm` update together; workaround comment + `WORKAROUNDS.md`
  entry together). Stage only named files/hunks (no `git add .` /
  `git add -A`). Message bodies capture WHY and the Issue link, not a
  restatement of the diff. See the `sjulia-logical-commits` Agent Skill.
- **Don't reset others' work:** never `git checkout` / `stash` / `reset --hard`
  files you didn't modify. No destructive git ops without explicit user
  approval.
- **Post-PR (Issue #1812):** update `DONE.md`, `UNIMPLEMENTED.md`,
  `STATUS.md`. After `base/` changes, verify exports:
  `cargo nextest run --test fixture_tests base_exports_do_not_exceed_upstream`.

## Documentation And Work Records

- Dated implementation plans (if any) are subordinate to the normative docs.
  When an implementation discovery changes architecture or rules, amend the
  canon first, then sync or supersede the dated plan.
- Nontrivial agent-driven work leaves a STATUS.md / DONE.md entry under the
  dated-header policy that identifies the canon consulted, the files changed,
  and the verification run. This is required when changing dispatch, type
  preservation, cache format, error design, audit gates, or the parser/
  lowering/compiler/VM pipeline.
- Operational setup and failure notes stay in `CLAUDE.md`/`AGENTS.md` until
  they become durable prohibitions or design rules, then are promoted here.

## Versioning And Licensing

- **Version bump** updates all of:
  `subset_julia_vm/Cargo.toml`, `subset_julia_vm_web/Cargo.toml`,
  `subset_julia_vm/src/julia/base/version.jl` (VersionNumber).
- The vendored `julia/` tree is reference only and retains its upstream
  license. Pure-Julia code ported from upstream `julia/base/` or
  `julia/stdlib/` is reproduced at the matching path under
  `subset_julia_vm/src/julia/` and remains subject to upstream Julia's
  license; preserve applicable notices where required.

## Agent Skills

The mandatory workflows below are encoded as project-scoped Cursor Agent
Skills under `.cursor/skills/`. They auto-load from their `description` trigger
terms and are the operational form of rules in this file.

| Skill | Encodes |
|-------|---------|
| `sjulia-dev` | Upstream-first function addition, fixture tests with parity checks, VM perf/bytecode, git/PR flow. |
| `sjulia-report-gap` | The Canon-First / Root-Cause rule: when upstream `julia` runs a construct but sjulia does not, STOP, file an `unsupported-feature`/`bug` Issue before any workaround. |
| `sjulia-bug-prevention` | Root-Cause Fix Discipline: after a fix, file a prevention Issue (root cause, missed-test reason, regression test, blast radius, prevention mechanism). |
| `sjulia-logical-commits` | Git Workflow: commit as buildable, self-contained logical units with WHY-focused messages and Issue links. |
