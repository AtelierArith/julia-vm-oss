# SubsetJuliaVM

**A JIT-free Julia interpreter for iOS with full App Store compliance**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-iOS%20%7C%20macOS%20%7C%20WASM-blue.svg)]()

SubsetJuliaVM executes Julia code on iOS devices, in web browsers, and as standalone executables—without JIT compilation. It uses a four-stage pipeline (Parser → Lowering → Compiler → VM) built entirely in Rust, with core library functions implemented in pure Julia.

```
Julia source → Parser (Pure Rust CST) → Lowering (feature gating) → Compiler (bytecode)
                                                                           ↓
Swift/iOS ← C ABI ← VM (stack-based interpreter) ← Bytecode
```

## Key Features

- **🚀 No JIT compilation** — Fully App Store compliant, runs on iOS devices
- **📦 Pure Rust** — No C dependencies, cross-platform (iOS, WASM, native)
- **📱 iOS native** — Swift FFI integration via C ABI
- **🌐 WASM support** — Runs in web browsers
- **🔢 Multiple dispatch** — Type-based method selection with `where` clauses
- **⚡ Standalone executables** — Bundle Julia code into standalone native binaries
- **🎨 Interactive REPL** — Full-featured Julia-like REPL with syntax highlighting
- **🎲 Deterministic RNG** — Seeded random numbers compatible with StableRNGs.jl
- **✅ Extensive testing** — 1,100+ fixture tests across ~90 categories

## Quick Start

### Install and Run

```bash
cd subset_julia_vm

# Build and run the REPL
cargo run --bin sjulia --features repl

# Execute a Julia file
cargo run --bin sjulia --features repl -- examples/hello.jl

# Execute an expression directly
cargo run --bin sjulia --features repl -- -e "println(sum([1, 2, 3, 4, 5]))"
```

### Try the REPL

```julia
julia> # Basic arithmetic
       x = 42
42

julia> # Arrays and iteration
       squares = [x^2 for x in 1:5]
[1.0, 4.0, 9.0, 16.0, 25.0]

julia> # Functions with recursion
       function fib(n)
           n <= 1 ? n : fib(n-1) + fib(n-2)
       end
       fib(10)
55

julia> # Complex numbers
       z = 3 + 4im
       abs(z)
5.0

julia> # Measure execution time
       @time sum([i^2 for i in 1:1000])
  0.000234 seconds
333833500.0
```

### Create Standalone Executables

Bundle your Julia code into a native executable (~2.2MB) with no external dependencies:

```bash
# Build the bundle tool
cargo build --bin bundle --features tempfile --release

# Bundle a Julia program
./target/release/bundle examples/hello.jl -o hello

# Run the standalone executable
./hello
# Output:
# Hello, World! from SubsetJuliaVM
# distance(p, q) = 5.0
```

The bundled executable embeds pre-compiled IR JSON and the VM runtime.

## Language Support

### ✅ Fully Supported

**Control Flow**
- `if/elseif/else`, `while`, `for` with ranges
- `break`, `continue`, `return`
- `try/catch/finally` error handling
- Ternary operator: `cond ? a : b`

**Functions & Dispatch**
- Multiple methods with type signatures
- Type parameters: `function f(x::T) where T <: Number`
- Lambda expressions: `x -> 2x`, `(x, y) -> x + y`
- Higher-order functions: `map`, `filter`, `reduce`, `foreach`
- Do syntax: `map(arr) do x; x^2 end`
- Recursion (with configurable stack limits)

**Data Structures**
- **Arrays**: 1D, 2D, N-D with indexing, slicing, broadcasting
- **Comprehensions**: `[x^2 for x in 1:10 if x % 2 == 0]`
- **Ranges**: `1:n`, `start:step:stop` (lazy iteration)
- **Tuples**: `(1, 2, 3)`, named tuples `(a=1, b=2)`
- **Dicts**: `Dict("key" => value)` with `get`, `haskey`, `keys`, `values`
- **Sets**: `Set([1, 2, 3])` with union, intersect, setdiff
- **Complex**: `1 + 2im`, full arithmetic and transcendental functions
- **Rational**: `1//2`, automatic simplification

**Types & Structs**
- Immutable structs: `struct Point x::Float64; y::Float64 end`
- Mutable structs: `mutable struct Counter count::Int64 end`
- Parametric types: `struct Point{T} x::T; y::T end`
- Abstract types: `abstract type Animal end`
- Union types: `Union{Int64, Float64}`
- Type dispatch with `Type{T}` patterns

**Operators**
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `^`
- Comparisons: `<`, `>`, `<=`, `>=`, `==`, `!=`
- Logical: `&&`, `||` (short-circuit), `!`
- Broadcast: `.+`, `.*`, `./`, `.^`, `f.(x)`
- Matrix multiply: `A * B`
- Implicit multiplication: `2x` ≡ `2 * x`

**Modules**
- Module definitions: `module Name ... end`
- Imports: `using Module`, `import Module: name`
- Exports: `export name1, name2`
- Relative imports: `using .MyModule`

**Advanced Features**
- String interpolation: `"value: $(x + 1)"`
- Let expressions: `let a = 1, b = 2; a + b end`
- Generator expressions: `(x^2 for x in 1:10)`
- Keyword arguments with defaults
- Varargs: `f(args...)`
- Kwargs splatting via `Base.Pairs`

**Macros**
- `@time expr` — Measure execution time
- `@assert cond ["msg"]` — Runtime assertions
- `@show expr` — Debug printing with expression
- `@test expr` — Unit testing (Test.jl compatible)
- `@testset "name" begin ... end` — Test grouping

### ❌ Not Supported (Intentional)

These features are excluded to maintain iOS App Store compliance:

- JIT compilation
- Arbitrary code generation (`eval`, `@generated`)
- C extensions / JLL packages
- Complex user-defined macros
- Package installation from registries
- `baremodule` definitions

## Built-in Functions

### Math Functions (140+ functions)

**Transcendental Functions**
```julia
sin, cos, tan, asin, acos, atan      # Trigonometric
sinh, cosh, tanh, asinh, acosh, atanh # Hyperbolic
exp, exp2, exp10, expm1               # Exponential
log, log2, log10, log1p               # Logarithmic
```

**Rounding & Number Theory**
```julia
floor, ceil, round, trunc             # Rounding
sqrt, cbrt, hypot                     # Roots
gcd, lcm, factorial, binomial         # Number theory
isqrt, powermod                       # Integer functions
```

**Special Functions**
```julia
abs, sign, clamp, mod, rem, div, fld
nextfloat, prevfloat, exponent, significand
fma, muladd                           # Fused operations
```

### Array Operations

**Construction**
```julia
zeros(n), ones(n), fill(v, n)         # 1D arrays
zeros(m, n), ones(m, n), fill(v, m, n) # 2D matrices
trues(n), falses(n)                   # Boolean arrays
similar(arr), copy(arr), reshape(arr, dims)
```

**Querying**
```julia
length(arr), size(arr), ndims(arr), eltype(arr)
```

**Mutation**
```julia
push!(arr, x), pop!(arr)              # End operations
pushfirst!(arr, x), popfirst!(arr)    # Start operations
append!(arr1, arr2), insert!(arr, i, x)
deleteat!(arr, i), reverse!(arr)
```

**Higher-Order Functions**
```julia
map(f, arr), filter(f, arr), reduce(op, arr)
foreach(f, arr), any(f, arr), all(f, arr), count(f, arr)
```

**Aggregation**
```julia
sum(arr), prod(arr), mean(arr)
minimum(arr), maximum(arr)
argmin(arr), argmax(arr)
```

**Sorting & Searching**
```julia
sort(arr), sort!(arr), sortperm(arr)
issorted(arr), searchsortedfirst(arr, x)
searchsortedlast(arr, x), insorted(x, arr)
```

**Set Operations**
```julia
unique(arr), union(a, b), intersect(a, b)
setdiff(a, b), issubset(a, b)
```

### Dictionary Operations

```julia
Dict("a" => 1, "b" => 2)              # Construction
haskey(dict, key), get(dict, key, default)
keys(dict), values(dict), pairs(dict)
delete!(dict, key), empty!(dict)
merge!(dict1, dict2)
```

### Statistics (Statistics.jl)

```julia
mean(arr), var(arr), std(arr)         # Basic statistics
median(arr), quantile(arr, p)         # Percentiles
cor(x, y), cov(x, y)                  # Correlation
```

### Random Numbers (Random.jl)

```julia
Random.seed!(123)                     # Set seed
rand(), rand(n), rand(m, n)           # Uniform [0, 1)
randn(), randn(n), randn(m, n)        # Standard normal
```

Deterministic RNG compatible with Julia's StableRNGs.jl.

### Linear Algebra (LinearAlgebra.jl)

```julia
A * B                                 # Matrix multiply
A * v                                 # Matrix-vector multiply
transpose(A), A'                      # Transpose
inv(A), det(A)                        # Inverse, determinant
```

### Complex & Rational Numbers

```julia
# Complex
complex(a, b), 1 + 2im
real(z), imag(z), conj(z)
abs(z), abs2(z), angle(z)

# Rational
1//2, 3//4
numerator(r), denominator(r)
```

### Type Introspection

```julia
typeof(x), isa(x, Type), supertype(T)
```

### I/O

```julia
println(args...), print(args...)
show(io, x), repr(x), string(x)
```

## Standard Library

SubsetJuliaVM includes standard library modules implemented in pure Julia:

| Module | Notes |
|--------|------|
| **Test** | `@test`, `@testset`, `@test_throws` |
| **Statistics** | `mean`, `var`, `std`, `median`, `cor`, `cov`, `quantile` |
| **Random** | Deterministic seeded RNG (StableRNGs.jl compatible) |
| **LinearAlgebra** | Matrix operations (e.g. `A * B`), `transpose`, `A'` |
| **Dates** | `Date`, `DateTime`, arithmetic, formatting |
| **InteractiveUtils** | Reflection utilities (`typeof`, `isa`, …) |
| **Iterators** | Iterator utilities |
| **Broadcast** | Broadcast implementation and utilities |
| **Printf** | `@sprintf`, `@printf` formatted output |
| **Base64** | Base64 encoding and decoding |

Usage:
```julia
using Statistics
println(mean([1, 2, 3, 4, 5]))  # 3.0

using Random
Random.seed!(42)
println(rand())  # Deterministic output

using LinearAlgebra
A = [1 2; 3 4]
println(det(A))  # -2.0
```

## Build Instructions

### Native (macOS/Linux)

```bash
cd subset_julia_vm

# Development build
cargo build

# Optimized release build
cargo build --release

# Run tests
timeout 180 cargo test

# Run fixture tests (cross-platform compatibility)
timeout 180 cargo test --test fixture_tests
```

### iOS

```bash
# Add iOS targets (one-time setup)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Build for device (from workspace root; outputs libsubset_julia_vm.a via subset_julia_vm_ffi)
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios

# Build for simulator
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim

# Create XCFramework for Xcode integration
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libsubset_julia_vm.a -headers ../subset_julia_vm_ffi/include \
  -library target/aarch64-apple-ios-sim/release/libsubset_julia_vm.a -headers ../subset_julia_vm_ffi/include \
  -output SubsetJuliaVM.xcframework
```

### WebAssembly

```bash
cd subset_julia_vm_web

# Install wasm-pack (one-time)
cargo install wasm-pack

# Build for web
wasm-pack build --target web --profile web-release
```

#### WebAssembly build with embedded Base/prelude caches (faster first run)

The browser Playground cannot use the native persistent on-disk caches. To skip
Base bytecode compilation and prelude source parse/lower on first execution,
generate Base and prelude Program caches on the host and embed them into the
`.wasm` via `SJULIA_BASE_CACHE` and `SJULIA_PRELUDE_PROGRAM_CACHE` (Issues
#2929, #6026).

```bash
# Helper script (from repo root):
scripts/wasm_build_with_cache.sh                                  # default: --target web
scripts/wasm_build_with_cache.sh --target nodejs
scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg # custom output dir

# Or manually, three steps:
cargo build --release --bin sjulia --features repl                          # Step 1
./target/release/sjulia --precompile-prelude "$(pwd)/target/prelude_program_cache.bin"
./target/release/sjulia --precompile-base "$(pwd)/target/base_cache.bin"    # Step 2
cd subset_julia_vm_web
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/../target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/../target/base_cache.bin" \
  wasm-pack build --target web --profile web-release                        # Step 3
```

Tradeoff: the caches are bundled into the resulting `.wasm`. Trade download
size against first-run latency per deployment.

### iOS App

```bash
xcodebuild \
  -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
  -scheme SubsetJuliaVMApp \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPad (A16)' \
  build
```

## Testing

SubsetJuliaVM has comprehensive test coverage across multiple test suites.

### Unit Tests

```bash
# Run all unit tests
timeout 180 cargo test

# Run specific test module
timeout 180 cargo test --test integration_tests
timeout 180 cargo test --test dispatch_tests
```

### Fixture Tests (Cross-Platform Compatibility)

Fixture tests verify identical behavior between SubsetJuliaVM and official Julia. The same `.jl` files are executed by both implementations.

```bash
# Run all fixture tests in SubsetJuliaVM
timeout 180 cargo test --test fixture_tests

# Run specific category
timeout 180 cargo test --test fixture_tests array::
timeout 180 cargo test --test fixture_tests operators::
timeout 180 cargo test --test fixture_tests rational::

# Run with official Julia
julia scripts/run_julia_tests.jl

# Run with sjulia CLI
./scripts/run_sjulia_tests.sh

# Compare all environments
./scripts/compare_all.sh
```

**Fixture manifests:** currently 1,107 test entries across 88 fixture manifest files under `tests/fixtures/`.

### Base & Stdlib Tests

```bash
# Build sjulia CLI first
cargo build --release --bin sjulia --features repl

# Run base function tests
./tests/base/run_all.sh

# Run stdlib tests
./tests/stdlib/run_all.sh

# Run specific test file
./target/release/sjulia tests/base/test_math.jl
./target/release/sjulia tests/stdlib/test_Statistics.jl
```

**Base Tests Include:**
- `test_operators.jl` — min, max, copysign, flipsign, cmp
- `test_math.jl` — Trigonometric, exponential, rounding functions
- `test_intfuncs.jl` — gcd, lcm, factorial, binomial, powermod
- `test_array.jl` — Array operations and aggregation
- `test_statistics.jl` — Statistical functions
- `test_sort.jl` — Sorting and searching algorithms
- `test_set.jl` — Set operations

### Benchmarking

SubsetJuliaVM uses [Criterion.rs](https://github.com/bheisler/criterion.rs) for statistical benchmarking:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench vm_benchmark        # Basic VM performance
cargo bench --bench detailed_benchmark  # Pipeline analysis
cargo bench --bench calc_pi_benchmark   # VM-only calc_pi workload

# Filter by benchmark name
cargo bench -- fib
cargo bench -- array_sum

# View HTML reports
open target/criterion/vm_benchmark/fib_20/report/index.html
```

## Project Structure

```
subset_julia_vm/           # Core Rust VM
├── src/
│   ├── parser/           # Pure Rust Julia parser
│   ├── lowering/         # CST → Core IR with feature gating
│   ├── ir/               # Core IR type definitions
│   ├── compile/          # IR → bytecode compiler
│   ├── vm/               # Stack-based VM (34 modules)
│   │   ├── exec/         # Instruction execution
│   │   ├── value.rs      # Value types (Int, Float, Array, etc.)
│   │   └── error.rs      # Error types with spans
│   ├── julia/            # Pure Julia implementations
│   │   ├── base/         # Base library files
│   │   └── stdlib/       # Standard library modules (10)
│   ├── builtins.rs       # Builtin function IDs
│   ├── intrinsics.rs     # CPU-level operations
│   ├── lib.rs            # Public Rust API + FFI exports
│   └── bin/
│       ├── sjulia.rs     # CLI REPL and file execution
│       └── bundle.rs     # Standalone executable bundler
├── tests/
│   ├── fixtures/         # 1,100+ cross-platform tests (87 categories)
│   ├── base/             # Base library tests
│   └── stdlib/           # Standard library tests
├── examples/             # Example Julia programs
├── scripts/              # Test runners and utilities
└── benches/              # Criterion.rs benchmarks

subset_julia_vm_web/       # WebAssembly bindings
├── src/lib.rs            # WASM exports
└── pkg/                  # Built WASM artifacts

SubsetJuliaVMApp/          # iOS app (Swift/SwiftUI)
├── Models/               # 47+ sample programs
│   ├── CodeSamples+Beginner.swift
│   ├── CodeSamples+Intermediate.swift
│   └── CodeSamples+Advanced.swift
├── Services/FFI/         # Rust FFI wrapper
│   └── JuliaVM.swift     # Swift interface to C ABI
├── Views/                # SwiftUI components
│   ├── ContentView.swift
│   └── CodeEditorView.swift
└── Resources/Samples/    # .jl sample files

docs/                      # Documentation
├── vm/                   # VM design and status
│   ├── STATUS.md         # Implementation status
│   ├── DESIGN.md         # Architecture rationale
│   └── UNIMPLEMENTED.md  # Known limitations
├── ios/                  # iOS app documentation
└── implementation/       # Design and implementation plans

CLAUDE.md                  # AI assistant development guidelines
```

## REPL Features

The interactive REPL (`sjulia`) provides a Julia-like development experience:

### Features

- **Persistent state** — Variables and functions persist across evaluations
- **Multi-line input** — Automatic detection of incomplete expressions
- **History navigation** — Up/down arrows to recall previous inputs
- **Syntax highlighting** — Monokai color scheme
- **LaTeX completion** — Type `\alpha<Tab>` → `α`
- **Special variables** — `ans` holds the last result

### Commands

```julia
help(), ?       # Show help message
exit(), quit()  # Exit the REPL
reset()         # Clear all variables and definitions
vars(), whos()  # Show defined variables
```

### Keyboard Shortcuts

- `Ctrl-C` — Cancel current input
- `Ctrl-D` — Exit the REPL
- `Up/Down` — Navigate command history
- `Tab` — Insert 4 spaces or complete LaTeX symbols

## C API for FFI

SubsetJuliaVM exposes a C-compatible API for integration with Swift, JavaScript, and other languages:

```c
// Cancellation (best-effort)
void vm_request_cancel(void);
void vm_reset_cancel(void);

// Compile Julia subset source to IR JSON (free with free_string)
char* compile_to_ir(const char* src);
void free_string(char* ptr);

// Compile & run (numeric return)
double compile_and_run(const char* src, uint64_t seed);
double compile_and_run_auto(const char* src, uint64_t seed);

// Compile & run with captured output (free with free_string)
char* compile_and_run_with_output(const char* src, uint64_t seed);

// Compile & run with detailed error information (free with free_execution_result)
CExecutionResult* compile_and_run_detailed(const char* src, uint64_t seed);
void free_execution_result(CExecutionResult* result);

// REPL helpers
int32_t is_expression_complete(const char* src);
char* split_expressions(const char* src); // JSON array, free with free_string

// Stateful REPL session API
void* repl_session_new(uint64_t seed);
CREPLResult* repl_session_eval(void* session, const char* src);
void repl_session_reset(void* session);
void repl_session_free(void* session);
void free_repl_result(CREPLResult* result);
```

See `include/subset_vm.h` for the authoritative, up-to-date C API surface.

### Swift Integration Example

```swift
@_silgen_name("compile_and_run_auto")
func compile_and_run_auto(_ src: UnsafePointer<CChar>, _ seed: UInt64) -> Double

let code = """
function fib(n)
    n <= 1 ? n : fib(n-1) + fib(n-2)
end
fib(20)
"""

let result = compile_and_run_auto(code, 42)
print("Result: \(result)")  // Result: 6765.0
```

## Error Handling

SubsetJuliaVM provides three types of errors with precise source locations:

### 1. SyntaxError (Parser Level)

Invalid Julia syntax. Rare because the parser is permissive.

```julia
julia> x = 1 +
ERROR: SyntaxError: Incomplete expression
  at line 1, column 7
```

### 2. UnsupportedFeature (Lowering Level)

Valid syntax that isn't yet implemented. Provides helpful hints.

```julia
julia> @mymacro expr
ERROR: UnsupportedFeature: User-defined macro expansion
Hint: Only built-in macros (@time, @assert, @show, @test) are supported
  at line 1, column 1
```

### 3. RuntimeError (VM Level)

Execution-time errors like division by zero or undefined variables.

```julia
julia> x = 10 / 0
ERROR: RuntimeError: Division by zero
  at line 1, column 7

julia> println(undefined_var)
ERROR: RuntimeError: Undefined variable: undefined_var
  at line 1, column 9
```

## Examples

### Mandelbrot Set

```julia
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute escape time for a point
result = mandelbrot_escape(-0.5 + 0.5im, 100)
println("Escape time: $result")
```

### Matrix Operations

```julia
using LinearAlgebra

# Create matrices
A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]
v = [1.0, 2.0, 3.0]

# Matrix-vector multiplication
result = A * v
println("A * v = $result")

# Element-wise operations with broadcasting
B = A .* 2
println("A .* 2 = $B")

# Transpose
println("A' = $(transpose(A))")
```

### Geometry Module with Multiple Dispatch

```julia
module MyGeometry

using Statistics: mean

export Point, distance, centroid

# Parametric struct with type constraint
struct Point{T<:Real}
    x::T
    y::T
end

# Extend Base operators
Base.:+(p::Point{T}, q::Point{T}) where T <: Real =
    Point{T}(p.x + q.x, p.y + q.y)

Base.:-(p::Point{T}, q::Point{T}) where T <: Real =
    Point{T}(p.x - q.x, p.y - q.y)

# Method with where clause
function distance(p::Point{T}, q::Point{T}) where T <: Real
    sqrt((q.x - p.x)^2 + (q.y - p.y)^2)
end

# Higher-order function with comprehension
function centroid(points::Vector{Point{T}}) where T <: Real
    x_mean = mean([p.x for p in points])
    y_mean = mean([p.y for p in points])
    T_new = promote_type(typeof(x_mean), typeof(y_mean))
    return Point{T_new}(x_mean, y_mean)
end

end  # module

# Use the module
using .MyGeometry

p = Point(3, 4)
q = Point(0, 0)

@assert distance(p, q) == 5.0
@assert p + q == Point(3, 4)

points = [Point(1, 2), Point(3, 4), Point(5, 6)]
center = centroid(points)
@assert center == Point(3.0, 4.0)

println("✓ All geometry tests passed!")
```

### Statistical Analysis

```julia
using Statistics

# Generate sample data
data = [rand() * 100 for _ in 1:1000]

# Compute statistics
μ = mean(data)
σ = std(data)
med = median(data)

println("Mean: $μ")
println("Std Dev: $σ")
println("Median: $med")

# Correlation
x = [1, 2, 3, 4, 5]
y = [2, 4, 6, 8, 10]
r = cor(x, y)
println("Correlation: $r")  # Perfect correlation: 1.0
```

## Architecture

### Three-Layer Function Implementation

SubsetJuliaVM uses a layered architecture for implementing functionality:

```
┌─────────────────────────────────────────────────┐
│ Layer 3: Pure Julia (src/julia/base/*.jl)      │
│          Implements: abs, gcd, factorial, sort  │
│          Benefits: Type dispatch, extensibility │
└─────────────────────────────────────────────────┘
                       ↓ (calls)
┌─────────────────────────────────────────────────┐
│ Layer 2: Builtins (builtins.rs)                │
│          Implements: sin, cos, map, filter      │
│          Benefits: Performance, standard lib    │
└─────────────────────────────────────────────────┘
                       ↓ (calls)
┌─────────────────────────────────────────────────┐
│ Layer 1: Intrinsics (intrinsics.rs)            │
│          Implements: add_int, mul_float, etc.   │
│          Benefits: Direct CPU operations        │
└─────────────────────────────────────────────────┘
```

**Design Principle:** Prefer Layer 3 (Pure Julia) when possible, enabling:
- Multiple dispatch with `where` clauses
- User-extensible operations
- Julia-compatible semantics
- Easier maintenance and testing

### Pipeline Stages

1. **Parser** — Converts Julia source to Concrete Syntax Tree (CST)
   - Pure Rust implementation (no C dependencies)
   - Never fails — permissive parsing with spans

2. **Lowering** — Converts CST to Core IR
   - Feature gating — rejects unsupported syntax
   - Helpful error messages for unsupported features

3. **Compiler** — Converts Core IR to bytecode
   - Optimizations (constant folding, dead code elimination)
   - Type specialization hints

4. **VM** — Stack-based interpreter
   - Typed values (not generic slots)
   - Deterministic execution
   - Configurable limits (stack depth, iteration count)

## Performance

SubsetJuliaVM is designed for **education, prototyping, and iOS deployment**. It is not a replacement for production Julia with LLVM JIT.

**Performance Characteristics:**
- Interpreted execution (no JIT compilation)
- Stack-based VM with typed value optimization
- Deterministic RNG (seeded, reproducible)
- Configurable execution limits prevent runaway computation on mobile

**Typical Performance:**
- `fib(20)`: ~200 μs
- 1000-element array sum: ~50 μs
- Simple arithmetic: ~1-5 μs per operation

**Benchmarking:**
```bash
cargo bench  # Run full benchmark suite with Criterion.rs
```

## Documentation

- **[CLAUDE.md](../CLAUDE.md)** — AI assistant development guidelines
- **[STATUS.md](../docs/vm/STATUS.md)** — Implementation status and recent changes
- **[DESIGN.md](../docs/vm/DESIGN.md)** — Architecture and design rationale
- **[iOS Documentation](../docs/ios/)** — iOS app implementation details

## Use Cases

SubsetJuliaVM is ideal for:

✅ **iOS Apps** — Julia computing on mobile devices
✅ **Web Applications** — Julia in the browser via WASM
✅ **Education** — Teaching Julia concepts with instant feedback
✅ **Prototyping** — Quick experiments without full Julia installation
✅ **Embedded Systems** — Lightweight Julia interpreter (~2MB)
✅ **Standalone Tools** — Distribute Julia programs as native executables

SubsetJuliaVM is NOT suitable for:

❌ **High-performance computing** — Use official Julia with LLVM JIT
❌ **Large-scale data processing** — Memory and speed constraints
❌ **Package ecosystem** — Cannot install arbitrary Julia packages
❌ **Dynamic code generation** — No `eval` or `@generated` functions

## Contributing

Contributions are welcome! Areas for improvement:

1. **Language coverage** — Implement more Julia syntax features
2. **Standard library** — Add more stdlib modules and functions
3. **Performance** — Optimize VM hot paths
4. **Error messages** — Improve UnsupportedFeature hints
5. **WASM optimizations** — Better web performance
6. **Documentation** — More examples and tutorials

**Development Workflow:**
1. Check [CLAUDE.md](../CLAUDE.md) for project guidelines
2. Create an issue for unsupported features or bugs
3. Branch from `main` (never commit directly to `main`)
4. Add tests for new features
5. Ensure `cargo test` passes
6. Submit a PR with clear description

**Running Tests:**
```bash
# Unit tests
cargo test

# Fixture tests (cross-platform compatibility)
cargo test --test fixture_tests

# All tests including Julia comparison
./scripts/compare_all.sh
```

## License

MIT License — See [LICENSE](../LICENSE) file for details.

## Acknowledgments

- **[subset_julia_vm_parser](https://github.com/AtelierArith/subset_julia_vm_parser)** — Pure Rust Julia parser
- **[Julia Language](https://julialang.org/)** — Syntax and semantics inspiration
- **[StableRNGs.jl](https://github.com/JuliaRandom/StableRNGs.jl)** — Deterministic RNG compatibility

## Contact

For questions, issues, or feature requests:
- **GitHub Issues**: [github.com/AtelierArith/ailujsoi/issues](https://github.com/AtelierArith/ailujsoi/issues)
- **Discussions**: Use GitHub Discussions for general questions

---

**Made for Julia on iOS**
