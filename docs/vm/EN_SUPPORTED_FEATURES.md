# English Supported Features

**Status**: English translation snapshot from 2026-02-20. Feature support
has continued to change; use the Japanese
[`SUPPORTED_FEATURES.md`](SUPPORTED_FEATURES.md) as the current canonical
feature-status document.

> **Note**: This is an English reference document for international contributors. The
> canonical source of truth for feature status is the Japanese `SUPPORTED_FEATURES.md`
> in the same directory. For architecture context, see
> [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md).

This document organizes the **Julia features currently supported** by SubsetJuliaVM (`subset_julia_vm`), based on the codebase (particularly `docs/vm/DONE.md`, `docs/vm/STATUS.md`, `tests/fixtures/`).

- **Unimplemented/Unsupported list**: `docs/vm/UNIMPLEMENTED.md`
- **Implementation history log (feature details)**: `docs/vm/STATUS.md`
- **Implemented features list (DONE)**: `docs/vm/DONE.md`

> Note: "Supported" here means **working on SubsetJuliaVM and verified by tests** (compatibility goal: "return the same results as official Julia"). Some features are **simplified/partially implemented** with limitations (noted in each section).

---

## Target Platforms and Differences

- **Native (macOS/Linux/Windows)**: Most feature-complete execution environment. File reading functions like `include()` are primarily for this platform.
- **iOS**: AoT/VM format adapted to App Store constraints (JIT prohibited, etc.). External processes are not supported.
- **WASM/Web**: Executed via `subset_julia_vm_web`. The Web API provides execution and completion.
  - `get_supported_features()` in `subset_julia_vm_web/src/lib.rs` is a **brief overview list**; this document contains the authoritative details.

---

## Pipeline (Overall Architecture)

> See [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) for a detailed, up-to-date
> description of the full pipeline and module structure.

- **Pure Rust Parser** (`subset_julia_vm_parser`) for Julia syntax parsing
  - Same code path for WASM/Native
  - Parses `where` clauses, macros, arrow functions, qualified operator names, juxtaposition (`2x`, `3.0im`), etc.
- **Lowering** (CST → Core IR)
  - Feature support/unsupported (UnsupportedFeature) is determined during lowering, returning errors with **span + hint**
  - Macro expansion primarily occurs at **compile time (during lowering)**
- **Compiler** (Core IR → Bytecode)
  - Includes multiple dispatch, parametric types, type inference (v2 integration)
  - Peephole optimization and Base cache integration
- **VM** (Stack-based VM)
  - Executes exceptions, HOFs (map/filter/reduce, etc.), broadcast, typed arrays, etc.
- **AoT / persisted formats**
  - Save/load `.sjir` Core IR files
  - Save/load `.sjvmbc` VM bytecode files
  - Toolchain from Core IR to AoT (Rust code generation)

---

## Language Syntax (Julia Core Syntax)

### Control Flow

- **Conditionals**: `if / elseif / else`
- **Loops**:
  - `for` (`start:stop`, `start:step:stop`)
  - `for x in iterable` (iterate protocol for arrays/tuples/strings/Range/Dict/Set, etc.)
  - `while ... end`
  - `break`, `continue`, `return`
  - **Tuple destructuring** in `for`: `for (i, x) in pairs(arr)`, etc.
- **Low-level control flow**:
  - `@label name` - Define a jump target label
  - `@goto name` - Unconditionally jump to the specified label
  - Supports both forward and backward jumps
  - Function-local only (cannot jump across function boundaries)
  - `@goto` to an undefined label causes a compile error
  - Supports jumps from within nested control flow
- **Exception handling**:
  - `try/catch/finally`
  - `catch e` receives the exception value, allowing field access (if the exception is a struct)
  - `finally` executes even when `return` is in `try/finally`
- **Short-circuit evaluation**:
  - `&&`, `||` short-circuiting
  - Execution of `return`/`break`/`continue` patterns using these
- **Ternary operator**: `a ? b : c`
- **let block**: `let ... end`

### Functions

- **Definitions**:
  - Normal definition: `function f(x, y) ... end`
  - Short definition: `f(x) = expr`
  - Anonymous functions (lambda/arrow): `x -> x^2`, `(x, y) -> x + y`
  - Recursion
  - do syntax (HOF calls): `map(arr) do x ... end`
- **Arguments**:
  - **Keyword arguments**: `f(; x=1, y=2)`
  - **Varargs**: `f(args...)`, `f(a, b, rest...)`
  - **Splat calls**: `f(args...)` (array/tuple expansion)
  - **Keyword argument slurping**: `f(; kwargs...)` (received as `Base.Pairs`)
- **Return type annotation**:
  - `f(x)::T = ...` supported
  - `convert(T, value)` equivalent conversion is applied to return values (for compatibility)
- **First-class functions**:
  - Functions can be passed as arguments and called with multiple arguments
  - Pattern `caller(f, x, y) = f(x, y)` supported
  - Higher-order functions like `mergewith`, `mergewith!` are now usable

### Modules

- **Definition**: `module Name ... end`
- **baremodule**: `baremodule Name ... end` (currently operates with nearly equivalent semantics to `module`)
- **import/using/export**:
  - `using Module`, `import Module`
  - `using Module: f`, `import Module: f`
  - Module-qualified calls: `Module.f`
  - Module aliases: `S = Statistics; S.mean(...)`
- **stdlib modules** (detailed later):
  - `Statistics`, `Random`, `Dates`, `Test`, `LinearAlgebra` (partial)
  - `Iterators`, `Printf`, `Broadcast`, `InteractiveUtils`

### Literals / Basic Expressions

- **Numeric literals**:
  - Decimal integers/floating-point
  - Hex/binary/octal integers (underscore separators supported)
  - Float32 literals (`1.0f0`, `1f2`, etc.)
  - Hex floating-point (`0x1.8p3`, etc.)
  - Large integer literals (auto-promotion to Int128/BigInt)
- **Characters/Strings**:
  - Character: `Char`
  - String: `String`
  - String interpolation: `"x = $(x)"`
  - `raw"..."` (raw strings)
  - `r"..."` (regex literals, flags `i/m/s/x`)
  - `v"1.2.3"` (VersionNumber literal)
  - `b"data"` (byte string literal → generated as `Vector{Int64}`)
  - `html"<b>text</b>"` (HTML literal → `HTML{String}` object)
  - `text"..."` (Text literal → `Text{String}` object)
  - **Custom string macros**: `prefix"text"` → `prefix_str(text)` function call (user-definable)
- **Juxtaposition**:
  - `2x` (implicit multiplication)
  - `3.0im` (complex number imaginary unit)
- **Range**:
  - `1:10`, `1:2:10`
  - `OneTo(n)` / `oneto(n)`
- **Arrays/Tuples**:
  - Arrays: `[1,2,3]`, matrices: `[1 2; 3 4]`
  - Empty arrays: `[]`, typed empty arrays: `Int64[]`, `Float64[]`, `String[]`, etc.
  - Tuples: `(1,2,3)`, destructuring assignment: `a, b = (1,2)`
  - NamedTuple: `(x=1, y=2)`, field access: `nt.x`
- **Comprehensions/Generators**:
  - Array comprehensions (with filter)
  - Generator (lazy evaluation)
  - `Dict(...)` / `Set(...)` comprehensions (representative single-generator cases with filters)
  - Note: multi-dimensional comprehensions remain tracked in "Comprehensions/Generators" in `UNIMPLEMENTED.md`

---

## Macros & Metaprogramming

### Macro System (Compile-time Expansion + Hygiene)

- **User-defined macros**:
  - `macro name(args) ... end`
  - Variadic macro arguments: `macro f(p...)`
- **quote / interpolation**:
  - `quote ... end` (usable as expressions)
  - Expression interpolation: `$expr`
  - Runtime splat interpolation: `:(f($(args...)))`
- **Hygiene**:
  - Local variable collision avoidance (2-pass: collect → rename application)
  - Hygiene escape via `esc()`
  - `local` in quote supported
- **Base/stdlib macro loading**
  - Resolved via 3-tier registry: user → Base → stdlib
  - Early loading of stdlib macros via `using Test`, etc.

### Implemented Representative Macros/Features

- **Testing**: `@test`, `@testset`, `@test_throws` (`using Test`)
- **Timing/Allocation**: `@time`, `@elapsed`, `@timed`, `@timev`, `@showtime`, `@allocated`, `@allocations`
- **Debugging**: `@show`, `@assert`
- **Logging**: `@debug`, `@info`, `@warn`, `@error` (message + up to 3 `key=value`, no logger/filter)
- **Compatibility macros (no-op)**: `@inline`, `@noinline`, `@inbounds`, `@boundscheck`, `@propagate_inbounds`, `Base.@nospecializeinfer` (function-definition wrapper)
- **Simple compatibility**: `@eval`, `@deprecate` (`@eval` expands normally, `@deprecate` no warning)
- **Others**: `@something`, `@coalesce`, `@evalpoly`
- **Array views**: `@view`, `@views` (transforms slices to `view()` calls)
- **Location info**: `@__LINE__`, `@__FILE__`, `@__MODULE__` (`@__DIR__` is environment-dependent)
- **@kwdef**
  - `@kwdef struct ... end` generates keyword argument constructors (lowering implementation)
- **@static**
  - Compile-time conditional evaluation macro
  - `@static if cond ... else ... end` or `@static cond ? a : b`
  - Supported conditions: `true`, `false`, `Sys.isapple()`, `Sys.isunix()`, `Sys.iswindows()`, `Sys.islinux()`, `Sys.isbsd()`
- **@enum**
  - Enumeration type definition macro
  - `@enum TypeName member1 member2 ...` (auto-increment from 0)
  - `@enum TypeName member1=1 member2=5` (explicit values)
  - Type system: Supports `JuliaType::Enum`, `Value::Enum`
- **@generated**
  - Phase 1: Fallback for `if @generated ... else fallback end`
  - Phase 2: Extract N from `Val{N}` for runtime use
  - Phase 3: "Unquote" simple quotes for direct execution
    - `return :(x + y)` → `return x + y`
    - Multi-statement block expansion
    - Function calls in quotes: `:(sin(x))`, `:(abs(sin(x)))`
    - begin/end blocks, ternary operator support
  - Full `@generated function ... end` syntax for representative type/value-parameter returns
    - Note: SubsetJuliaVM-specific feature (differs from official Julia)

### Expr/QuoteNode/LineNumberNode/GlobalRef and eval

- AST construction via `Expr(:head, args...)`
- `QuoteNode(value)` and `qn.value`
- `LineNumberNode(line)` / `LineNumberNode(line, file)` with `.line` / `.file`
- `GlobalRef(mod, name)` with `.mod` / `.name`
- `eval(expr)` and `eval(mod, expr)` (module argument currently assumes Main)
- `macroexpand` / `macroexpand!` (essentially no-op at runtime since expressions are already expanded)

---

## Type System & Dispatch

### Type Hierarchy and Basic Types

- Hierarchy: `Any, Number, Real, Integer, AbstractFloat`, etc.
- Representative concrete types:
  - Integers: `Int8..Int128`, `UInt8..UInt128`, `Int64` (primary)
  - Floating-point: `Float32`, `Float64`
  - Arbitrary precision: `BigInt`, `BigFloat`
  - `Bool`, `Char`, `String`
  - `Complex{T}` (Pure Julia implementation)
  - `Rational{T}` (Pure Julia implementation)
  - Collections: `Array`, `Tuple`, `NamedTuple`, `Dict`, `Set`, Range types
  - `Module` (modules can be used as values)
- `Union{...}` supported (including `Union{}` Bottom)

### Multiple Dispatch

- Method selection based on type annotations
- Parametric types:
  - `struct Point{T} ... end`
  - `where` type variables: `f(x::MyStruct{T}) where T`
- `Type{T}` dispatch: `f(::Type{T}) where T`
- Runtime dispatch (binary operations with Any, etc.)

### Type-related Builtins/Utilities

- `typeof`, `isa`, `<:` (subtype expression evaluation)
- `convert`, `promote`, `promote_type`, `promote_rule` (Julia compliance goal)
- Reflection/Introspection (implemented scope):
  - `nameof`, `nfields`, `fieldnames`, `fieldcount`
  - `fieldtype(T, i)`, `fieldtype(T, name::Symbol)`, `fieldtypes`
  - `fieldindex(T, name::Symbol)`, `fieldindex(T, name, err)`
  - `methods`, `hasmethod`, `which`
  - `ispublic`, `isexported` (internal functions, not exported from Base)
- Property/Field access:
  - `getproperty(x, s::Symbol)` - Get property value
  - `setproperty!(x, s::Symbol, v)` - Set property value
  - `propertynames(x)` - Get property names
  - `hasproperty(x, s::Symbol)` - Check property existence
  - `getfield(x, name)` / `setfield!(x, name, v)` - Field operations (builtin)
- Type traits (subset from Julia `base/traits.jl`, internal use only):
  - `OrderStyle` (Ordered/Unordered)
  - `ArithmeticStyle` (Rounds/Wraps/Unknown)
  - `RangeStepStyle` (Regular/Irregular)
  - `IndexStyle` (IndexLinear/IndexCartesian)
  - Note: These are not exported from Base; for internal implementation only
- Exception types (many implemented & exported): `ArgumentError`, `DomainError`, `MethodError`, `UndefVarError`, `LoadError`, etc.

### Type Inference (Type Inference v2)

- Lattice-based (`LatticeType`: Bottom/Concrete/Union/Conditional/Top)
- Abstract interpretation (fixed-point iteration)
- Major transfer functions (arithmetic, arrays, strings, intrinsics)
- Type inference test categories in fixtures (`tests/fixtures/type_inference/`, loop variables/conditional branches/Union types, etc.)

---

## Data Structures and Arrays (Array/Typed Array/Broadcast)

### Arrays (1D/2D) and Typed Arrays

- 1D/2D array creation and basic operations
- Typed array storage (`ArrayData` + `ArrayElementType`) efficiently handles:
  - Numerics (I*/U*/F32/F64), Bool, Char, String
  - Tuple arrays (`TupleOf`)
  - AoS inline storage for isbits struct arrays (`StructInlineOf`)
- Linear indexing (for multidimensional arrays `A[i]`, column-major)
- N-dimensional slicing:
  - 1D: `arr[1:3]`, `arr[:]`
  - 2D: `mat[:, :]`, `mat[1:2, :]`
  - 3D+: `arr[1:5, 2:4, :]` and arbitrary dimensional slicing
  - Type preservation: Element type is preserved during slicing operations
  - Index `begin` / `end` (lowered to `firstindex`/`lastindex`)
- `getindex` / `setindex!` dispatch Julia-style (Array/String/Tuple/Dict, etc.)
- Logical indexing:
  - `arr[arr .> 0]`
  - `arr[[true,false,true]]`

### SubArray / view (Representative Subset)

- `SubArray` and `view(A, ...)` (1D `Vector` range views, range / `OneTo` views, 2D matrix range/colon/dimension-dropping views, 3D range views)
- `@view` / `@views` (transform slices to `view` calls)
- Covered cases preserve parent aliasing across `getindex` / `setindex!` / `collect` / `map` / broadcast / `sum`
- Limitation: this is not the full upstream Julia SubArray index-combination surface; fixtures pin the representative cases above

### Broadcast

- Dot operators:
  - `.+, .-, .*, ./, .^`
  - `.<, .>, .<=, .>=, .==, .!=`
  - `.&, .|, .!`
  - `.=` and compound assignment (`.+=`, etc.)
- `broadcast(f, A, B)` / `broadcast!(f, dest, A, B)` (user-defined functions supported)
- Tuple broadcast:
  - `(1,2,3) .+ (4,5,6)`, etc.
  - Includes fallback to arrays when mixing arrays and tuples

---

## Iteration (iterate Protocol)

- `iterate(obj)` / `iterate(obj, state)` can be defined for user types
- `IterateDynamic` enables runtime iterate dispatch for `Any` types
- `collect(iterable)` (Pure Julia implementation)
- Implemented representative iterators/utilities (Pure Julia):
  - `enumerate`, `zip`
  - `take`, `drop`
  - `countfrom` (infinite count)
  - `eachcol`, `eachrow`
  - `skipmissing`
  - `peel` and `Rest`

---

## Numerics, Operators & Math Functions

### Operators

- Arithmetic/modulo/power: `+ - * / % ^`
- Rational: `//` (`Rational{T}`)
- Comparison: `< > <= >= == !=`
- Identity/Equality:
  - `===` / `≡`
  - `!==` / `≢`
  - `isequal` (considers NaN/±0.0)
- Chained comparisons (lowered to `&&` chain):
  - `1 <= x <= 10`, etc., any length
- Function composition: `∘` (`ComposedFunction` and execution support)
- Unicode math operators:
  - `√`, `∛`, `∜`
  - `≈`, `≉`

### Math Functions (Representative)

Combining Rust builtins and Pure Julia implementations, primarily supporting:

- Trigonometric/Inverse trig: `sin`, `cos`, `tan`, `asin`, `acos`, `atan` (plus many derivatives)
- Hyperbolic/Inverse hyperbolic: `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`
- Exponential/Logarithmic: `exp`, `exp2`, `exp10`, `expm1`, `log`, `log2`, `log10`, `log1p`
- Roots: `sqrt`, `cbrt`, `fourthroot`
  - `sqrt` throws `DomainError` for negative real arguments (Julia compatible)
- Rounding/Absolute value: `floor`, `ceil`, `round`, `trunc`, `abs`
- Bit operations:
  - `count_ones`, `count_zeros`, `leading_ones`, `leading_zeros`, `trailing_ones`, `trailing_zeros`
  - `bitreverse`, `bitrotate`, `bswap`
- Floating-point utilities:
  - `nextfloat`, `prevfloat`
  - `frexp`, `exponent`, `significand`, `issubnormal`, `maxintfloat`
- `fma`, `muladd`
- `gcd`, `lcm` (including BigInt support)

### Complex / Rational / BigInt / BigFloat

- **Complex**:
  - `Complex{T}` Pure Julia implementation (`im`, `real`, `imag`, `conj`, `abs2`, etc.)
  - `cis`, `cispi`, `reim`
  - Generic dispatch for `Complex{Float32}` / `Complex{Int64}` and mixed Real/Complex arithmetic/equality
  - Complex array broadcast (representative cases)
  - Complex array matrix-vector multiplication: `A * v` where v is `Vector{ComplexF64}`
- **Rational**:
  - `Rational{T<:Integer}`, normalization, arithmetic/comparison, `numerator`/`denominator`
  - Mixed multiplication of `Rational` and `Float64`
- **BigInt**:
  - Literal/conversion `big`, BigInt-Int64/Int128 mixed comparison and operations
  - `gcd`/`lcm`, `factorial(::BigInt)`, etc.
- **BigFloat**:
  - `precision(BigFloat)` - Get default precision (default: 256 bits)
  - `precision(x::BigFloat)` - Get precision of a specific BigFloat
  - `setprecision(BigFloat, n)` / `setprecision(n)` - Set default precision
  - `rounding(BigFloat)` - Get current rounding mode (returns `RoundingMode` struct)
  - `setrounding(BigFloat, mode)` - Set rounding mode
  - Supported rounding modes: `RoundNearest`, `RoundToZero`, `RoundUp`, `RoundDown`, `RoundFromZero`

---

## Collections (Dict / Set) and Related APIs

### Dict

- `Dict("a" => 1)` and basic operations
- `d[key]` / `d[key] = value`
- `haskey`, `get`, `keys`, `values`, `pairs`, `merge`, `merge!`, `get!`
- Iteration
- Dict comprehensions (`Dict(k => v for ...)`, including representative filtered cases)

### Set

- `Set([1,2,3])`
- `union`, `intersect`, `setdiff`, `symdiff`
- `issubset` (supports both Set/Array)
- `push!`, `delete!`
- `issetequal` (set equality check)
- Set comprehensions (`Set(x for ...)`, including representative filtered cases)

---

## Strings and Regular Expressions

### String Operations (Excerpt)

Combining Pure Julia and Rust builtins, primarily supporting:

- Basic: `string`, `repr`
- Case: `uppercase`, `lowercase`, `titlecase`, `lowercasefirst`, `uppercasefirst`
- Trim/Chop: `strip`, `lstrip`, `rstrip`, `chomp`, `chop`
- Padding: `lpad`, `rpad`
- Search/Split: `findnext`, `findprev`, `split`, `rsplit`, `join`, `contains`, `startswith`, `endswith`
- String indexing: `nextind`, `prevind`, `thisind`, `reverseind`, `isvalid`
- Escaping: `escape_string`, `unescape_string`
- Byte sequences: `codeunits`, `bytes2hex`, `hex2bytes`
- Binary representation: `bitstring`
- `tryparse(Int|Float, s)` (returns `nothing` on failure)

### Regular Expressions (Regex)

- `Regex` / `RegexMatch` types
- `r"pattern"` literal
- `match`, `occursin`, `eachmatch`
- Flags: `i` (ignorecase), `m` (multiline), `s` (dotall), `x` (extended)

---

## I/O, Display, Path/Filesystem (Subset)

### Basic I/O

- `print`, `println`
- `IOBuffer()` with `write(io, x)`, `take!(io)` (`Vector{UInt8}`), `String(take!(io))`
- `sprint(x)` / `sprint(f, args...)`
  - VM supports user-defined `f(io, args...)` (dedicated instruction)
  - `context`-enabled sprint (`IOContext`) supported
- `@printf`, `@sprintf`
- `printstyled` (ANSI color subset)

### IOContext (Context-aware Printing)

- `IOContext` type (generated via `iocontext(io, ...)`)
- `ioget(ctx, key, default)` - Get property value
- `iohaskey(ctx, key)` - Check property existence
- `iokeys(ctx)` - Get all property keys
- `pipe_reader(ctx)` / `pipe_writer(ctx)` - Access underlying IO
- Output adjustment using properties like `:compact => true`
- `sprint(...; context=:compact=>true)` supports IOContext-enabled output

### Display (Multimedia I/O)

- **MIME types**:
  - `MIME"text/plain"` / `MIME("text/plain")` literal and constructor
  - `@MIME_str` macro (`MIME"..."` syntax)
  - `istextmime` - Check if MIME is text type
- **Display functions**:
  - `show(io, mime, x)` (falls back to `show(io, x)` if undefined)
  - `display(x)` - Output to stdout
  - `displayable(mime)` - Returns true for text MIMEs
  - `showable(mime, x)` - Returns true for text/plain
  - `redisplay(x)` - Delegates to display
- **Display stack**:
  - `AbstractDisplay`, `TextDisplay` types
  - `pushdisplay`, `popdisplay` - Stub implementation
- Limitation: Full display stack backend selection is incomplete

### Path Operations (Base Subset)

- `basename`, `dirname`, `joinpath`, `splitdir`, `splitext`, `splitpath`
- `isabspath`, `isdirpath`

### Files/Directories (Rust Builtins Subset)

- `pwd`, `readdir`
- `mkdir`, `mkpath`, `rm`, `touch`, `cd`, `tempdir`, `tempname`, `islink`
- `isfile`, `isdir`, `ispath`, `filesize`, `mtime`
- `read(filename, String)`, `readlines(filename)`, `readline(filename)`, `countlines(filename)`
- `cp`, `mv`

### File Handles

- `open(filename)` / `open(filename, mode)` (`r`, `r+`, `w`, `w+`, `a`, `a+`)
- `close(io)`, `isopen(io)`
- `readline(io)` (file IO only, other IO not supported)

> Limitation: Does not cover the entire Julia I/O/filesystem (see `UNIMPLEMENTED.md` for details).

---

## Base Exports (Functions/Types/Constants)

Refer to `subset_julia_vm/src/julia/base/exports.jl` for the organized list of **public symbols exported from Base**.

- **Scope**: Functions, operators, macros (function-equivalent), types, constants
- **Excluded**: Internal symbols
- **Complete list**: `subset_julia_vm/src/julia/base/exports.jl`

### Types
- `AbstractArray`, `AbstractChar`, `AbstractDict`, `AbstractDisplay`, `HTML`, `MIME`, `AbstractFloat`, `AbstractIrrational`, `AbstractMatrix`, `AbstractRange`, `AbstractSet`, `AbstractString`, `AbstractUnitRange`, `AbstractVector`, `Any`, `ArgumentError`, `AssertionError`, `BigFloat`, `BigInt`, `Bool`, `BoundsError`, `CanonicalIndexError`, `CapturedException`, `CartesianIndex`, `CartesianIndices`, `Char`, `Complex`, `ComplexF64`, `CompositeException`, `DenseArray`, `DenseMatrix`, `DenseVector`, `Dict`, `DimensionMismatch`, `DivideError`, `DomainError`, `EOFError`, `ErrorException`, `Exception`, `IndexCartesian`, `IndexLinear`, `IndexStyle`, `InexactError`, `InvalidStateException`, `IOBuffer`, `IOContext`, `Irrational`, `KeyError`, `LinearIndices`, `LinRange`, `LoadError`, `MethodError`, `Missing`, `MissingException`, `Float32`, `Float64`, `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `Int128`, `Integer`, `Matrix`, `Memory`, `Nothing`, `Number`, `OutOfMemoryError`, `OverflowError`, `Pair`, `ProcessFailedException`, `Rational`, `Real`, `Ref`, `Regex`, `RoundingMode`, `RoundDown`, `RoundFromZero`, `RoundNearest`, `RoundNearestTiesAway`, `RoundNearestTiesUp`, `RoundToZero`, `RoundUp`, `Set`, `Signed`, `StackOverflowError`, `String`, `StringIndexError`, `Symbol`, `SystemError`, `Channel`, `Task`, `TaskFailedException`, `Condition`, `Text`, `TextDisplay`, `Tuple`, `TypeError`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`, `UndefKeywordError`, `UndefRefError`, `UndefVarError`, `UnitRange`, `Unsigned`, `Vector`, `VersionNumber`

### Mathematical Constants
- `VERSION`, `ENDIAN_BOM`, `Inf`, `Inf16`, `Inf32`, `Inf64`, `NaN`, `NaN16`, `NaN32`, `NaN64`, `im`, `missing`, `nothing`, `pi`, `π`, `ℯ`
- Note: `e`, `γ`, `eulergamma`, `φ`, `golden`, `catalan` are only accessible from the `Base.MathConstants` submodule (same as upstream Julia)

### Operators
- `!`, `!=`, `!==`, `%`, `&`, `*`, `+`, `-`, `/`, `//`, `<`, `<=`, `==`, `>`, `>=`, `\`, `^`, `|`, `~`, `:`, `=>`, `÷`, `≠`, `≡`, `≢`, `≤`, `≥`

### Scalar Math
- `abs`, `abs2`, `acos`, `acosd`, `acosh`, `acot`, `acotd`, `acoth`, `acsc`, `acscd`, `acsch`, `angle`, `asec`, `asecd`, `asech`, `asin`, `asind`, `asinh`, `atan`, `atand`, `atanh`, `big`, `binomial`, `bitreverse`, `bitrotate`, `bswap`, `cbrt`, `ceil`, `cis`, `cispi`, `clamp`, `clamp!`, `cld`, `cmp`, `complex`, `conj`, `conj!`, `copysign`, `cos`, `cosc`, `cosd`, `cosh`, `cospi`, `cot`, `cotd`, `coth`, `count_ones`, `count_zeros`, `csc`, `cscd`, `csch`, `deg2rad`, `denominator`, `div`, `divrem`, `eps`, `evalpoly`, `exp`, `exp10`, `exp2`, `expm1`, `exponent`, `factorial`, `fld`, `fld1`, `fldmod`, `fldmod1`, `flipsign`, `float`, `floatmax`, `floatmin`, `floor`, `fma`, `fourthroot`, `frexp`, `gcd`, `gcdx`, `get_zero_subnormals`, `hypot`, `identity`, `imag`, `inv`, `invmod`, `isapprox`, `isassigned`, `iseven`, `isfinite`, `isinf`, `isinteger`, `isnan`, `isnegative`, `isodd`, `isone`, `ispositive`, `ispow2`, `isqrt`, `isreal`, `issubnormal`, `iszero`, `lcm`, `ldexp`, `leading_ones`, `leading_zeros`, `log`, `log10`, `log1p`, `log2`, `max`, `maxintfloat`, `min`, `minmax`, `mod`, `mod1`, `mod2pi`, `modf`, `muladd`, `nand`, `nextfloat`, `nextpow`, `nextprod`, `nor`, `numerator`, `one`, `oneunit`, `powermod`, `precision`, `prevfloat`, `prevpow`, `rounding`, `setprecision`, `setrounding`, `set_zero_subnormals`, `rad2deg`, `rationalize`, `real`, `reim`, `reinterpret`, `rem`, `rem2pi`, `round`, `sec`, `secd`, `sech`, `sign`, `signbit`, `signed`, `significand`, `sin`, `sinc`, `sincos`, `sincosd`, `sincospi`, `sind`, `sinh`, `sinpi`, `sleep`, `sqrt`, `tan`, `tand`, `tanh`, `tanpi`, `time`, `time_ns`, `trailing_ones`, `trailing_zeros`, `trunc`, `tryparse`, `parse`, `typemax`, `typemin`, `unsafe_trunc`, `unsigned`, `widemul`, `xor`, `zero`, `√`, `∛`, `∜`, `≈`, `≉`

### Arrays
- `append!`, `axes`, `checkbounds`, `cat`, `checkindex`, `circshift`, `circshift!`, `copy`, `copy!`, `copyto!`, `deepcopy`, `cumprod`, `cumprod!`, `cumsum`, `cumsum!`, `accumulate`, `accumulate!`, `deleteat!`, `diff`, `dropdims`, `insertdims`, `eachcol`, `eachindex`, `eachrow`, `eachslice`, `empty`, `empty!`, `extrema`, `fill`, `fill!`, `first`, `firstindex`, `hcat`, `indexin`, `insert!`, `invperm`, `invpermute!`, `isperm`, `keepat!`, `last`, `lastindex`, `length`, `map!`, `mapslices`, `maximum`, `maximum!`, `minimum`, `minimum!`, `ndims`, `ones`, `permute!`, `permutedims`, `permutedims!`, `pop!`, `popat!`, `popfirst!`, `prepend!`, `prod`, `prod!`, `push!`, `pushfirst!`, `logrange`, `range`, `repeat`, `reshape`, `resize!`, `reverse`, `reverse!`, `rot180`, `rotl90`, `rotr90`, `selectdim`, `similar`, `size`, `splice!`, `stack`, `step`, `stride`, `strides`, `sum`, `sum!`, `transpose`, `vcat`, `vec`, `zeros`

### Search/Find
- `argmax`, `argmin`, `eachmatch`, `findall`, `findfirst`, `findlast`, `findmax`, `findmax!`, `findmin`, `findmin!`, `findnext`, `findprev`, `insorted`, `match`, `searchsorted`, `searchsortedfirst`, `searchsortedlast`

### Sorting
- `InsertionSort`, `issorted`, `MergeSort`, `partialsort`, `partialsort!`, `partialsortperm`, `partialsortperm!`, `PartialQuickSort`, `QuickSort`, `sort`, `sort!`, `sortperm`, `sortperm!`, `sortslices`

### Collections
- `all`, `allequal`, `allunique`, `any`, `collect`, `count`, `eltype`, `filter`, `filter!`, `foldl`, `foldr`, `foreach`, `mapfoldl`, `mapfoldr`, `get`, `get!`, `getindex`, `getkey`, `setindex!`, `haskey`, `hasmethod`, `applicable`, `in`, `in!`, `intersect`, `isdisjoint`, `isempty`, `issetequal`, `issubset`, `keytype`, `keys`, `map`, `mapreduce`, `merge`, `merge!`, `mergewith`, `mergewith!`, `pairs`, `reduce`, `sizehint!`, `setdiff`, `setdiff!`, `symdiff`, `symdiff!`, `union`, `union!`, `intersect!`, `unique`, `unique!`, `valtype`, `values`, `∈`, `∉`, `⊆`, `⊈`, `⊊`, `⊇`, `⊉`, `⊋`, `∩`, `∪`

### Strings and Characters
- `ascii`, `bitstring`, `bytes2hex`, `chomp`, `chop`, `chopprefix`, `chopsuffix`, `codepoint`, `codeunit`, `codeunits`, `contains`, `digits`, `endswith`, `escape_string`, `hex2bytes`, `isascii`, `iscntrl`, `isdigit`, `isletter`, `islowercase`, `isprint`, `isnumeric`, `ispunct`, `isspace`, `isuppercase`, `isvalid`, `isxdigit`, `join`, `lowercase`, `lowercasefirst`, `lpad`, `lstrip`, `ncodeunits`, `nextind`, `ndigits`, `occursin`, `prevind`, `replace`, `repr`, `summary`, `reverseind`, `rpad`, `rsplit`, `rstrip`, `split`, `startswith`, `string`, `strip`, `textwidth`, `thisind`, `titlecase`, `unescape_string`, `uppercase`, `uppercasefirst`

### Text Output
- `display`, `displayable`, `displaysize`, `dump`, `istextmime`, `popdisplay`, `print`, `println`, `printstyled`, `pushdisplay`, `redisplay`, `show`, `showable`, `showerror`, `sprint`, `take!`

### Path Manipulation
- `abspath`, `basename`, `dirname`, `homedir`, `isabspath`, `isdirpath`, `joinpath`, `normpath`, `splitdir`, `splitext`, `splitpath`

### Filesystem Operations
- `cd`, `close`, `countlines`, `cp`, `eof`, `filesize`, `isdir`, `isfile`, `islink`, `isopen`, `ispath`, `mkdir`, `mkpath`, `mtime`, `mv`, `open`, `pwd`, `read`, `readdir`, `readline`, `readlines`, `rm`, `tempdir`, `tempname`, `touch`, `write`

### Iteration
- `eachrsplit`, `eachsplit`, `enumerate`, `iterate`, `ntuple`, `only`, `tuple`, `zip`

### Object Identity and Equality
- `hash`, `identity`, `ifelse`, `isequal`, `isless`, `isnothing`, `oftype`, `Returns`, `Some`, `something`, `ismissing`, `coalesce`, `skipmissing`, `nonmissingtype`

### Types (Type-related Functions)
- `convert`, `promote`, `promote_rule`, `promote_type`, `typeof`, `isa`, `eltype`, `sizeof`, `isbits`, `isbitstype`, `supertype`, `fieldcount`, `fieldindex`, `fieldname`, `fieldnames`, `fieldoffset`, `fieldtype`, `fieldtypes`, `getfield`, `getproperty`, `hasfield`, `hasproperty`, `propertynames`, `setfield!`, `setproperty!`, `isconcretetype`, `isabstracttype`, `isprimitivetype`, `isstructtype`, `ismutable`, `ismutabletype`, `methods`, `nameof`, `nfields`, `objectid`, `which`, `isunordered`, `typeintersect`, `typejoin`, `widen`

### Linear Algebra
- `adjoint`

### Random
- `rand`, `randn`

### Bitarrays
- `BitArray`, `BitMatrix`, `BitVector`, `falses`, `trues`

### Dequeues
- `delete!`

### Errors
- `error`

### Tasks and Concurrency
- `asyncmap`, `schedule`, `fetch`, `wait`, `yield`, `yieldto`, `notify`, `istaskdone`, `istaskstarted`, `istaskfailed`, `current_task`, `task_local_storage`, `timedwait`, `waitany`, `waitall`, `errormonitor`

### Channels
- `bind`, `put!`, `isfull`, `isready`

### Metaprogramming
- `__precompile__`, `esc`, `evalfile`, `Expr`, `gensym`, `GlobalRef`, `include_dependency`, `include_string`, `LineNumberNode`, `macroexpand`, `macroexpand!`, `QuoteNode`

### Macros
- `@allocated`, `@allocations`, `@assert`, `@coalesce`, `@elapsed`, `@evalpoly`, `@lock`, `@show`, `@showtime`, `@something`, `@time`, `@timed`, `@timev`

---

## stdlib (Supported Modules)

- **Test**
  - `@test`, `@testset`, `@test_throws`
  - VM builtins: `_test_record!`, `_testset_begin!`, `_testset_end!`
- **Printf**
  - `@printf`, `@sprintf`
- **Iterators**
  - `enumerate`, `zip`, `rest`, `countfrom`, `take`, `drop`, `cycle`, `repeated`, `product`, `flatten`, `partition`, `peel`, `nth`
  - `Rest` iterator type (used in combination with `peel`)
- **Broadcast**
  - `broadcast`, `broadcast!` (dot operators/`f.` are processed by VM)
- **Statistics**
  - `mean`, `var`, `std`, `median`, `cov`, `cor`, `quantile`, etc.
- **Random**
  - `rand`, `randn`, `seed!` (deterministic RNG)
- **Dates**
  - Pure Julia implementation of Dates exists, with fixtures (module qualification may vary)
- **InteractiveUtils**
  - `versioninfo`, `supertypes` (simplified implementation, compiler internal reflection APIs not supported)
- **LinearAlgebra (Partial)**
  - Pure Julia: `tr`, `dot`, `norm`, `cross`, `kron`, `transpose`, `Diagonal`, matrix product `A * B`
  - Builtin routing: `svd`, `qr`, `lu`, `inv`, `det`, `eigvals`, `eigen`, `cholesky`, `rank`, `cond` (return values include NamedTuple)
  - `eigen()` enhancement: Supports both symmetric and non-symmetric matrices (computes complex eigenvalues/eigenvectors for non-symmetric)

---

## Tools / Peripheral Features (REPL / FFI / WASM / AoT)

### REPL (Development)

- Input splitting including block comments `#= ... =#` (nested)
- REPL sessions retain Expr/Symbol values, enabling round-trips via `Meta.parse` → `eval`

### Errors (span + hint)

- Error types: SyntaxError / UnsupportedFeature / RuntimeError, etc.
- Span information propagated to Swift/iOS side for highlighting

### C ABI (Swift/iOS Integration)

- `compile_and_run`, `compile_and_run_with_output`, `compile_and_run_detailed`
- Memory release API (`free_string`, `free_execution_result`)
- Execution cancel API (`vm_request_cancel`, `vm_reset_cancel`)

### WASM/Web API (subset_julia_vm_web)

- `run_from_source`, `run_from_source_typed`, `run_ir_json`, `run_ir_simple`
- `get_version`
- `get_supported_features`, `get_unsupported_features` (overview list)
- Unicode input assistance API (LaTeX → Unicode conversion/completion)

### AoT / persisted formats

- `.sjir` save/load (magic/version/flags + Core IR)
- `.sjvmbc` save/load (magic/version/flags + compiled VM bytecode)
- `sjulia --compile` for Core IR file generation
- `sjulia --compile-vm` / `--run-vm-bytecode` for VM bytecode generation and execution
- `aot --ir` for Rust generation from Core IR files (for AoT execution)
- **AoT Optimization Passes**:
  - **Constant Folding**: Integer/floating-point/comparison/logical/unary operations, string concatenation
  - **Dead Code Elimination (DCE)**: Removal of `if true/false` branches and `while false`
  - **Loop Optimization**: Loop unrolling (configurable max iterations), Loop Invariant Code Motion (LICM)
  - **Inlining**: Small function expansion
  - `optimize_aot_program_full()` executes all optimizations in recommended order

---

## What Determines "Supported" (Verification Basis)

- **Fixture tests**: `tests/fixtures/` (category-based manifest management, 17+ categories)
- **Integration tests**: `subset_julia_vm/tests/integration_*_tests.rs`, etc.
- **iOS/Sample tests**: `subset_julia_vm/tests/ios_samples_tests.rs`, etc.
- **Base exports consistency tests**: `subset_julia_vm/tests/base_exports_consistency_tests.rs`
- **Related documentation**:
  - Implemented: `docs/vm/DONE.md`
  - Progress log: `docs/vm/STATUS.md`
  - Unimplemented: `docs/vm/UNIMPLEMENTED.md`

---

## Removed Functions (Not Present in Upstream Julia)

The following functions have been removed from SubsetJuliaVM because they do not exist in upstream Julia Base (Issue #1322):

- `fliplr`, `flipud` - Only mentioned in Julia's HISTORY.md (deprecated)
- `isalnum` - Only mentioned in Julia's HISTORY.md (deprecated)

These functions have been removed to maintain compatibility with upstream Julia.
