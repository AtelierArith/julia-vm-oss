# SubsetJuliaVM

SubsetJuliaVM is a Rust implementation that runs a static subset of Julia without a JIT. It parses Julia source with a pure Rust parser and executes it through a pipeline of lowering → Core IR → bytecode compiler → stack VM.

It also provides a C ABI for iOS and WebAssembly bindings for the web.

## Quick Start

To use `sjulia`, you need to embed caches into the binary. It works without caches, but cold start is very slow, so caches are practically required.

```bash
$ ./scripts/sjulia_install.sh   # Windows: pwsh -File scripts/sjulia_install.ps1
$sjulia examples/mandelbrot.jl
  0.027875 seconds
Mandelbrot Set (50x25):

                              .
                              . .
                              .+
                             ###+.
                        .   .####.
                       .#++#########....
                      ..##############.
            .        ..################..
             ...... .##################.
            .#######.###################
          ...##########################.
#####################################..
          ...##########################.
            .#######.###################
             ...... .##################.
            .        ..################..
                      ..##############.
                       .#++#########....
                        .   .####.
                             ###+.
                              .+
                              . .
                              .


```

## AoT Compile Mandelbrot

To compile `examples/mandelbrot.jl` ahead of time into a native binary, build
`juliars` with the AoT feature and use the minimal AoT prelude:

```bash
$ cargo build --release -p subset_julia_vm --features aot --bin juliars
$ target/release/juliars --minimal-prelude examples/mandelbrot.jl \
    --emit-binary target/mandelbrot_aot
$ target/mandelbrot_aot
  9.4e-5 seconds
Mandelbrot Set (50x25):

                              .
                              . .
                              .+
                             ###+.
                        .   .####.
                       .#++#########....
                      ..##############.
            .        ..################..
             ...... .##################.
            .#######.###################
          ...##########################.
#####################################..
          ...##########################.
            .#######.###################
             ...... .##################.
            .        ..################..
                      ..##############.
                       .#++#########....
                        .   .####.
                             ###+.
                              .+
                              . .
                              .

```

### Windows

Build `juliars` from PowerShell with the same Cargo command. Windows executable
names use the `.exe` suffix:

```powershell
cargo build --release -p subset_julia_vm --features aot --bin juliars
.\target\release\juliars.exe --minimal-prelude examples\mandelbrot.jl --emit-binary target\mandelbrot_aot.exe
.\target\mandelbrot_aot.exe
```

Rust and the MSVC build tools must be installed. Run the commands from a
Developer PowerShell for Visual Studio so that the MSVC linker and runtime
libraries are available. The Cranelift backend also supports Windows/MSVC and
looks for `link.exe` or `lld-link` when linking native artifacts.

The default Rust backend currently builds the generated executable but fails to
copy it to the `--emit-binary` destination on Windows because it looks for the
temporary executable without its `.exe` suffix. Until that is fixed, generating
Rust source with `-o output.rs` remains available, while the complete
`--emit-binary` command above is expected to fail during the final copy step
(Issue [#11809](https://github.com/AtelierArith/ailujsoi/issues/11809)).

`--minimal-prelude` is currently required for this example. The full prelude
still includes Base paths that reach unsupported AoT `BigInt`/parametric
constructor code, even though the Mandelbrot program itself can compile through
the minimal AoT prelude.

For the full set of AoT options (Cranelift backend, object/library output, C ABI exports, etc.), see [`docs/CLI.md`](docs/CLI.md) or run `juliars --help`.

## Building the WebAssembly Package

Build the WASM package for the Web Playground as follows.

```bash
$ scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg
$ cd web
$ python3 server.py
Serving at http://localhost:8080
```

This script generates and embeds the prelude/Base caches, then builds `subset_julia_vm_web` with `wasm-pack`. See [`docs/CLI.md`](docs/CLI.md) for details.

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
