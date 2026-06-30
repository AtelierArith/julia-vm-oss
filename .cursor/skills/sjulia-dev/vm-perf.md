# VM Performance & Bytecode

For VM/codegen performance work, prioritize VM execution improvements over AoT
unless the user explicitly asks for AoT. Keep the no-JIT iOS runtime viable.

## Dump bytecode before changing fast paths

Inspect slot types, slotized loads/stores, peephole results, and
direct/dynamic calls *before* editing runtime fast paths:

```bash
cargo run --bin sjulia --features repl -- --dump-bytecode <file.jl>
cargo run --bin sjulia --features repl -- --dump-bytecode -e 'f(x)=x+1; f(41)'
# Add --all when Base/prelude or generated helpers are relevant.
cargo run --bin sjulia --features repl -- --dump-bytecode --all <file.jl>
```

The default dump shows user functions plus a short main tail.

## Do not report cold CLI timing as a VM-only result

For CLI comparisons, build both baseline and current with the **same
precompiled caches** so the comparison isolates the changed code, not first-run
Base compilation. The embedded caches cover prelude/Base, not the user program
bytecode.

```bash
# Step 1: helper binary (NOT cache-embedded yet)
cargo build --release --bin sjulia --features repl

# Step 2: generate caches
mkdir -p target
./target/release/sjulia --precompile-prelude "$(pwd)/target/prelude_program_cache.bin"
./target/release/sjulia --precompile-base    "$(pwd)/target/base_cache.bin"

# Step 3: rebuild with both caches embedded — NOW target/release/sjulia is cache-embedded
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release --bin sjulia --features repl
```

`cargo build --release` alone does NOT re-link the binary — the `repl` feature
is required to refresh `target/release/sjulia` when re-testing pure-Julia
base/ changes.

## VM-only measurement

Prefer a `Vm::run()`-only Criterion harness that reuses a precompiled
`CompiledProgram`, and report CLI and VM-only numbers separately. Add the
benchmark to `benches/` (Issue #3210).

## AoT gate (only when touching the AoT pipeline)

The default test run uses the empty feature set, so `#[cfg(feature = "aot")]`
code is NOT built or exercised. After ANY change touching the AoT pipeline
(and periodically), run the AoT gate so codegen regressions don't slip through
(#6629/#5658 did exactly that — there is no PR CI):

```bash
bash scripts/test_aot.sh
# equivalently, by hand:
timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot --no-fail-fast
timeout 1800 cargo clippy -p subset_julia_vm --features aot --all-targets -- -D warnings
```

Note: nextest filters match on `binary test` (space-separated), not
`binary::test`; pass a bare test-function name.
