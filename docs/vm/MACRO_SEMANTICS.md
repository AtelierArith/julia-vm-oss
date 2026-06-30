# Macro Semantics in SubsetJuliaVM

This document describes how macros work in SubsetJuliaVM, including differences from official Julia and special handling requirements.

## Overview

Macros in Julia are functions that transform code at compile time. They receive unevaluated expressions (AST) and return new expressions that replace the macro call.

## Macro Expansion Phases

### In Official Julia

```julia
macro show(ex)
    expr_str = string(ex)  # Runs at EXPANSION TIME
    quote
        println($expr_str, " = ", $(esc(ex)))  # Returned for RUNTIME
        $(esc(ex))
    end
end
```

1. **Expansion Time (Compile Time)**:
   - Macro receives `ex` as an `Expr` (unevaluated AST)
   - Code before `quote` executes immediately
   - `string(ex)` produces `"f(4)"` from AST

2. **Runtime**:
   - Only the `quote` block is returned
   - `$expr_str` is replaced with `"f(4)"` (literal string)
   - `$(esc(ex))` is replaced with the expression `f(4)`

### In SubsetJuliaVM

SubsetJuliaVM now mirrors this model for user macros and Base registry macros:

1. **Expansion-time execution**: The macro body is compiled into a synthetic
   function and executed by `lowering/macro_runtime.rs`.
2. **AST conversion**: The returned `Expr` / `QuoteNode` / literal value is
   converted back into IR through the shared macro-runtime value conversion path.
3. **Bootstrap kernel**: A small Rust lowering kernel remains for structural
   macros required before Base is fully available (`@inline`, `@noinline`,
   `@inbounds`, `@boundscheck`, `@nospecialize`, metadata macros, `@views`,
   `@view`, and multi-argument `@show`).

## Expansion-Time Functions

These functions receive special treatment during macro expansion:

### `string(param)` - Source Text Extraction

When `string(param)` is called where `param` is a macro parameter:
- **Behavior**: Returns the printed form of the expansion-time AST argument.
- **Implementation**: The macro body runs through `macro_runtime`, so `string`
  dispatches normally during expansion-time VM execution.
- **Example**: `string(ex)` with argument `f(4)` -> `"f(4)"`

### `esc(expr)` - Hygiene Escape

Marks an expression as escaped from hygiene renaming:
- **Behavior**: The expression is inserted directly without gensym renaming
- **Implementation**: Handled in `quote/main.rs`

### `gensym()` / `gensym(tag)` - Unique Symbol Generation

Generates unique symbols to avoid variable name collisions:
- **Behavior**: Returns a unique symbol like `##123` or `##tag#123`
- **Implementation**: Rust builtin

## Adding New Expansion-Time Functions

When adding support for a new function that should evaluate at expansion time,
prefer implementing the function as normal Julia/VM behavior and covering it
with macro-runtime fixtures. Add Rust-only handling only when the macro is part
of the bootstrap kernel and cannot wait for Base to be available.

## Testing Macro Behavior

### Fixture Tests

Tests in `tests/fixtures/macros/` verify macro behavior:

```julia
# show_expression_string.jl
using Test

@testset "@show displays source expression" begin
    f(x) = x + 1
    result = @show f(5)
    @test result == 6  # Verify return value
    # Output should show "f(5) = 6", not "6 = 6"
end
```

### Comparing with Official Julia

Run the same test in both systems:

```bash
# SubsetJuliaVM
timeout 1800 cargo nextest run --release --test fixture_tests macros_show

# Official Julia
julia tests/fixtures/macros/show_expression_string.jl
```

### Using @macroexpand for Debugging

```julia
# Check macro expansion result
expanded = @macroexpand @show f(4)
println(expanded)
# Should contain literal string "f(4)"
```

## Known Differences from Official Julia

1. **Bootstrap macros are still Rust kernels**: macros needed before Base is
   available (`@inline`, `@noinline`, `@inbounds`, metadata wrappers,
   `@view`/`@views`, and multi-argument `@show`) keep direct lowering support.
   Other Base registry macros should run through `macro_runtime`.

2. **Expr head coverage is explicit**: quote construction, macro-return
   lowering, and runtime `eval` are tracked in `src/expr_heads.rs`. A new Julia
   AST head may still need an entry plus value-to-IR conversion before advanced
   metaprogramming code works identically.

## Related Issues and PRs

- #1352 - Bug: @show macro outputs evaluated value instead of source expression
- #1353 - Initial fix attempt (syntax change, insufficient)
- #1354 - Root cause fix (expansion-time evaluation of `string(param)`)
- #1355 - This documentation and testing improvements
- #7719 - Central `ExprHead` registry for quote/macro/eval dispatch
- #7720 - Metaprogramming roundtrip gate
- #7721 - Base registry macro expansion unified on `macro_runtime`
