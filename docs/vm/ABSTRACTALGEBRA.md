# AbstractAlgebra.jl Support Audit

Issue: #7486 / parent milestone #7439
Reference upstream: AbstractAlgebra.jl 0.50.1.

## MVP Scope

AbstractAlgebra support is phased as a bundled pure-Julia compatibility package,
following the existing package-loader pattern used by MacroTools, StaticArrays,
Symbolics, and Distributions. The MVP is not a full upstream vendor import. It
keeps the upstream include shape where that shape is the compatibility contract,
then implements only the package-load and early algebra API needed by the phase
issues.

The initial MVP includes:

- package load through `using AbstractAlgebra`;
- dependency gates for `LinearAlgebra`, `MacroTools`, `PrecompileTools`,
  `Preferences`, `Random`, `RandomExtensions`, and `SparseArrays`;
- the early macro/AST files `AliasMacro.jl`, `Aliases.jl`, `Assertions.jl`,
  `Attributes.jl`, `AbstractTypes.jl`, and `ConcreteTypes.jl`;
- public seed names for `ZZ`, `QQ`, parent/element traits, aliases, and the
  integer/rational/poly entry points used by later phases.

## Upstream Dependency Map

AbstractAlgebra.jl 0.50.1 declares these direct dependencies in `Project.toml`:

- stdlib: `LinearAlgebra`, `Random`, `SparseArrays`;
- package: `MacroTools`, `PrecompileTools`, `Preferences`,
  `RandomExtensions`;
- weak deps/extensions: `IJulia`, `Test`.

The first support tranche should keep `IJulia` and exhaustive `Test` extension
behavior deferred. `PrecompileTools` can use the existing no-op compatibility
shim because sjulia does not execute Julia package precompile hooks at runtime.
`Preferences`, `RandomExtensions`, and `SparseArrays` should be compatibility
shims only to the extent required by package load and early includes.

## Upstream Include Order

The top-level upstream include order in `src/AbstractAlgebra.jl` starts with:

1. `imports.jl`
2. `exports.jl`
3. `AliasMacro.jl`
4. `Aliases.jl`
5. `Assertions.jl`
6. `Attributes.jl`
7. `PrintHelper.jl`
8. `PrettyOrdering.jl`
9. `WeakKeyIdDict.jl`
10. `WeakValueDict.jl`
11. `AbstractTypes.jl`
12. `julia/JuliaTypes.jl`
13. `ConcreteTypes.jl`
14. `fundamental_interface.jl`
15. `misc/VarNames.jl`
16. `Infinity.jl`
17. `PrettyPrinting.jl`
18. `ConformanceTests.jl`
19. generic algorithm and algebra family files.

For #7487/#7488 acceptance, only the early gate through
`ConcreteTypes.jl` is in scope. Later files such as polynomial algorithms,
matrix normal forms, number fields, sparse polynomial code, package extensions,
and precompile workloads stay deferred until phases #7489-#7493.

## Phase Mapping

- Phase 0 (#7486): document the source map and add parse-only seed fixtures for
  the top-level `@doc raw"""..."""` plus early module/alias/type skeleton.
- Phase 1 (#7487): bundle `AbstractAlgebra` and dependency shims so
  `using AbstractAlgebra` resolves through the normal package loader. The
  skeleton preserves `imports.jl` / `exports.jl` include boundaries and keeps
  the full early macro/type files for Phase 2.
- Phase 2 (#7488): support the parser, lowering, macrocall, nested quote, `esc`,
  `Expr(...)`, and module hygiene constructs needed by `AliasMacro.jl`,
  `Aliases.jl`, `Assertions.jl`, `Attributes.jl`, `AbstractTypes.jl`, and
  `ConcreteTypes.jl`.
- Phase 3 (#7489): add the core algebra type hierarchy, aliases, traits, and
  public seed names.
- Phase 4 (#7490): `ZZ`/`QQ` ground-ring and exact arithmetic MVP.
- Phase 5 (#7491): univariate polynomial, fraction, and residue-ring MVP.
- Phase 6 (#7492): matrices, modules, maps, and conformance tranche.
- Phase 7 (#7493): validation, docs, performance, and iOS/WASM readiness.

## Explicit Deferrals

- Exhaustive upstream tests and optional package extensions.
- IJulia integration.
- Noncommutative Groebner algorithms and advanced number fields.
- Full sparse linear algebra.
- Complete generic algorithm coverage beyond the phase-specific MVPs.

## Phase 1 Package Skeleton

Issue #7487 adds `AbstractAlgebra` under `subset_julia_vm/packages/` with the
upstream 0.50.1 dependency gate in `Project.toml`. The bundled source is a
loader skeleton: it imports the required dependencies, defines the upstream
`import_exclude` seed list, and includes `imports.jl` / `exports.jl` as separate
files so include hashing and virtual package path resolution match the existing
bundled package pattern.

`Preferences`, `RandomExtensions`, and `SparseArrays` are also registered as
minimal compatibility packages because `Project.toml` dependency loading is
eager. They intentionally expose only the names needed for dependency
resolution in this phase; full preference, random-extension, and sparse-array
behavior remains outside the AbstractAlgebra MVP until a later issue needs it.

## Phase 2 Macro/Type Driver

Issue #7723/#7488 extends the skeleton through the early upstream macro/type
driver:

- `AliasMacro.jl`, `Aliases.jl`, `Assertions.jl`, `Attributes.jl`,
  `AbstractTypes.jl`, and `ConcreteTypes.jl` are included in the same relative
  order as upstream's early tranche.
- `@alias`, `@attributes`, and `@req` are exported; `@req` is usable after
  `using AbstractAlgebra`.
- `PolynomialElem` and `MatrixElem` are module type aliases and resolve after
  `using AbstractAlgebra`.
- `MatSpace` and macro-expanded `UniversalRing` are registered as module type
  bindings and visible through `names(AbstractAlgebra)` /
  `isdefined(AbstractAlgebra, :UniversalRing)`.

The driver intentionally stops before full attribute runtime behavior and
full algebra construction semantics. Those gaps are tracked separately in
#7933, #7934, #7935, #7940, #7941, and #7948, and listed in
`docs/vm/UNIMPLEMENTED.md` / `docs/vm/WORKAROUNDS.md`.
