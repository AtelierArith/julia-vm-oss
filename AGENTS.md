# AGENTS.md

Guidelines for Working with This Repository

> **Canonical agent guide.** This file is the single operational entry for ALL
> coding agents (Claude Code, Codex, opencode, Cursor, Gemini, …). `CLAUDE.md`
> is a symlink to this file — edit here, never fork the content.
>
> **Normative authority:** `REPOSITORY_RULES.md` is the root durable rule
> book. This file is the operational entry (build/test commands, git workflow,
> fixture conventions, Agent Skills). When a durable rule is discovered here,
> it is promoted into `REPOSITORY_RULES.md` or `docs/vm/CHECKLISTS.md`. Read
> order and conflict resolution: see "Read Order And Authority" in
> `REPOSITORY_RULES.md`.

## Project Overview

**SubsetJuliaVM** is a static pipeline for running a strict subset of Julia on iOS (no JIT). Pipeline: `Julia source → Parser → Lowering → Compiler → VM → Swift/iOS via C ABI`.

## Hard Rules — Quick Check

Scan this list before EVERY commit and PR. Each line links to the detailed
section below or an Agent Skill; when in doubt, read the linked detail.

1. **Gap → Issue first.** Upstream `julia` runs it but sjulia doesn't → file
   the `unsupported-feature`/`bug` Issue BEFORE any workaround or silent
   rerouting (`sjulia-report-gap`; Discovery Rules below).
2. **Workarounds are registered.** In-code `(Issue #NNNN)` comment +
   `docs/vm/WORKAROUNDS.md` entry + both `check_workarounds_*.sh` green
   (`sjulia-document-workaround`).
3. **Tests are wrapped and unfiltered.** `timeout 1800 cargo nextest run
   --release …`; never pipe to `| tail`. Run the FULL suite (all binaries —
   fixtures + `--lib` green does not imply full green) after parallel merges
   and before merging VM/compiler/dispatch/inference changes. **Exception:**
   when the *only* change is resolving merge conflicts under `docs/` (no
   code/Rust/fixture/source changes outside `docs/`), full-suite verification
   is not required.
4. **`--features repl` to re-link.** `cargo build --release` alone does NOT
   refresh `target/release/sjulia`; use
   `cargo build --release -p subset_julia_vm --bin sjulia --features repl`.
5. **Pre-PR gates.** `cargo fmt --check` (clippy passing ≠ fmt-clean; format
   only files you touched: `rustfmt --edition 2021 <files>`) and
   `bash scripts/run_clippy_lanes.sh default`. Use the named `repl`, `aot`, and
   `aot-cranelift` lanes when those feature paths are in scope (Issue #11253).
6. **Git hygiene.** Stage named files only (never `git add .`/`-A`); regular
   merge only (never squash); run `git branch --show-current` immediately
   before each commit — shared worktrees get switched by other agents.
7. **Never `git stash`.** The stash is repo-global across sessions/worktrees
   (you can pop another agent's WIP). Isolate files from another ref with
   `git checkout <ref> -- <file>`; park WIP as a branch commit. Never
   checkout/reset files you didn't modify.
8. **AoT gate.** Any AoT-touching change → `bash scripts/test_aot.sh`
   (default features never build `#[cfg(feature = "aot")]` code).
9. **Fixtures verified against upstream.** Run `julia` on the fixture first
   (or `scripts/fixture_julia_parity.sh`); file ends with `true`;
   `manifest.toml` entry; category-prefixed unique name. Don't assert
   iteration/call counts of iterative solvers — assert tolerances.
10. **Finish-to-main, then post-mortem.** Draft PR + lead
    `premerge_gate.sh --pr <N>` certification/merge,
    then `sjulia-postmortem`: memory entry + prevention/follow-up Issues.
11. **No new per-issue test binaries.** Add a `mod` to an existing consolidated
    `tests/*.rs` (`regression_*_tests.rs`, `integration_tests.rs`), or a fixture.
    A new `tests/*.rs` binary links the whole VM rlib — allowed only with a
    `docs/vm/TEST_BINARY_ALLOWLIST.tsv` entry (Issue #9671;
    `check_test_binary_budget.sh`).

## Design Principles

1. **Full Julia Syntax** — True subset of Julia. Study `julia/` before design decisions, and when implementation choices are unclear, consult the upstream Julia implementation under `./julia` first.
2. **Upstream-Driven Compatibility** — For Julia compatibility gaps, especially parser/lowering/macro/runtime semantics, first identify the corresponding upstream Julia design in `./julia` and implement the sjulia feature in that shape. Do not add ad hoc special cases or package-specific shortcuts when a structural upstream-compatible path is required.
3. **Pure Julia First** — Implement in `subset_julia_vm/src/julia/`; match Julia paths. Avoid new Rust intrinsics.
4. **Multiple Dispatch & Lowering** — Prefer dispatch over type-checking; centralize feature checks in lowering.
5. **Error Spans & Compatibility** — All errors carry precise spans. Output must match official Julia.
6. **Test & Document** — Add fixture tests; verify against Julia. Create Issue first for workarounds. If you discover a bug while implementing, create an Issue with the `bug` label before fixing it. If you find a feature that **runs in upstream `julia` but errors, fails to parse, or is otherwise unsupported in sjulia**, file an Issue with the `unsupported-feature` label (MWE + julia/sjulia output) before working around it — even if it is incidental to your current task.
7. **Backend Strategy** — The interpreter VM (→ register VM, Issue #8448) is the default no-JIT iOS/WASM runtime; keep it viable and avoid AoT-only assumptions in shared code. AoT is a **first-class backend under active development** (owner decision 2026-07-02, `docs/vm/ADR_BACKEND_STRATEGY.md`): its guaranteed scope is the three acceptance kernels (coprime pi / Aizawa / Mandelbrot, `tests/fixtures/aot/*_acceptance_aot.jl`) — NOT third-party package loading. AoT-touching changes are CI-gated (pr-fast `aot-gate` + nightly `test_aot.sh`); run `bash scripts/test_aot.sh` locally after AoT changes.
8. **No Package-Name Compile Shortcuts** — Do not add compile/lowering/runtime branches that special-case package, module, or type names by string (for example `base_name == "AbstractAlgebra.Integers"`). Fix the structural dispatch, import, lowering, type, or runtime capability instead.
9. **Single-Threaded VM** — VM/session instances are single-threaded by design; new runtime code may use VM-local `Rc`/`RefCell`/`thread_local!` state and need not preserve `Send`/`Sync` unless a separate design record says so. See `docs/vm/SINGLE_THREADED_VM.md`.
10. **General Over Ad-hoc** — Always implement the general solution that covers all valid cases, not just the narrow case at hand. Scope restrictions (e.g. "Float64 only", "N≤2 only") are acceptable only when the *remaining* cases are structurally impossible or explicitly deferred by a tracked Issue. An ad-hoc shortcut that ignores obviously reachable inputs (other numeric types, larger shapes) is a bug, not a simplification. When in doubt, implement the full range and fall back gracefully rather than silently ignoring inputs.
11. **Don't Reset Others' Work** — Never `git checkout`/`stash` files you didn't modify.

## Second Opinion — 困ったら codex に聞く

When stuck — a subtle correctness bug that survives your first fix, a failure you
cannot explain, a design pivot with unclear consequences, or a risky diff before
merge — get an **independent adversarial review from codex** before pushing
forward: use the `codex-review` skill, or run the CLI directly (e.g.
`codex exec "review this diff for correctness: <paste>"` against your
`git diff`). Feed codex the ACTUAL diff plus the concrete failing symptoms, and
ask it to audit the specific invariant you are unsure about (reachability,
reindexing, transactionality, dispatch soundness, …).

- Treat codex's findings **adversarially**, not deferentially: verify each
  flagged point against a failing test or the code before implementing; if you
  disagree, say why with evidence. Don't blindly implement, don't dismiss.
- Proven decisive (Issue #9787): the full suite showed 3 corrupted-struct
  regressions but not WHY; codex identified the shared-`Rc` double-remap and the
  non-transactional error path — root causes the tests alone could not name.
- Complement, not substitute: codex review does not replace the local gates
  (full `cargo nextest run --release`, `test_aot.sh`, clippy/fmt) — run both.

## Agent Skills (`.agents/skills/`)

Project-scoped Agent Skills live in **`.agents/skills/<name>/SKILL.md`** — the
single canonical location for every agent. `.claude/skills` and
`.cursor/skills` are symlinks to it, so Claude Code and Cursor discover them
natively. Never add a skill under `.claude/` or `.cursor/` directly; add it
under `.agents/skills/` (frontmatter: `name`, `description`).

**Agents without native skill support (Codex, opencode, Gemini, …):** treat
the table below as a dispatch table. When the user's request (or your current
situation) matches a skill's trigger, READ that skill's
`.agents/skills/<name>/SKILL.md` and follow its instructions as if they were
part of this file. A user message like `/create-pr` or "use the report-issue
skill" refers to these files.

| Skill | When it applies |
|-------|-----------------|
| `sjulia-dev` | Adding Julia functions (upstream-first), writing fixture tests with parity checks, VM performance / bytecode work, and the git/PR flow. |
| `sjulia-build-iteration` | `cargo nextest run --release` is too slow, changing `subset_julia_vm` source causes many crates to recompile, or you need faster fixture-test iteration. |
| `sjulia-report-gap` | You hit a construct that runs in upstream `julia` but fails in sjulia — STOP, do NOT apply an ad-hoc workaround, file an `unsupported-feature` or `bug` Issue immediately with an MWE + julia-vs-sjulia output table. Applies even when the gap is incidental to your current task. |
| `report-issue` | File any bug / unsupported-feature / prevention Issue with MWE + julia-vs-sjulia output comparison (enforces the Discovery Rules below). |
| `sjulia-document-workaround` | Adding or removing an ad-hoc workaround: Issue first, in-code `Workaround:` comment with `(Issue #NNNN)`, full entry in `docs/vm/WORKAROUNDS.md` (section + Summary Table W-ID), then run both `check_workarounds_*.sh` scripts. |
| `sjulia-bug-prevention` | After fixing a bug, convert the knowledge into prevention: root cause, why existing tests missed it, a regression test, blast radius, and a prevention mechanism (audit script / checklist / coverage test / fixture / lint) filed as a follow-up Issue. |
| `sjulia-logical-commits` | Commit work as a sequence of logical, self-contained commits — one coherent change per commit (fix + its regression fixture + matching docs/vm update together), each buildable and testable, foundations ordered before consumers, message bodies capturing WHY and the Issue link. |
| `sjulia-finish-branch` | Finish a branch end-to-end: logical commits → draft PR with summary/test plan/linked Issue → lead-certified regular merge (`premerge_gate.sh --pr <N>`). |
| `create-pr` | Commit in logical units and create a PR with `gh` (lighter-weight than `sjulia-finish-branch`; returns to `main` after merge). |
| `resolve-pr-reviews-and-merge` | Merging a PR that has (or may have) open review comments / unresolved review threads: address or answer each thread, resolve, then merge. |
| `fix-bug-issues` | Clear the open `bug`-labeled Issue backlog: collect → fix in parallel git worktrees → PR → merge (multi-agent orchestration; invoking the skill is the opt-in). |
| `github-milestone-triage` | Triage open Issues that lack a milestone: classify and assign them to the appropriate milestone. |
| `sjulia-lead-review-merge` | You are the lead/管理者 for parallel implementation agents: review an agent PR's diff from `origin` (never checkout its branch), union-resolve conflicts with main (DONE/STATUS both-append, manifest `[[tests]]`), run the local gates (Actions is disabled — local checks are the only merge gate, incl. a negative test for any new audit), regular-merge, integrate sibling PRs. |
| `sjulia-postmortem` | A task is finished (PR merged / investigation concluded) and you are about to report completion: record insights in `./memory/`, file the prevention Issue for bug fixes, file follow-up Issues for deferred work. |
| `sjulia-coprime-pi-benchmark` | Run or compare the coprime π benchmark across Julia upstream, sjulia VM, sjulia AoT, and Python 3.14 via uv. |
| `sjulia-mandelbrot-benchmark` | Run or compare the Mandelbrot benchmark (for-loop and broadcast forms) across Julia upstream, sjulia VM, sjulia AoT, and Python 3.14 via uv. |

## Memory / 知見の記録 (`./memory/`)

作業中に得た再利用価値のある知見は、すべてリポジトリ内の `./memory/` 以下に記録する
(セッションを跨いで共有・バージョン管理するため)。`./memory/` がこのリポジトリにおける
知見の正準な置き場所であり、記録した知見はコミットして共有する。

- **階層**: `metadata.type` と同名のサブディレクトリに置く —
  `memory/user/` `memory/feedback/` `memory/project/`(現役 = open Issue が残る作業)
  `memory/reference/`。解決済み project エントリは `memory/archive/project/` へ
  逐語移動(索引外・grep 可)。
- **1ファイル1事実**: 1つの知見を1つの `*.md` ファイルに、frontmatter
  (`name`, `description`, `metadata.type` = `user | feedback | project | reference`)
  付きで書く。`feedback`/`project` は本文に **Why:** と **How to apply:** を続ける。
- **索引**: `./memory/MEMORY.md` に1行 (`- [Title](file.md) — hook`) のポインタを追加する。
  MEMORY.md は索引のみ。知見本体は書かない。索引行は1行・約200字以内に保つ
  (肥大化したら各トピックファイルに詳細を移す)。
- **関連付け**: 本文中で関連メモを `[[other-name]]` (相手の `name` slug) でリンクする。
- **重複回避**: 新規作成前に既存ファイルがカバーしていないか確認し、あれば更新する。
  誤りと判明したメモは削除する。
- コードベース (構造・過去の修正・git 履歴・本ファイル) から自明な内容や、その会話限りの
  内容は記録しない。
- 寿命と昇格 (Issue #8645): `project/` は対象 Issue の close 後に棚卸しし、知見を
  docs/vm/ へ昇格・統合してから `archive/project/` へ移動する。候補列挙:
  `scripts/memory_triage.sh`。open epic の進行に必要な内容は機械判定に関わらず現役に残す。

## Build & Test

For the day-to-day fast-feedback ladder and how to reduce recompilations, see
`sjulia-build-iteration`.

- **ALWAYS** wrap tests: `timeout 1800 cargo nextest run --release` (30-min max)
  is the **PR gate**. For local day-to-day iteration, standardize on
  `--cargo-profile release-fast` — it keeps the same optimization level while
  cutting link/codegen time (`codegen-units = 256`, `lto = false`).
- **Fast feedback first**: validate narrow changes with upstream Julia and
  direct `target/release/sjulia <fixture>` before running category/full nextest.
  Use targeted Rust test filters for local regressions, avoid running build and
  nextest concurrently because they contend on Cargo artifact locks, then finish
  with the relevant category nextest and required iOS builds.
  `scripts/fixture_fast_feedback.sh <fixture.jl>...` prints the recommended
  sequential command set for changed fixture files.

### Rust compile ergonomics

The VM is ~370k lines of Rust; compile time matters. The repo separates **daily
VM work** from **native FFI artifacts** and adds linker/profile knobs.

**Crate layout** (root `Cargo.toml`):

| Crate | `crate-type` | When to build |
|-------|-------------|---------------|
| `subset_julia_vm` | `rlib` only | Always — sjulia, nextest, tests, WASM dep |
| `subset_julia_vm_bytecode` | `rlib` only | Shared program representation: Instr, Value model, VmError, rng, slot/peephole, CompiledProgram, wire IDs (Issue #8656) |
| `subset_julia_vm_types` | `rlib` only | Type system: JuliaType, inference_core, lattice, ir/core, promotion, free_vars |
| `subset_julia_vm_ir` | `rlib` only | Span + error layer (below everything) |
| `subset_julia_vm_ffi` | `staticlib` + `cdylib` | iOS / native C ABI only (`-p subset_julia_vm_ffi`) |
| `subset_julia_vm_parser` | `rlib` only | Parser changes |
| `subset_julia_vm_web` | `cdylib` + `rlib` | WASM (`wasm-pack --profile web-release`) |

- `subset_julia_vm_ffi` keeps `[lib] name = "subset_julia_vm"` so Xcode scripts
  still produce `libsubset_julia_vm.a`. C ABI lives in `subset_julia_vm_ffi/src/`;
  headers in `subset_julia_vm_ffi/include/`. VM internals shared with FFI go through
  `subset_julia_vm::ffi_support` (`#[doc(hidden)]`).
- **`default-members`** excludes `subset_julia_vm_ffi`: bare `cargo build` at the
  workspace root does **not** compile `staticlib`/`cdylib`.
- **Do not** put `lto = true` on shared `[profile.release]` — WASM size opts live
  in `[profile.web-release]` only (Issue #6922).

**`.cargo/config.toml`** (committed; optional sccache locally):

- macOS: `split-debuginfo=unpacked` — faster linking of large VM artifacts
- Linux: `clang` + `lld` (`apt-get install lld` on Debian/Ubuntu)
- Optional: `brew install sccache` then `export RUSTC_WRAPPER=sccache` (CI sets
  this via workflow env + `mozilla-actions/sccache-action`)

**Iteration profiles** (root `Cargo.toml`; not for perf benchmarks or shipping):

| Profile | Use |
|---------|-----|
| `dev-fast` | Quick sjulia runs (`opt-level = 1`) → `target/dev-fast/sjulia` |
| `release-fast` | Faster category nextest (`codegen-units = 256`, `lto = false`) |

**Recommended inner loop** (fastest → slowest):

1. `cargo check -p subset_julia_vm --features repl` — typecheck only
2. `julia --startup-file=no path/to/fixture.jl` — upstream parity
3. `cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features repl`
   then `target/dev-fast/sjulia path/to/fixture.jl`
4. `timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests <category>::`
5. `timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests` (all fixtures, still fast)
6. iOS: `cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim`
   or `./build.sh` (embeds Base cache — see CHECKLISTS.md Issue #2929)

Run the full `timeout 1800 cargo nextest run --release` only as the final PR
merge gate; `release-fast` is the default profile for local nextest.

**Avoid**: full-suite `cargo nextest run --release` as the first check; concurrent
`cargo build` + nextest; rebuilding with `SJULIA_BASE_CACHE` set unless embedding
cache (forces relink of all dependents).

- **VM bytecode debug**: for VM/codegen performance work, dump the final compiled
  bytecode before changing runtime fast paths. Use `--dump-bytecode` to inspect
  slot types, slotized loads/stores, peephole results, and direct/dynamic calls:
  `cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode <file.jl>` or
  `cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode -e '<code>'`.
  The default dump shows user functions plus a short main tail; add `--all`
  when Base/prelude or generated helpers are relevant.
- **VM performance measurements**: do not report cold CLI timing as a VM-only
  result. For CLI comparisons, build both baseline and current with the same
  precompiled caches: generate `--precompile-prelude` and `--precompile-base`,
  then rebuild with `SJULIA_PRELUDE_PROGRAM_CACHE=<abs>` and
  `SJULIA_BASE_CACHE=<abs>`. This embeds the parsed/lowered prelude Program and
  Base bytecode cache into the release binary; it does not precompile the user
  program bytecode. For VM optimization work, also prefer a `Vm::run()`-only
  Criterion harness that reuses a precompiled `CompiledProgram`, and report CLI
  and VM-only numbers separately.

```bash
# PR gate (run before merge)
timeout 1800 cargo nextest run --release

# Local day-to-day iteration (default: use release-fast)
timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests
timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests <category>::
timeout 1800 cargo nextest run --cargo-profile release-fast --lib
cargo nextest list --test fixture_tests 2>/dev/null | awk '{print $2}' | awk -F'::' '{print $1}' | sort -u   # カテゴリ一覧

# Typecheck / narrow builds
cargo check -p subset_julia_vm --features repl
cargo build -p subset_julia_vm_parser                              # parser-only

# sjulia (always -p subset_julia_vm --features repl for release refresh)
cargo run -p subset_julia_vm --bin sjulia --features repl
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode <file.jl>
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode -e 'f(x)=x+1; f(41)'
cargo build --release -p subset_julia_vm --bin sjulia --features repl
cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features repl

# Long-session stress tests default to 100 iterations locally. Set this to run
# the full 1000-iteration CI version (e.g. before merging or when debugging #8625/#9787):
#   SJULIA_LONG_SESSION_ITERATIONS=1000 cargo nextest run --cargo-profile release-fast --test regression_scope_session_tests

# Precompiled cache build for cold CLI performance comparisons:
mkdir -p target
cargo build --release -p subset_julia_vm --bin sjulia --features repl
./target/release/sjulia --precompile-prelude "$(pwd)/target/prelude_program_cache.bin"
./target/release/sjulia --precompile-base "$(pwd)/target/base_cache.bin"
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
# IMPORTANT: for CLI timing or cold-start comparisons, the first release build is
# only the helper binary used to generate caches. Treat `target/release/sjulia` as
# cache-embedded only after the second build above, with both cache env vars set.
# The embedded caches cover prelude/Base, not the user program bytecode.

# iOS / WASM (native FFI crate — not built by default-members)
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim
wasm-pack build --target web --profile web-release
xcodebuild -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj -scheme SubsetJuliaVMApp -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPad (A16)' build
```

Precompiled Base Cache build procedure: see `docs/vm/CHECKLISTS.md` (Issue #2929).

**iOS REPL E2E automation**: `scripts/ios_repl_e2e.sh` boots a simulator,
optionally builds+installs the app (`--build`), launches it, then pastes a Julia
snippet into the REPL, runs it, and screenshots — the scripted version of the
reproduce-in-REPL flow (e.g. Issue #8214). The UI driver is
`scripts/ios_repl_paste.py` (Quartz CGEvents + accessibility-tree element lookup;
runs via `uv`). Needs macOS Accessibility permission for the controlling
terminal. `--build` only rebuilds the Swift app; rebuild the xcframework with
`./build.sh` first to pick up Rust/VM or bundled-package `.jl` changes.

**Full sample sweep**: `scripts/ios_samples_e2e.py` runs *every* `samples.json`
sample through the app and screenshots each — `--mode editor` (default; paste into
the editor + Run) or `--mode repl` (Reset + paste + Enter). It best-effort reads
the output to flag samples whose result contains an error, and writes a
`report.txt` + per-sample PNGs. Note: in `--mode repl`, samples that assign a
top-level tuple global (e.g. the ODE samples' `tspan = (...)`) currently surface
Issue #8243.

```bash
scripts/ios_repl_e2e.sh --code-file snippet.jl --screenshot out.png
scripts/ios_repl_e2e.sh --build --code 'using Plots; plot(sin)' --screenshot out.png
uv run scripts/ios_repl_paste.py --dump-ax   # debug: print the REPL accessibility tree
uv run scripts/ios_samples_e2e.py --out-dir /tmp/e2e --launch              # all samples, Editor
uv run scripts/ios_samples_e2e.py --out-dir /tmp/e2e --mode repl --launch  # all samples, REPL
```

Caveat (macOS): heavy app/Simulator restarting can wedge System Events'
accessibility window enumeration (queries return 0 windows / `-1719` while Quartz
still sees the window). If element lookup starts failing, fully quit & reopen the
Simulator (or re-login) to reset its AX state.

For faster cold test runs (CI, fresh checkouts), `scripts/test_with_cache.sh` embeds
`target/base_cache.bin` into the test binaries so each test process loads Base
from a `&'static [u8]` instead of compiling from source or reading the persistent
on-disk cache. Forwards args to `cargo nextest run --release`. Local repeat runs
already benefit from the persistent cache, so the script is most valuable in CI.

**AoT changes (Issue #6679)**: the default test run uses the empty feature set, so
`#[cfg(feature = "aot")]` code (the `aot` module, `aot_e2e_tests`,
`core_ir_aot_tests`) is NOT built or exercised. After ANY change touching the AoT
pipeline (and periodically), run the AoT gate so codegen regressions don't slip
through (#6629/#5658 did exactly that — now caught before merge by pr-fast's
`aot-gate` on AoT-touching PRs, and nightly by `nightly-gates.yml`, Issue #8633;
see `docs/vm/ADR_BACKEND_STRATEGY.md`):

```bash
bash scripts/test_aot.sh                 # nextest --features aot + clippy --features aot
# equivalently, by hand:
timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot --no-fail-fast
timeout 1800 cargo clippy -p subset_julia_vm --features aot --all-targets -- -D warnings
```

Note: nextest filters match on `binary test` (space-separated), not `binary::test`;
pass a bare test-function name (`aot_e2e_tests::...` matches 0 tests).

## Directory Structure

- `subset_julia_vm/src/` — Core VM; `src/julia/` — Pure Julia (base/, stdlib/, packages/)
- `subset_julia_vm_bytecode/` — shared program representation (Instr, Value model, VmError, rng, slot/peephole, CompiledProgram, wire IDs)
- `subset_julia_vm_types/` — type system (JuliaType, inference_core, lattice, ir/core); `subset_julia_vm_ir/` — span/error layer
- `subset_julia_vm_ffi/` — C ABI (`staticlib`/`cdylib` → `libsubset_julia_vm.a`); `include/`
- `subset_julia_vm_parser/` — pure Rust Julia parser / lexer / CST
- `subset_julia_vm_web/` — `wasm-bindgen` bindings (entry: `src/lib.rs`)
- `subset_julia_vm_runtime/` — AoT bytecode runtime
- `.cargo/config.toml` — workspace linker defaults (macOS split-debuginfo, Linux lld)
- `.agents/skills/` — Agent Skills (canonical; `.claude/skills` / `.cursor/skills` are symlinks)
- `julia/` — Official Julia (reference only)
- `SubsetJuliaVMApp/` — SwiftUI iOS; `mobile/` — Flutter; `docs/vm/` — Docs

## Git Workflow

```bash
git checkout main && git pull
git checkout -b feat/your-feature
git add <files> && git commit -m "..."
git push -u origin your-branch
gh pr create --draft --title "..." --body "..."
bash scripts/premerge_gate.sh --pr <N>  # lead certification -> ready -> regular merge
```

- **Finish-to-main by default**: unless the user explicitly says otherwise,
  when implementation and verification are complete, create a PR and merge it
  into `main` through the guarded draft-certification flow below.
- **Guarded final gate for Rust-touching merges** (Issue #9644): before
  merging, run `bash scripts/premerge_gate.sh` (options: `--merge-main`,
  `--nextest '<filter>'`, `--full-suite`, `--pr <N>`). It refuses to certify a
  branch that does not contain the exact current `origin/main`, runs
  `cargo clippy --all-targets -- -D warnings` (+ requested nextest gates),
  re-fetches, and FAILS if `origin/main` advanced during the verification
  window — a clippy result from a stale base is not a gate result (#9641).
  Agent-created implementation PRs stay draft until this gate passes. With
  `--pr <N>`, the gate verifies the draft PR's exact base/head before and after
  the requested gates, publishes the required `sjulia/guarded-certification`
  status, marks it ready, and performs the pinned regular merge. The GitHub
  ruleset rejects uncertified or stale heads; check it with
  `bash scripts/github_merge_ruleset.sh --check`. Implementation agents never
  mark ready or merge themselves (Issues #11056/#11087).
  **Docs-only conflict resolution:** if the merge touch is *only* resolving
  conflicts under `docs/` (e.g. STATUS.md / DONE.md both-append under the
  shared daily header; no non-`docs/` file changes), skip the full nextest
  suite — do not run `premerge_gate.sh --full-suite` for that alone.
- **Issue-Driven**: Create Issue before workarounds; link in PRs. Labels: `unsupported-feature`, `bug`.
- **Bug Discovery Rule**: If you encounter an existing sjulia error during
  implementation, investigation, or reproduction commands, or find a crash,
  upstream Julia compatibility gap, or implementation blocker, create a GitHub
  Issue with the `bug` label before adding a workaround or fixing it. Reference
  the Issue number in the workaround comment, docs, tests, and PR.
- **Unsupported-Feature Discovery Rule**: When you hit a construct that **works in
  upstream `julia` but does not work in sjulia** (parse error, "unsupported"/"not
  implemented" runtime error, MethodError on otherwise-valid syntax, etc.) — even if
  you found it incidentally while doing something else — do NOT just route around it.
  Create a GitHub Issue with the `unsupported-feature` label first, including a minimal
  MWE and a julia-vs-sjulia output table, then reference that Issue number in any
  workaround comment, tests, and PR. Decision rule: use `unsupported-feature` when
  sjulia **cannot run** the construct; use `bug` when sjulia **runs but produces wrong
  output**.

## Workaround Management

- Comment format: `// Workaround: ... (Issue #XXXX)` in Rust or `# Workaround: ... (Issue #XXXX)` in Julia. Centralized list: `docs/vm/WORKAROUNDS.md` (Issue #2843).
- **Adding**: Create Issue → add `(Issue #NNNN)` comment → add to WORKAROUNDS.md → run `bash scripts/check_workarounds_documented.sh` and `bash scripts/check_workarounds_sync.sh`
- **Removing**: Delete comment → move to "Resolved" in WORKAROUNDS.md → add regression test → run both check scripts

## Code Audits

All audit policies with full details and the "Adding a New Audit Script" checklist: see `docs/vm/CODE_AUDITS.md`. Key rule: `cargo clippy --all-targets -- -D warnings` must pass (zero warnings). Each `scripts/check_*.sh` is registered in CI.

## Version Bump

Update: `subset_julia_vm/Cargo.toml`, `subset_julia_vm_web/Cargo.toml`, `subset_julia_vm/src/julia/base/version.jl` (VersionNumber).

**C ABI version bump** (when struct/enum/function signatures change in `subset_vm.h`): see
`docs/vm/CHECKLISTS.md` §"ABI Change Checklist". Short form: bump `SUBSET_VM_ABI_VERSION`
in the header AND `SUBSET_VM_C_ABI_VERSION` in `subset_julia_vm_ffi/src/abi_version.rs`,
update the matching literal in `subset_julia_vm_web/src/lib.rs`, `kSubsetVMABIVersion`
in `REPLSessionManager.swift`, `_kSubsetVMABIVersion` in `mobile/lib/ffi/vm_bridge.dart`,
then run `bash scripts/check_ffi_abi_version.sh --update` to refresh the baseline.
`check_ffi_abi_version.sh` enforces this in CI; it exits 1 if the header signature hash
changed without a version bump.

## Adding Functions

1. Find official impl in `julia/base/` or `julia/stdlib/`
2. Reproduce at same path in `subset_julia_vm/src/julia/`
3. Add fixture tests. Use `::Type` (not `::DataType`) for type params — see `BUILTIN_REMOVAL.md`.

Implementation checklists (new types, literals, AoT ops, etc.): see `docs/vm/CHECKLISTS.md`.

### Numeric operators & the promote-fallback recursion trap (Issue #5966)

Numeric binary operators have a generic promote-based fallback, e.g.
`==(x::Number, y::Number) = (px,py = promote(x,y); px == py)`. This fallback only
**terminates when `promote` widens both operands to a type that has a more-specific
method.** If a mixed-type pair (e.g. `Real == Complex`) has **no specific method**
and `promote` fails to widen, the fallback re-dispatches itself on the unchanged
pair **forever** → unbounded VM call stack / host OOM.

- When adding or extending a numeric type, mirror upstream's **mixed-type** methods
  (e.g. `==(z::Complex{T}, x::Real) where {T<:Real}`, `==(x::Real, z::Complex{T})`),
  not just the same-type ones. Verify with `julia` that no mixed pair reaches the
  promote fallback. Prefer the **parametric** `Complex{T} where {T<:Real}` form over a
  bare `::Complex` annotation — a bare abstract annotation can be loosely matched to a
  non-Complex value by the runtime dispatcher under specialization (it mis-applied
  `Real == Real` to a Complex method).
- This recursion is **dispatch-order / cache / HashMap-seed dependent**: it can pass
  every targeted test and only OOM in the **full** suite (one process, many fixtures,
  cache-loaded Base, accumulated state). After parallel merges ALWAYS run a full
  `cargo nextest run --release`; never `| tail` (it hides which test failed).
- When adding a numeric type, boundary value, or binary operator, update the
  numeric matrix generator in `scripts/gen_numeric_matrix_fixture.jl`
  (`REDUCED_VALUE_SPECS` / `FULL_VALUE_SPECS` / `OP_SPECS`), regenerate the
  reduced oracle fixture, and run the reduced comparator. If the full profile
  count changes, update `docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv` or
  `docs/vm/NUMERIC_MATRIX_FULL_SKIPLIST.tsv` in the same PR with linked Issues.
  See `docs/vm/CHECKLISTS.md` for the exact commands (Issue #8698).

## Fixture Tests

- **Path**: `subset_julia_vm/tests/fixtures/<category>/` (NOT outer) — Issue #1768
- **Verify with Julia first**: `julia path/to/test.jl`
- **Parity check (recommended)**: `bash scripts/fixture_julia_parity.sh path/to/test.jl` — runs the fixture under both `sjulia` and upstream `julia` and exits non-zero on mismatched pass/fail counts (Issue #4712 / PR #4713)
- **Parity sweep (category-wide)**: `bash scripts/check_fixture_parity_sweep.sh <category> ...` — runs every registered fixture of the categories under both interpreters (`--red-green`) and ratchets known drift through `docs/vm/FIXTURE_PARITY_SWEEP_ALLOWLIST.tsv` (Issue #10246; nightly `fixture-parity-sweep` job)
- **Cache-sensitive fixtures** (Issue #10223): GC/WeakRef/finalizer or cache-restore-identity fixtures get `cache_sensitive = true` in their manifest entry; `bash scripts/check_cache_sensitive_fixture_lane.sh` runs tagged categories under BOTH cache modes and fails on divergence (nightly `cold-cached-parity` job)
- **manifest.toml**: `[[tests]]` with `name`, `file`, `expected`, `description`. End with `true`.
- **Name uniqueness** (Issue #3135): Prefix with category. Run `bash scripts/check_fixture_test_names.sh`.
- **Full-suite journal** (Issue #8708): set `SJULIA_FIXTURE_JOURNAL=/tmp/fixtures.jsonl` when running `fixture_tests` to append each executed fixture path, cache state, and RNG seed for post-crash replay.

## Unit Tests & Assertion Style

All conventions (IR literals, test helpers, pure function policy, assertion style, known limitations): see `docs/vm/TESTING_GUIDE.md`.

## Macro Doctests

Full paths in doctests. Use `no_run` for hygiene; never `ignore` except platform-specific.
For executable `docs/vm/*.md` examples, use a ```` ```julia-doctest ```` fence
with `# output` followed by expected stdout, then run `bash scripts/docs_doctest.sh`.

## After Features/Bugfixes

1. Add tests (fixtures, integration, parser)
2. Update `docs/vm/`: STATUS.md, DONE.md, UNIMPLEMENTED.md
   - STATUS.md / DONE.md merge-conflict policy (Issue #3760): group new entries under a shared date-bearing daily `## ...YYYY-MM-DD...` header, with each issue as its own `### ... (Issue #NNNN)` subsection. If today's header already exists, add a subsection under it instead of prepending another top-level "latest" block or rewriting older entries.
   - Yearly archive policy (Issue #6341): keep only the recent (~3 months, ≤3,000 lines) dated sections in STATUS.md / DONE.md. When the year changes (or a file exceeds 3,000 lines), move older dated sections verbatim to `docs/vm/archive/STATUS-<YYYY>.md` / `docs/vm/archive/DONE-<YYYY>.md` (mechanical cut & paste, no rewriting), upstream Julia NEWS/HISTORY style.
3. Performance impact → add benchmark to `benches/` (Issue #3210)
4. Pipeline/architecture change → update ARCHITECTURE_OVERVIEW.md (Issue #3244), English docs (Issue #3246)
5. New Clippy patterns → update Code Audits (Issue #3292)
6. Run category tests; `pr-fast` CI gate runs automatically on the PR (fmt / clippy / fixture-tests / audits). Full-suite verification is **not** required when (a) the diff only updates `docs/vm/STATUS.md` and/or `docs/vm/DONE.md`, or (b) the *only* change is resolving merge conflicts under `docs/` (no non-`docs/` file changes).
7. Post-mortem (`sjulia-postmortem`): record insights in `./memory/`, file the prevention Issue (bug fixes) and follow-up Issues (deferred work) before reporting completion

## Code Samples

- **iOS**: `.jl` in `SubsetJuliaVMApp/.../Samples/<folder>/`, entry in `samples.json`, Swift fallback in CodeSamples+*.swift
- **Web**: Add to `web/samples_ir.js`

## Parser/Lowering

Both forms: `function f(args...) ... end` and `f(args...) = expr`. See `docs/vm/LOWERING.md`.

## Nested Functions & Closures

- **Qualified names**: `parent#nested` internally
- **Closure** = captures outer vars; **non-closure** = shadows params only
- **Deep nesting** (Issue #1764): `collect_block_functions`, `analyze_free_variables`, `outer_scope_vars` (include `captured_vars`), `get_value_from_frame`. See `fixtures/closures/`.

## Key References (docs/vm/)

| Topic | File |
|-------|------|
| **Architecture overview** | **ARCHITECTURE_OVERVIEW.md** |
| **Code audits** | **CODE_AUDITS.md** |
| **Implementation checklists** | **CHECKLISTS.md** |
| **Testing guide** | **TESTING_GUIDE.md** |
| Lowering, CST | LOWERING.md |
| Generator representation decision (#9200 S6) | GENERATOR_REPRESENTATION.md |
| Type system | TYPE_SYSTEM.md |
| Subtyping algorithm: upstream shape vs. sjulia gap map | SUBTYPING.md |
| Collections | COLLECTIONS.md |
| Call instructions | CALL_INSTRUCTIONS.md |
| Numeric types | NUMERIC_TYPES.md |
| Binary dispatch | BINARY_DISPATCH.md |
| HOFs | HOF_GUIDE.md |
| Dict indexing | DICT_INDEXING.md |
| Float preservation | TYPE_PRESERVATION.md |
| Panic-free VM | PANIC_FREE.md |
| Grandfathered panic-debt classification + retirement plan (Issue #10869) | PANIC_DEBT_RETIREMENT.md |
| Task scheduler / VM-level continuations (#10269) | ADR_TASK_CONTINUATIONS.md |
| VM memory management | VM_MEMORY_MANAGEMENT.md |
| Pure Julia | PURE_JULIA_DESIGN.md |
| Builtin removal | BUILTIN_REMOVAL.md |
| Complex | COMPLEX.md |
| Status/Done/Unimplemented | STATUS.md, DONE.md, UNIMPLEMENTED.md |
| JSXGraph 統合 | JSXGRAPH.md |
| Builtin handler ownership | BUILTIN_OWNERSHIP.md |
| Thread-local cache + registry | CACHE_ARCHITECTURE.md |
| Type promotion system | PROMOTION.md |
| ConcreteType lattice | LATTICE_TYPE.md |
| Interned concrete type IDs (dispatch identity) | TYPE_INTERNING.md |
| Active workarounds | WORKAROUNDS.md |
| Symbolics subset | SYMBOLICS.md |
| Upstream Julia parity target | PARITY_TARGET.md |
| Lezer-compatible parser rewrite (Issue #11049) | LEZER_PARSER.md |
| Regex PCRE2 parity checklist (fancy-regex) | REGEX_PCRE2_PARITY.md |
| Crate split plan (compile/vm layering) | CRATE_SPLIT.md |
| North Star 指標 | NORTH_STAR.md |
| Prevention-issue root-cause verification map (Issue #10983) | PREVENTION_MAP.md |
| Exception taxonomy parity: type/layer/catchability vs. upstream (Issue #10813) | EXCEPTION_PARITY.md |

## Post-PR (Issue #1812)

Update DONE.md, UNIMPLEMENTED.md, STATUS.md. Verify exports: `cargo nextest run --test fixture_tests base_exports_do_not_exceed_upstream`.

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tools** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them. `codegraph_node` returns one symbol's source + callers, or reads a whole file with line numbers. If the tools are listed but deferred, load them by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
