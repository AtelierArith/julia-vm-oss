# ADR: Execution Backend Strategy — AoT is a first-class backend (Issue #8639)

*Status: accepted (owner decision, 2026-07-02). Sub-issues: #8651 (data),
#8652 (this document), #8653 (execution).*

> **Supersedes the interim freeze decision.** An earlier draft of this file
> (PR #8791, closing #8652) recorded option A — "freeze & isolate AoT" —
> before the owner ruled on the #8639 decision. The owner then decided
> explicitly to **grow AoT** (2026-07-02). This document replaces that draft;
> option A is retained below as the considered-and-rejected alternative. The
> owner's call is authoritative over an agent's interim interpretation.

This is the decision record for how SubsetJuliaVM maintains its four
execution backends. It complements [REGISTER_VM.md](./REGISTER_VM.md)
(Issue #8448), which remains the authoritative decision record for the
stack-VM → register-VM transition.

---

## Context

sjulia has four execution paths:

| Backend | Role | Verification (before this ADR) |
|---|---|---|
| **Stack VM** | Default runtime (CLI, iOS, WASM) | Full fixture suite, pr-fast CI |
| **Register VM** | Future default (#8448, SSA IR #8440) | Prototype measurements (#8559) |
| **AoT** (Core IR → Rust codegen, Cranelift; `subset_julia_vm_runtime`) | Static native-code output path | `--features aot` only: **nightly** `test_aot.sh` (#8633); no PR gate |
| **WASM** | Web runtime (`subset_julia_vm_web`) | Build-only; execution smoke tracked as #8688 |

The problem this ADR resolves: AoT's standing was **contradictory**. Design
principle 7 ("prioritize VM over AoT unless asked") declared it non-priority,
yet every developer paid its `clippy --features aot` maintenance cost, and its
code was not exercised by the default test run — regressions #6629/#5658
merged unnoticed. That is the worst of both options: strategic deprioritization
with full carrying cost and no protection. With the register VM on the horizon,
the stack VM risked inheriting the same limbo, so an explicit decision was
needed.

### Data (Issue #8651, measured 2026-07-02)

- AoT footprint: ~50.2k LOC in `subset_julia_vm/src/aot/` + ~2.2k in
  `subset_julia_vm_runtime/` — about **13% of the Rust codebase**.
- Activity: 100+ `aot`-labeled issues touched since 2026-05 (the label's
  6-phase milestone history shows sustained investment).
- Test assets: 313 test functions in `aot_e2e_tests` alone, plus
  `core_ir_aot_tests`; `scripts/test_aot.sh` wraps nextest + clippy +
  generated-Rust clippy smoke.
- Shipping status: iOS (`subset_julia_vm_ffi`) and WASM (`subset_julia_vm_web`)
  do **not** enable the `aot` feature — no shipped platform currently executes
  AoT output. Its present value is the CLI/native codegen path and the shared
  design assets (AoT inference engine, optimizer passes) that the SSA IR
  (#8440) / register VM (#8448) work draws on.

### Options considered

- **A. Freeze & isolate**: move `aot/` + runtime into a non-default crate,
  drop the daily clippy obligation, keep only the nightly gate. Lowest
  carrying cost; AoT becomes a preserved asset with a documented thaw
  condition.
- **B. Invest**: keep AoT in the default workspace and give it real CI
  protection so it can be developed as a product surface.

## Decision

**Option B — AoT is a first-class backend and will be actively grown**
(owner decision, 2026-07-02).

### Guaranteed scope (owner-set acceptance bar)

The AoT backend's guarantee is deliberately narrower than the VM's:

- **In scope — MUST compile and run under AoT with upstream-identical
  stdout**: the three benchmark kernels from `benchmarks/` (typed numeric
  code with `for` loops; complex numbers used actively — Mandelbrot is the
  gate proving Complex arithmetic works, not a real/imag decomposition):
  1. coprime-probability π estimate (`calc_pi` family) —
     `tests/fixtures/aot/coprime_pi_acceptance_aot.jl`
  2. Aizawa attractor explicit-Euler kernel —
     `tests/fixtures/aot/aizawa_acceptance_aot.jl`
  3. Mandelbrot escape time (scalar `for` loop on ComplexF64,
     `z = z * z + c`) — `tests/fixtures/aot/mandelbrot_acceptance_aot.jl`
- **Declared growth target (not yet in the bar)**: the **broadcast** form of
  the Mandelbrot gate (`mandelbrot_escape.(C, maxiter)` over
  `C = xs' .+ im .* ys`). It runs on the VM today; under AoT it is blocked by
  #8789 (CLI misses call-graph pruning) and #8790 (generated Rust fails rustc:
  `Value: From<Complex>`, `sum` iterator typing). When those close, promote
  `benchmarks/mandelbrot_bench_broadcast.jl` into the acceptance suite.
- **Out of scope — explicitly NOT guaranteed**: loading third-party /
  bundled packages (`using Plots`, Symbolics, AbstractAlgebra, …) under AoT.
  Package loading remains the VM's job; AoT coverage beyond the bar grows
  opportunistically, gap by gap (each gap is an Issue, not an obligation).

The bar is enforced as code: `test_aot_acceptance_*_8639` in
`aot_e2e_tests.rs` compile each program through the AoT pipeline, **run the
generated binary**, and assert exact stdout parity with upstream Julia. The
same fixtures also run under the VM via `fixtures/aot/manifest.toml`, keeping
VM / AoT / julia in three-way agreement. Benchmark-style acceptance programs
use `for` loops (not `while`) as the canonical hot-loop form.

Known annotation caveat: `::ComplexF64` parameter annotations currently fail
AoT inference against `im`-arithmetic results — use bare `::Complex` until
#8795 closes (the acceptance fixture documents this).

The 4-way benchmark comparison (julia / sjulia AoT / sjulia VM / Python 3.14)
for both Mandelbrot forms lives in
`benchmarks/results/mandelbrot_4way_8639.md`; the VM's Complex-boxing and
broadcast-HOF overheads found while measuring are tracked as #8796 / #8797.

Concretely:

1. **PR-level gate**: `pr-fast` gains a conditional `aot-gate` job that runs
   `scripts/test_aot.sh` whenever a PR touches AoT-relevant paths
   (`subset_julia_vm/src/aot/`, `subset_julia_vm_runtime/`,
   `scripts/test_aot.sh`, the AoT test binaries). The #6629/#5658 class of
   regression is now caught **before merge**, not the morning after.
2. **Nightly full gate stays** (`nightly-gates.yml`, #8633) — it also covers
   AoT breakage caused by non-AoT-path changes (shared VM/compiler code),
   which the path-filtered PR gate intentionally does not rebuild.
3. **The daily `clippy --features aot` obligation stays** — it is now an
   investment, not an unexamined tax.
4. **Design principle updated**: principle 7 in `AGENTS.md` no longer says
   "prioritize VM over AoT unless asked"; it now records both the VM's role
   as the default no-JIT iOS runtime **and** AoT's first-class status, with a
   pointer here.

### What this does NOT change

- The **interpreter VM (→ register VM, #8448) remains the shipped runtime**
  for iOS and WASM. Growing AoT does not mean shipping AoT on iOS — Apple's
  no-JIT constraint applies to runtime codegen, and AoT-on-device would be a
  separate decision with its own record.
- `#[cfg(feature = "aot")]` gating stays: the default build/test matrix is
  unchanged; protection comes from the new CI jobs, not from unconditional
  compilation.
- REGISTER_VM.md's side-by-side policy for the stack VM is untouched. When
  the register VM reaches parity, the stack VM's retirement terms get decided
  there, informed by this ADR's lesson: **no backend lingers in unverified
  limbo — it is either gated in CI or explicitly frozen with a thaw
  condition.**

### Differential verification: the vm_aot lane (Issue #10815)

2026-07-05–07-12 produced five VM/AoT semantic-drift bugs in one week
(#10796, #10731, #10663, #10537, #10523) — all instances of AoT
independently re-deriving scope/type/statement decisions the VM lane already
gets right, and drifting. The acceptance-bar tests above (`test_aot_acceptance_*_8639`)
prove the three guaranteed kernels run correctly; they say nothing about
whether AoT's OTHER supported constructs still agree with the VM. #10815
closed that verification gap:

- **`scripts/metamorphic_equivalence.sh --lane vm_aot`** (Issue #10465's
  differential harness) compares VM vs. AoT (`juliars --minimal-prelude
  --emit-binary`) stdout over a curated corpus, `tests/equivalence/vm_aot.tsv`.
  Before #10815 that corpus held exactly the 3 acceptance-kernel fixtures.
  #10815 widened it to 11 cases spanning Bool/comparison/unary operators,
  the `gcd`/`lcm`/`factorial` integer-utility family, `String`
  concatenation, user-defined recursion, `break`/`continue`, and the two
  existing-but-previously-unreferenced fixtures
  (`builtin_stdout_parity_6999.jl`, `mandelbrot_scalar_aot.jl`) — plus one
  KNOWN divergence (`scope_sibling_rebind_10251`, registered against the open
  #10523) via the harness's two-sided `docs/vm/EQUIVALENCE_KNOWN_DIVERGENCES.tsv`
  ratchet. Widening the corpus itself surfaced three NEW AoT bugs
  (#11180 `im`-shadowing Rust pattern-position collision, #11181 the
  documented `range()` prelude helper is not actually callable, #11182
  invalid-lvalue array index-assignment) and one broken build
  (#11196, `--features aot` test compilation was silently red on `main`).
- **Enforcement**: `bash scripts/test_aot.sh` — the mandatory per-AoT-change
  local gate (hard rule 8) every implementation agent already runs — now
  builds `sjulia --features repl` and runs `--lane vm_aot` plus the harness's
  own `--selftest` as steps [4/8]-[6/8], closing the "differential lane
  exists but only runs at LEAD certification time via `premerge_gate.sh
  --metamorphic`" gap. `scripts/check_test_aot_vm_aot_lane.sh` (source-only,
  registered in `scripts/source_only_audits.tsv`) pins both that
  `test_aot.sh` still invokes the lane + selftest and that the corpus holds
  at least 11 cases, so this specific gate cannot silently regress to the
  "audit exists, nothing runs it" shape #10870/#10912 already found
  elsewhere in the repo.
- **The re-implementation claim itself** (AoT re-derives scope/type
  derivation the VM already solved, e.g. via `SharedFunctionPlan`, Issue
  #9089) is real but not a single bounded fix — it is decomposed into:
  #11195 (scope/binding-identity analysis: #10251/#10523/#11180 are three
  independent name-keyed-map defects with one shared root cause; lands
  first), #11200 (statement/assignment-target lowering: #10796/#11182 are
  two independent instances of AoT re-deriving value-vs-effect and
  read-vs-write decisions `SharedFunctionPlan` already encodes for the VM
  lanes; lands second, depends on #11195), and #11202 (documenting AoT's
  generated-Rust ownership conventions so #10663-class bugs are reviewable
  before rustc catches them; lowest priority, independent of the other two).
  The original proposal's P0 item (a `--pure-rust` config-matrix smoke in
  `test_aot.sh`) is deliberately NOT wired in yet: `--pure-rust` is
  currently broken (#10731, open), so a smoke asserting success would red
  the mandatory gate immediately. That smoke lands in the same PR that fixes
  #10731, not before.

## Consequences

- AoT-touching PRs get slower CI (the gate builds `--features aot`); PRs not
  touching AoT paths are unaffected. This is the accepted price of growth.
- Shared-code refactors that break AoT surface at the nightly gate; the fix
  obligation lies with the breaking change (file an issue, fix forward — the
  same rule as any red gate).
- The AoT roadmap (feature coverage, `.sjir`/codegen surface, runtime crate)
  can now assume CI protection; milestone planning for AoT work should resume
  (the `aot` + `phase-*` labels already exist).

## Revisit conditions

Reopen this decision (new ADR, do not edit history) if any of:

- AoT sees no substantive development for two consecutive quarters despite
  the investment (the carrying cost then buys nothing — option A's freeze
  becomes the honest state).
- The register VM / SSA IR pipeline subsumes AoT's codegen role, making the
  Cranelift/Rust-codegen path redundant.
- A platform decision (e.g. shipping AoT output on iOS/WASM) requires
  stronger guarantees than the current gates — that decision gets its own
  record and likely promotes the AoT gate from conditional to unconditional.
