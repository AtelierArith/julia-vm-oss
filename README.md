# SubsetJuliaVM

SubsetJuliaVM is a Rust implementation that runs a static subset of Julia without a JIT. It parses Julia source with a pure Rust parser and executes it through a pipeline of lowering → Core IR → bytecode compiler → stack VM.

It also provides a C ABI for iOS and WebAssembly bindings for the web.

## Quick Start

To use `sjulia`, you need to embed caches into the binary. It works without caches, but cold start is very slow, so caches are practically required.

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
mkdir -p target
./target/release/sjulia --precompile-prelude target/prelude_program_cache.bin
./target/release/sjulia --precompile-base target/base_cache.bin

SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
```

After the second build, `target/release/sjulia` is the binary with embedded caches.

```bash
./target/release/sjulia path/to/file.jl
./target/release/sjulia -e 'println(1 + 2)'
```

## Building the WebAssembly Package

Build the WASM package for the Web Playground as follows.

```bash
scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg
```

This script generates and embeds the prelude/Base caches, then builds `subset_julia_vm_web` with `wasm-pack`. See [`docs/CLI.md`](docs/CLI.md) for details.

## What is SubsetJuliaVM

- **Static pipeline**: Julia source → parser → lowering → Core IR → bytecode compiler → stack VM
- **No JIT**: Runs on environments such as iOS where JIT is unavailable
- **Aiming for Julia compatibility**: See `subset_julia_vm/tests/fixtures/`, `subset_julia_vm/src/julia/`, `docs/vm/STATUS.md`, and `docs/vm/UNIMPLEMENTED.md` for the supported scope

The workspace is composed of the following crates.

```text
subset_julia_vm/          Core VM, compiler, lowering, CLI
subset_julia_vm_ffi/      C ABI staticlib/cdylib (iOS / native)
subset_julia_vm_parser/   pure Rust Julia parser
subset_julia_vm_web/      wasm-bindgen bindings
subset_julia_vm_runtime/  runtime crate for AoT compiled code
```

## Documentation

- [`docs/CLI.md`](docs/CLI.md): detailed CLI reference, cache instructions, Rust API, C ABI / iOS, WebAssembly, tests, audits
- [`docs/vm/`](docs/vm/): architecture, implementation status, testing and audit policies, etc.

Key documents:

- `docs/vm/ARCHITECTURE_OVERVIEW.md`
- `docs/vm/CHECKLISTS.md`
- `docs/vm/TESTING_GUIDE.md`
- `docs/vm/STATUS.md`
- `docs/vm/DONE.md`
- `docs/vm/UNIMPLEMENTED.md`
- `docs/vm/PURE_JULIA_DESIGN.md`
- `docs/vm/WORKAROUNDS.md`
- `docs/vm/CODE_AUDITS.md`
