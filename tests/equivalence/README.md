# Metamorphic equivalence-lane corpus (Issue #10465)

Inputs for `scripts/metamorphic_equivalence.sh`, a **differential** harness that
checks

```
normalize(run(program, lane A)) == normalize(run(transform(program), lane B))
```

for semantics-preserving `transform`s, over the **same** sjulia binary. This is
sjulia-vs-sjulia (does one call/scope shape diverge from another?), NOT
sjulia-vs-upstream parity — the latter is the existing fixture parity gate.

It exists because most fixtures assert one concrete execution path, while
recurrent bugs live *between* paths that should be identical (root-cause classes
#3 and #7 of the #10452 analysis).

## Lanes in #10465

### direct_callable — `direct_callable.tsv`

For each case, a public function/constructor is invoked four ways and every lane
must produce the same result value, result type, and exception class:

| lane   | invocation        |
|--------|-------------------|
| direct | `f(x)`            |
| base   | `Base.f(x)`       |
| bind   | `g = f; g(x)`     |
| hof    | `map(f, [x])[1]`  |

The first lane listed for a case is the reference; the rest are compared to it.
This is exactly the #10187 / #10250 divergence family (compiler intercept vs
Base / first-class / HOF dispatch).

TSV columns: `name`, `preamble` (`-` if none), `call`, `arg`, `lanes`.

### module_wrap — `module_wrap/*.jl`

Each `.jl` runs twice: verbatim at `Main` top-level, and wrapped in a generated
unique `module … end`. Observations are compared after normalizing the generated
module-name prefix and source line/col. Only **wrap-safe** fixtures belong here;
wrap-unsafe patterns are documented in
`docs/vm/EQUIVALENCE_MODULE_WRAP_EXCLUSIONS.tsv`.

### fresh_cache — `fresh_cache/*.jl`

Each `.jl` runs in two isolated runtime-cache lanes:

| lane   | invocation |
|--------|------------|
| fresh  | all sjulia persistent caches disabled, isolated target/cache dirs |
| cached | isolated persistent target/cache dirs, one priming run followed by a restored-cache observation run |

This guards cache-restore semantic parity for small, curated cases without
running the full cache-sensitive fixture lane. It is intended for regressions
whose observation should be byte-for-byte identical across fresh and restored
persistent Base/prelude/cache state.

### generic_optimized — `generic_optimized/*.jl`

Each `.jl` runs twice through the same `sjulia` binary:

| lane      | invocation |
|-----------|------------|
| generic   | `SJULIA_SSA_PIPELINE=0 sjulia case.jl` |
| optimized | default `sjulia case.jl` |

This guards optimizer-induced semantic drift. The corpus is intentionally small
and deterministic; it should cover optimizer-sensitive shapes without becoming a
second fixture suite.

### vm_aot — `vm_aot.tsv`

Each manifest row points at a canonical AoT acceptance fixture and compares VM
execution with a generated AoT binary:

| lane | invocation |
|------|------------|
| vm   | `target/release/sjulia fixture.jl` |
| aot  | `target/release/juliars fixture.jl --minimal-prelude -o generated.rs --emit-binary bin && bin` |

The manifest keeps this lane scoped to the documented AoT acceptance programs
instead of copying their bodies into the equivalence corpus. The lane uses
`--minimal-prelude` because the full-Base AoT path is intentionally broader than
the acceptance scope and still gates on unsupported BigInt constructor lowering
(Issue #6975).

## Normalizers (lane-induced noise only — never semantic values)

- `at line N:M` / `at line N` / `@ file:N` source-location suffixes;
- the generated unique module name prefix (module_wrap only);
- `Stacktrace:` frames (the call path differs by lane; the error class/message
  body is kept and compared).

## Tracked divergences

`docs/vm/EQUIVALENCE_KNOWN_DIVERGENCES.tsv` registers Issue-linked lane
divergences. It is **two-sided**: an un-registered divergence fails the gate
(file a `bug` Issue first, then register), and a registered divergence that later
agrees fails as STALE (remove the row and close the Issue). Running the harness
previously surfaced #10512 (`map(string,[42])[1]`), and the stale-row ratchet
later removed that row once the divergence was fixed.

## Local commands

```bash
bash scripts/metamorphic_equivalence.sh                 # all lanes, curated corpus
bash scripts/metamorphic_equivalence.sh --lane direct_callable
bash scripts/metamorphic_equivalence.sh --lane module_wrap
bash scripts/metamorphic_equivalence.sh --lane fresh_cache
bash scripts/metamorphic_equivalence.sh --lane generic_optimized
bash scripts/metamorphic_equivalence.sh --lane vm_aot
bash scripts/metamorphic_equivalence.sh --list          # list cases, run nothing
bash scripts/metamorphic_equivalence.sh --selftest      # negative + positive self-tests
bash scripts/premerge_gate.sh --metamorphic             # force it; semantic paths select it automatically
```
