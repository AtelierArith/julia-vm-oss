# JuliaVM OSS

## Overview

**JuliaVM OSS** (v0.6.5) is a minimal, fully static pipeline for executing a Julia subset. It uses a four-stage pipeline (Parser → Lowering → Compiler → VM) that accepts Julia syntax, determines feature support during lowering, compiles Core IR into bytecode, and executes bytecode in a deterministic stack-based VM. No JIT—fully App Store compliant.

**Pure Rust Implementation**: The entire pipeline is implemented in pure Rust with no C dependencies, making it fully portable across all platforms including iOS, macOS, Linux, WebAssembly, and Android.

The `subset_julia_vm` crate aims to implement a subset of upstream Julia itself, not a separate dialect.

## Architecture

```
Julia source (any valid syntax)
    ↓
Parser (Pure Rust) → CST with spans
    ↓
Lowering (CST → Core IR) → Supported: Core IR | Unsupported: Error with hint
    ↓
Compiler (Core IR → Bytecode)
    ↓
VM (executes bytecode) → Results or RuntimeError
```

**Key Design Principles**
- **No JIT** (entirely static and App Store–safe)
- **Parser accepts ALL Julia syntax** (rejection happens at lowering)
- **Span-aware errors** with helpful hints
- **Deterministic execution** with seedable PRNG
- **Clear separation**: Parser → Lowering → Compiler → VM → FFI

### Layer-by-Layer Breakdown

#### 1. **Parser Layer** (Never Fails)
- Parses any valid Julia syntax using the pure Rust parser
- No C dependencies - fully portable across all platforms
- Produces a **Concrete Syntax Tree (CST)** with source spans
- Accepts everything: macros, modules, `using`, `eval`, etc.
- **Key principle**: The parser never rejects code—it always produces a tree

#### 2. **Lowering Layer** (The Gatekeeper)
- Converts CST nodes to **Core IR** (our minimal representation)
- **Supported features** → Core IR nodes (Block, If, While, For, Call, etc.)
- **Unsupported features** → `UnsupportedFeature` error with:
  - Source span (exact line/column location)
  - Error kind (e.g., "MacroNotSupported", "TypeNotSupported")
  - Helpful hint message
- This is where we decide: "Can this code run on our VM?"

#### 3. **Compiler Layer** (IR → Bytecode)
- Transforms Core IR into **stack-based bytecode**
- Instruction set includes:
  - **Data**: `PushI64`, `PushF64`, `LoadI64`, `StoreI64`
  - **Arithmetic**: `AddI64`, `MulF64`, `DivF64`, `SqrtF64`
  - **Control**: `Jump`, `JumpIfZero`, `Call`, `Return`
  - **Arrays**: `ArrayNew`, `ArrayGet`, `ArraySet`, `ArrayPush`
  - **Matrix**: `MatMul` (matrix-vector, matrix-matrix)
- Optimizes local variable access and control flow

#### 4. **VM Layer** (Stack-based Interpreter)
- **No JIT**—pure interpreter executing bytecode sequentially
- **Typed value stack**: I64, F64, Str, Array, Complex, Nothing
- **Deterministic execution**: Uses seedable PRNG (currently StableRNG/LehmerRNG, Xoshiro256++ also supported)
- **Output capture**: Intercepts `println()` calls and captures to string
- **Error handling**: Runtime errors (division by zero, index out of bounds, etc.)

## Running the CLI REPL

The JuliaVM OSS includes an interactive Julia-like REPL for terminal use.

### Installing sjulia CLI

Install the `sjulia` command globally using `cargo install`:

```sh
# From the repository root
cargo install --path subset_julia_vm --features repl
```

This installs `sjulia` to `~/.cargo/bin/`. Ensure this directory is in your `PATH`.

After installation, you can run `sjulia` from anywhere:

```sh
sjulia                    # Start interactive REPL
sjulia path/to/file.jl    # Execute a Julia file
sjulia -e "println(1+1)"  # Execute code string
sjulia --precompile-base out.bin  # Generate Base cache for embedding
```

### Running without Installation

```sh
cd subset_julia_vm

# Run the REPL
cargo run --bin sjulia --features repl --release
```

### REPL Features

```
   ╔═╗╔═╗╔╦╗╔═╗╔═╗╔═╗╔╦╗╔═╗  (Colorful Monokai logo!)
   ║ ╦║ ║║║║╠═╣║ ╦║ ║║║║╠═╣
   ╚═╝╚═╝╩ ╩╩ ╩╚═╝╚═╝╩ ╩╩ ╩
   ╦╔═╦ ╦╦ ╦╦╔═╦╔═╦ ╦╦ ╦
   ╠╩╗╚╦╝║ ║╠╩╗╠╩╗╚╦╝║ ║
   ╩ ╩ ╩ ╚═╝╩ ╩╩ ╩ ╩ ╚═╝

julia> x = 10
10

julia> x * 2
20

julia> \alpha<Tab>    # LaTeX completion → α
julia> α = 3.14
3.14
```

**Commands:**
- `exit()`, `quit()` - Exit the REPL
- `reset()` - Clear all variables and definitions
- `vars()`, `whos()` - Show defined variables
- `?`, `help()` - Show help

**Keyboard Shortcuts:**
- `Tab` - Insert 4 spaces, or complete LaTeX (e.g., `\alpha` → `α`)
- `Up/Down` - Navigate history
- `Ctrl-C` - Cancel current input
- `Ctrl-D` - Exit

## Precompiled Base Cache (Fast Startup)

By default, `sjulia` compiles ~1,958 Base functions at startup (~420ms). You can precompile these at build time and embed the bytecode cache to reduce startup to ~20ms.

### Two-Pass Build Workflow

```bash
# Step 1: Build sjulia normally
cargo build --release --bin sjulia --features repl

# Step 2: Generate precompiled Base cache (~3 MB)
./target/release/sjulia --precompile-base target/base_cache.bin

# Step 3: Rebuild with embedded cache
SJULIA_BASE_CACHE=$(pwd)/target/base_cache.bin \
  cargo build --release --bin sjulia --features repl
```

The resulting binary contains the embedded bytecode and skips Base compilation entirely. Default builds (without `SJULIA_BASE_CACHE`) work exactly as before.

## Building for WebAssembly

See [subset_julia_vm_web/README.md](subset_julia_vm_web/README.md) to learn more.

## Ahead-of-Time (AoT) Compilation

The JuliaVM OSS includes an AoT compiler that compiles Julia code to native Rust code for maximum performance.

### AoT Compilation Workflow

```bash
# Build with AoT feature
cargo build --release --features aot

# Compile Julia to Rust
cargo run --release --bin aot --features aot -- path/to/file.jl -o output.rs

# Options:
#   --minimal-prelude    Use minimal typed prelude (cleaner code)
#   --emit-comments      Include source comments in generated Rust
#   --pure-rust          Generate standalone Rust (no runtime dependency)

# Compile the generated Rust
rustc -O output.rs -o output_binary \
    --extern subset_julia_vm_runtime="target/release/libsubset_julia_vm_runtime.rlib" \
    -L target/release/deps

# Run the binary
./output_binary
```
