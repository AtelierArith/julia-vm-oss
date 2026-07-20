# AoT Cargo Target Binary Path Design

## Context

Issue #11598 records a gate failure where Cargo honors an externally supplied
`CARGO_TARGET_DIR`, but AoT and metamorphic shell helpers later look for
`sjulia` or `juliars` under the repository-local `target/release`. A guarded
merge can therefore finish expensive builds and tests, then fail because its
consumer and producer disagree about the binary location.

## Decision

Every affected script derives its default release binaries from one environment
precedence contract:

1. An explicit `SJULIA_BIN` or `JULIARS_BIN` wins.
2. Otherwise use `${CARGO_TARGET_DIR}/release/<binary>`.
3. When `CARGO_TARGET_DIR` is unset, use `<repo-root>/target`.

`scripts/test_aot.sh` resolves the target directory once, exports the two
binary variables, and passes that environment through every downstream step.
The downstream helpers also implement the same defaults so direct invocation
does not depend on the parent gate. Relative `CARGO_TARGET_DIR` values are
resolved from the repository root, matching the scripts' repository-root
execution model; absolute paths remain unchanged.

The affected consumers are:

- `scripts/test_aot.sh`
- `scripts/metamorphic_equivalence.sh`
- `scripts/aot_numeric_matrix_reduced.sh`
- `scripts/aot_vm_differential.sh`
- `scripts/aot_fixture_julia_parity.sh`
- `scripts/aot_fixture_no_silent_mismatch.sh`
- `scripts/aot_cranelift_fixture_differential.sh`
- `scripts/aot_cranelift_backend_benchmark.sh`
- `scripts/fixture_julia_parity.sh`
- `scripts/test_fixture_julia_parity.sh`
- `scripts/check_fixture_parity_sweep.sh`

This change does not alter where generated probes or temporary artifacts are
written. It only makes executable discovery agree with Cargo output.

## Rejected alternatives

- Fix only `test_aot.sh`: this repairs the parent gate but leaves direct helper
  invocation and the premerge metamorphic path inconsistent.
- Add a new sourced shell library: it would remove a few repeated assignments,
  but introduces another runtime dependency and Bash sourcing boundary for a
  three-variable contract.

## Verification

Extend the existing source-only AoT gate audit with executable regression cases
for default target, external absolute target, relative target, and explicit
binary overrides. Discover all `aot_*.sh` and fixture-parity binary consumers,
require each binary environment variable to have exactly one authoritative
target-derived assignment, and reject active fixed paths in both `$ROOT` and
`${ROOT}` spellings. Negative mutations restore a fixed parent-gate path and add
a later direct-helper reassignment, proving both are rejected. Then run the
focused audit, its negative self-test, the full AoT gate with an external target
directory, and the repository premerge gate.

## Success criteria

- `CARGO_TARGET_DIR=<external> bash scripts/test_aot.sh` consumes binaries from
  `<external>/release` in every step.
- Direct affected-helper invocation uses the same default.
- Explicit `SJULIA_BIN` and `JULIARS_BIN` values are preserved.
- The default repository-local target behavior remains unchanged.
- A fixed-path regression fails the registered source-only negative self-test.
