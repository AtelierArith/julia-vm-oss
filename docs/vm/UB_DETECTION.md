# UB Detection Layer

Issue #9004 adds a UB-oriented detection layer separate from the panic-free
work in #8686/#8707. Panic containment stops Rust unwinds from crossing C ABI
boundaries; it does not detect invalid pointer ownership, use-after-free,
double-free, aliasing violations, or other undefined behavior.

## Unsafe Inventory

Run:

```bash
bash scripts/check_unsafe_inventory.sh
```

The check writes `target/ub-safety/unsafe_inventory.tsv` and
`target/ub-safety/report.md`, then compares unannotated unsafe sites against
`docs/vm/UNSAFE_INVENTORY_BASELINE.tsv`.

Current policy:

- Existing unannotated unsafe sites are grandfathered in the baseline.
- New unsafe code must carry a nearby `Safety:` comment with an Issue reference.
- If an existing unsafe site is audited, replace its baseline dependency with a
  local `Safety:` comment and regenerate the baseline.

## Miri

Run:

```bash
rustup toolchain install nightly --component miri rust-src
timeout 1800 bash scripts/test_miri_vm.sh
```

The gate runs `cargo +nightly miri test -p subset_julia_vm --test
miri_smoke_tests`. The smoke test constructs a tiny `CompiledProgram` directly
and executes it through `Vm::run`, so it covers VM stack/slot execution without
pulling parser or external-native dependencies into miri.

This is intentionally not the full lib or fixture suite. The full suite is too
broad for a practical first nightly miri gate because it includes file IO,
external/native dependencies, and long-running fixture paths. Add focused miri
tests as unsafe-bearing VM internals become auditable.

## FFI ASan/UBSan

Run:

```bash
rustup toolchain install nightly --component rust-src
timeout 1800 bash scripts/test_ffi_sanitizers.sh
```

The script builds `subset_julia_vm_ffi` as a sanitizer-instrumented cdylib with
Rust ASan (`RUSTFLAGS=-Zsanitizer=address`, `-Z build-std`) and compiles the C
harness `subset_julia_vm_ffi/tests/ffi_sanitizer_smoke.c` with
`-fsanitize=address,undefined`.

The C harness exercises the public `subset_vm.h` ownership contract:

- `compile_to_ir` / `free_string`
- detailed execution success and error result allocation/free
- streaming callback execution
- REPL session create/eval/reset/free
- feature-gated FFI panic probes when built with `ffi-panic-test`

Rust does not provide a stable UBSan equivalent for Rust code through `rustc` in
this setup. UBSan is therefore applied to the C harness, while ASan instruments
the Rust cdylib and C caller together. LeakSanitizer is disabled by default
(`detect_leaks=0`) because Rust global/thread-local caches create noisy leak
reports; the gate targets invalid access, use-after-free, double-free, and
C-side UB. The enforced nightly job runs this on Linux; macOS local runs are
skipped by default because Darwin ASan interposition can fail before the harness
starts. Set `SANITIZER_ALLOW_DARWIN=1` only for manual experiments.

Both `test_miri_vm.sh` and `test_ffi_sanitizers.sh` fail when prerequisites are
missing. Use `--skip-if-unavailable` only for optional local probes, never for a
CI gate that claims UB coverage.
