# REPL Runtime Global-Write Authority Design

**Issue:** #9784  
**Date:** 2026-07-19  
**Status:** Approved direction

## Goal

Remove `extract_assigned_variables` from the production REPL and from the tree.
After an evaluation starts, only stores that actually execute in the VM may
create, update, or invalidate a persisted Julia value binding. A syntactically
present but unreachable assignment must have no persistence effect.

This is one independently mergeable retirement slice of the broader
single-live-toplevel design. It does not claim to close #9784: the remaining
fresh full-recompile fallback and its other reconstruction helpers are retired
by later slices.

## Upstream authority

Julia evaluates successive REPL inputs in one module through
`jl_toplevel_eval_flex` (`julia/src/toplevel.c`) and writes module bindings
through the module binding table (`julia/src/module.c`). Persistence is a
consequence of an executed global store, not a scan of all assignments present
in the input AST.

Direct upstream checks establish the required boundary:

- an assignment in an untaken branch creates no binding;
- a `let` local does not become a `Main` binding;
- `global x = ...` inside a `let` creates or updates the `Main` binding;
- `global x = ...` inside a called function does the same;
- an unreachable assignment to a prior static type alias does not invalidate
  the alias;
- an assignment reached before a thrown error does replace that alias with a
  value binding.

## Current problem

`extract_assigned_variables` recursively walks lowered statements and serves
two unrelated purposes:

1. after execution, it guesses which VM globals should be mirrored into the
   transitional full-recompile state;
2. before execution, it conservatively identifies names that may shadow a
   prior type alias or require direct seeding around a struct-definition
   fallback.

The post-run use is semantically wrong-shaped. It includes assignments that did
not execute, cannot see global stores performed inside called functions, and
needs a second AST scope filter to distinguish `let` locals from true global
bindings. Recent recovery code already works around this by intersecting the
syntactic set with `Vm::repl_written_global_names` or by discovering new frame-0
bindings separately.

The VM already records every store instruction executed in frame 0, but that
single set does not identify a `StoreGlobalAny` performed from a nested frame or
from an explicit `global` inside a hard scope. Consequently the session still
consults `extract_declared_globals` to rescue those writes.

## Considered approaches

### A. Keep the syntax scan and add more cases

This would preserve the current control flow and add walkers for any missed IR
shape. It cannot distinguish reached from unreached statements and will remain
incomplete for calls that mutate globals. It does not advance the #9784
retirement list.

### B. Synchronize every binding visible in frame 0

This removes the assignment scan, but frame 0 also contains definition
bindings, seeded values, and stale slots left by hard-scope compilation. Treating
all of them as current value writes would leak locals and could convert methods
or types into ordinary persisted values.

### C. Record executed global-store provenance in the VM

This is the selected approach. Extend the existing per-evaluation write set so
the VM distinguishes an explicit/module-global store from an ordinary frame-0
slot store. Combine that runtime evidence with the compiler's final
`main_scope_names` and the already-known prior globals. The session no longer
derives committed state from the AST.

## Selected design

### 1. VM write evidence

Keep `repl_written_globals` as the set of frame-0 bindings whose store
instruction actually ran. Add a second per-evaluation set for stores that are
unconditionally module-global by instruction semantics:

- `StoreGlobalAny(name)` records `name` in both sets, regardless of the current
  call-frame depth;
- ordinary typed stores and `StoreAny` record only in
  `repl_written_globals` when they execute in frame 0;
- both sets are cleared by `reenter_appended_main` together with the other
  transient per-evaluation traces;
- fresh VMs begin with both sets empty.

Expose the explicit/module-global set through a read-only VM accessor. The
accessor reports execution evidence only; it does not classify Julia binding
kinds or mutate session state.

### 2. Authoritative value-binding projection

After a successful run or catchable-error recovery, construct the value names
to synchronize as the union of:

- prior persisted value globals that must be refreshed from the current VM;
- executed frame-0 writes whose names are in the compiler-produced
  `main_scope_names`;
- executed unqualified explicit/module-global writes, including new Main bindings
  created inside called functions or hard scopes. Qualified module stores remain
  under the existing `module_globals` mirror rather than entering Main's value map.

Remove names published as functions, nominal types, enum/type metadata, or
current type aliases before committing value rebindings. The existing
`runtime_value_rebindings` boundary remains the common binding-kind filter.

`extract_globals_from_vm` no longer receives the current `Program` and does not
scan statements. It projects exactly the authoritative runtime-derived name set
and retains its existing value/type/heap metadata conversion until those
remaining mirrors are retired later in #9784.

This rule is used consistently for:

- successful live-delta runs;
- successful fresh-delta/full-recompile runs;
- successful definition transactions;
- catchable live-error recovery;
- catchable definition-free fresh-VM recovery.

### 3. Narrow pre-run hazard detection

Compilation still needs conservative information before execution in one
specific situation: a struct-definition full rebuild may need to seed a same-name prior global
  directly so its old value is available to a self-rebinding RHS before the new
  type activation marker.

Do not retain or rename a general "assigned variables" persistence collector.
Instead, add a target-aware predicate that walks the current input only while
looking for prior global names when the input defines a struct. Its result is a
compile-preparation upper bound only. It never commits state.

After execution, the VM write sets decide which candidate rebindings actually
occurred, including all static type-alias invalidation. Thus an assignment after
`error(...)` or in an untaken branch cannot invalidate a type alias, while an
executed store can. No alias name needs to be classified before execution.

### 4. Deletion boundary

Delete:

- `extract_assigned_variables`;
- `extract_assigned_from_expr` and timing-macro-only assignment extraction used
  solely by it;
- unit tests that assert the obsolete syntax-collection contract;
- `extract_declared_globals` and its recursive helpers if the explicit-global
  VM trace makes their last production use unreachable.

Retain unrelated expression conversion helpers only where they still serve the
fresh fallback. Update `docs/vm/ADR_REPL_EVAL_MODEL.md` so the retirement list
records `extract_assigned_variables` as retired and explains that pre-run hazard
screening is not a persistence authority.

## Error handling and invariants

- A failed compile executes no store and changes neither runtime bindings nor
  mirrors.
- Catchable runtime errors commit only stores executed before the throw.
- Host cancellation and VM-internal invariant failures continue to drop the VM
  under the existing policy.
- A `let`-local frame-0 slot is excluded unless it is also a final
  `main_scope_name`; an unqualified explicit `global` store is included
  independently, while a qualified store remains module-owned.
- Definition publication is filtered from value rebinding, so a function/type
  marker cannot invalidate a same-named static alias as though it were an
  ordinary assignment.

## Verification

Add or strengthen regressions for:

1. an untaken branch assignment does not persist;
2. a `let` local does not persist;
3. `global` inside a `let` persists;
4. `global` inside a called function persists on success;
5. the same called-function write persists when a later statement throws;
6. an unreachable alias rebinding leaves the prior alias usable;
7. a reached alias rebinding before an error invalidates the alias;
8. a struct-bearing self-rebinding still preserves its old RHS value on the
   full-recompile fallback;
9. the existing `@time`/macro-generated global assignment behavior remains
   covered at the session level rather than by syntax-collector unit tests.

Run the narrow REPL regressions first, then the repository-required formatting,
clippy, full release nextest, source audits, and guarded premerge gate. This
touches VM/runtime behavior but not AoT-specific code or the C ABI.
