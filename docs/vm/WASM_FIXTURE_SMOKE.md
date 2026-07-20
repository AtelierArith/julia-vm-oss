# WASM Fixture Smoke

Issues #8710/#8711 add an executable parity smoke for `subset_julia_vm_web`.
The selected fixture list lives in `docs/vm/WASM_FIXTURE_SMOKE.tsv`; each row
is expected to evaluate to final `Bool` value `true` through the same
`run_from_source` entry point exposed by the wasm package. The optional
known-failure ratchet lives in `docs/vm/WASM_FIXTURE_SMOKE_ALLOWLIST.tsv`.

## Scope

The initial subset intentionally covers pure computation only:

- numeric tower basics: arithmetic, bool arithmetic, comparisons, ranges
- control flow: `if`, `for`, `while`, branch reassignment
- closures: simple capture, nesting, shadowing
- collections: arrays, tuples, Dict construction and iteration
- strings and iteration helpers

The first subset excludes areas that are not useful for a Node-hosted wasm
smoke or are already covered by target-specific gates:

- filesystem, path, process, logging, and terminal IO fixtures: browser/Node
  host capabilities differ from native CLI behavior.
- package loading, plotting, iOS, FFI, and AoT fixtures: those exercise other
  backends or platform glue rather than `subset_julia_vm_web` source execution.
- concurrency and timing fixtures: scheduling and clock behavior are
  host-dependent in wasm.
- fixtures that intentionally fail or compare diagnostic formatting: #8710 is
  a source-execution smoke; diagnostic parity belongs to #8690/#8713.

## Initial Result

Run on 2026-07-03:

```bash
scripts/wasm_fixture_smoke.sh --skip-build
```

Result: 43 selected fixtures passed, 0 failed. No wasm-vs-native divergence was
found in the initial subset, so no follow-up divergence Issue was filed.

The command writes the detailed TSV result to
`target/wasm-fixture-smoke/results.tsv` and the non-`ok` comparison rows to
`target/wasm-fixture-smoke/diff.tsv`.

## Nightly Tier

`nightly-gates.yml` runs this smoke after the scheduled WASM build job. This is
the semantic execution tier only: it proves that selected Julia fixture sources
compile and execute through the wasm package in Node and that their final
values match the expected result. It intentionally does not cover browser UI,
canvas/plot rendering, DOM integration, visual artifacts, download behavior, or
iOS/FFI embedding.

When a fixture is a known wasm-only divergence, add a row to
`WASM_FIXTURE_SMOKE_ALLOWLIST.tsv` with the linked Issue and reason. The
allowlist is a ratchet: an unexpected failure fails the smoke, a now-passing
allowlisted row fails as stale, and an allowlist row for a non-selected fixture
also fails as stale.
