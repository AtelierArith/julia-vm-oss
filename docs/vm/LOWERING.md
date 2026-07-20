# Lowering: Julia Source to Core IR

This document describes the lowering phase of SubsetJuliaVM, which transforms parsed Julia source code (CST) into Core IR.

## Executable documentation sweep status (Issue #8721)

Initial sweep date: 2026-07-02.

- Reviewed as one of the five major docs named by #8694/#8721.
- Checked the lowering module layout, parser/lowering CST contract, macro
  lowering paths, call-target registry, indexing keyword lowering, and known
  type-narrowing limitations against current source paths.
- No stale behavior claim was changed in this initial pass.
- Representative runnable Julia behavior snippets are now covered by
  `julia-doctest`; CST/IR shape examples should remain explanatory unless a
  stable renderer is added.

```julia-doctest
function sum_same_type(x::T, ys::T...) where T
    total = x
    for y in ys
        total += y
    end
    total
end

println(sum_same_type(1, 2, 3))
println(sum_same_type(1.0, 2.0))
# output
6
3.0
```

## Overview

The lowering phase is responsible for:
1. Converting CST (Concrete Syntax Tree) nodes to Core IR structures
2. Handling function signatures and parameter parsing
3. Processing type annotations and type expressions
4. Expanding macros and handling special forms

## Key Files

- `subset_julia_vm_lowering/src/lowering/mod.rs` - Main lowering module and `Lowering` struct
- `subset_julia_vm_lowering/src/lowering/function/` - Function definition lowering (directory module):
  - `mod.rs` - Entry point and shared logic
  - `signature.rs` - Parameter parsing, callable struct syntax
  - `full_form.rs` - `function ... end` form
  - `short_form.rs` - `f() = expr` form
  - `where_clause.rs` - `where T` clause handling
  - `defaults.rs` - Default parameter value handling
  - `tests.rs` - Unit tests
- `subset_julia_vm_lowering/src/lowering/struct_.rs` - Struct definition lowering
- `subset_julia_vm_lowering/src/lowering/abstract_.rs` - Abstract type definition lowering
- `subset_julia_vm_lowering/src/lowering/expr/` - Expression lowering (directory module):
  - `mod.rs` - Entry point, expression dispatch
  - `binary.rs` - Binary operator lowering
  - `call.rs` - Function call lowering
  - `collection.rs` - Array/Dict/comprehension lowering
  - `literal.rs` - Literal value lowering
  - `misc.rs` - Miscellaneous expressions (let, ternary, etc.)
  - `macros/` - Expression-level macro expansion (directory module):
    - `mod.rs` - Entry point, macro dispatch
    - `expand.rs` - Base macro expansion logic
    - `nested.rs` - Nested macro handling
    - `static_eval.rs` - Compile-time macro evaluation
    - `views.rs` - `@view`/`@views` macro handling
  - `quote/` - Quote metaprogramming (directory module):
    - `mod.rs` - Entry point
    - `code_generation.rs` - Generates IR from quoted expressions
    - `cst_to_constructor.rs` - Converts CST nodes to Expr constructors
    - `handlers.rs` - Handles specific quote patterns
    - `hygiene.rs` - Macro hygiene for quoted symbols
- `subset_julia_vm_lowering/src/lowering/stmt/` - Statement lowering (directory module):
  - `mod.rs` - Entry point, statement dispatch
  - `assignment.rs` - Assignment and `.=` broadcast lowering
  - `control_if.rs` - If/elseif/else lowering
  - `macros/` - Statement-level macro expansion (directory module):
    - `mod.rs` - Entry point
    - `expand.rs` - Statement macro expansion
    - `static_eval.rs` - Compile-time evaluation for statement macros
    - `enum_impl.rs` - `@enum` macro implementation

## Unified Source-File Top-Level Entry Point (Issue #10628)

`lowering/mod.rs` exposes two struct entry points over a source file's
top-level `NodeKind` children:

- **`Lowering`** — no `include()` support. Used by Base/prelude
  (`pipeline::parse_source`, per-file batched Base loading also goes through
  `LoweringWithInclude` — see below), REPL one-shot eval
  (`repl::session::Session`), and every other caller that never needs to
  resolve an `include(...)` call (`stdlib_loader.rs`, `bin/aot.rs`,
  `bin/bundle.rs`, `bin/compile_samples.rs`, and lowering's own unit tests).
- **`LoweringWithInclude`** — resolves `include("path")` calls against an
  `IncludeContext`, sharing the caller's `LambdaContext` across sequential
  `include`s in the same scope (Issue #7510). Used by `loader.rs` (file/CLI
  lowering) and, per-file, by the batched Base/prelude loader
  (`pipeline::parse_prelude_from_source_batched`,
  `lower_fragment_with_shared_context`).

Both `lower_source_file_inner` methods (the private per-struct entry points)
delegate to one shared free function, `lower_source_file_body`, parameterized
over `include_ctx: Option<&IncludeContext>`:

- `Lowering::lower_source_file_inner` builds a fresh `LambdaContext` (seeding
  `initial_usings`/`initial_macros` for REPL-carried state, Issue #9172), then
  calls `lower_source_file_body(&walker, node, &lambda_ctx, None)` — the
  `NodeKind::CallExpression` arm never special-cases `include(...)` when
  `include_ctx` is `None`, matching this struct's historical behavior.
  `LambdaContext::lifted_function_count()` on a fresh context is always `0`,
  so the shared function's internal `lifted_start`/`take_lifted_functions_from`
  bookkeeping is a no-op drain-everything for this caller too.
- `LoweringWithInclude::lower_source_file_inner` receives its `LambdaContext`
  from the caller (shared across sequential top-level `include`s) and calls
  `lower_source_file_body(&walker, node, lambda_ctx, Some(&include_ctx))` —
  the `CallExpression` arm checks `try_process_include_call` first and merges
  the included `Program` fragment when the call resolves to `include(...)`.

Before this unification the two structs carried hand-synced copies of the
same loop; a lowering feature (docstring capture into `pending_doc`,
`@kwdef` expansion, a new definition `NodeKind`, …) added to only one of them
silently diverged Base/prelude lowering from user-program lowering — exactly
the bug class #10164's 286 missing Base docstrings came from (see #10271).
`subset_julia_vm/tests/include_tests.rs`'s
`plain_and_include_aware_lowering_agree_on_representative_source_10628` locks
in that both entry points still produce identical `Program` IR for the same
include()-free source (docstrings, macros, structs, functions, a nested
module, `@kwdef`) as a permanent regression guard against reintroducing that
split.

## Static Stdlib-Macro Quote Expansion (Issue #10208)

Macros defined under `subset_julia_vm/src/julia/stdlib/*/src/*.jl` are
registered in the `STDLIB_MACROS` registry (`stdlib_loader.rs`) and expanded
by a **separate, static** template-substitution engine at lowering time —
`lowering/expr/quote/code_generation.rs` (`quote_constructor_to_code_with_hygiene`,
dispatching per `ExprHead` to the handlers in `handlers.rs`) plus the
nested-macro-calling-macro path in `lowering/expr/macros/nested.rs` (`qctc`,
used when a stdlib macro's `quote` block itself invokes another macro). This
is distinct from the VM-runtime `macroexpand` path
(`macro_runtime.rs`/`value_to_stmt`/`value_to_expr`) that **user-defined**
macros use, so the two engines can (and until #10208, did) support different
`Expr` head subsets for the same Julia source shape — tracked as a design
smell by epic #10266.

`Expr(:elseif, Expr(:block, condition), then_block[, else_or_elseif])` is now
handled by both static-expansion dispatchers, mirroring how upstream Julia
desugars `elseif` as a nested `Expr(:if, ...)` in the parent `if`'s
else-branch position (see the `IfStatement` case in
`quote/cst_to_constructor.rs` for the exact desugaring this mirrors). The
wrapped `condition` round-trips through the existing single-statement
`Expr(:block, ...)` handling (`handle_block_expr` / the `Block` arm of
`qctc`), which unwraps a one-element block to its bare expression — so an
`elseif` condition lowers to the same value a plain `if` condition would, and
`handle_elseif_expr` simply delegates to `handle_if_expr` (`nested.rs`'s
`qctc` handles `ExprHead::If` and `ExprHead::ElseIf` in one arm for the same
reason). `Test.@test`'s errored-outcome expansion (Issue #10093) was restored
from a nested-`if`/`else` workaround to the natural `if/elseif/else` form once
this landed.

Unifying the two macro-expansion engines (stdlib static vs. user-defined VM
runtime) into one, and generalizing `try`/`catch` inside stdlib-macro `quote`
blocks, remain open and are tracked by epic #10266 — out of scope for #10208.

### Macro Statement/Value Adapter Contract (Issue #10630)

An expanded stdlib macro quote lowers to a spine of nested `Block` and
`LetBlock` wrappers whose **final statement is the expansion's result value**
(for `Test` macros, the recorded `Test.Result`), while **every earlier
statement is a required effect** — the recorder control-flow subtree that
counts the pass/fail/error outcome and sets the sticky failure flag
(Issue #8191). Two adapters consume such an expansion, and each has one
invariant:

| Position | Path | Invariant |
|---|---|---|
| Statement (`@test x` as its own statement) | `discard_macro_tail_value` (`lowering/stmt/macros/expand.rs`) | Recurse through the `Block`/`LetBlock` wrapper spine and replace **only the innermost result tail** with `nothing`. Every effect statement is retained verbatim. |
| Expression (`r = @test x`, operand, call argument) | `expand_stdlib_macro_expr` (`lowering/expr/macros/expand.rs`) | Value-preserving: the whole expansion keeps its tail so the caller receives the `Test.Result` / TestSet value (Issues #10293/#10307/#10496). |

Both failure modes of PR #10625's root cause are forbidden by this contract:

- **Tail-effect loss** — treating the *outer* final statement as a disposable
  return value deletes the whole recorder subtree, so a failing bare `@test`
  records nothing and the process exits 0.
- **Slot clobbering** — keeping the result tail in statement position
  constructs a discarded result object whose store can overwrite an unrelated
  caller slot (Issue #10496).

Pinned by: unit tests in `lowering/stmt/macros/expand.rs`
(`*_10630`, nested `Block`/`LetBlock` tail-removal shape), the green parity
fixture `macros/test_macro_stmt_expr_value_matrix_10630.jl`, the
deliberately-failing halves in `tests/testset_exit_code_8191_tests.rs`
(`statement_position_matrix_10630` / `expression_position_matrix_10630` —
bare failing statements proving the sticky flag), and
`macros/stdlib_macro_expr_position_10293_10307.jl` (expression-position
dispatch). When adding a new statement/value adapter (or extending
`discard_macro_tail_value` to a new wrapper kind), extend the unit tests and
the matrix with the new shape in the same PR.

## Parameter Parsing Flow

### Function Signature Processing

When lowering a function definition, parameters are processed through several functions:

```
lower_function()
    └─> parse_parameter_list()
            └─> parse_parameter()
                    ├─> Identifier → untyped parameter (x)
                    ├─> TypedParameter → parse_typed_parameter() (x::Int64)
                    ├─> SplatParameter → parse_splat_parameter() (args...)
                    ├─> TypeClause → anonymous typed parameter (::Int64)
                    └─> UnaryTypedExpression → anonymous typed with parametric (::Type{T})
```

### Key Functions

#### `parse_parameter()`
Entry point for parsing a single parameter node. Dispatches based on `NodeKind`:
- `Identifier` - Untyped parameter (e.g., `x`)
- `TypedParameter`/`TypedExpression`/`Parameter` - Typed parameter (e.g., `x::Int64`)
- `SplatParameter` - Varargs parameter (e.g., `args...`)

#### `parse_typed_parameter()`
Handles typed parameters including varargs. Key behaviors:
- Detects varargs by checking if node text ends with `...`
- Handles anonymous typed parameters (starts with `::`)
- Processes parametric types like `Complex{Float64}` or `Vector{T}`
- Strips trailing `...` from type names for varargs

#### `parse_splat_parameter()`
Handles explicit splat/varargs parameters (when parser emits `SplatParameter` node).

## Parser Output Variations

The parser (`subset_julia_vm_parser`, a pure Rust reimplementation of the tree-sitter-julia grammar) may emit different node structures for similar Julia code:

### Typed Varargs: `xs::Int64...`

**Pattern 1**: Emitted as `SplatParameter` with `TypeClause`
```
SplatParameter
  ├─ Identifier("xs")
  └─ TypeClause
       └─ Identifier("Int64")
```

**Pattern 2**: Emitted as `Parameter` with trailing `...` in text
```
Parameter("xs::Int64...")
  ├─ Identifier("xs")
  └─ TypeClause
       └─ Identifier("Int64...")
```

Both patterns are handled correctly by `parse_typed_parameter()` (Issue #1678).

### Parametric Type Varargs: `vs::Vector{Int64}...`

```
Parameter
  ├─ Identifier("vs")
  └─ TypeClause
       └─ ParametrizedTypeExpression("Vector{Int64}...")
```

The lowering code strips `...` from the type name.

### Parametric Varargs with `where T` Clause: `(x::T, ys::T...) where T`

Varargs parameters can use type parameters defined in a `where` clause:

```julia
function sum_same_type(x::T, ys::T...) where T
    # All arguments must be the same type T
end

function process(vs::T...) where T
    # All varargs must be type T
end
```

The parser emits these similarly to regular typed varargs, with the `where` clause handled separately during function signature processing.

### Complex Type Varargs: `xs::Complex{Float64}...`

Varargs with complex parametric types work the same as simpler parametric types:

```julia
function sum_complex(x::Complex{Float64}, ys::Complex{Float64}...)
    # Accepts Complex{Float64} varargs
end
```

## TypedParam Structure

The result of parameter parsing is a `TypedParam`:

```rust
pub struct TypedParam {
    pub name: String,                       // Parameter name
    pub type_annotation: Option<JuliaType>, // Type annotation if present
    pub is_varargs: bool,                   // true if parameter is varargs (...)
    pub span: Span,                         // Source location
}
```

## Type Annotation Parsing

Type annotations are parsed through `parse_type_name()`:

| Julia Type | `JuliaType` Result |
|------------|-------------------|
| `Int64` | `JuliaType::Concrete(Int64)` |
| `Float64` | `JuliaType::Concrete(Float64)` |
| `Vector{Int64}` | `JuliaType::Parametric(...)` |
| `Union{Int64,Float64}` | `JuliaType::Union(...)` |
| `Any` | `JuliaType::Any` |
| Unknown types | `JuliaType::Any` |

## Testing

Unit tests for parameter parsing are in `subset_julia_vm_lowering/src/lowering/function/tests.rs`:

```bash
# Run lowering function unit tests
timeout 1800 cargo nextest run --release --lib lowering::function::tests
```

Fixture tests for varargs behavior:
- `tests/fixtures/varargs/varargs_basic.jl` - Basic untyped varargs
- `tests/fixtures/varargs/varargs_typed.jl` - Typed varargs (Issue #1678)
- `tests/fixtures/varargs/varargs_parametric.jl` - Parametric type varargs (Issue #1685)
- `tests/fixtures/varargs/varargs_union.jl` - Union type varargs (Issue #1685)
- `tests/fixtures/varargs/varargs_parametric_where.jl` - Parametric varargs with `where T` clause (Issue #1684)
- `tests/fixtures/varargs/varargs_complex.jl` - Complex type varargs (Issue #1684)
- `tests/fixtures/varargs/varargs_short_form.jl` - Short-form function varargs (Issue #1721)
- `tests/fixtures/varargs/varargs_closure.jl` - Closure capturing varargs-derived variables (Issue #1722)
- `tests/fixtures/varargs/varargs_hof.jl` - Higher-Order Functions with varargs (Issue #1679)

## Common Issues and Solutions

### Issue #1678: Typed Varargs Dispatch Failure

**Problem**: `xs::Int64...` parameters were not recognized as varargs when parser emitted them as `Parameter` nodes instead of `SplatParameter`.

**Solution**: `parse_typed_parameter()` now checks for trailing `...` in node text and sets `is_varargs` accordingly.

### Adding New Parameter Patterns

When adding support for new parameter patterns:

1. Check all possible CST node structures the parser may emit
2. Update `parse_parameter()` to handle new `NodeKind` variants
3. Ensure type annotation parsing handles any special cases
4. Add unit tests in `lowering/function/tests.rs`
5. Add fixture tests in `tests/fixtures/varargs/`

## Closures and Free Variable Analysis

Closures (nested functions that capture variables from enclosing scopes) are handled during compilation through free variable analysis.

### Key Concepts

- **Free Variable**: A variable used inside a function but defined in an enclosing scope
- **Captured Variable**: A free variable that will be stored in the closure's environment
- **Closure Environment**: The set of captured variables passed to a nested function at call time

Lexical `where` binders are part of that environment even though their runtime
values live in a frame's type-binding map rather than an ordinary local slot.
While an active type-parameter frame exists, assigned arrows and nested named
functions retain context-aware lowering, emit dynamic type applications for
binder occurrences, and snapshot the concrete type binding when the closure is
created. The active binder capability does not by itself enable nested-closure
representation for unrelated functions; closure routing and lexical lookup
remain separate decisions (Issue #11031, preserving the #10948 boundary).

### Implementation

#### Compilation Phase (`compile/mod.rs`)

The compiler performs free variable analysis for nested functions:

```
compile_function()
    └─> For each Stmt::FunctionDef (nested function)
            └─> collect_free_variables()
                    └─> Analyze function body for references to outer scope variables
                    └─> Track which variables are "free" (not local, not global)
            └─> Store in SharedCompileContext.closure_captures
```

#### Key Data Structures

**SharedCompileContext.closure_captures** (`compile/context.rs`):
```rust
/// Closure captured variables: maps function name -> set of captured variable names.
/// Used when compiling closures to know which variables to load via LoadCaptured.
pub closure_captures: HashMap<String, HashSet<String>>,
```

**Compiler.in_captured** (`compile/mod.rs`):
```rust
/// Set of variable names that are captured from enclosing scopes (for closures).
/// When compiling a closure body, this contains variables from outer scopes.
pub in_captured: HashSet<String>,
```

#### Instructions

| Instruction | Description |
|-------------|-------------|
| `LoadCaptured(index)` | Load a captured variable from the closure environment |
| `StoreCaptured(index)` | Store a value to a captured variable |
| `MakeClosure(func_idx, capture_count)` | Create a closure with captured values on stack |

#### Variable Resolution Flow

When compiling a variable reference (`Expr::Var`), the compiler checks in order:
1. **Locals** (`self.locals`) - Parameters and locally-defined variables
2. **Captured** (`self.in_captured`) - Variables from enclosing scopes
3. **Globals** (`shared_ctx.global_types`) - Top-level variables
4. **Const Structs** (`shared_ctx.global_const_structs`) - Constant struct instances

If a variable is in `in_captured`, the compiler emits `LoadCaptured` instead of `LoadLocal` or `LoadGlobal`.

### Variable Shadowing

When a parameter or local variable has the same name as a captured variable, the inner definition takes precedence:

```julia
function outer(x)
    function inner(x)  # This x shadows outer x
        x * 2  # Uses inner x, not captured x
    end
    inner
end
```

The shadowed variable is added to `locals`, so it won't be looked up in `in_captured`.

### Nested Closures

Variables can be captured across multiple nesting levels:

```julia
function outer(a)
    function middle(b)
        function inner(c)
            a + b + c  # Captures a from outer, b from middle
        end
        inner
    end
    middle
end
```

Each closure captures the variables it directly needs from its immediately enclosing scope. The compiler tracks captured variables for each function separately.

### Testing

Fixture tests for closure behavior:
- `tests/fixtures/closures/test_nested_closure.jl` - Nested closure capture (Issue #1738)
- `tests/fixtures/closures/test_shadowing.jl` - Variable shadowing (Issue #1738)
- `tests/fixtures/closures/test_captured_mutation.jl` - Mutable captured variables (Issue #1738)

### Common Issues

#### Issue #1734: Undefined Variable in Closure

**Problem**: The undefined variable check in `Expr::Var` handling didn't account for captured variables, causing "Undefined variable" errors for valid closures.

**Solution**: Added `in_captured` check alongside the existing checks for locals, globals, and const_structs.

#### Review Checklist for Closure Changes

- [ ] When adding new variable validation checks, ensure all variable sources are considered (locals, globals, const_structs, **in_captured**)
- [ ] When implementing closures/nested functions, verify both compile-time validation and runtime execution handle captured variables
- [ ] When modifying `Expr::Var` handling, test with nested function scenarios

### Closures in Pure Julia Macro-Expanded Blocks (Issue #2358)

Pure Julia macros (like `@testset`) expand their bodies into `LetBlock` expressions. This creates a special challenge for closure capture analysis because:

1. Variables assigned inside macro-expanded blocks are nested inside `Stmt::Expr { expr: LetBlock { ... } }`
2. Lambda functions defined inside these blocks are lifted to top-level during lowering
3. The lifted lambdas lose their definition context, making capture detection harder

#### How Pure Julia Macro Expansion Works

```julia
# Original code:
@testset "Example" begin
    x = 10
    f = () -> x + 1
    @test f() == 11
end

# After macro expansion (conceptually):
Stmt::Expr {
    expr: LetBlock {
        bindings: [...],
        body: [
            Stmt::Assign { name: "x", ... },
            Stmt::Assign { name: "f", value: FunctionRef("__lambda_1") },
            Stmt::Expr { ... @test ... }
        ]
    }
}

# Lambda lifted to top-level:
Function { name: "__lambda_1", body: [x + 1] }  # x is a free variable
```

#### Implementation Requirements

For closure capture to work in macro-expanded contexts:

1. **Local variable collection must recurse into LetBlock**: The `collect_local_types_with_mixed_tracking()` function must handle `Stmt::Expr` containing `LetBlock` expressions by recursively collecting locals from the LetBlock body.

2. **Pre-analyze lambda captures at module level**: Before compiling functions, analyze all `__lambda_N` functions against module-level locals to populate `closure_captures`.

3. **Check closure_captures in FunctionRef**: When compiling `Expr::FunctionRef`, check if the function name exists in `closure_captures` to emit `CreateClosure` instead of `PushFunction`.

#### Code Path

```
compile_main_block()
    └─> collect_local_types_with_mixed_tracking()
            └─> For Stmt::Expr containing LetBlock
                    └─> collect_expr_locals()  # Recurse into LetBlock body
    └─> Pre-analyze lambda captures (before function compilation)
            └─> For each __lambda_N function
                    └─> analyze_free_variables() against main_locals
                    └─> Store in shared_ctx.closure_captures
    └─> Compile functions (closure_captures already populated)
```

#### Testing

Fixture tests for macro-expanded closure behavior:
- `tests/fixtures/closures/testset_closure_capture.jl` - Closures inside @testset (Issue #2358)

#### Review Checklist for Macro-Expanded Closure Changes

- [ ] Does `collect_local_types_with_mixed_tracking` recurse into LetBlock expressions inside `Stmt::Expr`?
- [ ] Are lambda functions analyzed for captures BEFORE the function compilation loop?
- [ ] When adding new Pure Julia macros, test that closures work inside them
- [ ] When modifying local variable collection, verify it handles nested expression blocks

## Type Narrowing in Conditional Branches

The abstract interpreter performs type narrowing in conditional branches to improve type inference precision. This is implemented in `compile/abstract_interp/conditional.rs`.

### Supported Patterns

Type narrowing is applied when conditions match these patterns:

| Pattern | Then-branch | Else-branch |
|---------|-------------|-------------|
| `isa(val, Type)` | `val` narrowed to `Type` | `val` excludes `Type` |
| `val === nothing` | `val` is `Nothing` | `val` excludes `Nothing` |
| `val !== nothing` | `val` excludes `Nothing` | `val` is `Nothing` |
| `!cond` | Swaps then/else environments | |
| `cond1 && cond2` | Both narrowings applied | Either condition failed |
| `cond1 \|\| cond2` | Either narrowing holds | Both conditions failed |

### Path-Based Narrowing (Issues #1641, #5862)

Beyond simple variables, conditional narrowing also works for field access:

```julia
# Field access narrowing
if obj.field !== nothing
    obj.field + 1  # obj.field is narrowed, not Union{Int, Nothing}
end

# Nested field access narrowing
if obj.inner.value isa Int
    obj.inner.value + 1  # obj.inner.value is narrowed to Int
end
```

#### Implementation

The `extract_narrowable_path()` function converts expressions to trackable paths:

| Expression | Path |
|------------|------|
| `x` (variable) | `"x"` |
| `obj.field` | `"obj.field"` |
| `getfield(obj, :field)` | `"obj.field"` |
| `a.b.c` (nested) | `"a.b.c"` |

Field paths are stored in `TypeEnv` as structured refinements keyed by their
root variable, so path facts can be invalidated when the root or a tracked field
is rebound.

### Known Limitations

1. **Indexed loads** (Issue #9035, parent #9009): Mutable container `getindex`
   expressions are not conditional MustAlias refinement paths. This matches
   upstream Julia 1.12.6 for `a[1] !== nothing ? a[1] : fallback`, where
   `Base.infer_return_type` keeps the element union.
2. **Alias graph coverage** (Issue #9035, parent #9009): Fresh aliases do not
   inherit field path refinements; the guard remains tied to the original
   narrowed slot. This also matches upstream Julia 1.12.6 for `if x.f !==
   nothing; y = x; y.f; end`.

These two shapes are pinned by
`tests/fixtures/type_inference/mustalias_narrowing_limits_9035.jl`. Treating
them as an intentional compatibility boundary resolves #9035; a future
precision improvement should first identify an upstream-compatible
ConditionalsLattice/MustAlias model and open a new implementation issue.

### Testing

Fixture tests for type narrowing:
- `tests/fixtures/type_stability/type_narrowing_field.jl` - Field access narrowing (Issue #1740)
- `tests/fixtures/type_inference/field_access_narrowing.jl` - Field access and `getfield` narrowing (Issues #3520, #3716)
- `tests/fixtures/type_inference/nested_field_guard_refinement_5862.jl` - Nested field guard refinements
- `tests/fixtures/type_inference/nested_field_write_invalidation_5864.jl` - Nested field write invalidation

Unit tests in `compile/abstract_interp/conditional.rs`:
- `test_extract_narrowable_path_*` - Path extraction tests
- `test_split_env_isa_field_access` - Field access isa narrowing
- `test_split_env_nothing_check_field_access` - Field access nothing check

## Function Definition Forms and CST Differences

**Important**: The parser produces different CST node types for semantically equivalent Julia code depending on the syntactic form used. The lowering stage must handle all these variants.

Julia supports two forms of function definitions that are semantically equivalent:

```julia
# Full form (explicit function keyword)
function sum_all(args...)
    sum(args)
end

# Short form (assignment-style)
sum_all(args...) = sum(args)
```

### CST Node Type Differences

| Syntax Feature | Full Form (`function ... end`) | Short Form (`f() = expr`) |
|----------------|-------------------------------|---------------------------|
| Function itself | `FunctionDefinition` | `ShortFunctionDefinition` via `Assignment` |
| Varargs (`args...`) | `SplatParameter` | `SplatExpression` |
| Parameters | Direct `Parameter` nodes | Via `CallExpression` arguments |

### Lowering Implications

When modifying parameter handling in `subset_julia_vm_lowering/src/lowering/function/`:

1. **Always test both forms**: After changes, verify both full-form and short-form function definitions work
2. **Check `parse_parameter()`**: Ensure it handles all relevant CST node types
3. **Varargs handling**: Both `SplatParameter` AND `SplatExpression` must produce equivalent `TypedParam::varargs()`

**Historical Bug (Issue #1721):** `parse_parameter()` only handled `SplatParameter` (from full-form functions), not `SplatExpression` (from short-form functions). This caused short-form varargs functions to fail with "Undefined variable" errors.

### Varargs Edge Cases and Regression Prevention (Issue #1724)

Several varargs edge cases were identified during regression testing. These are now covered by dedicated fixture tests:

#### Short-Form Varargs (Issue #1721)

Short-form function definitions use `SplatExpression` nodes instead of `SplatParameter`. Both must produce identical `TypedParam::varargs()` output. Regression test: `varargs_short_form.jl`.

```julia
# Both forms must work identically:
sum_all(args...) = sum(args)         # Short-form (SplatExpression)
function sum_all(args...)             # Full-form (SplatParameter)
    sum(args)
end
```

#### Closure Capturing Varargs-Derived Variables (Issue #1722)

When a function with varargs creates local variables derived from the varargs parameter (e.g., `base_sum = sum(base)`), closures defined in that function must be able to capture those derived variables. Regression test: `varargs_closure.jl`.

```julia
function make_adder(base...)
    base_sum = sum(base)       # Derived from varargs
    function adder(x)
        x + base_sum           # Must capture base_sum
    end
    adder
end
```

#### Code Review Checklist for Varargs Changes

When modifying varargs handling code:
- [ ] Test both short-form (`f(args...) = expr`) and full-form (`function f(args...) ... end`) definitions
- [ ] Test closures that capture varargs-derived variables
- [ ] Test HOF patterns that pass varargs to other functions (`apply(f, args...) = f(args...)`)
- [ ] Verify variable resolution in nested function scopes
- [ ] Test with zero, one, and multiple varargs arguments

### SplatParameter/SplatExpression Duality Checklist (Issue #2253)

**MANDATORY**: When handling splat/varargs nodes in the lowering code, you MUST handle BOTH node types together:

| Full Form (`function ... end`) | Short Form (`f() = expr`) | Context |
|-------------------------------|---------------------------|---------|
| `SplatParameter` | `SplatExpression` | Positional varargs (`args...`) |
| `SplatParameter` | `SplatExpression` | Kwargs varargs (`; kwargs...`) |

**Why this matters**: The bug pattern is that when adding splat handling to a new code path, only `SplatParameter` is considered (the "obvious" function definition node), while `SplatExpression` (from short-form) is forgotten. This has caused bugs in:
- Issue #1721: Positional varargs in short-form functions
- Issue #2242: Kwargs varargs in short-form functions

**Checklist when adding or modifying splat handling**:

- [ ] When you `match` on `NodeKind::SplatParameter`, immediately add `| NodeKind::SplatExpression`
- [ ] Add a comment: `// per Issue #2253 duality requirement`
- [ ] Search for other `SplatParameter` occurrences in the same file and verify they also handle `SplatExpression`
- [ ] Write fixture tests that cover BOTH full-form and short-form syntax

**Audit command to verify duality**:
```bash
# Count should be roughly equal
rg -c "SplatParameter" subset_julia_vm_lowering/src/lowering/function -g '*.rs'
rg -c "SplatExpression" subset_julia_vm_lowering/src/lowering/function -g '*.rs'
```

**Code pattern to use**:
```rust
// CORRECT: Handle both node types together
NodeKind::SplatParameter | NodeKind::SplatExpression => {
    // Handle varargs
    // per Issue #2253 duality requirement
}

// WRONG: Only handling one node type
NodeKind::SplatParameter => {
    // This will miss short-form functions!
}
```

## Parser-Lowering CST Contract (Issue #2148)

The parser and lowering stages communicate through CST nodes. A class of silent bugs occurs when these stages **disagree on how many children a node contains**: the parser packs multiple children into one node, but the lowering only extracts the first child. This causes data to be silently dropped at runtime with no compile error.

### CST Multi-Child Node Contract Table

This table documents which CST nodes can contain multiple children of the same kind and how the lowering stage handles them.

| CST Node | Child Kind | Can Have Multiple? | Lowering Function | File | Handles All? |
|---|---|---|---|---|---|
| `ForClause` | `ForBinding` | Yes | `parse_for_clause_bindings()` | `lowering/expr/collection.rs` | Yes |
| `ParameterList` | `Parameter`, `KwParameter`, `SplatParameter` | Yes | `lower_function_definition()` | `lowering/function/` | Yes |
| `ArgumentList` | positional & keyword args | Yes | `lower_argument_list()` | `lowering/expr/call.rs` | Yes |
| `TupleExpression` | elements | Yes | `collect_index_nodes()` / `lower_arrow_function()` | `lowering/expr/collection.rs`, `lowering/expr/call.rs` | Yes |
| `VectorExpression` | elements | Yes | `lower_vector_expr()` | `lowering/expr/collection.rs` | Yes |
| `MatrixExpression` | `MatrixRow` | Yes | `lower_matrix_expr()` | `lowering/expr/collection.rs` | Yes |
| `ComprehensionExpression` | `ForClause`, `IfClause` | Yes | `lower_comprehension_expr()` | `lowering/expr/collection.rs` | Yes |
| `IfStatement` | `ElseifClause`, `ElseClause` | Yes | `lower_if_stmt()` | `lowering/stmt/control_if.rs` | Yes |
| `LetBindings` | `Assignment` | Yes | `lower_let_expr()` | `lowering/expr/misc.rs` | Yes |
| `TypeParameterList` | `TypeParameter`, constraints | Yes | `parse_where_clause()` / `parse_type_parameters()` | `lowering/function/where_clause.rs`, `lowering/struct_.rs` | Yes |
| `WhereClause` | `Identifier`, constraints | Yes | `parse_where_clause()` | `lowering/function/where_clause.rs` | Yes |
| `Block` | statements | Yes | `lower_block()` | `lowering/stmt/mod.rs` | Yes |
| `StructBody` | fields, inner constructors | Yes | `parse_struct_body()` | `lowering/struct_.rs` | Yes |
| `CurlyExpression` | type arguments | Yes | `lower_parametrized_type()` | `lowering/expr/mod.rs` | Yes |

### Risky Patterns to Watch

These patterns in the lowering stage use `.first()`, `.find()`, or early `break`/`return` and may silently drop children:

| Pattern | Location | Node Type | Risk | Notes |
|---|---|---|---|---|
| `parse_for_clause()` | `expr/collection.rs` | `ForClause` | Low | Deprecated fallback; `parse_for_clause_bindings()` should be used instead |
| `.first()` on interpolation children | `expr/literal.rs` | `StringInterpolation` | Low | Interpolation nodes have single expression child by design |
| `.find()` for `ArgumentList` | `expr/misc.rs` | broadcast call args | Low | CST structurally allows only one ArgumentList per call |
| `.first()` on inner splat children | `expr/call.rs` | `SplatExpression` inner | Low | Splat wraps single expression by design |
| `break` after first type annotation | `function.rs` | `SplatExpression` typed | Low | Type clause has single type by design |

### Code Review Checklist for Parser-Lowering Boundary

When modifying either the parser (`subset_julia_vm_parser/src/parser/`) or the lowering stage (`subset_julia_vm_lowering/src/lowering/`):

1. **Verify CST node children**: After parsing, check how many children each node type actually contains (single vs. multiple)
2. **Check lowering extraction functions**: Ensure lowering functions that extract data from CST nodes handle ALL children, not just the first match
3. **Avoid early-return on first match**: Functions that `return` on the first matching child are fragile -- prefer collecting all matches first
4. **Test with multi-element variants**: If a CST node can contain N children, always test with N=1 AND N>1
5. **Treat unused helper warnings as potential bugs**: If a helper function that processes all children exists but is unused (and a single-child version is used instead), investigate why

**Historical Bug (Issue #2143):** `parse_for_clause()` returned only the first `ForBinding`, silently dropping subsequent bindings in multi-variable comprehensions like `[i*j for i in 1:3, j in 1:3]`. The fix was to use `parse_for_clause_bindings()` which iterates ALL `ForBinding` children.

**Generator representation (Issue #9200):** generator *syntax* `(f(x) for x in it …)` desugars to the upstream `Base.Generator`/`Iterators.Filter`/`Iterators.Flatten`/`Iterators.product` shapes (S1–S4), but the compiler collapses them onto a native `MakeGenerator`/`GeneratorCallable` runtime representation, and eager bracket comprehensions `[…]` compile to a dedicated array-building loop (the "eager FilterMap fast path"). S6 measured whether these can be retired in favour of the pure `iterate` protocol and decided **KEEP** (5–21× / 2.5–5× regressions, plus correctness cases the pure-iterate route cannot reproduce). See `GENERATOR_REPRESENTATION.md` for the decision, A/B numbers, and epic summary.

## Call Target NodeKind Registry (Issue #2271)

The `resolve_call_target` function in `lowering/expr/call.rs` is the single source of truth for recognizing call target patterns. Both `lower_call_expr_with_ctx` and `lower_call_expr` delegate to it.

| NodeKind | Pattern | Result | Example |
|----------|---------|--------|---------|
| `FieldExpression` (module) | `Module.func(args)` | `ModuleCall` | `Base.sin(x)` |
| `FieldExpression` (field) | `obj.f(args)` | `IndirectCall` via LetBlock | `config.handler(x)` |
| `IndexExpression` | `coll[i](args)` | `IndirectCall` via LetBlock | `fns[1](x)` |
| `Identifier` | `func(args)` | `DirectCall` | `sin(x)` |
| `ParametrizedTypeExpression` | `Type{T}(args)` | `DirectCall` | `Point{Float64}(1.0)` |
| `Operator` (`!`) | `!(expr)` | `UnaryNot` | `!(true)` |
| `Operator` (broadcast) | `.op(a, b)` | `DirectCall` | `.*(a, b)` |

### Adding New Call Target Kinds

When adding a new `NodeKind` as a call target:

1. Add handling in `resolve_call_target` — this is the **only** place to modify
2. Both `lower_call_expr_with_ctx` and `lower_call_expr` will automatically pick up the change
3. Add a fixture test in `tests/fixtures/` to verify the new pattern
4. Update this table

### Quick Audit

```bash
# Verify the helper definition plus both call-lowering call sites (3 matches)
rg -n 'fn resolve_call_target|resolve_call_target\(' subset_julia_vm_lowering/src/lowering/expr/call.rs
```

## Contextual Keywords in Indexing

Julia supports `begin` and `end` as contextual keywords within array indexing expressions.
The lowering phase transforms these keywords into function calls.

### Transformations

| Context | Keyword | Transformation | Example |
|---------|---------|---------------|---------|
| 1D indexing | `end` | `lastindex(array)` | `a[end]` → `a[lastindex(a)]` |
| 1D indexing | `begin` | `firstindex(array)` | `a[begin]` → `a[firstindex(a)]` |
| Multi-dim indexing | `end` | `lastindex(array, dim)` | `m[1, end]` → `m[1, lastindex(m, 2)]` |
| Multi-dim indexing | `begin` | `firstindex(array, dim)` | `m[begin, 2]` → `m[firstindex(m, 1), 2]` |

### Implementation

Both transformations are implemented as parallel functions in `collection.rs`:

- `replace_end_with_lastindex(expr, array, dim)` - Transforms `end` → `lastindex(array)` or `lastindex(array, dim)`
- `replace_begin_with_firstindex(expr, array, dim)` - Transforms `begin` → `firstindex(array)` or `firstindex(array, dim)`

The `dim` parameter is:
- `None` for 1D indexing (single index) → uses `lastindex(array)` / `firstindex(array)`
- `Some(d)` for multi-dimensional indexing → uses `lastindex(array, d)` / `firstindex(array, d)`

These functions recursively process expressions to handle compound cases:
- `a[end-1]` → `a[lastindex(a) - 1]`
- `a[begin+1]` → `a[firstindex(a) + 1]`
- `a[begin:end]` → `a[firstindex(a):lastindex(a)]`
- `m[begin, end]` → `m[firstindex(m, 1), lastindex(m, 2)]`
- `m[end-1, begin+1]` → `m[lastindex(m, 1) - 1, firstindex(m, 2) + 1]`

### Dimension Tracking Pattern (Issue #2349)

For multi-dimensional indexing, the lowering code:

1. Collects all index nodes first to determine total number of dimensions
2. Enumerates indices with their dimension number (1-based, matching Julia's convention)
3. Passes the dimension to replacement functions when `total_indices > 1`

```rust
// In lower_index_expr():
let total_indices = all_index_nodes.len();

for (dim_index, idx_node) in all_index_nodes.into_iter().enumerate() {
    let dim = if total_indices > 1 {
        Some(dim_index + 1)  // Julia uses 1-based dimension indexing
    } else {
        None
    };
    let idx_expr = replace_end_with_lastindex(idx_expr, &array, dim);
    let idx_expr = replace_begin_with_firstindex(idx_expr, &array, dim);
    // ...
}
```

### Julia Library Support

The dimension-aware functions are implemented in `julia/base/range.jl`:

```julia
# 1-argument forms (used for 1D indexing)
firstindex(arr) = 1
lastindex(arr) = length(arr)

# 2-argument forms (used for multi-dimensional indexing)
firstindex(arr, d::Int64) = first(axes(arr, d))
lastindex(arr, d::Int64) = last(axes(arr, d))
```

### Symmetry Requirement

**When adding support for any new indexing keyword**:
1. Implement BOTH the parser disambiguation (in lexer/parser) AND the lowering transformation
2. Ensure the parser lookahead covers all binary operators that can follow the keyword
3. Add recursive handling for ALL `Expr` variants that can contain subexpressions
4. Support both 1D and multi-dimensional indexing contexts with dimension tracking

### Code Review Checklist for Index Keyword Changes

- [ ] Test with 1D arrays (single index)
- [ ] Test with 2D+ arrays (multiple indices)
- [ ] Test with arithmetic expressions (`end-1`, `begin+1`)
- [ ] Test with range expressions (`begin:end`)
- [ ] Ensure dimension parameter is propagated through all recursive calls
- [ ] Verify both 1-arg and 2-arg Julia library functions exist

### Related Issues

- Issue #2310: Original `begin` keyword support
- Issue #2325: begin/end symmetry prevention
- Issue #2349: Dimension-aware begin/end support (fixed in PR #2362)

### Fixture Tests

- `tests/fixtures/array/begin_indexing.jl` - 1D begin keyword tests (Issue #2310)
- `tests/fixtures/array/begin_end_multidim.jl` - Multi-dimensional begin/end tests (Issue #2349)

## Nested Field Assignment (Issue #2309, #2314)

### IR Asymmetry

There's an asymmetry between field read and write in the Core IR:

| Operation | IR Node | Object Representation |
|-----------|---------|----------------------|
| Field access (read) | `Expr::FieldAccess { object: Box<Expr>, ... }` | Recursive `Expr` (supports nesting) |
| Field assign (write) | `Stmt::FieldAssign { object: String, ... }` | Flat `String` (no nesting) |

This asymmetry means `o.inner.field = value` cannot be represented directly in the IR.

### Temporary Variable Decomposition

The lowering phase works around this by decomposing nested assignments into temporary variables:

```julia
# Original Julia code:
o.inner.value = 42

# Lowered IR (conceptually):
__temp = o.inner   # FieldAccess can be nested
__temp.value = 42  # FieldAssign uses flat object name
```

### Four Parallel Code Paths

Nested field assignment is handled by **four** code paths that must stay in sync:

| Function | Context | Handles |
|----------|---------|---------|
| `lower_field_assignment()` | Direct | `o.a.b = value` |
| `lower_field_assignment_with_ctx()` | Lambda | `() -> (o.a.b = value)` |
| `lower_compound_assignment()` | Direct | `o.a.b += value` |
| `lower_compound_assignment_with_ctx()` | Lambda | `() -> (o.a.b += value)` |

Each path uses the helper functions:
- `extract_field_target()` - Get field name from expression
- `extract_nested_field_target()` - Decompose nested field access with temp vars

### Code Review Checklist

When modifying field assignment code:

- [ ] Check both simple (`obj.field`) and nested (`obj.a.b.field`) expressions
- [ ] Update all four code paths (2 direct + 2 compound)
- [ ] Test in both direct and closure/loop contexts
- [ ] Verify `_with_ctx` variants get the same updates as base versions

### Fixture Tests

- `struct/nested_field_assignment.jl` - Basic nested assignment (Issue #2309)
- `struct/nested_field_context.jl` - Closure and loop contexts (Issue #2314)

### Long-term Improvement

Consider changing `Stmt::FieldAssign { object: String, ... }` to use `Expr` for the object,
making the IR self-consistent between read and write paths. This would eliminate the need
for temporary variable decomposition.

## Abstract Interpreter: Block Inference and Return Types (Issue #2255)

The abstract interpreter infers types for blocks of statements. A critical distinction exists between **implicit block values** and **explicit returns**.

### The Two Concepts

1. **Implicit block value** — The type of the last statement's evaluated expression. In Julia, every expression has a value, so a block like:
   ```julia
   begin
       x = 1
       y = 2
   end
   ```
   evaluates to `2` (the implicit value of the last assignment).

2. **Explicit return** — A `return` statement that explicitly exits the enclosing function:
   ```julia
   function foo()
       if cond
           return 42  # Explicit return
       end
       0  # Implicit function value
   end
   ```

### `infer_block` vs `infer_block_explicit_return_only`

| Function | Returns | Use For |
|----------|---------|---------|
| `infer_block` | Implicit value OR explicit return | if/else branches, begin blocks |
| `infer_block_explicit_return_only` | Only explicit `return` types | Loop bodies (while, for, foreach) |

### Why Loop Bodies Need Special Handling

Loop bodies should NOT contribute their implicit value to the function's return type:

```julia
function foo()
    i = 0
    while i < 3
        i = i + 1  # Implicit value: Int64
    end
    return "done"  # Actual return: String
end
```

If we used `infer_block` for the while body, it would incorrectly infer `i = i + 1` as a potential return value (`Int64`), conflating it with the actual `"done"` return (`String`).

**Historical Bug (Issue #2241)**: The loop body handlers called `infer_block` and propagated any non-Nothing result as a function return. This caused closures returned from functions with loops to be typed as the loop's implicit value type (e.g., `Int64` instead of `Closure`), breaking calls to the closure.

### Code Review Checklist

When modifying abstract interpreter code:

- [ ] Loop bodies (`while`, `for`, `foreach`) must use `infer_block_explicit_return_only`
- [ ] If/else branches may use `infer_block` (implicit values propagate)
- [ ] When adding new loop-like constructs, use `infer_block_explicit_return_only` for the body
- [ ] When checking `!= Nothing` on block results, verify whether you mean "explicit return" or "block value"

### Source File

- `subset_julia_vm_compile/src/compile/abstract_interp/engine/` - Contains both `infer_block` and `infer_block_explicit_return_only`

### Fixture Tests

- `closures/test_closure_with_loops.jl` - Closures with while/for loops (Issue #2241)
- `closures/nested_loops_closure.jl` - Closures with nested loops (Issue #2255)

## Dual Macro Lowering Paths (Issue #2604)

The macro expansion system has TWO parallel code paths that must handle the same set of macros:

| Function | File | When Used |
|----------|------|-----------|
| `lower_macro_expr()` | `lowering/expr/macros/` | No-context path: function bodies lowered via `lower_block` without `lambda_ctx` |
| `lower_macro_expr_with_ctx()` | `lowering/expr/macros/` | Context path: expressions lowered inside lambdas, closures, or top-level with `lambda_ctx` |

### The Divergence Risk

When adding support for a new Base macro:

1. The `_with_ctx` path naturally has access to `lambda_ctx`, so new macros are typically added there first.
2. The no-context path (`lower_macro_expr`) falls back to creating a temporary `LambdaContext::new()` to call `expand_base_macro_expr`. This fallback was added to handle `@nospecialize`, `@simd`, `@inline`, etc.
3. **Risk**: If a macro requires information from `lambda_ctx` (e.g., user-defined macros, `@__FILE__`, `@__DIR__`), it will work in the `_with_ctx` path but silently fail or produce wrong results in the no-context path.

### Macros Handled in Both Paths

| Macro | `lower_macro_expr` | `lower_macro_expr_with_ctx` | Notes |
|-------|--------------------|-----------------------------|-------|
| `@isdefined` | Direct handling | Direct handling | Same logic in both |
| `@__dot__` / `@.` | Direct handling | Direct handling | Same logic in both |
| Base macros | Via `expand_base_macro_expr` with temp ctx | Via `expand_base_macro_expr` | Temp ctx has limited info |
| `@macroexpand` | Not handled | Direct handling | Only in context path |
| `@__FILE__` | Not handled | Direct handling | Requires real `lambda_ctx` |
| `@__DIR__` | Not handled | Direct handling | Requires real `lambda_ctx` |
| `@__LINE__` | Not handled | Direct handling | Uses span |
| `@view` / `@views` | Not handled | Direct handling | Requires `lambda_ctx` |
| `@static` | Not handled | Direct handling | Requires `lambda_ctx` |
| User macros | Not handled | Via `lambda_ctx.has_macro()` | Requires real `lambda_ctx` |

### Pattern to Follow

When adding a new macro that works with a temporary context:
1. Add handling to `lower_macro_expr_with_ctx` (the context path)
2. If the macro does NOT require `lambda_ctx` state (no user macros, no file info), also verify it works via the Base macro fallback in `lower_macro_expr`
3. If the macro DOES require `lambda_ctx` state, add it as a direct match arm in `lower_macro_expr` with appropriate fallback behavior

### Audit Command

```bash
# Compare direct match arms between the no-context and context macro paths
rg -n 'match macro_name\.as_str\(\)|"[^"]+"\s*=>' subset_julia_vm_lowering/src/lowering/expr/macros/mod.rs
```

### Prevention Test

`tests/fixtures/macros/test_base_macro_in_function.jl` — Verifies that Base macros work inside function bodies (the no-context lowering path).

## Static Stdlib/Base Macro Quote-Expansion Hygiene (Issue #10242, epic #10253)

`subset_julia_vm/src/julia/stdlib/**/*.jl` macros (registered via the
`STDLIB_MACROS` registry — see `stdlib_loader.rs`) called in **statement
position** are expanded by the **static template-substitution path**:
`quote_constructor_to_code[_with_varargs|_with_locals]` in
`lowering/expr/quote/code_generation.rs`, which walks the macro's `quote`
constructor and converts it directly into executable IR. This is a separate
code path from the **dynamic, VM-backed** expansion
(`subset_julia_vm/src/macro_runtime.rs`) — used for user-defined macros,
bundled third-party-package macros, ALL Base (`julia/base/*.jl`) macros
(since commit `ef83266a2e` "Unify Base macro expansion runtime",
2026-06-25; see the correction note under "Scope" below), and stdlib macros
in *expression* position (`expand_stdlib_macro_expr`, Issues
#10293/#10307) — which compiles and runs the macro body on a real `vm::Vm`.
See "Dual Macro Lowering Paths" above for a different (unrelated) two-path
split at the expr-lowering layer, and "Static Pass-2 reachability" in the
#10627 section below for the complete verified engine-routing map
(Issue #10916).

### What gets renamed

The static path implements upstream-style macro hygiene via
`HygieneContext` (`lowering/expr/quote/hygiene.rs`), applied in two passes
per macro expansion (`quote_constructor_to_code_with_hygiene`,
`code_generation.rs`):

1. **Pass 1 — collect**: `collect_introduced_vars`
   (`lowering/expr/quote/handlers.rs`) walks the quote constructor and
   registers every **non-escaped** local the macro introduces:
   - `local x` / `local x = v` declarations (`ExprHead::Local`)
   - assignment targets (`ExprHead::Assign`, i.e. `x = v`)
   - the `catch` variable of a `try`/`catch` (`ExprHead::Try`, added by
     #10242)
   - a `for`-loop's own binding variable (`ExprHead::For`), a `let`
     binding (`ExprHead::Let`), and recursion into a `while` body
     (`ExprHead::While`) — given explicit arms by #10626 for documentation
     and robustness. These were already covered *before* #10626 by
     incidental structural coincidence (a `for`/`let` binding is itself an
     `Expr(:(=), var, value)`, the same shape the generic `_` fallback
     already recurses into and the `Assign` arm above already registers),
     so making them explicit is a completeness/clarity change with no
     behavior change — see "Known limitation" below for why no scope-stack
     rewrite was needed to make this correct.
   - `Expr(:function, ...)`/`where`-bound nested function definitions and
     `Expr(:comprehension, ...)`/`Expr(:generator, ...)` expressions remain
     **unsupported** in this static path's Pass 2 codegen (both hit the
     catch-all `UnsupportedExpression` error) — no current sjulia
     stdlib macro's `quote` body uses any of these forms, so adding
     codegen for them here has no reachable test surface; deferred to
     follow-up #10916 (see "Static Pass-2 reachability" below for the
     verified routing map behind that deferral). The **dynamic** path
     (below) already supports all four end-to-end, since it is what every
     user-defined, bundled-package, and Base macro (the only reachable
     users of these forms) actually goes through.

   Each registered name gets a fresh gensym via `HygieneContext::gensym`
   (format `"#<name>#<counter>"`, a global `AtomicU64` counter shared with
   `lowering/expr/macros::GENSYM_COUNTER`), so two expansions of the *same*
   macro invocation site, or two invocations of the *same* macro anywhere in
   a program, always get distinct renamed locals — they can never collide
   with each other or with a user/global variable of the same base name.

2. **Pass 2 — rewrite**: `quote_constructor_to_code_with_hygiene` converts
   the quote constructor into IR; every `SymbolNew`/`Var` reference is
   passed through `HygieneContext::resolve`, which substitutes the gensym'd
   name if the identifier was registered in pass 1, or leaves it unchanged
   otherwise (macro parameters, calls to other functions, and any name the
   macro body never assigns/declares/catches).

### How `esc($x)` is preserved as-is

`esc(...)` marks a sub-expression as the user's own code (interpolated
argument or explicitly escaped by the macro author) — it must resolve in the
**caller's** scope, never be renamed. `collect_introduced_vars` and
`quote_constructor_to_code_with_hygiene` both special-case the `esc` call
head: when recursing into an `esc(...)` argument, they flip
`HygieneContext.in_escaped` to `true` for that subtree (`enter_escaped()` /
the `in_esc` recursion parameter). While `in_escaped`, `register_local`
and `resolve` are no-ops that return the name unchanged and record nothing —
so a variable read or assigned inside `esc(...)` is never a hygiene-rename
candidate, and a use of the same bare name *outside* `esc(...)` elsewhere in
the same quote can still be renamed independently. `$x`-interpolated
argument nodes (macro parameters substituted from the call site, handled by
`substitute_quote_template_params`/the `params`/`args` lowering) are lowered
straight from the caller's CST and never pass through the constructor-level
hygiene walk at all, so they are unaffected by rename bookkeeping by
construction.

### "Flat namespace": not actually a limitation vs. upstream (Issue #10626)

`HygieneContext.renames` is a single flat map for the whole macro
expansion, not a scope stack: `register_local` for a name always
overwrites any earlier registration of that same base name within the
*same* invocation. Two *independent* invocations of a macro (each call site
gets its own fresh `HygieneContext::new()`) never collide — only a single
macro body that introduces the **same** bare name twice at different
nesting levels (e.g. two separate un-escaped `try ... catch e ... end`
blocks both named `e` in one `quote`) shares one gensym for all
resolutions of that name in that expansion.

This was originally documented (by #10253) as a limitation relative to
upstream Julia, to be fixed by a scope-aware rename stack. Investigating
that for #10626 found the opposite: `@macroexpand` on upstream Julia shows
its **own** hygiene is *also* a single flat rename per literal name per
macro expansion — e.g. a `for i in ...` loop and an unrelated sibling
`i = ...` assignment in the *same* quote are renamed to the exact *same*
gensym (`var"#2#i"` in one probe), and a function parameter that happens to
share a name with a sibling quote-local also collapses onto that sibling's
gensym. Upstream relies on the *scoping* of the construct that introduces
the name (a `for`/`let`/`function` body genuinely shadows or exits its own
binding) to keep same-renamed-but-different bindings independent, not on
giving them distinct names. So a flat map is the upstream-faithful design,
not a gap to close — `HygieneContext` does not need a scope-aware rename
stack, and none was added.

The corollary: renaming alone cannot fix a scope in sjulia that does not
itself correctly shadow/restore a pre-existing same-named binding. `let`
already does this correctly in sjulia (verified empirically — a same-named
sibling quote-local is left untouched after the `let` exits), so completing
its hygiene coverage is purely a naming-completeness exercise with no
behavioral bug to fix. `for`-loop and comprehension/generator induction
variables do **not** correctly shadow a pre-existing same-named local in
sjulia today (Issue #10903, a general lowering/VM scoping defect, not a
hygiene one) — hygiene renaming cannot paper over that, matching upstream's
own reliance on scoping rather than naming for these forms. Fixing #10903
is a prerequisite for full same-scope-collision parity on `for`/
comprehension; this epic's fixture coverage therefore verifies
non-colliding usage and `esc` preservation for comprehension/generator, not
a same-name-collision scenario for their own induction variable (see
`tests/fixtures/macros/quote_comprehension_hygiene_10626.jl`).

**Function parameters and `where` type parameters were the one case where
"upstream is flat" did NOT license copying that renaming into this
mechanism** — discovered by an adversarial review during #10626 and fixed
by *reverting* the registration before merge: unlike `for`/`let`/assignment
targets, a function parameter is scoped to the *function body*, not the
whole macro expansion, but `rename_quote_local_symbols` (dynamic path) /
`HygieneContext::resolve` (static path) were pure flat, whole-tree
substitutions with no notion of "inside this function's body." Registering
a parameter name (e.g. `sort`) for rename under that flat mechanism would
rewrite *every* non-esc occurrence of that bare name anywhere in the
expansion — including an unrelated sibling reference to `Base.sort` outside
the function — breaking it. (Regression MWE: a macro's quote defining
`function f(sort) sort + 1 end` alongside a sibling `sort([3, 1, 2])` call;
upstream and the pre-#10626 baseline both evaluate the sibling as
`Base.sort`, but a since-reverted version of this change broke it with
`Unknown function: sort##m#N`.) Upstream can rename parameters safely
because its rename is scope-aware in *resolution*, not just in collection —
#10626 concluded that since this mechanism's rename step was not, function
parameters and `where` type parameters had to be deliberately left
unregistered in both paths (sjulia's own function-call-frame scoping
already isolates them correctly at runtime without any renaming, so this
was safe to defer, not a correctness bug).

**Issue #10925 closed that gap in the dynamic path** by making the rename
mechanism itself scope-aware — see "Scope-aware dynamic-path rename
environment (Issue #10925)" below — rather than continuing to leave
parameters/`where`-vars unregistered. The static path is unaffected (its
Pass-2 codegen for `Expr(:function, ...)`/`Expr(:where, ...)` remains
unsupported and unreachable, Issue #10916), so `HygieneContext::resolve`'s
flat design is untouched. See
`apply_quote_function_hygiene_does_not_break_sibling_global_call_sharing_a_param_name_10626`
in `macro_runtime.rs` for the regression guard that stayed green through
this change, and the six `..._10925` tests alongside it for the new
positive coverage (parameter/`where`-var IS renamed within its own scope;
sibling shadowing reuses the enclosing gensym; sibling non-shadowing mints
a fresh, distinctly-scoped one).

#### Scope-aware dynamic-path rename environment (Issue #10925)

`rename_quote_local_symbols` no longer takes a single flat
`HashMap<String, String>`; it takes a `RenameEnv` (`macro_runtime.rs`):

- `base`: the SAME whole-expansion flat map as before (assignment
  targets/`local`-decls/`catch`-vars/function-names) — unchanged, since
  #10626 already established those are safe to treat as expansion-wide.
- `scopes: Vec<HashMap<String, String>>`: a stack of frames, pushed when the
  rename walk descends into a `function` definition's own parameter list +
  body (long form `function f(...) ... end` AND short form
  `f(...) = ...`, both `where`-wrappable), or a standalone `where` clause's
  own bound-type-variable list + wrapped expression, and popped again on
  the way back out.
- `resolve(name)` scans `scopes` innermost-to-outermost, then falls back to
  `base`; a name found nowhere is left unchanged (the sibling-safety
  invariant the #10626 regression guard depends on).
- `ensure_scoped(name)` reuses an already-resolvable gensym (matching
  upstream's own `@macroexpand`-verified behavior: a parameter/`where`-var
  that shadows an outer already-renamed binding — an enclosing function's
  own same-named parameter, or a whole-expansion quote-local — collapses
  onto that SAME gensym text even though it is a genuinely distinct binding
  at runtime) or mints a fresh one scoped to the current frame otherwise
  (two sibling, non-nested functions' same-named-but-unrelated parameters
  get DIFFERENT gensyms, since each function's own frame is popped before
  the next sibling is processed).

A `function`'s own frame registers BOTH its parameter names and (if
`where`-wrapped, including chained `where T where S`) its bound
type-variable names together, since upstream visibility treats them as one
scope spanning the signature's type annotations AND the function body — not
two nested scopes. A standalone `where` (not part of a function definition,
e.g. `Vector{T} where T` used as a bare type value) pushes its own frame
scoped only to its own subtree UNLESS every one of its bound-var names is
ALREADY resolvable via the currently-active scope stack (i.e. it is a
function's own `where`-clause, walked while that function's frame is still
open), in which case no extra frame is pushed and the names simply resolve
against the enclosing frame — this is what lets the same bound variable
stay visible in both the function's signature and its body despite the
`where` node's own subtree covering only the signature.

Verified against upstream `@macroexpand` + execution for every case this
mechanism newly handles: parameter renamed within its own scope; parameter
sharing a name with a sibling GLOBAL reference (the regression-guard MWE)
stays unrenamed there; parameter shadowing an outer quote-local reuses that
gensym; two sibling functions' same-named parameters get distinct gensyms;
a nested function's parameter shadowing its enclosing function's own
same-named parameter reuses that gensym (closure-capture semantics fall out
of the same frame-stack lookup, with no special case needed); a `where`
binder may shadow a builtin type name safely. See
`tests/fixtures/macros/quote_scope_aware_param_where_hygiene_10925.jl` for
the end-to-end fixture and `macro_runtime.rs`'s `..._10925`-suffixed unit
tests for the Value-tree-level probes mirroring each upstream case.

**`QuoteBindingRole` was deliberately NOT extended with new
`FunctionParams`/`WhereBinder` roles** for this issue, even though the
Issue text initially suggested extending the shared #10627 classifier.
`quote_binding_role(Function)` already returns `FunctionName` — one role per
`ExprHead` — so a param/`where`-var extraction keyed off a *different* head
(`Call`'s trailing args, `Where`'s trailing args) does not fit that
one-role-per-head shape without either overloading `FunctionName` to mean
two different things or adding a role the static path can never consult
(its Pass-2 codegen for these heads stays unsupported per #10916, so a
static-side classification would be unreachable, dead code). Instead,
`function_def_param_and_where_names`/`where_bound_var_name`/
`function_param_hygiene_name` live in `macro_runtime.rs` as dynamic-path-only
tree navigation, mirroring how #10627 itself kept per-engine tree-walk
mechanics separate while unifying only the head-role *decision* (see
"What did NOT get merged, and why" below) — the same rationale applies
here: the decision this issue makes (extracting param/`where`-var names
from a function signature) has no static-path counterpart to converge with.

### Scope: static path vs. dynamic path (Issue #8064, #10242, #10626)

This hygiene pass covers only the **static** stdlib-macro quote-expansion
path. The **dynamic** `macro_runtime.rs` path — user-defined macros,
bundled-package macros, ALL Base macros, and stdlib macros in expression
position (`base_macro_preserves_statement_value` in
`lowering/stmt/macros/expand.rs`, Issue #7764, no longer selects static
vs. dynamic: since commit `ef83266a2e`, 2026-06-25, it only selects the
value-preserving `expand_expr` entry vs. the `expand_stmt` entry WITHIN
the dynamic engine for its named `show`/`time`/… subset, whose quote-body
tail value must survive as the macro's own statement-position result) —
has its own, structurally separate hygiene mechanism
(`collect_quote_local_names`/`rename_quote_local_symbols`, Issue #8064) that
operates on runtime `Value`/`ExprValue` trees (the *executed* quote's
result) rather than the static path's pre-execution constructor `Expr`
tree — the two cannot share a single Rust implementation without either a
lossy adapter between representations or forcing one path to mimic the
other's data model, so #10626 kept them as two implementations of the same
*decision table* (which `ExprHead`s introduce which locals) rather than
literally shared code. Issue #10627 converged that decision table itself
(not the surrounding walk) into one shared classifier — see "Converging the
Two Engines' Pass-1 Decision Table" below.

**Correction of the #10627 correction (Issue #10916 investigation):** a
#10627-era note here claimed most Base macros (`@inline`, `@assert`, …)
expand through `expand_macro_with_def` → the static pipeline, with only the
#7764 subset carved out to the dynamic engine. That described the
PRE-2026-06-25 routing: commit `ef83266a2e` ("Unify Base macro expansion
runtime", 2026-06-25) replaced `expand_base_macro`'s
`expand_macro_with_def` call with the dynamic `macro_runtime` entry for ALL
Base macros — the #7764 subset check now only picks the value-preserving
`expand_expr` entry vs. the `expand_stmt` entry *within* the dynamic
engine. So the text #10627 "corrected" ("the dynamic path handles all Base
macros") was in fact accurate for the current code; verified 2026-07-14 by
git history plus a hygiene-behavior probe: `@lock` (NOT in the #7764
subset) leaks its quote-local `temp` into caller scope (`temp = 42;
@lock lk 1+1; println(temp)` prints the lock, upstream prints `42`) —
exactly the dynamic engine's Issue #10977 no-renaming behavior, where the
static engine would have gensym-renamed it. The #10977 gap itself is
unchanged but WIDER than the @time/@elapsed examples suggest: because
`base_loader.rs::register_base_macros` marks EVERY Base macro as
module-owned (`hygiene: Some(MacroHygieneInfo{module: "Base", ..})` —
added for Issue #9619, so a caller-spliced name like `@time grid = ...`'s
`grid` resolves in the caller's scope, not renamed),
`macro_has_module_hygiene` skips quote-local gensym renaming for the WHOLE
expansion of every Base macro — `@time`/`@elapsed`'s internal
`t0`/`result`/etc. locals (which `base/timing.jl`'s own comment incorrectly
claims are hygiene-renamed) AND e.g. `@lock`'s `temp` above. Tracked by
Issue #10977.

**Resolved (Issue #10977, 2026-07-14):** `maybe_apply_quote_hygiene` no
longer treats "module-owned" as "skip all quote-local hygiene". Module-owned
plain-quote macros (all Base macros, bundled-package macros, user `module`
macros) now get a NARROWER pass,
`apply_module_macro_quote_local_hygiene` (`macro_runtime.rs`): the rename
set is collected STATICALLY from the macro body's own quote constructor via
the static engine's Pass-1 collector
(`collect_quote_constructor_introduced_names`,
`lowering/expr/quote/handlers.rs`) instead of from the expanded value — a
`$param` splice position is a lowered value expression there, not a literal
`Symbol` constructor, so only names the quote body itself declares/assigns
(`local t0`, `temp = ...`, `catch err`) are renamed, and caller-spliced
names (`@time grid = ...`'s `grid`, the Issue #9619 case) are structurally
never in the set. Two exclusions keep prior guarantees: names declared
`global` in the expansion (as in `apply_quote_function_hygiene`), and names
the expansion ALSO references inside an `esc(...)` subtree
(`collect_escaped_symbol_names`) — Plots' `@animate`/`@gif` declare
`local _anim`/`_anim_counter` in the quote and reference them from the
macro-BUILT `esc`-ed loop body (the Issue #6355 bridging mechanism
documented in `packages/Plots/src/api.jl`), so renaming only the non-`esc`
side would sever that link. Trade-off of the esc-shared exclusion: a caller
argument that itself mentions a macro-internal name (`@elapsed(t0 + 1)`
against `@elapsed`'s own `t0`) still suppresses the rename — identical to
the pre-#10977 behavior for that corner. `base/timing.jl`'s hygiene comment
is accurate again. Regression fixture:
`tests/fixtures/macros/macros_base_macro_internal_local_hygiene_10977.jl`
(also re-verifies the #9619 `@time grid = ...` semantics).

| Form | `QuoteBindingRole` (Issue #10627) | Static path (`collect_introduced_vars`) | Dynamic path (`collect_quote_local_names`) |
|------|-----------------------------------|------------------------------------------|----------------------------------------------|
| `local x` | `LocalDecl` | yes (#10242 predecessor) | yes (#8064) |
| assignment target `x = v` | `Assign` | yes (#10980: `register_assignment_target_names` mirrors the dynamic path's recursion, so `Tuple`/`TypeAssert` destructuring targets — `(a, b) = f()`, `x::Int = 1` — register every bare name; previously bare-`Symbol` targets only) | yes (#8064/#9619; also unwraps `Tuple`/`TypeAssert` targets) |
| `catch` variable | `TryCatchVar` | yes (#10242) | not yet — tracked by #10369 (separate, dynamic-only bug) |
| `for`-loop binding | `None` (binding is a nested `Assign`) | yes (recursion reaches the nested `Assign`, explicit arm since #10626) | yes (generic recursion into the binding's `Assign` shape — already worked before #10626; blocked on #10903 for same-scope-collision parity) |
| `let` binding | `None` (binding is a nested `Assign`) | yes (explicit arm, #10626) | yes (same generic-recursion coverage; already correct, no #10903-class bug) |
| `while` | `None` | n/a (introduces no locals) | n/a |
| function name | `FunctionName` | n/a — Pass 2 codegen for `Expr(:function, ...)` is unsupported in the static path (see above), so this role is classified but never consulted on that path | yes (#8064) |
| function parameters | n/a (no role; not `quote_binding_role`-classified — see the `QuoteBindingRole` note below Issue #10925's own section) | n/a (unsupported codegen) | **yes, scope-aware (Issue #10925)** — registered in a per-function scope frame (`rename_function_def_scoped`), renamed only within its own signature+body; a sibling reference of the same bare name outside the function is untouched |
| `where` type parameters | n/a (no role; same reason) | n/a (unsupported codegen) | **yes, scope-aware (Issue #10925)** — registered in the SAME frame as the function's own parameters when `where`-wrapping a function; a standalone `where` (not attached to a function) gets its own frame scoped to just its own subtree (`rename_where_args_scoped`) |
| generator/comprehension binding | n/a (no role; not head-classified at all — binding extraction there is a separate mechanism, not `quote_binding_role`) | n/a (unsupported codegen; follow-up #10916) | yes (#10626; blocked on #10903 for same-scope-collision parity) |

Both engines' Pass-1 collectors now dispatch every row above (except the last
three, which are not `quote_binding_role`-classified at all) through the
SAME `quote_binding_role(head: ExprHead) -> QuoteBindingRole` function
(`expr_heads.rs`) instead of independently re-deriving "which argument
position is the binding" per engine — see "Converging the Two Engines'
Pass-1 Decision Table" below.

Before #10626, the dynamic path renamed only **function names**
(Issue #8064) plus, via the same generic-recursion mechanism the static
path relies on, plain assignment targets and `local` declarations
(Issues #8064/#9619). #10626 added `Expr(:comprehension, ...)`/
`Expr(:generator, ...)` → `Expr::Comprehension`/`Expr::Generator` conversion
(`generator_binding_from_generator_value`), closing a prior hard error
("macro expansion returned unsupported Expr head :comprehension"/
":generator") for the single-binding, unfiltered form. Registering function
parameter names and `where` type-parameter names was attempted (matching
upstream's `@macroexpand`-observed renaming) but reverted before merge once
an adversarial review found it broke an unrelated sibling reference sharing
the parameter's bare name — see the flat-namespace note above. **Issue
#10925 completed this properly**, by making the rename mechanism itself
scope-aware (see "Scope-aware dynamic-path rename environment" above)
instead of leaving the two forms permanently unregistered. The
`catch`-variable gap in the dynamic path (Issue #10369) remains open and is
out of this epic's scope; the static path's `Function`/`Where`/
`Comprehension`/`Generator` Pass-2 codegen gap is tracked by the #10626
follow-up #10916 and is unaffected by #10925 (which is dynamic-path-only).

### Converging the Two Engines' Pass-1 Decision Table (Issue #10627)

Follow-up to #10266/#10626: converge stdlib static quote expansion
(`lowering/expr/quote/`) and VM-backed user/package macro expansion
(`macro_runtime.rs`) onto **one recursive Expr lowering contract** — a
shared registry/dispatcher for per-head handling, replacing per-head drift
where each engine independently re-derived "which head introduces a
binding, at which argument position." #10626 already established (see
"Scope: static path vs. dynamic path" above) that the two engines'
recursive TREE WALKS cannot be literally unified — the static path's
pre-execution constructor `Expr` and the dynamic path's post-execution
runtime `Value`/`ExprValue` are structurally incompatible representations,
and forcing one to mimic the other's data model (or building a lossy
adapter between them) was rejected as the wrong trade. What #10627 unifies
instead is the **decision** each walk consults at every head — a smaller,
sharper target that was previously duplicated as two independently
hand-maintained `match` arms over the same `ExprHead` values.

#### The shared registry

`subset_julia_vm/src/expr_heads.rs` was already the crate's canonical
per-head registry for FOUR metaprogramming-adjacent code paths (see its own
module doc: "Keep the symbolic names and per-path coverage in one table so
unsupported directions are visible when a new head is added") — `EXPR_HEAD_REGISTRY`'s
`cst_to_expr_value`/`macro_return_to_stmt`/`macro_return_to_expr`/
`runtime_eval` boolean columns, each checked against a same-shaped hand-written
`match` via a `debug_assert_eq!` at the real dispatch site
(`macro_return_stmt_support`/`macro_return_expr_support` in
`macro_runtime.rs`; `debug_assert!(expr_head.spec().cst_to_expr_value)` in
`cst_to_constructor.rs`; `debug_assert_eq!(head.spec().runtime_eval, ...)`
in `vm/builtins_macro/eval.rs`). #10627 extends this SAME registry and
anti-drift pattern rather than inventing a parallel mechanism:

1. **New registry column — `static_quote_top_level: bool`.** Whether the
   static path's OUTER Pass-2 dispatch
   (`quote_constructor_to_code_with_hygiene`'s top match,
   `lowering/expr/quote/code_generation.rs`) recognizes a head directly —
   `Call`/`Block`/`MacroCall`/`Tuple`/`Try`/`If`/`ElseIf`/`For`/`While`/`Let`
   (10 heads) are `true`; everything else `false`. This is `false` for some
   heads the static path DOES support (`Local`/`Assign`) because those are
   only recognized as a STATEMENT nested inside a `Block`/`If`/`Try`/`For`/
   `While`/`Let` body (the per-statement matches embedded in
   `lowering/expr/quote/handlers.rs`'s `handle_*_expr` functions), never as
   the outer dispatch's own head — the column tracks only the outer
   dispatch, exactly like `macro_return_to_stmt`/`macro_return_to_expr`
   track only their own dispatchers. A `static_quote_top_level_dispatch_support`
   shadow function (mirroring `macro_return_stmt_support`/
   `macro_return_expr_support`'s shape) is checked against it via
   `debug_assert_eq!` right where the real dispatch runs — the SAME
   drift-detection idiom, now covering a 5th path.
2. **New shared classifier — `quote_binding_role(head: ExprHead) ->
   QuoteBindingRole`.** A pure function (no tree navigation, no shared
   walk — see the advisor-flagged design constraint below) classifying
   which heads introduce a Pass-1 hygiene-relevant binding, by ROLE name
   (`LocalDecl`/`Assign`/`TryCatchVar`/`FunctionName`/`None`) rather than by
   argument index, so each engine's own tree-navigation code (which still
   differs, because the trees differ) maps its own child positions onto a
   shared vocabulary instead of re-deriving "for `Try`, the catch var is
   argument N" independently. Both `collect_introduced_vars`
   (`lowering/expr/quote/handlers.rs`, static) and `collect_quote_local_names`
   (`macro_runtime.rs`, dynamic) now `match quote_binding_role(head) { ... }`
   instead of `match head { ... }` directly for every role-classified head;
   drift between the two engines on WHICH heads introduce a binding is
   impossible by construction, since both consult the identical function.
   See the decision table above for the full per-head role assignment.
3. **`tracked_static_quote_gap_issue(head) -> Option<u32>`.** When the
   static Pass-2 catch-all rejects a head, this names the tracked follow-up
   Issue for a KNOWN gap (`Function`/`Where`/`Comprehension`/`Generator` →
   `#10916`) instead of a bare "not yet supported", so the error text itself
   documents the gap — a differential test asserting this error can assert
   the Issue reference too, rather than merely "some error happened."

#### What did NOT get merged, and why (per-engine overrides)

The registry unifies the *decision*, not the surrounding mechanics — these
remain deliberately separate, each for a reason specific to that engine:

| Concern | Static path | Dynamic path | Why not shared |
|---|---|---|---|
| Tree representation | Pre-execution constructor `Expr` (`Expr::Builtin{name: BuiltinOp::ExprNew, args}`, head at `args[0]`) | Post-execution `Value::Expr(ExprValue)` (head is a separate field, `args_snapshot()` excludes it) | #10626's finding still holds: no single Rust type/trait cleanly navigates both without a lossy adapter or forcing one to mimic the other |
| Name registration | `HygieneContext::register_local` — always gensyms via a global counter, records a rename map | `HashSet<String>` collection with a `globals`-exclusion set, followed by a SEPARATE `rename_quote_local_symbols` rewrite pass | Different data flow shapes (immediate resolve-as-you-go vs. collect-then-rewrite); the ROLE decision (`quote_binding_role`) is shared, the registration side effect is not |
| `esc(...)` detection | By CALLEE NAME on an un-evaluated `Expr(:call, :esc, ...)` node | By the already-evaluated `Expr(:escape, ...)`/`Expr(:hygienic-scope, ...)` runtime head | Same underlying concept (an escape boundary), different tree shape at the point each engine sees it — not a `quote_binding_role` case |
| Assign-target destructuring | Bare `Symbol` targets only (tuple/type-assert targets silently register nothing) | Recursively unwraps `Tuple`/`TypeAssert` targets | Pre-existing asymmetry, unreachable by any current stdlib/Base macro (verified by grep); tracked by follow-up #10980, not fixed here (refactor-only scope) |
| `catch`-variable collection | Collected (Issue #10242) | Not collected (Issue #10369) | Pre-existing, separately-tracked dynamic-only gap; `quote_binding_role` classifies `Try` as `TryCatchVar` for BOTH engines, but only the static engine currently acts on that classification |
| `Function`/`Where`/`Comprehension`/`Generator` Pass-2 codegen | Unsupported (Issue #10916) | Supported (comprehension/generator: single-binding unfiltered only, Issue #10626; filtered/multi-binding tracked by #10923) | Pass-2 codegen shape differs entirely between engines (LetBlock-based expression construction vs. genuine `Stmt`/`Expr` AST construction) — out of scope for a Pass-1-classifier convergence |

#### Differential tests

- `subset_julia_vm/src/expr_heads.rs`'s own `#[cfg(test)] mod tests` is the
  direct convergence proof: `quote_binding_role_*` tests pin the classifier
  itself, and `expr_head_registry_round_trips_names`/
  `expr_head_registry_has_unique_names` guard the registry's own invariants.
- `lowering/expr/quote/code_generation.rs`'s
  `static_quote_top_level_gap_tests` module drives the constructor tree
  directly (same precedent as `handlers.rs`'s `collect_introduced_vars_tests`
  — none of `Function`/`Where`/`Comprehension`/`Generator` is reachable
  end-to-end through any current real stdlib/Base macro) and asserts the
  static Pass-2 catch-all names Issue #10916 for those four heads, and a
  genuinely unknown head does NOT get a spurious Issue reference.
- `tests/fixtures/macros/quote_engine_convergence_test_10627.jl`: an
  end-to-end differential fixture pairing `Test.@test` (a genuine
  static-path macro — its quote body's `Block`/`Local`×4/`Try`-`Catch`/`If`/
  `ElseIf`/nested-`If`/`Call`/`String` heads are ALL touched by Pass-1/Pass-2
  codegen regardless of which runtime branch executes, so a single SAFE
  passing invocation exercises the full matrix) against a hand-mirrored
  user-defined macro (`@my_test`, dynamic path) with the textually
  equivalent quote body, asserting identical pass/fail/error classification
  and that neither engine's quote-internal locals leak into the caller
  scope. The fixture deliberately avoids a genuinely FAILING real
  `Test.@test` invocation — that trips the Issue #9360 harness gate
  (`Vm::any_test_failed()`/`docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv`, which
  explicitly forbids allowlisting new fixtures) and, as discovered while
  writing this fixture, a failing top-level `@testset` is not even catchable
  via `try`/`catch` in sjulia today (Issue #10978, out of #10627's scope).
- `quote_comprehension_hygiene_10626.jl` (pre-existing, #10626) already
  covers the dynamic-path `for`/comprehension/generator forms end-to-end via
  user-defined macros — the static side of those forms is
  `collect_introduced_vars_tests`'s hand-built-tree coverage (unreachable
  end-to-end, same reachability status as the code-generation gap tests
  above).

#### Static Pass-2 reachability, and why #10916's codegen is deferred

Verified call-site map (2026-07-14, Issue #10916 investigation — every
entry traced through the code AND probed end-to-end where a behavioral
discriminator exists):

| Macro kind / position | Engine |
|---|---|
| User-defined `macro ... end` (stmt or expr position) | dynamic (`macro_runtime.rs`) |
| Base (`julia/base/*.jl`) macros — ALL, both positions | dynamic (since `ef83266a2e`, 2026-06-25; the #7764 subset only selects expr-vs-stmt conversion within it) |
| Bundled-package macros | dynamic |
| Stdlib (`STDLIB_MACROS`) macros, expression position | dynamic (`expand_stdlib_macro_expr`, #10293/#10307) |
| **Stdlib macros, statement position** | **static** (`expand_stdlib_macro` → `expand_macro_with_def` → `quote_constructor_to_code_with_varargs/_with_locals`) |
| Nested macrocall inside a static expansion's quote body | static (`handle_macrocall_expr` → `expand_nested_macro_from_expr_args`, can statically re-expand a nested user/Base macro template) |

The static engine's ONLY entry is therefore a statement-position call of a
macro defined under `subset_julia_vm/src/julia/stdlib/` — today that means
`Test`'s five macros plus `InteractiveUtils`' bodiless
`macro code_warntype end`, none of which uses a
`function`/`where`/comprehension/generator form in its quote body (probe:
temporarily adding `macro __probe_comp() quote [i for i in 1:3] end end` to
`Test.jl` reproduces the tracked #10916 error verbatim; the identical
comprehension in a user-defined macro already works via the dynamic path).
Consequences:

- A `tests/fixtures/macros/*.jl` fixture CANNOT exercise static Pass-2
  codegen for the #10916 heads — a fixture-defined macro takes the dynamic
  path. The only end-to-end exerciser would be gratuitously editing a real
  stdlib macro. This is why #10916's Pass-2 codegen
  (`Function`/`Where`/`Comprehension`/`Generator`) stays DEFERRED per the
  issue's own rule ("no codegen … before there is a real consumer",
  encoding #10626's postmortem lesson): implementation would be verifiable
  only by hand-built constructor trees, exactly the unreachable-codegen
  hazard that rule forbids. Pick-up criterion: a real stdlib-macro consumer
  appears (e.g. a future `Test`/`InteractiveUtils`/`Dates` macro port whose
  quote body needs one of these heads).
- Structural alternative (design option for the #10627 epic owners, noted
  here rather than filed as an Issue): once the two-engine convergence
  matures, retiring the static statement-position Pass-2 entirely — routing
  stdlib statement-position macros through the dynamic engine like every
  other macro kind since `ef83266a2e` — would close the whole #10916 gap
  class by DELETING code instead of adding speculative handlers. The known
  delta to reconcile first: the static engine's hygiene (#10242,
  gensym-renamed quote-locals, `Test.@test`'s `catch e` case) is currently
  STRONGER on this surface than the dynamic engine's (#10369 catch-var gap,
  #10977 Base-macro no-rename), and `expand_stdlib_macro`'s
  statement-position tail-value discard (#10307/#10496) must be preserved.

Adding a new operator to SubsetJuliaVM requires synchronized changes across **five layers**. Missing any layer will cause silent failures (the operator parses but doesn't execute, or executes with wrong semantics).

### The 5-Layer Synchronization Requirement

```
1. Parser    →  Token recognition (lexer/parser rules)
2. Lowering  →  CST → Core IR transformation
3. Pure Julia →  Wrapper functions in base/*.jl
4. Intrinsic →  Rust intrinsic implementation (if needed)
5. VM        →  Instruction handler
```

### Layer Details

| Layer | Files | What to add | Example (`>>>` unsigned right shift) |
|-------|-------|-------------|--------------------------------------|
| **1. Parser** | `subset_julia_vm_parser/src/parser/` | Token/operator recognition | Recognize `>>>` as operator token |
| **2. Lowering** | `lowering/expr/binary.rs` | Map CST operator → `BinaryOp` enum | `">>>" → BinaryOp::UnsignedRightShift` |
| **3. Pure Julia** | `src/julia/base/operators.jl` | Julia wrapper calling intrinsic | `>>>(x::Int64, y::Int64) = _intrinsic_lshr(x, y)` |
| **4. Intrinsic** | `src/intrinsics.rs` | Rust implementation | `_intrinsic_lshr` function |
| **5. VM** | `vm/exec/`, `compile/expr/binary/` | Instruction dispatch | Handle in `CallDynamicBinary` |

### Dual Lowering Function Sync Requirement

**CRITICAL**: The lowering stage has TWO parallel functions that must stay in sync:

| Function | File | Context |
|----------|------|---------|
| `lower_binary_expr()` | `lowering/expr/binary.rs` | Direct expression context |
| `lower_binary_expr_with_ctx()` | `lowering/expr/binary.rs` | Lambda/closure context |

When adding a new operator to one function, you **MUST** add it to the other as well. Failing to do so causes the operator to work at top-level but fail inside closures (or vice versa).

### Verification Audit Script

```bash
# Inspect parser operator CST construction sites.
rg -n 'CstNode::leaf\(NodeKind::Operator|NodeKind::Operator' subset_julia_vm_parser/src/parser -g '*.rs'

# Inspect lowering operator handling: helper mapping plus special-case branches.
rg -n 'fn map_binary_op|map_binary_op\(|op_text ==|is_broadcast_op|strip_broadcast_dot' \
  subset_julia_vm_lowering/src/lowering/expr/binary.rs \
  subset_julia_vm_lowering/src/lowering/expr/helpers.rs
```

### Checklist for Adding a New Operator

- [ ] **Parser**: Add token recognition rule
- [ ] **Lowering**: Add to `lower_binary_expr()` AND `lower_binary_expr_with_ctx()`
- [ ] **IR**: Add `BinaryOp` enum variant (if new)
- [ ] **Pure Julia**: Add wrapper in appropriate base file (operators.jl, etc.)
- [ ] **Intrinsic**: Add Rust intrinsic function (if needed for performance)
- [ ] **Compiler**: Add compile-time handling in `compile/expr/binary/`
- [ ] **VM**: Add instruction handler in `vm/exec/`
- [ ] **Tests**: Add fixture test covering the new operator
- [ ] **Verify both lowering paths**: Test operator at top-level AND inside a closure

### Related Issues

- Issue #2618: Bitwise operators implementation (followed this pattern)
- Issue #2620: Prevention issue documenting the synchronization requirement

## Broadcast Assignment `.=` Lowering (PR #2805)

The `.=` broadcast assignment operator is lowered to `materialize!(dest, expr)` for in-place broadcast semantics.

### Transformation

```julia
# Original Julia code:
Z .= expr

# Lowered to:
Z = materialize!(Z, expr)
```

This transformation is implemented in three parallel code paths in `lowering/stmt/assignment.rs`:

| Function | Context |
|----------|---------|
| `lower_assignment()` | Direct assignment context |
| `lower_assignment_with_ctx()` | Lambda/closure context |
| `lower_compound_assignment()` / `lower_compound_assignment_with_ctx()` | Compound assignment context |

Each path checks if `op_text == ".="` and emits the appropriate `materialize!` call instead of a regular assignment.

### Fixture Tests

- `tests/fixtures/broadcast/broadcast_regression_inplace.jl` - In-place broadcast assignment tests

## Callable Struct Syntax (Functor Pattern)

SubsetJuliaVM supports callable struct instances (functors) where a struct type has methods defined on it:

```julia
struct LinearMap
    a::Float64
    b::Float64
end

(f::LinearMap)(x) = f.a * x + f.b
```

This is handled in `lowering/function/signature.rs` where `TypedParameter` nodes with the `::StructType` annotation pattern are recognized. The compiled function is registered with the struct type for dispatch via `CallTypedDispatch` or `CallFunctionVariable` at runtime.

### Key Files

- `lowering/function/signature.rs` - Recognizes callable struct syntax in function signatures
- `compile/expr/call/` - Routes callable struct invocations
- `vm/exec/call_dynamic_typed.rs` - Runtime dispatch for callable structs
- `vm/exec/call_function_variable.rs` - Handles callable values (functions, closures, callable structs)
- `julia/base/broadcast.jl` - Uses callable structs (`AndAnd`, `OrOr`) for broadcast short-circuit operators

## Control-Flow Type Tracking (Issue #3044, #3049)

At control-flow merge points (try/catch, if/else, loops), a variable may have been
assigned different types in different branches. The compiler must **widen** the type to
`Any` when branches disagree — it cannot know at compile time which branch ran.

### `join_type()` — the merge operation

`compile/type_helpers.rs` provides `join_type(a, b)`:

```rust
pub(super) fn join_type(a: &ValueType, b: &ValueType) -> ValueType {
    if a == b { a.clone() } else { ValueType::Any }
}
```

**Always use `join_type()` to merge types at control-flow join points.** Do NOT
copy one branch's type when both branches are present.

### try/catch merge (`compile/stmt/stmt_try_catch.rs`)

After compiling both the try- and catch-blocks, the compiler merges local variable
types using `join_type()`:

```rust
for (name, try_ty) in &locals_after_try {
    let catch_ty = self.locals.get(name).cloned().unwrap_or(ValueType::Any);
    self.locals.insert(name.clone(), join_type(try_ty, &catch_ty));
}
```

### Common mistakes to avoid

**Bug pattern (Issue #3044)** — resetting instead of widening:
```rust
// WRONG: resets to try-path type when types differ
if self.locals.get(name) != Some(try_ty) {
    self.locals.insert(name.clone(), try_ty.clone());  // ← bug: should be Any!
}

// CORRECT: widen when types differ
let catch_ty = self.locals.get(name).cloned().unwrap_or(ValueType::Any);
self.locals.insert(name.clone(), join_type(try_ty, &catch_ty));
```

### Type inference pre-pass (`compile/inference.rs`)

For if/else and loop bodies, a **pre-pass** runs before compilation to determine
`mixed_type_vars` — variables that are assigned different types on different paths.
These variables are compiled with `StoreAny`/`LoadAny` (dynamic typing) rather than
type-specific instructions. See `compile/inference.rs` for the pre-pass logic.

## Module-scope name resolution: qualified keying (Issues #10214 / #10236 / epic #10243)

Nested functions and `let`/`@testset`-root helpers lifted out of a module body
must resolve their free names against **their own enclosing module's** globals.
The bookkeeping that assigns each lifted function its owning-module scope lives
in `subset_julia_vm_compile/src/compile/pipeline_ctx.rs` (`build_function_universe`) and
`subset_julia_vm_compile/src/compile/collect.rs` (`collect_from_module`,
`collect_module_body_let_functions`). Two maps drive it:

- **`function_module_paths`** (`inline function name -> owning module path`,
  Issue #7180) — gives a nested/closure function lifted from a module function
  body the enclosing module's scope.
- **`module_scope_overrides`** (Issue #10073) — gives a helper collected
  directly from a module-body `let`/`@testset` its root module scope (it has no
  enclosing named-function parent to inherit from).

**The bug (Issue #10214/#10236):** both maps were keyed by the **bare function
name**. When two different modules — or a module and Main — defined a function /
closure / helper with the same bare name, whichever was processed last won in
the map, so the other scope's same-named helper silently resolved against the
WRONG module's globals (wrong output, not a crash).

**The design — key every scope-resolution decision by a fully-qualified
identity, never a bare name:**

- `module_scope_overrides` is keyed by the helper's **collection index** in the
  `inline_functions` vec (a stable per-instance identity), so a module-body root
  and an unrelated same-named Main-level `let` root never share a key.
- `function_module_paths` is keyed by the **module-qualified** name
  (`"Module.path.func"`). `collect_from_module` gives a module's own top-level
  functions' nested children a **module-qualified parent identity**
  (`"Module.path.outer"`, not bare `"outer"`), so two modules' distinct
  `outer#helper` bodies get distinct `method_tables` / `function_indices`
  identities instead of dedup-colliding into one (`MethodTable::add_method`
  dedups by signature). `current_func_name` in `compile_functions` and
  `Stmt::FunctionDef`'s `qualified_name` are qualified to match.
- A module-body `let`/`@testset`-root helper is a lexically-scoped LOCAL, not a
  genuine exportable module generic. It keeps the compile-time bare
  method-table alias (which `module_owned_function_table_name`, Issue #7575,
  requires alongside the qualified table for the module's own in-scope call to
  redirect), but is kept OUT of the **runtime** bare short-name index
  (`VmState::function_name_index`, which `Value::Closure`/`Value::Function`
  dynamic dispatch consults) via `FunctionInfo::suppress_short_name_alias`
  (`#[serde(default)]`, no C ABI bump — bytecode/cache metadata). Otherwise a
  closure value created for an unrelated same-named Main-level `let` root could
  resolve to the module's helper body.
- A side effect of module-qualified nested names is the type-name shape
  `typeof(Module.parent#child)` (both `.` and `#`). `base_type_name` split on
  the LAST `.` — the module dot INSIDE the `typeof(...)` parens — dropping the
  `typeof(` wrapper so the value was no longer recognized as `<: Function`,
  breaking HOF dispatch (`findfirst(f::Function, v)`). `callable_singleton_struct_name`
  (`subset_julia_vm_types/src/inference_core/type_core.rs`) strips only the
  module from the INNER name and re-wraps, fixing a genuine latent bug for any
  `typeof(Module.func)` value.

**Open follow-ups (tracked separately, NOT resolved here):** the qualified key
is per-**enclosing-scope**, not per-`let`-block-instance, so two same-named
`let`/`@testset` roots in the SAME module (or two Main-level `let` roots) still
collide — Issue #10395. A full `local > module-const > global > Base` lexical
shadowing reorder, and the REPL completion-delegation item (#10235), remain
open items owned by their own issues under epic #10243. Issue #10363
(module-scope `Ref` index-assignment Int64->Float64 coercion) is independent.

## `try`/`catch`/`finally` as an Expression: Tail-Position Value Semantics (Issue #10254)

`try ... catch ... [else ...] [finally ...] end` is a first-class Julia
**expression**, not merely a statement: it can appear as an assignment RHS
(`r = try ... end`), inside arithmetic (`1 + (try ... end)`), or in the
implicit-return (tail) position of a function body. Its value is defined by
this rule set — the "design" this doc section locks in as a Rule, following
the concrete bug fixed by Issue #10074 (assign-only tails silently returning
`nothing`/crashing) and the design/prevention follow-up Issue #10254 (a
comprehensive regression matrix + this writeup):

1. **The value is the last expression of whichever branch actually ran.**
   No exception → the `try` block's tail value. An exception caught by
   `catch` → the `catch` block's tail value. An `else` block (Julia's
   "ran with no exception" branch) replaces the `try` value with its own
   tail when present.
2. **An assignment IS an expression, for every assignment-statement shape.**
   `x = v` (plain `Stmt::Assign`), `x += v` (`Stmt::AddAssign`), and
   `global x = v` all evaluate to `v` — the same rule the
   function-tail-return path already applies (Issues #8976/#10023). A
   branch that ends in an assignment still produces a value; it is not
   silently `nothing`. This generalizes to indexed (`v[i] = x`), field
   (`obj.field = x`), and dict (`d[k] = x`) assignment, and to tuple
   destructuring (`(a, b) = rhs`) for the RHS shapes that route through a
   compiler-internal temporary (Issue #10431) — see "Generalizing to Every
   Assignment Shape" below for the one still-open sub-case.
3. **`finally` NEVER contributes the produced value.** It runs purely for
   its side effect (e.g. resource cleanup); the try/catch expression's
   value is whatever it was immediately before `finally` ran, whether or
   not `finally`'s own body ends in a value-producing statement.
4. **The rule composes through nesting.** A trailing nested
   `try`/`catch` (Issue #4833), `if`/`elseif`/`else`, or bare
   `begin ... end` block inside a branch is itself recursively subject to
   this same "last statement is the value" rule, so its own tail value
   flows out to the *outer* try/catch's result.
5. **Uniform across all use sites.** The same value flows out whether the
   try/catch sits in function-tail (implicit-return) position, a local or
   module-level (top-level) rvalue position, or nested inside another
   control-flow expression — there is exactly one lowering rule, not one
   per syntactic position.

### Mechanism

The rewrite happens once, in lowering, and both the expression-position and
tail-position call sites share it:

- **`try_stmt_into_value_expr`** (`lowering/expr/mod.rs`) converts a lowered
  `Stmt::Try` into an `Expr::LetBlock`: it rewrites the tail of the `try`,
  `catch`, and `else` blocks (via `assign_block_tail_value`, below) to
  assign into one fresh `__sjvm_try_result_<span>` variable, leaves
  `finally_block` **untouched** (so it structurally cannot feed the result
  variable — this is what guarantees Rule 3), then wraps
  `[init result = nothing; rewritten try; read result]` in a
  `Expr::LetBlock`. Used both at expression position (Issue #4784) and,
  via the compile-layer implicit-return path, at function-tail position
  (Issue #6223).
- **`assign_block_tail_value`** (`lowering/expr/mod.rs`) is the recursive
  helper that rewrites a single branch's trailing statement into an
  assignment to the shared result variable. It has an arm for a trailing
  `Stmt::Expr` (Rule 1), `Stmt::Assign`/`Stmt::AddAssign` (Rule 2 — the
  gap that shipped as Issue #10074), and a trailing nested `Stmt::Try`,
  `Stmt::If`, or `Stmt::Block` that it recurses into (Rule 4). Any other
  trailing statement (`Stmt::Return`, a loop, a bare `break`, …) is left
  untouched — the result variable stays at its `nothing` default for that
  branch, matching Julia (`x = for ... end` is `nothing`). Shared by
  `try_stmt_into_value_expr` and the analogous `if_stmt_into_value_expr`.
- **`compile_block_value`** (`compile/expr/mod.rs`) is the codegen twin:
  it compiles the empty-binding `Expr::LetBlock` that a bare
  `begin ... end` block lowers to, and needs the matching
  `Stmt::Assign`/`Stmt::AddAssign` arm so a `begin ... end` block ending
  in a plain assignment (reachable directly, or nested as a branch tail
  per Rule 4) evaluates to the assigned value rather than `nothing`.
- **Return-type inference** independently joins the tail types of the
  `try`/`catch`/`else` branches — see `infer_block_branch`
  (`compile/abstract_interp/engine/mod.rs`) and the `Stmt::Try` arms in
  `compile/inference.rs` — so the type a caller sees (for arithmetic,
  `typeof()`, an `::T` return annotation, …) matches Rule 1's runtime
  value. This mirrors the `join_type()` local-variable merge described
  above in "Control-Flow Type Tracking", applied to the tail value itself
  rather than to individual local variables.

### Regression coverage

- `tests/fixtures/exceptions/try_catch_expression_4784.jl` — bare-value
  tails at expression position (Issue #4784).
- `tests/fixtures/exceptions/nested_try_catch_expression_4833.jl` — nested
  try/catch, bare-value tails (Issue #4833, Rule 4).
- `tests/fixtures/exceptions/try_implicit_return_6223.jl` — bare-value
  tails at function-tail/implicit-return position (Issue #6223).
- `tests/fixtures/exceptions/try_catch_type_inference_9131.jl` — env-join
  and return-type coverage (Issue #9131).
- `tests/fixtures/exceptions/try_tail_assign_implicit_return_10074.jl` —
  assign-only tails (Rule 2) at function-tail position (Issue #10074).
- `tests/fixtures/exceptions/try_catch_tail_value_semantics_10254.jl` —
  design close-out (Issue #10254): assign-only tails at rvalue/expression
  position, nested try/catch with assign-only tails, a bound catch
  variable with an assign-only tail, `try`/`finally` (no `catch`) with an
  assign-only tail, and typed post-use (arithmetic, `typeof`, string
  concatenation, `::T` return annotation) of an assign-tail result.
- `tests/fixtures/control_flow/assign_statement_tail_value_10431.jl` —
  the Rule 2 generalization below: indexed/field/dict assignment tails,
  and non-literal/dependent-swap tuple-destructuring tails, across every
  implicit-return context (not just try/catch).

### Generalizing to Every Assignment Shape (Issue #10431)

Rule 2 above was originally implemented only for `Stmt::Assign`/
`Stmt::AddAssign` (Issue #10074). Verifying the #10254 design close-out
surfaced the same gap for every OTHER assignment-statement shape — `v[i] =
x` (`Stmt::IndexAssign`), `obj.field = x` (`Stmt::FieldAssign`), `d[k] = x`
(`Stmt::DictAssign`), and tuple destructuring (`(a, b) = rhs`, which never
lowers to the dedicated `Stmt::DestructuringAssign` IR variant — see below)
— in **every** implicit-return context, not just try/catch: a plain
function-body tail, an `if`/`else` branch tail, and a bare `begin ... end`
block tail all had the identical gap. Issue #10431 fixed it generally,
across **four independent compilation backends** that each separately
implement "the last statement of a block is its value":

1. **Lowering rewrite** (`lowering/expr/mod.rs`) — `assign_block_tail_value`
   gained a shared `split_assign_stmt_via_temp` arm for `IndexAssign`/
   `FieldAssign`/`DictAssign`: bind the RHS to a fresh
   `__sjvm_tail_assign_tmp_<span>` temporary, perform the store using the
   temporary (so the RHS and any index expressions are evaluated exactly
   once — re-reading `v[i]` afterward would both re-evaluate `i`, a
   possible double side effect, and re-run `getindex` needlessly), then
   read the temporary as the result. `compile_block_value`
   (`compile/expr/mod.rs`) and the compile-layer `compile_function_body`/
   `compile_block_with_implicit_return` (`compile/stmt.rs`) each got the
   matching codegen-level arm via a new `compile_assign_stmt_tail_via_temp`
   helper (or reuse the same lowering-level rewrite, for `compile_block_value`).
2. **Return-type inference** — `infer_block_branch`'s per-statement loop
   (`compile/abstract_interp/engine/mod.rs`) needed matching arms so the
   DECLARED return type reflects the RHS value's type, not `Nothing` — a
   mismatch that doesn't just print wrong, it **crashes** the caller's
   type-specific instruction (e.g. `PrintI64NoNewline` fed an actual
   `Nothing`). This inference layer has its own CFG-based fast paths for
   straight-line (no-branch) and all-explicit-return bodies
   (`try_infer_straightline_cfg_return`/`cfg_authoritative_statement_value`,
   `try_infer_all_return_cfg`), which needed the identical fix independently
   — they bypass the general per-statement loop entirely when eligible.
3. **Lazy call-site specializer** (`vm/specialize/stmt.rs`) — a THIRD,
   independent compiled representation, produced on demand per call site
   (`CallSpecialize`) once the declared return type is known, has its own
   `compile_function_body`/`compile_block_with_implicit_return` with the
   same "last statement" match and needed the same
   `split_assign_stmt_via_temp`-based fix (reusing the shared lowering
   helper — this module compiles `Stmt` directly to bytecode rather than
   rewriting IR first, but the temp-binding logic is identical).

**Tuple destructuring is more structurally involved.** The lowering choice is
now explicit for the flat identifier cases covered by Issues #10444 and
#10464. Consumers do not infer assignment identity from source spans, block
shape, or generated names for these forms:

| Source shape | Lowered representation | Assignment value |
| --- | --- | --- |
| `(a, b) = (1, 2)` (independent literal) | `Stmt::DestructuringAssign` | Per-element temps reconstruct the original literal tuple in value position; statement position stays allocation-free. |
| `(a, b) = f()` (nonliteral/iterable) | `Stmt::DestructuringAssign` | The RHS is evaluated once into one internal value; the VM drives Julia's `iterate` protocol exactly far enough to bind the requested targets, and value position returns that same RHS object. This includes generators, custom iterators, and iterate-only wrappers such as `Iterators.partition`. |
| `(a, b) = (1, 2, 3)` / `(a, b, c) = (1, 2)` | `Stmt::DestructuringAssign` | Mismatched literal arity uses the same checked iteration path: extra RHS values are ignored after evaluating the complete RHS expression, while a missing requested value raises `BoundsError` at runtime. Conversion never rejects arity statically. |
| `(a, b) = (b, a)` (dependent literal) | Expanded `Stmt::Block` | Per-element `__tuple_tmp_` values preserve simultaneous assignment and reconstruct the tuple through `destructuring_tail_value`. |
| Nested or rest patterns | Expanded `Stmt::Block` | The existing generated-temp path remains in use. |

The final two rows remain follow-up scope for Issue #10464. Their legacy
`destructuring_tail_value` recognition is deliberately limited to reserved
compiler temporary names. It never uses span equality: macro expansion may
legitimately assign one call-site span to several unrelated statements.
Explicit-IR value carriers use collision-proof gensyms instead of span-derived
names; AoT additionally reserves the escaped Rust identifier, since two distinct
Core IR names can otherwise sanitize to the same Rust local.

The AoT Rust backend currently represents this cursor directly only for static
Tuple/NamedTuple, Array, and Range RHS types. A custom Julia struct that
implements `iterate`, an `Any`-typed RHS whose iterator shape is not known
statically, or a Generator fails with a span-bearing `UnsupportedInstruction`
until AoT has Julia method-dispatch iteration support. AoT's current Generator
is a stateful Rust `Box<dyn Iterator>` and cannot preserve Julia's separate
iteration state plus the assignment's original value identity. This remaining
AoT boundary keeps Issue #10464 open; it is not replaced with Rust
`IntoIterator` special cases.

A genuine nested `begin ... end` — including one wrapped in `@eval`, whose
statements can ALSO share a single macro-call span — is unaffected either
way: `assign_statement_tail_value_10431.jl` includes a negative regression
test proving `function f(); begin; a = 1; b = 2; end; end` still returns
`2`, matching upstream Julia, both with and without an `@eval` wrapper.

## Related Documentation

- `docs/vm/TYPE_SYSTEM.md` - Type system architecture and runtime type objects
- `docs/vm/LATTICE_TYPE.md` - Compile-time lattice representation and joins
- `docs/vm/TYPE_SYSTEM.md` - Type system architecture and ValueType variant checklist
- `docs/vm/STATUS.md` - Current implementation status
- `CLAUDE.md` - Contributor guidelines
