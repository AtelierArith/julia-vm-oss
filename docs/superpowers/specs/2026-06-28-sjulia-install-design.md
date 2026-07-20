# End-User Installer Script with Embedded Precompile Cache

**Status:** Implemented  
**Date:** 2026-06-28  
**Scope:** Add `scripts/sjulia_install.sh` and `scripts/sjulia_install.ps1` that build and install `sjulia` with embedded Base bytecode and prelude Program caches.

## Goal

Provide a single shell script that lets an end user install a precompile-cache-enabled `sjulia` after cloning the repository. The script must:

1. Build a host `sjulia` binary.
2. Generate the Base bytecode cache and the parsed/lowered prelude Program cache.
3. Embed both caches into the final installed binary via `cargo install`.

The installed binary therefore starts faster because Base compilation and prelude parse/lower are skipped at runtime.

## Non-Goals

- Remote one-liner installation (`curl | sh`). The user already has the repository cloned.
- Cross-compilation or iOS builds. This script targets the host platform only.
- Custom install roots or feature flags beyond `--force-cache`. These are reserved for future extensions.

## Assumptions

- The script can be run from any directory; it locates the repository from its own path and changes into it.
- macOS or Linux host for `sjulia_install.sh`; Windows host for `sjulia_install.ps1`.
- Rust toolchain (`cargo`) is installed and available on `PATH`.
- `cargo install` default location (`~/.cargo/bin`) is acceptable.
- The user has network access for crates.io if dependencies are not already cached.

## Design

### Installation flow

The script performs the following steps in order:

1. **Build host `sjulia`**
   ```bash
   cargo build --release -p subset_julia_vm --bin sjulia --features repl
   ```
   This binary is only used to generate caches in the next steps.

2. **Generate prelude Program cache** (skippable)
   ```bash
   "$SJULIA_BIN" --precompile-prelude "$PRELUDE_PROGRAM_CACHE"
   ```

3. **Generate Base bytecode cache** (skippable)
   ```bash
   "$SJULIA_BIN" --precompile-base "$BASE_CACHE"
   ```

4. **Install with embedded caches**
   ```bash
   SJULIA_BASE_CACHE="$BASE_CACHE" \
   SJULIA_PRELUDE_PROGRAM_CACHE="$PRELUDE_PROGRAM_CACHE" \
     cargo install --force --bin sjulia --path subset_julia_vm --features repl
   ```
   `subset_julia_vm/build.rs` detects the environment variables and embeds the cache files via `include_bytes!`, enabling the `has_embedded_base_cache` and `has_embedded_prelude_program` cfg flags.

   `--force` makes re-runs idempotent by overwriting an existing `~/.cargo/bin/sjulia`, and `--bin sjulia` is required because `subset_julia_vm/Cargo.toml` defines multiple `[[bin]]` targets.

The PowerShell implementation follows the same four steps, uses `sjulia.exe`,
and checks `$LASTEXITCODE` after each native command so that it also fails fast
under Windows PowerShell versions that do not support native-command error
propagation through `$ErrorActionPreference`.

### Cache regeneration policy

To make repeated installs fast, the script skips cache generation when the existing cache files are newer than both:

- the `sjulia` binary that produced them, and
- the prelude source directory (`subset_julia_vm/src/julia`).

This mirrors the policy in `scripts/wasm_build_with_cache.sh` and avoids unnecessary rebuilds.

The user can force regeneration by passing `--force-cache`.

### Arguments

- `--force-cache` — always regenerate both caches before installing.

### Environment variables

- `CARGO_TARGET_DIR` — respected; defaults to repository `target/`.
- `CARGO_INSTALL_OPTS` (future) — reserved for passing extra options to `cargo install` without changing the script.

### Error handling

- `set -euo pipefail` aborts on the first error.
- Each step prints a clear message to `stdout` or `stderr`.
- Missing cache files after generation are a fatal error.
- `cargo` availability is checked before any expensive work.

## Testing

- **Manual smoke test:** run `./scripts/sjulia_install.sh` or `pwsh -File scripts/sjulia_install.ps1` in a clean checkout and verify that the installed `sjulia` executable runs.
- **Cache embedding verification:** inspect the build output or run a small Julia program and compare cold-start latency against a cache-free build.
- **CI integration:** consider adding a GitHub Actions job that runs the installer on `ubuntu-latest`, `macos-latest`, and `windows-latest` after the regular test suite. This is left as a follow-up task to keep the initial change small.

## Future work

- `--root <dir>` to override the install location.
- `--features <features>` to pass additional feature flags to `cargo install`.
- Remote one-liner installer that clones, builds, and installs in a temporary directory.

## References

- `scripts/test_with_cache.sh`
- `scripts/wasm_build_with_cache.sh`
- `subset_julia_vm/build.rs`
- `subset_julia_vm_compile/src/compile/embedded_cache.rs`
- `subset_julia_vm/src/pipeline.rs`
- `subset_julia_vm/src/bin/sjulia.rs`
