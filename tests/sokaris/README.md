# Sokaris generated-Wasm coverage contract

`coverage.json` is the fail-closed inventory for generated-Wasm parity. It has
one row for every binding exported by the real sibling sources
`../sokaris/src/Glyph.jl` and `../sokaris/src/Imhotep.jl`: 14 Glyph exports and
27 Imhotep exports, 41 total.

Run the dependency-free checker from any directory:

```sh
node scripts/check_sokaris_wasm_coverage.mjs
node /absolute/path/to/subset-julia-wasm-compiler/scripts/check_sokaris_wasm_coverage.mjs
```

The script resolves its repository and source-oracle paths from the script URL,
not the caller's current working directory. `--manifest <path>` selects an
isolated manifest for negative tests. `--production-root <path>` replaces the
normal compiler Rust scopes so a temporary shortcut can be tested without
editing production code. Relative override paths are caller-CWD-relative.

## Schema version 1

Top-level fields:

- `schemaVersion`: exactly `1`.
- `sourceFiles`: exactly `Glyph` and `Imhotep`. Each entry pins the
  Sokaris-relative `file`, whole-file `sha256`, and exact `expectedExports`.
- `rows`: exactly 41 coverage rows.

Every row contains:

- `module` and `symbol`: the unique source export identity.
- `source`: Sokaris-relative `file`, one-based inclusive `startLine` and
  `endLine`, and SHA-256 of those lines joined with `\n`. A span must contain
  its symbol. Multiple method definitions may share one span; imported exports
  such as `load` and `save` point to their import declaration.
- `caseId`: globally unique stable differential-test identifier.
- `fixture` and `case`: explicit fixture/case placeholders until the later
  differential harness replaces them with executable cases.
- `exportWrapper`: globally unique generated-Wasm wrapper name.
- `arguments` and `result`: static descriptors. Every descriptor has a Julia
  `type`, generated ABI `elementType`, and non-negative `rank`; rank zero is a
  scalar, aggregate, callable, or `Nothing` descriptor.
- `comparison`: `mode` (`exact`, `absolute`, `relative`, or `nan-aware`) and a
  finite non-negative `tolerance`. Exact comparisons require zero tolerance.
- `requiredImports`: generated-Wasm imports as `{ "module", "name" }` rows.
- `effect`: `compute` or `host`.
- `wave`: implementation grouping used by later plan tasks.
- `status`: `planned`, `red`, or `green`. This inventory starts at `planned`;
  later tasks advance a row only with differential evidence.

## Enforced invariants

The checker independently parses every Julia `export` declaration. It rejects
missing, extra, or duplicate exports/rows; duplicate cases or wrappers; unknown
schema values; malformed descriptors; stale whole-file or span hashes; invalid
spans; and an effect/import mismatch. Exactly `load`, `save`, and
`text_overlay` are `host` rows and each declares exactly the matching
`sjulia_host` import. The other 38 rows are `compute` and import-free.

It also audits production compiler Rust only. Tests, documentation, comments,
and manifest metadata may name Sokaris and its exports, but production
matching/control-flow logic may not compare, branch, or match on Sokaris
package/module/export/transform string literals. The default explicit scopes
cover the AoT compiler, compile/lowering crates, browser compiler API, and AoT
CLI. The dependency-free scanner recognizes plain, byte, and raw Rust strings,
nested comments, and same-file `const`/`static` string indirection. It rejects
symlinked scopes and bounds directory depth, file count, and input sizes. The
audit refuses a missing or empty scope.

When Sokaris source intentionally changes, update the source span and hashes in
the same change as the corresponding coverage decision. Do not refresh hashes
merely to silence the checker: first confirm the parsed export set and row ABI
remain correct.

## Julia-first differential harness

The dependency-free harness validates this coverage contract before selecting
cases, runs the pinned sibling Julia project before compiler work, invokes the
checked `pkg-compiler-final` browser compiler artifact, executes generated Wasm
in Node, and writes schema-versioned NDJSON under `target/sokaris-parity/`.

```sh
node scripts/sokaris_wasm_differential.mjs --case glyph-apply --require-upstream
node scripts/sokaris_wasm_differential.mjs --case glyph-compose --require-upstream
node scripts/sokaris_wasm_differential.mjs --wave glyph --require-upstream
node scripts/sokaris_wasm_differential.mjs --module Glyph --require-upstream
node scripts/sokaris_wasm_differential.mjs --all --require-upstream
```

Exactly one selector is required. Upstream Julia is mandatory even when
`--require-upstream` is omitted; the flag documents the project safety policy.
`--keep-artifacts` preserves per-case bundles and Wasm files, while NDJSON is
always retained. `SOKARIS_JULIA` is the test/deployment seam for the Julia
executable, and every child process has a bounded timeout.

The initial `glyph-apply` executable fixture proves harness mechanics through an
already-supported scalar compiler path and records `mechanics_passed`; it does
not advance the coverage row from `planned` or claim that the generic closure
body compiled. `glyph-compose` proves Julia-first ordering followed by a typed
planned compiler diagnostic. Later parity Todos replace pending fixtures and
add descriptor ABI v2 result decoding without changing the harness protocol.
