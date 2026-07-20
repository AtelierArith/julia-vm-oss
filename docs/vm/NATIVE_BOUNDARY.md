# ADR: Native Boundary Strategy Under WASM + iOS Constraints (Issue #9570)

*Status: accepted, 2026-07-11.*

## Decision

SubsetJuliaVM supports Web/WASM and iOS as mandatory targets. Therefore the
runtime does **not** expose upstream Julia's arbitrary user-visible native
boundary (`ccall`, `@cfunction`, `dlopen`, raw `unsafe_*` pointer APIs) as a
general feature. A native call may enter sjulia only through a curated VM,
compiler, or host wrapper that has an explicit cross-target policy.

The per-library policy choices are:

| Policy | Meaning | Admission rule |
|---|---|---|
| A | Pure Rust / VM intrinsic / curated Rust host wrapper | Default. Must build for CLI, WASM, and iOS without dynamic native loading. |
| B | Dual-compiled C/C++ library | Exception only. Requires a separate ADR plus a successful iOS static-library build and a WASM build path before landing. |
| C | Permanent divergence or unsupported native boundary | Document the divergence or exclusion. Do not route through a fake/stub `ccall` that returns upstream-looking but false values. |

Current ledger state: **no Policy-B rows**. The project has no accepted
dual-compiled C library boundary today. Introducing one requires a new ADR and
must update the generated ledger in this directory.

## Generated ccall Ledger

The line-level upstream inventory is generated into
[`NATIVE_BOUNDARY_CCALL_LEDGER.tsv`](./NATIVE_BOUNDARY_CCALL_LEDGER.tsv).
Regenerate and check it with:

```bash
SJULIA_UPSTREAM_JULIA=/path/to/julia scripts/audit_native_boundary_ccall.sh --write
SJULIA_UPSTREAM_JULIA=/path/to/julia scripts/audit_native_boundary_ccall.sh
```

This worktree's `julia/` submodule may be uninitialized, so the script accepts
`SJULIA_UPSTREAM_JULIA`. The ledger is intentionally line-level: when upstream
adds or moves a `ccall`, the diff shows which policy bucket must be reviewed.

Snapshot from the initial ledger:

| Policy | Rows | Main families |
|---|---:|---|
| A | 958 | Julia runtime intrinsics, OS/libuv/IO wrappers, BigInt, BigFloat, Regex, libm/LLVM intrinsics, Unicode/string-memory helpers, reflection/profiling metadata |
| C | 285 | LibGit2/Pkg native surface, dynamic loading, dSFMT divergence, user-native boundary syntax, shared-memory/process/thread-only surfaces, docs/examples, upstream test-only calls |
| B | 0 | None |

## Family Decisions

| Family | Policy | Issue | Decision |
|---|---|---|---|
| Julia runtime `jl_*` primitives | A | #9570 | Mirror through VM/compiler metadata or explicit Rust intrinsics. These are not user-extensible native calls. |
| libuv / filesystem / process / sockets / file watching | A | #9570 | If supported, implement as curated Rust host wrappers with WASM/iOS fallbacks. Do not expose arbitrary libuv `ccall`. |
| libc / OS utility wrappers | A | #9570 | If supported, implement as curated Rust host wrappers with WASM/iOS fallbacks. |
| memory / string primitives | A | #9570 | Keep as VM-owned or pure-Rust helpers. Do not expose raw libc entry points. |
| Unicode / utf8proc surface | A | #9570 | Mirror through VM-owned or pure-Rust Unicode support; utf8proc is not exposed as a user native boundary. |
| BigInt / GMP surface | A | #9570 | Use the current pure-Rust BigInt implementation. GMP dynamic/native calls are not exposed. |
| BigFloat / MPFR surface | A | #9290 | Use the current pure-Rust `astro-float` based surface. MPFR exactness gaps are documented as parity work; switching to MPFR would be a Policy-B ADR. |
| Regex / PCRE2 surface | A | #8992 | Use pure-Rust regex/fancy-regex coverage. PCRE2 is not a general native dependency. |
| libm / transcendentals | A | #9570 | Keep math behavior in the shared Rust/VM surface. Do not rely on platform-native libm as the parity oracle without adding explicit CLI/WASM/iOS comparison gates. |
| LLVM intrinsic spellings | A | #9570 | Compiler-owned intrinsic boundary only; not a user-visible native call surface. |
| Random / dSFMT | C | #8998 | MersenneTwister/dSFMT bitstream parity is a documented permanent divergence; sjulia uses its own RNG surface. |
| LibGit2 / Pkg native boundary | C | #9570 | Full package-manager native integration is outside the current subset. |
| Dynamic loading (`Libdl`, `dlopen`) | C | #9570 | Incompatible with the shared WASM+iOS target requirement. |
| User native boundary syntax (`ccall`, `threadcall`) | C | #9570 | Unsupported by design under the shared WASM+iOS requirement. |
| Shared memory / process-level parallel surfaces | C | #9570 | Outside the single-threaded VM target unless a separate design narrows the semantics. |
| Documentation-only examples | C | #9570 | Not a shipped sjulia surface. |
| Upstream test-only ccall sites | C | #9570 | Not a shipped sjulia surface. |

## Rules For Future Changes

1. A new native-backed capability must update the ledger and this ADR before
   implementation lands.
2. Policy A must be implemented as Rust/pure-Rust/VM-owned behavior, with no
   target-specific hidden dynamic loading.
3. Policy B must include:
   - iOS static-library build evidence,
   - WASM build evidence,
   - size/runtime impact,
   - parity tests proving why Policy A is insufficient.
4. Policy C must fail clearly or document the intentional divergence. Silent
   upstream-looking stubs are bugs.
5. User-visible `ccall` remains unsupported by design. Upstream `ccall` sites
   may be mirrored only as explicit VM intrinsics or host wrappers.

## Relation To Existing Docs

- [`RUST_BOUNDARY_JUSTIFICATION.md`](./RUST_BOUNDARY_JUSTIFICATION.md) explains
  when Rust is the right implementation layer.
- [`AOT_NATIVE_CALLS.md`](./AOT_NATIVE_CALLS.md) records AoT's current `ccall`
  rejection contract.
- [`ADR_BACKEND_STRATEGY.md`](./ADR_BACKEND_STRATEGY.md) records that the VM,
  not AoT, is the shipped iOS/WASM runtime today.
