# Lowering: Julia Source to Core IR

This document describes the lowering phase of SubsetJuliaVM, which transforms parsed Julia source code (CST) into Core IR.

## Overview

The lowering phase is responsible for:
1. Converting CST (Concrete Syntax Tree) nodes to Core IR structures
2. Handling function signatures and parameter parsing
3. Processing type annotations and type expressions
4. Expanding macros and handling special forms

## Key Files

- `subset_julia_vm/src/lowering/mod.rs` - Main lowering module and `Lowering` struct
- `subset_julia_vm/src/lowering/function/` - Function definition lowering (directory module):
  - `mod.rs` - Entry point and shared logic
  - `signature.rs` - Parameter parsing, callable struct syntax
  - `full_form.rs` - `function ... end` form
  - `short_form.rs` - `f() = expr` form
  - `where_clause.rs` - `where T` clause handling
  - `defaults.rs` - Default parameter value handling
  - `tests.rs` - Unit tests
- `subset_julia_vm/src/lowering/struct_.rs` - Struct definition lowering
- `subset_julia_vm/src/lowering/abstract_.rs` - Abstract type definition lowering
- `subset_julia_vm/src/lowering/expr/` - Expression lowering (directory module):
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
- `subset_julia_vm/src/lowering/stmt/` - Statement lowering (directory module):
  - `mod.rs` - Entry point, statement dispatch
  - `assignment.rs` - Assignment and `.=` broadcast lowering
  - `control_if.rs` - If/elseif/else lowering
  - `macros/` - Statement-level macro expansion (directory module):
    - `mod.rs` - Entry point
    - `expand.rs` - Statement macro expansion
    - `static_eval.rs` - Compile-time evaluation for statement macros
    - `enum_impl.rs` - `@enum` macro implementation

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

Unit tests for parameter parsing are in `subset_julia_vm/src/lowering/function/tests.rs`:

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

1. **Indexed loads**: Mutable container `getindex` expressions are not
   conditional MustAlias refinement paths.
2. **Alias graph coverage**: Fresh aliases do not inherit field path refinements;
   the guard remains tied to the original narrowed slot.

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

When modifying parameter handling in `subset_julia_vm/src/lowering/function/`:

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
rg -c "SplatParameter" subset_julia_vm/src/lowering/function -g '*.rs'
rg -c "SplatExpression" subset_julia_vm/src/lowering/function -g '*.rs'
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

When modifying either the parser (`subset_julia_vm_parser/src/parser/`) or the lowering stage (`subset_julia_vm/src/lowering/`):

1. **Verify CST node children**: After parsing, check how many children each node type actually contains (single vs. multiple)
2. **Check lowering extraction functions**: Ensure lowering functions that extract data from CST nodes handle ALL children, not just the first match
3. **Avoid early-return on first match**: Functions that `return` on the first matching child are fragile -- prefer collecting all matches first
4. **Test with multi-element variants**: If a CST node can contain N children, always test with N=1 AND N>1
5. **Treat unused helper warnings as potential bugs**: If a helper function that processes all children exists but is unused (and a single-child version is used instead), investigate why

**Historical Bug (Issue #2143):** `parse_for_clause()` returned only the first `ForBinding`, silently dropping subsequent bindings in multi-variable comprehensions like `[i*j for i in 1:3, j in 1:3]`. The fix was to use `parse_for_clause_bindings()` which iterates ALL `ForBinding` children.

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
rg -n 'fn resolve_call_target|resolve_call_target\(' subset_julia_vm/src/lowering/expr/call.rs
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

- `subset_julia_vm/src/compile/abstract_interp/engine/` - Contains both `infer_block` and `infer_block_explicit_return_only`

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
rg -n 'match macro_name\.as_str\(\)|"[^"]+"\s*=>' subset_julia_vm/src/lowering/expr/macros/mod.rs
```

### Prevention Test

`tests/fixtures/macros/test_base_macro_in_function.jl` — Verifies that Base macros work inside function bodies (the no-context lowering path).

## Adding New Operators (Issue #2620)

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
  subset_julia_vm/src/lowering/expr/binary.rs \
  subset_julia_vm/src/lowering/expr/helpers.rs
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

## Related Documentation

- `docs/vm/TYPE_SYSTEM.md` - Type system architecture and runtime type objects
- `docs/vm/LATTICE_TYPE.md` - Compile-time lattice representation and joins
- `docs/vm/TYPE_SYSTEM.md` - Type system architecture and ValueType variant checklist
- `docs/vm/STATUS.md` - Current implementation status
- `CLAUDE.md` - Contributor guidelines
