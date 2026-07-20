---
name: sjulia-build-iteration
description: "Use when cargo nextest run --release is too slow, changing subset_julia_vm source causes many crates to recompile, or you need faster fixture-test iteration in SubsetJuliaVM."
---

# SubsetJuliaVM Build & Test Iteration

Speed up the compile/test inner loop without bypassing the release gates.

## Hard rules

1. **Full `cargo nextest run --release` is a PR gate, not a debugger.** Run it
   after parallel merges and before merging VM/compiler/dispatch/inference
   changes; do not use it as the first check after every edit.
2. **Never run `cargo build` and `cargo nextest` concurrently.** They contend on
   Cargo artifact locks and slow each other down.
3. **Always wrap nextest with `timeout 1800`** (30-minute ceiling).
4. **The `repl` feature is required to re-link `target/release/sjulia`.**
   `cargo build --release` alone does NOT refresh the binary.

## Why it feels slow

`subset_julia_vm/Cargo.toml` defines **5 binaries, 9 test targets, and 35+
bench targets** that all depend on the main lib. Changing the lib forces Cargo
to rebuild every one of those targets — that is the "~30 crates" you see.
`cargo nextest run --release` also builds everything in release mode, which is
intentionally slow (`lto = false` by default for release, but still
`codegen-units = 16`).

## Fastest → slowest iteration ladder

Pick the first command that gives you enough signal.

1. **Type-check only** (no codegen, no link):
   ```bash
   cargo check -p subset_julia_vm --features repl
   ```

2. **Run the fixture directly** (no nextest overhead):
   ```bash
   cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features repl
   target/dev-fast/sjulia subset_julia_vm/tests/fixtures/<category>/<file>.jl
   ```

3. **Print the recommended command sequence for a changed fixture**:
   ```bash
   bash scripts/fixture_fast_feedback.sh subset_julia_vm/tests/fixtures/<category>/<file>.jl
   ```

4. **Category gate with fast release-like settings**:
   ```bash
   timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests <category>::
   ```

5. **All fixture tests, still fast**:
   ```bash
   timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests
   ```

6. **Full PR gate** (only when you must):
   ```bash
   timeout 1800 cargo nextest run --release
   ```

## Reduce the "30 crate" recompile storm

`cargo nextest run --release` with no filter builds **bins, tests, and benches**.
Narrow the target set:

```bash
# Fixture tests only — skips bins and benches
timeout 1800 cargo nextest run --release --test fixture_tests <category>::

# Lib unit tests only
timeout 1800 cargo nextest run --release --lib -p subset_julia_vm

# One specific integration test target
timeout 1800 cargo nextest run --release --test sjulia_cli_stdin_tests
```

List fixture categories:

```bash
cargo nextest list --test fixture_tests 2>/dev/null | awk '{print $2}' | awk -F'::' '{print $1}' | sort -u
```

## Available fast profiles

Defined in root `Cargo.toml`:

| Profile | What it changes | Use for |
|---------|-----------------|---------|
| `dev-fast` | `opt-level = 1` | Direct `sjulia` runs during development |
| `release-fast` | `codegen-units = 256`, `lto = false` | Faster release-like nextest gates |

Do **not** use these profiles for final performance benchmarks or shipping
builds.

## Cache / linker ergonomics

- **sccache**: `brew install sccache` (macOS) or `cargo install sccache`, then
  `export RUSTC_WRAPPER=sccache`. CI already sets this. It caches LLVM IR
  across clean checkouts and dramatically reduces rebuilds of unchanged crates.
- **macOS**: `.cargo/config.toml` already enables `-C split-debuginfo=unpacked`.
- **Linux**: install `lld` and `clang` so the `.cargo/config.toml` linker
  settings apply (`sudo apt-get install -y lld clang` on Debian/Ubuntu).

## When the change is AoT-touching

If your diff touches the AoT pipeline, the fast profiles above still help for
local sanity, but you must also run the AoT gate before merging:

```bash
bash scripts/test_aot.sh
```

See `AGENTS.md` §"Build & Test" and `sjulia-dev` for adding functions, fixtures,
and VM performance measurement.
