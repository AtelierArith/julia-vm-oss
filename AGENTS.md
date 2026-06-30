# CLAUDE.md

Guidelines for Working with This Repository

> **Normative authority:** `REPOSITORY_RULES.md` is the root durable rule
> book. This file is the operational entry (build/test commands, git workflow,
> fixture conventions, Agent Skills). When a durable rule is discovered here,
> it is promoted into `REPOSITORY_RULES.md` or `docs/vm/CHECKLISTS.md`. Read
> order and conflict resolution: see "Read Order And Authority" in
> `REPOSITORY_RULES.md`.

## Project Overview

**SubsetJuliaVM** is a static pipeline for running a strict subset of Julia on iOS (no JIT). Pipeline: `Julia source → Parser → Lowering → Compiler → VM → Swift/iOS via C ABI`.

## Design Principles

1. **Full Julia Syntax** — True subset of Julia. Study `julia/` before design decisions, and when implementation choices are unclear, consult the upstream Julia implementation under `./julia` first.
2. **Upstream-Driven Compatibility** — For Julia compatibility gaps, especially parser/lowering/macro/runtime semantics, first identify the corresponding upstream Julia design in `./julia` and implement the sjulia feature in that shape. Do not add ad hoc special cases or package-specific shortcuts when a structural upstream-compatible path is required.
3. **Pure Julia First** — Implement in `subset_julia_vm/src/julia/`; match Julia paths. Avoid new Rust intrinsics.
4. **Multiple Dispatch & Lowering** — Prefer dispatch over type-checking; centralize feature checks in lowering.
5. **Error Spans & Compatibility** — All errors carry precise spans. Output must match official Julia.
6. **Test & Document** — Add fixture tests; verify against Julia. Create Issue first for workarounds. If you discover a bug while implementing, create an Issue with the `bug` label before fixing it. When an sjulia error or Julia-incompatibility is encountered during implementation or probing, report it as a bug Issue before adding a workaround or fix. If you find a feature that **runs in upstream `julia` but errors, fails to parse, or is otherwise unsupported in sjulia**, file an Issue with the `unsupported-feature` label (MWE + julia/sjulia output) before working around it — even if it is incidental to your current task.
7. **VM Performance Priority** — While preserving upstream Julia compatibility as the gold standard, prioritize VM execution improvements over AoT work unless the user explicitly asks for AoT. Prefer optimizations that keep the no-JIT iOS runtime viable and avoid AoT-only assumptions.
8. **No Package-Name Compile Shortcuts** — Do not add compile/lowering/runtime branches that special-case package, module, or type names by string (for example `base_name == "AbstractAlgebra.Integers"`). Fix the structural dispatch, import, lowering, type, or runtime capability instead.
9. **Don't Reset Others' Work** — Never `git checkout`/`stash` files you didn't modify.

## Build & Test

- Subagents: `rust-build-validator`, `test-runner-analyzer`
- **ALWAYS** wrap tests: `timeout 1800 cargo nextest run --release` (30-min max)
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
4. `cargo nextest run --profile release-fast --test fixture_tests <category>::`
5. `timeout 1800 cargo nextest run --release --test fixture_tests <category>::`
6. iOS: `cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim`
   or `./build.sh` (embeds Base cache — see CHECKLISTS.md Issue #2929)

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
timeout 1800 cargo nextest run --release
timeout 1800 cargo nextest run --release --test fixture_tests
timeout 1800 cargo nextest run --test fixture_tests <category>::   # 開発中はカテゴリ指定
timeout 1800 cargo nextest run --lib
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
cargo nextest run --profile release-fast --test fixture_tests <category>::

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

For faster cold test runs (CI, fresh checkouts), `scripts/test_with_cache.sh` embeds
`target/base_cache.bin` into the test binaries so each test process loads Base
from a `&'static [u8]` instead of compiling from source or reading the persistent
on-disk cache. Forwards args to `cargo nextest run --release`. Local repeat runs
already benefit from the persistent cache, so the script is most valuable in CI.

**AoT changes (Issue #6679)**: the default test run uses the empty feature set, so
`#[cfg(feature = "aot")]` code (the `aot` module, `aot_e2e_tests`,
`core_ir_aot_tests`) is NOT built or exercised. After ANY change touching the AoT
pipeline (and periodically), run the AoT gate so codegen regressions don't slip
through (#6629/#5658 did exactly that — there is no PR CI):

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
- `subset_julia_vm_ffi/` — C ABI (`staticlib`/`cdylib` → `libsubset_julia_vm.a`); `include/`
- `subset_julia_vm_parser/` — pure Rust Julia parser / lexer / CST
- `subset_julia_vm_web/` — `wasm-bindgen` bindings (entry: `src/lib.rs`)
- `subset_julia_vm_runtime/` — AoT bytecode runtime
- `.cargo/config.toml` — workspace linker defaults (macOS split-debuginfo, Linux lld)
- `julia/` — Official Julia (reference only)
- `SubsetJuliaVMApp/` — SwiftUI iOS; `mobile/` — Flutter; `docs/vm/` — Docs

## Git Workflow

```bash
git checkout main && git pull
git checkout -b feat/your-feature
git add <files> && git commit -m "..."
git push -u origin your-branch
gh pr create --title "..." --body "..."
gh pr merge --auto --merge   # regular merge, never squash
```

- **Finish-to-main by default**: unless the user explicitly says otherwise,
  when implementation and verification are complete, create a PR and merge it
  into `main` with a regular merge (`gh pr merge --auto --merge` or equivalent).
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

## Adding Functions

1. Find official impl in `julia/base/` or `julia/stdlib/`
2. Reproduce at same path in `subset_julia_vm/src/julia/`
3. Add fixture tests. Use `::Type` (not `::DataType`) for type params — see `BUILTIN_REMOVAL.md`.

Implementation checklists (new types, literals, AoT ops, etc.): see `docs/vm/CHECKLISTS.md`.

## Fixture Tests

- **Path**: `subset_julia_vm/tests/fixtures/<category>/` (NOT outer) — Issue #1768
- **Verify with Julia first**: `julia path/to/test.jl`
- `manifest.toml`: `[[tests]]` with `name`, `file`, `expected`, `description`. End with `true`.
- **Name uniqueness** (Issue #3135): Prefix with category. Run `bash scripts/check_fixture_test_names.sh`.

## Unit Tests & Assertion Style

All conventions (IR literals, test helpers, pure function policy, assertion style, known limitations): see `docs/vm/TESTING_GUIDE.md`.

## Macro Doctests

Full paths in doctests. Use `no_run` for hygiene; never `ignore` except platform-specific.

## After Features/Bugfixes

1. Add tests (fixtures, integration, parser)
2. Update `docs/vm/`: STATUS.md, DONE.md, UNIMPLEMENTED.md
   - STATUS.md / DONE.md merge-conflict policy (Issue #3760): group new entries under a shared date-bearing daily `## ...YYYY-MM-DD...` header, with each issue as its own `### ... (Issue #NNNN)` subsection. If today's header already exists, add a subsection under it instead of prepending another top-level "latest" block or rewriting older entries.
   - Yearly archive policy (Issue #6341): keep only the recent (~3 months, ≤3,000 lines) dated sections in STATUS.md / DONE.md. When the year changes (or a file exceeds 3,000 lines), move older dated sections verbatim to `docs/vm/archive/STATUS-<YYYY>.md` / `docs/vm/archive/DONE-<YYYY>.md` (mechanical cut & paste, no rewriting), upstream Julia NEWS/HISTORY style.
3. Performance impact → add benchmark to `benches/` (Issue #3210)
4. Pipeline/architecture change → update ARCHITECTURE_OVERVIEW.md (Issue #3244), English docs (Issue #3246)
5. New Clippy patterns → update Code Audits (Issue #3292)
6. Run category tests; full test before PR

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
| Type system | TYPE_SYSTEM.md |
| Collections | COLLECTIONS.md |
| Call instructions | CALL_INSTRUCTIONS.md |
| Numeric types | NUMERIC_TYPES.md |
| Binary dispatch | BINARY_DISPATCH.md |
| HOFs | HOF_GUIDE.md |
| Dict indexing | DICT_INDEXING.md |
| Float preservation | TYPE_PRESERVATION.md |
| Panic-free VM | PANIC_FREE.md |
| Pure Julia | PURE_JULIA_DESIGN.md |
| Builtin removal | BUILTIN_REMOVAL.md |
| Complex | COMPLEX.md |
| Status/Done/Unimplemented | STATUS.md, DONE.md, UNIMPLEMENTED.md |
| JSXGraph 統合 | JSXGRAPH.md |
| Builtin handler ownership | BUILTIN_OWNERSHIP.md |
| Thread-local cache + registry | CACHE_ARCHITECTURE.md |
| Type promotion system | PROMOTION.md |
| ConcreteType lattice | LATTICE_TYPE.md |
| Active workarounds | WORKAROUNDS.md |

## Agent Skills (`.cursor/skills/`)

Project-scoped Cursor Agent Skills encode the mandatory workflows below. They
auto-load from their `description` trigger terms; reference explicitly with
`@<name>` when needed.

| Skill | When it applies |
|-------|-----------------|
| `sjulia-dev` | Adding Julia functions (upstream-first), writing fixture tests with parity checks, VM performance / bytecode work, and the git/PR flow. |
| `sjulia-report-gap` | You hit a construct that runs in upstream `julia` but fails in sjulia — STOP, do NOT apply an ad-hoc workaround, file an `unsupported-feature` or `bug` Issue immediately with an MWE + julia-vs-sjulia output table. Applies even when the gap is incidental to your current task. |
| `sjulia-document-workaround` | Adding or removing an ad-hoc workaround: Issue first, in-code `Workaround:` comment with `(Issue #NNNN)`, full entry in `docs/vm/WORKAROUNDS.md` (section + Summary Table W-ID), then run both `check_workarounds_*.sh` scripts. |
| `sjulia-bug-prevention` | After fixing a bug, convert the knowledge into prevention: root cause, why existing tests missed it, a regression test, blast radius, and a prevention mechanism (audit script / checklist / coverage test / fixture / lint) filed as a follow-up Issue. |
| `sjulia-logical-commits` | Commit work as a sequence of logical, self-contained commits — one coherent change per commit (fix + its regression fixture + matching docs/vm update together), each buildable and testable, foundations ordered before consumers, message bodies capturing WHY and the Issue link. Use when finishing a multi-file change or separating mixed concerns before a PR. |
| `sjulia-finish-branch` | Finish a sjulia branch end-to-end: turn its changes into logical commits, open a PR with summary/test plan/linked Issue, and merge with regular merge (`gh pr merge --auto --merge`). Use when a multi-file change is complete and the next step is commit → PR → merge in one continuous flow. |

## Post-PR (Issue #1812)

Update DONE.md, UNIMPLEMENTED.md, STATUS.md. Verify exports: `cargo nextest run --test fixture_tests base_exports_do_not_exceed_upstream`.

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tools** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them. `codegraph_node` returns one symbol's source + callers, or reads a whole file with line numbers. If the tools are listed but deferred, load them by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
