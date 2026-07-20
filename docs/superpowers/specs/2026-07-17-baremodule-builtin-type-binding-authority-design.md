# Baremodule Builtin Type Binding Authority Design

## Problem

SubsetJuliaVM recognizes builtin type spellings before it checks whether the
current lexical module is allowed to see the binding. In a `baremodule`, this
lets Base-owned names such as `BigInt` reach type annotations, `isa`, and `<:`
even though upstream Julia raises `UndefVarError` unless the module explicitly
imports Base (Issue #11419).

The failure is broader than the three reported expressions. Any compiler path
that converts a bare name directly into a builtin `JuliaType` can erase the
binding's owning module and bypass lexical visibility. A fix that special-cases
`BigInt` or only guards the reported operators would leave the same bug class
reachable through other builtin type names and future compiler consumers.

## Upstream model

Julia creates ordinary modules with standard imports, including `using Base`.
A `baremodule` skips those imports but still receives Core implicitly. Lowered
bare names remain bindings of the current module; they are not rewritten into
unconditional Base/Core type constants. Consequently:

- Core-owned type bindings are visible in both `module` and `baremodule`;
- Base-owned type bindings are visible in an ordinary `module`;
- Base-owned type bindings become visible in a `baremodule` only through
  `using Base` or an explicit import/using of that binding. Merely importing
  the `Base` module name does not expose its exported type names.

SubsetJuliaVM will preserve its static type projection while enforcing this
same binding authority before projection.

## Chosen architecture

The compiler will expose one authority query for builtin type visibility in the
current lexical module. The query uses the canonical
`builtin_type_binding_authority` registry rather than classifying names at each
consumer. It combines the registry's Core/Base ownership with existing module
state: whether the current module is bare and which export-using or named
import/using edges are active.

The invariant is:

- a Core-owned builtin type is visible through the implicit Core edge;
- a Base-owned builtin type is visible through an ordinary module's implicit
  Base export-using edge, `using Base`, or an active named import/using edge for
  that binding;
- a builtin name with no visible authority must not be projected to a
  `JuliaType` merely because its spelling is registered.

The existing visible-type resolver will use this authority query, and direct
builtin fast paths will delegate to the same query. This makes the resolver the
policy boundary while retaining optimized type representation after visibility
has been established.

## Compiler consumers

The shared authority must cover every path exercised by Issue #11419:

1. `Expr::Var` may take the builtin-type fast path only after the authority
   query succeeds. This also protects type operands compiled indirectly.
2. `isa` may perform compile-time folding only when the right-hand type binding
   is visible. An invisible name must follow the undefined-binding path rather
   than produce a folded Boolean.
3. `<:` receives its type operands through authority-aware expression/type
   resolution, so neither side can acquire a Base-owned type by spelling alone.
4. Signature definition probes must not blanket-skip every registered builtin
   type name. They may skip a probe only when the shared query proves that the
   binding is visible. An invisible annotation is rejected before method
   activation.

Parametric builtin types and aliases are not separate exceptions. Resolution
checks the root binding authority before projecting or expanding the full type
expression. Existing source-order and imported-type rules continue to apply
after the binding has been proven visible.

## Error behavior

An invisible Base-owned name in a `baremodule` raises the existing
Julia-compatible undefined-binding error for that lexical module. The compiler
must not silently return `false`, report an unrelated type error, or register a
method with an unreachable signature.

The authority query is a visibility predicate, not a new error constructor.
Consumers preserve their normal source spans and route failure through the
existing undefined-name probe/error path. Ordinary modules, explicit Base
imports/usings, and implicit Core access remain behavior-preserving positive
cases.

## Tests

Regression coverage belongs in an existing consolidated module fixture/test
binary. It will include the following matrix:

- Base-negative `baremodule` cases for a type annotation, `isa`, and `<:`;
- `using Base`-positive `baremodule` cases for the same three consumers;
- a negative `import Base` control and a positive named-import control;
- Core-positive `baremodule` cases for the same three consumers;
- at least one non-`BigInt` or parametric builtin case proving that the policy
  is ownership-based rather than spelling-specific;
- ordinary-module controls proving that implicit Base visibility is unchanged.

Every Julia fixture is run with upstream Julia first and ends with `true` when
the fixture form permits it. Focused compiler tests exercise the authority
query and the no-fold/no-activation behavior. Verification then proceeds
sequentially through a refreshed `sjulia` build with `--features repl`, direct
reproduction, the relevant release-fast category, formatting, default Clippy,
the required iOS device/simulator builds, and the full release nextest suite in
the guarded pre-merge gate.

## Delivery

The implementation remains linked to Issue #11419 and is published as a Draft
PR as soon as the regression and structural fix form a reviewable commit. It
stays draft until the exact head and exact current `origin/main` pass the local
guarded certification. The lead flow then marks it ready and performs a regular
merge.

After merge, the post-mortem records the reusable binding-authority lesson and
files or updates prevention work if a repo-wide audit can detect future direct
builtin-name projection that bypasses authority.

## Alternatives rejected

Per-consumer guards are smaller initially but duplicate policy across
annotations and operators, which is the mechanism that produced this drift.
A complete lowering rewrite to module-qualified global references would be
closer to upstream's representation, but it crosses a much larger type,
lowering, cache, and dispatch boundary than Issue #11419 requires. That broader
retirement can be handled by the existing tech-debt program without weakening
this structural visibility fix.

## Non-goals

- Replacing the complete lowering/name-resolution representation.
- Changing JuliaType or bytecode serialization formats.
- Adding name-, package-, or consumer-specific exceptions.
- Expanding AoT behavior unless verification shows the shared compiler change
  reaches an AoT-specific path that must be kept in parity.
