# Error Design Guidelines

This document describes the error type design guidelines for SubsetJuliaVM. It prevents anti-patterns like the `PipelineResult` custom enum (Issue #2898, fixed in PR #3054).

## The Anti-Pattern: Custom Result-Like Enums

**Do NOT** define enums that mimic `Result<T, E>` with `Ok`-like and error variants:

```rust
// BAD: PipelineResult anti-pattern (removed in PR #3054)
pub enum PipelineResult {
    Ok(Program),
    ParseError(ParseError),
    LoweringError(LoweringError),
    CompileError(CompileError),
}
```

Problems with this pattern:
1. **No `?` operator support** — cannot use standard error propagation
2. **No `std::error::Error` impl** — incompatible with `anyhow`, `thiserror`, `Box<dyn Error>`
3. **Non-standard API** — callers must pattern-match custom variants instead of `Ok`/`Err`
4. **Confusing** — looks like `Result` but isn't, misleading new contributors

## Standard Error Type Pattern

New error types must follow this pattern:

```rust
/// Error enum with one variant per error kind.
#[derive(Debug)]
pub enum FooError {
    Parse(ParseError),
    Compile(CompileError),
    Io(std::io::Error),
}

impl std::fmt::Display for FooError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FooError::Parse(e) => write!(f, "parse error: {}", e),
            FooError::Compile(e) => write!(f, "compile error: {}", e),
            FooError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for FooError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FooError::Parse(e) => Some(e),
            FooError::Compile(e) => Some(e),
            FooError::Io(e) => Some(e),
        }
    }
}

/// Type alias for Result with FooError.
pub type FooResult<T> = Result<T, FooError>;
```

## Checklist for New Error Types

When defining a new error type, verify:

- [ ] The error enum does NOT have an `Ok`-like variant — use `Result<T, YourError>` instead
- [ ] The error enum implements `std::fmt::Display`
- [ ] The error enum implements `std::error::Error`
- [ ] The error enum derives `Debug`
- [ ] A type alias `pub type FooResult<T> = Result<T, FooError>` is provided
- [ ] The `?` operator works for callers (test this)
- [ ] Error variants wrap inner errors via `From` impls or `map_err()`

## Checklist for Code Review

When reviewing PRs that add error types:

- [ ] No enum has `SomeName::Ok(T)` / `SomeName::SomeError(E)` variants
- [ ] Error types implement `std::error::Error`
- [ ] `?` operator can be used by callers
- [ ] Result alias uses `pub type XxxResult<T> = Result<T, XxxError>`

## Existing Error Types

The following error types in the codebase follow the standard pattern:

- `VmError` — VM runtime errors (see `PANIC_FREE.md` for propagation rules)
- `ParseError` — Parser errors with source spans
- `LoweringError` — Lowering phase errors with source spans
- `CompileError` — Compiler errors

## Related

- Issue #2898: Original `PipelineResult` anti-pattern bug
- PR #3054: Removed `PipelineResult`, replaced with `Result<T, PipelineError>`
- `PANIC_FREE.md`: VM-specific error propagation (raise vs return)
