# Testing Guide

This document describes the test suites in SubsetJuliaVM, when to run each, and how to write new tests.

## Test Suites

### Fixture Tests (`fixture_tests.rs`)

The primary test suite. Auto-generated from `manifest.toml` files in `tests/fixtures/<category>/`. Each test compiles and runs a `.jl` file, comparing the result against an expected value.

- **2,377 manifest-defined tests** across 104 categories
- Float comparison with `epsilon = 1e-10`
- 16 MB thread stack per test

**When to run:** After any change to parser, lowering, compiler, or VM.

```bash
timeout 1800 cargo nextest run --release --test fixture_tests
timeout 1800 cargo nextest run --release --test fixture_tests <category>::   # specific category
```

### Integration Tests

Multiple files covering end-to-end scenarios:

| File | Purpose |
|------|---------|
| `integration_array_tests.rs` | Array, matrix, broadcast, complex numbers |
| `integration_string_type_tests.rs` | Char, strings, math constants, BigInt |
| `integration_dict_broadcast_tests.rs` | Dictionary and broadcast operations |
| `integration_struct_hof_tests.rs` | Structs and higher-order functions |
| `integration_module_base_tests.rs` | Module system and Base functions |
| `integration_compile_sample_tests.rs` | Compilation validation for code samples |

**When to run:** After changes to specific subsystems (arrays, strings, etc.).

## Test Organization

Keep implementation files readable by putting public/API-level behavior tests in
`subset_julia_vm/tests/` whenever they can exercise the same surface as an
external crate. Use `src/**/tests.rs` or inline `#[cfg(test)]` modules only for
tests that need private functions, private types, module-local helpers, or
white-box state that should not become part of the public API.

When moving tests out of `src/`, do not promote private APIs just to satisfy an
integration test. Prefer one of these outcomes:

1. Move the test unchanged if it uses only public crate APIs.
2. Split the module so public behavior moves to `tests/` and private helper
   coverage stays in `src/`.
3. Keep the test in `src/` if it is genuinely white-box coverage.

Issue #8460 audit snapshot:

| Source file | Classification and action |
|-------------|---------------------------|
| `compile/abstract_interp/engine/mod.rs` | Internal inference-engine helpers and `mod tests;`; keep source-side. |
| `compile/method_table.rs` | Private `MethodTable` construction/matching coverage; keep source-side. |
| `inference_core/type_core.rs` | Public standalone subtype modules moved to `tests/core_type_public_tests.rs`; private parser/helper coverage stays source-side. |
| `inference_core/dispatch_resolver.rs` | Resolver tests use test-only helper entry points and internal candidate structs; keep source-side until a public dispatch harness exists. |
| `compile/expr/binary/mod.rs` | Private compiler predicate coverage for a private module; keep source-side. |
| `types/julia_type/comparison.rs` | Public `JuliaType` behavior moved to `tests/julia_type_comparison_tests.rs`. |

The structural-debt audit also ratchets large `#[cfg(test)]` blocks in
`subset_julia_vm/src/`: `bash scripts/check_structural_debt_inventory.sh` fails
if the number or total line budget of source-side test blocks over 200 lines
increases. Lower those baselines when migrating more tests to `tests/`.

### Panic-Free VM Tests (`panic_free_vm_tests.rs`)

Counts `.unwrap()`, `.expect()`, and `panic!()` in VM runtime code. Fails if counts increase.

- Baselines: `.unwrap()` = 0, `.expect()` = 1 (SystemTime), `panic!()` = 0
- Excludes test code, doc comments, `unwrap_or`/`unwrap_or_else`/`unwrap_or_default`

**When to run:** After adding code to `src/vm/exec/` or `src/vm/builtins_*.rs`.

```bash
timeout 1800 cargo nextest run --release --test panic_free_vm_tests
```

### Dispatch Tests (`dispatch_tests.rs`)

Tests multiple dispatch functionality and the type system.

**When to run:** After changes to dispatch logic, type matching, or method resolution.

### AoT E2E Tests (`aot_e2e_tests.rs`)

End-to-end Ahead-of-Time compilation tests. Verifies Julia-to-Rust codegen and type inference.

**When to run:** After changes to the AoT compiler (`src/aot/`).

```bash
timeout 1800 cargo nextest run --release --test aot_e2e_tests
```

### Code Samples Tests (`code_samples_tests.rs`, `ios_samples_tests.rs`)

Tests that all code samples (Hello World, arrays, matrices, etc.) compile and run correctly. `ios_samples_tests.rs` covers iOS app `CodeSample.swift` samples.

**When to run:** After adding or modifying code samples, or after changes that could affect sample output.

### Parser Tests (`parser_pure_rust.rs`)

400+ pure Rust parser tests covering Julia syntax edge cases.

**When to run:** After changes to the parser or tree-sitter grammar.

### Other Test Files

| File | Purpose |
|------|---------|
| `unicode_tests.rs` | Unicode handling |
| `broadcast_dispatch_analysis_tests.rs` | Broadcast and dispatch analysis |
| `type_propagation_call_tests.rs` | Type propagation in function calls |
| `core_ir_aot_tests.rs` | AoT Core IR file roundtrip |
| `include_tests.rs` | `include()` directive |
| `base_exports_consistency_tests.rs` | Base exports don't exceed upstream Julia |

## Which Tests to Run

| Change | Tests to Run |
|--------|-------------|
| Parser | `parser_pure_rust`, `fixture_tests` |
| Lowering | `fixture_tests`, unit tests (`--lib`) |
| Compiler | `fixture_tests`, `dispatch_tests` |
| VM execution | `fixture_tests`, `panic_free_vm_tests` |
| VM builtins | `fixture_tests`, `panic_free_vm_tests` |
| AoT compiler | `aot_e2e_tests` |
| Code samples | `code_samples_tests`, `ios_samples_tests` |
| Base/stdlib Julia | `fixture_tests`, `base_exports_consistency_tests` |
| Any PR | Full: `timeout 1800 cargo nextest run --release` |

## Writing Fixture Tests

### Directory Structure

```
subset_julia_vm/tests/fixtures/
  manifest.toml              # Root config (epsilon)
  <category>/
    manifest.toml            # Test definitions for this category
    test_file.jl             # Julia test file
```

### manifest.toml Format

```toml
[[tests]]
name = "category_test_name"
file = "test_file.jl"
expected = true
description = "What this test verifies (Issue #XXXX)"
```

**Fields:**
- `name` — Unique across ALL categories. Prefix with category name (e.g., `array_basic_indexing`).
- `file` — Relative path to `.jl` file within the category directory.
- `expected` — Expected result: `true`/`false` (bool), `42` (integer), `3.14` (float), `"hello"` (string).
- `description` — What the test verifies. Include issue number if applicable.
- `skip` — Optional. Set to `true` to skip the test.

### Julia Test File Rules

1. Types, functions, and modules must be defined OUTSIDE `@testset`.
2. The file must end with an expression that produces the expected value.
3. Typically end with `true` for tests that verify behavior via assertions.
4. Verify with Julia first: `julia path/to/test.jl`

**Example (`tests/fixtures/arithmetic/basic.jl`):**

```julia
function test_basic_arithmetic()
    a = 1 + 2
    b = 10 - 3
    c = 4 * 5
    a == 3 && b == 7 && c == 20
end
test_basic_arithmetic()
```

### Name Uniqueness (Issue #3135)

Test names must be unique across ALL categories. The runtime uses `find()` on merged tests — duplicates silently load the wrong file. Always prefix with the category name.

Run before opening a PR:
```bash
bash scripts/check_fixture_test_names.sh
```

## Test Execution Commands

```bash
# Full test suite (always use timeout)
timeout 1800 cargo nextest run --release

# Fixture tests only
timeout 1800 cargo nextest run --release --test fixture_tests

# Specific fixture category
timeout 1800 cargo nextest run --release --test fixture_tests array::

# Library unit tests only
timeout 1800 cargo nextest run --release --lib

# List all fixture categories
cargo nextest list --test fixture_tests 2>/dev/null | sed 's/::.*/::/;s/ .*//' | sort -u

# Specific test file
timeout 1800 cargo nextest run --release --test dispatch_tests

# Clippy (lint checks)
cargo clippy
```

## Helpers (`tests/common/mod.rs`)

Shared utilities for integration tests:

- `run_core_pipeline(src, seed)` — Parse, lower, compile, run.
- `compile_and_run_str_with_output(src, seed)` — Returns output string.
- `compile_and_run_program_direct(src, seed)` — Returns `(Value, String)`.
- `assert_i64()`, `assert_f64()`, `assert_f32()` — Type-specific assertions.
- `assert_ok_numeric()` — Accepts either I64 or F64 result.

## Adding Rust Unit Tests

For `#[cfg(test)]` modules in library code:

1. Add `#[cfg(test)] mod tests;` to the module's `mod.rs`
2. Create a `tests.rs` file in the same directory
3. Follow the pattern from `lowering/function/tests.rs`:

```rust
use crate::lowering::Lowering;
use crate::parser::Parser;

fn lower_source(source: &str) -> crate::ir::core::Program {
    let mut parser = Parser::new().expect("Failed to init parser");
    let parse_outcome = parser.parse(source).expect("Failed to parse");
    let mut lowering = Lowering::new(source);
    lowering.lower(parse_outcome).expect("Failed to lower")
}

#[test]
fn test_something() {
    let program = lower_source("...");
    // assertions
}
```

4. Run: `timeout 1800 cargo nextest run --release --lib`

## Known SubsetJuliaVM Limitations in Tests

Before writing fixture tests, avoid these tracked patterns that fail in SubsetJuliaVM even though they work in Julia:

- **Avoid property-bearing direct `IOContext(...)` fixtures**: `IOContext(io, :key => value)` fails in sjulia (Issue #6409), while the `iocontext(...)` workaround is sjulia-only and fails upstream Julia fixture validation (Issue #6408). `get(ctx, key, default)` itself works once a context exists.
- **Avoid the `for outer i in itr` modifier form**: `for outer in itr` works as a normal loop variable, but the upstream `outer` modifier form is rejected during lowering rather than mis-executed (Issue #6465).

**Keeping this list updated** (Issue #3173): When a bug fix or issue reveals a new SubsetJuliaVM limitation that affects fixture test authoring, add a bullet here in the same PR. When a limitation is resolved (feature implemented), remove the corresponding bullet.

## Behavioral Changes in Fixture Tests (Issue #2261)

When making behavioral changes, search affected tests; verify hardcoded expectations; document the computation chain.

## Unit Test Conventions for compile/ and vm/ Modules

### IR Literal Pitfall (Issue #3194)

`ir::core::Literal` uses `Literal::Int(i64)` for integer literals — there is NO `Literal::Int64` variant. Quick reference:
- `Literal::Int(v)` — i64 integer
- `Literal::Float(v)` — f64 float
- `Literal::Float32(v)` — f32 float
- `Literal::Bool(v)` — boolean
- `Literal::Str(s)` — string

### Test Helpers for compile/ (Issue #3183)

Use `compile::test_helpers` (only available in `#[cfg(test)]`) for constructing IR nodes:
- `zero_span()` — creates `Span::new(0,0,0,0,0,0)` (Span has no `Default` impl)
- `int_lit(v)` — creates `Expr::Literal(Literal::Int(v), zero_span())`
- `var_expr(name)` — creates `Expr::Var(name, zero_span())`
- `call_expr(fn_name, args)` — creates `Expr::Call` with empty splat/kwargs masks

### Pure Function Test Policy (Issue #3185, #3189, #3191, #3207, #3214, #3224)

Every new standalone `fn` (no `&self`) in `compile/` or `vm/` that takes only primitive/standard types MUST have at least:
- One happy-path test
- One edge-case test (empty input, `None`-returning, boundary condition)

When adding a new pure function, add tests in the same file's `#[cfg(test)] mod tests { ... }` block.

### Lowering Module Test Patterns (Issue #3198, #3200)

Lowering modules fall into two categories:
1. **Pure functions** (testable): `helpers.rs`, `literal.rs`, `collection.rs`, `views.rs` — test with unit tests
2. **CST-walker functions** (hard to isolate): `expr/mod.rs`, `binary.rs`, `call.rs` — test via fixture tests

When adding a pure function to `lowering/`, always add unit tests. For `replace_end_with_lastindex` / `replace_begin_with_firstindex` patterns, test: identity case, dimension-aware case, recursive BinaryOp application, and pass-through for non-keyword vars.

### Self-Free CoreCompiler Method Extraction (Issue #3238)

When adding a method to `impl CoreCompiler` whose body makes no `self.field` access:
1. Extract it as a standalone `pub(super) fn` outside the `impl` block
2. Add unit tests for the standalone function
3. If the method mixes static + `self`-dependent logic, extract the static sub-predicate returning `Option<bool>`

Signals: name starts with `can_`, `is_`, `should_`, `static_`.

### vm/matmul/ and vm/builtins_macro/ Helper Tests (Issue #3227)

Leaf helper modules (`vm/matmul/complex.rs`, `vm/builtins_macro/helpers.rs`) with pure math or string predicate logic MUST have `#[cfg(test)] mod tests` blocks. The parent handler is not a substitute for unit tests.

### Global Atomic State Tests (Issue #3251)

Tests for `get/set_bigfloat_precision` and `get/set_bigfloat_rounding_mode` mutate global atomics.
Each test MUST save and restore state. Use `cargo nextest` (process isolation) for reliable execution.

## Rust Test Assertion Style

When writing `#[cfg(test)]` or `tests.rs` Rust tests (Issue #3053, #3090, #3098):

- **DO NOT** use `match result { pat => {} other => panic!("Expected...", other) }` — fragile anti-pattern
- **DO** use `assert!(matches!(result, ExpectedVariant(..)), "Expected ..., got {:?}", result)`
- **DO** use `assert_eq!` for types implementing `PartialEq`
- **DO** ensure all types used in `assert!(matches!())` with `{:?}` derive `Debug` — if not, add `#[derive(Debug)]`
- New enums and structs should derive `Debug` by default: `#[derive(Debug, Clone, PartialEq)]` for data types
- `std::assert_matches::assert_matches!` is still nightly-only (rust-lang/rust#82775), so use `assert!(matches!())`
- If `panic!` is legitimately needed, annotate with `// OK: panic! — <reason>` on the same line
- This applies to ALL `#[cfg(test)]` blocks — both `tests.rs` files and inline modules in `mod.rs`, `lib.rs`, etc.
- Run `bash scripts/check_no_panic_in_tests.sh` to check for violations (baseline: 0, zero tolerance, scans ALL `.rs` files)
- **Float test values** (Issue #3288, #3290): Avoid `3.14`, `2.71`, `1.41`, `1.73` in test float literals — these trigger the `approx_constant` Clippy lint (π ≈ 3.14159, e ≈ 2.71828, √2 ≈ 1.41421, √3 ≈ 1.73205). Use unambiguous values like `1.25`, `6.78`, `0.75` instead.
- **Duplicate edit pattern workaround** (Issue #3204): When a manual edit has multiple possible matches, make the patch context unique or anchor the insertion at the final module boundary with `apply_patch`. Do not use shell append/redirection for repository edits.

## Related Documentation

- `PANIC_FREE.md` — VM panic-prevention policies
- `ERROR_DESIGN.md` — Error type design guidelines
- `LOWERING.md` — Parser/lowering details
- `CODE_AUDITS.md` — Code audit policies and scripts
- `CHECKLISTS.md` — Implementation checklists for new types/variants
