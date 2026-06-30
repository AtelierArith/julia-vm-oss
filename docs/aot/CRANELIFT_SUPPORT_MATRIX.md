# Cranelift Backend Support Matrix

**Last updated**: 2026-06-24

This document tracks the Cranelift-specific `juliars --backend cranelift`
surface. The general AoT matrix in [SUPPORT_MATRIX.md](./SUPPORT_MATRIX.md)
describes the Rust backend, which remains the stable `juliars` backend. This
file is intentionally narrower: it records what the Cranelift backend can lower
today, what is deliberately gated, and which milestone issues own the remaining
JuliaC.jl-replacement work (Issue #7130).

Legend:

- `supported`: expected to compile in the Cranelift backend today.
- `partial`: implemented for a restricted scalar/static subset.
- `gate`: rejected intentionally with a diagnostic to avoid silent mismatch.
- `planned`: tracked by an open milestone issue.

## CLI and Build Surface

| Surface | State | Notes |
|---|---:|---|
| `--backend cranelift` | partial | Reaches the experimental backend when `subset_julia_vm` is built with the `cranelift` feature. Feature-less builds emit a rebuild diagnostic (Issue #6927). |
| `--check` / `-o -` | partial | Useful for probing the Cranelift lowering path without claiming standalone binary output. |
| `--jit-run --backend cranelift` | partial | Opt-in desktop JIT execution compiles the Cranelift module in-process and calls `__juliars_main`; unsupported runtime/printing features remain explicit gates (Issue #7131). |
| `--emit-object --backend cranelift` | partial | Emits a relocatable object file through `cranelift-object::ObjectModule` for the currently supported scalar subset. `--target` selects the Cranelift ISA/object triple when provided, with ELF/Mach-O/COFF smoke coverage. Linking is factored into the reusable system-linker driver, while executable packaging remains separate (Issues #7082, #7087, #7088, #7089). |
| `--emit-binary --backend cranelift` | partial | Emits a Cranelift object into a temporary file, then invokes the reusable system linker driver to produce a native executable. This currently covers the scalar/object subset; cross-target linker availability and richer runtime startup remain platform/toolchain dependent (Issues #7081, #7083). |
| `--emit-library --backend cranelift` | partial | Emits the Cranelift object as a static archive (`--library-kind static`, default) or links it through the system linker driver as a shared library (`--library-kind shared`). This currently covers the scalar/object subset and depends on host/cross linker availability (Issue #7085). |
| `--target` with Cranelift native artifact output | partial | `--emit-object`, `--emit-binary`, and `--emit-library` with `--backend cranelift --target <triple>` select the Cranelift target ISA/object format when Cranelift supports that triple. Binary/shared-library packaging also uses the target to select the linker family. |
| `--export-c-abi` with Cranelift object/library output | partial | Cranelift object, static library, and shared library output can carry C-stable scalar/Nothing export symbols as object-level wrappers. Runtime `Value`, aggregate ABI exports, and platform-specific export-map controls remain gated (Issue #7086). |
| Optimization level mapping | supported | `-O0` maps to Cranelift `opt_level=none`, `-O1` / `-O2` map to `speed`, and `-O3` maps to `speed_and_size` (Issue #7091). |
| Desktop opt-in JIT mode | partial | `juliars --backend cranelift --jit-run` is an explicit in-process desktop JIT execution path, separate from object/binary output (Issue #7131). |

## Generated Artifacts

| Artifact | State | Notes |
|---|---:|---|
| In-process JIT function compilation | partial | Implemented through `cranelift-jit::JITModule` for the currently supported low-level IR subset. |
| Object file output | partial | Implemented through `cranelift-object::ObjectModule` for the currently supported scalar low-level IR subset, with explicit target triple selection for object emission. |
| Standalone executable | partial | Cranelift lowering emits a C ABI `main() -> Int32` wrapper that calls `__juliars_main` and returns zero. `--emit-binary --backend cranelift` packages the object through the system linker driver for supported local toolchains (Issues #7083, #7084). |
| System linker / lld integration | partial | `aot::linker` plans and runs platform linker invocations for C-driver, Unix `ld`/`ld.lld`, and MSVC `link.exe`/`lld-link` paths, including object/runtime/system-library ordering. Cranelift binary and shared-library output reuse this linker boundary (Issues #7083, #7085, #7089). |
| Static/shared library output | partial | `--emit-library --backend cranelift` archives `.a` output with `ar crs` and links shared libraries through `aot::linker` (`.so` / `.dylib` / `.dll`, depending on target/toolchain). Exportable entry points use the existing Cranelift C ABI wrapper surface; runtime-shaped exports remain gated (Issue #7085). |
| ELF / Mach-O / COFF coverage | partial | Representative `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, and `x86_64-pc-windows-msvc` triples emit ELF, Mach-O, and COFF object headers through `cranelift-object` (Issue #7088). |
| DWARF debug info | partial | `--debug-info` on Cranelift native artifact output emits DWARF compile-unit, subprogram, and line sections with function-level source lines from Core spans. Per-instruction span precision still requires span-carrying AoT/low-level IR (Issue #7090). |

## Native Scalar Types

| Julia/AoT carrier | State | Notes |
|---|---:|---|
| `Int8/16/32/64`, `UInt8/16/32/64` | partial | Native Cranelift integer carriers exist for scalar lowering. Completed milestone parity work covers wrapping arithmetic, division/remainder semantics, and Bool-as-integer behavior. |
| `Float32`, `Float64` | partial | Native float carriers exist. Completed milestone parity work covers numeric conversions, printing gates, and NaN comparison semantics. |
| `Bool` | supported | Lowered as a native scalar carrier while preserving Bool result type and mixed Bool/numeric operand behavior. |
| `Nothing` | partial | Native no-value carrier is accepted where the low-level signature can represent it. |
| `Char` | partial | Lowered as an `i32` codepoint scalar for literals, locals, parameters, returns, and static calls. Display/runtime formatting remains gated separately. |
| `Int128`, `UInt128` | partial | Lowered through Cranelift `I128`; x64 JIT enables LLVM ABI extensions for i128 args/returns (Issue #7092). |
| `Float16` | partial | Lowered with the existing AoT widened `F32` carrier, matching the Rust backend projection; Float16-specific literal/rounding/conversion parity remains outside the Cranelift scalar carrier gate (Issue #7093). |
| `Missing` | gate | Preserved in AoT types but not represented by the current low-level Cranelift scalar path. |

## Control Flow and Scalar Operations

| Feature | State | Notes |
|---|---:|---|
| Straight-line scalar expressions | partial | Supported for the low-level IR operations currently mapped in `aot::codegen::cranelift`. |
| `if` / branches / loops | partial | Completed milestone work covers CFG loop/back-edge handling, phi nodes, switch lowering, and nested `break` / `continue` targets. |
| Comparisons | partial | Integer/Bool/Float scalar comparisons are supported in covered forms; NaN comparison parity is fixed by Issue #7124. |
| Integer `+`, `-`, `*`, `div`, `rem` | partial | Native arithmetic is supported in covered low-level forms; signed/unsigned and zero-divisor behavior has regression coverage. |
| Bit operations and shifts | partial | Integer `&` / `|` / `xor`, `~`, and same-width/mixed-count `<<` / `>>` are lowered for supported scalar widths; unsigned right shift uses logical fill and signed right shift uses sign fill (Issue #7120). |
| Short-circuit `&&` / `||` | partial | Bool `&&` / `||` are lowered as branch-preserving CFG with a join phi, so RHS lowering is reached only on the Julia short-circuit path (Issue #7115). |
| libm math builtins | partial | `pow`/`powf`, `fmod`/`fmodf`, and unary `sqrt` / `sin` / `cos` / `exp` / `log` / `abs` are declared and lowered for supported scalar paths, with Float16 routed through the `*f` F32-family symbols (Issues #7122, #7093). |
| Runtime-checked conversions/calls | gate | Unsupported runtime-checked sites are rejected instead of placeholder-lowered, following Issue #7111. |

## Aggregate, Heap, and Runtime Value Boundaries

| Feature | State | Notes |
|---|---:|---|
| Runtime `Value`, `Any`, multi-variant `Union` | gate | Runtime boundary contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): `Any` and multi-variant `Union` use opaque GC-managed `SjuliaValue*` handles, boxing/unboxing helpers, runtime tag checks, and root slots across safepoints. Codegen remains gated until helper/runtime implementations are connected (Issues #7080, #7102). |
| Rooting / safepoint contract | gate | The Cranelift GC/rooting contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): scalar/native stack aggregates and read-only data pointers need no roots, while managed runtime pointers require an explicit `SjuliaGcContext`, root slots, and safepoints. Heap-shaped or runtime-value carriers remain rejected until runtime helper implementations land (Issues #7080, #7104). |
| Heap allocation hooks | gate | Allocation hook ABI is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): `__sjulia_gc_alloc`, `__sjulia_array_alloc`, and `__sjulia_string_alloc` are C-ABI imports and allocating safepoints. Emitting calls remains gated until runtime symbol binding and status-based exception failure paths land (Issues #7105, #7108). |
| Stack maps / precise safepoints | gate | Safepoint metadata contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): each safepoint has a function-scoped ID, live managed values must be in root slots, and managed-value lowering is allowed only with emitted stack maps or the explicit root-stack fallback. Heap paths remain gated until ownership/runtime-value/array follow-ups enable managed values (Issue #7106). |
| String / Array ownership model | gate | Ownership contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): heap strings and arrays are GC-managed pointer handles, handle copies are non-owning reference copies, borrowed byte/buffer pointers cannot cross safepoints, and array mutation needs rooted handles plus future write barriers. Heap string lowering remains gated; Array/Vector layout and memory-op lowering contract is fixed by Issue #7098. |
| String constants and values | partial | Local String literals lower to read-only length-prefixed data payloads and use pointer carriers inside Cranelift functions; `length(::String)` reads the payload length. String params/returns, mutating/allocating operations, and runtime `Value::Str` bridging remain gated (Issue #7094). |
| Arrays / Vectors | gate | Array/Vector layout and lowering contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): `SjuliaArray*` owns header metadata, shape, and data buffer; `length`, `size`, 1-based column-major indexing, allocation, and bounds failure paths are specified. Codegen remains gated until runtime helpers/bindings and write-barrier support are connected (Issue #7098). |
| Tuples | partial | Local tuple literals are split into scalar field carriers, constant tuple field access lowers to the selected scalar field, and tuple-returning scalar functions lower through Cranelift multiple returns. Tuple parameters and heap/runtime tuple objects remain gated (Issues #7097, #7117). |
| Multiple return / destructuring | partial | Tuple-returning scalar functions lower as Cranelift multi-result signatures, `ReturnMany`, and `CallMulti`, so destructuring via the existing temp tuple + constant index path can consume the results without heap tuple allocation. Out-param/runtime `Value` returns remain future runtime work (Issue #7117). |
| User structs | partial | Non-parametric scalar-field structs lower to stack slots with Julia-compatible field offsets for construction, field load, and mutable field store. Struct parameters/returns, nested/heap fields, parametric layouts, and runtime object identity remain gated (Issues #7079, #7095). |
| `Complex` | partial | Local `Complex` / `ComplexF64` / `Complex{Float64}` and `ComplexF32` / `Complex{Float32}` values lower as stack aggregate pairs with scalar `re` / `im` fields. `real`, `imag`, `abs2`, and same-element `+` / `-` / `*` lower to scalar field arithmetic. Complex params/returns, heap/runtime object identity, and non-Float element layouts remain gated (Issue #7099). |
| `@enum` | partial | Enum definitions are accepted as metadata, and member references lower to their Int32 backing constants for scalar Cranelift codegen. Display/runtime enum object parity remains outside the native scalar subset (Issues #7079, #7096). |
| Top-level globals | partial | Initialized scalar globals are lowered as read-only constants in the current JIT path. Heap-shaped initializers and non-scalar globals remain gated until the runtime/data-section bridge is available (Issues #7079, #7103). |

## Calls and Exceptions

| Feature | State | Notes |
|---|---:|---|
| Direct low-level function calls | partial | Supported when every parameter and return type is representable by the current Cranelift signature mapper. |
| Dynamic dispatch / unresolved calls | gate | Rejected rather than lowered to typed placeholders. |
| Varargs / kwargs | gate | Adapter contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): static splats may expand to fixed native signatures, true varargs use tuple packing, and keyword calls canonicalize through deterministic keyword adapter symbols. Codegen remains gated until adapter generation and runtime tuple/NamedTuple helpers are connected (Issue #7118). |
| `try` / `catch` / `throw` / unwinding | gate | Exception propagation contract is defined in [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md): Cranelift does not native-unwind across generated/Rust/C ABI frames, and throw-capable functions use an explicit `SjuliaCallStatus` plus pending exception state in `SjuliaGcContext`. Codegen remains gated until helper/runtime implementations are connected (Issue #7108). |

## Quality Gates

| Gate | State | Notes |
|---|---:|---|
| Cranelift IR verifier before compile | supported | Each generated Cranelift function is verified after `FunctionBuilder::finalize()` and before `define_function`, so invalid CLIF fails before native compilation (Issue #7125). |
| Differential tests against Rust backend and upstream Julia | partial | `scripts/aot_cranelift_fixture_differential.sh` exact-diffs stdout across upstream Julia, the Rust backend generated binary, and `juliars --backend cranelift --jit-run` for fixtures supported by all three paths (Issue #7126). |
| Property / fuzz tests for lowering | partial | Deterministic scalar AoT IR generation checks that the Rust backend accepts the same programs and Cranelift lowering reaches verifier/codegen without invalid CLIF (Issue #7128). |
| Compile-time/runtime benchmarks vs Rust backend | partial | `scripts/aot_cranelift_backend_benchmark.sh` reports Rust backend `--check`, Rust emit-binary/link time, generated binary runtime/size, Cranelift `--check`, and Cranelift `--jit-run` timings for the same fixture (Issue #7127). |
| Span-aware Cranelift diagnostics | partial | AoT-level Cranelift gates use `UnsupportedInstructionDiagnostic`; low-level `CraneliftError::Unsupported` now maps to `AotError::UnsupportedInstruction` with workaround text at the backend boundary. Full source spans require span-carrying AoT/low-level IR (Issue #7129). |

## Practical Roadmap

1. Stabilize diagnostics: Issue #7129.
2. Add the object backend path, native packaging, library driver, and initial
   DWARF sections for scalar programs: Issues #7082, #7089, #7084, #7083,
   #7085, and #7090.
3. Split desktop JIT and AoT artifact responsibilities: Issue #7131.
4. Expand scalar coverage while heap values remain gated: Issues #7120,
   #7101, #7092, #7093, #7103, and #7096.
5. Implement the runtime `Value` / GC / rooting contract before enabling heap
   strings, arrays, heap structs, and exceptions. The design contract,
   allocation hook ABI, safepoint metadata contract, ownership model, runtime
   value boundary, exception propagation contract, Array/Vector layout
   contract, and varargs/kwargs adapter contract are fixed by Issues #7104,
   #7105, #7106, #7107, #7102, #7108, #7098, and #7118. Remaining heap/call
   implementation work proceeds by connecting the runtime helper implementations
   and write barriers to these contracts.

This ordering keeps the current no-silent-mismatch policy intact: unsupported
Julia features should remain explicit gates until the Cranelift backend can
represent their Julia semantics and runtime obligations directly.
