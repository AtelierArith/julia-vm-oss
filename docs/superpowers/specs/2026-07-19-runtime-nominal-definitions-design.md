# Runtime Nominal Definitions in Top-Level Control Flow

**Issues:** #9784, #11654
**Date:** 2026-07-19
**Status:** Owner-approved design; implementation pending

## Objective

Make nominal type declarations obey Julia's runtime source order when they are
nested in top-level `if`, `for`, `while`, or `try` forms. The supported nominal
families are concrete `struct`, `abstract type`, `primitive type`, and `@enum`.
Executing a control-flow path publishes exactly the declarations reached on
that path; a skipped path publishes nothing.

This is a direct #9784 slice. It replaces the current split behavior in which
nested concrete structs are routed through opaque runtime `eval`, nested
abstract and primitive declarations fail during lowering, and nested enums
publish member constants without publishing their type. The result must use one
structured compiler/VM definition mechanism rather than adding more syntax-
specific AST interpretation.

## Upstream Evidence and Required Matrix

The behavior below was measured with upstream Julia and the exact current
`target/dev-fast/sjulia` built from `main` before this design.

| Control form | `struct` | `abstract type` | `primitive type` | `@enum` |
|---|---:|---:|---:|---:|
| executed `if` | publish | publish | publish | publish type and members |
| one-iteration `for` | publish | publish | publish | publish nothing |
| one-iteration `while` | publish | publish | publish | publish type and members |
| executed `try` | publish | publish | publish | publish type and members |

The `@enum`/`for` cell is deliberately different. Upstream `@enum` expands
through a top-level expression whose loop placement does not create either the
enum type or its members. sjulia must preserve that upstream macro-placement
rule; it must not normalize the cell to the behavior of the other nominal
families.

Additional upstream constraints are part of the acceptance contract:

- a false `if`, zero-iteration `for`, or unentered `while` publishes nothing;
- a declaration failure inside `try` is Julia-catchable and does not publish
  the failed type;
- a successful declaration before a later caught or uncaught error remains
  visible;
- declarations inside a function body remain invalid and are rejected; and
- using `try` as an expression has the same declaration behavior as statement
  `try`.

The original #11654 abstract-parent example produces `(true, false, true)` in
Julia: the first declaration succeeds, the declaration with an undefined
parent fails without publication, and the catch branch can publish a later
declaration. This is the minimum recovery case, not an exception to the general
model.

## Scope

### In scope

- top-level `if`, `for`, `while`, and `try`, including nested combinations;
- `struct`, `abstract type`, `primitive type`, and `@enum` declaration
  statements in those forms;
- statement lowering and expression lowering where the control-flow form can
  appear as an expression;
- source-ordered publication, duplicate-definition errors, parent resolution,
  generated constructors, enum members, and dispatch-visible hierarchy data;
- successful REPL continuation and Julia-catchable error recovery using the
  exact declarations that executed;
- serialized program/cache schema changes required by the new structured
  statement and bytecode representation; and
- fail-closed behavior in AoT, whose guaranteed acceptance-kernel scope does
  not include runtime nominal declarations.

### Out of scope

- making type declarations valid inside function bodies;
- changing Julia's redefinition rules;
- extending `eval` to interpret more declaration ASTs;
- package-name, macro-name, or type-name special cases;
- expanding the AoT guaranteed feature surface; and
- completing the remaining #9784 retirement list.

## Selected Architecture

### 1. A structured runtime declaration in lowered IR

Add a `Stmt::RuntimeNominalDef` form whose payload is a typed declaration
template, not an opaque quoted Julia expression. The payload has one variant
for each nominal family and preserves all information already produced by
normal declaration lowering:

- owner/module and declared binding;
- type parameters where the existing family supports them;
- parent expression or resolved parent requirement;
- field names, field types, mutability, and generated-constructor templates for
  concrete structs;
- primitive width and parent for primitive types;
- enum carrier information, ordered member values, and ordered binding stores;
- declaration span and a stable definition-site token; and
- relocatable compiler artifacts needed to install constructors or methods
  after the runtime type identity is allocated.

The lowerer emits this statement only for declarations whose visibility is
runtime-conditional because they are inside supported top-level control flow.
Root declarations keep the existing pre-reserved activation path. Function-
body declarations continue to return the existing Julia-compatible lowering
error.

This representation removes the nested-struct opaque-`eval` route introduced
for #10401. It also avoids extending the separate runtime AST walker tracked by
#11285. There is one declaration representation shared by lowering, compiler,
VM installation, REPL recovery, and cache serialization.

### 2. Runtime templates are not active type-registry entries

Compiling an input may discover every declaration syntax node, but a
`RuntimeNominalDef` contributes only an inert template pool entry. It does not
reserve an active type ID, add a name binding, extend a hierarchy, or enter a
pending FIFO queue.

This distinction is necessary for conditional control flow. Pre-reserving all
branch declarations would create holes whenever an earlier branch is skipped
and a later sibling is reached. The current nominal activation recovery model
can safely project a reached prefix; it cannot infer an arbitrary non-
contiguous reached subset. Runtime allocation eliminates that assumption.

Code in the same input that reads a conditionally declared name uses the
world-visible/dynamic binding and dispatch paths. It must not embed an active
type ID merely because the compiler saw the syntax. Constructor and method
templates owned by the declaration use relocatable symbolic references until
the installer assigns the actual runtime identity. A later REPL input can use
normal static identities after the activation event has been committed to the
persistent compiler snapshot.

### 3. Dedicated bytecode at the exact execution point

The compiler emits an append-only runtime-definition instruction referencing
the structured template pool. The instruction executes exactly where the
declaration appeared in the control-flow bytecode. It has no effect if its
branch or loop body is not entered.

The VM instruction delegates to a single nominal-definition installer. The
installer performs these phases:

1. resolve the owner, parent, field types, carrier, and binding prerequisites
   against the current runtime world;
2. validate duplicate/redefinition rules and all family-specific invariants
   without publishing the new type;
3. allocate the next runtime identity and relocate any declaration-owned
   constructor or method artifacts;
4. publish the type binding, hierarchy/ancestor data, constructors, dispatch
   surfaces, and family-specific metadata in Julia source order; and
5. append a successful activation event containing the actual assigned
   identities and structured declaration provenance.

A failure in phases 1 or 2 allocates and publishes nothing. An internal failure
after mutation begins is an invariant violation: the live transaction is
rejected and the VM is dropped rather than guessing a compiler snapshot.
Julia-level publication errors retain precisely the side effects upstream Julia
would already have made, as described below.

### 4. One actual-execution activation log

Extend the VM-to-REPL activation protocol with a runtime nominal event that
contains the complete committed payload needed by later compilation:

- definition-site token and source span;
- nominal family and owner binding;
- assigned runtime type identity;
- resolved parent/hierarchy identity;
- installed constructor/method identities; and
- for enums, the type activation and exact successfully stored member prefix.

The VM appends events in actual execution order. Runtime-conditional events are
validated against their immutable template token and payload, not against a
statically predicted prefix. A definition site cannot successfully activate
twice under Julia's nominal redefinition rules, so a second reached iteration
raises the normal Julia error and adds no second successful activation.

Existing root-level pre-reserved declarations may continue to validate their
planned prefix. The interleaved transaction log remains chronological, but
runtime nominal events are committed as an observed sequence. A skipped branch
therefore creates neither an event nor a registry hole.

On successful input completion, the session applies all observed events to
`ReplPersistentCompile` in runtime order. On an uncaught Julia exception, it
applies the same successful event sequence before making the next input
available. If the exception is caught within the input, execution naturally
continues and later successful declarations append later events. This supports
the #11654 pattern without reconstructing a prefix from source syntax.

### 5. Compiler snapshot adoption

Applying an activation event appends the structured definition to the matching
compiler registry using the runtime-assigned identity. It also updates every
derived surface used by later lowering and compilation:

- type-name and owner-qualified lookup;
- struct, abstract, primitive, and enum registries;
- hierarchy and ancestor projections;
- constructor and method ownership;
- global binding type information;
- dispatch/backedge invalidation inputs; and
- stored source definitions required by the remaining #9784 fallback.

The session never infers these updates by rescanning the original control-flow
AST and never assumes that source-order template indices equal runtime type
IDs. The activation event is authoritative because it records what executed.

If event validation, relocation, or snapshot adoption finds a VM/compiler
identity mismatch, the session fails closed and discards the held VM. It must
not publish a partially guessed persistent compile state.

## Lowering Semantics

### Context propagation

Both statement and expression lowering must carry the same `LambdaContext`
through `if`, `for`, `while`, and `try`. In particular, the context-aware
expression arm for `try` must call a context-aware try helper; it must not fall
back to the current contextless `lower_try_as_expr` path. This is why a concrete
struct currently works in statement `try` but fails when the same `try` is used
as an expression.

The declaration decision is based on semantic context:

- root top level: existing root declaration representation;
- top-level control flow: `RuntimeNominalDef` or the upstream enum/for no-op;
- function/lambda body: reject; and
- quoted syntax: preserve syntax without executing a declaration.

### The `@enum` inside `for` rule

Macro lowering records whether an enum expansion occurs under a top-level
`for`. For this placement, it emits no runtime enum definition and no member
stores, matching the measured upstream result. This is an upstream macro
placement rule, not a general ban on nominal declarations in loops; concrete,
abstract, and primitive declarations still execute in a reached loop body.

An enum under `if`, `while`, or `try` emits one runtime nominal event at the
macro call's execution point. On successful execution, the enum type and all
members are visible as one reached declaration group. Publication internally
retains Julia's ordered subevents: the type is published before member stores,
and a member collision can therefore leave the type plus the exact earlier
member prefix visible, as required by #11652 and #11656.

## Error and Transaction Semantics

The common installer follows these rules for every nominal family:

- an undefined or not-yet-published parent raises `UndefVarError` before the
  child is allocated or bound;
- an invalid parent or primitive width raises the family-appropriate Julia
  error before publication;
- a duplicate declaration raises the upstream-shaped redefinition error and
  does not replace the existing identity;
- a skipped control-flow path produces no side effect and no activation event;
- a successfully published declaration remains visible if later code throws;
  and
- a caught failure does not prevent a later catch/finally declaration from
  publishing.

Enum publication has finer observable ordering. Reaching the declaration first
publishes the enum type, then validates and stores members in the shared
`julia_enum_member_binding_order`. An existing colliding global is never
overwritten. Recovery records the enum metadata separately from the exact
member stores that completed so a later full rebuild cannot revive a rejected
or unreached member.

No catchable failure may leave an identity that is present in the VM but absent
from the compiler snapshot, or vice versa. Internal failures that prevent exact
event adoption invalidate the live VM instead of continuing with divergent
authorities.

## Serialization, Cache, and Exhaustive Consumers

`RuntimeNominalDef`, its template payload, the bytecode instruction, and the
activation-event payload are serialized structures. Their introduction must:

- use append-only instruction and statement wire identifiers;
- bump the applicable program/Base-cache schema or fingerprint;
- update encode/decode round-trip tests;
- update bytecode validation, display/dump, relocation, stack-effect, and
  instruction-audit consumers;
- update statement visitors, definition collectors, and source-map handling;
  and
- avoid wildcard matches that silently ignore the new form.

Old caches must fail with the normal schema mismatch and rebuild. They must not
decode the new variant as an older instruction.

## AoT Behavior

Runtime nominal definition is outside the current AoT acceptance-kernel scope.
AoT conversion must recognize the new instruction and return a typed unsupported
diagnostic that carries the declaration span. It must not panic, silently skip
the definition, or generate a partially initialized type.

Because the shared IR/compiler path changes, the complete AoT gate remains
mandatory even though the new operation is rejected by AoT.

## Test Strategy

Tests are added to existing consolidated test binaries and fixture categories;
no per-issue test binary is created. TDD begins with upstream-parity fixtures
that fail on current sjulia.

### Required parity fixtures

1. The complete 16-cell family/control-flow matrix shown above.
2. False `if`, zero-iteration `for`, and initially false `while` for every
   family, proving absence of bindings and activation events.
3. A one-iteration `while` and a second-iteration redefinition error.
4. Statement `try` and expression `try` with the same nominal declarations.
5. The original #11654 sequence: successful definition, failed late-parent
   definition, and successful catch definition, yielding `(true,false,true)`.
6. A definition followed by a later uncaught error, then a subsequent REPL
   input that reads/constructs/dispatches on the reached type.
7. Conditional sibling declarations where an earlier site is skipped and a
   later site executes, proving no static-ID hole or prefix assumption.
8. Nested control flow, including `try` in `if` and `if` in `try`.
9. Function-body declarations for every family, proving they remain rejected.
10. Enum type/member order and collision recovery using the shared Julia slot
    order, plus the special `for` no-publication result.

Each fixture is first run with upstream `julia --startup-file=no`; expected
outputs assert bindings, construction, hierarchy, dispatch, `instances`, and
error class rather than relying only on `@isdefined`.

### Rust and protocol tests

- lowerer snapshots distinguish root, control-flow, function, expression-try,
  quote, and enum-under-for contexts;
- instruction encode/decode and cache rejection cover the new wire variant;
- VM unit tests prove parent validation happens before allocation;
- REPL transaction tests compare the actual activation sequence and runtime
  IDs for skipped/reached sibling branches;
- caught and uncaught errors adopt exactly the successful event sequence;
- a forced next-input fallback reconstructs the same reached definitions and
  enum member prefix; and
- AoT returns the typed unsupported diagnostic while existing acceptance
  kernels remain green.

## Documentation and Migration

The implementation updates `docs/vm/UNIMPLEMENTED.md`: its existing #10401
entry still claims that concrete structs inside control flow are unsupported,
although the fixture is already green. The updated entry documents the unified
runtime nominal path and keeps only genuinely unsupported contexts.

The #10401 opaque-`eval` nested-struct special path is removed once all parity
tests pass through `RuntimeNominalDef`. Any temporary coexistence during the
TDD sequence must not survive the final PR.

## Rejected Alternatives

### Extend the runtime `eval` AST walker

Adding abstract, primitive, and enum cases beside its existing struct case
would create another full declaration semantics implementation. It deepens
#11285, duplicates compiler validation and constructor generation, and leaves
REPL recovery dependent on opaque side effects.

### Pre-reserve every syntactic declaration

This works only when reached declarations form a source prefix. Conditional
siblings and skipped loop bodies form arbitrary subsets, so reservation creates
type-ID holes and makes the existing FIFO/prefix recovery invalid. Rolling back
unreached entries after execution cannot safely remap already-published runtime
identities.

### Add separate control-flow handling for each family

Four lowerer/compiler/VM paths would duplicate source-order, parent validation,
binding publication, hierarchy updates, error recovery, and cache rules. The
invariant is one runtime nominal transaction with family-specific payloads.

## Acceptance Criteria

The issue is complete when:

- all 16 upstream matrix cells match, including enum-under-for;
- skipped branches and failed declarations publish nothing;
- caught and uncaught errors preserve exactly the successfully executed
  declarations, including non-prefix sibling paths;
- statement and expression `try` behave identically;
- function-body type declarations remain rejected;
- the old nested-struct opaque-`eval` path is removed;
- subsequent REPL inputs compile and run against runtime-assigned identities;
- cache/wire audits, default clippy, formatting, relevant fixture categories,
  full release nextest, metamorphic equivalence, and the full AoT gate pass;
- stale #10401 documentation is corrected; and
- the draft PR is certified and regular-merged through
  `scripts/premerge_gate.sh --pr <N> --full-suite`.
