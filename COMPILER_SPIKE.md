# General Wasm AoT Backend Spike

Date: 2026-08-18

## Decision

**GO** for the next integration slice. The backend compiles general supported
AoT IR rather than recognizing transforms, executes standalone in Node, and the
888×862 RGBA p95 is below the 100 ms gate. This is experimental and has no
dynamic-dispatch or unsupported-IR fallback.

## Browser compiler package

`subset_julia_vm_web` now forwards its opt-in `aot-wasm` feature to
`subset_julia_vm/aot-wasm` and exports:

```typescript
compile_to_wasm(source: string, options?: CompileOptions): CompileToWasmResult
```

The result contains `success`, `wasm_bytes: Uint8Array`, typed diagnostics,
compiler version, generated-module ABI version, and these exact millisecond
timings: `source_parse_lower_ms`, `dead_code_elimination_ms`,
`type_inference_ms`, `ir_conversion_ms`, `optimization_ms`,
`wasm_ir_lowering_ms`, `wasm_codegen_ms`, and their `total_ms` sum. The byte
field uses `serde_bytes` through `serde-wasm-bindgen`, so JavaScript receives a
copied `Uint8Array`, not a number array.

Options accept `source_name`, optimization level 0 through 3, and explicit
exports (`export_name`, `function_name`, and Julia `arg_types`). Export requests
reuse `CAbiExport` and `StaticType`; compilation calls `compile_wasm_source`
once with `CompileConfig.backend = AotBackend::Wasm`. No compiler stage or
transform is duplicated in the binding. When explicit exports are requested,
the generated module exposes only the requested `export_name` aliases plus
`memory` and `__sjulia_wasm_abi_version`; internal direct calls continue to use
function indices and do not depend on public names. With no explicit requests,
the backend preserves the spike's default of exporting all unique functions.

The boundary rejects source above 1 MiB and output modules above 16 MiB.
Invalid options, parser/lowering failures, unsupported AoT instructions,
inference/codegen failures, and limit failures return typed diagnostics rather
than panicking. Parser/lowering and unsupported-instruction spans retain byte
and one-indexed line/column locations.

The corrected distributable web-target package is `pkg-compiler-final/`. It was built with
Rust 1.95.0, wasm-pack 0.15.0, and the workspace `web-release` profile. The
compiler Wasm size and digest are recorded in
`pkg-compiler-final/ARTIFACT_MANIFEST.json`, which records
tool versions, generated-module ABI v1, artifact sizes, and SHA-256 hashes. The
packaged artifact remains v1 until its separate regeneration slice; source-built
generated modules now use ABI v2. This source snapshot has no Git metadata, so
its `source_commit` is explicitly
`unavailable_non_git_source_tree` rather than an invented commit.

AoT timing originally used `std::time::Instant`, which panics under
`wasm32-unknown-unknown`. The shared AoT timer now uses native `Instant` on host
targets and `performance.now()` in compiler Wasm (with `Date.now()` fallback),
preserving the same `Duration`-based timing result and seven phase names.

## RED / GREEN

RED was added first in the existing `aot_e2e_tests` binary. The focused compile
failed because `compile_wasm` and `AotBackend::Wasm` did not exist. Full output
was captured temporarily at
`/var/folders/dk/l8npmj551dd_6ydk8858ckkc0000gn/T/opencode/sjulia-wasm-red.log`.

```text
error[E0432]: unresolved import `subset_julia_vm::aot::compile_wasm`
error[E0599]: no variant or associated item named `Wasm` found for enum `AotBackend`
```

GREEN command (`cargo-nextest` is unavailable in this extracted environment):

```bash
cargo test --profile release-fast -p subset_julia_vm \
  --features aot-wasm --test aot_e2e_tests wasm_backend_tests -- --nocapture
```

Result: 18 passed (the original 12 plus six adversarial tests). Node
`WebAssembly.compile` validates every executable case and
`WebAssembly.instantiate(module, {})` proves there are no imports.

## Pipeline and supported IR

The public input is the existing lowered `Program`. `compile_wasm` reuses
dead-code elimination, `TypeInferenceEngine`, `program_to_aot_ir`, optimizer,
and pass diagnostics before lowering to backend-neutral `IrModule`.

Supported: i64/f64/bool/u8 constants and locals; copy/conversion; unary negate,
not, bit-not; integer/float arithmetic and comparisons; bitwise/shift; IR blocks,
jump/branch/return, phi edge copies; direct calls; arbitrary-rank statically typed
UInt8 descriptor length, load, and store. Unsupported high-level expressions and
low-level instructions
return `UnsupportedInstructionDiagnostic`; source spans are retained where the
upstream AoT conversion still provides them, while backend-only IR has no span.
Phi inputs are staged in typed scratch locals before destination writes, so edge
copies have SSA parallel-assignment semantics even for cycles.

UInt8 uses a normalized i32 carrier: arithmetic and bitwise results are masked
to eight bits, relational comparison/division/remainder/right shift are unsigned,
and widening to Int64 zero-extends. A retained final `ValueCarrier` in a typed
function lowers to `Return`; if prior AoT passes do not retain an unambiguous
carrier, Wasm lowering returns `UnsupportedInstructionDiagnostic` instead of
emitting a self-jump. Duplicate/overloaded internal names and the reserved names
`memory` and `__sjulia_wasm_abi_version` are rejected before function indices
are built, so the compiler never returns an invalid module for those cases.

## Linear-memory descriptor ABI v2

The module exports `memory` and `__sjulia_wasm_abi_version`. A descriptor has a
40-byte, 8-byte-aligned little-endian header followed immediately by one
16-byte `{dim:u64, stride:i64}` pair per axis:

| Offset | Field | ABI v2 contract |
|---:|---|---|
| 0 | `abi_version:u32` | `2` |
| 4 | `flags:u32` | `MODULE_OWNED=1`, `READONLY=2`; no other bits |
| 8 | `element_tag:u32` | stable append-only table; UInt8 is `1` |
| 12 | `element_size:u32` | mirror of tag-derived size; UInt8 is `1` |
| 16 | `layout_id:u32` | `0` until generic isbits layouts land |
| 20 | `rank:u32` | expected static rank, at most `8` |
| 24 | `data_ptr:u32` | tag-aligned; zero only for zero elements |
| 28 | `reserved:u32` | `0` |
| 32 | `element_count:u64` | checked product of inline dimensions |
| 40+16k | `dim[k]:u64` | axis length, inclusive maximum `2^31` |
| 48+16k | `stride[k]:i64` | nonnegative stride in element units; zero aliases one element |

Julia indexing is one-based per axis. Validation is fail-closed before every
length/load/store: pointer/header alignment and range, ABI/flags/reserved,
inline metadata extent, expected tag/size/layout/rank, checked dimension product,
rank-0 and zero-count rules, nonnegative strides, maximum address/data extent,
and metadata/data disjointness must all hold. Addressing computes
`sum((index-1)*stride)` in widened arithmetic and wraps to i32 only after proving
the byte address lies in current memory. Each dimension is at most `2^31`, so a
single-axis length remains a nonnegative Julia `Int64`; `2^31` is accepted and
`2^31+1` traps. Stride zero is intentionally valid for aliasing views, where
multiple logical indices address the same element. `MODULE_OWNED` controls data
lifetime only and does not currently imply canonical strides. Negative strides
and canonical-stride enforcement are deferred to the general array work in Todo
5. The host owns allocation in this slice.

## Coverage

Actual Julia parse→lower→AoT→IR→Wasm→Node cases cover integer arithmetic,
Float64 conditional, counted loop, direct helper call, UInt8 mutation, and RGBA
invert preserving alpha. Upstream Julia 1.12.4 was run directly for all six
programs and returned, respectively, `42`, `5.5`, `45`, `42`,
`4:2,3,255,1`, and `245,235,225,40,155,105,55,250`, exactly matching Node.
Additional tests prove cyclic phi copies remain parallel and malformed ABI,
flags, tags, sizes, rank, inline shapes, strides, extents, overlap, and pointer
ranges trap before data mutation.
Adversarial parity cases additionally pin UInt8 subtraction/addition wrapping,
unsigned UInt8-to-Int64 widening (`255`), bounded implicit-tail handling,
duplicate/reserved symbol diagnostics, and alias-only exports. Upstream Julia
1.12.4 returned `true`, `false`, `255`, and `42` for the four semantic programs.

## Timings

Observed on this development machine for the RGBA source:

```text
source-parse-lower_ms=0.906
type-inference_ms=0.764
ir-conversion_ms=0.161
optimization_ms=0.911
wasm-ir-lowering_ms=0.012
wasm-codegen_ms=0.030
WebAssembly.compile_ms=0.508
instantiate_ms=0.042
20 warm iterations, 888x862 RGBA: median_ms=5.753 p95_ms=6.244
```

Run the host benchmark with:

```bash
node benchmarks/wasm_aot_rgba.mjs path/to/module.wasm
```

The corrected compiler Wasm is 25,540,486 bytes. Compiler-in-Wasm smoke
measurements from Node v23.7.0 were 20.52 ms total for aliased typed Int64
arithmetic and 4.41 ms total for the UInt8 mutation loop. Generated modules were
152 and 612 bytes respectively.

## Validation commands

```bash
cargo check -p subset_julia_vm --features aot-wasm --tests
cargo fmt --check
bash scripts/test_aot.sh
bash scripts/run_clippy_lanes.sh aot-wasm
wasm-pack build --target web --profile web-release --out-dir ../pkg-compiler-final --locked -- --features aot-wasm
node scripts/compiler_wasm_smoke.mjs
```

The package smoke initializes the compiler Wasm, compiles aliased arithmetic and generic
UInt8 mutation sources, checks `wasm_bytes instanceof Uint8Array`, runs
`WebAssembly.validate`, `WebAssembly.compile`, and import-free
`WebAssembly.instantiate`, proves the requested arithmetic alias exists while
the internal name is not exported, executes both exports, and checks parse, unsupported,
and source-limit diagnostics. The mutation case transforms `[1, 2, 254, 0]` to
`[2, 3, 255, 1]` through the versioned descriptor ABI.

Local environment blockers: `cargo-nextest` is not installed. Rust 1.95.0,
its Cargo, Node v23.7.0, Julia 1.12.4, wasm-pack 0.15.0, and `wasm-tools` are
available; direct Cargo fallbacks are used where nextest is unavailable.

For this extracted compiler tree, existing `subset_julia_vm_web` native tests
also cannot compile because their unrelated sample-parity block uses
`include_str!` paths into an absent sibling `SubsetJuliaVMApp` directory. The
new compiler tests were confirmed RED on the missing API before implementation;
production library checks and the packaged compiler E2E do not depend on those
missing sample assets.
